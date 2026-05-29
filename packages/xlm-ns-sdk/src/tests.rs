#[cfg(test)]
mod tests {
    use crate::client::XlmNsClient;
    use crate::errors::SdkError;
    use crate::types::{
        RegistrationRequest, RenewalRequest, SubmissionStatus, TextRecordUpdate,
        TextRecordsUpdate, TransferRequest,
    };
    use std::collections::HashMap;

    fn client() -> XlmNsClient {
        XlmNsClient::builder("http://localhost")
            .network_passphrase("Test SDF Network ; September 2015")
            .registry("CDAD...REGISTRY")
            .subdomain("CDAD...SUBDOMAIN")
            .bridge("CDAD...BRIDGE")
            .auction("CDAD...AUCTION")
            .registrar("CDAD...REGISTRAR")
            .resolver("CDAD...RESOLVER")
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
                owner: "GDRA...OWNER".into(),
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
                new_owner: "GDRA...NEW".into(),
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
        let portfolio = client().get_owner_portfolio("GDRA...OWNER").await.unwrap();
        assert!(!portfolio.is_empty());
        assert_eq!(portfolio[0].owner, "GDRA...OWNER");
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
        let name = client().get_primary_name("GDRA...ADDR").await.unwrap();
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
            .registry("CDAD...REGISTRY")
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
    async fn register_builds_real_submission() {
        // "gamma" = 5 chars → 250_000_000 stroops/year (contract tier: 4–6 chars)
        let receipt = client()
            .register(RegistrationRequest {
                label: "gamma".into(),
                owner: "GDRA...OWNER".into(),
                duration_years: 2,
                signer: Some("registrar".into()),
            })
            .await
            .unwrap();

        assert_eq!(receipt.name, "gamma.xlm");
        assert_eq!(receipt.owner, "GDRA...OWNER");
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
            .registry("CDAD...REGISTRY")
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
            .registry("CDAD...REGISTRY")
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
            .registry("CDAD...REGISTRY")
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
                owner: "GDRA...OWNER".into(),
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
}
