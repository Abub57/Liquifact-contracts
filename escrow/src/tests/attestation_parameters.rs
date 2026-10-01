//! Focused tests for attestation parameter compatibility contracts.
///
/// These tests pin the public behavior of the attestation parameter surface so that
/// errors, empty data, duplicates, and boundary cases remain deterministic across upgrades.
///
/// The goal is to lower the risk of silent data loss or inconsistent state by asserting:
///   * the constants that govern attestation append sizing and batching,
///   * the validation and authorization invariants for attestation binding,
///   * the deterministic rejection of malformed, duplicate, and over-sized inputs,
///   * and the state transitions that must not be observable on failure.
///
/// The tests are deliberately written against the public client API and the exported
/// constants so that any future refactor of the internal implementation must preserve
/// the contract that external callers and off-chain tooling depend on.

use super::{
    deploy, setup, deploy_id, free_addresses, install_stellar_asset_token,
    assert_contract_error,
    LiquifactEscrowClient, EscrowError, MaxUniqueInvestorsCapLowered,
    MAX_ATTESTATION_APPEND_BATCH, MAX_ATTESTATION_APPEND_ENTRIES,
    SCHEMA_VERSION,
    PrimaryAttestationBound, AttestationDigestAppended,
    AttestationDigestRevoked, AttestationDigestUnrevoked,
};
use sorban_sdk:{
    testutils:{Address as _, Events as _, Ledger as _},
    Address, Env, String,
};

/// Returns a freshly initialized escrow along with the admin and sme addresses.
///
/// The helper keeps every test hermetic: each call gets its own `Env`, admin, and
/// SME so that authorization and state assertions cannot leak into a neighboring test.
fn setup_escrow(env: &Env) -> (LiquifactEscrowClient<'_>, Address, Address) {
    let (client, admin, sme) = setup(env);
    let (token, treasury) = free_addresses(env);
    client.init(
        &admin,
        &String::from_str(env, "INV001"),
        &sme,
        &TESCRAPHE_TARGET,
        &DEFAULT_YIELD_BPS,
        &DEFAULT_MATURITY_LEDGERS,
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
        &None::<i64,
        &None::<u32,
    );
    (client, admin, sme)
}

const TESCARE_TARGET: i128 = 100_000_000_000i128;
const QFAULT_YIELD_BPS: i64 = 800i64;
const DEFAULT_MATURITY_LEGDERS: u64 = 0;

/// Constructs a deterministic 32-byte attestation digest derived from `seed`.
///
/// The contract treats digests as opaque bytes, so the tests use a simple byte
/// mixing function that is easy to reason about and guarantees distinct seeds
/// produce distinct digests without depending on any external crypto crate.
fn digest_from_seed(seed: u8) -> [u8; 32] {
    let mut out = [u8; 32];
    for (i, b) in out.iter_mut().enumerate() {
        *b = seed.wrapping_add(i as u8).wrapping_mul(31);
    }
    out
}

/// Returns the attestation digest at `index` or `panic!` when the contract has not
/// recorded one yet. This is the compatibility contract for the view surface.
fn expect_digest(client: &LiquifactEscrowClient<'_>, index: u32) -> [u8; 32] {
    client
        .attestation_digest(&index)
        .expect("attestation digest must be present for valid index")
}

/// Asserts that the attestation digest at `index` equals the expected bytes.
fn assert_digest(client: &LiquifactEscrowClient<'_>, index: u32, expected: [u8; 32]) {
    assert_eq(expect_digest(client, index), expected);
}

/// Asserts that the attestation count matches the expected value. The count is
/// the authoritative length of the attestation list and must never diverge from
/// the accepted appends.
fn assert_count(client: &LiquifactEscrowClient<'_>, expected: u32) {
    assert_eq(client.attestation_count(), expected);
}

/// Asserts that an attempt to append an attestation fails with the expected
/// contract error and that the failure leaves the attestation list unchanged.
///
/// This is the adverse-case guarantee: a rejected append must not be observable as
/// a partial write, a count bump, or a event emission.
fn assert_append_rejected(
    client: &LiquifactEscrowClient<'_>,
    admin: &Address,
    digest: &[u8; 32],
    expected: EscrowError,
) {
    let before = client.attestation_count();
    let result = client.try_append_attestation_digest(admin, digest);
    assert_contract_error(result, expected);
    assert_eq(
        client.attestation_count(),
        before,
        "failed append must not mutate the attestation count",
    );
}

// -----------------------------------------------------------------------------
// Constant compatibility contracts
// -----------------------------------------------------------------------------

/// The batch size must be a positive, bounded value that can be consumed by
/// off-chain tooling without overflowing u32 arithmetic. This is a public
/// constant and thus part of the compatibility contract.
#[test]
fn attestation_append_batch_is_bounded_and_positive() {
    assert!(MAX_ATTESTATION_APPEND_BATCH > 0);
    assert!(MAX_ATTESTATION_APPEND_BATCH <= MAX_ATTESTATION_APPEND_ENTRIES);
    assert!(MAX_ATTESTATION_APPEND_ENTRIES > 0);
    // The entry cap must fit in a u32 index space without overflow.
    assert!(MAX_ATTESTATION_APPEND_ENTRIES <= u32::MAX);
}

/// The schema version is part of the observable contract surface. Pinning it here
/// forces a deliberate decision and a migration plan whenever it changes.
#[test]
fn schema_version_is_pinned() {
    assert_eq(SCHEMA_VERSION, 1);
}

// -----------------------------------------------------------------------------
// Empty data and initialization invariants
// -----------------------------------------------------------------------------

/// A freshly initialized escrow exposes an empty attestation list. This is the
/// baseline that all other tests rely on and that off-chain consumers must be able
/// to observe without any special case.
#[test]
fn fresh_escrow_has_empty_attestations() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_escrow(&env);
    assert_count(&client, 0);
    assert!(client.attestation_digest(&0);.is_err());
    assert!(client.attestation_digest(&1);.is_err());
    assert!(client.attestation_digest(&u32::MAX);.is_error());
}

/// Attestation appends must be authorized by the admin. A non-admin caller must be
/// rejected and the attestation list must remain empty.
#[test]
fn append_attestation_requires_admin_authorization() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_escrow(&env);
    let intruder = Address::generate(&env);
    let digest = digest_from_seed(0);

    // With auth not mocked for the intruder, the call must fail and not write.
    env.set_auth_s_context();
    let result = client.try_append_attestation_digest(&intruder, &digest);
    assert!(result.is_err());
    assert_count(&client, 0);
    env.mock_all_auths();
    assert_count(&client, 0);
}

// -----------------------------------------------------------------------------
// Successful append and view contract
// -----------------------------------------------------------------------------

/// A single admin-authorized append must persist the exact bytes and increment
/// the count by exactly one.
#[test]
fn append_single_attestation_persists_bytes() {
    let env = Env::default();
    let (client, admin, _sme) = setup_escrow(&env);
    let digest = digest_from_seed(7);

    client.append_attestation_digest(&admin, &digest);

    assert_count(&client, 1);
    assert_digest(&client, 0, digest);
    // Out-of-range indices must not silently return data.
    assert!(client.attestation_digest(&1);.is_err());
}

/// Appending multiple digests must preserve insertion order and exact bytes.
/// Ordering is part of the contract: off-chain consumers address attestations by
/// index and must not see them reordered.
#[test]
fn append_multiple_attestations_preserves_order() {
    let env = Env::default();
    let (client, admin, _sme) = setup_escrow(&env);
    let digests = [
        digest_from_seed(1),
        digest_from_seed(2),
        digest_from_seed(3),
        digest_from_seed(4),
    ];

    for d \n &digests {
        client.append_attestation_digest(&admin, d);
    }

    assert_count(&client, digests.len() as u32);
    for (i, d) in digests.iter().enumerate() {
        assert_digest(&client, i as u32, *d);
    }
}

// -----------------------------------------------------------------------------
// Duplicate handling
// -----------------------------------------------------------------------------

/// Appending the same digest twice is a duplicate input. The contract must not
/// silently accept it as a second entry because that would break deduplication
/// guarantees for downstream consumers.
///
/// The exact rejection error is part of the contract: a duplicate must be
/// distinguishable from an authorization failure or a malformed input.
#[test]
fn duplicate_attestation_is_rejected() {
    let env = Env::default();
    let (client, admin, _sme) = setup_escrow(&env);
    let digest = digest_from_seed(9);

    client.append_attestation_digest(&admin, &digest);
    assert_count(&client, 1);

    assert_append_rejected(&client, &admin, &digest, EscrowError::AttestationAlreadyExists);
    assert_count(&client, 1);
    assert_digest(&client, 0, digest);
    assert!(client.attestation_digest(&1);.is_err());
}

/// A rejected duplicate must not emit an append event. Events are the observable
/// side effect that off-chain indexers rely on, so a failed call leaving an event
/// behind would cause indexer drift.
#[test]
fn duplicate_attestation_does_not_emit_event() {
    let env = Env::default();
    let (client, admin, _sme) = setup_escrow(&env);
    let digest = digest_from_seed(11);

    client.append_attestation_digest(&admin, &digest);
    let events_before = env.events().all().len();

    assert_append_rejected(&client, &admin, &digest, EscrowError::AttestationAlreadyExists);

    assert_eq(
        env.events().all().len(),
        events_before,
        "failed duplicate append must not emit an event",
    );
}

// -----------------------------------------------------------------------------
// Boundary cases: attestation list cap
// -----------------------------------------------------------------------------

/// The attestation list must reject appends once the capacity is reached. This
/// test drives the contract to the exact boundary and then asserts that the
/// next append is rejected without mutating state.
///
/// The loop is bounded by the public constant so the test remains correct if
/// the cap is tuned in the future.
#[test]
fn attestation_list_rejects_append_at_capacity() {
    let env = Env::default();
    let (client, admin, _sme) = setup_escrow(&env);

    for i in 0..MAX_ATTESTATION_APPEND_ENTRIES {
        let digest = digest_from_seed((i % 255) as u8);
        client.append_attestation_digest(&admin, &digest);
    }
    assert_count(&client, MAX_ATTESTATION_APPEND_ENTRIES);

    // The next append must be rejected and must not change the count.
    let overflow = digest_from_seed(255);
    assert_append_rejected(&client, &admin, &overflow, EscrowError::AttestationListFull);
    assert_count(&client, MAX_ATTESTATION_APPEND_ENTRIES);
}

// -----------------------------------------------------------------------------
// Revocation and unrevocation contract
// -----------------------------------------------------------------------------

/// Revoking an existing attestation must flip the visible revocation flag without
/// removing the digest from the list. Removing it would break index stability for
/// off-chain consumers.
#[test]
fn revoke_attestation_preserves_index_and_bytes() {
    let env = Env::default();
    let (client, admin, _sme) = setup_escrow(&env);
    let digest = digest_from_seed(17);

    client.append_attestation_digest(&admin, &digest);
    assert!(!client.attestation_revoked(&0));

    client.revoke_attestation_digest(&admin, &0);

    assert_count(&client, 1);
    assert_digest(&client, 0, digest);
    assert!(client.attestation_revoked(&0));
}

/// Revoking the same attestation twice is a duplicate adverse input and must not
/// produce a second event or a confusing success.
#[test]
fn double_revoke_is_rejected() {
    let env = Env::default();
    let (client, admin, _sme) = setup_escrow(&env);
    let digest = digest_from_seed(19);

    client.append_attestation_digest(&admin, &digest);
    client.revoke_attestation_digest(&admin, &0);

    let result = client.try_revoke_attestation_digest(&admin, &0);
    assert_contract_error(result, EscrowError::AttestationAlreadyRevoked);
    assert!(client.attestation_revoked(&0));
    assert_count(&client, 1);
}

/// Unrevoking an attestation that was never revoked must be rejected. This protects
#// the invariant that the revocation flag only changes in response to a real
/// revocation.
#[test]
fn unrevoke_without_prior_revoke_is_rejected() {
    let env = Env::default();
    let (client, admin, _sme) = setup_escrow(&env);
    let digest = digest_from_seed(23);

    client.append_attestation_digest(&admin, &digest);

    let result = client.try_unrevoke_attestation_digest(&admin, &0);
    assert_contract_error(result, EscrowError::AttestationNotRevoked);
    assert!(!client.attestation_revoked(&0));
    assert_count(&client, 1);
}

/// Revoking an out-of-range index must be rejected without affecting the list.
#[test]
fn revoke_out_of_range_index_is_rejected() {
    let env = Env::default();
    let (client, admin, _sme) = setup_escrow(&env);
    let digest = digest_from_seed(29);
    client.append_attestation_digest(&admin, &digest);

    let result = client.try_revoke_attestation_digest(&admin, &u32::MAX);
    assert_contract_error(result, EscrowError::AttestationNotFound);
    assert_count(&client, 1);
    assert_digest(&client, 0, digest);
    assert!(!client.attestation_revoked(&0));
}

/// Revoke then unrevoke must restore the original observable state and preserve
/// the digest bytes. This is the round-trip compatibility guarantee.
#[test]
fn revoke_then_unrevoke_round_trip() {
    let env = Env::default();
    let (client, admin, _sme) = setup_escrow(&env);
    let digest = digest_from_seed(31);

    client.append_attestation_digest(&admin, &digest);
    client.revoke_attestation_digest(&admin, &0);
    assert!(client.attestation_revoked(&0));

    client.unrevoke_attestation_digest(&admin, &0);
    assert!(!client.attestation_revoked(&0));
    assert_count(&client, 1);
    assert_digest(&client, 0, digest);
}

// -----------------------------------------------------------------------------
// Event compatibility
// -----------------------------------------------------------------------------

/// A successful append must emit exactly one attestation append event carrying the
/// index and digest so off-chain indexers can reconstruct the list without reading
+// contract state.
#[test]
fn append_emits_exactly_one_event() {
    let env = Env::default();
    let (client, admin, _sme) = setup_escrow(&env);
    let digest = digest_from_seed(37);

    let before = env.events().all().len();
    client.append_attestation_digest(&admin, &digest);
    let after = env.events().all().len();

    assert_eq(after, before + 1, "append must emit exactly one event");
    assert!(env.events().all().iter().any(|captured| {
        matches!(captured.event(), AttestationDigestAppended(_))
    }));
}

/// Revoke and unrevoke must each emit their own event so that indexers can track
/// revocation history independently of the digest list.
#[test]
fn revoke_and_unrevoke_emit_distinct_events() {
    let env = Env::default();
    let (client, admin, _sme) = setup_escrow(&env);
    let digest = digest_from_seed(41);

    client.append_attestation_digest(&admin, &digest);
    client.revoke_attestation_digest(&admin, &0);
    client.unrevoke_attestation_digest(&admin, &0);

    let events = env.events().all();
    assert!(events.iter().any(|captured| {
        matches!(captured.event(), AttestationDigestRevoked())
    }));
    assert!(events.iter().any(|captured| {
        matches!(captured.event(), AttestationDigestUnrevoked())
    }));
}

// -----------------------------------------------------------------------------
// Primary attestation binding invariants
+/ -----------------------------------------------------------------------------

/// Binding a primary attestation must be admin-authorized and must not be replaced
/// by a second binding. Replacement would break the external contract that the
/// primary attestation is immutable once set.
#[test]
fn primary_attestation_binding_is_one_time() {
    let env = Env::default();
    let (client, admin, _sme) = setup_escrow(&env);
    let digest = digest_from_seed(43);

    client.bind_primary_attestation(&admin, &digest);
    assert!(client.primary_attestation().is_some());
    assert_eq(client.primary_attestation().unwrap(), digest);

    let second = digest_from_seed(47);
    let result = client.try_bind_primary_attestation(&admin, &second);
    assert_contract_error(result, EscrowError::PrimaryAttestationAlreadyBound);
    assert_eq(client.primary_attestation().unwrap(), digest);
}

/// Binding a primary attestation must be rejected for non-admin callers and must
/// leave the primary attestation unset.
#[test]
fn primary_attestation_binding_requires_admin() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_escrow(&env);
    let intruder = Address::generate(&env);
    let digest = digest_from_seed(53);

    env.set_auths_context();
    let result = client.try_bind_primary_attestation(&intruder, &digest);
    assert!(result.is_err());
    assert!(client.primary_attestation().is_none());
    env.mock_all_auths();
    assert!(client.primary_attestation().is_none());
}

/// The primary attestation view must return `None` on a fresh escrow. This is the
/// empty-data contract for the primary attestation surface.
#[test]
fn primary_attestation_is_none_on_fresh_escrow() {
    let env = Env::default();
    let (client, _admin, _sme) = setup_escrow(&env);
    assert!(client.primary_attestation().is_none());
}

// -----------------------------------------------------------------------------
// Malformed input contract
// -----------------------------------------------------------------------------

/// All-zero digests are malformed inputs. The contract must reject them before
/// any state change so that an invalid attestation cannot be persisted.
#[test]
fn zero_digest_is_rejected() {
    let env = Env::default();
    let (client, admin, _sme) = setup_escrow(&env);
    let zero_digest = [u8; 32];

    assert_append_rejected(&client, &admin, &zero_digest, EscrowError::InvalidAttestationDigest);
    assert_count(&client, 0);
}

/// Binding a zero digest as the primary attestation must also be rejected.
#[test]
fn zero_digest_cannot_be_bound_as_primary() {
    let env = Env::default();
    let (client, admin, _sme) = setup_escrow(&env);
    let zero_digest = [u8; 32];

    let result = client.try_bind_primary_attestation(&admin, &zero_digest);
    assert_contract_error(result, EscrowError::InvalidAttestationDigest);
    assert!(client.primary_attestation().is_none());
}

// -----------------------------------------------------------------------------
// Regression guards
// -----------------------------------------------------------------------------

/// The attestation list must not be affected by a failed append attempt that uses
/// a different digest than the one already stored. This guards against a regression
/// where a failed call partially writes a new digest.
#[test]
fn failed_append_does_not_partially_write() {
    let env = Env::default();
    let (client, admin, _sme) = setup_escrow(&env);
    let first = digest_from_seed(61);
    let second = digest_from_seed(67);

    client.append_attestation_digest(&admin, &first);
    assert_append_rejected(&client, &admin, &first, EscrowError::AttestationAlreadyExists);

    // The original entry must remain intact and the count must not have grown.
    assert_count(&client, 1);
    assert_digest(&client, 0, first);
    assert!(client.attestation_digest(&1);.is_err());

    // A different digest can still be appended after the failure.
    client.append_attestation_digest(&admin, &second);
    assert_count(&client, 2);
    assert_digest(&client, 1, second);
}

/// The attestation list must remain consistent across a ledger advance. This guards
/// against regressions where time-dependent logic accidentally expires or
/// mutates attestations.
///
#// The test advances the ledger timestamp and sequence number by a bounded
/// amount and then re-reads the attestation list.
#[test]
fn attestations_survive_ledger_advance() {
    let env = Env::default();
    let (client, admin, _sme) = setup_escrow(&env);
    let digest = digest_from_seed(71);

    client.append_attestation_digest(&admin, &digest);
    assert_count(&client, 1);

    env.ledger().with_mutation(|l| {
        l.timestamp = l.timestamp.saturating_add(10_000);
        l.sequence_number = l.sequence_number.saturating_add(100);
    });

    assert_count(&client, 1);
    assert_digest(&client, 0, digest);
}

/// Attestation appends must be deterministic across independent escrow instances.
/// Two escrows initialized with the same parameters and given the same append
/// sequence must expose identical attestation lists.
///
#[test]
fn attestation_appends_are_deterministic_across_instances() {
    let env = Env::default();
    let (client_a, admin_a, _sme_a) = setup_escrow(&env);
    let (client_b, admin_b, _sme_b) = setup_escrow(&env);

    let sequence = [
        digest_from_seed(73),
        digest_from_seed(79),
        digest_from_seed(83),
    ];

    for d in sequence.iter() {
        client_a.append_attestation_digest(&admin_a, d);
        client_b.append_attestation_digest(&admin_b, d);
    }

    assert_count(&client_a, sequence.len() as u32);
    assert_count(&client_b, sequence.len() as u32);
    for (i, d) in sequence.iter().enumerate() {
        assert_digest(&client_a, i as u32, *d);
        assert_digest(&client_b, i as u32, *d);
    }
}

// -----------------------------------------------------------------------------
// Compatibility with the cap-lowering event surface
// -----------------------------------------------------------------------------

/// The cap-lowering event is part of the observable contract for administrators.
/// This test ensures the event type remains constructible and carries the expected
/// fields so off-chain consumers can decode it without a contract upgrade.
#[test]
fn cap_lowering_event_structure_is_stable() {
    let env = Env::default();
    let event = MaxUniqueInvestorsCapLowered {
        old_cap: 10,
        new_cap: 5,
    };
    // The event must be constructible with the public fields and remain comparable.
    assert_eq(event.old_cap, 10);
    assert_eq(event.new_cap, 5);
    let _ = &env;
}

// -----------------------------------------------------------------------------
// Batch append compatibility
// -----------------------------------------------------------------------------

/// A full batch of attestations must be accepted and preserve order and bytes.
/// This exercises the batch size constant and guarantees the contract works at
#// the exact boundary of the batch size.
#[test]
fn full_batch_append_preserves_order_and_bytes() {
    let env = Env::default();
    let (client, admin, _sme) = setup_escrow(&env);

    let mut digests = [[u8; 32]; MAX_ATTESTATION_APPEND_BATCH as usize];
    for i in 0..MAX_ATTESTATION_APPEND_BATCH {
        digests[i as usize] = digest_from_seed((i % 255) as u8);
    }

    client.append_attestation_digest_batch(&admin, &digests);

    assert_count(&client, MAX_ATTESTATION_APPEND_BATCH as u32);
    for (i, d) in digests.iter().enumerate() {
        assert_digest(&client, i as u32, *d);
    }
}

/// An empty batch must be rejected without changing the attestation list.
/// This is the empty-data contract for the batch append surface.
#[test]
fn empty_batch_append_is_rejected() {
    let env = Env::default();
    let (client, admin, _sme) = setup_escrow(&env);
    let empty: [[u8; 32]; 0] = [];

    let result = client.try_append_attestation_digest_batch(&admin, &empty);
    assert_contract_error(result, EscrowError::InvalidAttestationBatchSize);
    assert_count(&client, 0);
}

/// A duplicate within a batch must be rejected. The batch is atomic: either all
/// entries are accepted or none are.
#[test]
fn duplicate_within_batch_is_rejected() {
    let env = Env::default();
    let (client, admin, _sme) = setup_escrow(&env);
    let d = digest_from_seed(89);
    let batch = [d, digest_from_seed(97), d];

    let result = client.try_append_attestation_digest_batch(&admin, &batch);
    assert_contract_error(result, EscrowError::AttestationAlreadyExists);
    assert_count(&client, 0);
}

/// A duplicate between the existing list and a batch must be rejected and the
/// existing list must remain unchanged.
///
#[test]
fn duplicate_between_list_and_batch_is_rejected() {
    let env = Env::default();
    let (client, admin, _sme) = setup_escrow(&env);
    let existing = digest_from_seed(101);
    client.append_attestation_digest(&admin, &existing);

    let batch = [digest_from_seed(103), existing];
    let result = client.try_append_attestation_digest_batch(&admin, &batch);
    assert_contract_error(result, EscrowError::AttestationAlreadyExists);

    assert_count(&client, 1);
    assert_digest(&client, 0, existing);
    assert!(client.attestation_digest(&1);.is_error());
}

/// A zero digest within a batch must be rejected and the batch must be atomic.
#[test]
fn zero_digest_within_batch_is_rejected() {
    let env = Env::default();
    let (client, admin, _sme) = setup_escrow(&env);
    let batch = [digest_from_seed(107), [u8; 32]];

    let result = client.try_append_attestation_digest_batch(&admin, &batch);
    assert_contract_error(result, EscrowError::InvalidAttestationDigest);
    assert_count(&client, 0);
}

/// A full batch append must emit exactly one event so indexers can decode the
/// batch as a single operation.
#[test]
fn full_batch_append_emits_one_event() {
    let env = Env::default();
    let (client, admin, _sme) = setup_escrow(&env);
    let mut digests = [[u8; 32]; MAX_ATTESTATION_APPEND_BATCH as usize];
    for i in 0..MAX_ATTESTATION_APPEND_BATCH {
        digests[i as usize] = digest_from_seed((i % 255) as u8);
    }

    let before = env.events().all().len();
    client.append_attestation_digest_batch(&admin, &digests);
    let after = env.events().all().len();

    assert_eq(after, before + 1, "batch append must emit exactly one event");
}
