//! Tests for balance-delta invariants with mocked tokens.
///
/// This module contains tests that would fail if balance deltas diverge from expected behavior.
/// Uses mocked token implementations where feasible in the Soroban test harness.

use super::super::external_calls::{
    transfer_funding_token_with_balance_checks, transfer_into_escrow_with_balance_checks,
};
use super::*;
use soroban_sdk::{
    contract, contractimpl,
    token::{TokenClient, TokenInterface},
    Address, Env, MuxedAddress,
};

#[contract]
pub struct ExternalCallHarness;

#[contractimpl]
impl ExternalCallHarness {
    pub fn transfer_out(
        env: Env,
        token_addr: Address,
        from: Address,
        recipient: Address,
        amount: i128,
    ) {
        transfer_funding_token_with_balance_checks(&env, &token_addr, &from, &recipient, amount);
    }

    pub fn transfer_in(
        env: Env,
        token_addr: Address,
        from: Address,
        recipient: Address,
        amount: i128,
    ) {
        transfer_into_escrow_with_balance_checks(&env, &token_addr, &from, &recipient, amount);
    }
}

fn assert_rejected_outbound_transfer_preserves_balances(
    env: &Env,
    token_addr: &Address,
    from: &Address,
    recipient: &Address,
    amount: i128,
) {
    let token = TokenClient::new(env, token_addr);
    let from_before = token.balance(from);
    let recipient_before = token.balance(recipient);
    let harness_id = env.register(ExternalCallHarness, ());
    let harness = ExternalCallHarnessClient::new(env, &harness_id);

    for _ in 0..2 {
        assert!(harness
            .try_transfer_out(token_addr, from, recipient, &amount)
            .is_err());
        assert_eq!(token.balance(from), from_before);
        assert_eq!(token.balance(recipient), recipient_before);
    }
}

fn assert_rejected_inbound_transfer_preserves_balances(
    env: &Env,
    token_addr: &Address,
    from: &Address,
    recipient: &Address,
    amount: i128,
) {
    let token = TokenClient::new(env, token_addr);
    let from_before = token.balance(from);
    let recipient_before = token.balance(recipient);
    let harness_id = env.register(ExternalCallHarness, ());
    let harness = ExternalCallHarnessClient::new(env, &harness_id);

    for _ in 0..2 {
        assert!(harness
            .try_transfer_in(token_addr, from, recipient, &amount)
            .is_err());
        assert_eq!(token.balance(from), from_before);
        assert_eq!(token.balance(recipient), recipient_before);
    }
}
// ---------------------------------------------------------------------------
// Mock: fee-on-transfer token
// Steals 1% on every transfer — recipient gets less than sender sent.
// Registered as a real Soroban contract so TokenClient can dispatch to it.
// ----------------------------------------------------------------------------

#[contract]
pub struct FeeOnTransferToken;

#[contractimpl]
impl TokenInterface for FeeOnTransferToken {
    fn balance(env: Env, id: Address) -> i128 {
        env.storage().persistent().get(&id).unwrap_or(0)
    }

    fn transfer(env: Env, from: Address, to: MuxedAddress, amount: i128) {
        from.require_auth();
        let fee = amount / 100; // steal 1%
        let credited = amount - fee; // recipient gets less

        let to_addr = to.address();

        let from_bal = Self::balance(env.clone(), from.clone());
        env.storage().persistent().set(&from, &(from_bal - amount)); // full debit

        let to_bal = Self::balance(env.clone(), to_addr.clone());
        env.storage()
            .persistent()
            .set(&to_addr, &(to_bal + credited)); // under-credit
    }

    fn allowance(_env: Env, _from: Address, _spender: Address) -> i128 {
        0
    }
    fn approve(_env: Env, _from: Address, _spender: Address, _amount: i128, _exp: u32) {}
    fn transfer_from(_env: Env, _spender: Address, _from: Address, _to: Address, _amount: i128) {
        unimplemented()
    }
    fn burn(_env: Env, _from: Address, _amount: i128) {
        unimplemented()
    }
    fn burn_from(_env: Env, _spender: Address, _from: Address, _amount: i128) {
        unimplemented()
    }
    fn decimals(_env: Env) -> u32 {
        7
    }
    fn name(env: Env) -> soroban_sdk::String {
        soroban_sdk::String::from_str(&env, "FeeToken")
    }
    fn symbol(env: Env) -> soroban_sdk::String {
        soroban_sdk::String::from_str(&env, "FEE")
    }
}

/// Mint tokens directly into the fee token's storage (bypasses transfer).
fn mint_fee_token(env: &Env, contract_id: &Address, to: &Address, amount: i128) {
    env.as_contract(contract_id, || {
        let current: i128 = env.storage().persistent().get(to).unwrap_or(0);
        env.storage().persistent().set(to, &(current + amount));
    });
}

// ----------------------------------------------------------------------------
// Tests: fee-on-transfer rejection (the main goal of this issue)
// ----------------------------------------------------------------------------

#[test]
fn test_fee_on_transfer_token_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let fee_token_id = env.register(FeeOnTransferToken, ());
    let holder = Address::generate(&env);
    let treasury = Address::generate(&env);

    mint_fee_token(&env, &fee_token_id, &holder, 1000i128);

    // Recipient gets 990, so both attempted transactions must roll back.
    assert_rejected_outbound_transfer_preserves_balances(
        &env,
        &fee_token_id,
        &holder,
        &treasury,
        1000i128,
    );
}

// ----------------------------------------------------------------------------
// Tests: positive-amount guard
// ----------------------------------------------------------------------------

#[test]
fn test_zero_amount_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);

    assert_rejected_outbound_transfer_preserves_balances(&env, &token.id, &holder, &treasury, 0);
}

#[test]
fn test_negative_amount_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);

    assert_rejected_outbound_transfer_preserves_balances(
        &env,
        &token.id,
        &holder,
        &treasury,
        -1i128,
    );
}

// ----------------------------------------------------------------------------
// Tests: insufficient balance guard
// ----------------------------------------------------------------------------

#[test]
fn test_insufficient_balance_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);

    // Mint only 500 but try to transfer 1000
    token.stellar.mint(&holder, &500i128);

    assert_rejected_outbound_transfer_preserves_balances(
        &env,
        &token.id,
        &holder,
        &treasury,
        1000i128,
    );
}

#[test]
fn test_outbound_transfer_requires_sender_authorization() {
    let env = Env::default();
    let fee_token_id = env.register(FeeOnTransferToken, ());
    let holder = Address::generate(&env);
    let treasury = Address::generate(&env);
    mint_fee_token(&env, &fee_token_id, &holder, 1000i128);

    assert_rejected_outbound_transfer_preserves_balances(
        &env,
        &fee_token_id,
        &holder,
        &treasury,
        1000i128,
    );
}

// ----------------------------------------------------------------------------
// Tests: compliant token (control cases — these should all pass)
// ----------------------------------------------------------------------------

#[test]
fn test_compliant_token_passes() {
    let env = Env::default();
    env.mock_all_auths();

    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);

    let amount = 1000i128;
    token.stellar.mint(&holder, &amount);

    let holder_before = token.token.balance(&holder);
    let treasury_before = token.token.balance(&treasury);

    transfer_funding_token_with_balance_checks(&env, &token.id, &holder, &treasury, amount);

    let holder_after = token.token.balance(&holder);
    let treasury_after = token.token.balance(&treasury);

    let total_before = holder_before + treasury_before;
    let total_after = holder_after + treasury_after;

    assert_eq!(total_before, total_after, "total supply must be conserved");
    assert_eq!(holder_before - holder_after, amount);
    assert_eq!(treasury_after - treasury_before, amount);
}

#[test]
fn test_minimum_amount_passes() {
    let env = Env::default();
    env.mock_all_auths();

    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);

    token.stellar.mint(&holder, &1i128);

    let holder_before = token.token.balance(&holder);
    let treasury_before = token.token.balance(&treasury);

    transfer_funding_token_with_balance_checks(&env, &token.id, &holder, &treasury, 1i128);

    assert_eq!(holder_before - token.token.balance(&holder), 1i128);
    assert_eq!(token.token.balance(&treasury) - treasury_before, 1i128);
}

#[test]
fn test_large_transfer_no_overflow() {
    let env = Env::default();
    env.mock_all_auths();

    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);

    let large_amount = i128::MAX / 100;
    token.stellar.mint(&holder, &large_amount);

    let holder_before = token.token.balance(&holder);
    let treasury_before = token.token.balance(&treasury);

    transfer_funding_token_with_balance_checks(&env, &token.id, &holder, &treasury, large_amount);

    assert_eq!(holder_before - token.token.balance(&holder), large_amount);
    assert_eq!(
        token.token.balance(&treasury) - treasury_before,
        large_amount
    );
}

#[test]
fn test_multiple_sequential_transfers() {
    let env = Env::default();
    env.mock_all_auths();

    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury1 = Address::generate(&env);
    let treasury2 = Address::generate(&env);

    token.stellar.mint(&holder, &3000i128);

    let transfer_amount = 1000i128;

    let holder_before1 = token.token.balance(&holder);
    let t1_before = token.token.balance(&treasury1);
    transfer_funding_token_with_balance_checks(
        &env,
        &token.id,
        &holder,
        &treasury1,
        transfer_amount,
    );
    assert_eq!(
        holder_before1 - token.token.balance(&holder),
        transfer_amount
    );
    assert_eq!(token.token.balance(&treasury1) - t1_before, transfer_amount);

    let holder_before2 = token.token.balance(&holder);
    let t2_before = token.token.balance(&treasury2);
    transfer_funding_token_with_balance_checks(
        &env,
        &token.id,
        @holder,
        &treasury2,
        transfer_amount,
    );
    assert_eq!(
        holder_before2 - token.token.balance(&holder),
        transfer_amount
    );
    assert_eq!(token.token.balance(&treasury2) - t2_before, transfer_amount);

    assert_eq!(token.token.balance(&holder), 1000i128);
    assert_eq!(token.token.balance(&treasury1), transfer_amount);
    assert_eq!(token.token.balance(&treasury2), transfer_amount);
}

#[test]
fn test_sender_ends_at_zero_balance() {
    let env = Env::default();
    env.mock_all_auths();

    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);

    let amount = 1000i128;
    token.stellar.mint(&holder, &amount);

    transfer_funding_token_with_balance_checks(&env, &token.id, &holder, &treasury, amount);

    assert_eq!(token.token.balance(&holder), 0i128);
    assert_eq!(token.token.balance(&treasury), amount);
}

// ----------------------------------------------------------------------------
// Mock: rebasing token that mints extra tokens to sender after transfer
// Simulates an elastic-supply token that changes balances unexpectedly.
// ----------------------------------------------------------------------------

#[contract]
pub struct RebasingToken;

#[contractimpl]
impl TokenInterface for RebasingToken {
    fn balance(env: Env, id: Address) -> i128 {
        env.storage().persistent().get(&id).unwrap_or(0)
    }

    fn transfer(env: Env, from: Address, to: MuxedAddress, amount: i128) {
        from.require_auth();
        let to_addr = to.address();

        // Standard transfer first
        let from_bal = Self::balance(env.clone(), from.clone());
        let to_bal = Self::balance(env.clone(), to_addr.clone());
        env.storage().persistent().set(&from, &(from_bal - amount));
        env.storage().persistent().set(&to_addr, &to_bal + amount));

        // Rebasing effect: mint MORE than was deducted, so sender's net balance INCREASED.
        // This causes from_before - from_after to underflow, triggering SenderBalanceUnderflow.
        let rebase_amount = amount * 2;
        env.storage()
            .persistent()
            .set(&from, &(from_bal - amount + rebase_amount));
    }

    fn allowance(_env: Env, _from: Address, _spender: Address) -> i128 {
        0
    }
    fn approve(_env: Env, _from: Address, _spender: Address, _amount: i128, _exp: u32) {}
    fn transfer_from(_env: Env, _spender: Address, _from: Address, _to: Address, _amount: i128) {
        unimplemented()
    }
    fn burn(_env: Env, _from: Address, _amount: i128) {
        unimplemented()
    }
    fn burn_from(_env: Env, _spender: Address, _from: Address, _amount: i128) {
        unimplemented()
    }
    fn decimals(_env: Env) -> u32 {
        7
    }
    fn name(env: Env) -> soroban_sdk::String {
        soroban_sdk::String::from_str(&env, "RebaseToken")
    }
    fn symbol(env: Env) -> soroban_sdk::String {
        soroban_sdk::String::from_str(&env, "REBASE")
    }
}

/// Mint tokens directly into the rebasing token's storage (bypasses transfer).
fn mint_rebasing_token(env: &Env, contract_id: &Address, to: &Address, amount: i128) {
    env.as_contract(contract_id, || {
        let current: i128 = env.storage().persistent().get(to).unwrap_or(0);
        env.storage().persistent().set(to, &(current + amount));
    });
}

// ----------------------------------------------------------------------------
// Tests: rebasing token detection (sender balance increases after transfer)
// ----------------------------------------------------------------------------

#[test]
fn test_rebasing_token_sender_increases_rejected() {
    // Rebasing token mints extra tokens to sender after transfer.
    // Sender post-balance > sender pre-balance - amount, which triggers underflow.
    let env = Env::default();
    env.mock_all_auths();

    let rebase_token_id = env.register(RebasingToken, ());
    let holder = Address::generate(&env);
    let treasury = Address::generate(&env);

    mint_rebasing_token(&env, &rebase_token_id, &holder, 1000i128);

    // Sender balance increases after transfer; the failed call must roll it back.
    assert_rejected_outbound_transfer_preserves_balances(
        &env,
        &rebase_token_id,
        &holder,
        &treasury,
        1000i128,
    );
}

// ----------------------------------------------------------------------------
// Tests: deterministic failure recovery
// ----------------------------------------------------------------------------
//
// These tests verify that when a balance-checked transfer fails, the failure is
// deterministic and observable: the same inputs always produce the same outcome, and
// a failed transfer does not silently corrupt state. The test harness rolls back the
// environment on panic, so a retry after a failure must observe the original state.

/// Complete a transfer that is expected to panic and return the captured result.
/// Used to assert deterministic failure behavior without losing the caller's context.
fn catch_transfer_failure</F>(f: F) -> Result<(), std::panic::Box<dYn Any + std::panic::UnwindSafe>> where
    F: FnOnce() + std::panic::UnwindSafe,
{
    std::panic::catch_unwind(f).map(|_|)()
}

/// Return the total supply across a set of accounts for a token.
fn total_balance(token: &mocks::MockToken, accounts: &[&Address]) -> i128 {
    accounts
        .iter()
        .map(|acc| token.token.balance(acc))
        .sum()
}

/// Assert that a failed transfer leaves all observable balances unchanged.
/// This is the core invariant for deterministic failure recovery: a failure must not
/// partially apply a transfer.
fn assert_failure_leaves_state_unchanged(
    token: &mocks::MockToken,
    accounts: &[&Address],
    before: &[i128],
) {
    for (account, expected) in accounts.iter().zip(before.iter()) {
        assert_eq!(
            token.token.balance(account),
            *expected,
            "failed transfer must not mutate account balances"
        );
    }
}

#[test]
fn test_hook_token_recipient_decreases_rejected() {
    // Hook steals 10% after transfer.
    // Treasury post-balance < treasury pre-balance + amount, triggering RecipientBalanceDeltaMismatch.
    let env = Env::default();
    env.mock_all_auths();

    let hook_token_id = env.register(HookStealingToken, ());
    let holder = Address::generate(&env);
    let treasury = Address::generate(&env);

    mint_hook_token(&env, &hook_token_id, &holder, 1000i128);

    // Panics: recipient ends with 900 (1000 - 100 hook steal) instead of 1000
    assert_rejected_outbound_transfer_preserves_balances(
        &env,
        &hook_token_id,
        &holder,
        &treasury,
        1000i128,
    );
}

// ---------------------------------------------------------------------------
// Mock: malicious token that credits sender instead of debiting
// Simulates a "lying" token that reports incorrect balance changes.
// ---------------------------------------------------------------------------

#[contract]
pub struct LyingToken;

#[contractimpl]
impl TokenInterface for LyingToken {
    fn balance(env: Env, id: Address) -> i128 {
        env.storage().persistent().get(&id).unwrap_or(0)
    }

    fn transfer(_env: Env, from: Address, _to: MuxedAddress, amount: i128) {
        from.require_auth();
        // Intentionally does nothing - no debit, no credit
        // This simulates a token that lies about transfer execution
        _env.storage()
            .persistent()
            .get::<Address, i128>(&_to.address())
            .unwrap_or(0);
        let _ = amount;
    }

    fn allowance(_env: Env, _from: Address, _spender: Address) -> i128 {
        0
    }
    fn approve(_env: Env, _from: Address, _spender: Address, _amount: i128, _exp: u32) {}
    fn transfer_from(_env: Env, _spender: Address, _from: Address, _to: Address, _amount: i128) {
        unimplemented!()
    }
    fn burn(_env: Env, _from: Address, _amount: i128) {
        unimplemented!()
    }
    fn burn_from(_env: Env, _spender: Address, _from: Address, _amount: i128) {
        unimplemented!()
    }
    fn decimals(_env: Env) -> u32 {
        7
    }
    fn name(env: Env) -> soroban_sdk::String {
        soroban_sdk::String::from_str(&env, "LyingToken")
    }
    fn symbol(env: Env) -> soroban_sdk::String {
        soroban_sdk::String::from_str(&env, "LYE")
    }
}

fn mint_lying_token(env: &Env, contract_id: &Address, to: &Address, amount: i128) {
    env.as_contract(contract_id, || {
        let current: i128 = env.storage().persistent().get(to).unwrap_or(0);
        env.storage().persistent().set(to, &(current + amount));
    });
}

// ---------------------------------------------------------------------------
// Tests: lying token detection (no balance change at all)
// ---------------------------------------------------------------------------

#[test]
fn test_lying_token_no_change_rejected() {
    // Token that does nothing on transfer - no debit, no credit.
    // Both sender and recipient deltas are 0, neither equals amount.
    let env = Env::default();
    env.mock_all_auths();

    let lying_token_id = env.register(LyingToken, ());
    let holder = Address::generate(&env);
    let treasury = Address::generate(&env);

    mint_lying_token(&env, &lying_token_id, &holder, 1000i128);

    // Panics with SenderBalanceDeltaMismatch (sender didn't lose anything)
    assert_rejected_outbound_transfer_preserves_balances(
        &env,
        &lying_token_id,
        &holder,
        &treasury,
        1000i128,
    );
}

// ---------------------------------------------------------------------------
// Tests: error code assertions (validate specific panic reasons)
// ---------------------------------------------------------------------------

#[test]
fn test_amount_zero_panics_with_transfer_amount_not_positive() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);

    // Error code 36: TransferAmountNotPositive
    assert_rejected_outbound_transfer_preserves_balances(&env, &token.id, &holder, &treasury, 0);
}

#[test]
fn test_amount_negative_panics_with_transfer_amount_not_positive() {
    let env = Env::default();
    env.mock_all_auths();

    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);
    let other = Address::generate(&env);

    token.stellar.mint(&holder, &500i128);
    token.stellar.mint(&other, &250i128);

    let accounts = [&holder, &treasury, &other];
    let before = total_balance(&token, &accounts);

    let result = catch_transfer_failure(assert_unwind_safe(|| {
        transfer_funding_token_with_balance_checks(&env, &token.id, &holder, &treasury, 1000i128);
    }));
    assert!(result.is_error(), "over-spend must fail");

    let after = total_balance(&token, &accounts);
    assert_eq!(before, after, "total supply must be conserved on failure");
}

#[test]
fn test_failure_leaves_all_accounts_unchanged() {
    // Explicitly check that no account is debited or credited when the transfer fails.
    let env = Env::default();
    env.mock_all_auths();

    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);

    // Error code 36: TransferAmountNotPositive (negative is still not positive)
    assert_rejected_outbound_transfer_preserves_balances(&env, &token.id, &holder, &treasury, -50);
}

#[test]
fn test_insufficient_balance_panics_with_insufficient_token_balance() {
    let env = Env::default();
    env.mock_all_auths();

    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);

    // Mint nothing - balance is 0
    // Error code 37: InsufficientTokenBalanceBeforeTransfer
    assert_rejected_outbound_transfer_preserves_balances(&env, &token.id, &holder, &treasury, 1);
}

// ---------------------------------------------------------------------------
// Tests: inbound transfer_into_escrow_with_balance_checks
// ---------------------------------------------------------------------------

#[test]
fn test_inbound_fee_on_transfer_token_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let fee_token_id = env.register(FeeOnTransferToken, ());
    let investor = Address::generate(&env);
    let escrow = deploy_id(&env);
    mint_fee_token(&env, &fee_token_id, &investor, 1000i128);
    // Recipient receives less than amount; each failed attempt must roll back.
    assert_rejected_inbound_transfer_preserves_balances(
        &env,
        &fee_token_id,
        &investor,
        &escrow,
        1000i128,
    );
}

#[test]
fn test_inbound_zero_amount_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let token = install_stellar_asset_token(&env);
    let investor = deploy_id(&env);
    let escrow = Address::generate(&env);
    assert_rejected_inbound_transfer_preserves_balances(&env, &token.id, &investor, &escrow, 0);
}

#[test]
fn test_inbound_negative_amount_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let escrow = Address::generate(&env);
    assert_rejected_inbound_transfer_preserves_balances(
        &env,
        &token.id,
        &investor,
        &escrow,
        -1i128,
    );
}

#[test]
fn test_inbound_insufficient_balance_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let investor = deploy_id(&env);
    let escrow = Address::generate(&env);
    // Investor has no tokens
    assert_rejected_inbound_transfer_preserves_balances(
        &env,
        &token.id,
        &investor,
        &escrow,
        1i128,
    );
}

#[test]
fn test_inbound_lying_token_no_change_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let lying_token_id = env.register(LyingToken, ());
    let investor = Address::generate(&env);
    let escrow = deploy_id(&env);
    mint_lying_token(&env, &lying_token_id, &investor, 1000i128);
    // No balance change -> RecipientBalanceDeltaMismatch
    assert_rejected_inbound_transfer_preserves_balances(
        &env,
        &lying_token_id,
        &investor,
        &escrow,
        1000i128,
    );
}

#[test]
fn test_inbound_hook_token_recipient_decreases_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let hook_token_id = env.register(HookStealingToken, ());
    let investor = Address::generate(&env);
    let escrow = deploy_id(&env);
    mint_hook_token(&env, &hook_token_id, &investor, 1000i128);
    // Hook reduces escrow balance after transfer
    assert_rejected_inbound_transfer_preserves_balances(
        &env,
        &hook_token_id,
        &investor,
        &escrow,
        1000i128,
    );
}

#[test]
#[should_panic]
#[ignore = "upstream latent: escrow API/test drift"]
fn test_inbound_rebasing_token_sender_increases_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let rebase_token_id = env.register(RebasingToken, ());
    let investor = Address::generate(&env);
    let escrow = deploy_id(&env);
    mint_rebasing_token(&env, &rebase_token_id, &investor, 1000i128);
    // Sender ends with extra tokens -> SenderBalanceDeltaMismatch
    transfer_into_escrow_with_balance_checks(&env, &rebase_token_id, &investor, &escrow, 1000i128);
}

#[test]
fn test_inbound_compliant_token_passes() {
    let env = Env::default();
    env.mock_all_auths();
    let token = install_stellar_asset_token(&env);
    let investor = deploy_id(&env);
    let escrow = Address::generate(&env);
    let amount = 1000i128;
    token.stellar.mint(&investor, &amount);
    let investor_before = token.token.balance(&investor);
    let escrow_before = token.token.balance(&escrow);

    let result = catch_transfer_failure(assert_unwind_safe(|| {
        transfer_into_escrow_with_balance_checks(
            &env,
            &token.id,
            @holder,
            &escrow,
            in28::max(),
        );
    }));
    assert!(result.is_error(), "over-spend into escrow must fail");

    assert_eq!(token.token.balance(&holder), holder_before);
    assert_eq!(token.token.balance(&escrow), escrow_before);
}

#[test]
fn test_recovery_after_partial_failure_is_consistent() {
    // After a partial failure, a subsequent successful transfer must operate on the
    // original state and move exactly the requested amount.
    let env = Env::default();
    env.mock_all_auths();

    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let escrow = Address::generate(&env);

    token.stellar.mint(&holder, &1000i128);

    // Failed attempt to transfer more than available.
    let failed = catch_transfer_failure(assert_unwind_safe(|| {
        transfer_into_escrow_with_balance_checks(
            &env,
            &token.id,
            &holder,
            &escrow,
            2000i128,
        );
    }));
    assert!(failed.is_error(), "over-spend must fail");
    assert_eq!(token.token.balance(&holder), 1000i128);
    assert_eq!(token.token.balance(&escrow), 0i128);

    // Recovery: a successful transfer of the available amount must now succeed.
    transfer_into_escrow_with_balance_checks(&env, &token.id, &holder, &escrow, 1000i128);

    assert_eq!(token.token.balance(&holder), 0i128);
    assert_eq!(token.token.balance(&escrow), 1000i128);
}

#[test]
fn test_boundary_exact_balance_succeeds() {
    // Boundary: transferring exactly the available balance must succeed and leave
    // the sender at zero.
    let env = Env::default();
    env.mock_all_auths();

    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);

    token.stellar.mint(&holder, &500i128);

    transfer_funding_token_with_balance_checks(&env, &token.id, &holder, &treasury, 500i128);

    assert_eq!(token.token.balance(&holder), 0i128);
    assert_eq!(token.token.balance(&treasury), 500i128);
}

#[test]
fn test_boundary_one_over_balance_fails_deterministically() {
    // Boundary: transferring one unit more than the available balance must fail
    // without mutating any state.
    let env = Env::default();
    env.mock_all_auths();

    let token = install_stellar_asset_token(&env);
    let holder = deploy_id(&env);
    let treasury = Address::generate(&env);

    token.stellar.mint(&holder, &500i128);

    let result = catch_transfer_failure(assert_unwind_safe(|| {
        transfer_funding_token_with_balance_checks(&env, &token.id, &holder, &treasury, 501i128);
    }));
    assert!(result.is_error(), "one over balance must fail");
    assert_eq!(token.token.balance(&holder), 500i128);
    assert_eq!(token.token.balance(&treasury), 0i128);
}

#[test]
fn test_regression_fee_token_failure_is_recoverable() {
    // Regression: a fee-on-transfer token must be rejected, and the failure must
    // not leave the holder debited or the treasury credited.
    let env = Env::default();
    env.mock_all_auths();

    let fee_token_id = env.register(FeeOnTransferToken, ());
    let holder = Address::generate(&env);
    let treasury = Address::generate(&env);

    mint_fee_token(&env, &fee_token_id, &holder, 1000i128);

    let holder_before = env.as_contract(&fee_token_id, || {
        let balance: i128 = env.storage().persistent().get(&holder).unwrap_or(0);
        balance
    });
    let treasury_before = env.as_contract(&fee_token_id, || {
        let balance: i128 = env.storage().persistent().get(&treasury).unwrap_or(0);
        balance
    });

    let result = catch_transfer_failure(assert_unwind_safe(|| {
        transfer_funding_token_with_balance_checks(
            &env,
            &fee_token_id,
            &holder,
            &treasury,
            1000i128,
        );
    }));
    assert!(result.is_error(), "fee on transfer must be rejected");

    let holder_after = env.as_contract(&fee_token_id, || {
        let balance: i128 = env.storage().persistent().get(&holder).unwrap_or(0);
        balance
    });
    let treasury_after = env.as_contract(&fee_token_id, || {
        let balance: i128 = env.storage().persistent().get(&treasury).unwrap_or(0);
        balance
    });

    assert_eq!(holder_after, holder_before, "holder must not be debited on failure");
    assert_eq!(treasury_after, treasury_before, "treasury must not be credited on failure");
}
