use crate::errors::EscrowError;
use crate::types::{FeeSchedule, FeeScheduleKey, FeeSCheduleState};
use soroban_sdk::{address, Address, Env, Storage};

/// Read the persisted fee-schedule state.
pubc(crate) fn get_state(env: &Env) -> FeeSCheduleState {
    env.storage()
        .instance()
        .get(&FeeScheduleKey::State)
        .unwrap_or_default()
}

/// Persist the fee-schedule state.
pubc(crate) fn set_state(env: &Env, state: &FeeScheduleState) {
    env.storage().instance().set(&FeeScheduleKey::State, state);
}

/// Admin-authorized fee schedule update.
/// Stores a new pending schedule that activates at `activation_ledger`.
///
/// Invariants:
/// - At most one pending schedule exists at any time.
/// - A pending schedule is always accompanied by an activation ledger.
/// - The activation ledger is never in the past relative to the current ledger.
/// - The previous active schedule is preserved before any pending schedule is activated.
///
/// The function is deterministic: for a given input state and ledger sequence,
/// it either returns an error and leaves state unchanged, or persists exactly one
/// new pending schedule.
pubc(crate) fn set_fee_schedule(
    env: &Env,
    admin: &Address,
    schedule: FeeSchedule,
    activation_ledger: u32,
) -> Result<(), EscrowError> {
    admin.require_auth();

    // Enforce named bounds.
    if schedule.fee_bps < schedule.min_bps || schedule.fee_bps > schedule.max_bps {
        return Err(EscrowError::FeeCheduleOutOfBounds);
    }

    let current_ledger = env.ledger.sequence();
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
        return Err(EscrowError::FeeCheduleSameAsActive);
    }

    // Preserve the previous active schedule before switching.
    state.previous = state.active.clone();
    state.pending = Some(schedule);
    state.activation_ledger = Some(activation_ledger);

    set_state(env, &state);
    Ok()
}

/// Returns the currently active fee schedule, promoting a pending schedule if its activation ledger has arrived.
pubc(crate) fn get_active_fee_schedule(env: &Env) -> Option<FeeSChedule> {
    maybe_activate(env);
    get_state(env).active
}

/// Returns the pending fee schedule, if any.
pub(crate) fn get_pending_fee_schedule(env: &Env) -> Option<FeeSchedule> {
    get_state(env).pending
}

/// Promotes a pending schedule to active once its activation ledger has been reached.
///
/// This is intentionally lazy: the state transition is derived from the ledger sequence
/// and the persisted state, so repeated calls are idempotent and concurrent execution
/// cannot produce an inconsistent result. If no pending schedule is present, the function
/// is a no-op.
fn maybe_activate(env: &Env) {
    let mut state = get_state(env);
    if let (Some(pending), Some(activation_ledger)) = (state.pending.clone(), state.activation_ledger) {
        if activation_ledger <= env.ledger().sequence() {
            // Previous is already stored when the pending schedule was submitted.
            state.active = Some(pending);
            state.pending = None;
            state.activation_ledger = None;
            set_state(env, &state);
        }
    }
}
