use crate::config::NetworkConfig;
use anyhow::Context;
use xlm_ns_sdk::client::XlmNsClient;
use xlm_ns_sdk::types::{
    AddControllerRequest, CreateSubdomainRequest, RegisterParentRequest, TransferSubdomainRequest,
};

pub async fn run_register_parent(
    config: NetworkConfig,
    parent: &str,
    owner: &str,
) -> anyhow::Result<()> {
    let client = XlmNsClient::new(
        config.rpc_url,
        Some(config.network_passphrase),
        config.registry_contract_id.clone(),
        config.subdomain_contract_id.clone(),
        config.bridge_contract_id.clone(),
        config.auction_contract_id.clone(),
    );

    let submission = client
        .register_parent(RegisterParentRequest {
            parent: parent.into(),
            owner: owner.into(),
        }, false)
        .await
        .context("Failed to register parent domain")?;

    println!("SUCCESS: registered parent domain {parent} with owner {owner}");
    println!("  Transaction Hash: {}", submission.tx_hash);
    Ok(())
}

pub async fn run_add_controller(
    config: NetworkConfig,
    parent: &str,
    controller: &str,
) -> anyhow::Result<()> {
    let client = XlmNsClient::new(
        config.rpc_url,
        Some(config.network_passphrase),
        config.registry_contract_id.clone(),
        config.subdomain_contract_id.clone(),
        config.bridge_contract_id.clone(),
        config.auction_contract_id.clone(),
    );

    let submission = client
        .add_controller(AddControllerRequest {
            parent: parent.into(),
            controller: controller.into(),
        }, false)
        .await
        .context("Failed to add controller")?;

    println!("SUCCESS: added controller {controller} to parent domain {parent}");
    println!("  Transaction Hash: {}", submission.tx_hash);
    Ok(())
}

pub async fn run_create_subdomain(
    config: NetworkConfig,
    label: &str,
    parent: &str,
    owner: &str,
) -> anyhow::Result<()> {
    let client = XlmNsClient::new(
        config.rpc_url,
        Some(config.network_passphrase),
        config.registry_contract_id.clone(),
        config.subdomain_contract_id.clone(),
        config.bridge_contract_id.clone(),
        config.auction_contract_id.clone(),
    );

    let submission = client
        .create_subdomain(CreateSubdomainRequest {
            label: label.into(),
            parent: parent.into(),
            owner: owner.into(),
        }, false)
        .await
        .context("Failed to create subdomain")?;

    let fqdn = format!("{label}.{parent}");
    println!("SUCCESS: created subdomain {fqdn} with owner {owner}");
    println!("  Transaction Hash: {}", submission.tx_hash);
    Ok(())
}

pub async fn run_transfer_subdomain(
    config: NetworkConfig,
    fqdn: &str,
    new_owner: &str,
) -> anyhow::Result<()> {
    let client = XlmNsClient::new(
        config.rpc_url,
        Some(config.network_passphrase),
        config.registry_contract_id.clone(),
        config.subdomain_contract_id.clone(),
        config.bridge_contract_id.clone(),
        config.auction_contract_id.clone(),
    );

    let submission = client
        .transfer_subdomain(TransferSubdomainRequest {
            fqdn: fqdn.into(),
            new_owner: new_owner.into(),
        }, false)
        .await
        .context("Failed to transfer subdomain")?;

    println!("SUCCESS: transferred subdomain {fqdn} to new owner {new_owner}");
    println!("  Transaction Hash: {}", submission.tx_hash);
    Ok(())
}
