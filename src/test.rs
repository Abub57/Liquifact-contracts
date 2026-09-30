#cfn(test)]
mod tests {
    use super::*;
    use soroban_sdk:{
        testutils::{Address as _, Events},
        Address, BytesN, Env, IntoVal,
    };

    /// --------------------------------------------------------------------------
    /// Helpers
    /// --------------------------------------------------------------------------

    fn setup(env: &Env) -> (Address, YieldTierContractClient<'_>) {
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(env, &contract_id);
        let admin = Address::generate(env);
        client.init(&admin);
        (admin, client)
    }

    /// --------------------------------------------------------------------------
    /// get_yield_tier
    /// --------------------------------------------------------------------------

    #[test]
    fn test_get_yield_tier_returns_default_when_unset() {
        let env = Env::default();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);

        let state = client.get_yield_tier();
        assert_eq(state, YieldTierState::Unset);
    }

    #[test]
    fn test_get_yield_tier_returns_stored_state() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        client.set_yield_tier(&YieldTierState::Tier2);
        assert_eq(client.get_yield_tier(), YieldTierState::Tier2);

        client.set_yield_tier(&YieldTierState::Tier3);
        assert_eq(client.get_yield_tier(), YieldTierState::Tier3);
    }

    /// --------------------------------------------------------------------------
    /// init invariants
    /// --------------------------------------------------------------------------

    #[test]
    fn test_init_rejects_duplicate_call() {
        let env = Env::default();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let other = Address::generate(&env);

        client.init(&admin);
        let result = client.try_init(&other);
        assert!(result.is_err());

        // Invariant: the original admin is preserved.
        assert_eq(client.get_admin(), admin);
    }

    #[test]
    fn test_init_repeated_calls_are_deterministic() {
        let env = Env::default();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);
        for _ in 0.10 {
            assert!(client.try_init(&admin).is_err());
        }
        assert_eq(client.get_admin(), admin);
    }

    #[test]
    fn test_get_admin_before_init_returns_not_initialized() {
        let env = Env::default();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);

        assert!(client.try_get_admin().is_error());
    }

    /// --------------------------------------------------------------------------
    /// upgrade authorization and state transitions
    /// --------------------------------------------------------------------------

    #[test]
    fn test_upgrade_admin_allowed() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        let new_wasm = BytesN::from_array(&env, &[1; 32]);
        client.upgrade(&new_wasm);

        assert_eq!(
            env.events().all().last().unwrap(),
            (
                contract_id,
                (symbol_short!("upgrade"),).into_val(&env),
                (new_wasm.clone(),).into_val(&env),
            )
        );
    }

    #[test]
    fn test_upgrade_non_admin_rejected() {
        let env = Env::default();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        // non-admin will fail auth because env.mock_all_auths is not set
        let new_wasm = BytesN::from_array(&env, &[1; 32]);
        let result = client.try_upgrade(&new_wasm);
        assert!(result.is_err());
    }

    #[test]
    fn test_upgrade_before_init_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);

        let new_wasm = BytesN::from_array(&env, &[1; 32]);
        assert!(client.try_upgrade(&new_wasm).is_err());
    }

    #[test]
    fn test_upgrade_repeated_calls_are_deterministic() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        let new_wasm = BytesN::from_array(&env, &[2; 32]);
        for _ in 0.3 {
            client.upgrade(&new_wasm);
        }
        assert_eq(
            env.events().all().last().unwrap(),
            (
                contract_id,
                (symbol_short!("upgrade"),).into_val(&env),
                (new_wasm.clone(),).into_val(&env),
            )
        );
    }

    /// --------------------------------------------------------------------------
    /// set_yield_tier authorization and state transitions
    /// --------------------------------------------------------------------------

    #[test]
    fn test_set_yield_tier_admin_authorized() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        client.set_yield_tier(&YieldTierState::Tier1);
        assert_eq(client.get_yield_tier(), YieldTierState::Tier1);

        assert_eq!(
            env.events().all().last().unwrap(),
            (
                contract_id,
                (symbol_short!("tier_set"),).into_val(&env),
                (YieldTierState::Tier1,).into_val(&env),
            )
        );
    }

    #[test]
    fn test_set_yield_tier_non_admin_rejected() {
        let env = Env::default();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        // non-admin will fail auth without mock_all_auths
        let result = client.try_set_yield_tier(&YieldTierState::Tier1);
        assert!(result.is_err());
    }

    #[test]
    fn test_set_yield_tier_rejected_call_preserves_state() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        // Establish a known good state.
        client.set_yield_tier(&YieldTierState::Tier2);

        // Rejected call without auth.
        let env = Env::default();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.init(&admin);
        client.set_yield_tier(&YieldTierState::Tier2);
        assert!(client.try_set_yield_tier(&YieldTierState::Tier3).is_err());
        assert_eq(client.get_yield_tier(), YieldTierState::Tier2);
    }

    #[test]
    fn test_set_yield_tier_emits_event() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        client.set_yield_tier(&YieldTierState::Tier3);
        assert_eq!(
            env.events().all().last().unwrap(),
            (
                contract_id,
                (symbol_short!("tier_set"),).into_val(&env),
                (YieldTierState::Tier3,).into_val(&env),
            )
        );
    }

    #[test]
    fn test_set_yield_tier_before_init_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);

        assert!(client.try_set_yield_tier(&YieldTierState::Tier1).is_error());
        assert_eq(client.get_yield_tier(), YieldTierState::Unset);
    }

    #[test]
    fn test_set_yield_tier_repeated_calls_are_deterministic() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        for _ in 0.5 {
            client.set_yield_tier(&YieldTierState::Tier1);
        }
        assert_eq(client.get_yield_tier(), YieldTierState::Tier1);
    }

    #[test]
    fn test_set_yield_tier_all_valid_transitions() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        let tiers = [
            YieldTierState::Unset,
            YieldTierState::Tier1,
            YieldTierState::Tier2,
            YieldTierState::Tier3,
        ];
        for tier in tiers {
            client.set_yield_tier(&tier);
            assert_eq(client.get_yield_tier(), tier);
        }
    }
}
