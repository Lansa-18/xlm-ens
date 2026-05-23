use crate::config::NetworkConfig;
use anyhow::{anyhow, Context};
use xlm_ns_sdk::client::XlmNsClient;
use xlm_ns_sdk::types::{BuildMessageRequest, RegisterChainRequest};

pub async fn run_register_chain(config: NetworkConfig, chain: &str) -> anyhow::Result<()> {
    let client = XlmNsClient::new(
        config.rpc_url,
        Some(config.network_passphrase),
        config.registry_contract_id.clone(),
        config.subdomain_contract_id.clone(),
        config.bridge_contract_id.clone(),
        config.auction_contract_id.clone(),
    );

    client
        .register_chain(RegisterChainRequest {
            chain: chain.into(),
        })
        .await
        .context("Failed to register chain")?;

    println!("SUCCESS: registered bridge route for chain {}", chain);
    Ok(())
}

pub async fn run_inspect_route(config: NetworkConfig, chain: &str) -> anyhow::Result<()> {
    let client = XlmNsClient::new(
        config.rpc_url,
        Some(config.network_passphrase),
        config.registry_contract_id.clone(),
        config.subdomain_contract_id.clone(),
        config.bridge_contract_id.clone(),
        config.auction_contract_id.clone(),
    );

    let route = client
        .get_route(chain)
        .await
        .context("Failed to inspect route")?
        .ok_or_else(|| anyhow!("No route found for chain '{}'", chain))?;

    println!("Bridge route for chain '{}':", chain);
    println!("  Chain: {}", route.destination_chain);
    println!("  Gateway: {}", route.gateway);
    println!("  Resolver: {}", route.destination_resolver);

    Ok(())
}

pub async fn run_generate_payload(
    config: NetworkConfig,
    name: &str,
    chain: &str,
) -> anyhow::Result<()> {
    let client = XlmNsClient::new(
        config.rpc_url,
        Some(config.network_passphrase),
        config.registry_contract_id.clone(),
        config.subdomain_contract_id.clone(),
        config.bridge_contract_id.clone(),
        config.auction_contract_id.clone(),
    );

    let payload = client
        .build_message(BuildMessageRequest {
            name: name.into(),
            chain: chain.into(),
        })
        .await
        .context("Failed to generate payload")?;

    println!("Generated payload for '{}' on chain '{}':", name, chain);
    println!("{}", payload);

    Ok(())
}
