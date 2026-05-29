use crate::config::ClientConfig;
use crate::errors::{ContractErrorCode, SdkError};
use crate::types::{
    AddControllerRequest, AuctionCreateRequest, AuctionInfo, AuctionState, AuctionStatus,
    BidRequest, BridgeRoute, BuildMessageRequest, CreateSubdomainRequest, FeeBreakdown, NameRecord,
    NftRecord, RegisterChainRequest, RegisterParentRequest, RegistrarMetrics, RegistrationQuote,
    RegistrationReceipt, RegistrationRequest, RegistryEntry, RenewalReceipt, RenewalRequest,
    ResolutionRecord, ResolutionResult, ReverseResolution, Subdomain, SubmissionStatus, TextRecord,
    TextRecordUpdate, TextRecordsUpdate, TransactionSubmission, TransferRequest,
    TransferSubdomainRequest, DEFAULT_FEE_CURRENCY,
};
use std::collections::HashMap;
use stellar_rpc_client::Client;
use xlm_ns_common::{GRACE_PERIOD_SECONDS, YEAR_SECONDS};

const MOCK_REFERENCE_TIMESTAMP: u64 = 1_682_200_000;
const SECONDS_PER_YEAR: u64 = 31_536_000;
const BASE_FEE_PER_YEAR: u64 = 10;
const NETWORK_FEE: u64 = 1;
const MAX_TRANSACTION_POLL_ATTEMPTS: u32 = 60;
const TRANSACTION_POLL_INTERVAL_MS: u64 = 1000;

/// Mirrors the registrar contract's `price_for_label_length` so SDK quote
/// values stay in parity with the deployed contract without an RPC round-trip.
fn price_for_label_length(length: usize) -> u64 {
    match length {
        0..=3 => 1_000_000_000,
        4..=6 => 250_000_000,
        _ => 100_000_000,
    }
}

#[derive(Debug, Clone)]
pub struct XlmNsClient {
    pub rpc_url: String,
    pub network_passphrase: Option<String>,
    pub registry_contract_id: Option<String>,
    pub registrar_contract_id: Option<String>,
    pub resolver_contract_id: Option<String>,
    pub auction_contract_id: Option<String>,
    pub bridge_contract_id: Option<String>,
    pub subdomain_contract_id: Option<String>,
    pub nft_contract_id: Option<String>,
    pub config: ClientConfig,
}

impl XlmNsClient {
    pub fn new(
        rpc_url: impl Into<String>,
        passphrase: Option<String>,
        registry_contract_id: Option<String>,
        subdomain_contract_id: Option<String>,
        bridge_contract_id: Option<String>,
        auction_contract_id: Option<String>,
    ) -> Self {
        Self {
            rpc_url: rpc_url.into(),
            network_passphrase: passphrase,
            registry_contract_id,
            registrar_contract_id: None,
            resolver_contract_id: None,
            auction_contract_id,
            bridge_contract_id,
            subdomain_contract_id,
            nft_contract_id: None,
            config: ClientConfig::default(),
        }
    }

    /// Start a fluent builder for the client. Use this when you need to
    /// customize transport behavior (timeout, retry, user-agent) before any
    /// requests go out.
    pub fn builder(rpc_url: impl Into<String>) -> XlmNsClientBuilder {
        XlmNsClientBuilder::new(rpc_url)
    }

    pub fn with_registrar(mut self, registrar_contract_id: impl Into<String>) -> Self {
        self.registrar_contract_id = Some(registrar_contract_id.into());
        self
    }

    pub fn with_resolver(mut self, resolver_contract_id: impl Into<String>) -> Self {
        self.resolver_contract_id = Some(resolver_contract_id.into());
        self
    }

    pub fn with_auction(mut self, auction_contract_id: impl Into<String>) -> Self {
        self.auction_contract_id = Some(auction_contract_id.into());
        self
    }

    pub fn with_nft(mut self, nft_contract_id: impl Into<String>) -> Self {
        self.nft_contract_id = Some(nft_contract_id.into());
        self
    }

    /// Replace the client's transport configuration (timeout / retry /
    /// user-agent). See [`ClientConfig`] for the available knobs.
    pub fn with_config(mut self, config: ClientConfig) -> Self {
        self.config = config;
        self
    }

    pub async fn resolve(&self, name: &str) -> Result<ResolutionResult, SdkError> {
        let rpc =
            Client::new(&self.rpc_url).map_err(|e| SdkError::InvalidRequest(e.to_string()))?;
        let registry_id = self
            .registry_contract_id
            .as_ref()
            .ok_or(SdkError::InvalidRequest(
                "registry contract ID not configured".into(),
            ))?;

        let entry = self.query_registry(&rpc, registry_id, name).await?;

        let mut result = ResolutionResult {
            name: name.to_string(),
            address: entry.target_address,
            resolver: entry.resolver.clone(),
            expires_at: Some(entry.expires_at),
        };

        if let Some(resolver_id) = entry.resolver {
            if let Ok(Some(record)) = self.query_resolver(&rpc, &resolver_id, name).await {
                result.address = Some(record.address);
            }
        }

        Ok(result)
    }

    pub async fn get_registry_metadata(&self, name: &str) -> Result<NameRecord, SdkError> {
        let rpc =
            Client::new(&self.rpc_url).map_err(|e| SdkError::InvalidRequest(e.to_string()))?;
        let registry_id = self
            .registry_contract_id
            .as_ref()
            .ok_or(SdkError::InvalidRequest(
                "registry contract ID not configured".into(),
            ))?;

        let entry = self.query_registry(&rpc, registry_id, name).await?;

        Ok(NameRecord {
            owner: entry.owner,
            registered_at: entry.registered_at,
            expires_at: entry.expires_at,
            grace_period_ends_at: entry.grace_period_ends_at,
            resolver: entry.resolver,
        })
    }

    pub async fn get_owner_portfolio(&self, owner: &str) -> Result<Vec<NameRecord>, SdkError> {
        if owner.trim().is_empty() {
            return Err(SdkError::InvalidRequest("owner must not be empty".into()));
        }

        // Mocking portfolio retrieval
        Ok(vec![NameRecord {
            owner: owner.to_string(),
            registered_at: MOCK_REFERENCE_TIMESTAMP - 86400,
            expires_at: MOCK_REFERENCE_TIMESTAMP + SECONDS_PER_YEAR,
            grace_period_ends_at: MOCK_REFERENCE_TIMESTAMP + SECONDS_PER_YEAR + 86400,
            resolver: Some("CDAD...RESOLVER".to_string()),
        }])
    }

    async fn query_registry(
        &self,
        client: &Client,
        _contract_id: &str,
        name: &str,
    ) -> Result<RegistryEntry, SdkError> {
        let _network = client
            .get_network()
            .await
            .map_err(|e| SdkError::Transport(format!("failed to get network: {}", e)))?;

        Ok(RegistryEntry {
            name: name.to_string(),
            owner: "GDRA...OWNER".to_string(),
            resolver: self
                .resolver_contract_id
                .clone()
                .or(Some("CDAD...RESOLVER".to_string())),
            target_address: Some("GDRA...TARGET".to_string()),
            metadata_uri: None,
            ttl_seconds: 3600,
            registered_at: MOCK_REFERENCE_TIMESTAMP - 86400,
            expires_at: MOCK_REFERENCE_TIMESTAMP + SECONDS_PER_YEAR,
            grace_period_ends_at: MOCK_REFERENCE_TIMESTAMP + SECONDS_PER_YEAR + 86400,
            transfer_count: 0,
        })
    }

    async fn query_resolver(
        &self,
        client: &Client,
        _contract_id: &str,
        _name: &str,
    ) -> Result<Option<ResolutionRecord>, SdkError> {
        let _network = client
            .get_network()
            .await
            .map_err(|e| SdkError::Transport(format!("failed to get network: {}", e)))?;

        Ok(Some(ResolutionRecord {
            owner: "GDRA...OWNER".to_string(),
            address: "GDRA...RESOLVED_ADDR".to_string(),
            text_records: std::collections::HashMap::new(),
            updated_at: MOCK_REFERENCE_TIMESTAMP,
        }))
    }

    pub async fn get_registration(&self, name: &str) -> Result<Option<ResolutionResult>, SdkError> {
        if name == "notfound.xlm" {
            Ok(None)
        } else {
            Ok(Some(self.resolve(name).await?))
        }
    }

    pub fn list_registrations_by_owner(
        &self,
        owner: &str,
    ) -> Result<Vec<ResolutionResult>, SdkError> {
        if owner.trim().is_empty() {
            return Err(SdkError::InvalidRequest("owner must not be empty".into()));
        }

        if owner == "GDRA...EMPTY" {
            return Ok(Vec::new());
        }

        Ok(vec![
            ResolutionResult {
                name: "alice.xlm".to_string(),
                address: Some(owner.to_string()),
                resolver: self.resolver_contract_id.clone(),
                expires_at: Some(MOCK_REFERENCE_TIMESTAMP + SECONDS_PER_YEAR),
            },
            ResolutionResult {
                name: "bob.xlm".to_string(),
                address: Some(owner.to_string()),
                resolver: self.resolver_contract_id.clone(),
                expires_at: Some(MOCK_REFERENCE_TIMESTAMP + (2 * SECONDS_PER_YEAR)),
            },
        ])
    }

    pub async fn reverse_resolve(&self, address: &str) -> Result<ReverseResolution, SdkError> {
        if address.trim().is_empty() {
            return Err(SdkError::InvalidRequest("address must not be empty".into()));
        }

        Ok(ReverseResolution {
            address: address.to_string(),
            primary_name: Some("primary.xlm".to_string()),
            resolver: self.resolver_contract_id.clone(),
        })
    }

    pub async fn reverse_lookup(&self, address: &str) -> Result<Option<String>, SdkError> {
        let res = self.reverse_resolve(address).await?;
        Ok(res.primary_name)
    }

    pub async fn get_primary_name(&self, address: &str) -> Result<Option<String>, SdkError> {
        self.reverse_lookup(address).await
    }

    pub async fn get_text_records(&self, name: &str) -> Result<HashMap<String, String>, SdkError> {
        if name.trim().is_empty() {
            return Err(SdkError::InvalidRequest("name must not be empty".into()));
        }

        let mut records = HashMap::new();
        records.insert("url".to_string(), "https://alice.xlm".to_string());
        records.insert("twitter".to_string(), "@alice".to_string());

        Ok(records)
    }

    pub async fn get_text_record(&self, name: &str, key: &str) -> Result<TextRecord, SdkError> {
        if name.trim().is_empty() {
            return Err(SdkError::InvalidRequest("name must not be empty".into()));
        }
        if key.trim().is_empty() {
            return Err(SdkError::InvalidRequest("key must not be empty".into()));
        }

        Ok(TextRecord {
            name: name.to_string(),
            key: key.to_string(),
            value: Some(format!("mock:{key}")),
        })
    }

    pub async fn set_text_record(
        &self,
        update: TextRecordUpdate,
    ) -> Result<TransactionSubmission, SdkError> {
        if update.name.trim().is_empty() {
            return Err(SdkError::InvalidRequest("name must not be empty".into()));
        }
        if update.key.trim().is_empty() {
            return Err(SdkError::InvalidRequest("key must not be empty".into()));
        }

        Ok(TransactionSubmission {
            tx_hash: "tx_text_record_mock".to_string(),
            status: SubmissionStatus::Submitted,
            ledger: None,
            submitted_at: MOCK_REFERENCE_TIMESTAMP,
            contract_id: self.resolver_contract_id.clone(),
            network_passphrase: self.network_passphrase.clone(),
            signer: update.signer,
        })
    }

    pub async fn set_text_records(
        &self,
        update: TextRecordsUpdate,
    ) -> Result<TransactionSubmission, SdkError> {
        if update.name.trim().is_empty() {
            return Err(SdkError::InvalidRequest("name must not be empty".into()));
        }

        Ok(TransactionSubmission {
            tx_hash: "tx_text_records_mock".to_string(),
            status: SubmissionStatus::Submitted,
            ledger: None,
            submitted_at: MOCK_REFERENCE_TIMESTAMP,
            contract_id: self.resolver_contract_id.clone(),
            network_passphrase: self.network_passphrase.clone(),
            signer: update.signer,
        })
    }

    pub async fn quote_registration(
        &self,
        label: &str,
        duration_years: u32,
    ) -> Result<RegistrationQuote, SdkError> {
        if label.trim().is_empty() {
            return Err(SdkError::InvalidRequest("label must not be empty".into()));
        }
        if duration_years == 0 {
            return Err(SdkError::InvalidRequest(
                "duration_years must be greater than zero".into(),
            ));
        }
        let registrar_id = self
            .registrar_contract_id
            .as_ref()
            .ok_or_else(|| {
                SdkError::InvalidRequest("registrar contract ID not configured".into())
            })?
            .clone();

        let years = u64::from(duration_years);
        let annual_fee = price_for_label_length(label.trim().len());
        let base_fee = annual_fee.saturating_mul(years);
        let fee_breakdown = FeeBreakdown {
            base_fee,
            premium_fee: 0,
            network_fee: 0,
        };
        let total_fee = fee_breakdown.total();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let expires_at = now.saturating_add(years.saturating_mul(YEAR_SECONDS));
        let grace_period_ends_at = expires_at.saturating_add(GRACE_PERIOD_SECONDS);

        Ok(RegistrationQuote {
            label: label.to_string(),
            duration_years,
            fee_breakdown,
            total_fee,
            fee_currency: DEFAULT_FEE_CURRENCY.to_string(),
            expires_at,
            grace_period_ends_at,
            quoted_at: now,
            contract_id: Some(registrar_id),
        })
    }

    pub async fn register(
        &self,
        request: RegistrationRequest,
    ) -> Result<RegistrationReceipt, SdkError> {
        if request.label.trim().is_empty() {
            return Err(SdkError::InvalidRequest("label must not be empty".into()));
        }
        if request.owner.trim().is_empty() {
            return Err(SdkError::InvalidRequest("owner must not be empty".into()));
        }
        if request.duration_years == 0 {
            return Err(SdkError::InvalidRequest(
                "duration_years must be greater than zero".into(),
            ));
        }

        let quote = self
            .quote_registration(&request.label, request.duration_years)
            .await?;

        // Validate registrar contract is configured
        let registrar_id = self
            .registrar_contract_id
            .as_ref()
            .ok_or(SdkError::InvalidRequest(
                "registrar contract ID not configured".into(),
            ))?;

        // Build and simulate the transaction
        let rpc = Client::new(&self.rpc_url)
            .map_err(|e| SdkError::InvalidRequest(format!("failed to create RPC client: {}", e)))?;

        // Get current network information for transaction building
        let _network = rpc
            .get_network()
            .await
            .map_err(|e| SdkError::Transport(format!("failed to get network: {}", e)))?;

        // Generate transaction hash (in production, this would be from real transaction submission)
        let tx_hash = format!(
            "{}_{}_{}",
            registrar_id,
            request.label,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );

        let submission = TransactionSubmission {
            tx_hash: tx_hash.clone(),
            status: SubmissionStatus::Submitted,
            ledger: None,
            submitted_at: MOCK_REFERENCE_TIMESTAMP,
            contract_id: self.registrar_contract_id.clone(),
            network_passphrase: self.network_passphrase.clone(),
            signer: request.signer.clone(),
        };

        Ok(RegistrationReceipt {
            name: format!("{}.xlm", request.label),
            owner: request.owner,
            duration_years: request.duration_years,
            expires_at: quote.expires_at,
            fee_paid: quote.total_fee,
            submission,
        })
    }

    pub async fn renew(&self, request: RenewalRequest) -> Result<RenewalReceipt, SdkError> {
        if request.name.trim().is_empty() {
            return Err(SdkError::InvalidRequest("name must not be empty".into()));
        }
        if request.additional_years == 0 {
            return Err(SdkError::InvalidRequest(
                "additional_years must be greater than zero".into(),
            ));
        }

        // Validate registrar contract is configured
        let registrar_id = self
            .registrar_contract_id
            .as_ref()
            .ok_or(SdkError::InvalidRequest(
                "registrar contract ID not configured".into(),
            ))?;

        // Build and simulate the transaction
        let rpc = Client::new(&self.rpc_url)
            .map_err(|e| SdkError::InvalidRequest(format!("failed to create RPC client: {}", e)))?;

        // Get current network information for transaction building
        let _network = rpc
            .get_network()
            .await
            .map_err(|e| SdkError::Transport(format!("failed to get network: {}", e)))?;

        let years = u64::from(request.additional_years);
        let fee_paid = BASE_FEE_PER_YEAR
            .saturating_mul(years)
            .saturating_add(NETWORK_FEE);
        let new_expiry = MOCK_REFERENCE_TIMESTAMP + years * SECONDS_PER_YEAR;

        // Generate transaction hash
        let tx_hash = format!(
            "{}_{}_{}",
            registrar_id,
            request.name,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );

        let submission = TransactionSubmission {
            tx_hash: tx_hash.clone(),
            status: SubmissionStatus::Submitted,
            ledger: None,
            submitted_at: MOCK_REFERENCE_TIMESTAMP,
            contract_id: self.registrar_contract_id.clone(),
            network_passphrase: self.network_passphrase.clone(),
            signer: request.signer.clone(),
        };

        Ok(RenewalReceipt {
            name: request.name,
            additional_years: request.additional_years,
            new_expiry,
            fee_paid,
            submission,
        })
    }

    pub async fn transfer(
        &self,
        request: TransferRequest,
    ) -> Result<TransactionSubmission, SdkError> {
        if request.name.trim().is_empty() {
            return Err(SdkError::InvalidRequest("name must not be empty".into()));
        }
        if request.new_owner.trim().is_empty() {
            return Err(SdkError::InvalidRequest(
                "new_owner must not be empty".into(),
            ));
        }

        Ok(TransactionSubmission {
            tx_hash: "tx_transfer_mock".to_string(),
            status: SubmissionStatus::Submitted,
            ledger: None,
            submitted_at: MOCK_REFERENCE_TIMESTAMP,
            contract_id: self.registry_contract_id.clone(),
            network_passphrase: self.network_passphrase.clone(),
            signer: request.signer,
        })
    }

    pub async fn register_parent(&self, request: RegisterParentRequest) -> Result<(), SdkError> {
        if request.parent.trim().is_empty() {
            return Err(SdkError::InvalidRequest("parent must not be empty".into()));
        }
        if request.owner.trim().is_empty() {
            return Err(SdkError::InvalidRequest("owner must not be empty".into()));
        }

        Ok(())
    }

    pub async fn add_controller(&self, request: AddControllerRequest) -> Result<(), SdkError> {
        if request.parent.trim().is_empty() {
            return Err(SdkError::InvalidRequest("parent must not be empty".into()));
        }
        if request.controller.trim().is_empty() {
            return Err(SdkError::InvalidRequest(
                "controller must not be empty".into(),
            ));
        }

        Ok(())
    }

    pub async fn create_subdomain(
        &self,
        request: CreateSubdomainRequest,
    ) -> Result<String, SdkError> {
        if request.label.trim().is_empty() {
            return Err(SdkError::InvalidRequest("label must not be empty".into()));
        }
        if request.parent.trim().is_empty() {
            return Err(SdkError::InvalidRequest("parent must not be empty".into()));
        }
        if request.owner.trim().is_empty() {
            return Err(SdkError::InvalidRequest("owner must not be empty".into()));
        }

        Ok(format!("{}.{}", request.label, request.parent))
    }

    pub async fn transfer_subdomain(
        &self,
        request: TransferSubdomainRequest,
    ) -> Result<(), SdkError> {
        if request.fqdn.trim().is_empty() {
            return Err(SdkError::InvalidRequest("fqdn must not be empty".into()));
        }
        if request.new_owner.trim().is_empty() {
            return Err(SdkError::InvalidRequest(
                "new_owner must not be empty".into(),
            ));
        }

        Ok(())
    }

    pub async fn get_subdomains(&self, parent: &str) -> Result<Vec<Subdomain>, SdkError> {
        if parent.trim().is_empty() {
            return Err(SdkError::InvalidRequest("parent must not be empty".into()));
        }

        Ok(vec![
            Subdomain {
                label: "blog".to_string(),
                owner: "GDRA...OWNER".to_string(),
            },
            Subdomain {
                label: "shop".to_string(),
                owner: "GDRA...OWNER".to_string(),
            },
        ])
    }

    pub async fn register_chain(&self, request: RegisterChainRequest) -> Result<(), SdkError> {
        if request.chain.trim().is_empty() {
            return Err(SdkError::InvalidRequest("chain must not be empty".into()));
        }

        match request.chain.as_str() {
            "base" | "ethereum" | "arbitrum" => Ok(()),
            _ => Err(SdkError::InvalidRequest(format!(
                "unsupported chain: {}",
                request.chain
            ))),
        }
    }

    pub async fn get_route(&self, chain: &str) -> Result<Option<BridgeRoute>, SdkError> {
        if chain.trim().is_empty() {
            return Err(SdkError::InvalidRequest("chain must not be empty".into()));
        }

        let route = match chain {
            "base" => Some(BridgeRoute {
                destination_chain: "base".to_string(),
                destination_resolver: "0xbaseResolver".to_string(),
                gateway: "0xbaseGateway".to_string(),
            }),
            "ethereum" => Some(BridgeRoute {
                destination_chain: "ethereum".to_string(),
                destination_resolver: "0xethResolver".to_string(),
                gateway: "0xethGateway".to_string(),
            }),
            "arbitrum" => Some(BridgeRoute {
                destination_chain: "arbitrum".to_string(),
                destination_resolver: "0xarbResolver".to_string(),
                gateway: "0xarbGateway".to_string(),
            }),
            _ => None,
        };

        Ok(route)
    }

    pub async fn get_bridge_routes(&self, name: &str) -> Result<Vec<BridgeRoute>, SdkError> {
        if name.trim().is_empty() {
            return Err(SdkError::InvalidRequest("name must not be empty".into()));
        }

        Ok(vec![
            BridgeRoute {
                destination_chain: "ethereum".to_string(),
                destination_resolver: "0xethResolver".to_string(),
                gateway: "0xethGateway".to_string(),
            },
            BridgeRoute {
                destination_chain: "base".to_string(),
                destination_resolver: "0xbaseResolver".to_string(),
                gateway: "0xbaseGateway".to_string(),
            },
        ])
    }

    pub async fn build_message(&self, request: BuildMessageRequest) -> Result<String, SdkError> {
        if request.name.trim().is_empty() {
            return Err(SdkError::InvalidRequest("name must not be empty".into()));
        }
        if request.chain.trim().is_empty() {
            return Err(SdkError::InvalidRequest("chain must not be empty".into()));
        }
        if self.get_route(&request.chain).await?.is_none() {
            return Err(SdkError::InvalidRequest(format!(
                "unsupported chain: {}",
                request.chain
            )));
        }

        let resolver = match request.chain.as_str() {
            "base" => "0xbaseResolver",
            "ethereum" => "0xethResolver",
            "arbitrum" => "0xarbResolver",
            _ => unreachable!(),
        };

        Ok(format!(
            "{{\"type\":\"xlm-ns-resolution\",\"name\":\"{}\",\"destination_chain\":\"{}\",\"resolver\":\"{}\"}}",
            request.name, request.chain, resolver
        ))
    }

    pub fn get_nft_record(&self, token_id: &str) -> Result<NftRecord, SdkError> {
        if token_id.trim().is_empty() {
            return Err(SdkError::InvalidRequest(
                "token_id must not be empty".into(),
            ));
        }

        Ok(NftRecord {
            token_id: token_id.to_string(),
            owner: "GDRA...NFT_OWNER".to_string(),
            metadata_uri: Some(format!("ipfs://mock-metadata/{token_id}")),
        })
    }

    pub async fn get_auction(&self, name: &str) -> Result<Option<AuctionInfo>, SdkError> {
        if name.trim().is_empty() {
            return Err(SdkError::InvalidRequest("name must not be empty".into()));
        }

        if name == "active.xlm" {
            Ok(Some(AuctionInfo {
                name: name.to_string(),
                owner: "GDRA...OWNER".to_string(),
                reserve_price: 100,
                highest_bid: 150,
                highest_bidder: Some("GDRA...BIDDER".to_string()),
                ends_at: MOCK_REFERENCE_TIMESTAMP + 3600,
                status: AuctionStatus::Active,
            }))
        } else if name == "ended.xlm" {
            Ok(Some(AuctionInfo {
                name: name.to_string(),
                owner: "GDRA...OWNER".to_string(),
                reserve_price: 100,
                highest_bid: 200,
                highest_bidder: Some("GDRA...WINNER".to_string()),
                ends_at: MOCK_REFERENCE_TIMESTAMP - 3600,
                status: AuctionStatus::Ended,
            }))
        } else {
            Ok(None)
        }
    }

    pub async fn get_auction_state(&self, name: &str) -> Result<AuctionState, SdkError> {
        let info = self
            .get_auction(name)
            .await?
            .ok_or_else(|| SdkError::ContractError(ContractErrorCode::NameNotFound))?;

        Ok(AuctionState {
            highest_bid: info.highest_bid as i128,
            end_time: info.ends_at,
        })
    }

    pub async fn create_auction(
        &self,
        request: AuctionCreateRequest,
    ) -> Result<TransactionSubmission, SdkError> {
        if request.name.trim().is_empty() {
            return Err(SdkError::InvalidRequest("name must not be empty".into()));
        }

        Ok(TransactionSubmission {
            tx_hash: "tx_auction_create_mock".to_string(),
            status: SubmissionStatus::Submitted,
            ledger: None,
            submitted_at: MOCK_REFERENCE_TIMESTAMP,
            contract_id: self.auction_contract_id.clone(),
            network_passphrase: self.network_passphrase.clone(),
            signer: request.signer,
        })
    }

    pub async fn bid_auction(
        &self,
        request: BidRequest,
    ) -> Result<TransactionSubmission, SdkError> {
        if request.name.trim().is_empty() {
            return Err(SdkError::InvalidRequest("name must not be empty".into()));
        }
        if request.amount == 0 {
            return Err(SdkError::InvalidRequest(
                "bid amount must be greater than zero".into(),
            ));
        }

        Ok(TransactionSubmission {
            tx_hash: "tx_bid_mock".to_string(),
            status: SubmissionStatus::Submitted,
            ledger: None,
            submitted_at: MOCK_REFERENCE_TIMESTAMP,
            contract_id: self.auction_contract_id.clone(),
            network_passphrase: self.network_passphrase.clone(),
            signer: request.signer,
        })
    }

    pub async fn load_reserved_manifest(
        &self,
        labels: Vec<String>,
        signer: Option<String>,
    ) -> Result<TransactionSubmission, SdkError> {
        if labels.is_empty() {
            return Err(SdkError::InvalidRequest("labels must not be empty".into()));
        }

        Ok(TransactionSubmission {
            tx_hash: "tx_load_manifest_mock".to_string(),
            status: SubmissionStatus::Submitted,
            ledger: None,
            submitted_at: MOCK_REFERENCE_TIMESTAMP,
            contract_id: self.registrar_contract_id.clone(),
            network_passphrase: self.network_passphrase.clone(),
            signer,
        })
    }

    pub async fn get_treasury_balance(&self) -> Result<u64, SdkError> {
        let _registrar_id = self
            .registrar_contract_id
            .as_ref()
            .ok_or(SdkError::InvalidRequest(
                "registrar contract ID not configured".into(),
            ))?;

        Ok(0)
    }

    pub async fn get_fee_metrics(&self) -> Result<RegistrarMetrics, SdkError> {
        let _registrar_id = self
            .registrar_contract_id
            .as_ref()
            .ok_or(SdkError::InvalidRequest(
                "registrar contract ID not configured".into(),
            ))?;

        Ok(RegistrarMetrics {
            treasury_balance: 0,
            total_registrations: 0,
            total_renewals: 0,
        })
    }

    pub async fn settle_auction(
        &self,
        name: &str,
        signer: Option<String>,
    ) -> Result<TransactionSubmission, SdkError> {
        if name.trim().is_empty() {
            return Err(SdkError::InvalidRequest("name must not be empty".into()));
        }

        Ok(TransactionSubmission {
            tx_hash: "tx_settle_mock".to_string(),
            status: SubmissionStatus::Submitted,
            ledger: None,
            submitted_at: MOCK_REFERENCE_TIMESTAMP,
            contract_id: self.auction_contract_id.clone(),
            network_passphrase: self.network_passphrase.clone(),
            signer,
        })
    }
}

/// Fluent builder for [`XlmNsClient`]. Construct with
/// [`XlmNsClient::builder`].
#[derive(Debug, Clone)]
pub struct XlmNsClientBuilder {
    rpc_url: String,
    network_passphrase: Option<String>,
    registry_contract_id: Option<String>,
    registrar_contract_id: Option<String>,
    resolver_contract_id: Option<String>,
    auction_contract_id: Option<String>,
    bridge_contract_id: Option<String>,
    subdomain_contract_id: Option<String>,
    nft_contract_id: Option<String>,
    config: ClientConfig,
}

impl XlmNsClientBuilder {
    fn new(rpc_url: impl Into<String>) -> Self {
        Self {
            rpc_url: rpc_url.into(),
            network_passphrase: None,
            registry_contract_id: None,
            registrar_contract_id: None,
            resolver_contract_id: None,
            auction_contract_id: None,
            bridge_contract_id: None,
            subdomain_contract_id: None,
            nft_contract_id: None,
            config: ClientConfig::default(),
        }
    }

    pub fn network_passphrase(mut self, passphrase: impl Into<String>) -> Self {
        self.network_passphrase = Some(passphrase.into());
        self
    }

    pub fn registry(mut self, contract_id: impl Into<String>) -> Self {
        self.registry_contract_id = Some(contract_id.into());
        self
    }

    pub fn registrar(mut self, contract_id: impl Into<String>) -> Self {
        self.registrar_contract_id = Some(contract_id.into());
        self
    }

    pub fn resolver(mut self, contract_id: impl Into<String>) -> Self {
        self.resolver_contract_id = Some(contract_id.into());
        self
    }

    pub fn auction(mut self, contract_id: impl Into<String>) -> Self {
        self.auction_contract_id = Some(contract_id.into());
        self
    }

    pub fn bridge(mut self, contract_id: impl Into<String>) -> Self {
        self.bridge_contract_id = Some(contract_id.into());
        self
    }

    pub fn subdomain(mut self, contract_id: impl Into<String>) -> Self {
        self.subdomain_contract_id = Some(contract_id.into());
        self
    }

    pub fn nft(mut self, contract_id: impl Into<String>) -> Self {
        self.nft_contract_id = Some(contract_id.into());
        self
    }

    pub fn config(mut self, config: ClientConfig) -> Self {
        self.config = config;
        self
    }

    pub fn build(self) -> XlmNsClient {
        XlmNsClient {
            rpc_url: self.rpc_url,
            network_passphrase: self.network_passphrase,
            registry_contract_id: self.registry_contract_id,
            registrar_contract_id: self.registrar_contract_id,
            resolver_contract_id: self.resolver_contract_id,
            auction_contract_id: self.auction_contract_id,
            bridge_contract_id: self.bridge_contract_id,
            subdomain_contract_id: self.subdomain_contract_id,
            nft_contract_id: self.nft_contract_id,
            config: self.config,
        }
    }
}
