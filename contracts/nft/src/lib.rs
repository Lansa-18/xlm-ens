#![cfg_attr(not(test), no_std)]
mod events;
mod test;

use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, symbol_short, Address,
    Bytes, BytesN, Env, String, Vec,
};

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct TokenRecord {
    pub owner: Address,
    pub approved: Option<Address>,
    pub metadata_uri: Option<String>,
}

#[derive(Clone)]
#[contracttype]
enum DataKey {
    Token(String),
    TokenIds,
    OwnerTokens(Address),
    Admin,
    ContractVersion,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum NftError {
    AlreadyMinted = 1,
    NotFound = 2,
    Unauthorized = 3,
    UpgradeFailed = 4,
}

pub const CONTRACT_VERSION: u32 = 1;

#[contractevent]
pub struct ContractUpgraded {
    pub old_version: u32,
    pub new_version: u32,
    pub admin: Address,
}

#[contract]
pub struct NftContract;

#[contractimpl]
impl NftContract {
    pub fn version(_env: Env) -> u32 {
        CONTRACT_VERSION
    }

    pub fn initialize(env: Env, admin: Address) -> Result<(), NftError> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(NftError::AlreadyMinted);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .persistent()
            .set(&DataKey::ContractVersion, &CONTRACT_VERSION);
        Ok(())
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
    ) -> Result<(), NftError> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(NftError::UpgradeFailed)?;
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

    pub fn mint(
        env: Env,
        token_id: String,
        owner: Address,
        metadata_uri: Option<String>,
    ) -> Result<(), NftError> {
        let key = DataKey::Token(token_id.clone());
        if env.storage().persistent().has(&key) {
            return Err(NftError::AlreadyMinted);
        }
        let record = TokenRecord {
            owner: owner.clone(),
            approved: None,
            metadata_uri,
        };
        env.storage().persistent().set(&key, &record);
        append_token_id(&env, &token_id);
        add_owner_token(&env, &owner, &token_id);
        events::mint(&env, owner.clone(), owner, token_id);
        Ok(())
    }

    pub fn approve(
        env: Env,
        token_id: String,
        caller: Address,
        approved: Address,
    ) -> Result<(), NftError> {
        let mut record = get_token(&env, &token_id)?;
        if record.owner != caller {
            return Err(NftError::Unauthorized);
        }
        record.approved = Some(approved.clone());
        env.storage()
            .persistent()
            .set(&DataKey::Token(token_id.clone()), &record);
        events::approve(&env, caller, approved, token_id);
        Ok(())
    }

    pub fn approve_clear(env: Env, token_id: String, caller: Address) -> Result<(), NftError> {
        let mut record = get_token(&env, &token_id)?;
        if record.owner != caller {
            return Err(NftError::Unauthorized);
        }
        record.approved = None;
        env.storage()
            .persistent()
            .set(&DataKey::Token(token_id.clone()), &record);
        events::approve_clear(&env, caller, token_id);
        Ok(())
    }

    pub fn transfer(
        env: Env,
        token_id: String,
        caller: Address,
        new_owner: Address,
    ) -> Result<(), NftError> {
        let mut record = get_token(&env, &token_id)?;
        if record.owner != caller && record.approved.as_ref() != Some(&caller) {
            return Err(NftError::Unauthorized);
        }
        let previous_owner = record.owner.clone();
        record.owner = new_owner.clone();
        record.approved = None;
        env.storage()
            .persistent()
            .set(&DataKey::Token(token_id.clone()), &record);
        reindex_owner_token(&env, &previous_owner, &record.owner, &token_id);
        events::transfer(&env, previous_owner, new_owner, token_id);
        Ok(())
    }

    pub fn transfer_from(
        env: Env,
        spender: Address,
        owner: Address,
        recipient: Address,
        token_id: String,
    ) -> Result<(), NftError> {
        let mut record = get_token(&env, &token_id)?;
        if record.owner != owner || record.approved.as_ref() != Some(&spender) {
            return Err(NftError::Unauthorized);
        }
        record.owner = recipient.clone();
        record.approved = None;
        env.storage()
            .persistent()
            .set(&DataKey::Token(token_id.clone()), &record);
        reindex_owner_token(&env, &owner, &record.owner, &token_id);
        events::transfer(&env, owner, recipient, token_id);
        Ok(())
    }

    pub fn owner_of(env: Env, token_id: String) -> Option<Address> {
        env.storage()
            .persistent()
            .get::<_, TokenRecord>(&DataKey::Token(token_id))
            .map(|record| record.owner)
    }

    pub fn token(env: Env, token_id: String) -> Option<TokenRecord> {
        env.storage().persistent().get(&DataKey::Token(token_id))
    }

    pub fn balance_of(env: Env, owner: Address) -> u32 {
        owner_tokens(&env, &owner).len()
    }

    pub fn total_supply(env: Env) -> u32 {
        token_ids(&env).len()
    }

    pub fn token_by_index(env: Env, index: u32) -> Option<String> {
        token_ids(&env).get(index)
    }

    pub fn token_of_owner_by_index(env: Env, owner: Address, index: u32) -> Option<String> {
        owner_tokens(&env, &owner).get(index)
    }

    pub fn token_uri(env: Env, token_id: String) -> Option<String> {
        env.storage()
            .persistent()
            .get::<_, TokenRecord>(&DataKey::Token(token_id))
            .and_then(|record| record.metadata_uri)
    }
}

fn get_token(env: &Env, token_id: &String) -> Result<TokenRecord, NftError> {
    env.storage()
        .persistent()
        .get(&DataKey::Token(token_id.clone()))
        .ok_or(NftError::NotFound)
}

fn token_ids(env: &Env) -> Vec<String> {
    env.storage()
        .persistent()
        .get(&DataKey::TokenIds)
        .unwrap_or(Vec::new(env))
}

fn owner_tokens(env: &Env, owner: &Address) -> Vec<String> {
    env.storage()
        .persistent()
        .get(&DataKey::OwnerTokens(owner.clone()))
        .unwrap_or(Vec::new(env))
}

fn append_token_id(env: &Env, token_id: &String) {
    let mut token_ids = token_ids(env);
    token_ids.push_back(token_id.clone());
    env.storage()
        .persistent()
        .set(&DataKey::TokenIds, &token_ids);
}

fn add_owner_token(env: &Env, owner: &Address, token_id: &String) {
    let key = DataKey::OwnerTokens(owner.clone());
    let mut tokens = owner_tokens(env, owner);
    if !tokens.contains(token_id) {
        tokens.push_back(token_id.clone());
        env.storage().persistent().set(&key, &tokens);
    }
}

fn remove_owner_token(env: &Env, owner: &Address, token_id: &String) {
    let key = DataKey::OwnerTokens(owner.clone());
    let tokens = owner_tokens(env, owner);
    let mut filtered = Vec::new(env);
    for existing in tokens.iter() {
        if existing != *token_id {
            filtered.push_back(existing);
        }
    }
    env.storage().persistent().set(&key, &filtered);
}

fn reindex_owner_token(
    env: &Env,
    previous_owner: &Address,
    new_owner: &Address,
    token_id: &String,
) {
    if previous_owner == new_owner {
        return;
    }

    remove_owner_token(env, previous_owner, token_id);
    add_owner_token(env, new_owner, token_id);
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
