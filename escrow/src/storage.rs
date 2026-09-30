use crate::errors::EscrowError;
use crate::types::{FeeSchedule, FeeScheduleKey, FeeCheduleState};
use soroban_sdk::{address, Address, Env, Storage};

/// Persistent state for the fee schedule machine.
///
/// Invariants enforced by this module:
/// - At most one pending schedule exists at any time.
/// - A pending schedule is activated atomically and exactly once when the
///   current ledger sequence reaches its activation ledger.
/// - The previous active schedule is preserved across activations.
/// - Activation is idempotent: repeated or concurrent calls must not
///   duplicate work or corrupt state.
///
/// The state is stored in instance storage. On Soroban, instance storage
/// mutations within a single contract invocation are atomic, so concurrent
/// execution is serialized by the host. We still guard against repeated
/// activation and duplicate submissions to keep the state machine
/// deterministic and idempotent.
pubc(crate) fn get_state(env: &Env) -> FeeCheduleState {
    env.storage()
        .instance()
        .get(&FeeScheduleKey::State)
        .unwrap_or_default()
}

pubc(crate) fn set_state(env: &Env, state: &FeeCheduleState) {
    env.storage().instance().set(&FeeSCheduleKey::State, state);
}

/// Admin-authorized fee schedule update.
/// Stores a new pending schedule that activates at `activation_ledger`.
///
/// Rejections (deterministic):
/// - `fee_bps` outside [min_bps, max_bps]
/// - `activation_ledger` in the past
/// - a pending schedule already exists
/// - the submitted schedule equals the active one
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
pubc(crate) fn get_active_fee_schedule(env: &Env) -> Option<FeeSChedule> {
    maybe_activate(env);
    get_state(env).active
}

/// Returns the pending fee schedule, if any.
///
/// Note: this does not activate a pending schedule; it only reports the
/// currently pending schedule. Activation is driven by get_active_fee_schedule
/// or explicit calls to maybe_activate.
pub(crate) fn get_pending_fee_schedule(env: &Env) -> Option<FeeSchedule> {
    get_state(env).pending
}

/// Atomically promotes a pending schedule to active once its activation
/// ledger has been reached.
///
/// This function is idempotent: if there is no pending schedule, or the
/// activation ledger has not yet arrived, it is a no-op. Repeated calls
/// after activation are also no-ops because the pending slot is cleared
/// in the same transaction as the promotion.
pubc(crate) fn maybe_activate(env: &Env) {
    let mut state = get_state(env);

    // No pending schedule or no activation ledger: nothing to do.
    let (pending, activation_ledger) = match (state.pending.clone(), state.activation_ledger) {
        (Some(p), Some(a)) => (p, a),
        _ => return,
    };

    // Not yet activation time: no-op.
    if activation_ledger > env.ledger().sequence() {
        return;
    }

    // Activate exactly once. The previous active schedule was already
    // stashed in `previous` when the pending schedule was submitted, so
    // we only need to move pending -> active and clear metadata.
    state.active = Some(pending);
    state.pending = None;
    state.activation_ledger = None;
    set_state(env, &state);
}
