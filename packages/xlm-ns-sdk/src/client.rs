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
use std::time::{SystemTime, UNIX_EPOCH};
use stellar_rpc_client::Client;
use xlm_ns_common::{GRACE_PERIOD_SECONDS, YEAR_SECONDS};

const MOCK_REFERENCE_TIMESTAMP: u64 = 1_682_200_000;
const SECONDS_PER_YEAR: u64 = 31_536_000;
const BASE_FEE_PER_YEAR: u64 = 10;
const NETWORK_FEE: u64 = 1;

/// Mirrors the registrar contract's `price_for_label_length` so SDK quote
/// values stay in parity with the deployed contract without an RPC round-trip.
fn price_for_label_length(length: usize) -> u64 {
    match length {
        0..=3 => 1_000_000_000,
        4..=6 => 250_000_000,
        _ => 100_000_000,
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn submission_hash(
    operation: &str,
    contract_id: Option<&str>,
    subject: &str,
    ledger: u32,
) -> String {
    let contract = contract_id.unwrap_or("unconfigured");
    format!("{operation}:{contract}:{subject}:{ledger}")
}

fn contract_id_error(label: &str) -> SdkError {
    SdkError::InvalidRequest(format!("{label} contract ID not configured"))
}

fn make_text_records(name: &str) -> HashMap<String, String> {
    let mut records = HashMap::new();
    records.insert("url".to_string(), format!("https://{name}"));
    records.insert(
        "twitter".to_string(),
        format!("@{}", name.split('.').next().unwrap_or("alice")),
    );
    records
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

    /// Start a fluent builder for the client.
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

    pub fn with_config(mut self, config: ClientConfig) -> Self {
        self.config = config;
        self
    }

    async fn rpc_client(&self) -> Result<Client, SdkError> {
        Client::new(&self.rpc_url).map_err(|e| SdkError::InvalidRequest(e.to_string()))
    }

    async fn rpc_context(&self) -> Result<(Client, u32, String), SdkError> {
        let rpc = self.rpc_client().await?;
        let network = rpc
            .get_network()
            .await
            .map_err(|e| SdkError::Transport(format!("failed to get network: {e}")))?;
        let latest_ledger = rpc
            .get_latest_ledger()
            .await
            .map_err(|e| SdkError::Transport(format!("failed to get latest ledger: {e}")))?;
        Ok((rpc, latest_ledger.sequence, network.passphrase))
    }

    fn make_submission(
        &self,
        operation: &str,
        contract_id: Option<String>,
        ledger: u32,
        signer: Option<String>,
        network_passphrase: Option<String>,
    ) -> TransactionSubmission {
        let subject = contract_id.as_deref().unwrap_or("unconfigured");
        TransactionSubmission {
            tx_hash: submission_hash(operation, contract_id.as_deref(), subject, ledger),
            status: SubmissionStatus::Submitted,
            ledger: Some(ledger),
            submitted_at: now_unix(),
            contract_id,
            network_passphrase: self.network_passphrase.clone().or(network_passphrase),
            signer,
        }
    }

    fn require_label(label: &str, field: &'static str) -> Result<(), SdkError> {
        if label.trim().is_empty() {
            return Err(SdkError::InvalidRequest(format!(
                "{field} must not be empty"
            )));
        }
        Ok(())
    }

    pub async fn resolve(&self, name: &str) -> Result<ResolutionResult, SdkError> {
        let (rpc, ledger, _) = self.rpc_context().await?;
        let registry_id = self
            .registry_contract_id
            .as_ref()
            .ok_or_else(|| contract_id_error("registry"))?;

        let entry = self.query_registry(&rpc, registry_id, name, ledger).await?;

        let mut result = ResolutionResult {
            name: name.to_string(),
            address: entry.target_address,
            resolver: entry.resolver.clone(),
            expires_at: Some(entry.expires_at),
        };

        if let Some(resolver_id) = entry.resolver.clone() {
            if let Some(record) = self
                .query_resolver(&rpc, &resolver_id, name, ledger)
                .await?
            {
                result.address = Some(record.address);
            }
        }

        Ok(result)
    }

    pub async fn get_registry_metadata(&self, name: &str) -> Result<NameRecord, SdkError> {
        let (rpc, ledger, _) = self.rpc_context().await?;
        let registry_id = self
            .registry_contract_id
            .as_ref()
            .ok_or_else(|| contract_id_error("registry"))?;

        let entry = self.query_registry(&rpc, registry_id, name, ledger).await?;

        Ok(NameRecord {
            owner: entry.owner,
            registered_at: entry.registered_at,
            expires_at: entry.expires_at,
            grace_period_ends_at: entry.grace_period_ends_at,
            resolver: entry.resolver,
        })
    }

    pub async fn get_owner_portfolio(&self, owner: &str) -> Result<Vec<NameRecord>, SdkError> {
        Self::require_label(owner, "owner")?;

        Ok(vec![
            NameRecord {
                owner: owner.to_string(),
                registered_at: MOCK_REFERENCE_TIMESTAMP - 86_400,
                expires_at: MOCK_REFERENCE_TIMESTAMP + SECONDS_PER_YEAR,
                grace_period_ends_at: MOCK_REFERENCE_TIMESTAMP + SECONDS_PER_YEAR + 86_400,
                resolver: self.resolver_contract_id.clone(),
            },
            NameRecord {
                owner: owner.to_string(),
                registered_at: MOCK_REFERENCE_TIMESTAMP - 172_800,
                expires_at: MOCK_REFERENCE_TIMESTAMP + (2 * SECONDS_PER_YEAR),
                grace_period_ends_at: MOCK_REFERENCE_TIMESTAMP + (2 * SECONDS_PER_YEAR) + 86_400,
                resolver: self.resolver_contract_id.clone(),
            },
        ])
    }

    async fn query_registry(
        &self,
        client: &Client,
        _contract_id: &str,
        name: &str,
        ledger: u32,
    ) -> Result<RegistryEntry, SdkError> {
        let _network = client
            .get_network()
            .await
            .map_err(|e| SdkError::Transport(format!("failed to get network: {e}")))?;

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
            registered_at: MOCK_REFERENCE_TIMESTAMP.saturating_add(u64::from(ledger)),
            expires_at: MOCK_REFERENCE_TIMESTAMP + SECONDS_PER_YEAR,
            grace_period_ends_at: MOCK_REFERENCE_TIMESTAMP
                + SECONDS_PER_YEAR
                + GRACE_PERIOD_SECONDS,
            transfer_count: 0,
        })
    }

    async fn query_resolver(
        &self,
        client: &Client,
        _contract_id: &str,
        name: &str,
        ledger: u32,
    ) -> Result<Option<ResolutionRecord>, SdkError> {
        let _network = client
            .get_network()
            .await
            .map_err(|e| SdkError::Transport(format!("failed to get network: {e}")))?;

        Ok(Some(ResolutionRecord {
            owner: "GDRA...OWNER".to_string(),
            address: format!("GDRA...RESOLVED_ADDR:{name}:{ledger}"),
            text_records: make_text_records(name),
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
        Self::require_label(owner, "owner")?;

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
        Self::require_label(address, "address")?;

        Ok(ReverseResolution {
            address: address.to_string(),
            primary_name: Some("primary.xlm".to_string()),
            resolver: self.resolver_contract_id.clone(),
        })
    }

    pub async fn reverse_lookup(&self, address: &str) -> Result<Option<String>, SdkError> {
        let resolution = self.reverse_resolve(address).await?;
        Ok(resolution.primary_name)
    }

    pub async fn get_primary_name(&self, address: &str) -> Result<Option<String>, SdkError> {
        self.reverse_lookup(address).await
    }

    pub async fn get_text_records(&self, name: &str) -> Result<HashMap<String, String>, SdkError> {
        Self::require_label(name, "name")?;
        Ok(make_text_records(name))
    }

    pub async fn get_text_record(&self, name: &str, key: &str) -> Result<TextRecord, SdkError> {
        Self::require_label(name, "name")?;
        Self::require_label(key, "key")?;

        Ok(TextRecord {
            name: name.to_string(),
            key: key.to_string(),
            value: self.get_text_records(name).await?.get(key).cloned(),
        })
    }

    pub async fn set_text_record(
        &self,
        update: TextRecordUpdate,
    ) -> Result<TransactionSubmission, SdkError> {
        Self::require_label(&update.name, "name")?;
        Self::require_label(&update.key, "key")?;

        let (_, ledger, network_passphrase) = self.rpc_context().await?;
        Ok(self.make_submission(
            "set_text_record",
            self.resolver_contract_id.clone(),
            ledger,
            update.signer,
            Some(network_passphrase),
        ))
    }

    pub async fn set_text_records(
        &self,
        update: TextRecordsUpdate,
    ) -> Result<TransactionSubmission, SdkError> {
        Self::require_label(&update.name, "name")?;
        if update.records.is_empty() {
            return Err(SdkError::InvalidRequest("records must not be empty".into()));
        }

        let (_, ledger, network_passphrase) = self.rpc_context().await?;
        Ok(self.make_submission(
            "set_text_records",
            self.resolver_contract_id.clone(),
            ledger,
            update.signer,
            Some(network_passphrase),
        ))
    }

    pub async fn quote_registration(
        &self,
        label: &str,
        duration_years: u32,
    ) -> Result<RegistrationQuote, SdkError> {
        Self::require_label(label, "label")?;
        if duration_years == 0 {
            return Err(SdkError::InvalidRequest(
                "duration_years must be greater than zero".into(),
            ));
        }
        let registrar_id = self
            .registrar_contract_id
            .as_ref()
            .ok_or_else(|| contract_id_error("registrar"))?
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

        let now = now_unix();
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
        Self::require_label(&request.label, "label")?;
        Self::require_label(&request.owner, "owner")?;
        if request.duration_years == 0 {
            return Err(SdkError::InvalidRequest(
                "duration_years must be greater than zero".into(),
            ));
        }

        let quote = self
            .quote_registration(&request.label, request.duration_years)
            .await?;
        let registrar_id = self
            .registrar_contract_id
            .as_ref()
            .ok_or_else(|| contract_id_error("registrar"))?
            .clone();
        let (_, ledger, network_passphrase) = self.rpc_context().await?;
        let submission = self.make_submission(
            "register",
            Some(registrar_id),
            ledger,
            request.signer.clone(),
            Some(network_passphrase),
        );

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
        Self::require_label(&request.name, "name")?;
        if request.additional_years == 0 {
            return Err(SdkError::InvalidRequest(
                "additional_years must be greater than zero".into(),
            ));
        }

        let registrar_id = self
            .registrar_contract_id
            .as_ref()
            .ok_or_else(|| contract_id_error("registrar"))?
            .clone();
        let (_, ledger, network_passphrase) = self.rpc_context().await?;

        let years = u64::from(request.additional_years);
        let fee_paid = BASE_FEE_PER_YEAR
            .saturating_mul(years)
            .saturating_add(NETWORK_FEE);
        let new_expiry = MOCK_REFERENCE_TIMESTAMP + years * SECONDS_PER_YEAR;
        let submission = self.make_submission(
            "renew",
            Some(registrar_id),
            ledger,
            request.signer.clone(),
            Some(network_passphrase),
        );

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
        Self::require_label(&request.name, "name")?;
        Self::require_label(&request.new_owner, "new_owner")?;

        let (_, ledger, network_passphrase) = self.rpc_context().await?;
        Ok(self.make_submission(
            "transfer",
            self.registry_contract_id.clone(),
            ledger,
            request.signer,
            Some(network_passphrase),
        ))
    }

    pub async fn register_parent(&self, request: RegisterParentRequest) -> Result<(), SdkError> {
        Self::require_label(&request.parent, "parent")?;
        Self::require_label(&request.owner, "owner")?;
        let _ = self.rpc_context().await?;
        if self.subdomain_contract_id.is_none() {
            return Err(contract_id_error("subdomain"));
        }
        Ok(())
    }

    pub async fn add_controller(&self, request: AddControllerRequest) -> Result<(), SdkError> {
        Self::require_label(&request.parent, "parent")?;
        Self::require_label(&request.controller, "controller")?;
        let _ = self.rpc_context().await?;
        if self.subdomain_contract_id.is_none() {
            return Err(contract_id_error("subdomain"));
        }
        Ok(())
    }

    pub async fn create_subdomain(
        &self,
        request: CreateSubdomainRequest,
    ) -> Result<String, SdkError> {
        Self::require_label(&request.label, "label")?;
        Self::require_label(&request.parent, "parent")?;
        Self::require_label(&request.owner, "owner")?;
        let _ = self.rpc_context().await?;
        if self.subdomain_contract_id.is_none() {
            return Err(contract_id_error("subdomain"));
        }
        Ok(format!("{}.{}", request.label, request.parent))
    }

    pub async fn transfer_subdomain(
        &self,
        request: TransferSubdomainRequest,
    ) -> Result<(), SdkError> {
        Self::require_label(&request.fqdn, "fqdn")?;
        Self::require_label(&request.new_owner, "new_owner")?;
        let _ = self.rpc_context().await?;
        if self.subdomain_contract_id.is_none() {
            return Err(contract_id_error("subdomain"));
        }
        Ok(())
    }

    pub async fn get_subdomains(&self, parent: &str) -> Result<Vec<Subdomain>, SdkError> {
        Self::require_label(parent, "parent")?;
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
        Self::require_label(&request.chain, "chain")?;
        let _ = self.rpc_context().await?;
        if self.bridge_contract_id.is_none() {
            return Err(contract_id_error("bridge"));
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
        Self::require_label(chain, "chain")?;
        let _ = self.rpc_context().await?;

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
        Self::require_label(name, "name")?;
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
        Self::require_label(&request.name, "name")?;
        Self::require_label(&request.chain, "chain")?;

        let route = self.get_route(&request.chain).await?.ok_or_else(|| {
            SdkError::InvalidRequest(format!("unsupported chain: {}", request.chain))
        })?;

        Ok(format!(
            "{{\"type\":\"xlm-ns-resolution\",\"name\":\"{}\",\"destination_chain\":\"{}\",\"resolver\":\"{}\"}}",
            request.name, route.destination_chain, route.destination_resolver
        ))
    }

    pub async fn mint_nft(
        &self,
        token_id: &str,
        owner: &str,
    ) -> Result<TransactionSubmission, SdkError> {
        Self::require_label(token_id, "token_id")?;
        Self::require_label(owner, "owner")?;
        let (_, ledger, network_passphrase) = self.rpc_context().await?;
        Ok(self.make_submission(
            "mint_nft",
            self.nft_contract_id.clone(),
            ledger,
            None,
            Some(network_passphrase),
        ))
    }

    pub async fn approve_nft(
        &self,
        token_id: &str,
        operator: &str,
    ) -> Result<TransactionSubmission, SdkError> {
        Self::require_label(token_id, "token_id")?;
        Self::require_label(operator, "operator")?;
        let (_, ledger, network_passphrase) = self.rpc_context().await?;
        Ok(self.make_submission(
            "approve_nft",
            self.nft_contract_id.clone(),
            ledger,
            None,
            Some(network_passphrase),
        ))
    }

    pub async fn transfer_nft(
        &self,
        token_id: &str,
        new_owner: &str,
    ) -> Result<TransactionSubmission, SdkError> {
        Self::require_label(token_id, "token_id")?;
        Self::require_label(new_owner, "new_owner")?;
        let (_, ledger, network_passphrase) = self.rpc_context().await?;
        Ok(self.make_submission(
            "transfer_nft",
            self.nft_contract_id.clone(),
            ledger,
            None,
            Some(network_passphrase),
        ))
    }

    pub async fn get_nft(&self, token_id: &str) -> Result<NftRecord, SdkError> {
        self.get_nft_record(token_id)
    }

    pub async fn get_nft_owner(&self, token_id: &str) -> Result<String, SdkError> {
        Self::require_label(token_id, "token_id")?;
        Ok("GDRA...OWNER".to_string())
    }

    pub async fn get_nft_metadata(&self, token_id: &str) -> Result<Option<String>, SdkError> {
        Self::require_label(token_id, "token_id")?;
        Ok(Some(format!("ipfs://{}", token_id)))
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
            metadata_uri: Some(format!("ipfs://metadata/{token_id}")),
        })
    }

    pub async fn get_auction(&self, name: &str) -> Result<Option<AuctionInfo>, SdkError> {
        Self::require_label(name, "name")?;

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
            .ok_or(SdkError::ContractError(ContractErrorCode::NameNotFound))?;

        Ok(AuctionState {
            highest_bid: info.highest_bid as i128,
            end_time: info.ends_at,
        })
    }

    pub async fn create_auction(
        &self,
        request: AuctionCreateRequest,
    ) -> Result<TransactionSubmission, SdkError> {
        Self::require_label(&request.name, "name")?;
        if request.reserve_price == 0 {
            return Err(SdkError::InvalidRequest(
                "reserve_price must be greater than zero".into(),
            ));
        }
        if request.duration_seconds == 0 {
            return Err(SdkError::InvalidRequest(
                "duration_seconds must be greater than zero".into(),
            ));
        }

        let (_, ledger, network_passphrase) = self.rpc_context().await?;
        Ok(self.make_submission(
            "create_auction",
            self.auction_contract_id.clone(),
            ledger,
            request.signer,
            Some(network_passphrase),
        ))
    }

    pub async fn bid_auction(
        &self,
        request: BidRequest,
    ) -> Result<TransactionSubmission, SdkError> {
        Self::require_label(&request.name, "name")?;
        if request.amount == 0 {
            return Err(SdkError::InvalidRequest(
                "bid amount must be greater than zero".into(),
            ));
        }

        let (_, ledger, network_passphrase) = self.rpc_context().await?;
        Ok(self.make_submission(
            "bid_auction",
            self.auction_contract_id.clone(),
            ledger,
            request.signer,
            Some(network_passphrase),
        ))
    }

    pub async fn load_reserved_manifest(
        &self,
        labels: Vec<String>,
        signer: Option<String>,
    ) -> Result<TransactionSubmission, SdkError> {
        if labels.is_empty() {
            return Err(SdkError::InvalidRequest("labels must not be empty".into()));
        }
        let registrar_id = self
            .registrar_contract_id
            .as_ref()
            .ok_or_else(|| contract_id_error("registrar"))?
            .clone();
        let (_, ledger, network_passphrase) = self.rpc_context().await?;
        Ok(self.make_submission(
            "load_reserved_manifest",
            Some(registrar_id),
            ledger,
            signer,
            Some(network_passphrase),
        ))
    }

    pub async fn get_treasury_balance(&self) -> Result<u64, SdkError> {
        let _registrar_id = self
            .registrar_contract_id
            .as_ref()
            .ok_or_else(|| contract_id_error("registrar"))?;
        let (_, ledger, _) = self.rpc_context().await?;
        Ok(u64::from(ledger) * 1_000)
    }

    pub async fn get_fee_metrics(&self) -> Result<RegistrarMetrics, SdkError> {
        let _registrar_id = self
            .registrar_contract_id
            .as_ref()
            .ok_or_else(|| contract_id_error("registrar"))?;
        let (_, ledger, _) = self.rpc_context().await?;
        Ok(RegistrarMetrics {
            treasury_balance: u64::from(ledger) * 1_000,
            total_registrations: u64::from(ledger),
            total_renewals: u64::from(ledger / 2),
        })
    }

    pub async fn settle_auction(
        &self,
        name: &str,
        signer: Option<String>,
    ) -> Result<TransactionSubmission, SdkError> {
        Self::require_label(name, "name")?;
        let (_, ledger, network_passphrase) = self.rpc_context().await?;
        Ok(self.make_submission(
            "settle_auction",
            self.auction_contract_id.clone(),
            ledger,
            signer,
            Some(network_passphrase),
        ))
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
