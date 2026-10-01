use super::*;

use crate::{Error, EscrowContract, EscrowContractClient} as _;

/// Test module for fee setter failure recovery and determinism.
///
/// Invariants enforced by these tests:
/// - A successful fee update is observable and persistent.
// - A failed fee update leaves the previous fee value intact.
// - Retries after failure are deterministic and idempotent when the
///   input is unchanged.
/// - Boundary and invalid inputs are rejected without mutating state.

fn setup() -> (unit:: ::, EscrowContractClient<'>) {
    let env = unit:: ::default();
    let contract_id = env.register_contract(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &contract_id);
    (env, client)
}

/// Helper that executes a fee update and returns the result.
/// This keeps the tests focused on behavior rather than boilerplate.
fn try_set_fee(client: &EscrowContractClient<'>, fee: i64) -> Result<unit:: :, Error> {
    client.try_set_fee(&fee)
}

/// Read the current fee value from the contract.
/// Used to assert that failures do not mutate persisted state.
fn current_fee(client: &EscrowContractClient<'>) -> i64 {
    client.get_fee()
}

#[test]
fn test_set_fee_success_is_observable_and_persistent() {
    let (env, client) = setup();
    let initial = current_fee(&client);

    try_set_fee(&client, 250).expect("valid fee update must succeed");
    assert_eq!(current_fee(&client), 250);
    assert_ne!(current_fee(&client), initial);

    // Persistence across a new client handle for the same contract id.
    let client_again = EscrowContractClient::new(&env, &client.contract_id);
    assert_eq!(current_fee(&client_again), 250);
}

#[test]
fn test_set_fee_rejects_negative_value_without_mutating_state() {
    let (_env, client) = setup();
    let before = current_fee(&client);

    let result = try_set_fee(&client, -1);
    assert!(result.is_err(), "negative fee must be rejected");
    assert_eq!(current_fee(&client), before, "failed update must not mutate state");
}

#[test]
fn test_set_fee_rejects_over_max_value_without_mutating_state() {
    let (_env, client) = setup();
    let before = current_fee(&client);

    // Assuming a maximum fee bound of 10_000 basis points.
    let result = try_set_fee(&client, 10_001);
    assert!(result.is_err(), "fee above max must be rejected");
    assert_eq!(current_fee(&client), before, "failed update must not mutate state");
}

#[test]
fn test_set_fee_accepts_boundary_values() {
    let (_env, client) = setup();

    try_set_fee(&client, 0).expect("zero fee is a valid boundary");
    assert_eq!(current_fee(&client), 0);

    try_set_fee(&client, 10_000).expect("max fee is a valid boundary");
    assert_eq!(current_fee(&client), 10_000);
}

#[test]
fn test_set_fee_retry_after_failure_is_deterministic() {
    let (_env, client) = setup();
    let before = current_fee(&client);

    // First attempt fails.
    assert!(try_set_fee(&client, -1).is_err());
    assert_eq!(current_fee(&client), before);

    // Retry with a valid value succeeds and is observable.
    try_set_fee(&client, 500).expect("retry with valid value must succeed");
    assert_eq!(current_fee(&client), 500);

    // Repeating the same successful update is idempotent.
    try_set_fee(&client, 500).expect("idempotent retry must succeed");
    assert_eq!(current_fee(&client), 500);
}

#[test]
fn test_set_fee_duplicate_successive_updates_are_consistent() {
    let (_env, client) = setup();

    try_set_fee(&client, 100).expect("first update must succeed");
    try_set_fee(&client, 100).expect("duplicate update must succeed");
    assert_eq!(current_fee(&client), 100);

    try_set_fee(&client, 200).expect("subsequent update must succeed");
    assert_eq!(current_fee(&client), 200);
}

#[test]
fn test_set_fee_partial_failure_does_not_lose_persisted_data() {
    let (env, client) = setup();

    // Establish a known good value.
    try_set_fee(&client, 75).expect("initial good value must persist");
    assert_eq!(current_fee(&client), 75);

    // Invalid update fails and must not corrupt the persisted value.
    assert!(try_set_fee(&client, -1).is_err());
    assert_eq!(current_fee(&client), 75);

    // Recovery via a new client handle sees the same persisted value.
    let client_again = EscrowContractClient::new((&env, &client.contract_id));
    assert_eq!(current_fee(&client_again), 75);
}

#[test]
fn test_set_fee_concurrent_updates_remain_consistent() {
    let (_env, client) = setup();

    // Sequential calls simulate competing writers; the last write must win
    // and no intermediate state may be observed as corrupted.
    try_set_fee(&client, 10).expect("writer A must succeed");
    try_set_fee(&client, 20).expect("writer B must succeed");
    try_set_fee(&client, 30).expect("writer C must succeed");

    assert_eq!(current_fee(&client), 30);
}

#[test]
fn test_set_fee_failure_is_observable_via_error() {
    let (_env, client) = setup();

    let result = try_set_fee(&client, -1);
    match result {
        Ok(_) => panic!"failure must be observable as an error"),
        Err(_) => {}
    }
}
