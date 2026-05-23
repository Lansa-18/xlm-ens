use crate::config::NetworkConfig;
use crate::output::{emit, emit_error, OutputFormat};
use serde_json::json;
use xlm_ns_sdk::client::XlmNsClient;

pub async fn run_inspect(
    config: NetworkConfig,
    output: OutputFormat,
    token_id: &str,
) -> anyhow::Result<()> {
    let nft_contract_id = config
        .nft_contract_id
        .clone()
        .ok_or_else(|| anyhow::anyhow!("NFT contract ID not configured"))?;

    let client = XlmNsClient::new(
        config.rpc_url.clone(),
        Some(config.network_passphrase.clone()),
        config.registry_contract_id.clone(),
        config.subdomain_contract_id.clone(),
        config.bridge_contract_id.clone(),
        config.auction_contract_id.clone(),
    )
    .with_nft(nft_contract_id.clone());

    match client.get_nft_record(token_id) {
        Ok(record) => {
            let human = format!(
                "Token {token_id}\n  Owner: {}\n  Metadata URI: {}\n  NFT: {nft_contract_id}",
                record.owner,
                record
                    .metadata_uri
                    .clone()
                    .unwrap_or_else(|| "[NONE]".to_string())
            );
            emit(
                output,
                &human,
                json!({
                    "token_id": record.token_id,
                    "owner": record.owner,
                    "metadata_uri": record.metadata_uri,
                    "nft_contract_id": nft_contract_id,
                    "rpc_url": config.rpc_url,
                    "network": config.network.as_str(),
                }),
            );
        }
        Err(err) => {
            let message = format!("ERROR: Failed to inspect token {token_id}: {err}");
            emit_error(
                output,
                &message,
                json!({
                    "error": message,
                    "token_id": token_id,
                    "nft_contract_id": nft_contract_id,
                }),
            );
        }
    }
    Ok(())
}
