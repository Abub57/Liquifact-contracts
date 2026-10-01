use soroban_sdk::{contracterror, contractimpl, symbol_short, Address, BytesN, Env, Symbol};

const YIELD_TIER_KEY: Symbol = symbol_short!("YLD_TIER");
const ADMIN_KEY: Symbol = symbol_short!("ADMIN");

/// Errors returned by the contract.
/// These are deterministic and do not leak sensitive data.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    NotAuthorized = 1,
    AlreadyInitialized = 2,
    NotInitialized = 3,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[soroban_sdk::contracttype]
pub enum YieldTierState {
    Unset,
    Tier1,
    Tier2,
    Tier3,
}

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
    /// Initializes the admin. Idempotent on repeated calls: the second call
    /// fails with `AlreadyInitialized` and does not overwrite the existing admin.
    pub fn init(env: Env, admin: Address) -> Result<(), Error> {
        if env.storage().instance().has(&ADMIN_KEY) {
            return Err(Error::AlreadyInitialized);
        }
        env.storage().instance().set(&ADMIN_KEY, &admin);
        Ok()
    }

    /// Returns the current admin address, if initialized.
    pub fn get_admin(env: Env) -> Result<Address, Error> {
        env.storage()
            .instance()
            .get(&ADMIN_KEY)
            .ok_or(Error::NotInitialized)
    }

    /// Upgrades the contract WASM. Admin-only.
    /// The authorization check runs before the deployer call so a rejected
    /// call cannot mutate the contract code or emit an event.
    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) -> Result<(), Error> {
        let admin: Address = env.storage()
            .instance()
            .get(&ADMIN_KEY)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();

        env.deployer().update_current_contract_wasm(new_wasm_hash.clone());
        env.events().publish((symbol_short!("upgrade"),), (new_wasm_hash.clone(),));

        Ok()
    }

    /// Returns the current yield-tier state without mutating contract storage.
    /// Returns `YieldTierState::Unset` as a default if no state has been initialized.
    pub fn get_yield_tier(env: Env) -> YieldTierState {
        env.storage()
            .instance()
            .get(&YIELD_TIER_KEY)
            .unwrap_or(YieldTierState::Unset)
    }

    /// Sets the yield-tier state (admin-only).
    /// The authorization check runs before the storage write and event so a
    /// rejected call cannot change the stored tier or emit an event.
    pub fn set_yield_tier(env: Env, tier: YieldTierState) -> Result<(), Error> {
        let admin: Address = env.storage()
            .instance()
            .get(&ADMIN_KEY)
            .ok_or(Error::NotInitialized)?;
        admin.require_auth();
        env.storage().instance().set(&YIELD_TIER_KEY, &tier);
        env.events().publish((symbol_short!("tier_set"),), (tier.clone(),));
        Ok(()
    }
}
