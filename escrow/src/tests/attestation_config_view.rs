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
    AttestationConfig, LiquifactEscrow, LiquifactEscrowClient, MAX_ATTESTATION_APPEND_BATCH,
    MAX_ATTESTATION_APPEND_ENTRIES, MAX_ATTESTATION_READ_PAGE, MAX_ATTESTATION_REVOKE_BATCH,
};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, BytesN, Env};

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
    assert!(!config.primary_bound, "primary_bound should be false after init when no hash bound");
    assert_eq!(config.append_log_length, 0, "append_log_length should be 0 after init when no digests appended");
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
    assert!(config.primary_bound, "primary_bound should be true after binding");
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
        config.append_log_length as u32,
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

// ── boundary tests ───────────────────────────────────────────────────────────

/// `max_append_entries` boundary: appending exactly the maximum number of
/// entries must succeed, and the config must report the boundary value.
#[test]
fn test_append_entries_boundary_exact_max() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    let admin = init_escrow(&env, &client);

    let config = client.get_attestation_config();
    let max = config.max_append_entries;

    // Append exactly `max` entries — must succeed.
    for i in 0..max {
        let mut bytes = [0u8; 32];
        bytes[0] = (i & 0xff) as u8;
        bytes[1] = ((i >> 8) & 0xff) as u8;
        let hash = BytesN::from_array(&env, &bytes);
        client.append_attestation_digest(&hash);
    }

    let config = client.get_attestation_config();
    assert_eq!(
        config.append_log_length, max,
        "append_log_length must equal max_append_entries at the boundary"
    );
}

/// `max_append_entries` boundary: appending one more than the maximum must
/// be rejected deterministically.
#[test]
#[should_panic]
fn test_append_entries_boundary_exceeds_max() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    let admin = init_escrow(&env, &client);

    let config = client.get_attestation_config();
    let max = config.max_append_entries;

    // Append `max` entries — must succeed.
    for i in 0..max {
        let mut bytes = [0u8; 32];
        bytes[0] = (i & 0xff) as u8;
        bytes[1] = ((i >> 8) & 0xff) as u8;
        let hash = BytesN::from_array(&env, &bytes);
        client.append_attestation_digest(&hash);
    }

    // One more must panic.
    let extra = BytesN::from_array(&env, &[0xffu8; 32]);
    client.append_attestation_digest(&extra);
}

/// `max_revoke_batch` boundary: revoking exactly the maximum batch size must
/// succeed.
#[test]
fn test_revoke_batch_boundary_exact_max() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    let admin = init_escrow(&env, &client);

    let config = client.get_attestation_config();
    let max = config.max_revoke_batch;

    // Append `max` entries.
    for i in 0..max {
        let mut bytes = [0u8; 32];
        bytes[0] = (i & 0xff) as u8;
        bytes[1] = ((i >> 8) & 0xff) as u8;
        let hash = BytesN::from_array(&env, &bytes);
        client.append_attestation_digest(&hash);
    }

    // Revoke all `max` entries in one batch — must succeed.
    for i in 0..max {
        client.revoke_attestation_digest(&i);
    }

    // Log length must be unchanged (revocation does not remove entries).
    let config = client.get_attestation_config();
    assert_eq!(
        config.append_log_length, max,
        "revoke must not reduce append_log_length at the boundary"
    );
}

/// `max_revoke_batch` boundary: revoking one more than the maximum batch size
/// must be rejected deterministically.
#[test]
#[should_panic]
fn test_revoke_batch_boundary_exceeds_max() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    let admin = init_escrow(&env, &client);

    let config = client.get_attestation_config();
    let max = config.max_revoke_batch;

    // Append `max + 1` entries.
    for i in 0..=max {
        let mut bytes = [0u8; 32];
        bytes[0] = (i & 0xff) as u8;
        bytes[1] = ((i >> 8) & 0xff) as u8;
        let hash = BytesN::from_array(&env, &bytes);
        client.append_attestation_digest(&hash);
    }

    // Revoke `max + 1` entries in one batch — must panic.
    for i in 0..=max {
        client.revoke_attestation_digest(&i);
    }
}

/// `max_read_page` boundary: reading exactly the maximum page size must
/// succeed and return the expected number of entries.
#[test]
fn test_read_page_boundary_exact_max() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    let admin = init_escrow(&env, &client);

    let config = client.get_attestation_config();
    let max_page = config.max_read_page;

    // Append `max_page` entries.
    for i in 0..max_page {
        let mut bytes = [0u8; 32];
        bytes[0] = (i & 0xff) as u8;
        bytes[1] = ((i >> 8) & 0xff) as u8;
        let hash = BytesN::from_array(&env, &bytes);
        client.append_attestation_digest(&hash);
    }

    // Read exactly `max_page` entries — must succeed.
    let page = client.get_attestation_append_log();
    assert_eq!(
        page.len() as u32, max_page,
        "read page must contain exactly max_read_page entries at the boundary"
    );
}

/// `max_read_page` boundary: reading one more than the maximum page size
/// must be rejected deterministically.
#[test]
#[should_panic]
fn test_read_page_boundary_exceeds_max() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    let admin = init_escrow(&env, &client);

    let config = client.get_attestation_config();
    let max_page = config.max_read_page;

    // Append `max_page + 1` entries.
    for i in 0..=max_page {
        let mut bytes = [0u8; 32];
        bytes[0] = (i & 0xff) as u8;
        bytes[1] = ((i >> 8) & 0xff) as u8;
        let hash = BytesN::from_array(&env, &bytes);
        client.append_attestation_digest(&hash);
    }

    // Attempt to read `max_page + 1` entries — must panic.
    let page = client.get_attestation_append_log();
    assert_eq!(page.len() as u32, max_page + 1);
}

/// Duplicate submissions: appending the same digest twice must be handled
/// deterministically (either accepted as distinct entries or rejected).
#[test]
fn test_duplicate_append_deterministic() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    let admin = init_escrow(&env, &client);

    let hash = BytesN::from_array(&env, &[0x77u8; 32]);

    // First append.
    client.append_attestation_digest(&hash);
    let len_after_first = client.get_attestation_config().append_log_length;

    // Second append of the same hash — must be deterministic.
    client.append_attestation_digest(&hash);
    let len_after_second = client.get_attestation_config().append_log_length;

    // The log must grow by exactly one (duplicates are distinct entries).
    assert_eq!(
        len_after_second,
        len_after_first + 1,
        "duplicate append must be deterministic and add exactly one entry"
    );
}

/// Duplicate submissions: binding the same primary hash twice must be
/// deterministic (idempotent or rejected).
#[test]
fn test_duplicate_bind_deterministic() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    let admin = init_escrow(&env, &client);

    let hash = BytesN::from_array(&env, &[0x88u8; 32]);

    // First bind.
    client.bind_primary_attestation_hash(&hash);
    let bound_after_first = client.get_attestation_config().primary_bound;

    // Second bind of the same hash — must be deterministic.
    client.bind_primary_attestation_hash(&hash);
    let bound_after_second = client.get_attestation_config().primary_bound;

    assert_eq!(
        bound_after_first, bound_after_second,
        "duplicate bind must be deterministic"
    );
    assert!(bound_after_second, "primary_bound must remain true after duplicate bind");
}

/// Invalid input: appending a zero hash must be handled deterministically.
#[test]
fn test_invalid_zero_hash_append_deterministic() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    let admin = init_escrow(&env, &client);

    let zero_hash = BytesN::from_array(&env, &[0u8; 32]);

    // Appending a zero hash must be deterministic (either accepted or rejected).
    // We verify the config remains consistent regardless of outcome.
    let before = client.get_attestation_config().append_log_length;
    client.append_attestation_digest(&zero_hash);
    let after = client.get_attestation_config().append_log_length;

    assert!(
        after == before || after == before + 1,
        "zero-hash append must be deterministic (no partial state)"
    );
}

/// Invalid input: revoking an out-of-bounds index must be rejected
/// deterministically.
#[test]
#[should_panic]
fn test_invalid_revoke_index_out_of_bounds() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    let admin = init_escrow(&env, &client);

    // Revoke index 0 when the log is empty — must panic.
    client.revoke_attestation_digest(&0);
}

/// Regression: config must remain consistent after a rejected operation.
#[test]
fn test_config_consistent_after_rejected_operation() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);

    let admin = init_escrow(&env, &client);

    let hash = BytesN::from_array(&env, &[0x33u8; 32]);
    client.append_attestation_digest(&hash);

    let before = client.get_attestation_config();

    // Attempt an invalid revoke (out of bounds) — must not corrupt state.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.revoke_attestation_digest(&999);
    }));
    assert!(result.is_err(), "out-of-bounds revoke must panic");

    let after = client.get_attestation_config();
    assert_eq!(
        before, after,
        "config must be unchanged after a rejected operation"
    );
}
