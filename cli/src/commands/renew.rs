use crate::config::NetworkConfig;
use crate::signer::SignerProfile;
use anyhow::{anyhow, Context};
use xlm_ns_sdk::client::XlmNsClient;
use xlm_ns_sdk::types::RenewalRequest;

pub async fn run_renew(
    config: NetworkConfig,
    name: &str,
    years: u64,
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

    let registration = client
        .get_registration(name)
        .await
        .context("Failed to fetch registration state")?;

    match registration {
        Some(_) => {
            if let Some(ref s) = signer {
                println!("  Signer: {}", s.describe());
            }
            let receipt = client
                .renew(RenewalRequest {
                    name: name.into(),
                    additional_years: years as u32,
                    signer: signer.as_ref().map(|s| s.name.clone()),
                })
                .await
                .context("Failed to renew name")?;

            println!("SUCCESS: Renewed {name} for {years} year(s)");
            println!("  Fee Paid: {} XLM", receipt.fee_paid);
            println!("  New Expiry: {}", receipt.new_expiry);
            println!("  Status: {}", receipt.submission.status);
            println!("  Transaction Hash: {}", receipt.submission.tx_hash);
        }
        None => {
            return Err(anyhow!(
                "Name '{}' is not registered and cannot be renewed.",
                name
            ));
        }
    }

    Ok(())
}
