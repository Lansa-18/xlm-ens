//! Blocking SDK surface.
//!
//! [`XlmNsBlockingClient`] is a thin wrapper around the async [`XlmNsClient`]
//! that owns its own single-threaded `tokio` runtime and exposes every async
//! method as a synchronous one. It exists so callers in synchronous codebases
//! (CLIs, build scripts, simple bots) can use the SDK without taking a
//! dependency on `tokio` or wrapping every call in `block_on` themselves.
//!
//! New code in async services should prefer [`XlmNsClient`] directly — the
//! blocking client is implemented on top of the same async methods, so the
//! async path is the source of truth.
//!
//! ```ignore
//! use xlm_ns_sdk::{XlmNsClient, XlmNsBlockingClient};
//!
//! let async_client = XlmNsClient::builder("https://soroban-rpc.example")
//!     .registry("CDAD...REGISTRY")
//!     .build();
//! let client = XlmNsBlockingClient::from_async(async_client).unwrap();
//! let resolution = client.resolve("alice.xlm").unwrap();
//! ```

use std::sync::Arc;

use tokio::runtime::{Builder as RuntimeBuilder, Runtime};

use crate::client::XlmNsClient;
use crate::config::ClientConfig;
use crate::errors::SdkError;
use crate::types::{
    AddControllerRequest, AuctionCreateRequest, AuctionInfo, BidRequest, BridgeRoute,
    BuildMessageRequest, CreateSubdomainRequest, NftRecord, RegisterChainRequest,
    RegisterParentRequest, RegistrationQuote, RegistrationReceipt, RegistrationRequest,
    RenewalReceipt, RenewalRequest, ResolutionResult, ReverseResolution, TextRecord,
    TextRecordUpdate, TextRecordsUpdate, TransactionSubmission, TransferRequest, TransferSubdomainRequest,
};

/// A synchronous facade over [`XlmNsClient`].
///
/// Owns a single-threaded `tokio` current-thread runtime so each call drives
/// the async method to completion on the caller's thread. Cheap to clone
/// (the runtime is reference-counted).
#[derive(Clone)]
pub struct XlmNsBlockingClient {
    inner: XlmNsClient,
    runtime: Arc<Runtime>,
}

impl XlmNsBlockingClient {
    /// Wrap an existing async client. The blocking client takes ownership of
    /// `inner` and drives every method on a freshly-built current-thread
    /// runtime.
    pub fn from_async(inner: XlmNsClient) -> Result<Self, SdkError> {
        let runtime = RuntimeBuilder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| SdkError::Transport(format!("failed to start runtime: {e}")))?;
        Ok(Self {
            inner,
            runtime: Arc::new(runtime),
        })
    }

    /// Convenience constructor that mirrors [`XlmNsClient::new`].
    pub fn new(
        rpc_url: impl Into<String>,
        passphrase: Option<String>,
        registry_contract_id: Option<String>,
        subdomain_contract_id: Option<String>,
        bridge_contract_id: Option<String>,
        auction_contract_id: Option<String>,
    ) -> Result<Self, SdkError> {
        Self::from_async(XlmNsClient::new(
            rpc_url,
            passphrase,
            registry_contract_id,
            subdomain_contract_id,
            bridge_contract_id,
            auction_contract_id,
        ))
    }

    /// Borrow the underlying async client. Useful for callers that need the
    /// async API for a specific call path while keeping the blocking client
    /// for everything else.
    pub fn as_async(&self) -> &XlmNsClient {
        &self.inner
    }

    /// Clone the underlying async client.
    pub fn into_async(self) -> XlmNsClient {
        self.inner
    }

    /// Read the active transport configuration.
    pub fn config(&self) -> &ClientConfig {
        &self.inner.config
    }

    fn block_on<T, F>(&self, fut: F) -> T
    where
        F: std::future::Future<Output = T>,
    {
        self.runtime.block_on(fut)
    }

    // ── Read-path wrappers ────────────────────────────────────────────────

    pub fn resolve(&self, name: &str) -> Result<ResolutionResult, SdkError> {
        self.block_on(self.inner.resolve(name))
    }

    pub fn get_registration(&self, name: &str) -> Result<Option<ResolutionResult>, SdkError> {
        self.block_on(self.inner.get_registration(name))
    }

    pub fn list_registrations_by_owner(
        &self,
        owner: &str,
    ) -> Result<Vec<ResolutionResult>, SdkError> {
        self.inner.list_registrations_by_owner(owner)
    }

    pub fn reverse_resolve(&self, address: &str) -> Result<ReverseResolution, SdkError> {
        self.block_on(self.inner.reverse_resolve(address))
    }

    pub fn get_text_record(&self, name: &str, key: &str) -> Result<TextRecord, SdkError> {
        self.block_on(self.inner.get_text_record(name, key))
    }

    pub fn quote_registration(
        &self,
        label: &str,
        duration_years: u32,
    ) -> Result<RegistrationQuote, SdkError> {
        self.block_on(self.inner.quote_registration(label, duration_years))
    }

    pub fn get_route(&self, chain: &str) -> Result<Option<BridgeRoute>, SdkError> {
        self.block_on(self.inner.get_route(chain))
    }

    pub fn get_nft_record(&self, token_id: &str) -> Result<NftRecord, SdkError> {
        self.inner.get_nft_record(token_id)
    }

    pub fn get_auction(&self, name: &str) -> Result<Option<AuctionInfo>, SdkError> {
        self.block_on(self.inner.get_auction(name))
    }

    // ── Write-path wrappers ───────────────────────────────────────────────

    pub fn set_text_record(
        &self,
        update: TextRecordUpdate,
    ) -> Result<TransactionSubmission, SdkError> {
        self.block_on(self.inner.set_text_record(update))
    }

    pub fn set_text_records(
        &self,
        update: TextRecordsUpdate,
    ) -> Result<TransactionSubmission, SdkError> {
        self.block_on(self.inner.set_text_records(update))
    }

    pub fn register(&self, request: RegistrationRequest) -> Result<RegistrationReceipt, SdkError> {
        self.block_on(self.inner.register(request))
    }

    pub fn renew(&self, request: RenewalRequest) -> Result<RenewalReceipt, SdkError> {
        self.block_on(self.inner.renew(request))
    }

    pub fn transfer(&self, request: TransferRequest) -> Result<TransactionSubmission, SdkError> {
        self.block_on(self.inner.transfer(request))
    }

    pub fn register_parent(&self, request: RegisterParentRequest) -> Result<TransactionSubmission, SdkError> {
        self.block_on(self.inner.register_parent(request, false))
    }

    pub fn add_controller(&self, request: AddControllerRequest) -> Result<TransactionSubmission, SdkError> {
        self.block_on(self.inner.add_controller(request, false))
    }

    pub fn create_subdomain(&self, request: CreateSubdomainRequest) -> Result<TransactionSubmission, SdkError> {
        self.block_on(self.inner.create_subdomain(request, false))
    }

    pub fn transfer_subdomain(&self, request: TransferSubdomainRequest) -> Result<TransactionSubmission, SdkError> {
        self.block_on(self.inner.transfer_subdomain(request, false))
    }

    pub fn register_chain(&self, request: RegisterChainRequest) -> Result<(), SdkError> {
        self.block_on(self.inner.register_chain(request))
    }

    pub fn build_message(&self, request: BuildMessageRequest) -> Result<String, SdkError> {
        self.block_on(self.inner.build_message(request))
    }

    pub fn load_reserved_manifest(
        &self,
        labels: Vec<String>,
        signer: Option<String>,
    ) -> Result<TransactionSubmission, SdkError> {
        self.block_on(self.inner.load_reserved_manifest(labels, signer))
    }

    pub fn create_auction(
        &self,
        request: AuctionCreateRequest,
    ) -> Result<TransactionSubmission, SdkError> {
        self.block_on(self.inner.create_auction(request))
    }

    pub fn bid_auction(&self, request: BidRequest) -> Result<TransactionSubmission, SdkError> {
        self.block_on(self.inner.bid_auction(request))
    }

    pub fn settle_auction(
        &self,
        name: &str,
        signer: Option<String>,
    ) -> Result<TransactionSubmission, SdkError> {
        self.block_on(self.inner.settle_auction(name, signer))
    }
}

impl std::fmt::Debug for XlmNsBlockingClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("XlmNsBlockingClient")
            .field("inner", &self.inner)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{RenewalRequest, SubmissionStatus};

    fn blocking_client() -> XlmNsBlockingClient {
        XlmNsBlockingClient::from_async(
            XlmNsClient::builder("http://localhost")
                .network_passphrase("Test SDF Network ; September 2015")
                .registry("CDAD...REGISTRY")
                .registrar("CDAD...REGISTRAR")
                .build(),
        )
        .unwrap()
    }

    #[test]
    fn renew_runs_synchronously_via_blocking_facade() {
        let receipt = blocking_client()
            .renew(RenewalRequest {
                name: "blocking.xlm".into(),
                additional_years: 1,
                signer: Some("alice".into()),
            })
            .unwrap();

        assert_eq!(receipt.additional_years, 1);
        assert_eq!(receipt.submission.status, SubmissionStatus::Submitted);
    }

    #[test]
    fn config_passes_through_from_inner_client() {
        let client = blocking_client();
        let default_ua = crate::config::default_user_agent();
        assert_eq!(client.config().user_agent, default_ua);
    }
}
