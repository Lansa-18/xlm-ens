use crate::config::NetworkConfig;
use crate::signer::SignerProfile;
use anyhow::{anyhow, Context};
use xlm_ns_sdk::client::XlmNsClient;
use xlm_ns_sdk::types::{AuctionCreateRequest, BidRequest};

pub async fn run_create(
    config: NetworkConfig,
    name: &str,
    reserve: u64,
    duration: u64,
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

    println!("Creating auction for {name}...");
    if let Some(ref s) = signer {
        println!("  Signer: {}", s.describe());
    }

    let submission = client
        .create_auction(AuctionCreateRequest {
            name: name.into(),
            reserve_price: reserve,
            duration_seconds: duration,
            signer: signer.as_ref().map(|s| s.name.clone()),
        })
        .await
        .context("Failed to create auction")?;

    println!("SUCCESS: auction created for {name}");
    println!("  Reserve: {reserve} XLM");
    println!("  Duration: {duration}s");
    println!("  Transaction Hash: {}", submission.tx_hash);

    Ok(())
}

pub async fn run_bid(
    config: NetworkConfig,
    name: &str,
    amount: u64,
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

    println!("Placing bid of {amount} XLM on {name}...");
    if let Some(ref s) = signer {
        println!("  Signer: {}", s.describe());
    }

    let submission = client
        .bid_auction(BidRequest {
            name: name.into(),
            amount,
            signer: signer.as_ref().map(|s| s.name.clone()),
        })
        .await
        .context("Failed to place bid")?;

    println!("SUCCESS: bid placed on {name}");
    println!("  Transaction Hash: {}", submission.tx_hash);

    Ok(())
}

pub async fn run_inspect(config: NetworkConfig, name: &str) -> anyhow::Result<()> {
    let client = XlmNsClient::new(
        config.rpc_url,
        Some(config.network_passphrase),
        config.registry_contract_id.clone(),
        config.subdomain_contract_id.clone(),
        config.bridge_contract_id.clone(),
        config.auction_contract_id.clone(),
    );

    let auction = client
        .get_auction(name)
        .await
        .context("Failed to fetch auction state")?
        .ok_or_else(|| anyhow!("No active auction found for '{}'", name))?;

    println!("Auction for {}:", auction.name);
    println!("  Status: {}", auction.status);
    println!("  Owner: {}", auction.owner);
    println!("  Reserve Price: {} XLM", auction.reserve_price);
    println!("  Highest Bid: {} XLM", auction.highest_bid);
    if let Some(bidder) = auction.highest_bidder {
        println!("  Highest Bidder: {}", bidder);
    }
    println!("  Ends at: {}", auction.ends_at);

    Ok(())
}

pub async fn run_settle(
    config: NetworkConfig,
    name: &str,
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

    println!("Settling auction for {name}...");
    if let Some(ref s) = signer {
        println!("  Signer: {}", s.describe());
    }

    let submission = client
        .settle_auction(name, signer.as_ref().map(|s| s.name.clone()))
        .await
        .context("Failed to settle auction")?;

    println!("SUCCESS: auction settled for {name}");
    println!("  Transaction Hash: {}", submission.tx_hash);

    Ok(())
}
///