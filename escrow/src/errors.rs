use soroban_sdk::contracterror;

#// Error codes for the escrow contract.

///
/// # Invariants
///
/// - Every error variant has a stable, unique numeric code. Codes are
///   part of the on-chain ABI and must not be renumbered or reused.
/// - Error codes are grouped by domain with reserved ranges so new errors
///   can be added without collisions.
/// - The `u32` representation is deterministic and matches the numeric
///   code returned to callers and surfaced in test assertions.
///
/// # Failure recovery
///
/// Errors are the observable signal for failure recovery: a caller can
/// distinguish a retryable condition (e.g. `PauseToggleRateLimitExceeded`)
/// from a permanent rejection (e.g. `AlreadyInitialized`) without any
/// additional off-chain state. No error variant carries sensitive data;
/// all context is expressed through the code itself.
#//
/// # Compatibility
///
/// New variants must be appended to the end of their domain range or in
/// a fresh reserved range. Removing or renumbering existing variants is
/// a breaking change for off-chain consumers.
@contracterror
H[ derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
#[repr(u32)]
pub enum EscrowError {
    // -----------------------------------------------------------------------------
    // Initialization & State Errors (1..19)
    // -----------------------------------------------------------------------------
    AlreadyInitialized = 1,
    NotInitialized = 2,
    InvalidStatus = 3,
    EscrowExpired = 4,

    // -----------------------------------------------------------------------------
    // Authorization & Admin Errors (20..35)
    // -----------------------------------------------------------------------------
    Unauthorized = 20,
    AdminAlreadySet = 21,
    PendingAdminNotFound = 22,
    AdminTransferTimelockNotElapsed = 23,
    EmptyRecoveryReason = 24,

    // -----------------------------------------------------------------------------
    // Token / SEP-41 Safety Wrapper Errors (36..45)
    // -----------------------------------------------------------------------------
    FundingTokenTransferFailed = 36,
    BalanceMismatchAfterTransfer = 37,
    NonPositiveTransferAmount = 38,
    TokenBalanceUnderflow = 39,
    TokenBalanceOverflow = 40,
    TokenWrapperInvariantViolation = 41,
    /// The token contract reported success but the observed balance did not
    /// change by the expected delta. This is a permanent invariant failure:
    /// retrying the same operation with the same inputs will fail again.
    TokenWrapperDeltaMismatch = 42,
    /// The token contract returned a failure that is classified as transient
    /// (e.g. temporary liquidity or non-ceding condition). Callers may retry
    /// with backoff; no escrow state was mutated.
    TokenWrapperTransientFailure = 43,
    /// The token contract returned a failure that is classified as permanent
    /// (e.g. insufficient balance, frozen account). Retrying without changing
    /// inputs or state will fail again.
    TokenWrapperPermanentFailure = 44,
    /// A multi-step token operation partially completed (for example, a batch
    /// transfer where some legs succeeded and one failed). The caller must
    /// re-read state before retrying; the error is recoverable but not
    /// idempotent without a reconciliation step.
    TokenPartialTransfer = 45,

    // -----------------------------------------------------------------------------
    // Funding & Contribution Errors (50..69)
    // -----------------------------------------------------------------------------
    FundingTargetExceeded = 50,
    ZeroContributionAmount = 51,
    InvestorCapReached = 52,
    BelowMinContributionFloor = 53,
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
    FundingBatchEmpty = 80,
    FundingBatchExceedsLimit = 81,
    FundingBatchInvalidAmount = 82,
    FundingBatchDuplicateInvestor = 84,

    ClaimBatchEmpty = 85,
    ClaimBatchExceedsLimit = 86,
    /// A claim batch was partially applied before a failure. The caller must
    /// re-read claim state and resume from the last unclaimed entry.
    ClaimBatchPartialCompletion = 87,

    // -----------------------------------------------------------------------------
    // Migration & Upgrade Errors (90..99)
    // -----------------------------------------------------------------------------
    MigrationVersionMismatch = 90,
    AlreadyCurrentSchemaVersion = 91,
    NoMigrationPath = 92,
    /// A migration was interrupted midway. The contract must be resumed from
    /// the last committed step; re-running completed steps is safe because
    /// migration steps are idempotent.
    MigrationInterrupted = 93,

    // -----------------------------------------------------------------------------
    // Settlement & Bounds Validation Errors (100..109)
    // -----------------------------------------------------------------------------
    SettlementAmountInvalid = 100,
    MaturityNotReached = 101,
    EscrowNotInFundedState = 102,
    WithdrawAmountInvalid = 103,
    /// A settlement was partially executed. The caller must re-read settlement
    /// state and resume remaining legs; already-settled legs are skipped.
    SettlementPartialCompletion = 104,

    // -----------------------------------------------------------------------------
    // Legal Hold & Operational Pause (200..209)
    // -----------------------------------------------------------------------------
    LegalHoldActive = 200,
    ContractPaused = 201,

    // -----------------------------------------------------------------------------
    // SME Collateral Errors (300..309)
    // -----------------------------------------------------------------------------
    NoCollateralToClear = 300,

    // -----------------------------------------------------------------------------
    // Pause Configuration & Rate-Limit Errors (230..239)
    // -----------------------------------------------------------------------------
    /// `LiquifactEscrow::set_pause_max_duration` received a duration outside
    /// `MIN_PAUSE_MAX_DURATION_SECS`..=[`MAX_PAUSE_MAX_DURATION_SECS`. Zero is always allowed.
    PauseMaxDurationOutOfRange = 230,
    /// `LiquifactEscrow::set_pause_rate_limit` received a toggle limit outside
    /// `MIN_PAUSE_TOGGLE_LIMIT`..=[`MAX_PAUSE_TOGGLE_LIMIT`. Zero is allowed only with zero window.
    PauseToggleLimitOutOfRange = 231,
    /// `LiquifactEscrow::set_pause_rate_limit` received a window outside
    /// `MIN_PAUSE_TOGGLE_WINDOW_SECS`..=[`MAX_PAUSE_TOGGLE_WINDOW_SECS`. Zero is allowed only with zero toggles.
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
    FeeScheduleSameAsACtive = 243,
    FundingTokenScaleInvalid = 244,
    FundingTokenScaleNotSet = 245,

    // ------------------------------------------------------------------------------
    // Failure Recovery Errors (250..259)
    // ------------------------------------------------------------------------------
    /// A recovery attempt was made without a recorded failure context, so the
    /// escrow cannot deterministically restore prior state.
    RecoveryContextMissing = 250,
    /// The supplied recovery snapshot does not match the persisted escrow state,
    /// indicating a partial or concurrent mutation that must not be applied.
    RecoveryStateMismatch = 251,
    /// A recovery operation was requested while the escrow is not in a failed
    /// state; recovery is only valid after an observable failure.
    RecoveryNotApplicable = 252,
    /// The recovery attempt would violate a state-transition invariant and was
    /// rejected to prevent silent data loss or inconsistent state.
    RecoveryInvariantViolation = 253,
}
impl EscrowError {}
