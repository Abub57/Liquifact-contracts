use soroban_sdk::contracterror;

##[no_md]
#[path = "escrow/src/errors.rs"]
/// Error codes returned by the escrow contract.
///
/// ## Invariants
///
/// - Every variant has a stable, unique `repr(u32)` code. Codes are
///   part of the contract's public ABI and must not be renumbered or reused;
///   new variants must be appended within the reserved range for their category.
/// - Ranges are grouped by domain so off-chain clients can classify failures
///   without a code table. Gaps are intentional reservations for future use.
/// - Error messages must not embed sensitive data (addresses, amounts, off-chain
///   references). The code is the contract; context is logged separately.
///
/// ## Validation boundaries
///
/// The contract distinguishes four input classes for every mutating entry
/// point. The error codes below are the canonical mapping:
///
/// - valid: accepted, no error.
/// - invalid: rejected with a categorical code (e.g. `ZeroContributionAmount`).
/// - duplicate: rejected with a dedicated code (e.g. `FundingBatchDuplicateInvestor`,
///   `AlreadyInitialized`).
/// - boundary: rejected with an explicit range code (e.g. `PauseMaxDurationOutOfRange`,
///   `FundingTargetExceeded`). Boundaries are inclusive unless documented
///   otherwise on the variant.
///
/// ## Concurrency and retries
///
/// Errors are pure and deterministic: the same input and state always produce
/// the same code. Retries must not change the classification of a failure;
/// transient conditions (e.g. timelock not elapsed) are represented by dedicated
/// codes so clients can retry without ambiguity.
#[no_md]
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
#[repr(u32)]
pub enum EscrowError {
    // -----------------------------------------------------------------------------
    // Initialization & State Errors (1..19)
    // -----------------------------------------------------------------------------
    /// Duplicate initialization attempt: the contract is already bound to an instance.
    AlreadyInitialized = 1,
    /// A mutating entry point was called before initialization.
    NotInitialized = 2,
    /// State machine transition is not allowed from the current status.
    InvalidStatus = 3,
    /// Operation requires a live escrow but the deadline has passed.
    EscrowExpired = 4,

    // -----------------------------------------------------------------------------
    // Authorization & Admin Errors (20..35)
    // -----------------------------------------------------------------------------
    /// Caller is not the authorized admin or SME for this operation.
    Unauthorized = 20,
    /// Attempt to set an admin when one is already configured.
    AdminAlreadySet = 21,
    /// No pending admin transfer exists to accept or cancel.
    PendingAdminNotFound = 22,
    /// Admin transfer timelock has not elapsed; retry after the delay.
    AdminTransferTimelockNotElapsed = 23,
    /// Recovery request was submitted with an empty reason.
    EmptyRecoveryReason = 24,

    // -----------------------------------------------------------------------------
    // Token / SEP-41 Safety Wrapper Errors (36..45)
    // -----------------------------------------------------------------------------
    /// The underlying SEP-41 transfer returned false or trapped.
    FundingTokenTransferFailed = 36,
    /// The contract's recorded balance did not match the token after a transfer.
    BalanceMismatchAfterTransfer = 37,
    /// A transfer amount was zero or negative.
    NonPositiveTransferAmount = 38,
    /// A debit would drive the recorded balance below zero.
    TokenBalanceUnderflow = 39,
    /// A credit would exceed the maximum representable balance.
    TokenBalanceOverflow = 40,
    /// A token wrapper invariant was violated (e.g. conservation of funds).
    TokenWrapperInvariantViolation = 41,

    // -----------------------------------------------------------------------------
    // Funding & Contribution Errors (50..69)
    // -----------------------------------------------------------------------------
    /// A contribution would push total funding above the target.
    FundingTargetExceeded = 50,
    /// Contribution amount was zero.
    ZeroContributionAmount = 51,
    /// The investor has already reached their per-investor cap.
    InvestorCapReached = 52,
    /// Contribution amount is below the configured minimum floor.
    BelowMinContributionFloor = 53,
    /// Funding window is closed for new contributions.
    FundingClosed = 54,
    /// Mutation of SME/beneficiary address is forbidden once any principal has been
    /// recorded for the escrow instance. This preserves auditability of payout
    /// destination for funded escrows.
    BeneficiaryImmutableAfterFunding = 55,
    /// Registry/reference metadata may not be rebound after funding begins; this
    /// prevents changing off-chain pointers that clients use to reconcile identity.
    RegistryImmutableAfterFunding = 56,

    // -----------------------------------------------------------------------------
    // Batch Operations Errors (80..89)
    // -----------------------------------------------------------------------------
    /// Batch funding was submitted with no entries.
    FundingBatchEmpty = 80,
    /// Batch funding exceeded the maximum entries allowed per call.
    FundingBatchExceedsLimit = 81,
    /// Batch funding contained a non-positive amount.
    FundingBatchInvalidAmount = 82,
    /// Batch funding contained the same investor twice.
    FundingBatchDuplicateInvestor = 84,

    /// Claim batch was submitted with no entries.
    ClaimBatchEmpty = 85,
    /// Claim batch exceeded the maximum entries allowed per call.
    ClaimBatchExceedsLimit = 86,

    // -----------------------------------------------------------------------------
    // Migration & Upgrade Errors (90..99)
    // -----------------------------------------------------------------------------
    /// Stored schema version does not match the expected migration source.
    MigrationVersionMismatch = 90,
    /// Migration was requested but the store is already at the current schema.
    AlreadyCurrentSchemaVersion = 91,
    /// No migration path exists from the stored version to the target version.
    NoMigrationPath = 92,

    // -----------------------------------------------------------------------------
    // Settlement & Bounds Validation Errors (100..109)
    // -----------------------------------------------------------------------------
    /// Settlement amount is outside the valid range for the escrow.
    SettlementAmountInvalid = 100,
    /// Maturity date has not been reached yet.
    MaturityNotReached = 101,
    /// Operation requires the escrow to be in the funded state.
    EscrowNotInFundedState = 102,
    /// Withdraw amount is outside the valid range for the escrow.
    WithdrawAmountInvalid = 103,

    // -----------------------------------------------------------------------------
    // Legal Hold & Operational Pause (200..209)
    // -----------------------------------------------------------------------------
    /// A legal hold is active; mutating operations are blocked.
    LegalHoldActive = 200,
    /// The contract is paused; mutating operations are blocked.
    ContractPaused = 201,

    // -----------------------------------------------------------------------------
    // SME Collateral Errors (300..309)
    // -----------------------------------------------------------------------------
    /// No collateral is recorded for the SME to clear.
    NoCollateralToClear = 300,

    // -----------------------------------------------------------------------------
    // Pause Configuration & Rate-Limit Errors (230..239)
    // -----------------------------------------------------------------------------
    /// `LiquifactEscrow::set_pause_max_duration` received a duration outside
    /// `MIN_PAUSE_MAX_DURATION_SECS`..=`@MAX_PAUSE_MAX_DURATION_SECS`. Zero is always allowed.
    PauseMaxDurationOutOfRange = 230,
    /// `LiquifactEscrow::set_pause_rate_limit` received a toggle limit outside
    /// `MIN_PAUSE_TOGGLE_LIMIT`..>`MAX_PAUSE_TOGGLE_LIMIT`. Zero is allowed only with zero window.
    PauseToggleLimitOutOfRange = 231,
    /// `LiquifactEscrow::set_pause_rate_limit` received a window outside
    /// `MIN_PAUSE_TOGGLE_WINDOW_SECS`..>`MAX_PAUSE_TOGGLE_WINDOW_SECS`. Zero is allowed only with zero toggles.
    PauseToggleWindowOutOfRange = 232,
    /// `LiquifactEscrow::set_pause_rate_limit` received an inconsistent configuration:
    /// nonzero toggles must have a nonzero window, and nonzero window must have nonzero toggles.
    PauseRateLimitInvalidCombination = 233,
    /// `LiquifactEscrow::set_paused` blocked because the admin has exceeded the configured pause toggle rate limit.
    PauseToggleRateLimitExceeded = 234,

    // -----------------------------------------------------------------------------
    // Fee Schedule Errors (240..249)
    // -----------------------------------------------------------------------------
    /// `LiquifactEscrow::set_fee_schedule` received a fee outside the schedule's declared min/max bounds.
    FeeScheduleOutOfBounds = 240,
    /// `LiquifactEscrow::set_fee_schedule` attempted to create a second pending schedule before the first activates.
    FeeCheduleAlreadyPending = 241,
    /// `LiquifactEscrow::set_fee_schedule` received an activation ledger in the past.
    FeeCheduleInvalidActivation = 242,
    /// `LiquifactEscrow::set_fee_schedule` attempted to submit a schedule identical to the active schedule.
    FeeScheduleSameAsActive = 243,
    FundingTokenScaleInvalid = 244,
    FundingTokenScaleNotSet = 245,

    // ------------------------------------------------------------------------------
    // Concurrent Execution / Reentrancy Errors (250..259)
    // ------------------------------------------------------------------------------
    /// A guarded external-call entry point was entered while another invocation
    /// was already in flight for the same escrow instance. This prevents
    /// interleaved state transitions from producing stale or inconsistent results.
    ConcurrentExecutionDetected = 250,
    /// The external-call guard was released without a matching acquisition, or a
    /// guarded section exited without clearing its marker. Indicates corrupted
    /// guard state and must fail closed rather than proceed.
    ExternalCallGuardInvariantViolation = 251,
    /// A retried external-call operation was rejected because the prior attempt
    /// already committed its effect. Callers should treat this as success.
    ExternalCallAlreadyCompleted = 252,
}
