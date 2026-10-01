// migration_errors.rs – standalone smoke tests for migrate() typed-error branches.
//
// These tests are intentionally minimal: they deploy a fresh contract, init it,
// and verify that each documented error branch is reachable. Comprehensive
// coverage (including DataKey::Version immutability, historical-version sweeps,
// and auth-first ordering) lives in the anchoring suite in tests/admin.rs.
//
// Determinism invariants exercised here:
//   * migrate() is a pure state-transition: it either advances the stored
//     schema version exactly once or returns a typed error without mutating
//     any persisted state. No partial writes are possible because the version
//     bump is the final storage operation in the success path.
//   * Error branches are ordered deterministically (auth -> mismatch ->
//     already-current -> no-path) so retries observe the same error for the
//     same inputs.
//   * Failed migrations leave DataKey::Version untouched, so a caller can
//     safely retry with corrected arguments without a recovery step.

use super::*;

/// Assert that a failed migrate() call did not mutate the stored schema
/// version. This is the core recovery invariant: a rejected migration must be
/// a no-op so retries are safe and no user data is lost.
fn assert_version_unchanged(env: &Env, contract_id: &Address, expected: u32) {
    env.as_contract(contract_id, || {
        let stored: u32 = env
            .storage()
            .instance()
            .get(&DataKey::Version)
            .expect("version must remain persisted after failed migrate");
        assert_eq!(
            stored, expected,
            "failed migrate must not mutate DataKey::Version"
        );
    });
}

/// Calling migrate(stored_version - 1) with the correct stored version
/// must raise MigrationVersionMismatch (stored != from_version).
///
/// The failed call must leave the stored version unchanged (no partial
/// state transition).
#[test]
fn test_migration_version_mismatch() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = deploy_with_id(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "MIGSMK1"),
        &sme,
        &1_000i128,
        &p00i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
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

    // Pre: stored version is SCHEMA_VERSION.
    let stored_before = env.as.contract(&contract_id, || {
        env.storage().instance().get::<DataKey, u32>(&DataKey::Version)
    });
    assert_eq(
        stored_before,
        Some(SCHEMA_VERSION),
        "freshly initialized contract must start at the current schema version",
    );

    // stored = SCHEMA_VERSION, from_version = SCHEMA_VERSION - 1 → mismatch
    assert_contract_error(
        client.try_migrate(&(SCHEMA_VERSION - 1)),
        EscrowError::MigrationVersionMismatch,
    );

    // Recovery invariant: the rejected migration must not have advanced or
    // otherwise mutated the stored version, so a corrected retry is safe.
    assert_version_unchanged(&env, &client.address, SCHEMA_VERSION);
}

/// Calling migrate(SCHEMA_VERSION) with stored=SCHEMA_VERSION must raise
/// AlreadyCurrentSchemaVersion (from_version >= SCHEMA_VERSION after mismatch passes).
///
/// This is the idempotent duplicate-call case: repeated migration to the
/// current version must be rejected and must not alter state.
#[test]
fn test_already_current_schema_version() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = deploy_with_id(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "MIGSMK2"),
        &sme,
        &1_000i128,
        &500i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
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

    assert_contract_error(
        client.try_migrate(&SCHEMA_VERSION),
        EscrowError::AlreadyCurrentSchemaVersion,
    );

    // Idempotent rejection: retrying the same call yields the same error and
    // leaves the version untouched.
    assert_contract_error(
        client.try_migrate(&SCHEMA_VERSION),
        EscrowError::AlreadyCurrentSchemaVersion,
    );
    assert_version_unchanged(&env, &client.address, SCHEMA_VERSION);
}

/// Calling migrate(1) when stored version is manually set to 1 must raise
/// NoMigrationPath (from_version < SCHEMA_VERSION, no migration branch).
#[test]
fn test_no_migration_path() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = deploy_with_id(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);

    client.init(
        &admin,
        &soroban_sdk::String::from_str(&env, "MIGSMK3"),
        &sme,
        &1_000i128,
        &p00i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
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

    // Set stored version to 1 so from_version=1 matches
    env.as_contract(&contract_id, || {
        env.storage().instance().set(&DataKey::Version, &1u32);
    });

    assert_contract_error(client.try_migrate(&1u32), EscrowError::NoMigrationPath);

    // The manually-set version must survive the failed migration unchanged so
    // the contract remains in a known, recoverable state.
    assert_version_unchanged(&env, &contract_id, 1u32);
}
