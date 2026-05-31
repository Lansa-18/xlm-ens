use crate::config::NetworkConfig;
use anyhow::Context;
use xlm_ns_sdk::client::XlmNsClient;

pub async fn run_reverse(config: NetworkConfig, address: &str) -> anyhow::Result<()> {
    let client = XlmNsClient::new(
        config.rpc_url,
        Some(config.network_passphrase),
        config.registry_contract_id.clone(),
        config.subdomain_contract_id.clone(),
        config.bridge_contract_id.clone(),
        config.auction_contract_id.clone(),
    );

    let result = client
        .reverse_resolve(address)
        .await
        .context("Failed to perform reverse lookup")?;

    if let Some(name) = result.primary_name {
        println!("{} -> {}", result.address, name);
    } else {
        println!("{} -> [NO PRIMARY NAME]", result.address);
    }

    Ok(())
}
