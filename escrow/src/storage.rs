use crate::errors::EscrowError;
use crate::types::{FeeSchedule, FeeScheduleKey, FeeScheduleState};
use soroban_sdk::{Address, Env, Storage};

/// Reads the persisted fee schedule state, defaulting to empty on first use.
pub(crate) fn get_state(env: &Env) -> FeeScheduleState {
    env.storage()
        .instance()
        .get(&FeeScheduleKey::State)
        .unwrap_or_default()
}

/// Persists the fee schedule state atomically as a single instance entry.
pub(crate) fn set_state(env: &Env, state: &FeeScheduleState) {
    env.storage().instance().set(&FeeScheduleKey::State, state);
}

/// Admin-authorized fee schedule update.
/// Stores a new pending schedule that activates at `activation_ledger`.
pub(crate) fn set_fee_schedule(
    env: &Env,
    admin: &Address,
    schedule: FeeSchedule,
    activation_ledger: u32,
) -> Result<(), EscrowError> {
    admin.require_auth();

    // Enforce named bounds.
    if schedule.fee_bps < schedule.min_bps || schedule.fee_bps > schedule.max_bps {
        return Err(EscrowError::FeeScheduleOutOfBounds);
    }

    let current_ledger = env.ledger().sequence();
    if activation_ledger < current_ledger {
        return Err(EscrowError::FeeScheduleInvalidActivation);
    }

    let mut state = get_state(env);

    // Reject if a pending schedule already exists.
    if state.pending.is_some() {
        return Err(EscrowError::FeeScheduleAlreadyPending);
    }

    // Reject duplicate submission of the active schedule.
    if state.active.as_ref() == Some(&schedule) {
        return Err(EscrowError::FeeScheduleSameAsActive);
    }

    // Preserve the previous active schedule before switching.
    state.previous = state.active.clone();
    state.pending = Some(schedule);
    state.activation_ledger = Some(activation_ledger);

    set_state(env, &state);
    Ok(())
}

/// Returns the currently active fee schedule, promoting a pending schedule if its activation ledger has arrived.
pub(crate) fn get_active_fee_schedule(env: &Env) -> Option<FeeSchedule> {
    maybe_activate(env);
    get_state(env).active
}

/// Returns the pending fee schedule, if any.
pub(crate) fn get_pending_fee_schedule(env: &Env) -> Option<FeeSchedule> {
    get_state(env).pending
}

/// Deterministically promotes a pending schedule to active once its activation
/// ledger has been reached. The promotion is idempotent: repeated calls after
/// activation observe `pending == None` and perform no further writes, so
/// retries, partial failures, and concurrent invocations cannot double-apply
/// or lose the previously active schedule.
fn maybe_activate(env: &Env) {
    let mut state = get_state(env);
    if let (Some(pending), Some(activation_ledger)) = (state.pending.clone(), state.activation_ledger) {
        if activation_ledger <= env.ledger().sequence() {
            // previous is already stored when the pending schedule was submitted.
            state.active = Some(pending);
            state.pending = None;
            state.activation_ledger = None;
            set_state(env, &state);
        }
    }
}

/// Test-only helper that exposes the raw persisted state for assertions.
/// Kept behind `cfg(test)` so production builds cannot observe or mutate
/// internal state outside the authorized entry points above.
#[cfg(test)]
pub(crate) fn peek_state(env: &Env) -> FeeScheduleState {
    get_state(env)
}

/// Test-only helper that forces activation at the current ledger without
/// going through `get_active_fee_schedule`, allowing tests to exercise the
/// promotion path in isolation and verify idempotency across repeated calls.
#[cfg(test)]
pub(crate) fn force_activate(env: &Env) {
    maybe_activate(env);
}
