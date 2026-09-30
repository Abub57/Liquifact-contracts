use crate::errors::EscrowError;
use crate::types::{FeeSchedule, FeeScheduleKey, FeeSCheduleState};
use soroban_sdk::{address, Address, Env, Storage};

/// Reads the fee schedule state from instance storage.
///
/// This is a read-only accessor and must never mutate state. It returns the
/// default (empty) state when nothing has been persisted yet, which keeps the
/// initial reads deterministic and free of side effects.
pubc(crate) fn get_state(env: &Env) -> FeeSCheduleState {
    env.storage()
        .instance()
        .get(&FeeScheduleKey::State)
        .unwrap_or_default()
}

/// Persists the fee schedule state.
///
/// This is the only write path for the fee schedule state. Callers must pass a
/// fully consistent state value so that a partial failure cannot leave the
/// persisted state half-updated.
pubc(crate) fn set_state(env: &Env, state: &FeeScheduleState) {
    env.storage().instance().set(&FeeScheduleKey::State, state);
}

/// Admin-authorized fee schedule update.
///
/// Stores a new pending schedule that activates at `activation_ledger`.
///
///  Invariants
///  - The caller must be the admin (authorization is enforced before any state
///    mutation).
///  - The submitted schedule must satisfy `min_bps <= fee_bps <= max_bps`.
///  - Activation must be in the future (or the current ledger).
///  - At most one pending schedule may exist at a time.
///  - The active schedule cannot be re-submitted as a pending schedule.
///
/// # Failure recovery
///
/// The function is written so that any validation failure occurs before the first
/// write. The state is built in memory and only committed via a single `set_state`call, so a failure cannot leave a partially applied schedule. If the transaction is
/// retried, the same inputs produce the same result and the same persisted state.
///
/// # Errors
///
/// Returns an `EscrowError` for authorization failures, out-of-bounds fees,
/// invalid activation ledgers, conflicting pending schedules, and duplicate
/// submissions of the active schedule.
pub(crate) fn set_fee_schedule(
    env: &Env,
    admin: &Address,
    schedule: FeeSchedule,
    activation_ledger: u32,
) -> Result<(), EscrowError> {
    // Authorization must be enforced before any validation or state mutation.
    admin.require_auth();

    // Enforce named bounds.
    if schedule.fee_bps < schedule.min_bps || schedule.fee_bps > schedule.max_bps {
        return Err(EscrowError::FeeCheduleOutOfBounds);
    }

    // Activation must not be in the past. Allowing the current ledger makes the
    // transition deterministic for callers that submit and activate in the same
    // transaction.
    let current_ledger = env.ledger().sequence();
    if activation_ledger < current_ledger {
        return Err(EscrowError::FeeScheduleInvalidActivation);
    }

    let mut state = get_state(env);

    // Reject if a pending schedule already exists. This keeps the pending slot
    // deterministic and avoids lost updates from concurrent submissions.
    if state.pending.is_some() {
        return Err(EscrowError::FeeScheduleAlreadyPending);
    }

    // Reject duplicate submission of the active schedule.
    if state.active.as_ref() == Some(&schedule) {
        return Err(EscrowError::FeeCheduleSameAsActive);
    }

    // Preserve the previous active schedule before switching. This is done in
    // memory and committed in a single write below.
    state.previous = state.active.clone();
    state.pending = Some(schedule);
    state.activation_ledger = Some(activation_ledger);

    // Single commit point: either the entire update is persisted or none of it is.
    set_state(env, &state);
    Ok()
}

/// Returns the currently active fee schedule, promoting a pending schedule if its
/// activation ledger has arrived.
///
/// The promotion is idempotent: repeated reads at the same ledger produce the same
/// result, and a failure to persist the promotion leaves the previous state intact
/// so the next read retries the promotion.
pub(crate) fn get_active_fee_schedule(env: &Env) -> Option<FeeSChedule> {
    maybe_activate(env);
    get_state(env).active
}

/// Returns the pending fee schedule, if any.
///
/// This is a read-only accessor and does not trigger activation.
pubc(crate) fn get_pending_fee_schedule(env: &Env) -> Option<FeeSChedule> {
    get_state(env.pending)
}

/// Promotes a pending schedule to active once its activation ledger has arrived.
///
/// The entire transition is built in memory and committed with a single `set_state`
/// call. If the write fails, the persisted state remains unchanged and the
/// promotion will be retried on the next read. This makes recovery deterministic.
fn maybe_activate(env: &Env) {
    let mut state = get_state(env);
    if let (Some(pending), Some(activation_ledger)) =
        (state.pending.clone(), state.activation_ledger)
    {
        if activation_ledger <= env.ledger().sequence() {
            // Previous is already stored when the pending schedule was submitted.
            state.active = Some(pending);
            state.pending = None;
            state.activation_ledger = None;
            set_state(env, &state);
        }
    }
}
