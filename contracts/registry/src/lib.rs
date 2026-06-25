#![cfg_attr(not(test), no_std)]
mod test;

use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, symbol_short, Address,
    Bytes, BytesN, Env, String, Vec,
};
use xlm_ns_common::soroban::validate_fqdn_soroban;
use xlm_ns_common::time::{is_active_at, is_claimable_at};
use xlm_ns_common::{DEFAULT_TTL_SECONDS, MAX_METADATA_URI_LENGTH};

pub const ADMIN_RECOVERY_SUPPORTED: bool = false;
pub const STORAGE_SCHEMA_VERSION: u32 = 1;
pub const CONTRACT_VERSION: u32 = 1;

#[contractevent]
pub struct ContractUpgraded {
    pub old_version: u32,
    pub new_version: u32,
    pub admin: Address,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct RegistryEntry {
    pub name: String,
    pub owner: Address,
    pub resolver: Option<String>,
    pub target_address: Option<String>,
    pub metadata_uri: Option<String>,
    pub ttl_seconds: u64,
    pub registered_at: u64,
    pub expires_at: u64,
    pub grace_period_ends_at: u64,
    pub transfer_count: u32,
}

impl RegistryEntry {
    fn is_active_at(&self, now_unix: u64) -> bool {
        is_active_at(self.expires_at, now_unix)
    }

    fn is_claimable_at(&self, now_unix: u64) -> bool {
        is_claimable_at(self.grace_period_ends_at, now_unix)
    }
}

/// Issue #213: Lifecycle state of a name, so callers can branch on the state
/// directly instead of inferring it from `resolve`/`register` errors.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum NameState {
    /// No entry exists for this name.
    Missing,
    /// Registered and not yet expired.
    Active,
    /// Expired but still within the grace period (only the owner may renew).
    GracePeriod,
    /// Past the grace period; anyone may claim/register it.
    Claimable,
}

#[derive(Clone)]
#[contracttype]
enum DataKey {
    Entry(String),
    OwnerNames(Address),
    Admin,
    ContractVersion,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum RegistryError {
    AlreadyRegistered = 1,
    NotFound = 2,
    NotYetClaimable = 3,
    NotActive = 4,
    Unauthorized = 5,
    MetadataTooLong = 6,
    Validation = 7,
    InvalidExpiry = 8,
    InvalidGracePeriod = 9,
    UpgradeFailed = 10,
}

#[contract]
pub struct RegistryContract;

#[contractimpl]
impl RegistryContract {
    pub fn initialize(env: Env, admin: Address) -> Result<(), RegistryError> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(RegistryError::Unauthorized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .persistent()
            .set(&DataKey::ContractVersion, &CONTRACT_VERSION);
        Ok(())
    }

    pub fn version(_env: Env) -> u32 {
        CONTRACT_VERSION
    }

    pub fn get_version(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::ContractVersion)
            .unwrap_or(CONTRACT_VERSION)
    }

    pub fn upgrade(
        env: Env,
        new_wasm_hash: BytesN<32>,
        migration_data: Bytes,
    ) -> Result<(), RegistryError> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(RegistryError::UpgradeFailed)?;
        admin.require_auth();

        let current_version = Self::get_version(env.clone());
        let target_version = decode_target_version(&migration_data);

        for v in current_version..target_version {
            migrate(v, v + 1, &migration_data);
        }

        env.storage()
            .persistent()
            .set(&DataKey::ContractVersion, &target_version);

        ContractUpgraded {
            old_version: current_version,
            new_version: target_version,
            admin,
        }
        .publish(&env);

        env.deployer().update_current_contract_wasm(new_wasm_hash);

        Ok(())
    }

    // Mutating entrypoints require Soroban auth from the address that is
    // authorizing the state change, rather than relying on address equality
    // checks alone.
    //
    // Release policy: this registry does not support admin recovery or forced
    // reassignment. Names can only leave an owner-controlled state through the
    // normal expiry and grace-period flow.
    ///
    /// Registers a new name, setting its initial lifecycle and ownership.
    /// This expects cross-contract authorization from the caller via the Registrar.
    pub fn register(
        env: Env,
        name: String,
        owner: Address,
        target_address: Option<String>,
        metadata_uri: Option<String>,
        now_unix: u64,
        expires_at: u64,
        grace_period_ends_at: u64,
    ) -> Result<(), RegistryError> {
        owner.require_auth();
        validate_fqdn_soroban(&name).map_err(|_| RegistryError::Validation)?;
        validate_metadata(&metadata_uri)?;
        validate_lifecycle_timestamps(now_unix, expires_at, grace_period_ends_at)?;

        let key = DataKey::Entry(name.clone());
        if let Some(existing) = env.storage().persistent().get::<_, RegistryEntry>(&key) {
            if existing.is_active_at(now_unix) {
                return Err(RegistryError::AlreadyRegistered);
            }
            if !existing.is_claimable_at(now_unix) {
                return Err(RegistryError::NotYetClaimable);
            }
            remove_owner_name(&env, &existing.owner, &name);
            env.storage().persistent().remove(&key);

            env.events().publish(
                (symbol_short!("name"), symbol_short!("burn")),
                (name.clone(), existing.owner),
            );
        }

        let entry = RegistryEntry {
            name: name.clone(),
            owner: owner.clone(),
            resolver: None,
            target_address,
            metadata_uri,
            ttl_seconds: DEFAULT_TTL_SECONDS,
            registered_at: now_unix,
            expires_at,
            grace_period_ends_at,
            transfer_count: 0,
        };
        env.storage().persistent().set(&key, &entry);
        add_owner_name(&env, &owner, &name);
        Ok(())
    }

    pub fn resolve(env: Env, name: String, now_unix: u64) -> Result<RegistryEntry, RegistryError> {
        validate_fqdn_soroban(&name).map_err(|_| RegistryError::Validation)?;
        let entry = get_entry(&env, &name)?;
        if !entry.is_active_at(now_unix) {
            return Err(RegistryError::NotActive);
        }
        Ok(entry)
    }

    /// Issue #213: Read-only lifecycle state of a name, distinguishing active,
    /// grace-period, claimable, and missing names without forcing callers to
    /// infer the state from `resolve`/`register` errors. Unknown or invalid
    /// names report as [`NameState::Missing`].
    pub fn name_state(env: Env, name: String, now_unix: u64) -> NameState {
        match env
            .storage()
            .persistent()
            .get::<_, RegistryEntry>(&DataKey::Entry(name))
        {
            None => NameState::Missing,
            Some(entry) => {
                if entry.is_active_at(now_unix) {
                    NameState::Active
                } else if entry.is_claimable_at(now_unix) {
                    NameState::Claimable
                } else {
                    NameState::GracePeriod
                }
            }
        }
    }

    pub fn check_owner(
        env: Env,
        name: String,
        caller: Address,
        now_unix: u64,
    ) -> Result<(), RegistryError> {
        let entry = get_entry(&env, &name)?;
        ensure_owner(&entry, &caller, now_unix)
    }

    pub fn transfer(
        env: Env,
        name: String,
        caller: Address,
        new_owner: Address,
        now_unix: u64,
    ) -> Result<(), RegistryError> {
        caller.require_auth();
        let mut entry = get_entry(&env, &name)?;
        ensure_owner(&entry, &caller, now_unix)?;
        let old_owner = entry.owner.clone();
        entry.owner = new_owner.clone();
        entry.transfer_count = entry.transfer_count.saturating_add(1);
        put_entry(&env, &name, &entry);
        remove_owner_name(&env, &old_owner, &name);
        add_owner_name(&env, &new_owner, &name);
        env.events().publish(
            (symbol_short!("name"), symbol_short!("transfer")),
            (name, old_owner, new_owner),
        );
        Ok(())
    }

    pub fn set_resolver(
        env: Env,
        name: String,
        caller: Address,
        resolver: Option<String>,
        now_unix: u64,
    ) -> Result<(), RegistryError> {
        caller.require_auth();
        let mut entry = get_entry(&env, &name)?;
        ensure_owner(&entry, &caller, now_unix)?;
        entry.resolver = resolver;
        put_entry(&env, &name, &entry);
        Ok(())
    }

    pub fn set_target_address(
        env: Env,
        name: String,
        caller: Address,
        target_address: Option<String>,
        now_unix: u64,
    ) -> Result<(), RegistryError> {
        caller.require_auth();
        let mut entry = get_entry(&env, &name)?;
        ensure_owner(&entry, &caller, now_unix)?;
        entry.target_address = target_address;
        put_entry(&env, &name, &entry);
        Ok(())
    }

    pub fn set_metadata(
        env: Env,
        name: String,
        caller: Address,
        metadata_uri: Option<String>,
        now_unix: u64,
    ) -> Result<(), RegistryError> {
        caller.require_auth();
        validate_metadata(&metadata_uri)?;
        let mut entry = get_entry(&env, &name)?;
        ensure_owner(&entry, &caller, now_unix)?;
        entry.metadata_uri = metadata_uri;
        put_entry(&env, &name, &entry);
        Ok(())
    }

    /// Renews a name by extending its expiry and grace period.
    /// This expects cross-contract authorization from the caller via the
    /// Registrar. Unauthorized attempts (where caller is not the owner) are rejected.
    pub fn renew(
        env: Env,
        name: String,
        caller: Address,
        expires_at: u64,
        grace_period_ends_at: u64,
        now_unix: u64,
    ) -> Result<(), RegistryError> {
        caller.require_auth();
        let mut entry = get_entry(&env, &name)?;
        // Allow renewal for the owner as long as the name has not become
        // claimable (i.e. now <= grace_period_ends_at).
        if entry.is_claimable_at(now_unix) {
            return Err(RegistryError::NotActive);
        }
        if entry.owner != caller {
            return Err(RegistryError::Unauthorized);
        }

        if expires_at < entry.expires_at {
            return Err(RegistryError::InvalidExpiry);
        }
        if grace_period_ends_at < entry.grace_period_ends_at {
            return Err(RegistryError::InvalidGracePeriod);
        }
        validate_lifecycle_timestamps(now_unix, expires_at, grace_period_ends_at)?;

        entry.expires_at = expires_at;
        entry.grace_period_ends_at = grace_period_ends_at;
        put_entry(&env, &name, &entry);
        Ok(())
    }

    pub fn names_for_owner(env: Env, owner: Address) -> Vec<String> {
        env.storage()
            .persistent()
            .get(&DataKey::OwnerNames(owner))
            .unwrap_or(Vec::new(&env))
    }

    /// Returns names present in the owner index that are inconsistent with
    /// persistent storage — either the entry is missing, or its owner field
    /// does not match the queried address.
    ///
    /// A consistent registry always returns an empty vec. Non-empty results
    /// indicate that an external write bypassed the normal registration flow
    /// (e.g. a storage migration gone wrong) and should be investigated before
    /// proceeding.
    pub fn audit_owner_index(env: Env, owner: Address) -> Vec<String> {
        let indexed_names: Vec<String> = env
            .storage()
            .persistent()
            .get(&DataKey::OwnerNames(owner.clone()))
            .unwrap_or(Vec::new(&env));

        let mut stale = Vec::new(&env);
        for name in indexed_names.iter() {
            match env
                .storage()
                .persistent()
                .get::<_, RegistryEntry>(&DataKey::Entry(name.clone()))
            {
                None => stale.push_back(name),
                Some(entry) if entry.owner != owner => stale.push_back(name),
                _ => {}
            }
        }
        stale
    }

    pub fn burn(
        env: Env,
        name: String,
        caller: Address,
        now_unix: u64,
    ) -> Result<(), RegistryError> {
        caller.require_auth();
        let entry = get_entry(&env, &name)?;

        // Only the owner can burn their active name.
        // If the name is claimable, anyone can burn it to clean up the state.
        if entry.owner != caller && !entry.is_claimable_at(now_unix) {
            return Err(RegistryError::Unauthorized);
        }

        remove_owner_name(&env, &entry.owner, &name);
        env.storage()
            .persistent()
            .remove(&DataKey::Entry(name.clone()));

        env.events().publish(
            (symbol_short!("name"), symbol_short!("burn")),
            (name, entry.owner),
        );
        Ok(())
    }

    pub fn supports_admin_recovery(_env: Env) -> bool {
        ADMIN_RECOVERY_SUPPORTED
    }

    /// Returns the current persistent-storage schema version for upgrade
    /// planning. Future migrations should branch on this value before
    /// rewriting any derived indexes.
    pub fn storage_schema_version(_env: Env) -> u32 {
        STORAGE_SCHEMA_VERSION
    }
}

/// Inserts `name` into `owner`'s index without creating a corresponding
/// registry entry. Call only from tests to simulate an inconsistent state
/// that `audit_owner_index` should detect.
#[cfg(test)]
pub fn inject_stale_index_entry(env: &Env, owner: &Address, name: &String) {
    let key = DataKey::OwnerNames(owner.clone());
    let mut names: Vec<String> = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or(Vec::new(env));
    if !names.contains(name) {
        names.push_back(name.clone());
        env.storage().persistent().set(&key, &names);
    }
}

fn get_entry(env: &Env, name: &String) -> Result<RegistryEntry, RegistryError> {
    env.storage()
        .persistent()
        .get(&DataKey::Entry(name.clone()))
        .ok_or(RegistryError::NotFound)
}

fn put_entry(env: &Env, name: &String, entry: &RegistryEntry) {
    env.storage()
        .persistent()
        .set(&DataKey::Entry(name.clone()), entry);
}

fn validate_metadata(metadata_uri: &Option<String>) -> Result<(), RegistryError> {
    if metadata_uri
        .as_ref()
        .map(|value| value.len() as usize > MAX_METADATA_URI_LENGTH)
        .unwrap_or(false)
    {
        return Err(RegistryError::MetadataTooLong);
    }

    Ok(())
}

fn validate_lifecycle_timestamps(
    now_unix: u64,
    expires_at: u64,
    grace_period_ends_at: u64,
) -> Result<(), RegistryError> {
    if !is_active_at(expires_at, now_unix) {
        return Err(RegistryError::InvalidExpiry);
    }

    if grace_period_ends_at < expires_at {
        return Err(RegistryError::InvalidGracePeriod);
    }

    Ok(())
}

fn ensure_owner(
    entry: &RegistryEntry,
    caller: &Address,
    now_unix: u64,
) -> Result<(), RegistryError> {
    if !entry.is_active_at(now_unix) {
        return Err(RegistryError::NotActive);
    }
    if entry.owner != *caller {
        return Err(RegistryError::Unauthorized);
    }

    Ok(())
}

fn add_owner_name(env: &Env, owner: &Address, name: &String) {
    let key = DataKey::OwnerNames(owner.clone());
    let mut names = env
        .storage()
        .persistent()
        .get::<_, Vec<String>>(&key)
        .unwrap_or(Vec::new(env));

    if !names.contains(name) {
        names.push_back(name.clone());
        env.storage().persistent().set(&key, &names);
    }
}

fn remove_owner_name(env: &Env, owner: &Address, name: &String) {
    let key = DataKey::OwnerNames(owner.clone());
    let names = env
        .storage()
        .persistent()
        .get::<_, Vec<String>>(&key)
        .unwrap_or(Vec::new(env));

    let mut filtered = Vec::new(env);
    for existing in names.iter() {
        if existing != *name {
            filtered.push_back(existing);
        }
    }

    env.storage().persistent().set(&key, &filtered);
}

fn migrate(from_version: u32, to_version: u32, _data: &Bytes) {
    let _ = (from_version, to_version);
}

fn decode_target_version(data: &Bytes) -> u32 {
    if data.len() < 4 {
        return CONTRACT_VERSION + 1;
    }
    let b0 = data.get(0).unwrap_or(0);
    let b1 = data.get(1).unwrap_or(0);
    let b2 = data.get(2).unwrap_or(0);
    let b3 = data.get(3).unwrap_or(0);
    u32::from_be_bytes([b0, b1, b2, b3])
}
