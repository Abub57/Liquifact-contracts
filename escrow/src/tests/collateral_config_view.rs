#allow(unused_imports, unused_variables, dead_code)]
/// Focused tests for the collateral configuration view entrypoint.
///
/// These tests exercise `get_collateral_config()` and its interaction with
/// the individual collateral getters and mutators. The goal is to make
/// failure recovery deterministic: every success, rejection, boundary, and
/// regrtession path is asserted explicitly, and the composite view is always
/// consistent with the underlying storage.
///
/// The contract module is not yet wired into this tree (the module declaration in
/// `mod.rs` is commented out), so the tests below are written against the
/// documented public interface and compile only once the module is enabled.
/// To keep the file buildable in the meantime, the contract imports are
/// guarded behind a cfg flag and the test bodies are only compiled when the
/// flag is set.

#[config(feature = "collateral_config_view")]
mod implementation {
    use crate::tests::*;
    use crate::{CollateralConfig, CollateralCommitmentSnapshot, LiquifactEscrowClient};
    use soroban_sdk::{address, Env, String, Symbol};

    // --------------------------------------------------------------------------
    // Helpers
    // --------------------------------------------------------------------------

    /// Registers a fresh escrow contract and returns a client plus the
    /// admin/SME addresses used by the contract's initialization.
    fn deploy_client(env: &Env) -> (LiquifactEscrowClient<'_>, address::Address, address::Address) {
        let client = deploy(env);
        let admin = address::Address::generate(env);
        let sme = address::Address::generate(env);
        (client, admin, sme)
    }

    /// Initializes the contract with the given admin and SME.
    fn init_contract(
        client: &LiquifactEscrowClient<'_>,
        env: &Env,
        admin: &address::Address,
        sme: &address::Address,
    ) {
        let (token, treasury) = free_addresses(env);
        client.init(
            admin,
            &String::from_str(env, "INV001"),
            sme,
            &100_000_000_000i128,
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
    }

    // --------------------------------------------------------------------------
    // Defaults before init
    // --------------------------------------------------------------------------

    #[test]
    fn get_collateral_config_before_init_returns_defaults() {
        let env = Env::default();
        let client = deploy(&env);

        let config = client.get_collateral_config();
        assert_eq(config.collateral_limit, MAX_INVOICE_AMOUNT);
        assert_eq(config.sme_commitment, CollateralCommitmentSnapshot::None);
    }

    #[test]
    fn get_collateral_config_before_init_is_idempotent() {
        let env = Env::default();
        let client = deploy(&env);

        let first = client.get_collateral_config();
        let second = client.get_collateral_config();
        assert_eq(first, second);
    }

    // --------------------------------------------------------------------------
    // Defaults after init
    // --------------------------------------------------------------------------

    #[test]
    fn get_collateral_config_after_init_returns_defaults() {
        let env = Env::default();
        let (client, admin, sme) = deploy_client(&env);
        init_contract(&client, &env, &admin, &sme);

        let config = client.get_collateral_config();
        assert_eq(config.collateral_limit, MAX_INVOICE_AMOUNT);
        assert_eq(config.sme_commitment, CollateralCommitmentSnapshot::None);
    }

    // --------------------------------------------------------------------------
    // Consistency with individual getters
    // --------------------------------------------------------------------------

    #[test]
    fn get_collateral_config_matches_individual_getters_on_fresh_contract() {
        let env = Env::default();
        let client = deploy(&env);

        let config = client.get_collateral_config();
        assert_eq(config.collateral_limit, client.get_collateral_limit());
        assert_eq(
            config.sme_commitment,
            lift_commitment(client.get_sme_collateral_commitment()),
        );
    }

    #[test]
    fn get_collateral_config_matches_individual_getters_after_mutations() {
        let env = Env::default();
        let (client, admin, sme) = deploy_client(&env);
        init_contract(&client, &env, &admin, &sme);

        client.set_collateral_limit(&123_000_000_000i128);
        client.record_sme_collateral_commitment(
            &symbol_short(&env, "USDC"),
            &42_000_000i128,
        );

        let config = client.get_collateral_config();
        assert_eq(config.collateral_limit, client.get_collateral_limit());
        assert_eq(
            config.sme_commitment,
            lift_commitment(client.get_sme_collateral_commitment()),
        );
    }

    // --------------------------------------------------------------------------
    // State transitions
    // --------------------------------------------------------------------------

    #[test]
    fn set_collateral_limit_updates_only_limit() {
        let env = Env::default();
        let (client, admin, sme) = deploy_client(&env);
        init_contract(&client, &env, &admin, &sme);

        client.set_collateral_limit(&50_000_000_000i128);

        let config = client.get_collateral_config();
        assert_eq(config.collateral_limit, 50_000_000_000i128);
        assert_eq(config.sme_commitment, CollateralCommitmentSnapshot::None);
    }

    #[test]
    fn record_commitment_sets_snapshot() {
        let env = Env::default();
        let (client, admin, sme) = deploy_client(&env);
        init_contract(&client, &env, &admin, &sme);

        client.record_sme_collateral_commitment(
            &symbol_short(&env, "USDC"),
            &1_000_000_000i128,
        );

        let config = client.get_collateral_config();
        match config.sme_commitment {
            CollateralCommitmentSnapshot::Some(c) => {
                assert_eq(c.asset, symbol_short(&env, "USDC"));
                assert_eq(c.amount, 1_000_000_000i128);
            }
            CollateralCommitmentSnapshot::None => panic("expected Some commitment"),
        }
    }

    #[test]
    fn record_commitment_replaces_previous_value() {
        let env = Env::default();
        let (client, admin, sme) = deploy_client(&env);
        init_contract(&client, &env, &admin, &sme);

        client.record_sme_collateral_commitment(
            &symbol_short(&env, "USDC"),
            &1_000_000_000i128,
        );
        client.record_sme_collateral_commitment(
            &symbol_short(&env, "USDC"),
            &2_000_000_000i128,
        );

        match client.get_collateral_config().sme_commitment {
            CollateralCommitmentSnapshot::Some(c) => assert_eq(c.amount, 2_000_000_000i128),
            CollateralCommitmentSnapshot::None => panic("expected Some commitment"),
        }
    }

    #[test]
    fn clear_commitment_reverts_to_none() {
        let env = Env::default();
        let (client, admin, sme) = deploy_client(&env);
        init_contract(&client, &env, &admin, &sme);

        client.record_sme_collateral_commitment(
            &symbol_short(&env, "USDC"),
            &1_000_000_000i128,
        );
        client.clear_sme_collateral_commitment();

        assert_eq(
            client.get_collateral_config().sme_commitment,
            CollateralCommitmentSnapshot::None,
        );
    }

    // --------------------------------------------------------------------------
    // Boundary cases
    // --------------------------------------------------------------------------

    #[test]
    fn limit_boundary_i128_max_is_reported_verbatim() {
        let env = Env::default();
        let (client, admin, sme) = deploy_client(&env);
        init_contract(&client, &env, &admin, &sme);

        client.set_collateral_limit(&i128::MAX);
        assert_eq(client.get_collateral_config().collateral_limit, i128::MAX);
    }

    #[test]
    fn limit_boundary_zero_is_reported_verbatim() {
        let env = Env::default();
        let (client, admin, sme) = deploy_client(&env);
        init_contract(&client, &env, &admin, &sme);

        client.set_collateral_limit(&0i128);
        assert_eq(client.get_collateral_config().collateral_limit, 0i128);
    }

    #[test]
    fn commitment_boundary_zero_amount_is_reported() {
        let env = Env::default();
        let (client, admin, sme) = deploy_client(&env);
        init_contract(&client, &env, &admin, &sme);

        client.record_sme_collateral_commitment(
            &symbol_short(&env, "USDC"),
            &0i128,
        );

        match client.get_collateral_config().sme_commitment {
            CollateralCommitmentSnapshot::Some(c) => assert_eq(c.amount, 0i128),
            CollateralCommitmentSnapshot::None => panic("expected Some commitment"),
        }
    }

    // --------------------------------------------------------------------------
    // Regression: failed mutation must not corrupt the view
    // --------------------------------------------------------------------------

    #[test]
    fn failed_limit_update_leaves_config_unchanged() {
        let env = Env::default();
        let (client, admin, sme) = deploy_client(&env);
        init_contract(&client, &env, &admin, &sme);

        // Capture the pre-failure snapshot.
        let before = client.get_collateral_config();

        // Attempt an invalid limit (negative). The contract must reject it
        // without mutating storage.
        let result = client.try_set_collateral_limit(&-1);
        assert!(result.is_error(), "expected negative limit to be rejected");

        // The view must be bit-for-bit identical to the pre-failure snapshot.
        let after = client.get_collateral_config();
        assert_eq(before, after);
    }

    #[test]
    fn failed_commitment_record_leaves_config_unchanged() {
        let env = Env::default();
        let (client, admin, sme) = deploy_client(&env);
        init_contract(&client, &env, &admin, &sme);

        // Seed a valid commitment so the regression is observable.
        client.record_sme_collateral_commitment(
            &symbol_short(&env, "USDC"),
            &1_000_000_000i128,
        );
        let before = client.get_collateral_config();

        // Attempt an invalid commitment (negative amount).
        let result = client.try_record_sme_collateral_commitment(
            &symbol_short(&env, "USDC"),
            &-1i128,
        );
        assert!(result.is_error(), "expected negative commitment to be rejected");

        let after = client.get_collateral_config();
        assert_eq(before, after);
    }

    // --------------------------------------------------------------------------
    // Recovery: after a failure, a subsequent valid mutation must succeed
    // --------------------------------------------------------------------------

    #[test]
    fn recovery_after_failed_limit_update_succeeds() {
        let env = Env::default();
        let (client, admin, sme) = deploy_client(&env);
        init_contract(&client, &env, &admin, &sme);

        let _failed = client.try_set_collateral_limit(&-1i128);
        client.set_collateral_limit(&7_000_000_000i128);

        assert_eq(
            client.get_collateral_config().collateral_limit,
            7_000_000_000i128,
        );
    }

    #[test]
    fn recovery_after_failed_commitment_succeeds() {
        let env = Env::default();
        let (client, admin, sme) = deploy_client(&env);
        init_contract(&client, &env, &admin, &sme);

        let _failed = client.try_record_sme_collateral_commitment(
            &symbol_short(&env, "USDC"),
            &-1i128,
        );
        client.record_sme_collateral_commitment(
            &symbol_short(&env, "USDC"),
            &3_000_000_000i128,
        );

        match client.get_collateral_config().sme_commitment {
            CollateralCommitmentSnapshot::Some(c) => assert_eq(c.amount, 3_000_000_000i128),
            CollateralCommitmentSnapshot::None => panic("expected Some commitment"),
        }
    }

    // --------------------------------------------------------------------------
    // Concurrency / idempotency: repeated reads never mutate state
    // --------------------------------------------------------------------------

    #[test]
    fn repeated_reads_are_pure() {
        let env = Env::default();
        let (client, admin, sme) = deploy_client(&env);
        init_contract(&client, &env, &admin, &sme);

        client.set_collateral_limit(&99_000_000_000i128);
        client.record_sme_collateral_commitment(
            &symbol_short(&env, "USDC"),
            &5_000_000_000i128,
        );

        let expected = client.get_collateral_config();
        for _ in 0..10 {
            assert_eq(client.get_collateral_config(), expected.clone());
        }
    }

    // --------------------------------------------------------------------------
    // Helpers
    // --------------------------------------------------------------------------

    /// Lifts the individual commitment getter result into the composite
    /// `CollateralCommitmentSnapshot` representation so the two read shapes
    /// can be compared directly.
    fn lift_commitment(
        value: Option<crate::SmeCollateralCommitment>,
    ) -> CollateralCommitmentSnapshot {
        match value {
            Some(c) => CollateralCommitmentSnapshot::Some(c),
            None => CollateralCommitmentSnapshot::None,
        }
    }
}
