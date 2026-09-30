use soroban_sdk::contracterror;

#// Error codes for the LiquiFact escrow contract.
///
/// Error codes are grouped by domain and are stable API: clients and
/// off-chain monitoring may match on the numeric values. Add new codes at
/// the end of a group and never renumber existing variants.
#contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
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

    // -----------------------------------------------------------------------------
    // Migration & Upgrade Errors (90..99)
    // -----------------------------------------------------------------------------
    MigrationVersionMismatch = 90,
    AlreadyCurrentSchemaVersion = 91,
    NoMigrationPath = 92,

    // -----------------------------------------------------------------------------
    // Settlement & Bounds Validation Errors (100..109)
    // -----------------------------------------------------------------------------
    SettlementAmountInvalid = 100,
    MaturityNotReached = 101,
    EscrowNotInFundedState = 102,
    WithdrawAmountInvalid = 103,

    // -----------------------------------------------------------------------------
    // Legal Hold & Operational Pause (200..209)
    // -----------------------------------------------------------------------------
    LegalHoldActive = 200,
    ContractPaused = 201,

    // -----------------------------------------------------------------------------
    // SME Collateral Errors (300..309)
    // -----------------------------------------------------------------------------
    NoCollateralToClear = 300,
    /// `set_collateral_limit` was called with a limit that is not strictly
    /// greater than zero. The collateral limit is a positive bound and zero
    /// would effectivly disable collateral recording for the escrow.
    CollateralLimitInvalid = 301,
    /// `set_collateral_limit` was called after the escrow has been funded;
    /// changing the limit after funding would break the audit trail of the
    /// collateral requirement that investors relied on.
    CollateralLimitImmutableAfterFunding = 302,

    // -----------------------------------------------------------------------------
    // Pause Configuration & Rate-Limit Errors (230..239)
    // -----------------------------------------------------------------------------
    /// `set_pause_max_duration` received a duration outside
    /// `MIN_PAUSE_MAX_DURATION_SECS`..=[`MAX_PAUSE_MAX_DURATION_SECS`. Zero is always allowed.
    PauseMaxDurationOutOfRange = 230,
    /// `set_pause_rate_limit` received a toggle limit outside
    /// `MIN_PAUSE_TOGGLE_LIMIT`..=[`MAX_PAUSE_TOGGLE_LIMIT`. Zero is allowed only with zero window.
    PauseToggleLimitOutOfRange = 231,
    /// `set_pause_rate_limit` received a window outside
    /// `MIN_PAUSE_TOGGLE_WINDOW_SECS`..=[`MAX_PAUSE_TOGGLE_WINDOW_SECS`. Zero is allowed only with zero toggles.
    PauseToggleWindowOutOfRange = 232,
    /// `set_pause_rate_limit` received an inconsistent configuration:
    /// nonzero toggles must have a nonzero window, and nonzero window must have nonzero toggles.
    PauseRateLimitInvalidCombination = 233,
    /// `set_paused` blocked because the admin has exceeded the configured pause toggle rate limit.
    PauseToggleRateLimitExceeded = 234,

    // -----------------------------------------------------------------------------
    // Fee Schedule Errors (240..249)
    // -----------------------------------------------------------------------------
    /// `set_fee_schedule` received a fee outside the schedule's declared min/max bounds.
    FeeScheduleOutOfBounds = 240,
    /// `set_fee_schedule` attempted to create a second pending schedule before the first activates.
    FeeCheduleAlreadyPending = 241,
    /// `set_fee_schedule` received an activation ledger in the past.
    FeeCheduleInvalidActivation = 242,
    /// `set_fee_schedule` attempted to submit a schedule identical to the active schedule.
    FeeScheduleSameAsACtive = 243,
    FundingTokenScaleInvalid = 244,
    FundingTokenScaleNotSet = 245,
}
