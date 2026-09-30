#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Events},
        Address, BytesN, Env, IntoVal,
    };

    fn assert_contract_error<T: std::fmt::Debug>(
        result: Result<T, Result<soroban_sdk::Error, soroban_sdk::InvokeError>>,
        expected: Error,
    ) {
        let expected_code = expected as u32;
        match result {
            Err(Ok(error)) => assert_eq!(
                error,
                soroban_sdk::Error::from_contract_error(expected_code)
            ),
            Err(Err(soroban_sdk::InvokeError::Contract(code))) => {
                assert_eq!(code, expected_code)
            }
            other => panic!("expected contract error {expected_code}, got {other:?}"),
        }
    }

    #[test]
    fn test_init_rejects_duplicate_without_changing_admin() {
        let env = Env::default();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let replacement_admin = Address::generate(&env);
        client.init(&admin);
        let events_before = env.events().all();

        assert_contract_error(
            client.try_init(&replacement_admin),
            Error::AlreadyInitialized,
        );
        assert_eq!(
            env.storage().instance().get::<_, Address>(&ADMIN_KEY),
            Some(admin)
        );
        assert_eq!(env.events().all(), events_before);
    }

    #[test]
    fn test_admin_operations_reject_uninitialized_contract() {
        let env = Env::default();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);
        let new_wasm = BytesN::from_array(&env, &[1; 32]);

        assert_eq!(
            client.try_upgrade(&new_wasm),
            Ok(Err(Error::NotInitialized))
        );
        assert_eq!(
            client.try_set_yield_tier(&YieldTierState::Tier1),
            Ok(Err(Error::NotInitialized))
        );
        assert_eq!(client.get_yield_tier(), YieldTierState::Unset);
        assert!(env.events().all().is_empty());
    }

    #[test]
    fn test_all_yield_tier_values_and_repeated_submissions_are_deterministic() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.init(&admin);

        for tier in [
            YieldTierState::Unset,
            YieldTierState::Tier1,
            YieldTierState::Tier2,
            YieldTierState::Tier3,
        ] {
            client.set_yield_tier(&tier);
            client.set_yield_tier(&tier);
            assert_eq!(client.get_yield_tier(), tier);
        }

        assert_eq!(env.events().all().len(), 8);
    }

    #[test]
    fn test_get_yield_tier_returns_default_when_unset() {
        let env = Env::default();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);

        let state = client.get_yield_tier();
        assert_eq!(state, YieldTierState::Unset);
    }

    #[test]
    fn test_get_yield_tier_returns_stored_state() {
        let env = Env::default();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        client.set_yield_tier(&YieldTierState::Tier2);
        assert_eq!(client.get_yield_tier(), YieldTierState::Tier2);

        client.set_yield_tier(&YieldTierState::Tier3);
        assert_eq!(client.get_yield_tier(), YieldTierState::Tier3);
    }

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
    fn test_set_yield_tier_admin_authorized() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, YieldTierContract);
        let client = YieldTierContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        client.init(&admin);

        client.set_yield_tier(&YieldTierState::Tier1);
        assert_eq!(client.get_yield_tier(), YieldTierState::Tier1);

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
}
