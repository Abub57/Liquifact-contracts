use soroban_sdk::{
    contract, contracterror, contractimpl, panic_with_error, symbol_short, Address, BytesN, Env,
    Symbol,
};

const YIELD_TIER_KEY: Symbol = symbol_short!("YLD_TIER");
const ADMIN_KEY: Symbol = symbol_short!("ADMIN");
const VERSION_KEY: Symbol = symbol_short!("VERSION");

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    NotAuthorized = 1,
    NotInitialized = 2,
    AlreadyInitialized = 3,
    StaleVersion = 4,
}

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

        Ok(())
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
