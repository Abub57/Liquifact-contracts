//! Centralised, concurrency-hardened storage for the bounded **fee-schedule**
//! subsystem (`submit_fee_schedule` / `activate_fee_schedule` and the
//! `get_*_fee_schedule` views).
//!
//! # Why this module exists
//!
//! The fee-schedule lifecycle used to be open-coded at the entrypoints: each
//! function read and wrote the `Active` / `Pending` / `Previous` instance cells
//! inline, with the ordering of those writes left implicit. That is fragile
//! under repeated or re-entrant execution — a retried/duplicated submission, or
//! a second mutation interleaved before the first has returned, could observe or
//! persist a partially-applied schedule. This module makes the whole lifecycle
//! go through one place with explicit, testable guarantees.
//!
//! # Execution model (why this is safe on Soroban)
//!
//! Soroban executes one invocation at a time and commits all storage writes of a
//! succeeded invocation as a single atomic unit: if the invocation panics or
//! returns an error, **none** of its writes are visible. "Concurrent execution"
//! therefore does not mean parallel threads stomping on cells; it means:
//!
//! * **Re-entrancy** — the contract can re-enter itself (or be re-entered via a
//!   callback) before the outer call has returned. A shared storage cell can be
//!   mutated twice in one invocation.
//! * **Retries / duplicates** — an at-least-once client can submit the same
//!   schedule twice.
//! * **Ledger boundaries** — activation must be a pure function of
//!   `env.ledger().sequence()` so every validator agrees.
//!
//! The guarantees below are aimed exactly at those.
//!
//! # Invariants
//!
//! 1. **Exclusive mutation.** Every mutating operation runs under
//!    [`FeeScheduleStorageKey::MutationLock`]. A second mutation while the lock
//!    is held fails fast with [`FeeScheduleError::ConcurrentMutation`] and
//!    changes nothing.
//! 2. **Single pending.** `Pending` is either absent or exactly one schedule
//!    whose `activation_ledger` was strictly in the future when accepted. A
//!    second, *different* pending submission is rejected.
//! 3. **Idempotent retry.** Re-submitting the exact pending schedule is a
//!    deterministic no-op (`Ok`); an at-least-once retry can never fail the
//!    caller or double-apply.
//! 4. **Monotone activation.** Promotion moves `Pending` into `Active` and
//!    records the old active in `Previous`, in one invocation. It never demotes
//!    or rewrites `Active` in any other direction and can happen at most once
//!    per pending schedule.
//! 5. **Deterministic boundary.** Activation is
//!    `pending.activation_ledger <= env.ledger().sequence()`. The same ledger
//!    always yields the same answer.
//! 6. **Pure views.** The `get_*` reads never mutate storage; a due-but-not-yet
//!    -persisted schedule is projected on the fly, exactly as before.
//! 7. **No leaked lock.** The lock is released by RAII on every normal return
//!    path; a panic unwinds the invocation and rolls the lock write back too, so
//!    a transaction can never strand it.
//!
//! # Backward compatibility
//!
//! The storage layout (`Active` / `Pending` / `Previous`) and every public
//! signature are unchanged. The only new cell is the additive `MutationLock`
//! flag (ADR-007): absent ⇒ unlocked, so instances written before this change
//! behave identically. No `migrate` call is required.

use crate::{FeeSchedule, FeeScheduleError, FeeScheduleStorageKey};
use soroban_sdk::{Address, Env};

/// Transient in-memory view of the three fee-schedule cells.
///
/// Deliberately **not** a `#[contracttype]`: it is never persisted as a single
/// value (the Soroban SDK does not support nesting a custom contract type inside
/// an `Option` field of another `contracttype`). It exists only to let a single
/// mutation read-modify-write all three cells coherently before returning.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct FeeScheduleState {
    pub active: Option<FeeSchedule>,
    pub pending: Option<FeeSchedule>,
    pub previous: Option<FeeSchedule>,
}

/// RAII guard for [`FeeScheduleStorageKey::MutationLock`].
///
/// The lock is cleared on `Drop`, so every normal early return releases it. A
/// panic unwinds the whole invocation and Soroban rolls the storage write back
/// anyway, so a transaction can never strand the lock.
struct MutationGuard<'a> {
    env: &'a Env,
}

impl Drop for MutationGuard<'_> {
    fn drop(&mut self) {
        self.env
            .storage()
            .instance()
            .remove(&FeeScheduleStorageKey::MutationLock);
    }
}

/// Acquire the exclusive fee-schedule mutation lock, or fail if another
/// mutation is already in flight.
///
/// This is the single gate that serialises fee-schedule writes. It is
/// intentionally conservative: a re-entrant call fails fast with a typed,
/// diagnosable error rather than racing the outer mutation.
fn acquire_mutation_lock(env: &Env) -> Result<MutationGuard<'_>, FeeScheduleError> {
    let held: bool = env
        .storage()
        .instance()
        .get(&FeeScheduleStorageKey::MutationLock)
        .unwrap_or(false);
    if held {
        return Err(FeeScheduleError::ConcurrentMutation);
    }
    env.storage()
        .instance()
        .set(&FeeScheduleStorageKey::MutationLock, &true);
    Ok(MutationGuard { env })
}

/// Read the three fee-schedule cells into an in-memory state. Never writes.
pub(crate) fn read_state(env: &Env) -> FeeScheduleState {
    FeeScheduleState {
        active: env.storage().instance().get(&FeeScheduleStorageKey::Active),
        pending: env
            .storage()
            .instance()
            .get(&FeeScheduleStorageKey::Pending),
        previous: env
            .storage()
            .instance()
            .get(&FeeScheduleStorageKey::Previous),
    }
}

/// Commit an in-memory state back to the three cells.
///
/// Only ever called from a mutating operation (i.e. under the mutation lock);
/// the individual writes are committed atomically by the surrounding
/// invocation, so no partially-applied state is ever observable.
fn write_state(env: &Env, state: &FeeScheduleState) {
    // Store the schedule directly (not `Option<FeeSchedule>`) so a read of the
    // key yields `Option<FeeSchedule>` exactly as the pre-existing call sites
    // expect; absence is represented by removing the key.
    match &state.active {
        Some(active) => env
            .storage()
            .instance()
            .set(&FeeScheduleStorageKey::Active, active),
        None => env
            .storage()
            .instance()
            .remove(&FeeScheduleStorageKey::Active),
    }
    match &state.pending {
        Some(pending) => env
            .storage()
            .instance()
            .set(&FeeScheduleStorageKey::Pending, pending),
        None => env
            .storage()
            .instance()
            .remove(&FeeScheduleStorageKey::Pending),
    }
    match &state.previous {
        Some(previous) => env
            .storage()
            .instance()
            .set(&FeeScheduleStorageKey::Previous, previous),
        None => env
            .storage()
            .instance()
            .remove(&FeeScheduleStorageKey::Previous),
    }
}

/// If the pending schedule's activation ledger has arrived, move it to
/// `active`, record the prior active as `previous`, and clear `pending`.
///
/// Returns `true` iff a promotion happened. Pure in-memory transformation of
/// `state`; the caller decides whether to persist.
fn promote_if_due(env: &Env, state: &mut FeeScheduleState) -> bool {
    let current_ledger = env.ledger().sequence();
    match state.pending.clone() {
        Some(pending) if pending.activation_ledger <= current_ledger => {
            state.previous = state.active.clone();
            state.active = Some(pending);
            state.pending = None;
            true
        }
        _ => false,
    }
}

/// Validate a candidate schedule's declared bounds and activation ledger.
fn validate(env: &Env, schedule: &FeeSchedule) -> Result<(), FeeScheduleError> {
    if schedule.min_fee_bps > schedule.fee_bps
        || schedule.fee_bps > schedule.max_fee_bps
        || schedule.max_fee_bps > 10_000
    {
        return Err(FeeScheduleError::FeeOutOfBounds);
    }
    if schedule.activation_ledger <= env.ledger().sequence() {
        return Err(FeeScheduleError::InvalidActivationLedger);
    }
    Ok(())
}

/// Admin-authorised submission of a new pending fee schedule.
///
/// # Guarantees
/// * Authorises `admin` **before** any mutation.
/// * Rejects out-of-bounds schedules and non-future activation ledgers.
/// * A pending schedule whose activation ledger has already arrived is promoted
///   first, so a stale pending schedule cannot wedge the lifecycle.
/// * Re-submitting the exact pending schedule is an idempotent no-op.
/// * A *different* pending schedule is rejected with
///   [`FeeScheduleError::PendingScheduleExists`].
///
/// The whole read-modify-write runs under the mutation lock.
pub(crate) fn submit_fee_schedule(
    env: &Env,
    admin: &Address,
    schedule: &FeeSchedule,
) -> Result<(), FeeScheduleError> {
    // 1. Authorise first: no storage mutation happens before this succeeds.
    admin.require_auth();
    apply_submission(env, schedule)
}

/// The authorisation-free body of [`submit_fee_schedule`].
///
/// Split out so the storage invariants (idempotency, the mutation lock, bounds
/// and boundary validation) can be unit-tested without a contract invocation
/// frame, which [`Address::require_auth`] needs. [`submit_fee_schedule`] calls
/// this only after `require_auth` has succeeded.
pub(crate) fn apply_submission(env: &Env, schedule: &FeeSchedule) -> Result<(), FeeScheduleError> {
    // Read-only validation (bounds + future activation).
    validate(env, schedule)?;

    // Serialise the read-modify-write against re-entrant / overlapping calls.
    let _guard = acquire_mutation_lock(env)?;

    let mut state = read_state(env);

    // Promote a due predecessor first so a stale pending schedule does not
    // wedge the lifecycle.
    promote_if_due(env, &mut state);

    if let Some(pending) = state.pending.as_ref() {
        if pending == schedule {
            // Idempotent retry of an already-accepted schedule: report success
            // without a second effect. Persist the (possibly promoted) state so
            // an already-due predecessor is committed too.
            write_state(env, &state);
            return Ok(());
        }
        return Err(FeeScheduleError::PendingScheduleExists);
    }

    // `previous` is recorded when the promotion actually happens, not here.
    state.pending = Some(schedule.clone());
    write_state(env, &state);
    Ok(())
}

/// Promote a pending schedule if its activation ledger has arrived.
///
/// Idempotent: returns `true` on the invocation that promotes, and `false`
/// thereafter. Runs under the mutation lock and writes at most once.
pub(crate) fn activate_if_due(env: &Env) -> Result<bool, FeeScheduleError> {
    let _guard = acquire_mutation_lock(env)?;
    let mut state = read_state(env);
    let promoted = promote_if_due(env, &mut state);
    if promoted {
        write_state(env, &state);
    }
    Ok(promoted)
}

/// The schedule in force for the current ledger.
///
/// Pure: projects a due-but-not-yet-persisted pending schedule on the fly.
pub(crate) fn active(env: &Env) -> Option<FeeSchedule> {
    let state = read_state(env);
    match state.pending {
        Some(pending) if pending.activation_ledger <= env.ledger().sequence() => Some(pending),
        _ => state.active,
    }
}

/// The pending schedule, if it has not yet reached its activation ledger.
///
/// Pure: returns `None` once the pending schedule is due (it is then reported
/// by [`active`]).
pub(crate) fn pending(env: &Env) -> Option<FeeSchedule> {
    let state = read_state(env);
    match state.pending {
        Some(pending) if pending.activation_ledger > env.ledger().sequence() => Some(pending),
        _ => None,
    }
}

/// The schedule that was active before the most recent promotion.
///
/// Pure: if a promotion is due but not yet persisted, the currently-active
/// schedule is the correct "previous" answer.
pub(crate) fn previous(env: &Env) -> Option<FeeSchedule> {
    let state = read_state(env);
    if let Some(pending) = state.pending.as_ref() {
        if pending.activation_ledger <= env.ledger().sequence() {
            return state.active;
        }
    }
    state.previous
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FeeSchedule, FeeScheduleError, FeeScheduleStorageKey};
    use soroban_sdk::testutils::Ledger as _;
    use soroban_sdk::{contract, contractimpl, Env};

    /// Minimal registered contract so the tests get a valid contract frame for
    /// instance-storage access (`env.as_contract`).
    #[contract]
    pub struct Harness;

    #[contractimpl]
    impl Harness {
        pub fn noop() {}
    }

    /// Run `f` inside a fresh environment **and** a registered contract frame,
    /// with the ledger sequence set to `seq`.
    fn with_harness<R>(seq: u32, f: impl FnOnce(&Env) -> R) -> R {
        let env = Env::default();
        env.ledger().with_mut(|l| l.sequence_number = seq);
        let id = env.register(Harness, ());
        env.as_contract(&id, || f(&env))
    }

    fn set_ledger(env: &Env, seq: u32) {
        env.ledger().with_mut(|l| l.sequence_number = seq);
    }

    fn schedule(fee_bps: u32, min_bps: u32, max_bps: u32, activation_ledger: u32) -> FeeSchedule {
        FeeSchedule {
            fee_bps,
            min_fee_bps: min_bps,
            max_fee_bps: max_bps,
            activation_ledger,
        }
    }

    fn lock_held(env: &Env) -> bool {
        env.storage()
            .instance()
            .get::<FeeScheduleStorageKey, bool>(&FeeScheduleStorageKey::MutationLock)
            .unwrap_or(false)
    }

    fn stored_active(env: &Env) -> Option<FeeSchedule> {
        env.storage().instance().get(&FeeScheduleStorageKey::Active)
    }

    fn stored_pending(env: &Env) -> Option<FeeSchedule> {
        env.storage()
            .instance()
            .get(&FeeScheduleStorageKey::Pending)
    }

    // --- success -----------------------------------------------------------

    #[test]
    fn submit_stores_pending_and_releases_lock() {
        with_harness(100, |env| {
            let s = schedule(250, 100, 500, 101);
            assert_eq!(apply_submission(env, &s), Ok(()));
            assert_eq!(pending(env), Some(s));
            assert_eq!(active(env), None);
            // Guard must not leak across invocations.
            assert!(!lock_held(env));
        });
    }

    #[test]
    fn activate_promotes_exactly_at_boundary_and_records_previous() {
        with_harness(100, |env| {
            // Seed an active schedule by promoting a first pending one.
            assert_eq!(apply_submission(env, &schedule(100, 0, 500, 101)), Ok(()));
            set_ledger(env, 101);
            assert_eq!(activate_if_due(env), Ok(true));
            let first = schedule(100, 0, 500, 101);
            assert_eq!(active(env), Some(first.clone()));
            assert_eq!(pending(env), None);

            // Second schedule activates exactly at its boundary ledger.
            assert_eq!(apply_submission(env, &schedule(300, 0, 500, 150)), Ok(()));
            set_ledger(env, 149);
            assert_eq!(active(env), Some(first.clone()));
            set_ledger(env, 150);
            assert_eq!(active(env), Some(schedule(300, 0, 500, 150)));
            assert_eq!(previous(env), Some(first));
            assert_eq!(activate_if_due(env), Ok(true));
        });
    }

    // --- idempotent retries ------------------------------------------------

    #[test]
    fn resubmitting_identical_pending_schedule_is_idempotent() {
        with_harness(100, |env| {
            let s = schedule(250, 100, 500, 120);
            assert_eq!(apply_submission(env, &s), Ok(()));
            // At-least-once retry must converge to the same state, not error.
            assert_eq!(apply_submission(env, &s), Ok(()));
            assert_eq!(pending(env), Some(s));
            assert!(!lock_held(env));
        });
    }

    #[test]
    fn activate_is_idempotent_after_first_promotion() {
        with_harness(100, |env| {
            assert_eq!(apply_submission(env, &schedule(250, 100, 500, 101)), Ok(()));
            set_ledger(env, 101);
            // Boundary is inclusive: due at exactly the activation ledger.
            assert_eq!(activate_if_due(env), Ok(true));
            assert_eq!(activate_if_due(env), Ok(false));
            assert!(!lock_held(env));
        });
    }

    // --- rejection ---------------------------------------------------------

    #[test]
    fn submit_rejects_out_of_bounds() {
        with_harness(100, |env| {
            // fee below declared min
            assert_eq!(
                apply_submission(env, &schedule(50, 100, 500, 200)),
                Err(FeeScheduleError::FeeOutOfBounds)
            );
            // fee above declared max
            assert_eq!(
                apply_submission(env, &schedule(600, 100, 500, 200)),
                Err(FeeScheduleError::FeeOutOfBounds)
            );
            // max above global 10_000 ceiling
            assert_eq!(
                apply_submission(env, &schedule(10_001, 0, 10_001, 200)),
                Err(FeeScheduleError::FeeOutOfBounds)
            );
            assert_eq!(pending(env), None);
            assert!(!lock_held(env));
        });
    }

    #[test]
    fn submit_rejects_non_future_activation_ledger() {
        with_harness(100, |env| {
            // exactly current ledger
            assert_eq!(
                apply_submission(env, &schedule(250, 100, 500, 100)),
                Err(FeeScheduleError::InvalidActivationLedger)
            );
            // in the past
            assert_eq!(
                apply_submission(env, &schedule(250, 100, 500, 99)),
                Err(FeeScheduleError::InvalidActivationLedger)
            );
            assert!(!lock_held(env));
        });
    }

    #[test]
    fn submit_rejects_conflicting_pending() {
        with_harness(100, |env| {
            assert_eq!(apply_submission(env, &schedule(250, 100, 500, 200)), Ok(()));
            assert_eq!(
                apply_submission(env, &schedule(300, 100, 500, 300)),
                Err(FeeScheduleError::PendingScheduleExists)
            );
        });
    }

    // --- concurrency / re-entrancy ----------------------------------------

    #[test]
    fn overlapping_mutation_is_rejected_without_side_effects() {
        with_harness(100, |env| {
            // Simulate an outer invocation holding the lock.
            env.storage()
                .instance()
                .set(&FeeScheduleStorageKey::MutationLock, &true);
            let s = schedule(250, 100, 500, 200);
            assert_eq!(
                apply_submission(env, &s),
                Err(FeeScheduleError::ConcurrentMutation)
            );
            assert_eq!(
                activate_if_due(env),
                Err(FeeScheduleError::ConcurrentMutation)
            );
            // The rejected call must not have modified the schedule state.
            assert_eq!(pending(env), None);
            assert_eq!(stored_pending(env), None);
            // The lock remains owned by the outer invocation.
            assert!(lock_held(env));
        });
    }

    // --- views are non-mutating -------------------------------------------

    #[test]
    fn views_do_not_persist_promotions_or_leak_the_lock() {
        with_harness(100, |env| {
            assert_eq!(apply_submission(env, &schedule(250, 100, 500, 101)), Ok(()));
            set_ledger(env, 101);
            // Reading across the boundary must not write the cells.
            assert_eq!(active(env), Some(schedule(250, 100, 500, 101)));
            assert_eq!(pending(env), None);
            assert!(!lock_held(env));
            // The persisted `Active` cell is still empty until an explicit activation.
            assert_eq!(stored_active(env), None);
            assert!(stored_pending(env).is_some());
        });
    }

    #[test]
    fn activation_commits_all_three_cells_in_one_invocation() {
        with_harness(100, |env| {
            let first = schedule(100, 0, 500, 101);
            assert_eq!(apply_submission(env, &first), Ok(()));
            set_ledger(env, 101);
            assert_eq!(activate_if_due(env), Ok(true));
            assert_eq!(stored_active(env), Some(first));
            assert_eq!(stored_pending(env), None);
        });
    }
}
