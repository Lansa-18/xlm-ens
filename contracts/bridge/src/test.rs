#[cfg(test)]
mod tests {
    use soroban_sdk::{Env, String};

    use crate::{BridgeContract, BridgeContractClient};

    #[test]
    fn stores_bridge_routes_in_contract_storage() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(BridgeContract, ());
        let client = BridgeContractClient::new(&env, &contract_id);

        let base = String::from_str(&env, "base");
        let name = String::from_str(&env, "timmy.xlm");

        client.register_chain(&base);
        let route = client.route(&base).unwrap();
        let payload = client.build_message(&name, &base);

        assert_eq!(
            route.destination_resolver,
            String::from_str(&env, "0xbaseResolver")
        );
        assert!(payload.to_string().contains("timmy.xlm"));
        env.as_contract(&contract_id, || {
            assert!(env.storage().persistent().has(&crate::DataKey::Route(base)));
        });
    }

    #[test]
    fn build_message_exact_payload_base_chain() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(BridgeContract, ());
        let client = BridgeContractClient::new(&env, &contract_id);

        let chain = String::from_str(&env, "base");
        let name = String::from_str(&env, "alice.xlm");

        client.register_chain(&chain);
        let payload = client.build_message(&name, &chain);

        // Assert exact payload structure with field order
        let expected = String::from_str(
            &env,
            r#"{"type":"xlm-ns-resolution","name":"alice.xlm","destination_chain":"base","resolver":"0xbaseResolver"}"#,
        );
        assert_eq!(payload, expected);
    }

    #[test]
    fn build_message_exact_payload_ethereum_chain() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(BridgeContract, ());
        let client = BridgeContractClient::new(&env, &contract_id);

        let chain = String::from_str(&env, "ethereum");
        let name = String::from_str(&env, "bob.xlm");

        client.register_chain(&chain);
        let payload = client.build_message(&name, &chain);

        // Assert exact payload structure with field order
        let expected = String::from_str(
            &env,
            r#"{"type":"xlm-ns-resolution","name":"bob.xlm","destination_chain":"ethereum","resolver":"0xethResolver"}"#,
        );
        assert_eq!(payload, expected);
    }

    #[test]
    fn build_message_exact_payload_arbitrum_chain() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(BridgeContract, ());
        let client = BridgeContractClient::new(&env, &contract_id);

        let chain = String::from_str(&env, "arbitrum");
        let name = String::from_str(&env, "charlie.xlm");

        client.register_chain(&chain);
        let payload = client.build_message(&name, &chain);

        // Assert exact payload structure with field order
        let expected = String::from_str(
            &env,
            r#"{"type":"xlm-ns-resolution","name":"charlie.xlm","destination_chain":"arbitrum","resolver":"0xarbResolver"}"#,
        );
        assert_eq!(payload, expected);
    }

    #[test]
    fn build_message_payload_with_long_name() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(BridgeContract, ());
        let client = BridgeContractClient::new(&env, &contract_id);

        let chain = String::from_str(&env, "base");
        let name = String::from_str(&env, "very-long-domain-name.xlm");

        client.register_chain(&chain);
        let payload = client.build_message(&name, &chain);

        // Assert exact payload with longer name
        let expected = String::from_str(
            &env,
            r#"{"type":"xlm-ns-resolution","name":"very-long-domain-name.xlm","destination_chain":"base","resolver":"0xbaseResolver"}"#,
        );
        assert_eq!(payload, expected);
    }

    #[test]
    fn build_message_payload_special_characters() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(BridgeContract, ());
        let client = BridgeContractClient::new(&env, &contract_id);

        let chain = String::from_str(&env, "ethereum");
        let name = String::from_str(&env, "test-name.xlm");

        client.register_chain(&chain);
        let payload = client.build_message(&name, &chain);

        // Assert exact payload preserves special characters
        let expected = String::from_str(
            &env,
            r#"{"type":"xlm-ns-resolution","name":"test-name.xlm","destination_chain":"ethereum","resolver":"0xethResolver"}"#,
        );
        assert_eq!(payload, expected);
    }

    #[test]
    fn build_message_payload_version_locked() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(BridgeContract, ());
        let client = BridgeContractClient::new(&env, &contract_id);

        let chain = String::from_str(&env, "base");
        let name = String::from_str(&env, "version.xlm");

        client.register_chain(&chain);
        let payload = client.build_message(&name, &chain);

        // Verify payload type field is locked to "xlm-ns-resolution"
        assert!(payload
            .to_string()
            .starts_with(r#"{"type":"xlm-ns-resolution""#));

        // Verify exact field order: type, name, destination_chain, resolver
        let expected = String::from_str(
            &env,
            r#"{"type":"xlm-ns-resolution","name":"version.xlm","destination_chain":"base","resolver":"0xbaseResolver"}"#,
        );
        assert_eq!(payload, expected);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #2)")]
    fn build_message_fails_for_unregistered_chain() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(BridgeContract, ());
        let client = BridgeContractClient::new(&env, &contract_id);

        let chain = String::from_str(&env, "polygon");
        let name = String::from_str(&env, "test.xlm");

        // Should panic because chain is not registered (Error code #2 = UnsupportedChain)
        client.build_message(&name, &chain);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #1)")]
    fn build_message_fails_for_invalid_name() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(BridgeContract, ());
        let client = BridgeContractClient::new(&env, &contract_id);

        let chain = String::from_str(&env, "base");
        let invalid_name = String::from_str(&env, "invalid");

        client.register_chain(&chain);
        // Should panic because name doesn't end with .xlm (Error code #1 = Validation)
        client.build_message(&invalid_name, &chain);
    }

    #[test]
    fn threat_unauthorized_actor_cannot_register_chain() {
        let env = Env::default();
        let contract_id = env.register(BridgeContract, ());
        let client = BridgeContractClient::new(&env, &contract_id);

        let chain = String::from_str(&env, "base");

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.register_chain(&chain);
        }));

        assert!(
            result.is_err(),
            "registration of chain without auth should fail"
        );
    }
}
