//! Concurrency and interleaving tests for the attestation entrypoints on
//! [`LiquifactEscrow`].
//!
//! Issue: #1314 — Harden concurrent execution around escrow.
//!
//! # Why this file tests `get_escrow_summary` instead of `get_attestation_config`
//!
//! An earlier version of this file (never wired into `tests/mod.rs`) called
//! `client.get_attestation_config()` and destructured an `AttestationConfig`
//! struct. Neither the function nor the struct exists on `LiquifactEscrow` —
//! there is no `pub fn get_attestation_config` in `escrow/src/lib.rs`, no
//! `AttestationConfig` type, and no doc describing either. The two live-state
//! fields that tests need (`primary_bound`, `append_log_length`) are exposed
//! on [`EscrowSummary`] as `has_primary_attestation` and
//! `attestation_log_length` respectively, returned by
//! [`LiquifactEscrow::get_escrow_summary`].
//!
//! This file therefore reads state through the real, exported view and
//! hardens the actual attestation entrypoints against interleaved calls.
//!
//! # Invariants covered
//!
//! Per [`docs/attestation-invariants.md`]:
//!
//! - **INV-ATT-1**: admin authorization on every state-mutating entrypoint.
//! - **INV-ATT-2**: primary hash is write-once.
//! - **INV-ATT-3**: append log is bounded at `MAX_ATTESTATION_APPEND_ENTRIES`.
//! - **INV-ATT-4**: append log is positional and append-only.
//! - **INV-ATT-6**: revoke is single-shot per index.
//! - **INV-ATT-7**: batch revoke is atomic — any failure rolls back the batch.
//! - **INV-ATT-8**: unrevoke requires the index to be currently revoked.
//! - **INV-ATT-9**: views never mutate state.

use super::super::{
    EscrowError, EscrowSummary, LiquifactEscrow, LiquifactEscrowClient,
    MAX_ATTESTATION_APPEND_ENTRIES, MAX_ATTESTATION_REVOKE_BATCH,
};
use super::assert_contract_error;
use soroban_sdk::testutils::Address as _;
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
        &None::<u32>,
    );
    admin
}

/// Read the two attestation-related fields out of the real view.
///
/// Returns `(primary_bound, append_log_length)`.
fn attestation_view(client: &LiquifactEscrowClient) -> (bool, u32) {
    let summary: EscrowSummary = client.get_escrow_summary();
    (
        summary.has_primary_attestation,
        summary.attestation_log_length,
    )
}

// ── baseline view behavior ──────────────────────────────────────────────────

/// Before any attestation operation, both live fields are at defaults.
#[test]
fn test_view_defaults_after_init() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let _admin = init_escrow(&env, &client);

    let (primary_bound, log_len) = attestation_view(&client);
    assert!(!primary_bound, "primary_bound should be false after init");
    assert_eq!(log_len, 0, "append_log_length should be 0 after init");
}

/// The view reports `primary_bound = true` after a successful bind.
#[test]
fn test_view_primary_bound_true_after_bind() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let _admin = init_escrow(&env, &client);

    let hash = BytesN::from_array(&env, &[0xABu8; 32]);
    client.bind_primary_attestation_hash(&hash);

    let (primary_bound, log_len) = attestation_view(&client);
    assert!(primary_bound);
    assert_eq!(log_len, 0);
}

/// The view's `append_log_length` tracks each successful append by exactly 1.
#[test]
fn test_view_append_log_length_increments_by_one() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let _admin = init_escrow(&env, &client);

    for i in 0..3u8 {
        let digest = BytesN::from_array(&env, &[i; 32]);
        client.append_attestation_digest(&digest);
        let (_, len) = attestation_view(&client);
        assert_eq!(len, (i as u32) + 1);
    }
}

/// Revocation does not change `append_log_length` — the log is append-only,
/// only the revocation marker changes (INV-ATT-4).
#[test]
fn test_view_log_length_unaffected_by_revoke() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let _admin = init_escrow(&env, &client);

    client.append_attestation_digest(&BytesN::from_array(&env, &[0x42u8; 32]));
    let (_, before) = attestation_view(&client);
    assert_eq!(before, 1);

    client.revoke_attestation_digest(&0);
    let (_, after) = attestation_view(&client);
    assert_eq!(after, 1, "revoke must not reduce append_log_length");
}

/// Two consecutive reads return identical values (INV-ATT-9: pure read).
#[test]
fn test_view_is_idempotent() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let _admin = init_escrow(&env, &client);

    client.bind_primary_attestation_hash(&BytesN::from_array(&env, &[0x55u8; 32]));

    let a = attestation_view(&client);
    let b = attestation_view(&client);
    assert_eq!(a, b);
}

// ── racing bind attempts (INV-ATT-2) ────────────────────────────────────────

/// Second bind with a different digest is rejected; view still reports first.
#[test]
fn test_racing_bind_second_call_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let _admin = init_escrow(&env, &client);

    let first = BytesN::from_array(&env, &[0xA1u8; 32]);
    let second = BytesN::from_array(&env, &[0xB2u8; 32]);

    client.bind_primary_attestation_hash(&first);
    let result = client.try_bind_primary_attestation_hash(&second);
    assert_contract_error(result, EscrowError::PrimaryAttestationAlreadyBound);

    assert_eq!(client.get_primary_attestation_hash(), Some(first));
    let (bound, _) = attestation_view(&client);
    assert!(bound);
}

/// Even a duplicate bind of the same digest is rejected (write-once by
/// existence, not by value).
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
}

// ── racing revoke attempts (INV-ATT-6) ──────────────────────────────────────

/// Second revoke of the same index is rejected; state stays revoked.
#[test]
fn test_racing_revoke_same_index_second_call_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let _admin = init_escrow(&env, &client);

    client.append_attestation_digest(&BytesN::from_array(&env, &[0xD4u8; 32]));
    client.revoke_attestation_digest(&0);
    assert!(client.is_attestation_revoked(&0));

    let result = client.try_revoke_attestation_digest(&0);
    assert_contract_error(result, EscrowError::AttestationAlreadyRevoked);

    assert!(client.is_attestation_revoked(&0));
    let (_, len) = attestation_view(&client);
    assert_eq!(len, 1);
}

// ── batch revoke atomicity (INV-ATT-7) ──────────────────────────────────────

/// Duplicate index in a batch → atomic rollback; nothing is revoked.
#[test]
fn test_batch_revoke_with_duplicate_rolls_back() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let _admin = init_escrow(&env, &client);

    for b in [0x01u8, 0x02u8, 0x03u8] {
        client.append_attestation_digest(&BytesN::from_array(&env, &[b; 32]));
    }

    let indices = SorobanVec::from_array(&env, [1u32, 1u32]);
    let result = client.try_revoke_attestation_digests(&indices);
    assert_contract_error(result, EscrowError::AttestationAlreadyRevoked);

    for i in 0..3u32 {
        assert!(!client.is_attestation_revoked(&i));
    }
}

/// Out-of-range index in a batch → atomic rollback.
#[test]
fn test_batch_revoke_with_out_of_range_rolls_back() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let _admin = init_escrow(&env, &client);

    for b in [0x11u8, 0x12u8] {
        client.append_attestation_digest(&BytesN::from_array(&env, &[b; 32]));
    }

    let indices = SorobanVec::from_array(&env, [0u32, 99u32]);
    let result = client.try_revoke_attestation_digests(&indices);
    assert_contract_error(result, EscrowError::AttestationIndexOutOfRange);

    assert!(!client.is_attestation_revoked(&0));
    assert!(!client.is_attestation_revoked(&1));
}

/// Batch size beyond `MAX_ATTESTATION_REVOKE_BATCH` is rejected before any
/// state change.
#[test]
fn test_batch_revoke_too_large_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let _admin = init_escrow(&env, &client);

    // MAX_ATTESTATION_REVOKE_BATCH + 1 indices, all out of range on purpose
    // — batch-size check must fire before the range check.
    let n = MAX_ATTESTATION_REVOKE_BATCH + 1;
    let mut indices = SorobanVec::new(&env);
    for i in 0..n {
        indices.push_back(i);
    }
    let result = client.try_revoke_attestation_digests(&indices);
    assert_contract_error(result, EscrowError::AttestationBatchTooLarge);
}

// ── unrevoke preconditions (INV-ATT-8) ──────────────────────────────────────

/// Unrevoking an already-unrevoked index is rejected.
#[test]
fn test_unrevoke_twice_second_call_rejected() {
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

/// Full revoke → unrevoke → revoke cycle succeeds; log length never changes.
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

    let (_, len) = attestation_view(&client);
    assert_eq!(len, 1);
}

// ── append-log capacity boundary (INV-ATT-3) ────────────────────────────────

/// Fill the log to exactly `MAX_ATTESTATION_APPEND_ENTRIES`. The next append
/// fails; the view reports the boundary throughout.
#[test]
fn test_append_log_boundary_exactly_at_capacity() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let _admin = init_escrow(&env, &client);

    for i in 0..MAX_ATTESTATION_APPEND_ENTRIES {
        let digest = BytesN::from_array(&env, &[i as u8; 32]);
        client.append_attestation_digest(&digest);
        let (_, len) = attestation_view(&client);
        assert_eq!(len, i + 1);
    }

    let (_, len) = attestation_view(&client);
    assert_eq!(len, MAX_ATTESTATION_APPEND_ENTRIES);

    let overflow = BytesN::from_array(&env, &[0xFFu8; 32]);
    let result = client.try_append_attestation_digest(&overflow);
    assert_contract_error(result, EscrowError::AttestationAppendLogCapacityReached);

    let (_, len_after) = attestation_view(&client);
    assert_eq!(len_after, MAX_ATTESTATION_APPEND_ENTRIES);
}

/// Duplicate digests are allowed — the log is a trail, not a set.
#[test]
fn test_duplicate_digests_are_appended() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let _admin = init_escrow(&env, &client);

    let digest = BytesN::from_array(&env, &[0x77u8; 32]);
    client.append_attestation_digest(&digest);
    client.append_attestation_digest(&digest);

    let (_, len) = attestation_view(&client);
    assert_eq!(len, 2);

    let log = client.get_attestation_append_log();
    assert_eq!(log.len(), 2);
    assert_eq!(log.get(0).unwrap(), digest);
    assert_eq!(log.get(1).unwrap(), digest);
}

// ── interleaving append / view (INV-ATT-9) ──────────────────────────────────

/// Every read between appends reflects the log length at that exact point.
#[test]
fn test_view_reflects_each_interleaved_append() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let _admin = init_escrow(&env, &client);

    for i in 0..5u8 {
        let (_, before) = attestation_view(&client);
        assert_eq!(before, i as u32);

        client.append_attestation_digest(&BytesN::from_array(&env, &[i; 32]));

        let (_, after) = attestation_view(&client);
        assert_eq!(after, (i as u32) + 1);
    }
}

/// `primary_bound` and `append_log_length` are independent.
#[test]
fn test_primary_bound_and_log_length_are_independent() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let _admin = init_escrow(&env, &client);

    let (b0, l0) = attestation_view(&client);
    assert!(!b0);
    assert_eq!(l0, 0);

    client.bind_primary_attestation_hash(&BytesN::from_array(&env, &[1u8; 32]));
    let (b1, l1) = attestation_view(&client);
    assert!(b1);
    assert_eq!(l1, 0);

    client.append_attestation_digest(&BytesN::from_array(&env, &[2u8; 32]));
    let (b2, l2) = attestation_view(&client);
    assert!(b2);
    assert_eq!(l2, 1);
}

/// A previously-read pair of values is unaffected by later mutations.
#[test]
fn test_view_snapshot_isolated_from_later_mutations() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let _admin = init_escrow(&env, &client);

    client.append_attestation_digest(&BytesN::from_array(&env, &[3u8; 32]));
    let snapshot = attestation_view(&client);

    client.bind_primary_attestation_hash(&BytesN::from_array(&env, &[4u8; 32]));
    client.append_attestation_digest(&BytesN::from_array(&env, &[5u8; 32]));

    let fresh = attestation_view(&client);
    assert_ne!(snapshot, fresh);
    assert_eq!(snapshot, (false, 1));
    assert_eq!(fresh, (true, 2));
}

// ── authorization boundary (INV-ATT-1) ──────────────────────────────────────

/// Non-admin callers cannot mutate attestation state.
#[test]
fn test_non_admin_cannot_bind_append_or_revoke() {
    let env = Env::default();
    env.mock_all_auths();
    let client = deploy(&env);
    let _admin = init_escrow(&env, &client);

    // Disable blanket auth mocking so require_auth for the attacker fails.
    env.set_auths(&[]);

    let digest = BytesN::from_array(&env, &[0x88u8; 32]);

    assert!(client.try_bind_primary_attestation_hash(&digest).is_err());
    assert!(client.try_append_attestation_digest(&digest).is_err());
    assert!(client.try_revoke_attestation_digest(&0).is_err());

    let (bound, len) = attestation_view(&client);
    assert!(!bound);
    assert_eq!(len, 0);
}
