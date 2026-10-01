use soroban_sdk::{contracttype, IntoKey, Symbol};

/// Named fee schedule with explicit bounds.
///
/// Invariants:
/// - `min_bps <= fee_bps <= max_bps`
/// - `max_bps <= 10_000` (basis points cannot exceed 100%)
/// - `name` is non-empty (enforced by caller via `Symbol` construction)
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeeSchedule {
    pub name: Symbol,
    pub fee_bps: u32,
    pub min_bps: u32,
    pub max_bps: u32,
}

impl FeeSchedule {
    /// Maximum representable basis points (100%).
    pub const MAX_BPS: u32 = 10_000;

    /// Returns `true` iff the schedule satisfies all bounds invariants.
    pub fn is_valid(&self) -> bool {
        self.min_bps <= self.fee_bps
            && self.fee_bps <= self.max_bps
            && self.max_bps <= Self::MAX_BPS
    }

    /// Returns `true` iff `other` has the same `name` as `self`.
    pub fn same_name(&self, other: &FeeSchedule) -> bool {
        self.name == other.name
    }
}

/// Storage state for the fee schedule lifecycle.
///
/// Invariants:
/// - `active`, `pending`, and `previous` are each individually valid when `Some`.
/// - `pending` and `activation_ledger` are set or unset together.
/// - `previous` is only set once an activation has occurred.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct FeeScheduleState {
    pub active: Option<FeeSchedule>,
    pub pending: Option<FeeSchedule>,
    /// Ledger at which `pending` becomes `active`.
    pub activation_ledger: Option<u32>,
    /// The schedule that was active before the current active.
    pub previous: Option<FeeSchedule>,
}

impl FeeScheduleState {
    /// Returns `true` iff the state is internally consistent.
    pub fn is_consistent(&self) -> bool {
        if let Some(s) = &self.active {
            if !s.is_valid() {
                return false;
            }
        }
        if let Some(s) = &self.pending {
            if !s.is_valid() {
                return false;
            }
        }
        if let Some(s) = &self.previous {
            if !s.is_valid() {
                return false;
            }
        }
        self.pending.is_some() == self.activation_ledger.is_some()
    }
}

/// Storage keys used for fee-schedule state.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq, IntoKey)]
pub enum FeeScheduleKey {
    State,
}
