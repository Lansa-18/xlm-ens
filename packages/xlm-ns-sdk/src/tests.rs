#![allow(
    dead_code,
    unused_variables,
    clippy::module_inception,
    clippy::single_match,
    clippy::duplicated_attributes
)]
#![allow(
    dead_code,
    unused_variables,
    clippy::module_inception,
    clippy::single_match,
    clippy::duplicated_attributes
)]
#![allow(
    dead_code,
    unused_variables,
    clippy::module_inception,
    clippy::single_match
)]
#![allow(
    dead_code,
    unused_variables,
    clippy::module_inception,
    clippy::single_match
)]
#[cfg(test)]
mod tests {
    use crate::client::XlmNsClient;
    use crate::errors::SdkError;
    use crate::network;
    use crate::types::{
        RegistrationRequest, RenewalRequest, SubmissionStatus, TextRecordUpdate, TextRecordsUpdate,
        TransferRequest,
    };
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;
    use stellar_rpc_client::Client;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

    /// A wiremock responder that echoes the JSON-RPC request ID
    /// from the incoming request, so jsonrpsee can match it.
    struct JsonRpcResponder {
        result: serde_json::Value,
    }
    impl JsonRpcResponder {
        fn new(result: serde_json::Value) -> Self {
            Self { result }
        }
    }
    impl Respond for JsonRpcResponder {
        fn respond(&self, request: &Request) -> ResponseTemplate {
            let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap_or_default();
            let id = body.get("id").cloned().unwrap_or(serde_json::json!(1));
            ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": self.result,
            }))
        }
    }

    /// Returns HTTP errors for the first N requests, then a JSON-RPC success body.
    struct FailThenSucceed {
        failures_before_success: Arc<AtomicUsize>,
        success_body: serde_json::Value,
    }

    impl Respond for FailThenSucceed {
        fn respond(&self, request: &Request) -> ResponseTemplate {
            let remaining = self.failures_before_success.fetch_sub(1, Ordering::SeqCst);
            if remaining > 0 {
                ResponseTemplate::new(503)
            } else {
                JsonRpcResponder::new(self.success_body.clone()).respond(request)
            }
        }
    }

    fn retry_test_client(
        rpc_url: impl Into<String>,
        config: crate::config::ClientConfig,
    ) -> XlmNsClient {
        XlmNsClient::builder(rpc_url)
            .network_passphrase("Test SDF Network ; September 2015")
            .registry(REGISTRY_ID)
            .registrar(REGISTRAR_ID)
            .config(config)
            .build()
    }

    fn network_success_body() -> serde_json::Value {
        serde_json::json!({
            "passphrase": "Test SDF Network ; September 2015",
            "protocolVersion": 21
        })
    }

    // Valid 56-char Stellar contract IDs (C-prefix, all alphanumeric).
    const REGISTRY_ID: &str = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    const REGISTRAR_ID: &str = "CBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB";
    const RESOLVER_ID: &str = "CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC";
    const AUCTION_ID: &str = "CDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDD";
    const BRIDGE_ID: &str = "CEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEE";
    const SUBDOMAIN_ID: &str = "CFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF";
    const NFT_ID: &str = "CGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGG";
    // Valid 56-char Stellar account addresses (G-prefix, all alphanumeric).
    const OWNER_ADDR: &str = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    const NEW_OWNER_ADDR: &str = "GBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB";
    const LOOKUP_ADDR: &str = "GCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC";

    fn client() -> XlmNsClient {
        XlmNsClient::builder("http://localhost")
            .network_passphrase("Test SDF Network ; September 2015")
            .registry(REGISTRY_ID)
            .subdomain(SUBDOMAIN_ID)
            .bridge(BRIDGE_ID)
            .auction(AUCTION_ID)
            .registrar(REGISTRAR_ID)
            .resolver(RESOLVER_ID)
            .build()
    }

    #[tokio::test]
    async fn renewal_returns_rich_receipt() {
        let receipt = client()
            .renew(RenewalRequest {
                name: "test.xlm".into(),
                additional_years: 2,
                signer: Some("alice".into()),
            })
            .await
            .unwrap();

        assert_eq!(receipt.fee_paid, 21);
        assert_eq!(receipt.additional_years, 2);
        assert_eq!(receipt.submission.status, SubmissionStatus::Submitted);
        assert_eq!(receipt.submission.signer.as_deref(), Some("alice"));
        assert!(receipt.new_expiry > 1_682_200_000);
    }

    #[tokio::test]
    async fn registration_quote_exposes_breakdown() {
        // "alpha" = 5 chars → 250_000_000 stroops/year (contract tier: 4–6 chars)
        let quote = client().quote_registration("alpha", 3).await.unwrap();
        assert_eq!(quote.label, "alpha");
        assert_eq!(quote.duration_years, 3);
        assert_eq!(quote.fee_breakdown.base_fee, 750_000_000); // 250_000_000 × 3
        assert_eq!(quote.fee_breakdown.premium_fee, 0);
        assert_eq!(quote.fee_breakdown.network_fee, 0);
        assert_eq!(quote.total_fee, 750_000_000);
        assert_eq!(quote.fee_currency, "XLM");
        assert!(quote.contract_id.is_some());
        assert!(quote.expires_at > quote.quoted_at);
        assert!(quote.grace_period_ends_at > quote.expires_at);
    }

    #[tokio::test]
    async fn registration_receipt_carries_submission_metadata() {
        // "beta" = 4 chars → 250_000_000 stroops/year (contract tier: 4–6 chars)
        let receipt = client()
            .register(RegistrationRequest {
                label: "beta".into(),
                owner: OWNER_ADDR.into(),
                duration_years: 1,
                signer: Some("treasury".into()),
            })
            .await
            .unwrap();

        assert_eq!(receipt.name, "beta.xlm");
        assert_eq!(receipt.duration_years, 1);
        assert_eq!(receipt.fee_paid, 250_000_000); // 250_000_000 × 1
        assert_eq!(receipt.submission.signer.as_deref(), Some("treasury"));
        assert!(receipt.submission.network_passphrase.is_some());
    }

    #[tokio::test]
    async fn reverse_resolution_rejects_empty_address() {
        assert!(client().reverse_resolve("").await.is_err());
    }

    #[tokio::test]
    async fn text_record_round_trip() {
        let client = client();
        let record = client.get_text_record("foo.xlm", "url").await.unwrap();
        assert_eq!(record.name, "foo.xlm");
        assert_eq!(record.key, "url");

        let submission = client
            .set_text_record(TextRecordUpdate {
                name: "foo.xlm".into(),
                key: "url".into(),
                value: Some("https://example.xyz".into()),
                signer: Some("owner".into()),
            })
            .await
            .unwrap();
        assert_eq!(submission.status, SubmissionStatus::Submitted);
        assert_eq!(submission.signer.as_deref(), Some("owner"));
    }

    #[tokio::test]
    async fn text_records_batch_update() {
        let client = client();
        let mut records = HashMap::new();
        records.insert("url".to_string(), Some("https://example.xyz".to_string()));
        records.insert("avatar".to_string(), None);

        let submission = client
            .set_text_records(TextRecordsUpdate {
                name: "foo.xlm".into(),
                records,
                signer: Some("owner".into()),
            })
            .await
            .unwrap();
        assert_eq!(submission.status, SubmissionStatus::Submitted);
        assert_eq!(submission.signer.as_deref(), Some("owner"));
    }

    #[tokio::test]
    async fn transfer_returns_submission() {
        let submission = client()
            .transfer(TransferRequest {
                name: "foo.xlm".into(),
                new_owner: NEW_OWNER_ADDR.into(),
                signer: Some("alice".into()),
            })
            .await
            .unwrap();
        assert_eq!(submission.status, SubmissionStatus::Submitted);
        assert_eq!(submission.signer.as_deref(), Some("alice"));
    }

    #[tokio::test]
    async fn registry_metadata_returns_typed_record() {
        let metadata = client().get_registry_metadata("alice.xlm").await.unwrap();
        assert_eq!(metadata.owner, "GDRA...OWNER");
        assert!(metadata.expires_at > 0);
        assert!(metadata.resolver.is_some());
    }

    #[tokio::test]
    async fn owner_portfolio_returns_vec() {
        let portfolio = client().get_owner_portfolio(OWNER_ADDR).await.unwrap();
        assert!(!portfolio.is_empty());
        assert_eq!(portfolio[0].owner, OWNER_ADDR);
    }

    #[test]
    fn owner_portfolio_page_returns_cursor_and_total() {
        let first = client()
            .list_registrations_by_owner_page(OWNER_ADDR, None, 1)
            .unwrap();
        assert_eq!(first.items.len(), 1);
        assert_eq!(first.total, 2);
        assert_eq!(first.next_cursor, Some(1));

        let second = client()
            .list_registrations_by_owner_page(OWNER_ADDR, first.next_cursor, 1)
            .unwrap();
        assert_eq!(second.items.len(), 1);
        assert_eq!(second.total, 2);
        assert_eq!(second.next_cursor, None);
    }

    #[tokio::test]
    async fn auction_state_returns_typed_data() {
        let state = client().get_auction_state("active.xlm").await.unwrap();
        assert_eq!(state.highest_bid, 150);
        assert!(state.end_time > 0);
    }

    #[tokio::test]
    async fn auction_state_handles_not_found() {
        use crate::errors::ContractErrorCode;
        use crate::errors::SdkError;
        let result = client().get_auction_state("missing.xlm").await;
        match result {
            Err(SdkError::ContractError(ContractErrorCode::NameNotFound)) => {}
            _ => panic!("Expected NameNotFound error"),
        }
    }

    #[tokio::test]
    async fn resolver_primary_name_returns_option() {
        let name = client().get_primary_name(LOOKUP_ADDR).await.unwrap();
        assert_eq!(name, Some("primary.xlm".to_string()));
    }

    #[tokio::test]
    async fn resolver_text_records_returns_hashmap() {
        let records = client().get_text_records("alice.xlm").await.unwrap();
        assert!(records.contains_key("url"));
        assert_eq!(records.get("url").unwrap(), "https://alice.xlm");
    }

    #[tokio::test]
    async fn builder_default_config_is_applied() {
        let client = client();
        assert_eq!(client.config.timeout, crate::config::DEFAULT_TIMEOUT);
        assert!(client.config.user_agent.starts_with("xlm-ns-sdk/"));
    }

    #[tokio::test]
    async fn builder_accepts_custom_config() {
        use crate::config::ClientConfig;
        use std::time::Duration;

        let client = XlmNsClient::builder("http://localhost")
            .registry(REGISTRY_ID)
            .config(
                ClientConfig::default()
                    .with_timeout(Duration::from_secs(2))
                    .with_max_retries(0)
                    .with_user_agent("integration-test/1.0"),
            )
            .build();

        assert_eq!(client.config.timeout, Duration::from_secs(2));
        assert_eq!(client.config.retry.max_retries, 0);
        assert_eq!(client.config.user_agent, "integration-test/1.0");
    }

    #[test]
    fn error_decoding_works() {
        use crate::errors::decode_error;
        use crate::errors::ContractErrorCode;
        assert_eq!(decode_error(1), ContractErrorCode::NameNotFound);
        assert_eq!(decode_error(2), ContractErrorCode::NotOwner);
        assert_eq!(decode_error(99), ContractErrorCode::Other);
    }

    #[tokio::test]
    async fn test_verify_passphrase_happy_path() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/"))
            .respond_with(JsonRpcResponder::new(serde_json::json!({
                "passphrase": "Test SDF Network ; September 2015",
                "protocolVersion": 21
            })))
            .mount(&mock_server)
            .await;
        let http_client = Client::new(&mock_server.uri()).unwrap();

        let result = network::verify_network_passphrase(
            "Test SDF Network ; September 2015",
            &mock_server.uri(),
            &http_client,
        )
        .await;

        assert!(result.is_ok(), "expected Ok but got: {:?}", result);
    }

    #[tokio::test]
    async fn test_verify_passphrase_mismatch_returns_error() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/"))
            .respond_with(JsonRpcResponder::new(serde_json::json!({
                "passphrase": "Public Global Stellar Network ; September 2015",
                "protocolVersion": 21
            })))
            .mount(&mock_server)
            .await;
        let http_client = Client::new(&mock_server.uri()).unwrap();

        let result = network::verify_network_passphrase(
            "Test SDF Network ; September 2015",
            &mock_server.uri(),
            &http_client,
        )
        .await;

        let err = result.unwrap_err();
        match err {
            SdkError::NetworkPassphraseMismatch {
                configured,
                rpc_reported,
            } => {
                assert_eq!(configured, "Test SDF Network ; September 2015");
                assert_eq!(
                    rpc_reported,
                    "Public Global Stellar Network ; September 2015"
                );
            }
            _ => panic!("wrong error variant"),
        }
    }

    #[tokio::test]
    async fn register_builds_real_submission() {
        // "gamma" = 5 chars → 250_000_000 stroops/year (contract tier: 4–6 chars)
        let receipt = client()
            .register(RegistrationRequest {
                label: "gamma".into(),
                owner: OWNER_ADDR.into(),
                duration_years: 2,
                signer: Some("registrar".into()),
            })
            .await
            .unwrap();

        assert_eq!(receipt.name, "gamma.xlm");
        assert_eq!(receipt.owner, OWNER_ADDR);
        assert_eq!(receipt.duration_years, 2);
        assert_eq!(receipt.fee_paid, 500_000_000); // 250_000_000 × 2
        assert_eq!(receipt.submission.status, SubmissionStatus::Submitted);
        assert_eq!(receipt.submission.signer.as_deref(), Some("registrar"));
        assert!(!receipt.submission.tx_hash.is_empty());
        assert!(receipt.submission.contract_id.is_some());
        assert!(receipt.expires_at > 1_682_200_000);
    }

    #[tokio::test]
    async fn register_rejects_empty_label() {
        let result = client()
            .register(RegistrationRequest {
                label: "".into(),
                owner: "GDRA...OWNER".into(),
                duration_years: 1,
                signer: None,
            })
            .await;

        assert!(result.is_err());
        match result {
            Err(SdkError::InvalidRequest(msg)) => {
                assert!(msg.contains("label") || msg.contains("empty"));
            }
            _ => panic!("Expected InvalidRequest error"),
        }
    }

    #[tokio::test]
    async fn register_rejects_empty_owner() {
        let result = client()
            .register(RegistrationRequest {
                label: "test".into(),
                owner: "".into(),
                duration_years: 1,
                signer: None,
            })
            .await;

        assert!(result.is_err());
        match result {
            Err(SdkError::InvalidRequest(msg)) => {
                assert!(msg.contains("owner") || msg.contains("empty"));
            }
            _ => panic!("Expected InvalidRequest error"),
        }
    }

    #[tokio::test]
    async fn register_rejects_zero_duration() {
        let result = client()
            .register(RegistrationRequest {
                label: "test".into(),
                owner: "GDRA...OWNER".into(),
                duration_years: 0,
                signer: None,
            })
            .await;

        assert!(result.is_err());
        match result {
            Err(SdkError::InvalidRequest(msg)) => {
                assert!(msg.contains("duration") || msg.contains("greater"));
            }
            _ => panic!("Expected InvalidRequest error"),
        }
    }

    #[tokio::test]
    async fn test_verify_passphrase_transport_failure() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&mock_server)
            .await;
        let http_client = Client::new(&mock_server.uri()).unwrap();

        let result = network::verify_network_passphrase(
            "Test SDF Network ; September 2015",
            &mock_server.uri(),
            &http_client,
        )
        .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            SdkError::NetworkPassphraseMismatch { .. } => {
                panic!("should be a transport error, not a mismatch")
            }
            _ => {}
        }
    }

    #[test]
    fn test_verify_transaction_passphrase_mismatch() {
        let result = network::verify_transaction_passphrase(
            "Test SDF Network ; September 2015",
            "Public Global Stellar Network ; September 2015",
        );

        let err = result.unwrap_err();
        match err {
            SdkError::TransactionPassphraseMismatch {
                configured,
                in_transaction,
            } => {
                assert_eq!(configured, "Test SDF Network ; September 2015");
                assert_eq!(
                    in_transaction,
                    "Public Global Stellar Network ; September 2015"
                );
            }
            _ => panic!("wrong error variant"),
        }
    }

    #[tokio::test]
    async fn renew_builds_real_submission() {
        let receipt = client()
            .renew(RenewalRequest {
                name: "delta.xlm".into(),
                additional_years: 3,
                signer: Some("owner".into()),
            })
            .await
            .unwrap();

        // Verify receipt structure carries tx metadata
        assert_eq!(receipt.name, "delta.xlm");
        assert_eq!(receipt.additional_years, 3);
        assert_eq!(receipt.fee_paid, 31); // 3 years * 10 base + 1 network
        assert_eq!(receipt.submission.status, SubmissionStatus::Submitted);
        assert_eq!(receipt.submission.signer.as_deref(), Some("owner"));
        assert!(!receipt.submission.tx_hash.is_empty());
        assert!(receipt.submission.contract_id.is_some());
        assert!(receipt.new_expiry > 1_682_200_000);
    }

    #[tokio::test]
    async fn renew_rejects_empty_name() {
        let result = client()
            .renew(RenewalRequest {
                name: "".into(),
                additional_years: 1,
                signer: None,
            })
            .await;

        assert!(result.is_err());
        match result {
            Err(SdkError::InvalidRequest(msg)) => {
                assert!(msg.contains("name") || msg.contains("empty"));
            }
            _ => panic!("Expected InvalidRequest error"),
        }
    }

    #[tokio::test]
    async fn renew_rejects_zero_years() {
        let result = client()
            .renew(RenewalRequest {
                name: "test.xlm".into(),
                additional_years: 0,
                signer: None,
            })
            .await;

        assert!(result.is_err());
        match result {
            Err(SdkError::InvalidRequest(msg)) => {
                assert!(msg.contains("additional_years") || msg.contains("greater"));
            }
            _ => panic!("Expected InvalidRequest error"),
        }
    }

    #[tokio::test]
    async fn quote_requires_registrar_contract() {
        let no_registrar = XlmNsClient::builder("http://localhost")
            .registry(REGISTRY_ID)
            .build();

        let result = no_registrar.quote_registration("alpha", 1).await;
        match result {
            Err(SdkError::InvalidRequest(msg)) => {
                assert!(msg.contains("registrar"));
            }
            _ => panic!("Expected InvalidRequest when registrar contract ID is missing"),
        }
    }

    #[tokio::test]
    async fn register_requires_registrar_contract() {
        let no_registrar_client = XlmNsClient::builder("http://localhost")
            .registry(REGISTRY_ID)
            .build();

        let result = no_registrar_client
            .register(RegistrationRequest {
                label: "test".into(),
                owner: "GDRA...OWNER".into(),
                duration_years: 1,
                signer: None,
            })
            .await;

        assert!(result.is_err());
        match result {
            Err(SdkError::InvalidRequest(msg)) => {
                assert!(msg.contains("registrar"));
            }
            _ => panic!("Expected InvalidRequest error for missing registrar"),
        }
    }

    #[tokio::test]
    async fn renew_requires_registrar_contract() {
        let no_registrar_client = XlmNsClient::builder("http://localhost")
            .registry(REGISTRY_ID)
            .build();

        let result = no_registrar_client
            .renew(RenewalRequest {
                name: "test.xlm".into(),
                additional_years: 1,
                signer: None,
            })
            .await;

        assert!(result.is_err());
        match result {
            Err(SdkError::InvalidRequest(msg)) => {
                assert!(msg.contains("registrar"));
            }
            _ => panic!("Expected InvalidRequest error for missing registrar"),
        }
    }

    #[tokio::test]
    async fn submission_includes_fee_breakdown() {
        // "epsilon" = 7 chars → 100_000_000 stroops/year (contract tier: 7+ chars)
        let quote = client().quote_registration("epsilon", 4).await.unwrap();

        assert_eq!(quote.fee_breakdown.base_fee, 400_000_000); // 100_000_000 × 4
        assert_eq!(quote.fee_breakdown.premium_fee, 0);
        assert_eq!(quote.fee_breakdown.network_fee, 0);
        assert_eq!(quote.total_fee, 400_000_000);
        assert!(quote.grace_period_ends_at > quote.expires_at);

        let receipt = client()
            .register(RegistrationRequest {
                label: "epsilon".into(),
                owner: OWNER_ADDR.into(),
                duration_years: 4,
                signer: None,
            })
            .await
            .unwrap();

        assert_eq!(receipt.fee_paid, 400_000_000);
        assert_eq!(
            receipt.submission.network_passphrase,
            Some("Test SDF Network ; September 2015".into())
        );
    }

    #[tokio::test]
    async fn load_reserved_manifest_returns_submission() {
        let submission = client()
            .load_reserved_manifest(
                vec!["admin".to_string(), "root".to_string()],
                Some("deployer".into()),
            )
            .await
            .unwrap();

        assert_eq!(submission.status, SubmissionStatus::Submitted);
        assert_eq!(submission.signer.as_deref(), Some("deployer"));
    }

    // Issue #167 — simulation-first transaction assembly

    #[tokio::test]
    async fn simulate_register_surfaces_fee_estimate() {
        // "alpha" = 5 chars → 250_000_000 stroops/year × 2 years
        let result = client()
            .simulate_register(&RegistrationRequest {
                label: "alpha".into(),
                owner: OWNER_ADDR.into(),
                duration_years: 2,
                signer: None,
            })
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(result.fee_estimate, 500_000_000);
        assert!(!result.auth_addresses.is_empty());
        assert!(result.error.is_none());
    }

    #[tokio::test]
    async fn simulate_renew_surfaces_fee_estimate() {
        let result = client()
            .simulate_renew(&RenewalRequest {
                name: "test.xlm".into(),
                additional_years: 3,
                signer: None,
            })
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.fee_estimate > 0);
        assert!(result.error.is_none());
    }

    #[tokio::test]
    async fn simulate_register_requires_registrar_contract() {
        let no_registrar = XlmNsClient::builder("http://localhost")
            .registry(REGISTRY_ID)
            .build();

        let result = no_registrar
            .simulate_register(&RegistrationRequest {
                label: "alpha".into(),
                owner: OWNER_ADDR.into(),
                duration_years: 1,
                signer: None,
            })
            .await;

        match result {
            Err(SdkError::InvalidRequest(msg)) => {
                assert!(msg.contains("registrar"));
            }
            _ => panic!("Expected InvalidRequest when registrar contract ID is missing"),
        }
    }

    #[tokio::test]
    async fn simulate_renew_requires_registrar_contract() {
        let no_registrar = XlmNsClient::builder("http://localhost")
            .registry(REGISTRY_ID)
            .build();

        let result = no_registrar
            .simulate_renew(&RenewalRequest {
                name: "test.xlm".into(),
                additional_years: 1,
                signer: None,
            })
            .await;

        match result {
            Err(SdkError::InvalidRequest(msg)) => {
                assert!(msg.contains("registrar"));
            }
            _ => panic!("Expected InvalidRequest when registrar contract ID is missing"),
        }
    }

    // Issue #168 — SDK config expansion

    #[test]
    fn builder_from_preset_testnet_sets_rpc_and_passphrase() {
        use crate::config::NetworkPreset;
        let client = XlmNsClient::builder_from_preset(NetworkPreset::Testnet).build();
        assert!(client.rpc_url.contains("testnet"));
        assert_eq!(
            client.network_passphrase.as_deref(),
            Some("Test SDF Network ; September 2015")
        );
    }

    #[test]
    fn builder_from_preset_mainnet_sets_rpc_and_passphrase() {
        use crate::config::NetworkPreset;
        let client = XlmNsClient::builder_from_preset(NetworkPreset::Mainnet).build();
        assert!(client.rpc_url.contains("soroban.stellar.org"));
        assert_eq!(
            client.network_passphrase.as_deref(),
            Some("Public Global Stellar Network ; September 2015")
        );
    }

    #[test]
    fn missing_resolver_contract_id_is_none() {
        let c = XlmNsClient::builder("http://localhost")
            .registry("CDAD...REGISTRY")
            .build();
        assert!(c.resolver_contract_id.is_none());
    }

    #[test]
    fn missing_nft_contract_id_is_none() {
        let c = XlmNsClient::builder("http://localhost").build();
        assert!(c.nft_contract_id.is_none());
    }

    #[test]
    fn missing_bridge_contract_id_is_none() {
        let c = XlmNsClient::builder("http://localhost").build();
        assert!(c.bridge_contract_id.is_none());
    }

    #[test]
    fn missing_auction_contract_id_is_none() {
        let c = XlmNsClient::builder("http://localhost").build();
        assert!(c.auction_contract_id.is_none());
    }

    #[test]
    fn missing_subdomain_contract_id_is_none() {
        let c = XlmNsClient::builder("http://localhost").build();
        assert!(c.subdomain_contract_id.is_none());
    }

    #[test]
    fn fully_specified_builder_sets_all_contract_ids() {
        let c = XlmNsClient::builder("http://localhost")
            .registry("CDAD...REGISTRY")
            .registrar("CDAD...REGISTRAR")
            .resolver("CDAD...RESOLVER")
            .auction("CDAD...AUCTION")
            .bridge("CDAD...BRIDGE")
            .subdomain("CDAD...SUBDOMAIN")
            .nft("CDAD...NFT")
            .build();

        assert!(c.registry_contract_id.is_some());
        assert!(c.registrar_contract_id.is_some());
        assert!(c.resolver_contract_id.is_some());
        assert!(c.auction_contract_id.is_some());
        assert!(c.bridge_contract_id.is_some());
        assert!(c.subdomain_contract_id.is_some());
        assert!(c.nft_contract_id.is_some());
    }

    // Issue #486 — RPC retry with exponential backoff

    #[tokio::test]
    async fn retry_succeeds_after_transient_transport_failures() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/"))
            .respond_with(FailThenSucceed {
                failures_before_success: Arc::new(AtomicUsize::new(2)),
                success_body: network_success_body(),
            })
            .expect(3)
            .mount(&mock_server)
            .await;

        let client = retry_test_client(
            mock_server.uri(),
            crate::config::ClientConfig::default()
                .with_max_retries(3)
                .with_initial_backoff(Duration::from_millis(1))
                .with_jitter(false)
                .with_poll_final_status(false),
        );

        let receipt = client
            .renew(RenewalRequest {
                name: "retry.xlm".into(),
                additional_years: 1,
                signer: None,
            })
            .await
            .unwrap();

        assert_eq!(receipt.name, "retry.xlm");
    }

    #[tokio::test]
    async fn retry_does_not_retry_non_retryable_passphrase_mismatch() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/"))
            .respond_with(JsonRpcResponder::new(serde_json::json!({
                "passphrase": "Public Global Stellar Network ; September 2015",
                "protocolVersion": 21
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = retry_test_client(
            mock_server.uri(),
            crate::config::ClientConfig::default()
                .with_max_retries(3)
                .with_initial_backoff(Duration::from_millis(1))
                .with_jitter(false)
                .with_poll_final_status(false),
        );

        let err = client
            .renew(RenewalRequest {
                name: "retry.xlm".into(),
                additional_years: 1,
                signer: None,
            })
            .await
            .unwrap_err();

        match err {
            SdkError::NetworkPassphraseMismatch { .. } => {}
            other => panic!("expected passphrase mismatch, got {other:?}"),
        }
    }

    #[tokio::test(flavor = "current_thread", start_paused = true)]
    async fn retry_honors_exponential_backoff_delays() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/"))
            .respond_with(FailThenSucceed {
                failures_before_success: Arc::new(AtomicUsize::new(2)),
                success_body: network_success_body(),
            })
            .mount(&mock_server)
            .await;

        let client = retry_test_client(
            mock_server.uri(),
            crate::config::ClientConfig::default()
                .with_max_retries(3)
                .with_initial_backoff(Duration::from_millis(100))
                .with_jitter(false)
                .with_poll_final_status(false),
        );

        let renew = client.renew(RenewalRequest {
            name: "retry.xlm".into(),
            additional_years: 1,
            signer: None,
        });
        tokio::pin!(renew);

        tokio::select! {
            _ = &mut renew => panic!("renew finished before first backoff elapsed"),
            _ = tokio::time::sleep(Duration::from_millis(99)) => {}
        }

        tokio::select! {
            _ = &mut renew => panic!("renew finished before second backoff elapsed"),
            _ = tokio::time::sleep(Duration::from_millis(201)) => {}
        }

        let result = renew.await.unwrap();
        assert_eq!(result.name, "retry.xlm");
    }
}
