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
        &None::<u32>,
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

// ─────────────────────────────────────────────────────────────────────────────
// #1314 — Harden concurrent execution around escrow
//
// These tests exercise *interleaved* entrypoints: the same escrow instance
// observed across append / revoke / bind sequences, repeated view calls during
// mutation, and racing attempts to mutate the same state. Because Soroban
// executes one transaction at a time per ledger, "concurrency" here means
// *interleaving* — a sequence where a stale read or double-write would produce
// an inconsistent result. The invariants below must hold under any interleaving
// allowed by the entrypoint set.
//
// Invariants covered (see docs/attestation-invariants.md):
//   INV-ATT-2  primary hash is write-once
//   INV-ATT-3  append log bounded at MAX_ATTESTATION_APPEND_ENTRIES
//   INV-ATT-6  revoke is single-shot per index
//   INV-ATT-8  unrevoke requires currently-revoked
//   INV-ATT-9  view reads never mutate financial or attestation state
// ─────────────────────────────────────────────────────────────────────────────

use super::super::EscrowError;
use super::assert_contract_error;
use soroban_sdk::Vec as SorobanVec;

// ── racing bind attempts ────────────────────────────────────────────────────

/// Two sequential `bind_primary_attestation_hash` calls with *different*
/// digests must not silently overwrite — the second must fail (INV-ATT-2),
/// and the view must still report the first digest.
#[test]
fn test_racing_bind_second_call_rejected_and_view_unchanged() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let _admin = init_escrow(&env, &client);

    let first = BytesN::from_array(&env, &[0xA1u8; 32]);
    let second = BytesN::from_array(&env, &[0xB2u8; 32]);

    client.bind_primary_attestation_hash(&first);

    let result = client.try_bind_primary_attestation_hash(&second);
    assert_contract_error(result, EscrowError::PrimaryAttestationAlreadyBound);

    // View must still reflect the *first* write.
    assert_eq!(client.get_primary_attestation_hash(), Some(first));
    assert!(client.get_attestation_config().primary_bound);
}

/// Even a *duplicate* bind of the same digest must be rejected — the
/// write-once rule is about existence, not equality.
#[test]
fn test_duplicate_bind_same_digest_still_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let _admin = init_escrow(&env, &client);

    let digest = BytesN::from_array(&env, &[0xC3u8; 32]);
    client.bind_primary_attestation_hash(&digest);

    let result = client.try_bind_primary_attestation_hash(&digest);
    assert_contract_error(result, EscrowError::PrimaryAttestationAlreadyBound);

    assert_eq!(client.get_primary_attestation_hash(), Some(digest));
}

// ── racing revoke attempts on the same index ────────────────────────────────

/// Revoking the same index twice in a row must fail the second time
/// (INV-ATT-6), and the view must not flip state.
#[test]
fn test_racing_revoke_same_index_second_call_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let _admin = init_escrow(&env, &client);

    let digest = BytesN::from_array(&env, &[0xD4u8; 32]);
    client.append_attestation_digest(&digest);

    client.revoke_attestation_digest(&0);
    assert!(client.is_attestation_revoked(&0));

    let result = client.try_revoke_attestation_digest(&0);
    assert_contract_error(result, EscrowError::AttestationAlreadyRevoked);

    // Still revoked, still one entry.
    assert!(client.is_attestation_revoked(&0));
    assert_eq!(client.get_attestation_config().append_log_length, 1);
}

/// Batch revoke with a duplicate index must fail atomically and leave
/// *no* index revoked (INV-ATT-7 atomicity).
#[test]
fn test_racing_batch_revoke_with_duplicate_rolls_back() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let _admin = init_escrow(&env, &client);

    for b in [0x01u8, 0x02u8, 0x03u8] {
        client.append_attestation_digest(&BytesN::from_array(&env, &[b; 32]));
    }

    // Duplicate index 1 twice in one batch.
    let indices = SorobanVec::from_array(&env, [1u32, 1u32]);
    let result = client.try_revoke_attestation_digests(&indices);
    assert_contract_error(result, EscrowError::AttestationAlreadyRevoked);

    // Nothing was revoked — atomic rollback.
    assert!(!client.is_attestation_revoked(&0));
    assert!(!client.is_attestation_revoked(&1));
    assert!(!client.is_attestation_revoked(&2));
}

/// A batch revoke where one index is out of range must roll back the whole
/// batch, leaving earlier indices un-revoked.
#[test]
fn test_racing_batch_revoke_with_out_of_range_rolls_back() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let _admin = init_escrow(&env, &client);

    for b in [0x11u8, 0x12u8] {
        client.append_attestation_digest(&BytesN::from_array(&env, &[b; 32]));
    }

    // 0 is valid, 99 is out of range.
    let indices = SorobanVec::from_array(&env, [0u32, 99u32]);
    let result = client.try_revoke_attestation_digests(&indices);
    assert_contract_error(result, EscrowError::AttestationIndexOutOfRange);

    assert!(!client.is_attestation_revoked(&0));
    assert!(!client.is_attestation_revoked(&1));
}

// ── unrevoke / re-revoke flip ───────────────────────────────────────────────

/// Unrevoking requires the index to be currently revoked; unrevoking twice
/// fails the second time (INV-ATT-8).
#[test]
fn test_racing_unrevoke_twice_second_call_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let _admin = init_escrow(&env, &client);

    client.append_attestation_digest(&BytesN::from_array(&env, &[0xE5u8; 32]));
    client.revoke_attestation_digest(&0);
    client.unrevoke_attestation_digest(&0);
    assert!(!client.is_attestation_revoked(&0));

    let result = client.try_unrevoke_attestation_digest(&0);
    assert_contract_error(result, EscrowError::AttestationNotRevoked);
}

/// Revoke → unrevoke → revoke must succeed each time with the correct state
/// at each step (state transitions are reversible but never ambiguous).
#[test]
fn test_revoke_unrevoke_revoke_cycle() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let _admin = init_escrow(&env, &client);

    client.append_attestation_digest(&BytesN::from_array(&env, &[0xF6u8; 32]));

    assert!(!client.is_attestation_revoked(&0));
    client.revoke_attestation_digest(&0);
    assert!(client.is_attestation_revoked(&0));
    client.unrevoke_attestation_digest(&0);
    assert!(!client.is_attestation_revoked(&0));
    client.revoke_attestation_digest(&0);
    assert!(client.is_attestation_revoked(&0));

    // Log length is invariant across all of the above.
    assert_eq!(client.get_attestation_config().append_log_length, 1);
}

// ── append-log boundary at exactly MAX_ATTESTATION_APPEND_ENTRIES ───────────

/// Filling the log to exactly `MAX_ATTESTATION_APPEND_ENTRIES` must succeed;
/// the next append must fail with capacity-reached (INV-ATT-3), and the view
/// must report exactly `MAX_ATTESTATION_APPEND_ENTRIES` throughout.
#[test]
fn test_append_log_boundary_exactly_at_capacity() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let _admin = init_escrow(&env, &client);

    for i in 0..MAX_ATTESTATION_APPEND_ENTRIES {
        let digest = BytesN::from_array(&env, &[i as u8; 32]);
        client.append_attestation_digest(&digest);
        assert_eq!(
            client.get_attestation_config().append_log_length,
            i + 1,
            "append_log_length must increment by exactly 1 per append"
        );
    }

    // Log is now full.
    assert_eq!(
        client.get_attestation_config().append_log_length,
        MAX_ATTESTATION_APPEND_ENTRIES
    );

    // Next append must fail.
    let overflow = BytesN::from_array(&env, &[0xFFu8; 32]);
    let result = client.try_append_attestation_digest(&overflow);
    assert_contract_error(result, EscrowError::AttestationAppendLogCapacityReached);

    // Length unchanged after the rejected append.
    assert_eq!(
        client.get_attestation_config().append_log_length,
        MAX_ATTESTATION_APPEND_ENTRIES
    );
}

/// Duplicate digests are allowed — the append log is an ordered audit trail,
/// not a set. Two identical digests produce two distinct entries.
#[test]
fn test_duplicate_digests_are_appended_twice() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let _admin = init_escrow(&env, &client);

    let digest = BytesN::from_array(&env, &[0x77u8; 32]);
    client.append_attestation_digest(&digest);
    client.append_attestation_digest(&digest);

    assert_eq!(client.get_attestation_config().append_log_length, 2);
    let log = client.get_attestation_append_log();
    assert_eq!(log.len(), 2);
    assert_eq!(log.get(0).unwrap(), digest);
    assert_eq!(log.get(1).unwrap(), digest);
}

// ── interleaving append and view ────────────────────────────────────────────

/// Repeatedly read the config between every append — each read must reflect
/// the log length at that exact point (no stale snapshot).
#[test]
fn test_config_view_reflects_each_interleaved_append() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let _admin = init_escrow(&env, &client);

    for i in 0..5u8 {
        // Observe before append.
        let before = client.get_attestation_config().append_log_length;
        assert_eq!(before, i as u32);

        client.append_attestation_digest(&BytesN::from_array(&env, &[i; 32]));

        // Observe after append.
        let after = client.get_attestation_config().append_log_length;
        assert_eq!(after, (i as u32) + 1);
    }
}

/// `primary_bound` and `append_log_length` are independent — binding does not
/// affect log length and appending does not affect `primary_bound`.
#[test]
fn test_primary_bound_and_log_length_are_independent() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let _admin = init_escrow(&env, &client);

    let c0 = client.get_attestation_config();
    assert!(!c0.primary_bound);
    assert_eq!(c0.append_log_length, 0);

    client.bind_primary_attestation_hash(&BytesN::from_array(&env, &[1u8; 32]));
    let c1 = client.get_attestation_config();
    assert!(c1.primary_bound);
    assert_eq!(c1.append_log_length, 0);

    client.append_attestation_digest(&BytesN::from_array(&env, &[2u8; 32]));
    let c2 = client.get_attestation_config();
    assert!(c2.primary_bound);
    assert_eq!(c2.append_log_length, 1);
}

// ── view snapshot isolation ─────────────────────────────────────────────────

/// A previously-read config value must not change when the underlying state
/// is mutated afterwards. Soroban returns value types, so this is a regression
/// guard against accidental reference semantics.
#[test]
fn test_config_snapshot_is_isolated_from_later_mutations() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let _admin = init_escrow(&env, &client);

    client.append_attestation_digest(&BytesN::from_array(&env, &[3u8; 32]));
    let snapshot = client.get_attestation_config();
    let snapshot_len = snapshot.append_log_length;
    let snapshot_bound = snapshot.primary_bound;

    // Mutate both fields after the snapshot.
    client.bind_primary_attestation_hash(&BytesN::from_array(&env, &[4u8; 32]));
    client.append_attestation_digest(&BytesN::from_array(&env, &[5u8; 32]));

    // Snapshot must be untouched.
    assert_eq!(snapshot.append_log_length, snapshot_len);
    assert_eq!(snapshot.primary_bound, snapshot_bound);

    // Fresh read reflects the new state.
    let fresh = client.get_attestation_config();
    assert!(fresh.primary_bound);
    assert_eq!(fresh.append_log_length, 2);
}

// ── authorization boundary under repeated attempts ──────────────────────────

/// A non-admin caller must fail every mutating attestation entrypoint, even
/// after a successful admin bind. Uses `try_` variants so the auth failure is
/// surfaced as a result rather than a panic.
#[test]
fn test_non_admin_cannot_bind_append_or_revoke() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let _admin = init_escrow(&env, &client);

    // Disable blanket auth mocking so require_auth for the attacker fails.
    env.set_auths(&[]);

    let attacker = Address::generate(&env);
    let digest = BytesN::from_array(&env, &[0x88u8; 32]);

    // Note: with mock_all_auths disabled, the host rejects before our error
    // type is reachable; use try_ and only require that it is an Err.
    assert!(client.try_bind_primary_attestation_hash(&digest).is_err());
    assert!(client.try_append_attestation_digest(&digest).is_err());
    // Index 0 does not exist yet — range check may fire first; either way Err.
    assert!(client.try_revoke_attestation_digest(&0).is_err());

    // And no state was mutated.
    let cfg = client.get_attestation_config();
    assert!(!cfg.primary_bound);
    assert_eq!(cfg.append_log_length, 0);
}
