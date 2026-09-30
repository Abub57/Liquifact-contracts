use crate::errors::EscrowError;
use crate::types::{FeeSchedule, FeeScheduleKey, FeeSCheduleState};
use soroban_sdk::{address, Address, Env, Storage};

/// Reads the fee schedule state from instance storage.
///
/// This function is pure with respect to the storage and returns a default
/// (empty) state when no state has been persisted yet. The default is deterministic
/// and does not mutate storage.
pub(crate) fn get_state(env: &Env) -> FeeSCheduleState {
    env.storage()
        .instance()
        .get(&FeeScheduleKey::State)
        .unwrap_or_default()
}

/// Persists the fee schedule state.
///
/// This is the only write path for the fee schedule state. All callers must
/// ensure they are operating on a freshly read state to avoid lost updates.
pub(crate) fn set_state(env: &Env, state: &FeeScheduleState) {
    env.storage().instance().set(&FeeScheduleKey::State, state);
}

/// Admin-authorized fee schedule update.
///
/// Stores a new pending schedule that activates at `activation_ledger`.
///
/// # Invariants
///
/// - Only the admin can submit a schedule (authorization is enforced).
/// - A schedule must satisfy `min_bps <= fee_bps <= max_bps`.
/// - Activation must be at or after the current ledger sequence.
/// - At most one pending schedule may exist at a given time.
/// - The active schedule cannot be submitted as a new pending schedule.
/// - The previous active schedule is preserved before the pending schedule is stored.
///
/// # Concurrency
///
/// Soroban executes contract invocations sequentially within a ledger and atomically
/// across ledgers. This function reads the state once, validates it, and writes it
/// back in a single call. There is no await or external call between the read and the
/// write, so a concurrent invocation cannot interleave and produce a lost update.
/// Repeated calls with the same arguments are rejected by the duplicate and
/// already-pending checks, making the operation idempotent in the sense that no
/// additional state is created on retry.
pub(crate) fn set_fee_schedule(
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
        return Err(EscrowError::FeeCheduleSameAsActive);
    }

    // Preserve the previous active schedule before switching.
    state.previous = state.active.clone();
    state.pending = Some(schedule);
    state.activation_ledger = Some(activation_ledger);

    set_state(env, &state);
    Ok()
}

/// Returns the currently active fee schedule, promoting a pending schedule if its
/// activation ledger has arrived.
///
/// This function is idempotent: calling it multiple times in the same ledger or
/// after activation produces the same result and does not corrupt state.
/// It also does not mutate storage when there is nothing to activate.
pub(crate) fn get_active_fee_schedule(env: &Env) -> Option<FeeSchedule> {
    maybe_activate(env);
    get_state(env).active
}

/// Returns the pending fee schedule, if any.
///
/// This is a pure read: it does not activate a pending schedule.
/// Use `get_active_fee_schedule` to observe activation.
pub(crate) fn get_pending_fee_schedule(env: &Env) -> Option<FeeSchedule> {
    get_state(env).pending
}

/// Promotes a pending schedule to active if its activation ledger has arrived.
///
/// # Invariants
"///
/// - Only one state transition is performed per call.
/// - The pending schedule is cleared and the activation ledger is cleared on activation.
/// - The previous active schedule is not overwritten during activation.
/// - The function is a no-op if there is no pending schedule or the activation
///   ledger has not yet arrived.
///
/// # Concurrency
///
/// The read-modify-write is atomic within a Soroban invocation. Since the function
/// does not yield control between the read and the write, concurrent invocations
/// cannot observe an intermediate state or produce a lost update. Repeated calls
/// are idempotent because the pending schedule is cleared before the function
/// returns.
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
