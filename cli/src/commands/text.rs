use crate::config::NetworkConfig;
use crate::signer::SignerProfile;
use anyhow::Context;
use xlm_ns_sdk::client::XlmNsClient;
use xlm_ns_sdk::types::TextRecordUpdate;

pub async fn run_get(config: NetworkConfig, name: &str, key: &str) -> anyhow::Result<()> {
    let client = XlmNsClient::new(
        config.rpc_url,
        Some(config.network_passphrase),
        config.registry_contract_id.clone(),
        config.subdomain_contract_id.clone(),
        config.bridge_contract_id.clone(),
        config.auction_contract_id.clone(),
    );

    let record = client
        .get_text_record(name, key)
        .await
        .context("Failed to fetch text record")?;

    if let Some(val) = record.value {
        println!("{}: {} = \"{}\"", name, key, val);
    } else {
        println!("{}: {} = [NOT SET]", name, key);
    }

    Ok(())
}

pub async fn run_set(
    config: NetworkConfig,
    name: &str,
    key: &str,
    value: Option<String>,
    signer: Option<SignerProfile>,
) -> anyhow::Result<()> {
    let client = XlmNsClient::new(
        config.rpc_url,
        Some(config.network_passphrase),
        config.registry_contract_id.clone(),
        config.subdomain_contract_id.clone(),
        config.bridge_contract_id.clone(),
        config.auction_contract_id.clone(),
    );

    if let Some(ref s) = signer {
        println!("  Signer: {}", s.describe());
    }

    let submission = client
        .set_text_record(TextRecordUpdate {
            name: name.into(),
            key: key.into(),
            value,
            signer: signer.as_ref().map(|s| s.name.clone()),
        })
        .await
        .context("Failed to update text record")?;

    println!("SUCCESS: text record update submitted");
    println!("  Status: {}", submission.status);
    println!("  Transaction Hash: {}", submission.tx_hash);

    Ok(())
}
