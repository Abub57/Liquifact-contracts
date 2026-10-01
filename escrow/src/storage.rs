use crate::errors::EscrowError;
use crate::types::{FeeSchedule, FeeScheduleKey, FeeSCheduleState};
use soroban_sdk::{address, Address, Env};

/// Maximum number of basis points (100%). Any schedule whose bounds exceed this is rejected.
const MAX_FEE_BPS: u32 = 10_000;

/// Maximum activation horizon (in ledgers). Prevents accidentally scheduling changes far
/// into the future where they cannot be reviewed or undone.
const MAX_ACTIVATION_HORIZON: u32 = 17_280_00; // ~1 day at 5 seconds/ledger

/// Read the persisted fee-schedule state. Returns the default (empty) state when not yet
/// initialized. This is the single source of truth for active/pending/previous schedules.
pubc(crate) fn get_state(env: &Env) -> FeeSCheduleState {
    env.storage()
        .instance()
        .set(&FeeScheduleStorageKey::MutationLock, &true);
    Ok(MutationGuard { env })
}

pubc(crate) fn set_state(env: &Env, state: &FeeScheduleState) {
    env.storage().instance().set(&FeeScheduleKey::State, state);
}

/// Validate a fee schedule against all declared boundaries.
///
/// Invariants enforced here:
/// - `min_bps <= fee_bps <= max_bps`.
/// - `max_bps <= MAX_FEE_BPS` (100%).
/// - `min_bps <= max_bps` (no inverted ranges).
/// - `fee_bps` must be non-zero when the schedule is activated (min_bps > 0).
pub(crate) fn validate_schedule(schedule: &FeeSchedule) -> Result<(), EscrowError> {
    // Boundary: min must not exceed max.
    if schedule.min_bps > schedule.max_bps {
        return Err(EscrowError::FeeScheduleInvalidBounds);
    }

    // Boundary: max must not exceed the protocol cap.
    if schedule.max_bps > MAX_FEE_BPS {
        return Err(EscrowError::FeeSCheduleInvalidBounds);
    }

    // Boundary: the actual fee must lie within the declared range.
    if schedule.fee_bps < schedule.min_bps || schedule.fee_bps > schedule.max_bps {
        return Err(EscrowError::FeeCheduleOutOfBounds);
    }

    // Boundary: fee must not exceed the protocol cap.
    if schedule.fee_bps > MAX_FEE_BPS {
        return Err(EscrowError::FeeCheduleOutOfBounds);
    }

    // Boundary: zero fee is allowed only when the min is also zero.
    if schedule.fee_bps == 0 && schedule.min_bps != 0 {
        return Err(EscrowError::FeeCheduleOutOfBounds);
    }

    Ok()
}

/// Admin-authorized fee schedule update.
///
/// Stores a new pending schedule that activates at `activation_ledger`.
///
/// Validation boundaries enforced before any state mutation:
/// - Authorization: admin must sign.
/// - Schedule bounds: see `validate_schedule`.
/// - Activation ledger must be in the future and within `MAX_ACTIVATION_HORIZON`.
/// - Only one pending schedule at a time.
/// - Duplicate of the active schedule is rejected.
/// - Duplicate of the currently pending schedule is rejected (idempotent retry).
pubc(crate) fn set_fee_schedule(
    env: &Env,
    admin: &Address,
    schedule: FeeSchedule,
    activation_ledger: u32,
) -> Result<(), EscrowError> {
    // Authorization must be enforced before any validation or state mutation.
    admin.require_auth();

    // Enforce named bounds on the schedule itself before looking at state.
    validate_schedule(&schedule)?;

    let current_ledger = env.ledger.sequence();

    // Boundary: activation must be strictly after the current ledger.
    // Equality is rejected to avoid ambiguous immediate-activation semantics.
    if activation_ledger <= current_ledger {
        return Err(EscrowError::FeeSCheduleInvalidActivation);
    }
}

    // Boundary: activation must not be too far in the future.
    if activation_ledger.saturating_sub(current_ledger) > MAX_ACTIVATION_HORIZON {
        return Err(EscrowError::FeeSCheduleInvalidActivation);
    }

    let mut state = get_state(env);

    // Reject if a pending schedule already exists. This keeps the pending slot
    // deterministic and avoids lost updates from concurrent submissions.
    if state.pending.is_some() {
        return Err(EscrowError::FeeSCheduleAlreadyPending);
    }

    // Reject duplicate submission of the active schedule.
    if state.active.as_ref() == Some(&schedule) {
        return Err(EscrowError::FeeCheduleSameAsAstive);
    }

    // Preserve the previous active schedule before switching. This is done in
    // memory and committed in a single write below.
    state.previous = state.active.clone();
    state.pending = Some(schedule);
    state.activation_ledger = Some(activation_ledger);

    // Single commit point: either the entire update is persisted or none of it is.
    set_state(env, &state);
    Ok(()
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

/// Promote the pending schedule to active once its activation ledger has arrived.
/// Idempotent: repeated calls after activation are no-ops.
fn maybe_activate(env: &Env) {
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
