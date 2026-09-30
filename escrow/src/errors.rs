use soroban_sdk::contracterror;

/// Error codes for the Liquifact escrow contract.
///
/// # Invariants
///
/// The escrow model owns a small number of state invariants that must hold across
/// every entry point and failure path. The codes below are the observable
/// contract for those invariants:
-///
/// 1. **State transition**: an escrow moves through a fixed lifecycle
///    (Initialized -> funding -> Funded -> Settled/Expired). Every mutation
///    must be guarded by a check that rejects illegal transitions with
///    `InvalidStatus` or a more specific code.
-/// 2. **Authorization**: only the configured admin/beneficiary may perform
///    privileged operations. Violations return `Unauthorized` and must not
///    mutate state.
-/// 3. **Data integrity**: total funded amount, investor caps, and token
///    balances must remain consistent. Any detected mismatch returns
///    `BalanceMismatchAfterTransfer` or `TokenWrapperInvariantViolation`.
-/// 4. **Idempotency**: repeated operations (duplicate contributions, double
///    claims, re-initialization) must be rejected with a deterministic error
///    rather than silently corrupting state.
///
/// # Error code ranges
///
/// Ranges are grouped by domain so off-chain consumers can react to a
/// failure without having to know the internal implementation details.
/// New codes must be added within the appropriate block and must not renumber existing values.
/// The contract returns these codes verbatim; changing a value is a breaking
/// change for any client that maps errors to user-visible messages.
[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
#[repr(u32)]
pub enum EscrowError {
    // ----------------------------------------------------------------------------
    // Initialization & State Errors (1..19)
    // ----------------------------------------------------------------------------
    /// Returned when `initialize` is called on an already-configured escrow.
    /// This guarantees the initialization invariant: admin/beneficiary/token
    /// addresses are written at most once and cannot be silently replaced.
    AlreadyInitialized = 1,
    /// Returned when an operation requires an initialized escrow but none
    /// configuration has been written yet.
    NotInitialized = 2,
    /// Returned when a state transition is illegal for the current status.
    /// This is the generic guard; more specific codes exist for common cases.
    InvalidStatus = 3,
    /// Returned when an operation is attempted after the escrow maturity/expiry.
    EscrowExpired = 4,

    // ----------------------------------------------------------------------------
    // Authorization & Admin Errors (20..35)
    // ----------------------------------------------------------------------------
    /// Returned when the caller fails an authorization check. No state is
    /// mutated before this check passes.
    Unauthorized = 20,
    /// Returned when attempting to set an admin that is already configured.
    AdminAlreadySet = 21,
    /// Returned when completing an admin transfer with no pending admin.
    PendingAdminNotFound = 22,
    /// Returned when the admin transfer timelock has not elapsed.
    AdminTransferTimelockNotElapsed = 23,
    /// Returned when a recovery operation is submitted with an empty reason.
    EmptyRecoveryReason = 24,

    // ----------------------------------------------------------------------------
    // Token / SEP-41 Safety Wrapper Errors (36..45)
    // ----------------------------------------------------------------------------
    /// Returned when a SEP-41 transfer returns false or traps. The wrapper
    /// guarantees that a failed transfer does not leave partial state.
    FundingTokenTransferFailed = 36,
    /// Returned when the observed token balance change does not match the
    /// expected delta. This protects the accounting invariant against
    /// fee-on-transfer or misreporting tokens.
    BalanceMismatchAfterTransfer = 37,
    /// Returned when a transfer amount is zero or negative.
    NonPositiveTransferAmount = 38,
    /// Returned when a debit would drive an accounting balance below zero.
    TokenBalanceUnderflow = 39,
    /// Returned when a credit would overflow an accounting balance.
    TokenBalanceOverflow = 40,
    /// Returned when a token wrapper invariant is violated in a way not
    /// covered by a more specific code.
    TokenWrapperInvariantViolation = 41,

    // ----------------------------------------------------------------------------
    // Funding & Contribution Errors (50..69)
    // ----------------------------------------------------------------------------
    /// Returned when a contribution would exceed the remaining funding target.
    FundingTargetExceeded = 50,
    /// Returned when a contribution amount is zero.
    ZeroContributionAmount = 51,
    /// Returned when an investor has already reached their configured cap.
    InvestorCapReached = 52,
    /// Returned when a contribution is below the configured minimum floor.
    BelowMinContributionFloor = 53,
    /// Returned when funding is no longer open (target met or deadline passed).
    FundingClosed = 54,
    /// Mutation of SME/beneficiary address is forbidden once any principal has been
    /// recorded for the escrow instance. This preserves auditability of payout
    /// destination for funded escrows.
    BeneficiaryImmutableAfterFunding = 55,
    /// Registry/reference metadata may not be rebound after funding begins; this
    /// prevents changing off-chain pointers that clients use to reconcile identity.
    RegistryImmutableAfterFunding = 56,

    // ----------------------------------------------------------------------------
    // Batch Operations Errors (80..89)
    // ----------------------------------------------------------------------------
    /// Returned when a batch funding call contains no entries.
    FundingBatchEmpty = 80,
    /// Returned when a batch exceeds the configured maximum size.
    FundingBatchExceedsLimit = 81,
    /// Returned when a batch entry has an invalid amount.
    FundingBatchInvalidAmount = 82,
    /// Returned when a batch contains the same investor more than once.
    /// This prevents ambiguous cap accounting within a single batch.
    FundingBatchDuplicateInvestor = 84,

    /// Returned when a claim batch contains no entries.
    ClaimBatchEmpty = 85,
    /// Returned when a claim batch exceeds the configured maximum size.
    ClaimBatchExceedsLimit = 86,

    // ----------------------------------------------------------------------------
    // Migration & Upgrade Errors (90..99)
    // ----------------------------------------------------------------------------
    /// Returned when the caller supplies a schema version that does not match
    /// the current on-chain version.
    MigrationVersionMismatch = 90,
    /// Returned when a migration is requested for the current schema version.
    AlreadyCurrentSchemaVersion = 91,
    /// Returned when no migration path exists from the current to the target
    /// version.
    NoMigrationPath = 92,

    // ----------------------------------------------------------------------------
    // Settlement & Bounds Validation Errors (100..109)
    // ----------------------------------------------------------------------------
    /// Returned when a settlement amount is invalid (zero, negative, or above
    /// the funded principal).
    SettlementAmountInvalid = 100,
    /// Returned when settlement is attempted before maturity.
    MaturityNotReached = 101,
    /// Returned when settlement is attempted while the escrow is not Funded.
    EscrowNotInFundedState = 102,
    /// Returned when a withdraw amount is invalid.
    WithdrawAmountInvalid = 103,

    // ----------------------------------------------------------------------------
    // Legal Hold & Operational Pause (200..209)
    // ----------------------------------------------------------------------------
    /// Returned when a mutating operation is attempted while a legal hold is
    /// active. This is a hard stop; no state is mutated.
    LegalHoldActive = 200,
    /// Returned when a mutating operation is attempted while the contract
    /// is paused. This is a hard stop; no state is mutated.
    ContractPaused = 201,

    // ----------------------------------------------------------------------------
    // SME Collateral Errors (300..309)
    // ----------------------------------------------------------------------------
    /// Returned when clearing collateral is attempted but no collateral is
    /// recorded.
    NoCollateralToClear = 300,

    // ----------------------------------------------------------------------------
    // Pause Configuration & Rate-Limit Errors (230..239)
    // ----------------------------------------------------------------------------
    /// `LiquifactEscrow::set_pause_max_duration` received a duration outside
    /// `MIN_PAUSE_MAX_DURATION_SECS`..=[`MAX_PAUSE_MAX_DURATION_SECS`. Zero is always allowed.
    PauseMaxDurationOutOfRange = 230,
    /// `LiquifactEscrow::set_pause_rate_limit` received a toggle limit outside
    /// `MIN_PAUSE_TOGGLE_LIMIT`..=[`MAX_PAUSE_TOGGLE_LIMIT`. Zero is allowed only with zero window.
    PauseToggleLimitOutOfRange = 231,
    /// `LiquifactEscrow::set_pause_rate_limit` received a window outside
    /// `MIN_PAUSE_TOGGLE_WINDOW_SECS`..=[`MAX_PAUSE_TOGGLE_WINDOW_SECS`. Zero is allowed only with zero toggles.
    PauseToggleWindowOutOfRange = 232,
    /// `LiquifactEscrow:set_pause_rate_limit` received an inconsistent configuration:
    /// nonzero toggles must have a nonzero window, and nonzero window must have nonzero toggles.
    PauseRateLimitInvalidCombination = 233,
    /// `LiquifactEscrow::set_paused` blocked because the admin has exceeded the configured pause toggle rate limit.
    PauseToggleRateLimitExceeded = 234,

    // ----------------------------------------------------------------------------
    // Fee Schedule Errors (240..249)
    // ----------------------------------------------------------------------------
    /// `LiquifactEscrow::set_fee_schedule` received a fee outside the schedule's declared min/max bounds.
    FeeScheduleOutOfBounds = 240,
    /// `LiquifactEscrow::set_fee_schedule` attempted to create a second pending schedule before the first activates.
    FeeCheduleAlreadyPending = 241,
    /// `LiquifactEscrow:set_fee_schedule` received an activation ledger in the past.
    FeeCheduleInvalidActivation = 242,
    /// `LiquifactEscrow::set_fee_schedule` attempted to submit a schedule identical to the active schedule.
    FeeScheduleSameAsActive = 243,
    /// Returned when the funding token scale is invalid (non-positive or out of range).
    FundingTokenScaleInvalid = 244,
    /// Returned when a funding token scale is required but has not been set.
    FundingTokenScaleNotSet = 245,
}
