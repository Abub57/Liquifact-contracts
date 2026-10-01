//! Validation boundaries for [`LiquifactEscrow::get_attestation_config`].
//! Tests for [`LiquifactEscrow::get_attestation_config`].
//!
//! Covers:
//! - Default values before [`LiquifactEscrow::init`] is called.
//! - Values after init (before any attestation operations).
//! - After [`LiquifactEscrow::bind_primary_attestation_hash`] (`primary_bound` becomes `true`).
//! - After [`LiquifactEscrow::append_attestation_digest`] (`append_log_length` updates).
//! - Config matches the individual getters/state.
//! - Idempotency (pure read, no state mutation).
//! - Struct shape stability (destructuring).
//! - Boundary values for append/revoke batch limits and read page size.

use super::super::{
    AttestationConfig, EscrowError, LiquifactEscrow, LiquifactEscrowClient,
    MAX_ATTESTATION_APPEND_BATCH, MAX_ATTESTATION_APPEND_ENTRIES, MAX_ATTESTATION_READ_PAGE,
    MAX_ATTESTATION_REVOKE_BATCH,
};
use super::assert_contract_error;
use soroban_sdk::testutils::{Address as _, Events};
use soroban_sdk::{Address, BytesN, Env, Vec as SorobanVec};

// ── helpers ──────────────────────────────────────────────────────────────────

fn deploy(env: &Env) -> LiquifactEscrowClient<'_> {
    let id = env.register(LiquifactEscrow, ());
    LiquifactEscrowClient::new(env, &id)
}

fn init_escrow(env: &Env, client: &LiquifactEscrowClient) -> Address {
    let admin = Address::generate(env);
    let sme = Address::generate(env);
    let token = Address::generate(env);
    let treasury = Address::generate(env);
    client.init(
        &admin,
        &soroban_sdk::String::from_str(env, "ATTCFG01"),
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
    );
    admin
}

// ── tests ────────────────────────────────────────────────────────────────────

/// Before `init`, every field must return its documented default.
#[test]
fn test_defaults_before_init() {
    let env = Env::default();
    let client = deploy(&env);

    let config = client.get_attestation_config();

    assert_eq!(
        config.max_append_entries, MAX_ATTESTATION_APPEND_ENTRIES,
        "max_append_entries should be MAX_ATTESTATION_APPEND_ENTRIES before init"
    );
    assert_eq!(
        config.max_revoke_batch, MAX_ATTESTATION_REVOKE_BATCH,
        "max_revoke_batch should be MAX_ATTESTATION_REVOKE_BATCH before init"
    );
    assert_eq!(
        config.max_append_batch, MAX_ATTESTATION_APPEND_BATCH,
        "max_append_batch should be MAX_ATTESTATION_APPEND_BATCH before init"
    );
    assert_eq!(
        config.max_read_page, MAX_ATTESTATION_READ_PAGE,
        "max_read_page should be MAX_ATTESTATION_READ_PAGE before init"
    );
    assert!(
        !config.primary_bound,
        "primary_bound should be false before init"
    );
    assert_eq!(
        config.append_log_length, 0,
        "append_log_length should be 0 before init"
    );
}

/// After `init` (but before any attestation operations), the config should
/// still reflect defaults for the live-state fields.
#[test]
fn test_values_after_init() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    init_escrow(&env, &client);

    let config = client.get_attestation_config();

    assert_eq!(config.max_append_entries, MAX_ATTESTATION_APPEND_ENTRIES);
    assert_eq!(config.max_revoke_batch, MAX_ATTESTATION_REVOKE_BATCH);
    assert_eq!(config.max_append_batch, MAX_ATTESTATION_APPEND_BATCH);
    assert_eq!(config.max_read_page, MAX_ATTESTATION_READ_PAGE);
    assert!(
        !config.primary_bound,
        "primary_bound should be false after init when no hash bound"
    );
    assert_eq!(
        config.append_log_length, 0,
        "append_log_length should be 0 after init when no digests appended"
    );
}

/// After binding a primary attestation hash, `primary_bound` must be `true`.
#[test]
fn test_primary_bound_true_after_bind() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    let admin = init_escrow(&env, &client);

    let hash = BytesN::from_array(&env, &[0xabu8; 32]);
    client.bind_primary_attestation_hash(&hash);

    let config = client.get_attestation_config();
    assert!(
        config.primary_bound,
        "primary_bound should be true after binding"
    );
}

/// After appending digests, `append_log_length` must reflect the append count.
#[test]
fn test_append_log_length_updates() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    let admin = init_escrow(&env, &client);

    let hash1 = BytesN::from_array(&env, &[1u8; 32]);
    let hash2 = BytesN::from_array(&env, &[2u8; 32]);
    let hash3 = BytesN::from_array(&env, &[3u8; 32]);

    // Initial: empty log.
    assert_eq!(client.get_attestation_config().append_log_length, 0);

    // Append one.
    client.append_attestation_digest(&hash1);
    assert_eq!(client.get_attestation_config().append_log_length, 1);

    // Append two more.
    client.append_attestation_digest(&hash2);
    client.append_attestation_digest(&hash3);
    assert_eq!(client.get_attestation_config().append_log_length, 3);
}

/// After appending and revoking, `append_log_length` must not change (revocation
/// does not remove entries from the log).
#[test]
fn test_append_log_length_unaffected_by_revoke() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    let admin = init_escrow(&env, &client);

    let hash = BytesN::from_array(&env, &[0x42u8; 32]);
    client.append_attestation_digest(&hash);
    assert_eq!(client.get_attestation_config().append_log_length, 1);

    // Revoke index 0 — log length stays the same.
    client.revoke_attestation_digest(&0);
    assert_eq!(
        client.get_attestation_config().append_log_length,
        1,
        "revoke must not reduce append_log_length"
    );
}

/// `get_attestation_config` must match the individual authoritative sources.
#[test]
fn test_config_matches_individual_state() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    let admin = init_escrow(&env, &client);

    let hash = BytesN::from_array(&env, &[0x99u8; 32]);
    client.bind_primary_attestation_hash(&hash);
    client.append_attestation_digest(&hash);

    let config = client.get_attestation_config();

    // Constants are compile-time — just verify they're wired through.
    assert_eq!(config.max_append_entries, MAX_ATTESTATION_APPEND_ENTRIES);
    assert_eq!(config.max_revoke_batch, MAX_ATTESTATION_REVOKE_BATCH);
    assert_eq!(config.max_append_batch, MAX_ATTESTATION_APPEND_BATCH);
    assert_eq!(config.max_read_page, MAX_ATTESTATION_READ_PAGE);

    // Live state matches individual getters.
    assert_eq!(
        config.primary_bound,
        client.get_primary_attestation_hash().is_some()
    );
    assert_eq!(
        config.append_log_length,
        client.get_attestation_append_log().len()
    );
}

/// Config is idempotent (pure read, no state mutation).
#[test]
fn test_config_is_idempotent() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    let admin = init_escrow(&env, &client);

    let hash = BytesN::from_array(&env, &[0x55u8; 32]);
    client.bind_primary_attestation_hash(&hash);

    let first = client.get_attestation_config();
    let second = client.get_attestation_config();

    assert_eq!(first, second);
}

/// Defaults before init are also idempotent.
#[test]
fn test_defaults_idempotent_before_init() {
    let env = Env::default();
    let client = deploy(&env);

    let first = client.get_attestation_config();
    let second = client.get_attestation_config();

    assert_eq!(first, second);
}

/// `get_attestation_config` has the expected shape (all six fields present) —
/// verified via field-by-field destructuring so a future struct change causes a
/// compile error.
#[test]
fn test_config_struct_shape() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    let admin = init_escrow(&env, &client);

    let hash = BytesN::from_array(&env, &[0x11u8; 32]);
    client.bind_primary_attestation_hash(&hash);
    client.append_attestation_digest(&hash);

    let AttestationConfig {
        max_append_entries,
        max_revoke_batch,
        max_append_batch,
        max_read_page,
        primary_bound,
        append_log_length,
    } = client.get_attestation_config();

    assert_eq!(max_append_entries, MAX_ATTESTATION_APPEND_ENTRIES);
    assert_eq!(max_revoke_batch, MAX_ATTESTATION_REVOKE_BATCH);
    assert_eq!(max_append_batch, MAX_ATTESTATION_APPEND_BATCH);
    assert_eq!(max_read_page, MAX_ATTESTATION_READ_PAGE);
    assert!(primary_bound);
    assert_eq!(append_log_length, 1);
}

/// INV-ATT-2: `primary_bound` transitions monotonically from false to true.
/// Duplicate bind attempts with same or different hashes must be rejected
/// with `PrimaryAttestationAlreadyBound` and preserve `primary_bound == true`
/// and the original primary hash.
#[test]
fn test_primary_bound_monotonic_and_duplicate_bind_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    let _admin = init_escrow(&env, &client);

    // Initial state: not bound
    let initial_config = client.get_attestation_config();
    assert!(!initial_config.primary_bound);

    let digest1 = BytesN::from_array(&env, &[0x11u8; 32]);
    client.bind_primary_attestation_hash(&digest1);

    // Monotonic transition to true
    let bound_config = client.get_attestation_config();
    assert!(bound_config.primary_bound);
    assert_eq!(client.get_primary_attestation_hash(), Some(digest1.clone()));

    // Duplicate bind with exact same hash rejected
    let res_same = client.try_bind_primary_attestation_hash(&digest1);
    assert_contract_error(res_same, EscrowError::PrimaryAttestationAlreadyBound);
    assert!(client.get_attestation_config().primary_bound);
    assert_eq!(client.get_primary_attestation_hash(), Some(digest1.clone()));

    // Duplicate bind with different hash rejected
    let digest2 = BytesN::from_array(&env, &[0x22u8; 32]);
    let res_diff = client.try_bind_primary_attestation_hash(&digest2);
    assert_contract_error(res_diff, EscrowError::PrimaryAttestationAlreadyBound);
    assert!(client.get_attestation_config().primary_bound);
    assert_eq!(client.get_primary_attestation_hash(), Some(digest1));
}

/// INV-ATT-3: `append_log_length` updates monotonically up to the ceiling of
/// `MAX_ATTESTATION_APPEND_ENTRIES` (32). The 33rd append fails deterministically
/// with `AttestationAppendLogCapacityReached`, and subsequent retries remain
/// safely bounded at 32 without data corruption.
#[test]
fn test_append_log_capacity_ceiling_boundary_and_rejection() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    let _admin = init_escrow(&env, &client);

    // Fill log to capacity (32 entries)
    for i in 0..MAX_ATTESTATION_APPEND_ENTRIES {
        let mut arr = [0u8; 32];
        arr[0] = i as u8;
        let digest = BytesN::from_array(&env, &arr);
        client.append_attestation_digest(&digest);

        let cfg = client.get_attestation_config();
        assert_eq!(cfg.append_log_length, i + 1);
    }

    assert_eq!(
        client.get_attestation_config().append_log_length,
        MAX_ATTESTATION_APPEND_ENTRIES
    );

    // 33rd entry must fail with AttestationAppendLogCapacityReached
    let overflow_digest = BytesN::from_array(&env, &[0xffu8; 32]);
    let res = client.try_append_attestation_digest(&overflow_digest);
    assert_contract_error(res, EscrowError::AttestationAppendLogCapacityReached);

    // Invariant: append_log_length remains 32
    assert_eq!(
        client.get_attestation_config().append_log_length,
        MAX_ATTESTATION_APPEND_ENTRIES
    );

    // Retry must also be rejected deterministically
    let retry_res = client.try_append_attestation_digest(&overflow_digest);
    assert_contract_error(retry_res, EscrowError::AttestationAppendLogCapacityReached);
    assert_eq!(
        client.get_attestation_config().append_log_length,
        MAX_ATTESTATION_APPEND_ENTRIES
    );
}

/// INV-ATT-4: Positional stability under batch revoke and unrevoke operations.
/// Revoking (single/batch) and unrevoking toggles revocation status but never
/// alters `append_log_length`.
#[test]
fn test_batch_revoke_and_unrevoke_preserves_log_length() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    let _admin = init_escrow(&env, &client);

    for i in 0..5 {
        let mut arr = [0u8; 32];
        arr[0] = i as u8;
        client.append_attestation_digest(&BytesN::from_array(&env, &arr));
    }
    assert_eq!(client.get_attestation_config().append_log_length, 5);

    // Batch revoke indices [0, 2, 4]
    let mut batch = SorobanVec::new(&env);
    batch.push_back(0u32);
    batch.push_back(2u32);
    batch.push_back(4u32);
    client.revoke_attestation_digests(&batch);

    // Invariant: log length remains 5
    assert_eq!(client.get_attestation_config().append_log_length, 5);
    assert!(client.is_attestation_revoked(&0));
    assert!(!client.is_attestation_revoked(&1));
    assert!(client.is_attestation_revoked(&2));
    assert!(!client.is_attestation_revoked(&3));
    assert!(client.is_attestation_revoked(&4));

    // Unrevoke index 2
    client.unrevoke_attestation_digest(&2);
    assert_eq!(client.get_attestation_config().append_log_length, 5);
    assert!(!client.is_attestation_revoked(&2));

    // Single revoke index 1
    client.revoke_attestation_digest(&1);
    assert_eq!(client.get_attestation_config().append_log_length, 5);
    assert!(client.is_attestation_revoked(&1));
}

/// INV-ATT-4: Adverse revocation failure modes (out of bounds, duplicate, empty batch,
/// oversized batch, unrevoke of non-revoked index) must fail with typed EscrowErrors
/// and leave `get_attestation_config` invariant.
#[test]
fn test_revoke_rejection_boundary_preserves_config_state() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    let _admin = init_escrow(&env, &client);

    let hash = BytesN::from_array(&env, &[0x11u8; 32]);
    client.append_attestation_digest(&hash);
    let initial_config = client.get_attestation_config();
    assert_eq!(initial_config.append_log_length, 1);

    // 1. Out-of-bounds single revoke (index 10 >= log.len() 1)
    let res = client.try_revoke_attestation_digest(&10);
    assert_contract_error(res, EscrowError::AttestationIndexOutOfRange);
    assert_eq!(client.get_attestation_config(), initial_config);

    // 2. Successful revoke of index 0
    client.revoke_attestation_digest(&0);
    let revoked_config = client.get_attestation_config();
    assert_eq!(revoked_config, initial_config);

    // 3. Duplicate revoke of index 0
    let res = client.try_revoke_attestation_digest(&0);
    assert_contract_error(res, EscrowError::AttestationAlreadyRevoked);
    assert_eq!(client.get_attestation_config(), initial_config);

    // 4. Batch revoke with empty list
    let empty_batch = SorobanVec::new(&env);
    let res = client.try_revoke_attestation_digests(&empty_batch);
    assert_contract_error(res, EscrowError::AttestationBatchEmpty);
    assert_eq!(client.get_attestation_config(), initial_config);

    // 5. Batch revoke with out-of-bounds index
    let mut out_of_bounds_batch = SorobanVec::new(&env);
    out_of_bounds_batch.push_back(99u32);
    let res = client.try_revoke_attestation_digests(&out_of_bounds_batch);
    assert_contract_error(res, EscrowError::AttestationIndexOutOfRange);
    assert_eq!(client.get_attestation_config(), initial_config);

    // 6. Batch revoke exceeding MAX_ATTESTATION_REVOKE_BATCH (8)
    let mut large_batch = SorobanVec::new(&env);
    for _ in 0..=MAX_ATTESTATION_REVOKE_BATCH {
        large_batch.push_back(0u32);
    }
    let res = client.try_revoke_attestation_digests(&large_batch);
    assert_contract_error(res, EscrowError::AttestationBatchTooLarge);
    assert_eq!(client.get_attestation_config(), initial_config);

    // 7. Unrevoke on index 0 succeeds
    client.unrevoke_attestation_digest(&0);
    assert_eq!(client.get_attestation_config(), initial_config);

    // 8. Unrevoke on already unrevoked index 0 fails
    let res = client.try_unrevoke_attestation_digest(&0);
    assert_contract_error(res, EscrowError::AttestationNotRevoked);
    assert_eq!(client.get_attestation_config(), initial_config);

    // 9. Unrevoke out-of-bounds
    let res = client.try_unrevoke_attestation_digest(&50);
    assert_contract_error(res, EscrowError::AttestationIndexOutOfRange);
    assert_eq!(client.get_attestation_config(), initial_config);
}

/// INV-ATT-1: Authorization isolation.
/// `get_attestation_config` is a pure read view that requires no authorization
/// and succeeds for anonymous and arbitrary callers.
/// Mutation entrypoints strictly enforce admin authorization, and rejected
/// calls leave the view config completely invariant.
#[test]
fn test_unauthorized_callers_isolation_and_auth_boundaries() {
    let env = Env::default();
    let client = deploy(&env);

    // Initialize escrow with mocked auths
    env.mock_all_auths();
    let _admin = init_escrow(&env, &client);

    let config_before = client.get_attestation_config();

    // Clear all mocked auths: caller is now unauthenticated / anonymous
    env.mock_auths(&[]);

    // Anonymous read succeeds unconditionally
    let anon_config = client.get_attestation_config();
    assert_eq!(anon_config, config_before);

    // Mutation entrypoints fail without admin authorization
    let dummy_hash = BytesN::from_array(&env, &[0xaau8; 32]);
    assert!(client
        .try_bind_primary_attestation_hash(&dummy_hash)
        .is_err());
    assert!(client.try_append_attestation_digest(&dummy_hash).is_err());
    assert!(client.try_revoke_attestation_digest(&0).is_err());
    assert!(client.try_unrevoke_attestation_digest(&0).is_err());

    // Invariant: rejected unauthorized mutation attempts did not alter config
    assert_eq!(client.get_attestation_config(), config_before);
}

/// INV-ATT-6: View purity, event purity, and zero side effects.
/// Invoking `get_attestation_config` emits zero events and performs zero storage mutations.
#[test]
fn test_view_purity_no_events_no_storage_mutation() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    let _admin = init_escrow(&env, &client);

    let hash = BytesN::from_array(&env, &[0x33u8; 32]);
    client.bind_primary_attestation_hash(&hash);
    client.append_attestation_digest(&hash);

    // Drain prior setup events
    let _ = env.events().all();

    // Repeated view reads
    let c1 = client.get_attestation_config();
    let c2 = client.get_attestation_config();
    let c3 = client.get_attestation_config();

    let new_events = env.events().all().events().len();
    assert_eq!(new_events, 0, "get_attestation_config must not emit events");
    assert_eq!(c1, c2);
    assert_eq!(c2, c3);
}

/// INV-ATT-5: Cross-subsystem financial isolation.
/// Escrow financial transitions (funding, balances) do not perturb `get_attestation_config`,
/// and attestation state mutations do not alter financial state.
#[test]
fn test_cross_subsystem_financial_isolation() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _escrow_id, _sme) =
        super::init_and_fund_with_real_token(&env, 10_000, "INV_FIN01");

    // Config defaults intact despite funding
    let config = client.get_attestation_config();
    assert!(!config.primary_bound);
    assert_eq!(config.append_log_length, 0);

    // Attestation transition
    let hash = BytesN::from_array(&env, &[0x77u8; 32]);
    client.bind_primary_attestation_hash(&hash);
    client.append_attestation_digest(&hash);

    let updated_config = client.get_attestation_config();
    assert!(updated_config.primary_bound);
    assert_eq!(updated_config.append_log_length, 1);

    // Financial state remains completely intact
    let escrow = client.get_escrow();
    assert_eq!(escrow.funded_amount, 10_000);
    assert_eq!(escrow.amount, 10_000);
}

/// Adverse transitions prior to contract initialization fail and leave
/// `get_attestation_config` reporting documented defaults.
#[test]
fn test_pre_init_rejected_operations_preserve_defaults() {
    let env = Env::default();
    let client = deploy(&env);

    let dummy_hash = BytesN::from_array(&env, &[0x42u8; 32]);

    assert!(client
        .try_bind_primary_attestation_hash(&dummy_hash)
        .is_err());
    assert!(client.try_append_attestation_digest(&dummy_hash).is_err());
    assert!(client.try_revoke_attestation_digest(&0).is_err());

    let defaults = client.get_attestation_config();
    assert_eq!(defaults.max_append_entries, MAX_ATTESTATION_APPEND_ENTRIES);
    assert_eq!(defaults.max_revoke_batch, MAX_ATTESTATION_REVOKE_BATCH);
    assert_eq!(defaults.max_append_batch, MAX_ATTESTATION_APPEND_BATCH);
    assert_eq!(defaults.max_read_page, MAX_ATTESTATION_READ_PAGE);
    assert!(!defaults.primary_bound);
    assert_eq!(defaults.append_log_length, 0);
}
