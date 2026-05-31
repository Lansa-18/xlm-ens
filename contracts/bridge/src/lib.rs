mod axelar;
mod test;

use soroban_sdk::{contract, contracterror, contractimpl, contracttype, Env, String};
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
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum BridgeError {
    Validation = 1,
    UnsupportedChain = 2,
}

#[contract]
pub struct BridgeContract;

#[contractimpl]
impl BridgeContract {
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

fn build_forward_gmp_message(
    env: &Env,
    name: &String,
    destination_chain: &String,
    resolver: &String,
) -> String {
    String::from_str(
        env,
        &axelar::build_forward_gmp_message(name, destination_chain, resolver),
    )
}

fn build_reverse_gmp_message(
    env: &Env,
    address: &String,
    primary_name: &String,
    destination_chain: &String,
    resolver: &String,
) -> String {
    String::from_str(
        env,
        &axelar::build_reverse_gmp_message(address, primary_name, destination_chain, resolver),
    )
}
