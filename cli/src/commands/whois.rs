use crate::config::NetworkConfig;
use crate::output::{emit, emit_error, OutputFormat};
use serde_json::json;
use xlm_ns_sdk::client::XlmNsClient;

pub async fn run_whois(
    config: NetworkConfig,
    output: OutputFormat,
    name: &str,
) -> anyhow::Result<()> {
    let client = XlmNsClient::new(
        config.rpc_url.clone(),
        Some(config.network_passphrase.clone()),
        config.registry_contract_id.clone(),
        config.subdomain_contract_id.clone(),
        config.bridge_contract_id.clone(),
        config.auction_contract_id.clone(),
    )
    .with_resolver(
        config
            .resolver_contract_id
            .clone()
            .unwrap_or_else(|| "unknown".to_string()),
    );

    match client.get_registration(name).await {
        Ok(Some(record)) => {
            let owner = record
                .address
                .clone()
                .unwrap_or_else(|| "[UNKNOWN]".to_string());
            let expires_at = record.expires_at;
            let human = format!(
                "{name}\n  Owner: {owner}\n  Expires at: {}\n  Resolver: {}\n  Registry: {}\n  RPC: {}",
                expires_at
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "[UNKNOWN]".to_string()),
                record
                    .resolver
                    .clone()
                    .unwrap_or_else(|| "[UNKNOWN]".to_string()),
                config
                    .registry_contract_id
                    .clone()
                    .unwrap_or_else(|| "[UNKNOWN]".to_string()),
                config.rpc_url
            );

            emit(
                output,
                &human,
                json!({
                    "name": record.name,
                    "owner": record.address,
                    "expires_at": record.expires_at,
                    "resolver_contract_id": record.resolver,
                    "registry_contract_id": config.registry_contract_id,
                    "rpc_url": config.rpc_url,
                    "network": config.network.as_str(),
                }),
            );
        }
        Ok(None) => {
            emit(
                output,
                &format!("{name}\n  Status: [NOT REGISTERED]"),
                json!({
                    "name": name,
                    "registered": false,
                    "registry_contract_id": config.registry_contract_id,
                    "rpc_url": config.rpc_url,
                    "network": config.network.as_str(),
                }),
            );
        }
        Err(err) => {
            let message = format!("ERROR: Failed to fetch registration for {name}: {err}");
            emit_error(
                output,
                &message,
                json!({
                    "error": message,
                    "name": name,
                    "registry_contract_id": config.registry_contract_id,
                    "rpc_url": config.rpc_url,
                    "network": config.network.as_str(),
                }),
            );
        }
    }
    Ok(())
}
