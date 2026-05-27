mod test;

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env, String, Vec,
};
use xlm_ns_common::soroban::{
    build_subdomain_name, validate_base_name_soroban, validate_fqdn_soroban,
};

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct ParentDomain {
    pub owner: Address,
    pub controllers: Vec<Address>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct SubdomainRecord {
    pub parent: String,
    pub owner: Address,
    pub created_at: u64,
}

#[derive(Clone)]
#[contracttype]
enum DataKey {
    Parent(String),
    Subdomain(String),
    ParentSubdomains(String),
    OwnerSubdomains(Address),
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum SubdomainError {
    Validation = 1,
    ParentNotFound = 2,
    AlreadyExists = 3,
    NotFound = 4,
    Unauthorized = 5,
}

#[contract]
pub struct SubdomainContract;

#[contractimpl]
impl SubdomainContract {
    /// Registers a parent domain to enable subdomain creation.
    ///
    /// Safe Bootstrap Path: The parent owner must register the parent domain
    /// exactly once. Subsequent attempts to register the same parent domain
    /// will be rejected to prevent unauthorized takeover of the parent namespace.
    pub fn register_parent(env: Env, parent: String, owner: Address) -> Result<(), SubdomainError> {
        validate_fqdn_soroban(&parent).map_err(|_| SubdomainError::Validation)?;
        validate_base_name_soroban(&parent).map_err(|_| SubdomainError::Validation)?;
        let key = DataKey::Parent(parent.clone());
        if env.storage().persistent().has(&key) {
            return Err(SubdomainError::AlreadyExists);
        }
        let record = ParentDomain {
            owner,
            controllers: Vec::new(&env),
        };
        env.storage().persistent().set(&key, &record);
        env.events().publish(
            (symbol_short!("subdomain"), symbol_short!("prnt_reg")),
            (parent, record.owner.clone()),
        );
        Ok(())
    }

    pub fn add_controller(
        env: Env,
        parent: String,
        caller: Address,
        controller: Address,
    ) -> Result<(), SubdomainError> {
        let mut parent_record = get_parent(&env, &parent)?;
        if parent_record.owner != caller {
            return Err(SubdomainError::Unauthorized);
        }
        if !parent_record.controllers.contains(&controller) {
            parent_record.controllers.push_back(controller.clone());
            env.storage()
                .persistent()
                .set(&DataKey::Parent(parent.clone()), &parent_record);
            env.events().publish(
                (symbol_short!("subdomain"), symbol_short!("ctrl_add")),
                (parent, caller, controller),
            );
        }
        Ok(())
    }

    pub fn remove_controller(
        env: Env,
        parent: String,
        caller: Address,
        controller: Address,
    ) -> Result<(), SubdomainError> {
        let mut parent_record = get_parent(&env, &parent)?;
        if parent_record.owner != caller {
            return Err(SubdomainError::Unauthorized);
        }

        if let Some(index) = parent_record.controllers.first_index_of(&controller) {
            parent_record.controllers.remove(index);
            env.storage()
                .persistent()
                .set(&DataKey::Parent(parent.clone()), &parent_record);
            env.events().publish(
                (symbol_short!("subdomain"), symbol_short!("ctrl_rm")),
                (parent, caller, controller),
            );
        }

        Ok(())
    }

    pub fn create(
        env: Env,
        label: String,
        parent: String,
        caller: Address,
        owner: Address,
        now_unix: u64,
    ) -> Result<String, SubdomainError> {
        let parent_record = get_parent(&env, &parent)?;
        if parent_record.owner != caller && !parent_record.controllers.contains(&caller) {
            return Err(SubdomainError::Unauthorized);
        }

        let fqdn =
            build_subdomain_name(&env, &label, &parent).map_err(|_| SubdomainError::Validation)?;
        let key = DataKey::Subdomain(fqdn.clone());
        if env.storage().persistent().has(&key) {
            return Err(SubdomainError::AlreadyExists);
        }

        let record = SubdomainRecord {
            parent: parent.clone(),
            owner: owner.clone(),
            created_at: now_unix,
        };
        env.storage().persistent().set(&key, &record);

        add_parent_subdomain(&env, &parent, &fqdn);
        add_owner_subdomain(&env, &owner, &fqdn);

        env.events().publish(
            (symbol_short!("subdomain"), symbol_short!("created")),
            (fqdn.clone(), parent, caller, owner),
        );

        Ok(fqdn)
    }

    pub fn transfer(
        env: Env,
        fqdn: String,
        caller: Address,
        new_owner: Address,
    ) -> Result<(), SubdomainError> {
        let mut record = get_subdomain(&env, &fqdn)?;
        if record.owner != caller {
            return Err(SubdomainError::Unauthorized);
        }
        let old_owner = record.owner.clone();
        record.owner = new_owner.clone();
        env.storage()
            .persistent()
            .set(&DataKey::Subdomain(fqdn.clone()), &record);

        remove_owner_subdomain(&env, &old_owner, &fqdn);
        add_owner_subdomain(&env, &new_owner, &fqdn);

        env.events().publish(
            (symbol_short!("subdomain"), symbol_short!("transfer")),
            (fqdn, old_owner, new_owner),
        );

        Ok(())
    }

    pub fn delete(env: Env, fqdn: String, caller: Address) -> Result<(), SubdomainError> {
        let record = get_subdomain(&env, &fqdn)?;
        if record.owner != caller {
            let parent_record = get_parent(&env, &record.parent)?;
            if parent_record.owner != caller && !parent_record.controllers.contains(&caller) {
                return Err(SubdomainError::Unauthorized);
            }
        }

        remove_parent_subdomain(&env, &record.parent, &fqdn);
        remove_owner_subdomain(&env, &record.owner, &fqdn);

        env.events().publish(
            (symbol_short!("subdomain"), symbol_short!("deleted")),
            (fqdn.clone(), caller),
        );

        env.storage().persistent().remove(&DataKey::Subdomain(fqdn));

        Ok(())
    }

    /// Revokes a subdomain, removing it from storage.
    ///
    /// Deletion Semantics:
    /// - The current owner of the subdomain can delete it.
    /// - The owner or a delegated controller of the parent domain can revoke it
    ///   (e.g., to reclaim the namespace or enforce namespace rules).
    pub fn revoke(env: Env, fqdn: String, caller: Address) -> Result<(), SubdomainError> {
        let record = get_subdomain(&env, &fqdn)?;

        let mut is_authorized = false;
        if record.owner == caller {
            is_authorized = true;
        } else if let Ok(parent_record) = get_parent(&env, &record.parent) {
            if parent_record.owner == caller || parent_record.controllers.contains(&caller) {
                is_authorized = true;
            }
        }

        if !is_authorized {
            return Err(SubdomainError::Unauthorized);
        }

        env.events().publish(
            (symbol_short!("subdomain"), symbol_short!("revoked")),
            (fqdn.clone(), caller),
        );

        env.storage().persistent().remove(&DataKey::Subdomain(fqdn));
        Ok(())
    }

    pub fn exists(env: Env, fqdn: String) -> bool {
        env.storage().persistent().has(&DataKey::Subdomain(fqdn))
    }

    pub fn parent(env: Env, parent: String) -> Option<ParentDomain> {
        env.storage().persistent().get(&DataKey::Parent(parent))
    }

    pub fn record(env: Env, fqdn: String) -> Option<SubdomainRecord> {
        env.storage().persistent().get(&DataKey::Subdomain(fqdn))
    }

    pub fn subdomains_for_parent(env: Env, parent: String) -> Vec<String> {
        get_parent_subdomains(&env, &parent)
    }

    pub fn subdomains_for_owner(env: Env, owner: Address) -> Vec<String> {
        get_owner_subdomains(&env, &owner)
    }
}

fn get_parent(env: &Env, parent: &String) -> Result<ParentDomain, SubdomainError> {
    env.storage()
        .persistent()
        .get(&DataKey::Parent(parent.clone()))
        .ok_or(SubdomainError::ParentNotFound)
}

fn get_subdomain(env: &Env, fqdn: &String) -> Result<SubdomainRecord, SubdomainError> {
    env.storage()
        .persistent()
        .get(&DataKey::Subdomain(fqdn.clone()))
        .ok_or(SubdomainError::NotFound)
}

fn get_parent_subdomains(env: &Env, parent: &String) -> Vec<String> {
    env.storage()
        .persistent()
        .get(&DataKey::ParentSubdomains(parent.clone()))
        .unwrap_or_else(|| Vec::new(env))
}

fn add_parent_subdomain(env: &Env, parent: &String, fqdn: &String) {
    let mut subdomains = get_parent_subdomains(env, parent);
    if !subdomains.contains(fqdn) {
        subdomains.push_back(fqdn.clone());
        env.storage()
            .persistent()
            .set(&DataKey::ParentSubdomains(parent.clone()), &subdomains);
    }
}

fn remove_parent_subdomain(env: &Env, parent: &String, fqdn: &String) {
    let mut subdomains = get_parent_subdomains(env, parent);
    if let Some(index) = subdomains.first_index_of(fqdn) {
        subdomains.remove(index);
        env.storage()
            .persistent()
            .set(&DataKey::ParentSubdomains(parent.clone()), &subdomains);
    }
}

fn get_owner_subdomains(env: &Env, owner: &Address) -> Vec<String> {
    env.storage()
        .persistent()
        .get(&DataKey::OwnerSubdomains(owner.clone()))
        .unwrap_or_else(|| Vec::new(env))
}

fn add_owner_subdomain(env: &Env, owner: &Address, fqdn: &String) {
    let mut subdomains = get_owner_subdomains(env, owner);
    if !subdomains.contains(fqdn) {
        subdomains.push_back(fqdn.clone());
        env.storage()
            .persistent()
            .set(&DataKey::OwnerSubdomains(owner.clone()), &subdomains);
    }
}

fn remove_owner_subdomain(env: &Env, owner: &Address, fqdn: &String) {
    let mut subdomains = get_owner_subdomains(env, owner);
    if let Some(index) = subdomains.first_index_of(fqdn) {
        subdomains.remove(index);
        env.storage()
            .persistent()
            .set(&DataKey::OwnerSubdomains(owner.clone()), &subdomains);
    }
}
