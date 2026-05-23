mod test;

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, Address, Env, IntoVal, Map, String, Symbol,
};
use xlm_ns_common::soroban::validate_fqdn_soroban;
use xlm_ns_common::RegistryEntry;
use xlm_ns_common::MAX_TEXT_RECORDS;

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct ResolutionRecord {
    pub owner: Address,
    pub address: String,
    pub text_records: Map<String, String>,
    pub updated_at: u64,
}

#[derive(Clone)]
#[contracttype]
enum DataKey {
    Forward(String),
    Reverse(String),
    Primary(String),
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
}

#[contractclient(name = "ResolverContractClient")]
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
        let canonical_owner = match registry_owner(&env, &name, now_unix)? {
            Some(registry_owner) => {
                if registry_owner != owner {
                    return Err(ResolverError::Unauthorized);
                }
                registry_owner
            }
            None => owner.clone(),
        };

        let text_records = match get_record(&env, &name) {
            Ok(existing) => {
                if existing.owner != canonical_owner {
                    return Err(ResolverError::Unauthorized);
                }
                existing.text_records
            }
            Err(ResolverError::RecordNotFound) => Map::new(&env),
            Err(err) => return Err(err),
        };

        let record = ResolutionRecord {
            owner: canonical_owner,
            address: address.clone(),
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

    pub fn set_text_record(
        env: Env,
        name: String,
        caller: Address,
        key: String,
        value: String,
        now_unix: u64,
    ) -> Result<(), ResolverError> {
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
        if record.address != address {
            return Err(ResolverError::Unauthorized);
        }
        env.storage()
            .persistent()
            .set(&DataKey::Primary(record.address.clone()), &name);
        Ok(())
    }

    pub fn remove_record(env: Env, name: String, caller: Address) -> Result<(), ResolverError> {
        let record = get_record(&env, &name)?;
        assert_owner(&env, &name, &record, &caller, 0)?;
        env.storage()
            .persistent()
            .remove(&DataKey::Forward(name.clone()));
        env.storage()
            .persistent()
            .remove(&DataKey::Reverse(record.address.clone()));
        env.storage()
            .persistent()
            .remove(&DataKey::Primary(record.address));
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

    let registry_entry = env
        .invoke_contract::<RegistryEntry>(
            &registry,
            &Symbol::new(env, "resolve"),
            (name.clone(), now_unix).into_val(env),
        )
        .map_err(|_| ResolverError::Unauthorized)?;

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
