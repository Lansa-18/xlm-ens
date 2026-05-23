use crate::config::NetworkConfig;
use crate::output::{emit, emit_error, OutputFormat};
use serde_json::json;
use xlm_ns_sdk::client::XlmNsClient;

pub async fn run_portfolio(
    config: NetworkConfig,
    output: OutputFormat,
    owner: &str,
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

    match client.list_registrations_by_owner(owner) {
        Ok(names) => {
            let mut lines = vec![format!("Portfolio for {owner}:")];
            if names.is_empty() {
                lines.push("  [no names found]".to_string());
            } else {
                for entry in &names {
                    let expires = entry
                        .expires_at
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "unknown".to_string());
                    lines.push(format!("  - {} (expires_at: {expires})", entry.name));
                }
            }

            emit(
                output,
                &lines.join("\n"),
                json!({
                    "owner": owner,
                    "names": names.into_iter().map(|entry| json!({
                        "name": entry.name,
                        "expires_at": entry.expires_at,
                        "resolver_contract_id": entry.resolver,
                    })).collect::<Vec<_>>(),
                    "registry_contract_id": config.registry_contract_id,
                    "rpc_url": config.rpc_url,
                    "network": config.network.as_str(),
                }),
            );
        }
        Err(err) => {
            let message = format!("ERROR: Failed to fetch portfolio for {owner}: {err}");
            emit_error(
                output,
                &message,
                json!({
                    "error": message,
                    "owner": owner,
                    "registry_contract_id": config.registry_contract_id,
                    "rpc_url": config.rpc_url,
                    "network": config.network.as_str(),
                }),
            );
        }
    }
    Ok(())
}
