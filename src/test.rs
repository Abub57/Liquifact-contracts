#[cfg(test)]
mod tests {
    use crate::*;
    use soroban_sdk::{
        testutils::{Address as _, Events, Ledger},
        Address, BytesN, Env,
    };
    use std::thread;

    fn setup_test(env: &Env) -> (YieldTierContractClient<'_>, Address) {
        let contract_id = env.register(YieldTierContract, ());
        let client = YieldTierContractClient::new(env, &contract_id);
        let admin = Address::generate(env);
        (client, admin)
    }

    fn setup_initialized_test(env: &Env) -> (YieldTierContractClient<'_>, Address) {
        let (client, admin) = setup_test(env);
        client.init(&admin);
        (client, admin)
    }

    // ── 1. Basic Functionality & Default States ──────────────────────────────

    #[test]
    fn test_get_yield_tier_returns_default_when_unset() {
        let env = Env::default();
        let (client, _admin) = setup_test(&env);
        let state = client.get_yield_tier();
        assert_eq!(state, YieldTierState::Unset);
        assert_eq!(client.get_version(), 0);
    }

    #[test]
    fn test_get_yield_tier_returns_stored_state() {
        let env = Env::default();
        let (client, _admin) = setup_initialized_test(&env);
        env.mock_all_auths();

        client.set_yield_tier(&YieldTierState::Tier2);
        assert_eq!(client.get_yield_tier(), YieldTierState::Tier2);
        assert_eq!(client.get_version(), 1);

        client.set_yield_tier(&YieldTierState::Tier3);
        assert_eq!(client.get_yield_tier(), YieldTierState::Tier3);
        assert_eq!(client.get_version(), 2);
    }

    // ── 2. Authorization & Initializer Guards ────────────────────────────────

    #[test]
    fn test_upgrade_admin_allowed() {
        let env = Env::default();
        let (client, _admin) = setup_initialized_test(&env);
        env.mock_all_auths();

        let new_wasm = BytesN::from_array(&env, &[1; 32]);
        let res = client.upgrade(&new_wasm);
        assert_eq!(res, ());

        let binding = env.events().all();
        assert_eq!(binding.events().len(), 1);

        assert_eq!(client.get_version(), 1);
    }

    #[test]
    fn test_upgrade_non_admin_rejected() {
        let env = Env::default();
        let (client, _admin) = setup_initialized_test(&env);

        let new_wasm = BytesN::from_array(&env, &[1; 32]);
        let result = client.try_upgrade(&new_wasm);
        assert!(result.is_err());
        assert_eq!(client.get_version(), 0);
    }

    #[test]
    fn test_upgrade_before_init_rejected() {
        let env = Env::default();
        let (client, _admin) = setup_test(&env);
        env.mock_all_auths();

        let new_wasm = BytesN::from_array(&env, &[1; 32]);
        let result = client.try_upgrade(&new_wasm);
        assert_eq!(result, Err(Ok(Error::NotInitialized)));
    }

    #[test]
    fn test_set_yield_tier_admin_authorized() {
        let env = Env::default();
        let (client, _admin) = setup_initialized_test(&env);
        env.mock_all_auths();

        client.set_yield_tier(&YieldTierState::Tier1);
        let binding = env.events().all();
        assert_eq!(binding.events().len(), 1);

        assert_eq!(client.get_yield_tier(), YieldTierState::Tier1);
        assert_eq!(client.get_version(), 1);
    }

    #[test]
    fn test_set_yield_tier_non_admin_rejected() {
        let env = Env::default();
        let (client, _admin) = setup_initialized_test(&env);

        let result = client.try_set_yield_tier(&YieldTierState::Tier1);
        assert!(result.is_err());
        assert_eq!(client.get_yield_tier(), YieldTierState::Unset);
        assert_eq!(client.get_version(), 0);
    }

    #[test]
    fn test_set_yield_tier_before_init_rejected() {
        let env = Env::default();
        let (client, _admin) = setup_test(&env);
        env.mock_all_auths();

        let result = client.try_set_yield_tier(&YieldTierState::Tier1);
        assert_eq!(result, Err(Ok(Error::NotInitialized)));
        assert_eq!(client.get_yield_tier(), YieldTierState::Unset);
    }

    #[test]
    fn test_get_admin_before_and_after_init() {
        let env = Env::default();
        let (client, admin) = setup_test(&env);
        assert_eq!(client.try_get_admin(), Err(Ok(Error::NotInitialized)));

        client.init(&admin);
        assert_eq!(client.get_admin(), admin);
    }

    // ── 3. Racing Initializations & State Preservation ───────────────────────

    #[test]
    fn test_racing_init_rejects_duplicate_and_preserves_admin() {
        let env = Env::default();
        let (client, admin) = setup_test(&env);
        let racing_attacker = Address::generate(&env);

        // First initialization succeeds
        client.init(&admin);
        assert_eq!(client.get_admin(), admin);

        // Concurrent/racing re-initialization fails deterministically
        let res = client.try_init(&racing_attacker);
        assert!(res.is_err());

        // Invariant check: original admin is preserved intact
        assert_eq!(client.get_admin(), admin);
        assert_eq!(client.get_version(), 0);
    }

    // ── 4. Concurrency Hardening: Monotonic Sequencing & Retries ─────────────

    #[test]
    fn test_duplicate_writes_are_deterministic_and_idempotent() {
        let env = Env::default();
        let (client, _admin) = setup_initialized_test(&env);
        env.mock_all_auths();

        // Repeated writes with the exact same tier state
        client.set_yield_tier(&YieldTierState::Tier2);
        assert_eq!(client.get_yield_tier(), YieldTierState::Tier2);
        assert_eq!(client.get_version(), 1);

        client.set_yield_tier(&YieldTierState::Tier2);
        assert_eq!(client.get_yield_tier(), YieldTierState::Tier2);
        assert_eq!(client.get_version(), 2);
    }

    #[test]
    fn test_concurrent_writes_are_serialized_with_monotonic_version() {
        let env = Env::default();
        let (client, _admin) = setup_initialized_test(&env);
        env.mock_all_auths();

        let sequence = [
            YieldTierState::Tier1,
            YieldTierState::Tier2,
            YieldTierState::Tier3,
            YieldTierState::Tier1,
            YieldTierState::Unset,
        ];

        for (idx, tier) in sequence.iter().enumerate() {
            client.set_yield_tier(tier);
            let binding = env.events().all();
            assert_eq!(binding.events().len(), 1);

            assert_eq!(client.get_yield_tier(), *tier);
            assert_eq!(client.get_version(), (idx + 1) as u32);
        }
    }

    #[test]
    fn test_optimistic_concurrency_control_success_and_stale_rejection() {
        let env = Env::default();
        let (client, _admin) = setup_initialized_test(&env);
        env.mock_all_auths();

        assert_eq!(client.get_version(), 0);

        // Writer 1 reads version 0 and writes Tier1 successfully
        let res1 = client.set_tier_with_version(&YieldTierState::Tier1, &0);
        assert_eq!(res1, ());
        assert_eq!(client.get_yield_tier(), YieldTierState::Tier1);
        assert_eq!(client.get_version(), 1);

        // Racing Writer 2 also based on stale version 0 attempts to write Tier2
        let res2 = client.try_set_tier_with_version(&YieldTierState::Tier2, &0);
        assert_eq!(res2, Err(Ok(Error::StaleVersion)));

        // Invariant: Tier1 remains intact, version has not incremented on failure
        assert_eq!(client.get_yield_tier(), YieldTierState::Tier1);
        assert_eq!(client.get_version(), 1);

        // Writer 2 refreshes to current version (1) and retries successfully
        let res3 = client.set_tier_with_version(&YieldTierState::Tier2, &1);
        assert_eq!(res3, ());
        assert_eq!(client.get_yield_tier(), YieldTierState::Tier2);
        assert_eq!(client.get_version(), 2);
    }

    // ── 5. Failure Recovery & Error Isolation ────────────────────────────────

    #[test]
    fn test_failed_unauthorized_call_does_not_mutate_state_or_leak_events() {
        let env = Env::default();
        let (client, _admin) = setup_initialized_test(&env);

        // Failed unauthorized mutation
        let res = client.try_set_yield_tier(&YieldTierState::Tier3);
        assert!(res.is_err());

        // No spurious events leaked from failed invocation
        let binding_before = env.events().all();
        assert_eq!(binding_before.events().len(), 0);

        // State and version remain unchanged
        assert_eq!(client.get_yield_tier(), YieldTierState::Unset);
        assert_eq!(client.get_version(), 0);

        // Subsequent authorized retry succeeds cleanly
        env.mock_all_auths();
        client.set_yield_tier(&YieldTierState::Tier3);

        let binding_after = env.events().all();
        assert_eq!(binding_after.events().len(), 1);

        assert_eq!(client.get_yield_tier(), YieldTierState::Tier3);
        assert_eq!(client.get_version(), 1);
    }

    #[test]
    fn test_failed_upgrade_does_not_corrupt_yield_tier_state() {
        let env = Env::default();
        let (client, _admin) = setup_initialized_test(&env);
        env.mock_all_auths();

        client.set_yield_tier(&YieldTierState::Tier2);
        assert_eq!(client.get_version(), 1);

        // Clear mock auths to test unauthenticated upgrade failure
        env.set_auths(&[]);
        let dummy_wasm = BytesN::from_array(&env, &[9; 32]);
        let res = client.try_upgrade(&dummy_wasm);
        assert!(res.is_err());

        // Prior state intact
        assert_eq!(client.get_yield_tier(), YieldTierState::Tier2);
        assert_eq!(client.get_version(), 1);
    }

    // ── 6. Timing & Ledger Sequence Boundaries ───────────────────────────────

    #[test]
    fn test_concurrency_across_ledger_sequence_and_timestamp_advance() {
        let env = Env::default();
        let (client, _admin) = setup_initialized_test(&env);
        env.mock_all_auths();

        // Write on initial ledger
        client.set_yield_tier(&YieldTierState::Tier1);
        assert_eq!(client.get_yield_tier(), YieldTierState::Tier1);
        assert_eq!(client.get_version(), 1);

        // Advance ledger sequence by 10,000 blocks and timestamp by 7 days
        let mut ledger_info = env.ledger().get();
        ledger_info.sequence_number += 10_000;
        ledger_info.timestamp += 7 * 86_400;
        env.ledger().set(ledger_info);

        // State remains completely intact across time boundary
        assert_eq!(client.get_yield_tier(), YieldTierState::Tier1);
        assert_eq!(client.get_version(), 1);

        // Subsequent operation succeeds deterministically
        client.set_yield_tier(&YieldTierState::Tier3);
        assert_eq!(client.get_yield_tier(), YieldTierState::Tier3);
        assert_eq!(client.get_version(), 2);
    }

    // ── 7. Multi-threaded Parallel Environment Isolation ─────────────────────

    #[test]
    fn test_multithreaded_concurrent_independent_deployments() {
        let num_threads = 8;
        let mut handles = std::vec::Vec::new();

        for thread_idx in 0..num_threads {
            let handle = thread::spawn(move || {
                let env = Env::default();
                env.mock_all_auths();
                let (client, admin) = setup_test(&env);

                client.init(&admin);
                assert_eq!(client.get_admin(), admin);

                let target_tier = match thread_idx % 3 {
                    0 => YieldTierState::Tier1,
                    1 => YieldTierState::Tier2,
                    _ => YieldTierState::Tier3,
                };

                client.set_yield_tier(&target_tier);
                assert_eq!(client.get_yield_tier(), target_tier);
                assert_eq!(client.get_version(), 1);

                // Duplicate idempotent write
                client.set_yield_tier(&target_tier);
                assert_eq!(client.get_yield_tier(), target_tier);
                assert_eq!(client.get_version(), 2);
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().expect("Worker thread panicked");
        }
    }
}
