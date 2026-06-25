#![cfg_attr(not(test), no_std)]
mod axelar;
mod test;

use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, Address, Bytes, BytesN,
    Env, String,
};
use xlm_ns_common::soroban::{validate_chain_name_soroban, validate_fqdn_soroban};

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct BridgeRoute {
    pub destination_chain: String,
    pub destination_resolver: String,
    pub gateway: String,
}

#[derive(Clone)]
#[contracttype]
enum DataKey {
    Route(String),
    Admin,
    ContractVersion,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum BridgeError {
    Validation = 1,
    UnsupportedChain = 2,
    UpgradeFailed = 3,
}

pub const CONTRACT_VERSION: u32 = 1;

#[contractevent]
pub struct ContractUpgraded {
    pub old_version: u32,
    pub new_version: u32,
    pub admin: Address,
}

#[contract]
pub struct BridgeContract;

#[contractimpl]
impl BridgeContract {
    pub fn version(_env: Env) -> u32 {
        CONTRACT_VERSION
    }

    pub fn initialize(env: Env, admin: Address) -> Result<(), BridgeError> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(BridgeError::Validation);
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
    ) -> Result<(), BridgeError> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(BridgeError::UpgradeFailed)?;
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

    pub fn register_chain(env: Env, chain: String) -> Result<(), BridgeError> {
        validate_chain_name_soroban(&chain).map_err(|_| BridgeError::Validation)?;
        let route = target_for_chain(&env, &chain).ok_or(BridgeError::UnsupportedChain)?;

        // Defensive check: Reject malformed route data
        if route.destination_chain.len() == 0
            || route.destination_resolver.len() == 0
            || route.gateway.len() == 0
        {
            return Err(BridgeError::Validation);
        }

        env.storage()
            .persistent()
            .set(&DataKey::Route(chain), &route);
        Ok(())
    }

    pub fn build_message(env: Env, name: String, chain: String) -> Result<String, BridgeError> {
        validate_fqdn_soroban(&name).map_err(|_| BridgeError::Validation)?;
        validate_chain_name_soroban(&chain).map_err(|_| BridgeError::Validation)?;
        let route: BridgeRoute = env
            .storage()
            .persistent()
            .get(&DataKey::Route(chain.clone()))
            .ok_or(BridgeError::UnsupportedChain)?;

        Ok(build_forward_gmp_message(
            &env,
            &name,
            &route.destination_chain,
            &route.destination_resolver,
        ))
    }

    pub fn build_reverse_message(
        env: Env,
        address: String,
        primary_name: String,
        chain: String,
    ) -> Result<String, BridgeError> {
        if address.len() == 0 || primary_name.len() == 0 {
            return Err(BridgeError::Validation);
        }
        validate_fqdn_soroban(&primary_name).map_err(|_| BridgeError::Validation)?;
        validate_chain_name_soroban(&chain).map_err(|_| BridgeError::Validation)?;
        let route: BridgeRoute = env
            .storage()
            .persistent()
            .get(&DataKey::Route(chain.clone()))
            .ok_or(BridgeError::UnsupportedChain)?;

        Ok(build_reverse_gmp_message(
            &env,
            &address,
            &primary_name,
            &route.destination_chain,
            &route.destination_resolver,
        ))
    }

    pub fn route(env: Env, chain: String) -> Option<BridgeRoute> {
        env.storage().persistent().get(&DataKey::Route(chain))
    }
}

fn target_for_chain(env: &Env, chain: &String) -> Option<BridgeRoute> {
    let base = String::from_str(env, "base");
    let ethereum = String::from_str(env, "ethereum");
    let arbitrum = String::from_str(env, "arbitrum");

    if *chain == base {
        Some(BridgeRoute {
            destination_chain: base,
            destination_resolver: String::from_str(env, "0xbaseResolver"),
            gateway: String::from_str(env, "0xbaseGateway"),
        })
    } else if *chain == ethereum {
        Some(BridgeRoute {
            destination_chain: ethereum,
            destination_resolver: String::from_str(env, "0xethResolver"),
            gateway: String::from_str(env, "0xethGateway"),
        })
    } else if *chain == arbitrum {
        Some(BridgeRoute {
            destination_chain: arbitrum,
            destination_resolver: String::from_str(env, "0xarbResolver"),
            gateway: String::from_str(env, "0xarbGateway"),
        })
    } else {
        None
    }
}

fn soroban_str_to_bytes(s: &String) -> [u8; 256] {
    let mut buf = [0u8; 256];
    let len = s.len() as usize;
    if len > 0 && len <= 256 {
        s.copy_into_slice(&mut buf[..len]);
    }
    buf
}

fn build_forward_gmp_message(
    env: &Env,
    name: &String,
    destination_chain: &String,
    resolver: &String,
) -> String {
    let name_bytes = soroban_str_to_bytes(name);
    let chain_bytes = soroban_str_to_bytes(destination_chain);
    let resolver_bytes = soroban_str_to_bytes(resolver);
    String::from_str(
        env,
        &axelar::build_forward_gmp_message(
            &name_bytes[..name.len() as usize],
            &chain_bytes[..destination_chain.len() as usize],
            &resolver_bytes[..resolver.len() as usize],
        ),
    )
}

fn build_reverse_gmp_message(
    env: &Env,
    address: &String,
    primary_name: &String,
    destination_chain: &String,
    resolver: &String,
) -> String {
    let addr_bytes = soroban_str_to_bytes(address);
    let name_bytes = soroban_str_to_bytes(primary_name);
    let chain_bytes = soroban_str_to_bytes(destination_chain);
    let resolver_bytes = soroban_str_to_bytes(resolver);
    String::from_str(
        env,
        &axelar::build_reverse_gmp_message(
            &addr_bytes[..address.len() as usize],
            &name_bytes[..primary_name.len() as usize],
            &chain_bytes[..destination_chain.len() as usize],
            &resolver_bytes[..resolver.len() as usize],
        ),
    )
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
