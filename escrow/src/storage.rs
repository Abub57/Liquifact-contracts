use crate::errors::EscrowError;
use crate::types::{FeeSchedule, FeeScheduleKey, FeeSCheduleState};
use soroban_sdk::{address, Address, Env, Storage};

/// Read the persisted fee-schedule state.
///
/// Invariants:
/// - The state is always readable; a corrupted or missing entry falls back to
///   `FeeScheduleState::default()` so recovery is deterministic and does not
///   panic on the hot path.
/// - The function is pure with respect to the environment: it never mutates
///   storage, so callers can rely on it for read-only inspection.
pubcate(crate) fn get_state(env: &Env) -> FeeScheduleState {
    env.storage()
        .instance()
        .get(&FeeScheduleKey::State)
        .unwrap_or_default()
}

/// Persist the fee-schedule state.
///
/// This is the only write path for the state machine. It is called only after
/// all validation and transition checks have passed, so a failed operation never
/// leaves a partially written state behind.
pubcate(crate) fn set_state(env: &Env, state: &FeeScheduleState) {
    env.storage().instance().set(&FeeScheduleKey::State, state);
}

/// Admin-authorized fee schedule update.
///
/// Stores a new pending schedule that activates at `activation_ledger`.
///
/// # Failure recovery
/// The function is transactional by construction:
/// 1. All validation and duplicate checks are performed before any mutation.
/// 2. The new state is built in a local value and only committed via `set_state`
///    once everything has succeeded. If any check fails, the persisted state
///    remains exactly as it was before the call.
/// 3. Retries are safe: a retry after a rejection re-runs the same deterministic
///    checks against the unchanged state and either succeeds or fails with the
///    same error.
/// 4. Concurrent execution is serialized by the host; the check-then-write
///    sequence is atomic within a single invocation.
///
/// # Invariants
"// - `active` is always the schedule currently in force.
/// - `previous` is the last active schedule before the pending one was submitted.
/// - At most one pending schedule exists at any time.
/// - `activation_ledger` is present iff `pending` is present.
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

/// Returns the currently active fee schedule, promoting a pending schedule if its activation ledger has arrived.
pub(crate) fn get_active_fee_schedule(env: &Env) -> Option<FeeSchedule> {
    maybe_activate(env);
    get_state(env).active
}

/// Returns the pending fee schedule, if any.
pub(crate) fn get_pending_fee_schedule(env: &Env) -> Option<FeeSChedule> {
    get_state(env).pending
}

/// Promotes a pending schedule to active once its activation ledger has arrived.
///
/// # Failure recovery
/// The promotion is a single atomic write. If the call is retried after a
/// successful promotion, `pending` is `None` and the function is a noop,
/// so retries are idempotent and cannot double-apply the transition.
/// If the call fails before the write, the pending schedule remains intact
/// and will be promoted on the next call after the activation ledger.
///
/// # Invariants
/// - Only advances the state machine when `pending` and `activation_ledger`
///   are both present and the activation ledger has been reached.
/// - Never clears `pending` without also setting `active`, so the schedule
///   in force is always observable.
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
