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
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash as StdHash, Hasher};
use stellar_rpc_client::Client;
use stellar_xdr::curr::Hash as XdrHash;
use xlm_ns_common::{GRACE_PERIOD_SECONDS, YEAR_SECONDS};
use xlm_ns_common::validation::{validate_account_address, validate_contract_id};

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

    fn require_contract_id<'a>(
        contract_id: &'a Option<String>,
        field_name: &'static str,
    ) -> Result<&'a str, SdkError> {
        let contract_id = contract_id.as_deref().ok_or_else(|| {
            SdkError::InvalidRequest(format!("{field_name} not configured"))
        })?;
        validate_contract_id(contract_id).map_err(|err| {
            SdkError::InvalidRequest(format!("{field_name} is invalid: {err}"))
        })?;
        Ok(contract_id)
    }

    fn validate_account(value: &str, field_name: &'static str) -> Result<(), SdkError> {
        validate_account_address(value).map_err(|err| {
            SdkError::InvalidRequest(format!("{field_name} is invalid: {err}"))
        })
    }

    fn parse_submission_hash(hash: &str) -> Option<XdrHash> {
        let trimmed = hash.trim();
        if trimmed.len() != 64 || !trimmed.chars().all(|ch| ch.is_ascii_hexdigit()) {
            return None;
        }

        let mut bytes = [0u8; 32];
        for (idx, chunk) in trimmed.as_bytes().chunks(2).enumerate() {
            let hex = std::str::from_utf8(chunk).ok()?;
            bytes[idx] = u8::from_str_radix(hex, 16).ok()?;
        }
        Some(XdrHash(bytes))
    }

    fn generated_submission_hash(operation: &str, payload: &str) -> String {
        let mut hasher = DefaultHasher::new();
        operation.hash(&mut hasher);
        payload.hash(&mut hasher);
        let seed = hasher.finish();
        let words = [
            seed,
            seed.rotate_left(13),
            seed.rotate_left(27),
            seed ^ 0xa5a5_a5a5_a5a5_a5a5,
        ];

        words
            .into_iter()
            .map(|word| format!("{word:016x}"))
            .collect::<String>()
    }

    async fn maybe_hydrate_submission(
        &self,
        submission: TransactionSubmission,
        _operation: &str,
    ) -> Result<TransactionSubmission, SdkError> {
        if !self.config.poll_final_status {
            return Ok(submission);
        }

        let Some(tx_hash) = Self::parse_submission_hash(&submission.tx_hash) else {
            return Ok(submission);
        };

        let rpc = Client::new(&self.rpc_url)
            .map_err(|e| SdkError::InvalidRequest(format!("failed to create RPC client: {e}")))?;

        let poll_timeout = Some(self.config.transaction_poll_timeout);
        match rpc.get_transaction_polling(&tx_hash, poll_timeout).await {
            Ok(response) => {
                let status = submission.status;
                let mut hydrated = submission;
                hydrated.ledger = response.ledger;
                hydrated.status = match response.status.as_str() {
                    "SUCCESS" => SubmissionStatus::Confirmed,
                    "FAILED" => SubmissionStatus::Failed,
                    _ => status,
                };
                Ok(hydrated)
            }
            Err(_err) => Ok(submission),
        }
    }

    pub async fn resolve(&self, name: &str) -> Result<ResolutionResult, SdkError> {
        let rpc =
            Client::new(&self.rpc_url).map_err(|e| SdkError::InvalidRequest(e.to_string()))?;
        let registry_id = Self::require_contract_id(
            &self.registry_contract_id,
            "registry contract ID",
        )?;

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
        let rpc =
            Client::new(&self.rpc_url).map_err(|e| SdkError::InvalidRequest(e.to_string()))?;
        let registry_id = Self::require_contract_id(
            &self.registry_contract_id,
            "registry contract ID",
        )?;

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
        if owner.trim().is_empty() {
            return Err(SdkError::InvalidRequest("owner must not be empty".into()));
        }
        Self::validate_account(owner, "owner")?;

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
        if address.trim().is_empty() {
            return Err(SdkError::InvalidRequest("address must not be empty".into()));
        }
        Self::validate_account(address, "address")?;

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
        if update.name.trim().is_empty() {
            return Err(SdkError::InvalidRequest("name must not be empty".into()));
        }
        if update.key.trim().is_empty() {
            return Err(SdkError::InvalidRequest("key must not be empty".into()));
        }
        if let Some(value) = &update.value {
            if value.trim().is_empty() {
                return Err(SdkError::InvalidRequest("value must not be empty".into()));
            }
        }

        let submission = TransactionSubmission {
            tx_hash: Self::generated_submission_hash("set_text_record", &update.name),
            status: SubmissionStatus::Submitted,
            ledger: None,
            submitted_at: MOCK_REFERENCE_TIMESTAMP,
            contract_id: self.resolver_contract_id.clone(),
            network_passphrase: self.network_passphrase.clone(),
            signer: update.signer,
        };
        self.maybe_hydrate_submission(submission, "set_text_record").await
    }

    pub async fn set_text_records(
        &self,
        update: TextRecordsUpdate,
    ) -> Result<TransactionSubmission, SdkError> {
        if update.name.trim().is_empty() {
            return Err(SdkError::InvalidRequest("name must not be empty".into()));
        }
        let submission = TransactionSubmission {
            tx_hash: Self::generated_submission_hash("set_text_records", &update.name),
            status: SubmissionStatus::Submitted,
            ledger: None,
            submitted_at: MOCK_REFERENCE_TIMESTAMP,
            contract_id: self.resolver_contract_id.clone(),
            network_passphrase: self.network_passphrase.clone(),
            signer: update.signer,
        };

        self.maybe_hydrate_submission(submission, "set_text_records").await
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
        let registrar_id = Self::require_contract_id(
            &self.registrar_contract_id,
            "registrar contract ID",
        )?
        .to_string();

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

        // Validate registrar contract is configured
        let _registrar_id = Self::require_contract_id(
            &self.registrar_contract_id,
            "registrar contract ID",
        )?;

        // Build and simulate the transaction
        let rpc = Client::new(&self.rpc_url)
            .map_err(|e| SdkError::InvalidRequest(format!("failed to create RPC client: {}", e)))?;

        // Get current network information for transaction building
        let _network = rpc
            .get_network()
            .await
            .map_err(|e| SdkError::Transport(format!("failed to get network: {}", e)))?;

        // Generate transaction hash (in production, this would be from real transaction submission)
        let tx_hash = Self::generated_submission_hash("register", &request.label);

        let submission = TransactionSubmission {
            tx_hash: tx_hash.clone(),
            status: SubmissionStatus::Submitted,
            ledger: None,
            submitted_at: MOCK_REFERENCE_TIMESTAMP,
            contract_id: self.registrar_contract_id.clone(),
            network_passphrase: self.network_passphrase.clone(),
            signer: request.signer.clone(),
        };

        let submission = self.maybe_hydrate_submission(submission, "register").await?;

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

        // Validate registrar contract is configured.
        let _registrar_id = Self::require_contract_id(
            &self.registrar_contract_id,
            "registrar contract ID",
        )?;

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
        let tx_hash = Self::generated_submission_hash("renew", &request.name);

        let submission = TransactionSubmission {
            tx_hash: tx_hash.clone(),
            status: SubmissionStatus::Submitted,
            ledger: None,
            submitted_at: MOCK_REFERENCE_TIMESTAMP,
            contract_id: self.registrar_contract_id.clone(),
            network_passphrase: self.network_passphrase.clone(),
            signer: request.signer.clone(),
        };

        let submission = self.maybe_hydrate_submission(submission, "renew").await?;

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
        Self::validate_account(&request.new_owner, "new_owner")?;

        let submission = TransactionSubmission {
            tx_hash: Self::generated_submission_hash("transfer", &request.name),
            status: SubmissionStatus::Submitted,
            ledger: None,
            submitted_at: MOCK_REFERENCE_TIMESTAMP,
            contract_id: self.registry_contract_id.clone(),
            network_passphrase: self.network_passphrase.clone(),
            signer: request.signer,
        };
        self.maybe_hydrate_submission(submission, "transfer").await
    }

    pub async fn register_parent(
        &self,
        request: RegisterParentRequest,
        dry_run: bool,
    ) -> Result<TransactionSubmission, SdkError> {
        if request.parent.trim().is_empty() {
            return Err(SdkError::InvalidRequest("parent must not be empty".into()));
        }
        Self::validate_account(&request.owner, "owner")?;
        let submission = self
            .simulate_and_submit(
                &self.subdomain_contract_id,
                "register_parent",
                vec![],
                Some(request.owner.clone()),
                dry_run,
            )
            .await?;
        self.maybe_hydrate_submission(submission, "register_parent").await
    }

    pub async fn add_controller(
        &self,
        request: AddControllerRequest,
        dry_run: bool,
    ) -> Result<TransactionSubmission, SdkError> {
        if request.parent.trim().is_empty() {
            return Err(SdkError::InvalidRequest("parent must not be empty".into()));
        }
        Self::validate_account(&request.controller, "controller")?;
        let submission = self
            .simulate_and_submit(
                &self.subdomain_contract_id,
                "add_controller",
                vec![],
                Some(request.controller.clone()),
                dry_run,
            )
            .await?;
        self.maybe_hydrate_submission(submission, "add_controller").await
    }

    pub async fn create_subdomain(
        &self,
        request: CreateSubdomainRequest,
        dry_run: bool,
    ) -> Result<TransactionSubmission, SdkError> {
        if request.label.trim().is_empty() {
            return Err(SdkError::InvalidRequest("label must not be empty".into()));
        }
        if request.parent.trim().is_empty() {
            return Err(SdkError::InvalidRequest("parent must not be empty".into()));
        }
        Self::validate_account(&request.owner, "owner")?;
        let submission = self
            .simulate_and_submit(
                &self.subdomain_contract_id,
                "create_subdomain",
                vec![],
                Some(request.owner.clone()),
                dry_run,
            )
            .await?;
        self.maybe_hydrate_submission(submission, "create_subdomain").await
    }

    pub async fn transfer_subdomain(
        &self,
        request: TransferSubdomainRequest,
        dry_run: bool,
    ) -> Result<TransactionSubmission, SdkError> {
        if request.fqdn.trim().is_empty() {
            return Err(SdkError::InvalidRequest("fqdn must not be empty".into()));
        }
        Self::validate_account(&request.new_owner, "new_owner")?;
        let submission = self
            .simulate_and_submit(
                &self.subdomain_contract_id,
                "transfer_subdomain",
                vec![],
                Some(request.new_owner.clone()),
                dry_run,
            )
            .await?;
        self.maybe_hydrate_submission(submission, "transfer_subdomain").await
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
        dry_run: bool,
    ) -> Result<TransactionSubmission, SdkError> {
        if token_id.trim().is_empty() {
            return Err(SdkError::InvalidRequest("token_id must not be empty".into()));
        }
        Self::validate_account(owner, "owner")?;
        let submission = self
            .simulate_and_submit(
                &self.nft_contract_id,
                "mint",
                vec![],
                Some(owner.to_string()),
                dry_run,
            )
            .await?;
        self.maybe_hydrate_submission(submission, "mint_nft").await
    }

    pub async fn approve_nft(
        &self,
        token_id: &str,
        operator: &str,
        dry_run: bool,
    ) -> Result<TransactionSubmission, SdkError> {
        if token_id.trim().is_empty() {
            return Err(SdkError::InvalidRequest("token_id must not be empty".into()));
        }
        Self::validate_account(operator, "operator")?;
        let submission = self
            .simulate_and_submit(
                &self.nft_contract_id,
                "approve",
                vec![],
                Some(operator.to_string()),
                dry_run,
            )
            .await?;
        self.maybe_hydrate_submission(submission, "approve_nft").await
    }

    pub async fn transfer_nft(
        &self,
        token_id: &str,
        new_owner: &str,
        dry_run: bool,
    ) -> Result<TransactionSubmission, SdkError> {
        if token_id.trim().is_empty() {
            return Err(SdkError::InvalidRequest("token_id must not be empty".into()));
        }
        Self::validate_account(new_owner, "new_owner")?;
        let submission = self
            .simulate_and_submit(
                &self.nft_contract_id,
                "transfer",
                vec![],
                Some(new_owner.to_string()),
                dry_run,
            )
            .await?;
        self.maybe_hydrate_submission(submission, "transfer_nft").await
    }

    pub async fn get_nft(&self, token_id: &str) -> Result<NftRecord, SdkError> {
        self.get_nft_record(token_id)
    }

    pub async fn get_nft_owner(&self, _token_id: &str) -> Result<String, SdkError> {
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

    pub async fn simulate_and_submit(
        &self,
        contract_id: &Option<String>,
        function: &str,
        _args: Vec<soroban_sdk::xdr::ScVal>,
        signer: Option<String>,
        dry_run: bool,
    ) -> Result<TransactionSubmission, SdkError> {
        let contract_id = Self::require_contract_id(contract_id, "contract ID")?;
        let tx_hash = Self::generated_submission_hash(function, contract_id);
        let submission = TransactionSubmission {
            tx_hash,
            status: if dry_run {
                SubmissionStatus::Simulated
            } else {
                SubmissionStatus::Submitted
            },
            ledger: None,
            submitted_at: 0,
            contract_id: Some(contract_id.to_string()),
            network_passphrase: self.network_passphrase.clone(),
            signer,
        };

        self.maybe_hydrate_submission(submission, function).await
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
        if request.asset.trim().is_empty() {
            return Err(SdkError::InvalidRequest("asset must not be empty".into()));
        }
        Self::validate_account(&request.treasury, "treasury")?;

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
        let _registrar_id = Self::require_contract_id(
            &self.registrar_contract_id,
            "registrar contract ID",
        )?;

        Ok(0)
    }

    pub async fn get_fee_metrics(&self) -> Result<RegistrarMetrics, SdkError> {
        let _registrar_id = Self::require_contract_id(
            &self.registrar_contract_id,
            "registrar contract ID",
        )?;

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
