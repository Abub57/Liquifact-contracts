// migration_errors.rs – compatibility regression for version / migrate / init bounds.
//
// Covers #1270: deterministic typed errors, empty-state defaults, duplicate
// handling, boundary versions, nonce gating, and storage persistence.

use super::*;

fn init_client(
    env: &Env,
    client: &LiquifactEscrowClient<'_>,
    admin: &Address,
    sme: &Address,
    id: &str,
) {
    client.init(
        admin,
        &soroban_sdk::String::from_str(env, id),
        sme,
        &1_000i128,
        &500i64,
        &0u64,
        &Address::generate(env),
        &None,
        &Address::generate(env),
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
}

#[test]
fn test_migration_version_mismatch() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = deploy_with_id(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    init_client(&env, &client, &admin, &sme, "MIGSMK1");
    // stored = SCHEMA_VERSION (6), from_version = 5 → mismatch
    assert_contract_error(
        client.try_migrate(&(SCHEMA_VERSION - 1), &0u32),
        EscrowError::MigrationVersionMismatch,
    );

    // Recovery invariant: the rejected migration must not have advanced or
    // otherwise mutated the stored version, so a corrected retry is safe.
    assert_version_unchanged(&env, &client.address, SCHEMA_VERSION);
}

#[test]
fn test_already_current_schema_version() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = deploy_with_id(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    init_client(&env, &client, &admin, &sme, "MIGSMK2");
    assert_contract_error(
        client.try_migrate(&SCHEMA_VERSION, &0u32),
        EscrowError::AlreadyCurrentSchemaVersion,
    );

    // Idempotent rejection: retrying the same call yields the same error and
    // leaves the version untouched.
    assert_contract_error(
        client.try_migrate(&SCHEMA_VERSION, &0u32),
        EscrowError::AlreadyCurrentSchemaVersion,
    );
    assert_version_unchanged(&env, &client.address, SCHEMA_VERSION);
}

#[test]
fn test_no_migration_path() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, client) = deploy_with_id(&env);
    let admin = Address::generate(&env);
    let sme = Address::generate(&env);
    init_client(&env, &client, &admin, &sme, "MIGSMK3");
    env.as_contract(&contract_id, || {
        env.storage().instance().set(&DataKey::Version, &1u32);
    });

    assert_contract_error(client.try_migrate(&1u32, &0u32), EscrowError::NoMigrationPath);
}
