mod events;
mod test;

use soroban_sdk::{contract, contracterror, contractimpl, contracttype, Address, Env, String, Vec};

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
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum NftError {
    AlreadyMinted = 1,
    NotFound = 2,
    Unauthorized = 3,
}

#[contract]
pub struct NftContract;

#[contractimpl]
impl NftContract {
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
        reindex_owner_token(env, &previous_owner, &record.owner, &token_id);
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

fn reindex_owner_token(env: &Env, previous_owner: &Address, new_owner: &Address, token_id: &String) {
    if previous_owner == new_owner {
        return;
    }

    remove_owner_token(env, previous_owner, token_id);
    add_owner_token(env, new_owner, token_id);
}
