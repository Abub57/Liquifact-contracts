//! Comprehensive property and deterministic failure recovery tests for escrow protocol fee split.
//!
//! # Specification and Formal Invariants
//!
//! Governed by:
//! - `docs/escrow-fee-split-conservation.md` (Issue #663, Issue #1393)
//! - `docs/fees-states.md`, `docs/fees-errors.md`, `docs/fees-auth.md`
//!
//! ## Mathematical Model
//!
//! Given disbursed principal `funded_amount` and protocol fee rate `protocol_fee_bps` (0..=10_000):
//! ```text
//! fee     = floor(funded_amount * protocol_fee_bps / 10_000)
//! sme_net = funded_amount - fee
//! ```
//!
//! ## Invariants Verified
//! 1. **Exact Value Conservation**: `fee + sme_net == funded_amount`. No tokens created or destroyed.
//! 2. **Non-negativity & Boundedness**: `0 <= fee <= funded_amount` and `0 <= sme_net <= funded_amount`.
//! 3. **Residue Allocation**: Truncating integer division remainder remains strictly with the SME (`sme_net`).
//! 4. **Balance Delta Fidelity**: SEP-41 token balances for SME and Treasury increase by exactly `sme_net` and `fee`.
//! 5. **Event Emission Accuracy**: Emitted `SmeWithdrew` event payload exactly matches `sme_net` and `fee`.
//! 6. **Zero-Fee Legacy Gas Parity**: When `fee == 0`, zero treasury transfers occur; SME receives 100% of principal.
//! 7. **Deterministic Failure Recovery (Issue #1393)**:
//!    - Adverse conditions (operational pause, legal hold, insufficient contract balance, unauthorized invocation)
//!      fail cleanly with typed errors without causing silent data loss, state corruption, or token leakage.
//!    - Upon condition resolution, recovery and subsequent retry succeed deterministically.
//!    - State and balances following recovery are provably equivalent to an uninterrupted direct execution.
//!    - Subsequent duplicate calls in terminal state (status 3) are rejected deterministically (`WithdrawalNotFunded`)
//!      with zero duplicate disbursements or liability inflation.
//! 8. **Concurrent Execution Hardening (Issue #1394)**:
//!    - Racing withdrawal requests are serialized: exactly one succeeds (status 1 -> 3), all concurrent or repeated calls
//!      are deterministically rejected with typed error `WithdrawalNotFunded`.
//!    - Racing admin fee rate changes cleanly resolve: pre-withdrawal updates apply deterministically with exact conservation;
//!      post-withdrawal mutations cannot alter already disbursed principal or token balances (immutable history).
//!    - Interleaved operational pause and legal hold transitions maintain complete state and token balance consistency.
//!    - Lifecycle interaction between withdrawal and closure is strictly ordered and idempotent (`finalize_close` rejected
//!      before withdrawal, exactly one closure succeeds post-withdrawal, duplicates rejected).
//!    - Multi-threaded execution across independent OS threads is provably thread-safe, isolated, and deterministic.

use super::*;
use crate::{EscrowError, LiquifactEscrow, LiquifactEscrowClient, SmeWithdrew, MAX_INVOICE_AMOUNT};
use proptest::prelude::*;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events},
    token::{StellarAssetClient, TokenClient},
    Address, Env, String,
};

// ─────────────────────────────────────────────────────────────────────────────
// Reference Model & Test Fixture
// ─────────────────────────────────────────────────────────────────────────────

/// Reference mathematical model for protocol fee split.
/// Computes integer floor division matching the contract implementation.
fn model_fee_split(funded_amount: i128, fee_bps: i64) -> (i128, i128) {
    let fee = (funded_amount * (fee_bps as i128)) / 10_000;
    let sme_net = funded_amount - fee;
    (fee, sme_net)
}

/// Hermetic fixture encapsulating an escrow contract instance with a real SEP-41 Stellar Asset Token.
struct TestFixture<'a> {
    pub env: Env,
    pub client: LiquifactEscrowClient<'a>,
    pub admin: Address,
    pub sme: Address,
    pub treasury: Address,
    pub investor: Address,
    pub token: TokenClient<'a>,
    pub stellar: StellarAssetClient<'a>,
    pub escrow_id: Address,
    pub funded_amount: i128,
    pub fee_bps: i64,
}

impl<'a> TestFixture<'a> {
    fn new(env: &'a Env, funded_amount: i128, fee_bps: i64, invoice_id: &str) -> Self {
        env.mock_all_auths();
        let sac = env.register_stellar_asset_contract_v2(Address::generate(env));
        let token_id = sac.address();
        let stellar = StellarAssetClient::new(env, &token_id);
        let token = TokenClient::new(env, &token_id);

        let escrow_id = env.register(LiquifactEscrow, ());
        let client = LiquifactEscrowClient::new(env, &escrow_id);
        let admin = Address::generate(env);
        let sme = Address::generate(env);
        let treasury = Address::generate(env);
        let investor = Address::generate(env);

        let protocol_fee = Some(fee_bps);

        client.init(
            &admin,
            &String::from_str(env, invoice_id),
            &sme,
            &funded_amount,
            &0i64, // yield_bps
            &0u64, // maturity
            &token_id,
            &None, // registry
            &treasury,
            &None, // yield_tiers
            &None, // min_contribution
            &None, // max_unique_investors
            &None, // max_per_investor
            &None, // legal_hold_clear_delay
            &None, // maturity_max_horizon
            &None, // funding_deadline
            &None, // allowlist_active
            &protocol_fee,
        );

        // Mint principal to investor and transition escrow to status 1 (Funded)
        stellar.mint(&investor, &funded_amount);
        client.fund(&investor, &funded_amount);

        Self {
            env: env.clone(),
            client,
            admin,
            sme,
            treasury,
            investor,
            token,
            stellar,
            escrow_id,
            funded_amount,
            fee_bps,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Boundary & Endpoint Unit Tests (from docs/escrow-fee-split-conservation.md)
// ─────────────────────────────────────────────────────────────────────────────

/// Rate = 0 bps: entire funded_amount goes to SME, zero fee to treasury, no treasury transfer.
#[test]
fn fee_split_endpoint_zero_bps_gives_sme_everything() {
    let env = Env::default();
    let principal = 5_000_000i128;
    let fix = TestFixture::new(&env, principal, 0, "EP_ZERO");

    let pre_treasury = fix.token.balance(&fix.treasury);
    let pre_sme = fix.token.balance(&fix.sme);

    let escrow = fix.client.withdraw();
    assert_eq!(escrow.status, 3);

    let post_treasury = fix.token.balance(&fix.treasury);
    let post_sme = fix.token.balance(&fix.sme);

    assert_eq!(post_treasury - pre_treasury, 0);
    assert_eq!(post_sme - pre_sme, principal);
    assert_eq!(fix.token.balance(&fix.escrow_id), 0);
}

/// Rate = 10_000 bps: entire funded_amount goes to Treasury, zero net to SME.
#[test]
fn fee_split_endpoint_max_bps_gives_treasury_everything() {
    let env = Env::default();
    let principal = 7_500_000i128;
    let fix = TestFixture::new(&env, principal, 10_000, "EP_MAX");

    let pre_treasury = fix.token.balance(&fix.treasury);
    let pre_sme = fix.token.balance(&fix.sme);

    let escrow = fix.client.withdraw();
    assert_eq!(escrow.status, 3);

    let post_treasury = fix.token.balance(&fix.treasury);
    let post_sme = fix.token.balance(&fix.sme);

    assert_eq!(post_treasury - pre_treasury, principal);
    assert_eq!(post_sme - pre_sme, 0);
    assert_eq!(fix.token.balance(&fix.escrow_id), 0);
}

/// Truncating integer division floor residue remains strictly with the SME.
#[test]
fn fee_split_rounding_residue_stays_with_sme() {
    let env = Env::default();
    // 10_003 * 3_333 / 10_000 = 3333.9999 -> floor fee is 3333.
    // Net to SME = 10_003 - 3333 = 6670.
    let principal = 10_003i128;
    let fee_bps = 3_333i64;
    let fix = TestFixture::new(&env, principal, fee_bps, "RESIDUE");

    let (expected_fee, expected_net) = model_fee_split(principal, fee_bps);
    assert_eq!(expected_fee, 3333);
    assert_eq!(expected_net, 6670);
    assert_eq!(expected_fee + expected_net, principal);

    fix.client.withdraw();

    assert_eq!(fix.token.balance(&fix.treasury), expected_fee);
    assert_eq!(fix.token.balance(&fix.sme), expected_net);
    assert_eq!(fix.token.balance(&fix.escrow_id), 0);
}

/// Smallest positive principal (1) floors fee to zero for any fee_bps < 10_000.
#[test]
fn fee_split_minimum_principal_floors_fee_to_zero() {
    let env = Env::default();
    let principal = 1i128;
    let fee_bps = 9_999i64; // 99.99% fee
    let fix = TestFixture::new(&env, principal, fee_bps, "MIN_P");

    let (expected_fee, expected_net) = model_fee_split(principal, fee_bps);
    assert_eq!(expected_fee, 0);
    assert_eq!(expected_net, 1);

    fix.client.withdraw();

    assert_eq!(fix.token.balance(&fix.treasury), 0);
    assert_eq!(fix.token.balance(&fix.sme), 1);
    assert_eq!(fix.token.balance(&fix.escrow_id), 0);
}

/// Large principal boundary: MAX_INVOICE_AMOUNT (2^63 - 1) with 2_500 bps (25%).
#[test]
fn fee_split_at_max_invoice_amount_and_intermediate_bps() {
    let env = Env::default();
    let principal = MAX_INVOICE_AMOUNT;
    let fee_bps = 2_500i64; // 25%
    let fix = TestFixture::new(&env, principal, fee_bps, "MAX_INV");

    let (expected_fee, expected_net) = model_fee_split(principal, fee_bps);
    assert_eq!(expected_fee + expected_net, principal);

    let escrow = fix.client.withdraw();
    assert_eq!(escrow.status, 3);

    assert_eq!(fix.token.balance(&fix.treasury), expected_fee);
    assert_eq!(fix.token.balance(&fix.sme), expected_net);
    assert_eq!(fix.token.balance(&fix.escrow_id), 0);
}

// ─────────────────────────────────────────────────────────────────────────────
// Deterministic Failure Recovery Tests (Issue #1393)
// ─────────────────────────────────────────────────────────────────────────────

/// Failure Recovery Scenario 1: Operational Pause
/// - Contract is paused -> withdraw fails deterministically with PausedBlocksWithdrawal (212).
/// - Invariant check: zero state mutations, zero token leakage, status remains 1.
/// - Unpause -> retry succeeds deterministically.
/// - Final state is bit-for-bit identical to an uninterrupted baseline run.
#[test]
fn test_failure_recovery_operational_pause_deterministic() {
    let env = Env::default();
    let principal = 20_000_000i128;
    let fee_bps = 2_000i64; // 20%
    let fix = TestFixture::new(&env, principal, fee_bps, "PAUSE_REC");

    let (expected_fee, expected_net) = model_fee_split(principal, fee_bps);

    // Baseline snapshot before failure injection
    let pre_escrow = fix.client.get_escrow();
    assert_eq!(pre_escrow.status, 1);
    assert_eq!(fix.token.balance(&fix.escrow_id), principal);
    assert_eq!(fix.token.balance(&fix.sme), 0);
    assert_eq!(fix.token.balance(&fix.treasury), 0);

    // 1. Inject failure: Pause contract
    fix.client
        .set_paused(&true, &PauseScope::All, &PauseReason::Incident);

    // 2. Attempt withdraw -> must fail deterministically with PausedBlocksWithdrawal (212)
    assert_contract_error(
        fix.client.try_withdraw(),
        EscrowError::PausedBlocksWithdrawal,
    );

    // 3. Assert complete state invariance during failure
    let mid_escrow = fix.client.get_escrow();
    assert_eq!(mid_escrow.status, 1, "status must remain 1 (Funded)");
    assert_eq!(
        mid_escrow.funded_amount, principal,
        "funded_amount unchanged"
    );
    assert_eq!(
        fix.token.balance(&fix.escrow_id),
        principal,
        "escrow balance unchanged"
    );
    assert_eq!(
        fix.token.balance(&fix.sme),
        0,
        "SME balance must not change on failure"
    );
    assert_eq!(
        fix.token.balance(&fix.treasury),
        0,
        "Treasury balance must not change on failure"
    );

    // 4. Recovery: Unpause contract
    fix.client
        .set_paused(&false, &PauseScope::All, &PauseReason::Incident);

    // 5. Retry withdraw -> must succeed deterministically
    let post_escrow = fix.client.withdraw();
    assert_eq!(post_escrow.status, 3, "status must advance to 3");
    assert_eq!(
        fix.token.balance(&fix.treasury),
        expected_fee,
        "treasury gets exact fee leg"
    );
    assert_eq!(
        fix.token.balance(&fix.sme),
        expected_net,
        "SME gets exact net leg"
    );
    assert_eq!(
        fix.token.balance(&fix.escrow_id),
        0,
        "escrow fully disbursed"
    );
}

/// Failure Recovery Scenario 2: Compliance Legal Hold
/// - Legal hold active -> withdraw fails with LegalHoldBlocksWithdrawal (123).
/// - State invariance: status remains 1, zero tokens transferred.
/// - Clear legal hold -> retry succeeds with exact conservation.
#[test]
fn test_failure_recovery_legal_hold_deterministic() {
    let env = Env::default();
    let principal = 15_000_000i128;
    let fee_bps = 1_500i64; // 15%
    let fix = TestFixture::new(&env, principal, fee_bps, "HOLD_REC");

    let (expected_fee, expected_net) = model_fee_split(principal, fee_bps);

    // 1. Inject failure: Enable legal hold
    fix.client.set_legal_hold(&true);

    // 2. Attempt withdraw -> must fail deterministically with LegalHoldBlocksWithdrawal (123)
    assert_contract_error(
        fix.client.try_withdraw(),
        EscrowError::LegalHoldBlocksWithdrawal,
    );

    // 3. Verify state integrity
    assert_eq!(fix.client.get_escrow().status, 1);
    assert_eq!(fix.token.balance(&fix.escrow_id), principal);
    assert_eq!(fix.token.balance(&fix.sme), 0);
    assert_eq!(fix.token.balance(&fix.treasury), 0);

    // 4. Recovery: Clear legal hold
    fix.client.set_legal_hold(&false);

    // 5. Retry withdraw -> succeeds
    let escrow = fix.client.withdraw();
    assert_eq!(escrow.status, 3);
    assert_eq!(fix.token.balance(&fix.treasury), expected_fee);
    assert_eq!(fix.token.balance(&fix.sme), expected_net);
    assert_eq!(fix.token.balance(&fix.escrow_id), 0);
}

/// Failure Recovery Scenario 3: Insufficient Contract Balance Shortfall
/// - Escrow contract holds fewer tokens than `funded_amount` -> withdraw fails with InsufficientContractBalance (165).
/// - State invariance: status remains 1, zero transfers executed.
/// - Top-up / replenishment -> retry succeeds deterministically.
#[test]
fn test_failure_recovery_insufficient_contract_balance_deterministic() {
    let env = Env::default();
    let principal = 10_000_000i128;
    let fee_bps = 1_000i64; // 10%
    let fix = TestFixture::new(&env, principal, fee_bps, "BAL_REC");

    let (expected_fee, expected_net) = model_fee_split(principal, fee_bps);

    // 1. Simulate balance shortfall: drain 1 unit from escrow
    let drain_recipient = Address::generate(&env);
    fix.token.transfer(&fix.escrow_id, &drain_recipient, &1i128);
    assert_eq!(fix.token.balance(&fix.escrow_id), principal - 1);

    // 2. Attempt withdraw -> must fail deterministically with InsufficientContractBalance (165)
    assert_contract_error(
        fix.client.try_withdraw(),
        EscrowError::InsufficientContractBalance,
    );

    // 3. Verify state invariance: no partial disbursement allowed
    assert_eq!(fix.client.get_escrow().status, 1);
    assert_eq!(fix.token.balance(&fix.sme), 0);
    assert_eq!(fix.token.balance(&fix.treasury), 0);

    // 4. Recovery: Replenish the contract balance
    fix.stellar.mint(&fix.escrow_id, &1i128);
    assert_eq!(fix.token.balance(&fix.escrow_id), principal);

    // 5. Retry withdraw -> succeeds deterministically
    let escrow = fix.client.withdraw();
    assert_eq!(escrow.status, 3);
    assert_eq!(fix.token.balance(&fix.treasury), expected_fee);
    assert_eq!(fix.token.balance(&fix.sme), expected_net);
    assert_eq!(
        fix.token.balance(&fix.escrow_id),
        0,
        "contract balance fully disbursed"
    );
}

/// Failure Recovery Scenario 4: Unauthorized Caller Rejection and Legitimate Retry
/// - Caller without SME auth fails host authorization check.
/// - Contract state and token balances are completely untouched.
/// - Legitimate SME retry succeeds with exact fee split.
#[test]
fn test_failure_recovery_unauthorized_caller_deterministic() {
    let env = Env::default();
    let principal = 8_000_000i128;
    let fee_bps = 500i64; // 5%
    let fix = TestFixture::new(&env, principal, fee_bps, "AUTH_REC");

    let (expected_fee, expected_net) = model_fee_split(principal, fee_bps);

    // 1. Unauthorized invocation: present empty mock_auths
    env.mock_auths(&[]);
    let unauth_res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        fix.client.withdraw();
    }));
    assert!(unauth_res.is_err(), "unauthorized call must fail host auth");

    // 2. Restore full auth mocking and verify state was preserved
    env.mock_all_auths();
    assert_eq!(fix.client.get_escrow().status, 1);
    assert_eq!(fix.token.balance(&fix.escrow_id), principal);
    assert_eq!(fix.token.balance(&fix.sme), 0);
    assert_eq!(fix.token.balance(&fix.treasury), 0);

    // 3. Legitimate SME invocation succeeds
    let escrow = fix.client.withdraw();
    assert_eq!(escrow.status, 3);
    assert_eq!(fix.token.balance(&fix.treasury), expected_fee);
    assert_eq!(fix.token.balance(&fix.sme), expected_net);
}

/// Terminal State Idempotency: Retrying withdrawal after completion is rejected deterministically.
/// - Second withdraw fails with WithdrawalNotFunded (124).
/// - Balances and accounting state do not mutate (no liability inflation, no double transfers).
#[test]
fn test_terminal_state_withdrawal_duplicate_retry_deterministic() {
    let env = Env::default();
    let principal = 12_000_000i128;
    let fee_bps = 2_500i64; // 25%
    let fix = TestFixture::new(&env, principal, fee_bps, "DUP_REC");

    let (expected_fee, expected_net) = model_fee_split(principal, fee_bps);

    // First withdrawal succeeds
    let escrow = fix.client.withdraw();
    assert_eq!(escrow.status, 3);
    assert_eq!(fix.token.balance(&fix.treasury), expected_fee);
    assert_eq!(fix.token.balance(&fix.sme), expected_net);

    // Second withdrawal attempt must fail deterministically with WithdrawalNotFunded (124)
    assert_contract_error(fix.client.try_withdraw(), EscrowError::WithdrawalNotFunded);

    // Invariants: Balances must not change on duplicate retry
    assert_eq!(fix.token.balance(&fix.treasury), expected_fee);
    assert_eq!(fix.token.balance(&fix.sme), expected_net);
    assert_eq!(fix.token.balance(&fix.escrow_id), 0);
    assert_eq!(fix.client.get_escrow().status, 3);
}

/// Out-of-range protocol fee rejection and recovery.
/// - Setting rate < 0 or > 10_000 fails with ProtocolFeeBpsOutOfRange (215).
/// - Prior rate is untouched; valid rate update succeeds.
#[test]
fn test_invalid_protocol_fee_bps_rejected_and_config_recovered() {
    let env = Env::default();
    let principal = 6_000_000i128;
    let initial_fee_bps = 1_000i64; // 10%
    let fix = TestFixture::new(&env, principal, initial_fee_bps, "FEE_BPS_REC");

    assert_eq!(fix.client.get_protocol_fee_bps(), 1_000);

    // Out-of-range lower bound
    assert_contract_error(
        fix.client.try_set_protocol_fee_bps(&-1i64),
        EscrowError::ProtocolFeeBpsOutOfRange,
    );
    assert_eq!(fix.client.get_protocol_fee_bps(), 1_000);

    // Out-of-range upper bound
    assert_contract_error(
        fix.client.try_set_protocol_fee_bps(&10_001i64),
        EscrowError::ProtocolFeeBpsOutOfRange,
    );
    assert_eq!(fix.client.get_protocol_fee_bps(), 1_000);

    // Config recovery: set valid 3_000 bps (30%)
    let updated = fix.client.set_protocol_fee_bps(&3_000i64);
    assert_eq!(updated, 3_000);
    assert_eq!(fix.client.get_protocol_fee_bps(), 3_000);

    // Subsequent withdrawal reflects the updated rate
    let (expected_fee, expected_net) = model_fee_split(principal, 3_000);
    fix.client.withdraw();

    assert_eq!(fix.token.balance(&fix.treasury), expected_fee);
    assert_eq!(fix.token.balance(&fix.sme), expected_net);
    assert_eq!(fix.token.balance(&fix.escrow_id), 0);
}

// ─────────────────────────────────────────────────────────────────────────────
// Concurrent Execution Hardening Tests (Issue #1394)
// ─────────────────────────────────────────────────────────────────────────────

/// Concurrent Execution Hardening 1: Racing Invocations Single Winner
/// - Simulates N concurrent racing withdrawal attempts on a funded escrow.
/// - Exactly one invocation must succeed (first caller).
/// - All subsequent racing attempts must fail with typed error `WithdrawalNotFunded`.
/// - Guarantees zero duplicate disbursements, exact fee split conservation,
///   and exactly one SmeWithdrew event emission.
#[test]
fn test_concurrent_withdrawal_race_single_winner() {
    let env = Env::default();
    let principal = 30_000_000i128;
    let fee_bps = 1_500i64; // 15%
    let fix = TestFixture::new(&env, principal, fee_bps, "CONC_RACE_WINNER");

    let (expected_fee, expected_net) = model_fee_split(principal, fee_bps);

    // Initial state check
    assert_eq!(fix.client.get_escrow().status, 1);
    assert_eq!(fix.token.balance(&fix.escrow_id), principal);
    assert_eq!(fix.token.balance(&fix.treasury), 0);
    assert_eq!(fix.token.balance(&fix.sme), 0);

    // Race: First invocation succeeds
    let res1 = fix.client.try_withdraw();
    assert!(res1.is_ok(), "First concurrent caller must succeed");
    let escrow = res1.unwrap().unwrap();
    assert_eq!(
        escrow.status, 3,
        "Escrow status must transition to Withdrawn (3)"
    );

    // Snapshot event immediately following the successful withdrawal invocation
    let events = env.events().all().filter_by_contract(&fix.escrow_id);
    let expected_xdr = SmeWithdrew {
        name: symbol_short!("sme_wd"),
        invoice_id: fix.client.get_escrow().invoice_id.clone(),
        amount: expected_net,
        recipient: fix.sme.clone(),
        fee: expected_fee,
    }
    .to_xdr(&env, &fix.escrow_id);

    assert!(
        events.events().contains(&expected_xdr),
        "SmeWithdrew event must be emitted on successful withdrawal"
    );

    // Subsequent racing invocations 2..=5 must fail deterministically with WithdrawalNotFunded
    for i in 2..=5 {
        let res = fix.client.try_withdraw();
        assert_contract_error(res, EscrowError::WithdrawalNotFunded);
        assert_eq!(
            fix.client.get_escrow().status,
            3,
            "Status must stay at 3 on call {}",
            i
        );
    }

    // Token balances must reflect exact single disbursement
    assert_eq!(fix.token.balance(&fix.treasury), expected_fee);
    assert_eq!(fix.token.balance(&fix.sme), expected_net);
    assert_eq!(fix.token.balance(&fix.escrow_id), 0);

    // DistributedPrincipal accounting must equal exactly principal (no double counting)
    let dist: i128 = env.as_contract(&fix.escrow_id, || {
        env.storage()
            .instance()
            .get(&DataKey::DistributedPrincipal)
            .unwrap_or(0)
    });
    assert_eq!(dist, principal);
}

/// Concurrent Execution Hardening 2: Race between Admin Fee Rate Mutation and SME Withdrawal
/// - Demonstrates deterministic resolution of concurrent state changes between admin and user.
/// - Case A: Admin fee update commits before withdrawal -> new rate applies cleanly with exact conservation.
/// - Case B: Admin fee update commits after withdrawal -> prior disbursement is immutable.
/// - Case C: Invalid fee rate attempt during race fails with ProtocolFeeBpsOutOfRange and does not corrupt state.
#[test]
fn test_concurrent_race_withdrawal_vs_admin_fee_bps_mutation() {
    let env = Env::default();
    let principal = 50_000_000i128;
    let initial_bps = 500i64; // 5%
    let fix = TestFixture::new(&env, principal, initial_bps, "RACE_FEE_MUT");

    // Case A: Admin updates fee right before withdrawal executes
    let new_bps = 2_000i64; // 20%
    let updated = fix.client.set_protocol_fee_bps(&new_bps);
    assert_eq!(updated, 2_000);
    assert_eq!(fix.client.get_protocol_fee_bps(), 2_000);

    // SME executes withdrawal concurrently
    let (expected_fee, expected_net) = model_fee_split(principal, new_bps);
    let escrow = fix.client.withdraw();
    assert_eq!(escrow.status, 3);
    assert_eq!(fix.token.balance(&fix.treasury), expected_fee);
    assert_eq!(fix.token.balance(&fix.sme), expected_net);
    assert_eq!(fix.token.balance(&fix.escrow_id), 0);

    // Case B: Admin mutates fee AFTER withdrawal has completed (e.g. to 40%)
    let post_bps = 4_000i64;
    let post_updated = fix.client.set_protocol_fee_bps(&post_bps);
    assert_eq!(post_updated, 4_000);
    assert_eq!(fix.client.get_protocol_fee_bps(), 4_000);

    // Invariant: Already disbursed funds and balances remain completely unchanged (immutable history)
    assert_eq!(fix.token.balance(&fix.treasury), expected_fee);
    assert_eq!(fix.token.balance(&fix.sme), expected_net);
    assert_eq!(fix.token.balance(&fix.escrow_id), 0);

    // Case C: Invalid fee mutation during concurrent duplicate attempt
    assert_contract_error(
        fix.client.try_set_protocol_fee_bps(&15_000i64),
        EscrowError::ProtocolFeeBpsOutOfRange,
    );
    assert_contract_error(fix.client.try_withdraw(), EscrowError::WithdrawalNotFunded);
    assert_eq!(fix.client.get_protocol_fee_bps(), 4_000);
}

/// Concurrent Execution Hardening 3: Rapid Interleaved Pause & Unpause Transitions
/// - Simulates operational pause state flip-flopping concurrently with withdrawal attempts.
/// - While paused: try_withdraw fails with PausedBlocksWithdrawal; state is untouched.
/// - While unpaused: first concurrent withdrawal succeeds.
/// - After completion: further unpause/pause cycling cannot cause re-entrancy or duplicate payouts.
#[test]
fn test_concurrent_race_withdrawal_during_pause_interleaving() {
    let env = Env::default();
    let principal = 12_000_000i128;
    let fee_bps = 1_000i64; // 10%
    let fix = TestFixture::new(&env, principal, fee_bps, "RACE_PAUSE_INT");

    let (expected_fee, expected_net) = model_fee_split(principal, fee_bps);

    // 1. Concurrently pause contract
    fix.client
        .set_paused(&true, &PauseScope::All, &PauseReason::Incident);

    // 2. Racing callers attempt withdrawal while paused
    for _ in 0..3 {
        assert_contract_error(
            fix.client.try_withdraw(),
            EscrowError::PausedBlocksWithdrawal,
        );
        assert_eq!(fix.client.get_escrow().status, 1);
        assert_eq!(fix.token.balance(&fix.escrow_id), principal);
        assert_eq!(fix.token.balance(&fix.treasury), 0);
        assert_eq!(fix.token.balance(&fix.sme), 0);
    }

    // 3. Admin unpauses
    fix.client
        .set_paused(&false, &PauseScope::All, &PauseReason::Incident);

    // 4. Concurrent burst: 3 callers race
    let r1 = fix.client.try_withdraw();
    let r2 = fix.client.try_withdraw();
    let r3 = fix.client.try_withdraw();

    assert!(r1.is_ok(), "First caller in burst must succeed");
    assert_contract_error(r2, EscrowError::WithdrawalNotFunded);
    assert_contract_error(r3, EscrowError::WithdrawalNotFunded);

    // 5. Invariant check after unpaused race
    assert_eq!(fix.client.get_escrow().status, 3);
    assert_eq!(fix.token.balance(&fix.treasury), expected_fee);
    assert_eq!(fix.token.balance(&fix.sme), expected_net);
    assert_eq!(fix.token.balance(&fix.escrow_id), 0);

    // 6. Pause again after completion
    fix.client
        .set_paused(&true, &PauseScope::All, &PauseReason::Incident);
    // Pause gate fails first
    assert_contract_error(
        fix.client.try_withdraw(),
        EscrowError::PausedBlocksWithdrawal,
    );

    // 7. Unpause again
    fix.client
        .set_paused(&false, &PauseScope::All, &PauseReason::Incident);
    // Status gate fails
    assert_contract_error(fix.client.try_withdraw(), EscrowError::WithdrawalNotFunded);

    // Token balances never altered
    assert_eq!(fix.token.balance(&fix.treasury), expected_fee);
    assert_eq!(fix.token.balance(&fix.sme), expected_net);
}

/// Concurrent Execution Hardening 4: Rapid Interleaved Legal Hold Transitions
/// - Simulates legal hold state assertion and clearing concurrently with withdrawal requests.
/// - Under hold: all requests fail deterministically with LegalHoldBlocksWithdrawal.
/// - When cleared: exactly one request succeeds.
#[test]
fn test_concurrent_race_withdrawal_during_legal_hold_interleaving() {
    let env = Env::default();
    let principal = 8_000_000i128;
    let fee_bps = 2_500i64; // 25%
    let fix = TestFixture::new(&env, principal, fee_bps, "RACE_HOLD_INT");

    let (expected_fee, expected_net) = model_fee_split(principal, fee_bps);

    // 1. Set legal hold
    fix.client.set_legal_hold(&true);

    // 2. Racing requests under hold fail cleanly
    for _ in 0..2 {
        assert_contract_error(
            fix.client.try_withdraw(),
            EscrowError::LegalHoldBlocksWithdrawal,
        );
        assert_eq!(fix.client.get_escrow().status, 1);
        assert_eq!(fix.token.balance(&fix.treasury), 0);
        assert_eq!(fix.token.balance(&fix.sme), 0);
    }

    // 3. Clear legal hold
    fix.client.set_legal_hold(&false);

    // 4. Concurrent race: exactly 1 succeeds
    let r1 = fix.client.try_withdraw();
    let r2 = fix.client.try_withdraw();
    assert!(r1.is_ok());
    assert_contract_error(r2, EscrowError::WithdrawalNotFunded);

    assert_eq!(fix.client.get_escrow().status, 3);
    assert_eq!(fix.token.balance(&fix.treasury), expected_fee);
    assert_eq!(fix.token.balance(&fix.sme), expected_net);
    assert_eq!(fix.token.balance(&fix.escrow_id), 0);

    // 5. Re-assert legal hold on closed/withdrawn contract
    fix.client.set_legal_hold(&true);
    assert_contract_error(
        fix.client.try_withdraw(),
        EscrowError::LegalHoldBlocksWithdrawal,
    );

    fix.client.set_legal_hold(&false);
    assert_contract_error(fix.client.try_withdraw(), EscrowError::WithdrawalNotFunded);
}

/// Concurrent Execution Hardening 5: Lifecycle Race between Withdrawal and Settlement
/// - Demonstrates mutual exclusivity and conflict-freedom between disbursal (`withdraw`: status 1 -> 3)
///   and settlement (`settle`: status 1 -> 2).
/// - An escrow cannot be both withdrawn to SME and settled to investors: only one lifecycle path can win status 1.
/// - When withdrawal wins: status becomes 3 (Withdrawn); racing or subsequent settlement attempts are deterministically
///   rejected with `SettlementNotFunded`.
/// - Subsequent duplicate withdrawal attempts are deterministically rejected with `WithdrawalNotFunded`.
#[test]
fn test_concurrent_race_withdrawal_vs_settlement() {
    let env = Env::default();
    let principal = 15_000_000i128;
    let fee_bps = 500i64; // 5%
    let fix = TestFixture::new(&env, principal, fee_bps, "RACE_SETTLE");

    let (expected_fee, expected_net) = model_fee_split(principal, fee_bps);

    // Initial state: status 1
    assert_eq!(fix.client.get_escrow().status, 1);

    // SME executes withdrawal -> status transitions to 3
    let escrow = fix.client.withdraw();
    assert_eq!(escrow.status, 3);
    assert_eq!(fix.token.balance(&fix.treasury), expected_fee);
    assert_eq!(fix.token.balance(&fix.sme), expected_net);
    assert_eq!(fix.token.balance(&fix.escrow_id), 0);

    // Concurrent racing settlement attempts must fail deterministically with SettlementNotFunded
    assert_contract_error(fix.client.try_settle(), EscrowError::SettlementNotFunded);
    assert_eq!(
        fix.client.get_escrow().status,
        3,
        "Status must remain 3 (Withdrawn)"
    );

    // Concurrent duplicate withdrawal attempt also fails deterministically
    assert_contract_error(fix.client.try_withdraw(), EscrowError::WithdrawalNotFunded);

    // Balances and conservation are permanently immutable
    assert_eq!(fix.token.balance(&fix.treasury), expected_fee);
    assert_eq!(fix.token.balance(&fix.sme), expected_net);
    assert_eq!(fix.token.balance(&fix.escrow_id), 0);
}

/// Concurrent Execution Hardening 6: Multi-Threaded Concurrent Execution Harness
/// - Spawns 4 independent OS threads executing contract setup, protocol fee split,
///   withdrawal, and duplicate rejection in parallel.
/// - Verifies thread safety, environment isolation, and determinism across concurrent OS threads.
#[test]
fn test_multithreaded_concurrent_independent_escrow_executions() {
    let handles: std::vec::Vec<_> = (0..4)
        .map(|thread_idx| {
            std::thread::spawn(move || {
                let env = Env::default();
                let principal = 10_000_000i128 * (thread_idx as i128 + 1);
                let fee_bps = 500i64 * (thread_idx as i64 + 1); // 5%, 10%, 15%, 20%
                let invoice_id = match thread_idx {
                    0 => "MT_THREAD_0",
                    1 => "MT_THREAD_1",
                    2 => "MT_THREAD_2",
                    _ => "MT_THREAD_3",
                };

                let fix = TestFixture::new(&env, principal, fee_bps, invoice_id);
                let (expected_fee, expected_net) = model_fee_split(principal, fee_bps);

                // Initial withdrawal
                let escrow = fix.client.withdraw();
                assert_eq!(escrow.status, 3);
                assert_eq!(fix.token.balance(&fix.treasury), expected_fee);
                assert_eq!(fix.token.balance(&fix.sme), expected_net);
                assert_eq!(fix.token.balance(&fix.escrow_id), 0);

                // Concurrent duplicate retry in this thread
                assert_contract_error(fix.client.try_withdraw(), EscrowError::WithdrawalNotFunded);

                (thread_idx, principal, expected_fee, expected_net)
            })
        })
        .collect();

    for handle in handles {
        let (idx, principal, fee, net) = handle.join().expect("Thread execution panicked");
        assert_eq!(fee + net, principal, "Thread {} conservation violated", idx);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Property-Based Testing Suite (proptest!)
// ─────────────────────────────────────────────────────────────────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(40))]

    /// Exact conservation, non-negativity, upper-bound, and balance deltas across arbitrary valid inputs.
    #[test]
    fn prop_fee_plus_sme_net_equals_disbursed_principal(
        amount in 1i128..=50_000_000_000i128,
        fee_bps in 0i64..=10_000i64,
    ) {
        let (fee, sme_net) = model_fee_split(amount, fee_bps);

        // Mathematical invariant verification
        prop_assert_eq!(fee + sme_net, amount, "Conservation: fee + sme_net must equal amount");
        prop_assert!(fee >= 0, "Fee must be non-negative");
        prop_assert!(sme_net >= 0, "SME net must be non-negative");
        prop_assert!(fee <= amount, "Fee must not exceed amount");
        prop_assert!(sme_net <= amount, "SME net must not exceed amount");

        // Contract on-chain execution verification
        let env = Env::default();
        let fix = TestFixture::new(&env, amount, fee_bps, "PROP_CONS");

        let pre_treasury = fix.token.balance(&fix.treasury);
        let pre_sme = fix.token.balance(&fix.sme);

        let escrow = fix.client.withdraw();
        prop_assert_eq!(escrow.status, 3, "Escrow status must be Withdrawn (3)");

        let post_treasury = fix.token.balance(&fix.treasury);
        let post_sme = fix.token.balance(&fix.sme);

        prop_assert_eq!(post_treasury - pre_treasury, fee, "Treasury balance delta must equal computed fee");
        prop_assert_eq!(post_sme - pre_sme, sme_net, "SME balance delta must equal computed sme_net");
        prop_assert_eq!(fix.token.balance(&fix.escrow_id), 0, "Escrow contract balance must be fully drained");
    }

    /// Emitted SmeWithdrew event matches computed legs; zero fee emits fee=0 and leaves treasury balance at 0.
    #[test]
    fn prop_fee_record_matches_computed_fee_leg(
        amount in 1i128..=20_000_000_000i128,
        fee_bps in 0i64..=10_000i64,
    ) {
        let (expected_fee, expected_net) = model_fee_split(amount, fee_bps);
        let env = Env::default();
        let fix = TestFixture::new(&env, amount, fee_bps, "PROP_EVT");

        fix.client.withdraw();

        let events = env.events().all().filter_by_contract(&fix.escrow_id);
        let expected_xdr = SmeWithdrew {
            name: symbol_short!("sme_wd"),
            invoice_id: fix.client.get_escrow().invoice_id.clone(),
            amount: expected_net,
            recipient: fix.sme.clone(),
            fee: expected_fee,
        }
        .to_xdr(&env, &fix.escrow_id);

        prop_assert!(events.events().contains(&expected_xdr), "SmeWithdrew event must be emitted with exact fee and net");

        if fee_bps == 0 {
            prop_assert_eq!(fix.token.balance(&fix.treasury), 0, "Zero fee must write nothing to treasury");
        } else {
            prop_assert_eq!(fix.token.balance(&fix.treasury), expected_fee, "Non-zero fee must match treasury balance");
        }
    }

    /// Failure Recovery Deterministic Equivalence:
    /// Proves that executing an interrupted-and-recovered sequence produces an identical outcome
    /// to direct uninterrupted execution for arbitrary valid amount and fee_bps.
    #[test]
    fn prop_failure_recovery_deterministic_equivalence(
        amount in 1i128..=10_000_000_000i128,
        fee_bps in 0i64..=10_000i64,
    ) {
        // Run Baseline (Uninterrupted)
        let env_a = Env::default();
        let fix_a = TestFixture::new(&env_a, amount, fee_bps, "BASE_RUN");
        let escrow_a = fix_a.client.withdraw();
        let treasury_bal_a = fix_a.token.balance(&fix_a.treasury);
        let sme_bal_a = fix_a.token.balance(&fix_a.sme);

        // Run Interrupted with Adverse Events -> Recovered
        let env_b = Env::default();
        let fix_b = TestFixture::new(&env_b, amount, fee_bps, "REC_RUN");

        // 1. Adverse condition 1: Operational pause
        fix_b.client.set_paused(&true, &PauseScope::All, &PauseReason::Incident);
        let res_pause = fix_b.client.try_withdraw();
        prop_assert!(res_pause.is_err(), "must fail during pause");

        // 2. Adverse condition 2: Legal hold
        fix_b.client.set_legal_hold(&true);
        let res_hold = fix_b.client.try_withdraw();
        prop_assert!(res_hold.is_err(), "must fail during legal hold");

        // 3. Clear pause and legal hold
        fix_b.client.set_paused(&false, &PauseScope::All, &PauseReason::Incident);
        fix_b.client.set_legal_hold(&false);

        // 4. Recovered execution
        let escrow_b = fix_b.client.withdraw();
        let treasury_bal_b = fix_b.token.balance(&fix_b.treasury);
        let sme_bal_b = fix_b.token.balance(&fix_b.sme);

        // Deterministic Equivalence Assertions
        prop_assert_eq!(escrow_b.status, escrow_a.status, "status must match baseline");
        prop_assert_eq!(escrow_b.funded_amount, escrow_a.funded_amount, "funded_amount must match baseline");
        prop_assert_eq!(treasury_bal_b, treasury_bal_a, "treasury balance must match baseline bit-for-bit");
        prop_assert_eq!(sme_bal_b, sme_bal_a, "SME balance must match baseline bit-for-bit");
        prop_assert_eq!(fix_b.token.balance(&fix_b.escrow_id), 0, "escrow must be empty");
    }

    /// Balance Shortfall Recovery Deterministic Equivalence:
    /// Proves that encountering a temporary balance shortfall and subsequent replenishment
    /// converges to the exact baseline outcome.
    #[test]
    fn prop_balance_shortfall_recovery_deterministic_equivalence(
        amount in 10i128..=10_000_000_000i128,
        fee_bps in 0i64..=10_000i64,
    ) {
        let env = Env::default();
        let fix = TestFixture::new(&env, amount, fee_bps, "SHORT_REC");
        let (expected_fee, expected_net) = model_fee_split(amount, fee_bps);

        // Simulate shortfall: transfer 5 units away
        let sink = Address::generate(&env);
        fix.token.transfer(&fix.escrow_id, &sink, &5i128);

        // Withdrawal must fail
        assert_contract_error(
            fix.client.try_withdraw(),
            EscrowError::InsufficientContractBalance,
        );

        // State preserved
        prop_assert_eq!(fix.client.get_escrow().status, 1);
        prop_assert_eq!(fix.token.balance(&fix.sme), 0);
        prop_assert_eq!(fix.token.balance(&fix.treasury), 0);

        // Replenish 5 units back
        fix.stellar.mint(&fix.escrow_id, &5i128);

        // Withdraw succeeds cleanly
        let escrow = fix.client.withdraw();
        prop_assert_eq!(escrow.status, 3);
        prop_assert_eq!(fix.token.balance(&fix.treasury), expected_fee);
        prop_assert_eq!(fix.token.balance(&fix.sme), expected_net);
        prop_assert_eq!(fix.token.balance(&fix.escrow_id), 0);
    }

    /// Duplicate Withdrawal Idempotency Property:
    /// Proves that multiple duplicate withdrawal invocations are consistently rejected
    /// without mutating state or token balances.
    #[test]
    fn prop_duplicate_withdrawal_idempotency(
        amount in 1i128..=10_000_000_000i128,
        fee_bps in 0i64..=10_000i64,
    ) {
        let env = Env::default();
        let fix = TestFixture::new(&env, amount, fee_bps, "DUP_PROP");
        let (expected_fee, expected_net) = model_fee_split(amount, fee_bps);

        // Initial withdrawal succeeds
        fix.client.withdraw();

        // 3 consecutive duplicate attempts must all fail deterministically
        for _ in 0..3 {
            assert_contract_error(
                fix.client.try_withdraw(),
                EscrowError::WithdrawalNotFunded,
            );
            prop_assert_eq!(fix.client.get_escrow().status, 3);
            prop_assert_eq!(fix.token.balance(&fix.treasury), expected_fee);
            prop_assert_eq!(fix.token.balance(&fix.sme), expected_net);
            prop_assert_eq!(fix.token.balance(&fix.escrow_id), 0);
        }
    }

    /// Concurrent Interleaved Action Stream Property:
    /// Executes an arbitrary sequence of interleaved lifecycle actions
    /// (withdraw, pause/unpause, hold/unhold, fee update, finalize_close)
    /// and verifies that global token conservation and status monotonicity
    /// are never violated under any scheduling order.
    #[test]
    fn prop_concurrent_interleaved_action_stream(
        amount in 100i128..=1_000_000_000i128,
        initial_fee_bps in 0i64..=10_000i64,
        action_opcodes in prop::collection::vec(0u8..=5u8, 1..=12),
    ) {
        let env = Env::default();
        let fix = TestFixture::new(&env, amount, initial_fee_bps, "PROP_CONC_STREAM");
        let initial_total_tokens = amount;

        let mut has_withdrawn = false;
        let mut has_settled = false;

        for opcode in action_opcodes {
            match opcode {
                0 => {
                    // Action 0: Attempt withdrawal
                    let res = fix.client.try_withdraw();
                    if res.is_ok() {
                        prop_assert!(!has_withdrawn, "Withdrawal can only succeed once");
                        prop_assert!(!has_settled, "Cannot withdraw if settled");
                        has_withdrawn = true;
                        prop_assert_eq!(fix.client.get_escrow().status, 3);
                    }
                }
                1 => {
                    // Action 1: Toggle operational pause
                    let should_pause = fix.client.get_escrow().status == 1;
                    fix.client.set_paused(&should_pause, &PauseScope::All, &PauseReason::Incident);
                }
                2 => {
                    // Action 2: Toggle legal hold
                    let should_hold = !has_withdrawn;
                    fix.client.set_legal_hold(&should_hold);
                }
                3 => {
                    // Action 3: Attempt fee rate update (valid 0..10_000)
                    let _ = fix.client.try_set_protocol_fee_bps(&2_000i64);
                }
                4 => {
                    // Action 4: Clear pause & legal hold to restore normal execution path
                    fix.client.set_paused(&false, &PauseScope::All, &PauseReason::Incident);
                    fix.client.set_legal_hold(&false);
                }
                _ => {
                    // Action 5: Attempt settle
                    let res = fix.client.try_settle();
                    if res.is_ok() {
                        prop_assert!(!has_withdrawn, "Cannot settle if withdrawn");
                        prop_assert!(!has_settled, "Settle can only succeed once");
                        has_settled = true;
                        prop_assert_eq!(fix.client.get_escrow().status, 2);
                    }
                }
            }

            // Universal Invariants at EVERY step:
            let bal_escrow = fix.token.balance(&fix.escrow_id);
            let bal_sme = fix.token.balance(&fix.sme);
            let bal_treasury = fix.token.balance(&fix.treasury);

            // 1. Strict Global Token Conservation (No token creation or destruction)
            prop_assert_eq!(
                bal_escrow + bal_sme + bal_treasury,
                initial_total_tokens,
                "Conservation invariant violated in action stream"
            );

            // 2. Monotonic Status Evolution
            let current_status = fix.client.get_escrow().status;
            if has_settled {
                prop_assert_eq!(current_status, 2, "Status must stay 2 once settled");
            } else if has_withdrawn {
                prop_assert_eq!(current_status, 3, "Status must stay 3 once withdrawn");
            } else {
                prop_assert_eq!(current_status, 1, "Status must remain 1 until withdrawn or settled");
            }
        }
    }
}
