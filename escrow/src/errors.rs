use soroban_sdk::contracterror;
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
#[repr(u32)]
pub enum EscrowError {
    // ------------------------------------------------------------------------------
    // Initialization & State Errors (1..19)
    // ------------------------------------------------------------------------------
    AlreadyInitialized = 1,
    NotInitialized = 2,
    InvalidStatus = 3,
    EscrowExpired = 4,

    // ------------------------------------------------------------------------------
    // Authorization & Admin Errors (20..35)
    // ------------------------------------------------------------------------------
    Unauthorized = 20,
    AdminAlreadySet = 21,
    PendingAdminNotFound = 22,
    AdminTransferTimelockNotElapsed = 23,
    EmptyRecoveryReason = 24,

    // -------------------------------------------------------------------------------
    // Token / SEP-41 Safety Wrapper Errors (36..45)
    // ------------------------------------------------------------------------------
    FundingTokenTransferFailed = 36,
    BalanceMismatchAfterTransfer = 37,
    NonPositiveTransferAmount = 38,
    TokenBalanceUnderflow = 39,
    TokenBalanceOverflow = 40,
    TokenWrapperInvariantViolation = 41,
    /// A concurrent or replayed mutation was detected against a stale escrow
    /// snapshot. Callers must re-read state and retry idempotently.
    ConcurrentMutationDetected = 42,
    /// A duplicate request was observed for an operation that must execute at
    /// most once per (caller, nonce) pair. Safe to treat as a no-op on retry.
    DuplicateRequest = 43,

    // ------------------------------------------------------------------------------
    // Funding & Contribution Errors (50..69)
    // ------------------------------------------------------------------------------
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
    /// The escrow state changed between the caller's read and the attempted
    /// write. Retry after re-reading; do not assume the prior snapshot is valid.
    StaleStateVersion = 57,

    // -------------------------------------------------------------------------------
    // Batch Operations Errors (80..89)
    // ------------------------------------------------------------------------------
    FundingBatchEmpty = 80,
    FundingBatchExceedsLimit = 81,
    FundingBatchInvalidAmount = 82,
    FundingBatchDuplicateInvestor = 84,
    /// A batch contained the same investor more than once. Rejected to keep
    /// per-investor accounting deterministic under concurrent submissions.
    FundingBatchDuplicateEntry = 83,

    ClaimBatchEmpty = 85,
    ClaimBatchExceedsLimit = 86,
    /// A claim batch contained a duplicate investor entry; rejected to prevent
    /// double-crediting under retried or racing submissions.
    ClaimBatchDuplicateEntry = 87,

    // ------------------------------------------------------------------------------
    // Migration & Upgrade Errors (90..99)
    // ------------------------------------------------------------------------------
    MigrationVersionMismatch = 90,
    AlreadyCurrentSchemaVersion = 91,
    NoMigrationPath = 92,
    /// A migration was attempted while another migration held the schema lock.
    MigrationInProgress = 93,

    // ------------------------------------------------------------------------------
    // Settlement & Bounds Validation Errors (100..109)
    // -------------------------------------------------------------------------------
    SettlementAmountInvalid = 100,
    MaturityNotReached = 101,
    EscrowNotInFundedState = 102,
    WithdrawAmountInvalid = 103,
    /// A settlement or withdrawal was attempted against a superseded state
    /// version. Re-read state and retry; the prior computation is invalid.
    SettlementStaleState = 104,

    // ------------------------------------------------------------------------------
    // Legal Hold & Operational Pause (200..209)
    // ------------------------------------------------------------------------------
    LegalHoldActive = 200,
    ContractPaused = 201,
    /// A pause/unpause toggle raced with another toggle and was rejected to
    /// preserve the configured rate-limit invariant.
    PauseToggleRaced = 202,

    // -------------------------------------------------------------------------------
    // SME Collateral Errors (300..309)
    // ------------------------------------------------------------------------------
    NoCollateralToClear = 300,
    /// Collateral clearing raced with a concurrent mutation; retry after
    /// re-reading collateral state.
    CollateralStaleState = 301,

    // ------------------------------------------------------------------------------
    // Pause Configuration & Rate-Limit Errors (230..239)
    // ------------------------------------------------------------------------------
    /// `LiquifactEscrow::set_pause_max_duration` received a duration outside
    /// `MIN_PAUSE_MAX_DURATION_SECS`..=[`MAX_PAUSE_MAX_DURATION_SECS`. Zero is always allowed.
    PauseMaxDurationOutOfRange = 230,
    /// `LiquifactEscrow::set_pause_rate_limit` received a toggle limit outside
    /// `MIN_PAUSE_TOGGLE_LIMIT`..=[`@MAX_PAUSE_TOGGLE_LIMIT`. Zero is allowed only with zero window.
    PauseToggleLimitOutOfRange = 231,
    /// `LiquifactEscrow::set_pause_rate_limit` received a window outside
    /// `MIN_PAUSE_TOGGLE_WINDOW_SECS`..=[`MAX_PAUSE_TOGGLE_WINDOW_SECS`. Zero is allowed only with zero toggles.
    PauseToggleWindowOutOfRange = 232,
    /// `LiquifactEscrow::set_pause_rate_limit` received an inconsistent configuration:
    /// nonzero toggles must have a nonzero window, and nonzero window must have nonzero toggles.
    PauseRateLimitInvalidCombination = 233,
    /// `LiquifactEscrow::set_paused` blocked because the admin has exceeded the configured pause toggle rate limit.
    PauseToggleRateLimitExceeded = 234,

    // ------------------------------------------------------------------------------
    // Fee Schedule Errors (240..249)
    // ------------------------------------------------------------------------------
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
    /// A fee schedule mutation raced with activation of the pending schedule;
    /// retry after re-reading the active/pending schedule state.
    FeeScheduleStaleState = 246,
}
