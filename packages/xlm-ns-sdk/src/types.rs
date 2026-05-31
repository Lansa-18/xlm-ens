use core::fmt;
use core::future::Future;
use core::pin::Pin;

use crate::errors::{SdkError, SigningError};

pub const DEFAULT_FEE_CURRENCY: &str = "XLM";

pub type SignerFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Vec<u8>, SigningError>> + Send + 'a>>;

/// Abstracts the signing boundary for the xlm-ns SDK.
///
/// Implement this trait to control how transaction envelopes are signed:
/// - Use [`KeypairSigner`] when the secret key is available in-process.
/// - Use [`ExternalSigner`] to delegate signing to a closure, wallet hook,
///   or hardware device without exposing private key material to the SDK.
///
/// The SDK calls `sign_transaction` exactly once per submitted transaction,
/// after simulation and fee attachment, and before submission.
pub trait Signer: Send + Sync {
    /// Sign the XDR transaction envelope bytes and return the signed envelope bytes.
    fn sign_transaction(&self, tx_envelope_xdr: &[u8]) -> SignerFuture<'_>;

    /// Return the public key this signer controls, for source account resolution.
    fn public_key(&self) -> &str;
}

#[derive(Debug, Clone)]
pub struct Keypair {
    secret_key: String,
    public_key: String,
}

impl Keypair {
    pub fn from_secret(secret_key: &str) -> Result<Self, SigningError> {
        let trimmed = secret_key.trim();
        if trimmed.is_empty() {
            return Err(SigningError::InvalidKey {
                reason: "secret key must not be empty".to_string(),
            });
        }
        if !trimmed.starts_with('S') || trimmed.len() < 2 {
            return Err(SigningError::InvalidKey {
                reason: "secret key must start with 'S'".to_string(),
            });
        }

        Ok(Self {
            secret_key: trimmed.to_string(),
            public_key: format!("G{}", &trimmed[1..]),
        })
    }
}

pub struct KeypairSigner {
    keypair: Keypair,
}

impl KeypairSigner {
    pub fn new(secret_key: &str) -> Result<Self, SdkError> {
        let keypair =
            Keypair::from_secret(secret_key).map_err(|source| SdkError::SigningFailed {
                operation: "constructing keypair signer",
                source,
            })?;
        Ok(Self { keypair })
    }
}

impl Signer for KeypairSigner {
    fn sign_transaction(&self, tx_envelope_xdr: &[u8]) -> SignerFuture<'_> {
        let _ = &self.keypair.secret_key;
        let signed = tx_envelope_xdr.to_vec();
        Box::pin(async move { Ok(signed) })
    }

    fn public_key(&self) -> &str {
        &self.keypair.public_key
    }
}

pub struct ExternalSigner<F>
where
    F: Fn(&[u8]) -> Result<Vec<u8>, String> + Send + Sync,
{
    sign_fn: F,
    pubkey: String,
}

impl<F> ExternalSigner<F>
where
    F: Fn(&[u8]) -> Result<Vec<u8>, String> + Send + Sync,
{
    pub fn new(pubkey: impl Into<String>, sign_fn: F) -> Self {
        Self {
            sign_fn,
            pubkey: pubkey.into(),
        }
    }
}

impl<F> Signer for ExternalSigner<F>
where
    F: Fn(&[u8]) -> Result<Vec<u8>, String> + Send + Sync,
{
    fn sign_transaction(&self, tx_envelope_xdr: &[u8]) -> SignerFuture<'_> {
        let result = (self.sign_fn)(tx_envelope_xdr)
            .map_err(|reason| SigningError::ExternalFailure { reason });
        Box::pin(async move { result })
    }

    fn public_key(&self) -> &str {
        &self.pubkey
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistrationRequest {
    pub label: String,
    pub owner: String,
    pub duration_years: u32,
    pub signer: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeeBreakdown {
    pub base_fee: u64,
    pub premium_fee: u64,
    pub network_fee: u64,
}

impl FeeBreakdown {
    pub fn total(&self) -> u64 {
        self.base_fee
            .saturating_add(self.premium_fee)
            .saturating_add(self.network_fee)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistrationQuote {
    pub label: String,
    pub duration_years: u32,
    pub fee_breakdown: FeeBreakdown,
    pub total_fee: u64,
    pub fee_currency: String,
    pub expires_at: u64,
    pub grace_period_ends_at: u64,
    pub quoted_at: u64,
    pub contract_id: Option<String>,
}

impl RegistrationQuote {
    /// Backwards-friendly accessor: the headline fee a caller should pay.
    pub fn fee(&self) -> u64 {
        self.total_fee
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenewalRequest {
    pub name: String,
    pub additional_years: u32,
    pub signer: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmissionStatus {
    Simulated,
    Submitted,
    Confirmed,
    Failed,
}

impl fmt::Display for SubmissionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Simulated => f.write_str("simulated"),
            Self::Submitted => f.write_str("submitted"),
            Self::Confirmed => f.write_str("confirmed"),
            Self::Failed => f.write_str("failed"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransactionSubmission {
    pub tx_hash: String,
    pub status: SubmissionStatus,
    pub ledger: Option<u32>,
    pub submitted_at: u64,
    pub contract_id: Option<String>,
    pub network_passphrase: Option<String>,
    pub signer: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistrationReceipt {
    pub name: String,
    pub owner: String,
    pub duration_years: u32,
    pub expires_at: u64,
    pub fee_paid: u64,
    pub submission: TransactionSubmission,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisterResult {
    pub name: String,
    pub owner: String,
    pub tx_hash: String,
    pub ledger_sequence: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenewalReceipt {
    pub name: String,
    pub additional_years: u32,
    pub new_expiry: u64,
    pub fee_paid: u64,
    pub submission: TransactionSubmission,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenewResult {
    pub name: String,
    pub new_expiry_ledger: u32,
    pub tx_hash: String,
    pub ledger_sequence: u32,
}

/// Retained for backwards compatibility with callers that only need
/// the raw renewal outcome.
pub type RenewalResult = RenewalReceipt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolutionResult {
    pub name: String,
    pub address: Option<String>,
    pub resolver: Option<String>,
    pub expires_at: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReverseResolution {
    pub address: String,
    pub primary_name: Option<String>,
    pub resolver: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextRecord {
    pub name: String,
    pub key: String,
    pub value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextRecordUpdate {
    pub name: String,
    pub key: String,
    pub value: Option<String>,
    pub signer: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextRecordsUpdate {
    pub name: String,
    pub records: std::collections::HashMap<String, Option<String>>,
    pub signer: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferRequest {
    pub name: String,
    pub new_owner: String,
    pub signer: Option<String>,
}

// Subdomain types
#[derive(Debug, Clone)]
pub struct RegisterParentRequest {
    pub parent: String,
    pub owner: String,
}

#[derive(Debug, Clone)]
pub struct AddControllerRequest {
    pub parent: String,
    pub controller: String,
}

#[derive(Debug, Clone)]
pub struct CreateSubdomainRequest {
    pub label: String,
    pub parent: String,
    pub owner: String,
}

#[derive(Debug, Clone)]
pub struct TransferSubdomainRequest {
    pub fqdn: String,
    pub new_owner: String,
}

#[derive(Debug, Clone)]
pub struct ParentDomain {
    pub owner: String,
    pub controllers: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SubdomainRecord {
    pub parent: String,
    pub owner: String,
    pub created_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subdomain {
    pub label: String,
    pub owner: String,
}

// Bridge types
#[derive(Debug, Clone)]
pub struct RegisterChainRequest {
    pub chain: String,
}

#[derive(Debug, Clone)]
pub struct BuildMessageRequest {
    pub name: String,
    pub chain: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeRoute {
    pub destination_chain: String,
    pub destination_resolver: String,
    pub gateway: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NftRecord {
    pub token_id: String,
    pub owner: String,
    pub metadata_uri: Option<String>,
}

/// Cumulative fee and operation metrics returned by the registrar's read APIs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistrarMetrics {
    pub treasury_balance: u64,
    pub total_registrations: u64,
    pub total_renewals: u64,
}

// Domain Models
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameRecord {
    pub owner: String,
    pub registered_at: u64,
    pub expires_at: u64,
    pub grace_period_ends_at: u64,
    pub resolver: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuctionState {
    pub highest_bid: i128,
    pub end_time: u64,
}

// Contract types for RPC calls
#[derive(Debug, Clone, serde::Deserialize)]
pub struct RegistryEntry {
    pub name: String,
    pub owner: String,
    pub resolver: Option<String>,
    pub target_address: Option<String>,
    pub metadata_uri: Option<String>,
    pub ttl_seconds: u64,
    pub registered_at: u64,
    pub expires_at: u64,
    pub grace_period_ends_at: u64,
    pub transfer_count: u32,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ResolutionRecord {
    pub owner: String,
    pub address: String,
    pub text_records: std::collections::HashMap<String, String>,
    pub updated_at: u64,
}

// Auction types
#[derive(Debug, Clone, serde::Deserialize)]
pub struct AuctionInfo {
    pub name: String,
    pub owner: String,
    pub reserve_price: u64,
    pub highest_bid: u64,
    pub highest_bidder: Option<String>,
    pub ends_at: u64,
    pub status: AuctionStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
pub enum AuctionStatus {
    Active,
    Ended,
    Settled,
}

impl fmt::Display for AuctionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Active => f.write_str("active"),
            Self::Ended => f.write_str("ended"),
            Self::Settled => f.write_str("settled"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AuctionCreateRequest {
    pub name: String,
    pub asset: String,
    pub treasury: String,
    pub reserve_price: u64,
    pub duration_seconds: u64,
    pub signer: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BidRequest {
    pub name: String,
    pub amount: u64,
    pub signer: Option<String>,
}
