use crate::config::NetworkConfig;
use crate::signer::SignerProfile;
use anyhow::Context;
use xlm_ns_sdk::client::XlmNsClient;
use xlm_ns_sdk::types::RegistrationRequest;

pub async fn run_register(
    config: NetworkConfig,
    label: &str,
    owner: &str,
    signer: Option<SignerProfile>,
) -> anyhow::Result<()> {
    // Note: I'm not passing output format here to keep it simple,
    // but the remote version had it. I'll just use Human format for now or
    // refactor main to pass it.
    // Actually, I'll pass it in the function signature if I want to be thorough.

    let registrar_id = config
        .registrar_contract_id
        .clone()
        .ok_or_else(|| anyhow::anyhow!("Registrar contract ID not configured"))?;

    let client = XlmNsClient::new(
        config.rpc_url.clone(),
        Some(config.network_passphrase.clone()),
        config.registry_contract_id.clone(),
        config.subdomain_contract_id.clone(),
        config.bridge_contract_id.clone(),
        config.auction_contract_id.clone(),
    )
    .with_registrar(registrar_id.clone());

    let duration_years = 1;
    let quote = client
        .quote_registration(label, duration_years)
        .await
        .context("Failed to fetch registration quote")?;

    let signer_name = signer.as_ref().map(|s| s.name.clone());
    let signer_description = signer.as_ref().map(|s| s.describe());

    let receipt = client
        .register(RegistrationRequest {
            label: label.into(),
            owner: owner.into(),
            duration_years,
            signer: signer_name.clone(),
        })
        .await
        .context("Failed to submit registration")?;

    println!("Registration quote for {label}.xlm:");
    println!("  Registrar: {registrar_id}");
    println!(
        "  Fee: {} {} (base {}, premium {}, network {})",
        quote.total_fee,
        quote.fee_currency,
        quote.fee_breakdown.base_fee,
        quote.fee_breakdown.premium_fee,
        quote.fee_breakdown.network_fee,
    );
    println!("  Duration: {duration_years} year(s)");
    println!("  Expiry: {}", quote.expires_at);
    if let Some(desc) = signer_description {
        println!("  Signer: {desc}");
    }
    println!(
        "\nSUCCESS: registered {} to {}",
        receipt.name, receipt.owner
    );
    println!("  Fee paid: {} {}", receipt.fee_paid, quote.fee_currency);
    println!("  Expires at: {}", receipt.expires_at);
    println!("  Status: {}", receipt.submission.status);
    println!("  Transaction Hash: {}", receipt.submission.tx_hash);

    Ok(())
}
