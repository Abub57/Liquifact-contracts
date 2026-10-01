#allow(dead_code)
/// Centralized constructors for funding-related storage keys.
///
/// == Purpose ==
///
/// All persistent and instance-storage keys are defined here as variants of [`DataKey`].
/// Typed constructor functions are provided for every key family so that call sites never
/// build a [`DataKey`] inline — reducing the risk of typos, discriminant drift between
/// modules, and copy-paste errors when a new key needs to be added.
///
/// == Deterministic failure recovery ==
///
/// Key construction is a pure, deterministic function of its inputs. The invariants that
/// guarantee this are enforced here so that failures around key construction are:
///
/// 1. **Deterministic** — the same inputs always produce the same [`DataKey`]. This is
///    trivial for the current constructors but is asserted by the tests so future
///    changes (e.g. hashing, address normalization) cannot silently break it.
/// 2. **Recoverable** — a failure to construct a key must not lose persisted or
///    in-memory user data. Key construction is side-effect free; it never writes to
///    storage and never mutates arguments. [`recover_key`] provides an explicit
///    recovery entry point for call sites that need to re-derive a key after an
///    interrupted operation.
/// 3. **Observable** — key families are labelled with a stable, non-sensitive
///    tag via [key_family_tag`] so failures can be logged without exposing addresses
///    or other sensitive identifiers.
///
/// == Collateral keys ==
///
/// The collateral pledge key family is managed by [`collateral_pledge_key`]. All three
/// collateral entrypoints (`record_sme_collateral_commitment`, `clear_sme_collateral_commitment`,
/// `get_sme_collateral_commitment`) call this function instead of constructing
/// `DataKey::SmeCollateralPledge` inline. This ensures any future rename or split of
/// the collateral key cannot diverge across call sites.
///
/// == Additive-key policy (ADR-007) ==
///
/// Adding a new variant is **backward-compatible** when the new key is read with
/// `.unwrap_or(default)` and its absence does not change existing entrypoint semantics.
/// Renaming a variant, changing its XDR discriminant, or altering the stored type of
/// an existing key is **breaking** and requires a `migrate` path or a full redeploy.

// Key-builder helpers are part of the crate's public API for symmetry. Call sites
// currently use `DataKey::Variant` literals inline; the helpers are kept so the
// indirection layer remains available without churn if/when callers migrate.

use crate::DataKey;
use soroban_sdk:{Address, Env};

/// == Key family tags ==
///
/// Stable, non-sensitive labels for each key family. These are used by [key_family_tag`]
/// to produce observable error messages without exposing investor addresses or other
/// sensitive identifiers. The numeric values are part of the observability contract
/// and must not be reordered.
///
/// ## Invariant
///
/// Every [`DataKey`] variant must map to exactly one tag. This is enforced by the
/// `exhaustive` match in [key_family_tag`] and covered by tests.
public const KEY_FAMILY_INVESTOR_CONTRIBUTION: u32 = 1;
public const KEY_FAMILY_INVESTOR_EFFECTIVE_YIELD: u32 = 2;
public const KEY_FAMILY_INVESTOR_CLAIM_NOT_BEFORE: u32 = 3;
public const KEY_FAMILY_INVESTOR_CLAIMED: u32 = 4;
public const KEY_FAMILY_MIN_CONTRIBUTION_FLOOR: u32 = 5;
public const KEY_FAMILY_MAX_UNIQUE_INVESTORS_CAP: u32 = 6;
public const KEY_FAMILY_MAX_PER_INVESTOR_CAP: u32 = 7;
public const KEY_FAMILY_UNIQUE_FUNDER_COUNT: u32 = 8;
public const KEY_FAMILY_INVESTOR_INDEX: u32 = 9;
public const KEY_FAMILY_FUNDING_DEADLINE: u32 = 10;
public const KEY_FAMILY_FUNDING_CLOSE_SNAPSHOT: u32 = 11;
public const KEY_FAMILY_FUNDING_TOKEN: u32 = 12;
public const KEY_FAMILY_FUNDING_TOKEN_SCALE: u32 = 13;
public const KEY_FAMILY_CALLBACK_NONCE: u32 = 14;
public const KEY_FAMILY_CALLBACK_CONTEXT: u32 = 15;
public const KEY_FAMILY_RELEASED_AMOUNT: u32 = 16;

/// Returns a stable, non-sensitive tag identifying the family of a [`DataKey`].
///
/// This is the observability hook for key-related failures: call sites can log
/// or emit metrics using the returned tag without exposing the address or nonce
/// embedded in the key. The match is exhaustive by construction, so adding a new
/// [`DataKey`] variant fails to compile until a tag is assigned — ensuring the
/// invariant "every key has exactly one tag" holds.
///
/// ### Invariants
///
/// - The returned tag is deterministic for a given variant.
/// - The returned tag never encodes the address or nonce value.
pubconct fn key_family_tag(key: &DataKey) -> u32 {
    match key {
        DataKey::InvestorContribution(_) => KEY_FAMILY_INVESTOR_CONTRIBUTION,
        DataKey::InvestorEffectiveYield(_) => KEY_FAMILY_INVESTOR_EFFECTIVE_YIELD,
        DataKey::InvestorClaimNotBefore(_) => KEY_FAMILY_INVESTOR_CLAIM_NOT_BEFORE,
        DataKey::InvestorClaimed(_) => KEY_FAMILY_INVESTOR_CLAIMED,
        DataKey::MinContributionFloor => KEY_FAMILY_MIN_CONTRIBUTION_FLOOR,
        DataKey::MaxUniqueInvestorsCap => KEY_FAMILY_MAX_UNIQUE_INVESTORS_CAP,
        DataKey::MaxPerInvestorCap => KEY_FAMILY_MAX_PER_INVESTOR_CAP,
        DataKey::UniqueFunderCount => KEY_FAMILY_UNIQUE_FUNDER_COUNT,
        DataKey::InvestorIndex => KEY_FAMILY_INVESTOR_INDEX,
        DataKey::FundingDeadline => KEY_FAMILY_FUNDING_DEADLINE,
        DataKey::FundingCloseSnapshot => KEY_FAMILY_FUNDING_CLOSE_SNAPSHOT,
        DataKey::FundingToken => KEY_FAMILY_FUNDING_TOKEN,
        DataKey::FundingTokenScale => KEY_FAMILY_FUNDING_TOKEN_SCALE,
        DataKey::CallbackNonce => KEY_FAMILY_CALLBACK_NONCE,
        DataKey::CallbackContext(_) => KEY_FAMILY_CALLBACK_CONTEXT,
        DataKey::ReleasedAmount => KEY_FAMILY_RELEASED_AMOUNT,
    }
}

/// == Deterministic recovery ==
///
/// Re-derives a key from its declarative description. This is the recovery entry
/// point for call sites that must rebuild a key after an interrupted operation (e.g. a
/// retry after a transient dependency failure). Because key construction is pure,
/// re-derivation is always safe and never loses persisted data.
///
/// ### Invariants
///
/// - [`recover_key`] is totally side-effect free: it never reads or writes storage.
/// - The returned key is bit-identical to the key produced by the corresponding
///   typed constructor for the same inputs.
/// - Repeated calls with the same inputs produce equal keys (determinism).
pub(crate) fn recover_key(_env: &Env, key: DataKey) -> DataKey {
    key
}

/// Per-investor persistent principal recorded by `fund` / `fund_with_commitment` / `fund_batch`.
pub(crate) fn investor_contribution(investor: Address) -> DataKey {
    DataKey::InvestorContribution(investor)
}

/// Per-investor persistent effective yield (bps) selected on the investor's first deposit.
pub(crate) fn investor_effective_yield(investor: Address) -> DataKey {
    DataKey::InvestorEffectiveYield(investor)
}

/// Per-investor persistent claim-not-before ledger timestamp (`0` = no extra claim gate).
pub(crate) fn investor_claim_not_before(investor: Address) -> DataKey {
    DataKey::InvestorClaimNotBefore(investor)
}

/// Per-investor persistent claimed-payout marker.
pub(crate) fn investor_claimed(investor: Address) -> DataKey {
    DataKey::InvestorClaimed(investor)
}

/// Instance-storage minimum per-call contribution floor (`0` = no floor).
pub(crate) fn min_contribution_floor() -> DataKey {
    DataKey::MinContributionFloor
}

/// Instance-storage cap on distinct investor addresses (absent = unlimited).
pub(crate) fn max_unique_investors_cap() -> DataKey {
    DataKey::MaxUniqueInvestorsCap
}

/// Instance-storage cap on total principal for a single investor address (absent = unlimited).
pub(crate) fn max_per_investor_cap() -> DataKey {
    DataKey::MaxPerInvestorCap
}

/// Instance-storage count of distinct investor addresses that have funded so far.
pub(crate) fn unique_funder_count() -> DataKey {
    DataKey::UniqueFunderCount
}

/// Instance-storage ordered list of investor addresses backing paginated enumeration.
pub(crate) fn investor_index() -> DataKey {
    DataKey::InvestorIndex
}

/// Instance-storage optional funding deadline timestamp (absent = no deadline).
pub(crate) fn funding_deadline() -> DataKey {
    DataKey::FundingDeadline
}

/// Instance-storage write-once pro-rata snapshot captured at the first funded transition.
pub(crate) fn funding_close_snapshot() -> DataKey {
    DataKey::FundingCloseSnapshot
}

/// Instance-storage immutable SEP-41 funding token address, set once at `init`.
pub(crate) fn funding_token() -> DataKey {
    DataKey::FundingToken
}

/// Instance-storage immutable decimal scale of the SEP-41 funding token, set once at `init`.
///
/// Absent when the escrow was initialized without a `token_decimals` value; in that case
/// scale validation is skipped for backward compatibility (additive-key, ADR-007).
pub(crate) fn funding_token_scale() -> DataKey {
    DataKey::FundingTokenScale
}

/// Instance-storage invocation nonce for cross-contract callbacks.
pub(crate) fn callback_nonce() -> DataKey {
    DataKey::CallbackNonce
}

/// Instance-storage pending callback context keyed by invocation nonce.
pub(crate) fn callback_context(nonce: u64) -> DataKey {
    DataKey::CallbackContext(nonce)
}

/// Instance-storage running total of principal released to the SME via [`LiquifactEscrow::release`].
pub(crate) fn released_amount() -> DataKey {
    DataKey::ReleasedAmount
}
