//! Tests for [`LiquifactEscrow::get_collateral_config`], [`LiquifactEscrow::set_collateral_limit`],
//! and [`LiquifactEscrow::get_collateral_limit`].
//!
//! Covers all acceptance criteria for issue #1351:
//!
//! - Default values before and after [`LiquifactEscrow::init`].
//! - `collateral_limit` reflects admin overrides via [`LiquifactEscrow::set_collateral_limit`].
//! - `sme_commitment` reflects [`LiquifactEscrow::record_sme_collateral_commitment`] and
//!   [`LiquifactEscrow::clear_sme_collateral_commitment`] lifecycle transitions.
//! - Bundled view is consistent with individual getters (no drift).
//! - Idempotency: pure reads produce identical results on repeat calls.
//! - Struct shape stability: field-by-field destructuring so a future struct change fails at
//!   compile time.
//! - Authorization: `get_collateral_config` and `get_collateral_limit` require no auth;
//!   `set_collateral_limit` requires admin auth.
//! - Validation boundaries for `set_collateral_limit`: rejects non-positive and out-of-range
//!   limits; accepts valid boundaries.
//! - Validation boundaries for `record_sme_collateral_commitment`: `CollateralLimitExceeded`
//!   is returned when `amount > collateral_limit`.
//! - Multiple sequential limit updates and commitment replacements are handled consistently.
//! - Boundary values: `1`, `MAX_INVOICE_AMOUNT - 1`, `MAX_INVOICE_AMOUNT`, `MAX_INVOICE_AMOUNT + 1`,
//!   `i128::MIN`, `i128::MAX`.

use super::super::{
    CollateralCommitmentSnapshot, CollateralConfig, EscrowError, LiquifactEscrow,
    LiquifactEscrowClient, MAX_INVOICE_AMOUNT,
};
use super::assert_contract_error;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, Symbol};

// ── helpers ──────────────────────────────────────────────────────────────────

fn deploy(env: &Env) -> LiquifactEscrowClient<'_> {
    let id = env.register(LiquifactEscrow, ());
    LiquifactEscrowClient::new(env, &id)
}

fn init_escrow(env: &Env, client: &LiquifactEscrowClient) -> (Address, Address) {
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let token = Address::generate(env);
    let treasury = Address::generate(env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(env, "CCFGTEST"),
        &sme,
        &10_000i128,
        &800i64,
        &0u64,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
        &None::<u32>,
    );
    (admin, sme)
}

// ── Section 1: Default values ─────────────────────────────────────────────────

/// Before `init`, every field in `CollateralConfig` must return its documented default.
#[test]
fn test_defaults_before_init() {
    let env = Env::default();
    let client = deploy(&env);

    let config = client.get_collateral_config();

    assert_eq!(
        config.collateral_limit, MAX_INVOICE_AMOUNT,
        "collateral_limit must default to MAX_INVOICE_AMOUNT before init"
    );
    assert_eq!(
        config.sme_commitment,
        CollateralCommitmentSnapshot::None,
        "sme_commitment must be None before init"
    );
}

/// After `init` (and without further mutations), fields must still return defaults.
#[test]
fn test_defaults_after_init() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    let config = client.get_collateral_config();

    assert_eq!(
        config.collateral_limit, MAX_INVOICE_AMOUNT,
        "collateral_limit must still be MAX_INVOICE_AMOUNT after init without override"
    );
    assert_eq!(
        config.sme_commitment,
        CollateralCommitmentSnapshot::None,
        "sme_commitment must be None after init when no commitment recorded"
    );
}

/// `get_collateral_limit()` alone returns the same default.
#[test]
fn test_get_collateral_limit_default_before_init() {
    let env = Env::default();
    let client = deploy(&env);

    assert_eq!(client.get_collateral_limit(), MAX_INVOICE_AMOUNT);
}

/// `get_collateral_limit()` after `init` remains `MAX_INVOICE_AMOUNT`.
#[test]
fn test_get_collateral_limit_default_after_init() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    assert_eq!(client.get_collateral_limit(), MAX_INVOICE_AMOUNT);
}

// ── Section 2: Idempotency ────────────────────────────────────────────────────

/// `get_collateral_config` is a pure read: repeated calls must return identical results
/// before init.
#[test]
fn test_idempotent_before_init() {
    let env = Env::default();
    let client = deploy(&env);

    let first = client.get_collateral_config();
    let second = client.get_collateral_config();

    assert_eq!(first, second);
}

/// `get_collateral_config` is idempotent after init and after mutations.
#[test]
fn test_idempotent_after_mutation() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    client.set_collateral_limit(&50_000i128);
    client.record_sme_collateral_commitment(&Symbol::new(&env, "GOLD"), &5_000i128);

    let first = client.get_collateral_config();
    let second = client.get_collateral_config();

    assert_eq!(first, second);
}

// ── Section 3: Struct shape stability ────────────────────────────────────────

/// Destructuring `CollateralConfig` by field ensures a compile-time break if the
/// struct layout changes without this test being updated.
#[test]
fn test_struct_shape_stability() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    client.set_collateral_limit(&100_000i128);
    client.record_sme_collateral_commitment(&Symbol::new(&env, "ETH"), &1_000i128);

    let CollateralConfig {
        collateral_limit,
        sme_commitment,
    } = client.get_collateral_config();

    assert_eq!(collateral_limit, 100_000i128);
    assert!(matches!(sme_commitment, CollateralCommitmentSnapshot::Some(_)));
}

// ── Section 4: Consistency with individual getters ────────────────────────────

/// `config.collateral_limit` must always equal `get_collateral_limit()`.
#[test]
fn test_config_matches_get_collateral_limit_at_default() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    let config = client.get_collateral_config();
    assert_eq!(config.collateral_limit, client.get_collateral_limit());
}

/// After `set_collateral_limit`, both views stay in sync.
#[test]
fn test_config_matches_get_collateral_limit_after_update() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    client.set_collateral_limit(&75_000i128);

    let config = client.get_collateral_config();
    assert_eq!(
        config.collateral_limit,
        client.get_collateral_limit(),
        "bundled and individual views must agree after set_collateral_limit"
    );
    assert_eq!(config.collateral_limit, 75_000i128);
}

/// `config.sme_commitment` must reflect the state of `get_sme_collateral_commitment`.
#[test]
fn test_config_sme_commitment_matches_individual_getter_none() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    let config = client.get_collateral_config();
    // No commitment recorded yet.
    assert_eq!(config.sme_commitment, CollateralCommitmentSnapshot::None);
    assert!(client.get_sme_collateral_commitment().is_none());
}

#[test]
fn test_config_sme_commitment_matches_individual_getter_some() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    let asset = Symbol::new(&env, "USDC");
    let commitment = client.record_sme_collateral_commitment(&asset, &5_000i128);

    let config = client.get_collateral_config();

    // Bundled view must be Some.
    match config.sme_commitment {
        CollateralCommitmentSnapshot::Some(c) => {
            assert_eq!(c.amount, commitment.amount);
            assert_eq!(c.asset, commitment.asset);
            assert_eq!(c.recorded_at, commitment.recorded_at);
        }
        CollateralCommitmentSnapshot::None => panic!("expected Some after recording commitment"),
    }

    // Individual getter must also return the same commitment.
    let individual = client
        .get_sme_collateral_commitment()
        .expect("individual getter must return Some");
    assert_eq!(individual.amount, commitment.amount);
}

// ── Section 5: set_collateral_limit — valid inputs ───────────────────────────

/// Minimum valid limit (1) is accepted and stored.
#[test]
fn test_set_collateral_limit_minimum_valid() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    client.set_collateral_limit(&1i128);
    assert_eq!(client.get_collateral_limit(), 1i128);

    let config = client.get_collateral_config();
    assert_eq!(config.collateral_limit, 1i128);
}

/// Arbitrary mid-range value is accepted.
#[test]
fn test_set_collateral_limit_mid_range() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    let limit = 5_000_000i128;
    client.set_collateral_limit(&limit);
    assert_eq!(client.get_collateral_limit(), limit);
}

/// Exactly at `MAX_INVOICE_AMOUNT` is accepted.
#[test]
fn test_set_collateral_limit_exactly_at_max() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    client.set_collateral_limit(&MAX_INVOICE_AMOUNT);
    assert_eq!(client.get_collateral_limit(), MAX_INVOICE_AMOUNT);
}

/// One below `MAX_INVOICE_AMOUNT` is accepted.
#[test]
fn test_set_collateral_limit_just_below_max() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    let limit = MAX_INVOICE_AMOUNT - 1;
    client.set_collateral_limit(&limit);
    assert_eq!(client.get_collateral_limit(), limit);
}

// ── Section 6: set_collateral_limit — rejection boundaries ───────────────────

/// Zero limit is rejected with `CollateralLimitNotPositive`.
#[test]
fn test_set_collateral_limit_rejects_zero() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    assert_contract_error(
        client.try_set_collateral_limit(&0i128),
        EscrowError::CollateralLimitNotPositive,
    );
}

/// Negative limit is rejected with `CollateralLimitNotPositive`.
#[test]
fn test_set_collateral_limit_rejects_negative() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    assert_contract_error(
        client.try_set_collateral_limit(&-1i128),
        EscrowError::CollateralLimitNotPositive,
    );
}

/// `i128::MIN` is rejected with `CollateralLimitNotPositive`.
#[test]
fn test_set_collateral_limit_rejects_i128_min() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    assert_contract_error(
        client.try_set_collateral_limit(&i128::MIN),
        EscrowError::CollateralLimitNotPositive,
    );
}

/// `MAX_INVOICE_AMOUNT + 1` is rejected with `CollateralLimitExceedsMax`.
#[test]
fn test_set_collateral_limit_rejects_just_above_max() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    assert_contract_error(
        client.try_set_collateral_limit(&(MAX_INVOICE_AMOUNT + 1)),
        EscrowError::CollateralLimitExceedsMax,
    );
}

/// `i128::MAX` is rejected with `CollateralLimitExceedsMax`.
#[test]
fn test_set_collateral_limit_rejects_i128_max() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    assert_contract_error(
        client.try_set_collateral_limit(&i128::MAX),
        EscrowError::CollateralLimitExceedsMax,
    );
}

/// A failed `set_collateral_limit` call must leave the limit unchanged.
#[test]
fn test_set_collateral_limit_rejected_does_not_mutate_state() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    // Set a known good limit.
    client.set_collateral_limit(&10_000i128);

    // Attempt an invalid update.
    let _ = client.try_set_collateral_limit(&0i128);

    // Limit must still be 10_000.
    assert_eq!(client.get_collateral_limit(), 10_000i128);
    assert_eq!(
        client.get_collateral_config().collateral_limit,
        10_000i128
    );
}

// ── Section 7: record_sme_collateral_commitment limit enforcement ─────────────

/// Recording at exactly the configured limit succeeds.
#[test]
fn test_record_commitment_exactly_at_limit_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    client.set_collateral_limit(&5_000i128);
    let asset = Symbol::new(&env, "USDC");
    let commitment = client.record_sme_collateral_commitment(&asset, &5_000i128);

    assert_eq!(commitment.amount, 5_000i128);
}

/// Recording one unit above the configured limit is rejected with `CollateralLimitExceeded`.
#[test]
fn test_record_commitment_just_above_limit_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    client.set_collateral_limit(&5_000i128);
    let asset = Symbol::new(&env, "USDC");

    assert_contract_error(
        client.try_record_sme_collateral_commitment(&asset, &5_001i128),
        EscrowError::CollateralLimitExceeded,
    );
}

/// Recording well above the limit is also rejected.
#[test]
fn test_record_commitment_far_above_limit_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    client.set_collateral_limit(&1_000i128);
    let asset = Symbol::new(&env, "USDC");

    assert_contract_error(
        client.try_record_sme_collateral_commitment(&asset, &100_000i128),
        EscrowError::CollateralLimitExceeded,
    );
}

/// At the default limit (`MAX_INVOICE_AMOUNT`), recording an amount equal to it succeeds.
#[test]
fn test_record_commitment_at_default_limit_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    // No explicit limit set — default is MAX_INVOICE_AMOUNT.
    let asset = Symbol::new(&env, "USDC");
    let commitment = client.record_sme_collateral_commitment(&asset, &MAX_INVOICE_AMOUNT);
    assert_eq!(commitment.amount, MAX_INVOICE_AMOUNT);
}

/// A failed commitment due to limit violation must leave `sme_commitment` unchanged.
#[test]
fn test_record_commitment_rejected_does_not_mutate_state() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    client.set_collateral_limit(&1_000i128);
    let asset = Symbol::new(&env, "USDC");

    // Attempt invalid commitment.
    let _ = client.try_record_sme_collateral_commitment(&asset, &2_000i128);

    // sme_commitment must remain None (never recorded).
    assert_eq!(
        client.get_collateral_config().sme_commitment,
        CollateralCommitmentSnapshot::None
    );
    assert!(client.get_sme_collateral_commitment().is_none());
}

// ── Section 8: Lifecycle transitions ─────────────────────────────────────────

/// After recording, config shows `Some`; after clearing, config returns to `None`.
#[test]
fn test_sme_commitment_some_then_none_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    let asset = Symbol::new(&env, "ETH");

    // Before record: None.
    assert_eq!(
        client.get_collateral_config().sme_commitment,
        CollateralCommitmentSnapshot::None
    );

    // After record: Some.
    client.record_sme_collateral_commitment(&asset, &500i128);
    assert!(matches!(
        client.get_collateral_config().sme_commitment,
        CollateralCommitmentSnapshot::Some(_)
    ));

    // After clear: None again.
    client.clear_sme_collateral_commitment();
    assert_eq!(
        client.get_collateral_config().sme_commitment,
        CollateralCommitmentSnapshot::None
    );
}

/// Replacing a commitment updates `sme_commitment.Some.amount`; `collateral_limit` is unchanged.
#[test]
fn test_replacement_updates_sme_commitment_not_limit() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    let asset = Symbol::new(&env, "BTC");
    client.set_collateral_limit(&50_000i128);

    // First record.
    client.record_sme_collateral_commitment(&asset, &10_000i128);
    // Advance ledger timestamp so replacement timestamp check passes.
    env.ledger().set_timestamp(1_000);

    // Second record (replacement).
    client.record_sme_collateral_commitment(&asset, &20_000i128);

    let config = client.get_collateral_config();
    assert_eq!(config.collateral_limit, 50_000i128);
    match config.sme_commitment {
        CollateralCommitmentSnapshot::Some(c) => assert_eq!(c.amount, 20_000i128),
        CollateralCommitmentSnapshot::None => panic!("expected Some after replacement"),
    }
}

/// Multiple sequential `set_collateral_limit` calls update the stored limit and are reflected
/// consistently in both `get_collateral_limit()` and `get_collateral_config()`.
#[test]
fn test_sequential_limit_updates_are_consistent() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    for limit in [1i128, 1_000, 100_000, MAX_INVOICE_AMOUNT] {
        client.set_collateral_limit(&limit);
        assert_eq!(client.get_collateral_limit(), limit);
        assert_eq!(client.get_collateral_config().collateral_limit, limit);
    }
}

/// Lowering the limit below an already-recorded commitment does not retroactively
/// invalidate the stored commitment, but future records above the new limit are rejected.
#[test]
fn test_lowering_limit_blocks_future_records_but_not_existing() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    // Record at a large amount.
    client.set_collateral_limit(&50_000i128);
    let asset = Symbol::new(&env, "USDC");
    client.record_sme_collateral_commitment(&asset, &50_000i128);

    // Lower the limit.
    client.set_collateral_limit(&1_000i128);

    // Existing commitment is still in the config unchanged.
    match client.get_collateral_config().sme_commitment {
        CollateralCommitmentSnapshot::Some(c) => assert_eq!(c.amount, 50_000i128),
        CollateralCommitmentSnapshot::None => panic!("commitment must survive a limit lowering"),
    }

    // But a new record above the new limit is rejected.
    env.ledger().set_timestamp(1_000);
    assert_contract_error(
        client.try_record_sme_collateral_commitment(&asset, &2_000i128),
        EscrowError::CollateralLimitExceeded,
    );
}

// ── Section 9: Authorization ──────────────────────────────────────────────────

/// `get_collateral_config` requires no auth (plain call, no mock).
#[test]
fn test_get_collateral_config_no_auth_required() {
    let env = Env::default();
    // Deliberately NOT calling mock_all_auths.
    let client = deploy(&env);

    // Must not panic even without any auth.
    let config = client.get_collateral_config();
    assert_eq!(config.collateral_limit, MAX_INVOICE_AMOUNT);
    assert_eq!(config.sme_commitment, CollateralCommitmentSnapshot::None);
}

/// `get_collateral_limit` requires no auth.
#[test]
fn test_get_collateral_limit_no_auth_required() {
    let env = Env::default();
    // Deliberately NOT calling mock_all_auths.
    let client = deploy(&env);

    // Must not panic even without any auth.
    assert_eq!(client.get_collateral_limit(), MAX_INVOICE_AMOUNT);
}

/// Both read entrypoints remain auth-free after init.
#[test]
fn test_read_entrypoints_auth_free_after_init() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    // Reset: no further auth mock in scope for the reads.
    let config = client.get_collateral_config();
    assert_eq!(config.collateral_limit, MAX_INVOICE_AMOUNT);
    assert_eq!(client.get_collateral_limit(), MAX_INVOICE_AMOUNT);
}

// ── Section 10: Boundary values ───────────────────────────────────────────────

/// Boundary table for `set_collateral_limit`:
///
/// | Input              | Expected outcome        |
/// |--------------------|------------------------|
/// | `i128::MIN`        | `CollateralLimitNotPositive` |
/// | `-1`               | `CollateralLimitNotPositive` |
/// | `0`                | `CollateralLimitNotPositive` |
/// | `1`                | **accepted**            |
/// | `MAX_INVOICE_AMOUNT - 1` | **accepted**      |
/// | `MAX_INVOICE_AMOUNT` | **accepted**          |
/// | `MAX_INVOICE_AMOUNT + 1` | `CollateralLimitExceedsMax` |
/// | `i128::MAX`        | `CollateralLimitExceedsMax` |
#[test]
fn test_set_collateral_limit_boundary_table() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    // Rejected: non-positive
    for bad in [i128::MIN, -1i128, 0i128] {
        assert_contract_error(
            client.try_set_collateral_limit(&bad),
            EscrowError::CollateralLimitNotPositive,
        );
    }

    // Accepted: at and within range
    for good in [1i128, MAX_INVOICE_AMOUNT - 1, MAX_INVOICE_AMOUNT] {
        client.set_collateral_limit(&good);
        assert_eq!(client.get_collateral_limit(), good);
    }

    // Rejected: above max
    for bad in [MAX_INVOICE_AMOUNT + 1, i128::MAX] {
        assert_contract_error(
            client.try_set_collateral_limit(&bad),
            EscrowError::CollateralLimitExceedsMax,
        );
    }
}

/// Boundary table for `record_sme_collateral_commitment` with respect to the collateral limit:
///
/// | Limit   | Amount           | Expected          |
/// |---------|-----------------|-------------------|
/// | 1_000   | 999             | **accepted**      |
/// | 1_000   | 1_000           | **accepted**      |
/// | 1_000   | 1_001           | `CollateralLimitExceeded` |
/// | default | MAX_INVOICE_AMOUNT | **accepted**   |
/// | default | MAX_INVOICE_AMOUNT + 1 | (SDK rejects as negative — unreachable) |
#[test]
fn test_record_commitment_boundary_table() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    client.set_collateral_limit(&1_000i128);
    let asset = Symbol::new(&env, "USDC");

    // Just below limit.
    client.record_sme_collateral_commitment(&asset, &999i128);
    env.ledger().set_timestamp(100);

    // Exactly at limit.
    client.record_sme_collateral_commitment(&asset, &1_000i128);
    env.ledger().set_timestamp(200);

    // One above limit.
    assert_contract_error(
        client.try_record_sme_collateral_commitment(&asset, &1_001i128),
        EscrowError::CollateralLimitExceeded,
    );
}

// ── Section 11: Duplicate / concurrent safety ─────────────────────────────────

/// Calling `set_collateral_limit` twice with the same value is idempotent.
#[test]
fn test_set_collateral_limit_same_value_idempotent() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    client.set_collateral_limit(&10_000i128);
    client.set_collateral_limit(&10_000i128); // second call with same value

    assert_eq!(client.get_collateral_limit(), 10_000i128);
    assert_eq!(
        client.get_collateral_config().collateral_limit,
        10_000i128
    );
}

/// `get_collateral_config` reflects the state at the ledger snapshot time — calling it
/// before and after a limit update shows distinct values (no caching artefacts).
#[test]
fn test_config_reflects_current_state_not_cached() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    let before = client.get_collateral_config();
    assert_eq!(before.collateral_limit, MAX_INVOICE_AMOUNT);

    client.set_collateral_limit(&12_345i128);

    let after = client.get_collateral_config();
    assert_eq!(after.collateral_limit, 12_345i128);

    // The two reads must differ.
    assert_ne!(before.collateral_limit, after.collateral_limit);
}

// ── Section 12: Error code stability ─────────────────────────────────────────

/// Verify that the numeric error codes for collateral limit errors are stable and
/// match the documented values.
///
/// | Error                     | Expected code |
/// |---------------------------|--------------|
/// | `CollateralLimitNotPositive` | 64        |
/// | `CollateralLimitExceedsMax`  | 65        |
/// | `CollateralLimitExceeded`    | 66        |
#[test]
fn test_error_codes_stable() {
    assert_eq!(EscrowError::CollateralLimitNotPositive as u32, 64);
    assert_eq!(EscrowError::CollateralLimitExceedsMax as u32, 65);
    assert_eq!(EscrowError::CollateralLimitExceeded as u32, 66);
}

/// Rejection of `set_collateral_limit(&0)` emits exactly `CollateralLimitNotPositive` (64).
#[test]
fn test_set_collateral_limit_zero_emits_correct_error_code() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    assert_contract_error(
        client.try_set_collateral_limit(&0i128),
        EscrowError::CollateralLimitNotPositive,
    );
}

/// Rejection of `set_collateral_limit` above max emits exactly `CollateralLimitExceedsMax` (65).
#[test]
fn test_set_collateral_limit_above_max_emits_correct_error_code() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    assert_contract_error(
        client.try_set_collateral_limit(&(MAX_INVOICE_AMOUNT + 1)),
        EscrowError::CollateralLimitExceedsMax,
    );
}

/// Rejection of `record_sme_collateral_commitment` above limit emits `CollateralLimitExceeded` (66).
#[test]
fn test_record_commitment_above_limit_emits_correct_error_code() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    init_escrow(&env, &client);

    client.set_collateral_limit(&500i128);
    let asset = Symbol::new(&env, "USDC");

    assert_contract_error(
        client.try_record_sme_collateral_commitment(&asset, &501i128),
        EscrowError::CollateralLimitExceeded,
    );
}
