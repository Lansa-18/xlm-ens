mod test;

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, Address, Env, IntoVal, Map, String,
    Symbol, Vec,
};
use xlm_ns_common::soroban::validate_fqdn_soroban;
use xlm_ns_common::RegistryEntry;
use xlm_ns_common::{MAX_TEXT_RECORDS, MAX_TEXT_RECORD_VALUE_LENGTH};

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct ResolutionRecord {
    pub owner: Address,
    pub addresses: Map<String, String>, // chain_name -> address (e.g., "stellar" -> address, "ethereum" -> address)
    pub text_records: Map<String, String>,
    pub updated_at: u64,
}

// For backwards compatibility, use a default chain identifier
const DEFAULT_CHAIN: &str = "stellar";

#[derive(Clone)]
#[contracttype]
enum DataKey {
    Forward(String),
    Reverse(String), // address -> name (for primary/reverse lookups)
    Primary(String), // address -> name (for primary names)
    Registry,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ResolverError {
    Validation = 1,
    RecordNotFound = 2,
    Unauthorized = 3,
    TooManyTextRecords = 4,
    NotInitialized = 5,
    TextRecordValueTooLong = 6,
    InvalidChain = 7,
    // #314: text-record key failed normalization check
    InvalidKey = 8,
}

#[contract]
pub struct ResolverContract;

#[contractimpl]
impl ResolverContract {
    pub fn initialize(env: Env, registry: Address) -> Result<(), ResolverError> {
        if env.storage().instance().has(&DataKey::Registry) {
            return Err(ResolverError::Unauthorized);
        }
        env.storage().instance().set(&DataKey::Registry, &registry);
        Ok(())
    }

    pub fn set_record(
        env: Env,
        name: String,
        owner: Address,
        address: String,
        now_unix: u64,
    ) -> Result<(), ResolverError> {
        validate_fqdn_soroban(&name).map_err(|_| ResolverError::Validation)?;
        let registry_backed_owner = registry_owner(&env, &name, now_unix)?;
        let canonical_owner = match registry_backed_owner.clone() {
            Some(registry_owner) => {
                if registry_owner != owner {
                    return Err(ResolverError::Unauthorized);
                }
                registry_owner
            }
            None => owner.clone(),
        };

        // Get existing record and clean up old primary mappings if address changes
        let mut addresses = match get_record(&env, &name) {
            Ok(existing) => {
                if registry_backed_owner.is_none() && existing.owner != canonical_owner {
                    return Err(ResolverError::Unauthorized);
                }
                // Issue #316: Clean up old reverse/primary mappings when address changes
                if let Some(old_stellar_addr) = existing
                    .addresses
                    .get(String::from_str(&env, DEFAULT_CHAIN))
                {
                    if old_stellar_addr != address {
                        env.storage()
                            .persistent()
                            .remove(&DataKey::Reverse(old_stellar_addr.clone()));
                        env.storage()
                            .persistent()
                            .remove(&DataKey::Primary(old_stellar_addr));
                    }
                }
                existing.addresses
            }
            Err(ResolverError::RecordNotFound) => Map::new(&env),
            Err(err) => return Err(err),
        };

        // Set the stellar address as the default chain
        addresses.set(String::from_str(&env, DEFAULT_CHAIN), address.clone());

        let text_records = match get_record(&env, &name) {
            Ok(existing) => existing.text_records,
            Err(ResolverError::RecordNotFound) => Map::new(&env),
            Err(err) => return Err(err),
        };

        let record = ResolutionRecord {
            owner: canonical_owner,
            addresses,
            text_records,
            updated_at: now_unix,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Forward(name.clone()), &record);
        env.storage()
            .persistent()
            .set(&DataKey::Reverse(address), &name);
        Ok(())
    }

    // Issue #317: Add multi-chain address setter
    pub fn set_address(
        env: Env,
        name: String,
        caller: Address,
        chain: String,
        address: String,
        now_unix: u64,
    ) -> Result<(), ResolverError> {
        let mut record = get_record(&env, &name)?;
        assert_owner(&env, &name, &record, &caller, now_unix)?;

        // For Stellar chain, handle reverse mappings
        if chain == String::from_str(&env, DEFAULT_CHAIN) {
            // Clean up old reverse mappings for Stellar
            if let Some(old_addr) = record.addresses.get(chain.clone()) {
                if old_addr != address {
                    env.storage()
                        .persistent()
                        .remove(&DataKey::Reverse(old_addr.clone()));
                    env.storage()
                        .persistent()
                        .remove(&DataKey::Primary(old_addr));
                }
            }
            // Set new reverse mapping
            env.storage()
                .persistent()
                .set(&DataKey::Reverse(address.clone()), &name);
        }

        record.addresses.set(chain, address);
        record.updated_at = now_unix;
        put_record(&env, &name, &record);
        Ok(())
    }

    // Issue #317: Get address for a specific chain
    pub fn get_address(env: Env, name: String, chain: String) -> Option<String> {
        get_record(&env, &name)
            .ok()
            .and_then(|record| record.addresses.get(chain))
    }

    pub fn set_text_record(
        env: Env,
        name: String,
        caller: Address,
        key: String,
        value: String,
        now_unix: u64,
    ) -> Result<(), ResolverError> {
        // Issue #314: Validate text-record key normalization.
        // Keys must be 1–64 bytes, lowercase ASCII, and contain only
        // letters, digits, dots (.), dashes (-), or underscores (_).
        validate_text_record_key(&key).map_err(|_| ResolverError::InvalidKey)?;

        // Issue #315: Validate text record value size
        if value.len() > MAX_TEXT_RECORD_VALUE_LENGTH as u32 {
            return Err(ResolverError::TextRecordValueTooLong);
        }

        let mut record = get_record(&env, &name)?;
        assert_owner(&env, &name, &record, &caller, now_unix)?;
        if !record.text_records.contains_key(key.clone())
            && record.text_records.len() >= MAX_TEXT_RECORDS as u32
        {
            return Err(ResolverError::TooManyTextRecords);
        }
        record.text_records.set(key, value);
        record.updated_at = now_unix;
        put_record(&env, &name, &record);
        Ok(())
    }

    pub fn set_primary_name(
        env: Env,
        address: String,
        caller: Address,
        name: String,
    ) -> Result<(), ResolverError> {
        let record = get_record(&env, &name)?;
        assert_owner(&env, &name, &record, &caller, 0)?;
        if let Some(stellar_addr) = record.addresses.get(String::from_str(&env, DEFAULT_CHAIN)) {
            if stellar_addr != address {
                return Err(ResolverError::Unauthorized);
            }
        } else {
            return Err(ResolverError::Unauthorized);
        }
        env.storage()
            .persistent()
            .set(&DataKey::Primary(address), &name);
        Ok(())
    }

    pub fn remove_record(env: Env, name: String, caller: Address) -> Result<(), ResolverError> {
        let record = get_record(&env, &name)?;
        assert_owner(&env, &name, &record, &caller, 0)?;

        // Clean up reverse mappings for all chains, particularly Stellar
        if let Some(stellar_addr) = record.addresses.get(String::from_str(&env, DEFAULT_CHAIN)) {
            env.storage()
                .persistent()
                .remove(&DataKey::Reverse(stellar_addr.clone()));
            env.storage()
                .persistent()
                .remove(&DataKey::Primary(stellar_addr));
        }

        env.storage()
            .persistent()
            .remove(&DataKey::Forward(name.clone()));
        Ok(())
    }

    pub fn update_owner(env: Env, name: String, new_owner: Address) -> Result<(), ResolverError> {
        let mut record = get_record(&env, &name)?;
        record.owner = new_owner;
        put_record(&env, &name, &record);
        Ok(())
    }

    pub fn resolve(env: Env, name: String) -> Option<ResolutionRecord> {
        env.storage().persistent().get(&DataKey::Forward(name))
    }

    // Helper method to get the default (Stellar) address for backwards compatibility
    pub fn get_stellar_address(env: Env, name: String) -> Option<String> {
        Self::resolve(env.clone(), name)
            .and_then(|record| record.addresses.get(String::from_str(&env, DEFAULT_CHAIN)))
    }

    pub fn has_record(env: Env, name: String) -> bool {
        env.storage().persistent().has(&DataKey::Forward(name))
    }

    pub fn reverse(env: Env, address: String) -> Option<String> {
        env.storage()
            .persistent()
            .get(&DataKey::Primary(address.clone()))
            .or_else(|| env.storage().persistent().get(&DataKey::Reverse(address)))
    }

    pub fn transfer_record_owner(
        env: Env,
        name: String,
        caller: Address,
        new_owner: Address,
    ) -> Result<(), ResolverError> {
        let mut record = get_record(&env, &name)?;
        if record.owner != caller {
            return Err(ResolverError::Unauthorized);
        }
        record.owner = new_owner;
        put_record(&env, &name, &record);
        Ok(())
    }

    // Issue #321: Batch resolver query for multiple names
    pub fn batch_resolve(env: Env, names: Vec<String>) -> Vec<Option<ResolutionRecord>> {
        let mut out = Vec::new(&env);
        for name in names.iter() {
            out.push_back(
                env.storage()
                    .persistent()
                    .get(&DataKey::Forward(name.clone())),
            );
        }
        out
    }

    // Issue #321: Batch reverse lookup for multiple addresses
    pub fn batch_reverse(env: Env, addresses: Vec<String>) -> Vec<Option<String>> {
        let mut out = Vec::new(&env);
        for address in addresses.iter() {
            out.push_back(
                env.storage()
                    .persistent()
                    .get(&DataKey::Primary(address.clone()))
                    .or_else(|| {
                        env.storage()
                            .persistent()
                            .get(&DataKey::Reverse(address.clone()))
                    }),
            );
        }
        out
    }
}

fn get_registry(env: &Env) -> Result<Address, ResolverError> {
    env.storage()
        .instance()
        .get(&DataKey::Registry)
        .ok_or(ResolverError::NotInitialized)
}

fn registry_owner(
    env: &Env,
    name: &String,
    now_unix: u64,
) -> Result<Option<Address>, ResolverError> {
    let registry = match get_registry(env) {
        Ok(registry) => registry,
        Err(ResolverError::NotInitialized) => return Ok(None),
        Err(err) => return Err(err),
    };

    let registry_entry = env.invoke_contract::<RegistryEntry>(
        &registry,
        &Symbol::new(env, "resolve"),
        (name.clone(), now_unix).into_val(env),
    );

    Ok(Some(registry_entry.owner))
}

fn assert_owner(
    env: &Env,
    name: &String,
    record: &ResolutionRecord,
    caller: &Address,
    now_unix: u64,
) -> Result<(), ResolverError> {
    if let Some(owner) = registry_owner(env, name, now_unix)? {
        if owner != *caller {
            return Err(ResolverError::Unauthorized);
        }
        return Ok(());
    }

    if record.owner != *caller {
        return Err(ResolverError::Unauthorized);
    }

    Ok(())
}

fn get_record(env: &Env, name: &String) -> Result<ResolutionRecord, ResolverError> {
    env.storage()
        .persistent()
        .get(&DataKey::Forward(name.clone()))
        .ok_or(ResolverError::RecordNotFound)
}

fn put_record(env: &Env, name: &String, record: &ResolutionRecord) {
    env.storage()
        .persistent()
        .set(&DataKey::Forward(name.clone()), record);
}

/// Issue #314: Validate a text-record key.
///
/// Rules:
/// - Length: 1–64 bytes (inclusive).
/// - Characters: lowercase ASCII letters `a-z`, digits `0-9`, dot `.`,
///   dash `-`, or underscore `_`.
/// - Namespace convention (e.g. `com.twitter`, `org.did`) is allowed via dots.
///
/// Keys are stored exactly as supplied; callers must normalise before calling
/// (e.g. lowercase the key) because two differently-cased writes produce two
/// distinct storage entries.
fn validate_text_record_key(key: &String) -> Result<(), ()> {
    const MAX_KEY_LEN: usize = 64;
    let len = key.len() as usize;
    if len == 0 || len > MAX_KEY_LEN {
        return Err(());
    }
    let mut buf = [0u8; MAX_KEY_LEN];
    key.copy_into_slice(&mut buf[..len]);
    for byte in &buf[..len] {
        let b = *byte;
        let ok =
            b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-' || b == b'_';
        if !ok {
            return Err(());
        }
    }
    Ok(())
}
