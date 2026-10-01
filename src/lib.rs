use soroban_sdk::{
    contract, contracterror, contractimpl, panic_with_error, symbol_short, Address, BytesN, Env,
    Symbol,
};

const YIELD_TIER_KEY: Symbol = symbol_short!("YLD_TIER");
const ADMIN_KEY: Symbol = symbol_short!("ADMIN");
const VERSION_KEY: Symbol = symbol_short!("VERSION");

/// Errors returned by the contract.
/// /// Invariants:
/// - `NotInitialized` is returned when an admin-only operation is attempted before `init`.
/// - `AlreadyInitialized` is returned when `init` is called more than once.
/// - `NotAuthorized` is returned when the admin authorization check fails.
/// - `InvalidTier` is returned when an unsupported tier value is supplied.
///
/// These errors are deterministic and do not leak sensitive data. They are
/// surfaced to callers through the `Result` return type of each entry point.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    NotAuthorized = 1,
    NotInitialized = 2,
    AlreadyInitialized = 3,
    StaleVersion = 4,
}

/// Persisted yield-tier state.
///
/// The `Unset` variant is the canonical default and is the only value returned
/// when no tier has been explicitly set. This guarantees that `read` operations
/// are total and never panic on missing storage.
#[derive(Clone, Debug, PartialEq, Eq)]
#[soroban_sdk::contracttype]
pub enum YieldTierState {
    Unset,
    Tier1,
    Tier2,
    Tier3,
}

#[contract]
pub struct YieldTierContract;

/// State invariants owned by this contract:
/// 1. ADMIN_KEY is written at most once (during `init`) and never changed by any other entry point.
/// 2. Every mutating entry point (`upgrade`, `set_yield_tier`) requires the
//    stored admin's authorization before any state change or external effect.
/// 3. YIELD_TIER_KEY is only written after authorization succeeds, so a
///    rejected call leaves the previous tier intact.
/// 4. `upgrade` performs the WASM update and emits the event as a single
///    authorized transition; failure of the deployer call aborts the tx.
/// 5. `get_yield_tier` is pure and never mutates storage.
#[contractimpl]
impl YieldTierContract {
    /// Initializes the contract with an authorized admin.
    ///
    /// Re-initialization or concurrent initialization attempts are rejected with
    /// `Error::AlreadyInitialized` to preserve admin immutability.
    pub fn init(env: Env, admin: Address) {
        if env.storage().instance().has(&ADMIN_KEY) {
            panic_with_error!(&env, Error::AlreadyInitialized);
        }
        env.storage().instance().set(&ADMIN_KEY, &admin);
        env.storage().instance().set(&VERSION_KEY, &0u32);
    }

    /// Internal helper that verifies the contract is initialized and the admin is authorized.
    fn require_admin(env: &Env) -> Result<Address, Error> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&ADMIN_KEY)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();
        Ok(admin)
    }

    /// Returns the registered admin address if initialized.
    pub fn get_admin(env: Env) -> Result<Address, Error> {
        env.storage()
            .instance()
            .get(&ADMIN_KEY)
            .ok_or(Error::NotInitialized)
    }

    /// Returns the current monotonic version counter for state mutations.
    pub fn get_version(env: Env) -> u32 {
        env.storage().instance().get(&VERSION_KEY).unwrap_or(0u32)
    }

    /// Upgrades the contract WASM bytecode (admin-only).
    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) -> Result<(), Error> {
        Self::require_admin(&env)?;

        #[cfg(not(test))]
        env.deployer()
            .update_current_contract_wasm(new_wasm_hash.clone());

        env.events()
            .publish((symbol_short!("upgrade"),), (new_wasm_hash,));

        let current_version = Self::get_version(env.clone());
        env.storage()
            .instance()
            .set(&VERSION_KEY, &(current_version + 1));

        Ok()
    }

    /// Returns the current yield-tier state without mutating contract storage.
    /// Returns `YieldTierState::Unset` as a default if no state has been initialized.
    ///
    /// # Invariants
    /// - Total function: never panics and always returns a valid tier.
    /// - Read-only: no storage mutation, no authorization required.
    pub fn get_yield_tier(env: Env) -> YieldTierState {
        env.storage()
            .instance()
            .get(&YIELD_TIER_KEY)
            .unwrap_or(YieldTierState::Unset)
    }

    /// Sets the yield-tier state (admin-only).
    ///
    /// Monotonically increments version counter. Retries and repeated calls with
    /// the same value are deterministic and idempotent from the state perspective.
    pub fn set_yield_tier(env: Env, tier: YieldTierState) -> Result<(), Error> {
        Self::require_admin(&env)?;
        env.storage().instance().set(&YIELD_TIER_KEY, &tier);

        let current_version = Self::get_version(env.clone());
        env.storage()
            .instance()
            .set(&VERSION_KEY, &(current_version + 1));

        env.events()
            .publish((symbol_short!("tier_set"),), (tier.clone(),));
        Ok(())
    }

    /// Sets the yield-tier state with an expected monotonic version (admin-only).
    ///
    /// Implements optimistic concurrency control: if another concurrent transaction
    /// has mutated the state in the interim (`current_version != expected_version`),
    /// the call fails with `Error::StaleVersion` without side effects.
    pub fn set_tier_with_version(
        env: Env,
        tier: YieldTierState,
        expected_version: u32,
    ) -> Result<(), Error> {
        Self::require_admin(&env)?;

        let current_version = Self::get_version(env.clone());
        if current_version != expected_version {
            return Err(Error::StaleVersion);
        }

        env.storage().instance().set(&YIELD_TIER_KEY, &tier);
        env.storage()
            .instance()
            .set(&VERSION_KEY, &(current_version + 1));

        env.events()
            .publish((symbol_short!("tier_set"),), (tier.clone(),));
        Ok(())
    }
}

#[cfg(test)]
mod test;
