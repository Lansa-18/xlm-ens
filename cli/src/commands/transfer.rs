use crate::config::NetworkConfig;
use crate::signer::SignerProfile;
use anyhow::Context;
use xlm_ns_sdk::client::XlmNsClient;
use xlm_ns_sdk::types::TransferRequest;

pub async fn run_transfer(
    config: NetworkConfig,
    name: &str,
    new_owner: &str,
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

    println!("Initiating transfer of {name} to {new_owner}...");
    if let Some(ref s) = signer {
        println!("  Signer: {}", s.describe());
    }

    let submission = client
        .transfer(TransferRequest {
            name: name.into(),
            new_owner: new_owner.into(),
            signer: signer.as_ref().map(|s| s.name.clone()),
        })
        .await
        .context("Failed to submit transfer")?;

    println!("SUCCESS: {name} ownership transferred to {new_owner}");
    println!("  Status: {}", submission.status);
    println!("  Transaction Hash: {}", submission.tx_hash);

    match client.get_registration(name).await {
        Ok(Some(reg)) => {
            if let Some(addr) = reg.address {
                println!("Verified: Current owner is now {addr}");
            }
        }
        _ => {
            println!("Warning: Could not verify ownership change immediately.");
        }
    }

    Ok(())
}
