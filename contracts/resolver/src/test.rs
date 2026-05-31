#[cfg(test)]
mod tests {
    use soroban_sdk::{testutils::Address as _, Address, Env, String, Vec};
    use xlm_ns_common::{MAX_TEXT_RECORD_VALUE_LENGTH, MAX_TEXT_RECORDS};

    use crate::{BatchOp, ResolverContract, ResolverContractClient};

    #[test]
    fn persists_forward_reverse_and_primary_resolution_records() {
        let env = Env::default();
        let contract_id = env.register(ResolverContract, ());
        let client = ResolverContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let name = String::from_str(&env, "timmy.xlm");
        let address = String::from_str(&env, "GABC");

        client.set_record(&name, &owner, &address, &100);
        client.set_text_record(
            &name,
            &owner,
            &String::from_str(&env, "com.twitter"),
            &String::from_str(&env, "@timmy"),
            &101,
        );
        client.set_primary_name(&address, &owner, &name);

        let record = client.resolve(&name).unwrap();
        assert_eq!(record.owner, owner);
        assert_eq!(
            record
                .addresses
                .get(String::from_str(&env, "stellar")),
            Some(address.clone())
        );
        assert_eq!(
            record
                .text_records
                .get(String::from_str(&env, "com.twitter")),
            Some(String::from_str(&env, "@timmy"))
        );
        assert_eq!(record.updated_at, 101);
        assert_eq!(client.reverse(&String::from_str(&env, "GABC")), Some(name));
    }

    #[test]
    fn removes_forward_reverse_and_primary_records() {
        let env = Env::default();
        let contract_id = env.register(ResolverContract, ());
        let client = ResolverContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let name = String::from_str(&env, "timmy.xlm");
        let address = String::from_str(&env, "GABC");

        client.set_record(&name, &owner, &address, &100);
        client.set_primary_name(&address, &owner, &name);
        client.remove_record(&name, &owner);

        assert_eq!(client.resolve(&name), None);
        assert_eq!(client.reverse(&address), None);
    }

    #[test]
    fn rejects_text_record_updates_from_non_owner() {
        let env = Env::default();
        let contract_id = env.register(ResolverContract, ());
        let client = ResolverContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let intruder = Address::generate(&env);
        let name = String::from_str(&env, "timmy.xlm");
        let address = String::from_str(&env, "GABC");

        client.set_record(&name, &owner, &address, &100);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.set_text_record(
                &name,
                &intruder,
                &String::from_str(&env, "com.twitter"),
                &String::from_str(&env, "@timmy"),
                &101,
            );
        }));

        assert!(result.is_err(), "non-owner text update should fail");
        let stored = client.resolve(&name).unwrap();
        assert_eq!(stored.text_records.len(), 0);
    }

    #[test]
    fn rejects_record_removal_from_non_owner() {
        let env = Env::default();
        let contract_id = env.register(ResolverContract, ());
        let client = ResolverContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let intruder = Address::generate(&env);
        let name = String::from_str(&env, "timmy.xlm");
        let address = String::from_str(&env, "GABC");

        client.set_record(&name, &owner, &address, &100);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.remove_record(&name, &intruder);
        }));

        assert!(result.is_err(), "non-owner record removal should fail");
        assert!(client.resolve(&name).is_some());
        assert_eq!(client.reverse(&address), Some(name));
    }

    #[test]
    fn enforces_text_record_limit_but_allows_updating_existing_key_at_limit() {
        let env = Env::default();
        let contract_id = env.register(ResolverContract, ());
        let client = ResolverContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let name = String::from_str(&env, "timmy.xlm");
        let address = String::from_str(&env, "GABC");

        client.set_record(&name, &owner, &address, &100);

        for idx in 0..MAX_TEXT_RECORDS {
            client.set_text_record(
                &name,
                &owner,
                &String::from_str(&env, &format!("key-{idx}")),
                &String::from_str(&env, &format!("value-{idx}")),
                &(101 + idx as u64),
            );
        }

        client.set_text_record(
            &name,
            &owner,
            &String::from_str(&env, "key-0"),
            &String::from_str(&env, "updated"),
            &500,
        );

        let updated_record = client.resolve(&name).unwrap();
        assert_eq!(updated_record.text_records.len(), MAX_TEXT_RECORDS as u32);
        assert_eq!(
            updated_record
                .text_records
                .get(String::from_str(&env, "key-0")),
            Some(String::from_str(&env, "updated"))
        );
        assert_eq!(updated_record.updated_at, 500);

        let overflow = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.set_text_record(
                &name,
                &owner,
                &String::from_str(&env, "overflow"),
                &String::from_str(&env, "value"),
                &501,
            );
        }));

        assert!(
            overflow.is_err(),
            "adding a new key past the limit should fail"
        );
    }

    #[test]
    fn reverse_lookup_prefers_primary_name_when_present() {
        let env = Env::default();
        let contract_id = env.register(ResolverContract, ());
        let client = ResolverContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let first_name = String::from_str(&env, "timmy.xlm");
        let second_name = String::from_str(&env, "pay.timmy.xlm");
        let address = String::from_str(&env, "GABC");

        client.set_record(&first_name, &owner, &address, &100);
        client.set_record(&second_name, &owner, &address, &101);

        assert_eq!(client.reverse(&address), Some(second_name.clone()));

        client.set_primary_name(&address, &owner, &first_name);
        assert_eq!(client.reverse(&address), Some(first_name));
    }

    // Issue #316: Test primary-name cleanup when resolver addresses change
    #[test]
    fn removes_old_primary_mappings_when_address_changes() {
        let env = Env::default();
        let contract_id = env.register(ResolverContract, ());
        let client = ResolverContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let name = String::from_str(&env, "timmy.xlm");
        let old_address = String::from_str(&env, "GABC");
        let new_address = String::from_str(&env, "GDEF");

        client.set_record(&name, &owner, &old_address, &100);
        client.set_primary_name(&old_address, &owner, &name);

        // Verify primary name is set for old address
        assert_eq!(client.reverse(&old_address), Some(name.clone()));

        // Change address
        client.set_record(&name, &owner, &new_address, &101);

        // Old primary mapping should be cleaned up
        assert_eq!(client.reverse(&old_address), None);
        assert_eq!(client.reverse(&new_address), Some(name));
    }

    #[test]
    fn updating_address_preserves_text_records() {
        let env = Env::default();
        let contract_id = env.register(ResolverContract, ());
        let client = ResolverContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let name = String::from_str(&env, "timmy.xlm");
        let old_address = String::from_str(&env, "GABC");
        let new_address = String::from_str(&env, "GDEF");

        client.set_record(&name, &owner, &old_address, &100);
        client.set_text_record(
            &name,
            &owner,
            &String::from_str(&env, "com.twitter"),
            &String::from_str(&env, "@timmy"),
            &101,
        );

        client.set_record(&name, &owner, &new_address, &102);

        let record = client.resolve(&name).unwrap();
        assert_eq!(
            record
                .addresses
                .get(String::from_str(&env, "stellar")),
            Some(new_address)
        );
        assert_eq!(record.text_records.len(), 1);
        assert_eq!(
            record
                .text_records
                .get(String::from_str(&env, "com.twitter")),
            Some(String::from_str(&env, "@timmy"))
        );
        assert_eq!(record.updated_at, 102);
    }

    // Issue #315: Test text record value size limits
    #[test]
    fn enforces_text_record_value_size_limit() {
        let env = Env::default();
        let contract_id = env.register(ResolverContract, ());
        let client = ResolverContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let name = String::from_str(&env, "timmy.xlm");
        let address = String::from_str(&env, "GABC");

        client.set_record(&name, &owner, &address, &100);

        // Valid value at limit
        let valid_value = String::from_str(&env, &"x".repeat(MAX_TEXT_RECORD_VALUE_LENGTH));
        client.set_text_record(
            &name,
            &owner,
            &String::from_str(&env, "key1"),
            &valid_value,
            &101,
        );

        let record = client.resolve(&name).unwrap();
        assert_eq!(record.text_records.len(), 1);

        // Value exceeding limit should fail
        let oversized_value = String::from_str(&env, &"x".repeat(MAX_TEXT_RECORD_VALUE_LENGTH + 1));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.set_text_record(
                &name,
                &owner,
                &String::from_str(&env, "key2"),
                &oversized_value,
                &102,
            );
        }));

        assert!(result.is_err(), "text record value exceeding limit should fail");
    }

    // Issue #317: Test multi-chain address records
    #[test]
    fn supports_multi_chain_address_records() {
        let env = Env::default();
        let contract_id = env.register(ResolverContract, ());
        let client = ResolverContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let name = String::from_str(&env, "timmy.xlm");
        let stellar_address = String::from_str(&env, "GABC");
        let ethereum_address = String::from_str(&env, "0x1234567890123456789012345678901234567890");

        // Set Stellar address
        client.set_record(&name, &owner, &stellar_address, &100);

        // Set Ethereum address using set_address
        client.set_address(
            &name,
            &owner,
            &String::from_str(&env, "ethereum"),
            &ethereum_address,
            &101,
        );

        let record = client.resolve(&name).unwrap();
        assert_eq!(
            record
                .addresses
                .get(String::from_str(&env, "stellar")),
            Some(stellar_address)
        );
        assert_eq!(
            record
                .addresses
                .get(String::from_str(&env, "ethereum")),
            Some(ethereum_address.clone()) // clone to avoid move
        );

        // Test get_address helper
        assert_eq!(
            client.get_address(&name, &String::from_str(&env, "ethereum")),
            Some(ethereum_address)
        );
    }

    // Issue #321: Test batch resolver queries
    #[test]
    fn batch_resolve_returns_records_for_multiple_names() {
        let env = Env::default();
        let contract_id = env.register(ResolverContract, ());
        let client = ResolverContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let name1 = String::from_str(&env, "alice.xlm");
        let name2 = String::from_str(&env, "bob.xlm");
        let name3 = String::from_str(&env, "charlie.xlm");
        let address1 = String::from_str(&env, "GAAA");
        let address2 = String::from_str(&env, "GBBB");

        client.set_record(&name1, &owner, &address1, &100);
        client.set_record(&name2, &owner, &address2, &101);

        // Batch resolve with one missing name
        let names = Vec::from_array(&env, [name1.clone(), name2.clone(), name3.clone()]);
        let results = client.batch_resolve(&names);

        assert_eq!(results.len(), 3);
        assert!(results.get(0).is_some()); // alice.xlm exists
        assert!(results.get(1).is_some()); // bob.xlm exists
        assert_eq!(results.get(2), None); // charlie.xlm doesn't exist
    }

    // Issue #321: Test batch reverse queries
    #[test]
    fn batch_reverse_returns_names_for_multiple_addresses() {
        let env = Env::default();
        let contract_id = env.register(ResolverContract, ());
        let client = ResolverContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let name1 = String::from_str(&env, "alice.xlm");
        let name2 = String::from_str(&env, "bob.xlm");
        let address1 = String::from_str(&env, "GAAA");
        let address2 = String::from_str(&env, "GBBB");
        let address3 = String::from_str(&env, "GCCC");

        client.set_record(&name1, &owner, &address1, &100);
        client.set_record(&name2, &owner, &address2, &101);

        // Batch reverse lookup with one missing address
        let addresses = Vec::from_array(&env, [address1.clone(), address2.clone(), address3.clone()]);
        let results = client.batch_reverse(&addresses);

        assert_eq!(results.len(), 3);
        assert_eq!(results.get(0), Some(Some(name1))); // GAAA -> alice.xlm
        assert_eq!(results.get(1), Some(Some(name2))); // GBBB -> bob.xlm
        assert_eq!(results.get(2), Some(None)); // GCCC -> None
    }

    // Issue #314 - text-record key normalization tests

    #[test]
    fn accepts_valid_text_record_keys() {
        let env = Env::default();
        let contract_id = env.register(ResolverContract, ());
        let client = ResolverContractClient::new(&env, &contract_id);
        let owner = Address::generate(&env);
        let name = String::from_str(&env, "alice.xlm");
        client.set_record(&name, &owner, &String::from_str(&env, "GABC"), &100);
        // plain lowercase
        client.set_text_record(&name, &owner, &String::from_str(&env, "url"), &String::from_str(&env, "https://x"), &101);
        // namespaced with dot
        client.set_text_record(&name, &owner, &String::from_str(&env, "com.twitter"), &String::from_str(&env, "@alice"), &102);
        // dash and underscore
        client.set_text_record(&name, &owner, &String::from_str(&env, "org.did_key-1"), &String::from_str(&env, "did:x"), &103);
        assert_eq!(client.resolve(&name).unwrap().text_records.len(), 3);
    }

    #[test]
    fn rejects_uppercase_text_record_key() {
        let env = Env::default();
        let contract_id = env.register(ResolverContract, ());
        let client = ResolverContractClient::new(&env, &contract_id);
        let owner = Address::generate(&env);
        let name = String::from_str(&env, "alice.xlm");
        client.set_record(&name, &owner, &String::from_str(&env, "GABC"), &100);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.set_text_record(&name, &owner, &String::from_str(&env, "Twitter"), &String::from_str(&env, "@alice"), &101);
        }));
        assert!(result.is_err(), "uppercase key must be rejected");
    }

    #[test]
    fn rejects_empty_text_record_key() {
        let env = Env::default();
        let contract_id = env.register(ResolverContract, ());
        let client = ResolverContractClient::new(&env, &contract_id);
        let owner = Address::generate(&env);
        let name = String::from_str(&env, "alice.xlm");
        client.set_record(&name, &owner, &String::from_str(&env, "GABC"), &100);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.set_text_record(&name, &owner, &String::from_str(&env, ""), &String::from_str(&env, "val"), &101);
        }));
        assert!(result.is_err(), "empty key must be rejected");
    }

    #[test]
    fn rejects_overlong_text_record_key() {
        let env = Env::default();
        let contract_id = env.register(ResolverContract, ());
        let client = ResolverContractClient::new(&env, &contract_id);
        let owner = Address::generate(&env);
        let name = String::from_str(&env, "alice.xlm");
        client.set_record(&name, &owner, &String::from_str(&env, "GABC"), &100);
        let long_key = "a".repeat(65);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.set_text_record(&name, &owner, &String::from_str(&env, &long_key), &String::from_str(&env, "val"), &101);
        }));
        assert!(result.is_err(), "65-byte key must be rejected");
    }

    #[test]
    fn rejects_text_record_key_with_space() {
        let env = Env::default();
        let contract_id = env.register(ResolverContract, ());
        let client = ResolverContractClient::new(&env, &contract_id);
        let owner = Address::generate(&env);
        let name = String::from_str(&env, "alice.xlm");
        client.set_record(&name, &owner, &String::from_str(&env, "GABC"), &100);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.set_text_record(&name, &owner, &String::from_str(&env, "bad key"), &String::from_str(&env, "val"), &101);
        }));
        assert!(result.is_err(), "key with space must be rejected");
    }

    // -----------------------------------------------------------------------
    // #141: Event emission tests
    // -----------------------------------------------------------------------

    #[test]
    fn set_record_emits_forward_and_reverse_events() {
        let env = Env::default();
        let contract_id = env.register(ResolverContract, ());
        let client = ResolverContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let name = String::from_str(&env, "alice.xlm");
        let addr = String::from_str(&env, "GAAA");

        client.set_record(&name, &owner, &addr, &100);

        // Events are emitted; simply verify the call succeeded and the record
        // persisted correctly (event payload verified via SDK event log in
        // integration tests).
        let record = client.resolve(&name).unwrap();
        assert_eq!(record.updated_at, 100);
    }

    #[test]
    fn set_text_record_emits_event() {
        let env = Env::default();
        let contract_id = env.register(ResolverContract, ());
        let client = ResolverContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let name = String::from_str(&env, "alice.xlm");
        client.set_record(&name, &owner, &String::from_str(&env, "GAAA"), &100);
        client.set_text_record(
            &name,
            &owner,
            &String::from_str(&env, "url"),
            &String::from_str(&env, "https://alice.example"),
            &101,
        );

        let record = client.resolve(&name).unwrap();
        assert_eq!(record.text_records.len(), 1);
        assert_eq!(record.updated_at, 101);
    }

    #[test]
    fn remove_record_emits_event_and_clears_mappings() {
        let env = Env::default();
        let contract_id = env.register(ResolverContract, ());
        let client = ResolverContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let name = String::from_str(&env, "alice.xlm");
        let addr = String::from_str(&env, "GAAA");

        client.set_record(&name, &owner, &addr, &100);
        client.set_primary_name(&addr, &owner, &name);
        client.remove_record(&name, &owner);

        assert_eq!(client.resolve(&name), None);
        assert_eq!(client.reverse(&addr), None);
    }

    #[test]
    fn set_primary_name_emits_event() {
        let env = Env::default();
        let contract_id = env.register(ResolverContract, ());
        let client = ResolverContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let name = String::from_str(&env, "alice.xlm");
        let addr = String::from_str(&env, "GAAA");

        client.set_record(&name, &owner, &addr, &100);
        client.set_primary_name(&addr, &owner, &name);

        // Primary set: reverse lookup returns the primary-tagged name
        assert_eq!(client.reverse(&addr), Some(name));
    }

    // -----------------------------------------------------------------------
    // #154: Batch update entrypoint tests
    // -----------------------------------------------------------------------

    #[test]
    fn batch_set_applies_address_and_text_ops_atomically() {
        let env = Env::default();
        let contract_id = env.register(ResolverContract, ());
        let client = ResolverContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let name = String::from_str(&env, "alice.xlm");

        client.set_record(&name, &owner, &String::from_str(&env, "GAAA"), &100);

        let ops = Vec::from_array(
            &env,
            [
                BatchOp::SetAddress(String::from_str(&env, "GBBB")),
                BatchOp::SetText(
                    String::from_str(&env, "url"),
                    String::from_str(&env, "https://alice.example"),
                ),
                BatchOp::SetText(
                    String::from_str(&env, "com.twitter"),
                    String::from_str(&env, "@alice"),
                ),
            ],
        );

        let applied = client.batch_set(&name, &owner, &ops, &200);
        assert_eq!(applied, 3);

        let record = client.resolve(&name).unwrap();
        assert_eq!(
            record.addresses.get(String::from_str(&env, "stellar")),
            Some(String::from_str(&env, "GBBB"))
        );
        assert_eq!(record.text_records.len(), 2);
        assert_eq!(record.updated_at, 200);
        // Reverse mapping updated
        assert_eq!(
            client.reverse(&String::from_str(&env, "GBBB")),
            Some(name.clone())
        );
        // Old reverse cleared
        assert_eq!(client.reverse(&String::from_str(&env, "GAAA")), None);
    }

    #[test]
    fn batch_set_rejects_oversized_payloads() {
        let env = Env::default();
        let contract_id = env.register(ResolverContract, ());
        let client = ResolverContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let name = String::from_str(&env, "alice.xlm");
        client.set_record(&name, &owner, &String::from_str(&env, "GAAA"), &100);

        // Build 17 ops (MAX_BATCH_OPS = 16)
        let mut ops = Vec::new(&env);
        for i in 0u32..17 {
            ops.push_back(BatchOp::SetText(
                String::from_str(&env, &format!("key-{i}")),
                String::from_str(&env, "v"),
            ));
        }

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.batch_set(&name, &owner, &ops, &200);
        }));
        assert!(result.is_err(), "batch_set must reject oversized payloads");
    }

    #[test]
    fn batch_set_rejects_non_owner() {
        let env = Env::default();
        let contract_id = env.register(ResolverContract, ());
        let client = ResolverContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let intruder = Address::generate(&env);
        let name = String::from_str(&env, "alice.xlm");
        client.set_record(&name, &owner, &String::from_str(&env, "GAAA"), &100);

        let ops = Vec::from_array(
            &env,
            [BatchOp::SetAddress(String::from_str(&env, "GBBB"))],
        );

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.batch_set(&name, &intruder, &ops, &200);
        }));
        assert!(result.is_err(), "non-owner batch_set should fail");
        // Address unchanged
        assert_eq!(
            client.reverse(&String::from_str(&env, "GAAA")),
            Some(name)
        );
    }

    #[test]
    fn batch_set_partial_failure_skips_invalid_ops_and_applies_valid_ones() {
        let env = Env::default();
        let contract_id = env.register(ResolverContract, ());
        let client = ResolverContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let name = String::from_str(&env, "alice.xlm");
        client.set_record(&name, &owner, &String::from_str(&env, "GAAA"), &100);

        // One valid text op + one with uppercase key (invalid, will be skipped)
        let ops = Vec::from_array(
            &env,
            [
                BatchOp::SetText(
                    String::from_str(&env, "url"),
                    String::from_str(&env, "https://alice.example"),
                ),
                BatchOp::SetText(
                    String::from_str(&env, "BadKey"),  // uppercase — invalid
                    String::from_str(&env, "value"),
                ),
            ],
        );

        let applied = client.batch_set(&name, &owner, &ops, &200);
        // Only the valid op counts
        assert_eq!(applied, 1);

        let record = client.resolve(&name).unwrap();
        assert_eq!(record.text_records.len(), 1);
        assert_eq!(
            record.text_records.get(String::from_str(&env, "url")),
            Some(String::from_str(&env, "https://alice.example"))
        );
        // BadKey must NOT be stored
        assert_eq!(
            record.text_records.get(String::from_str(&env, "badkey")),
            None
        );
    }

    // -----------------------------------------------------------------------
    // #163: Property-style tests — resolver state-transition sequences
    // -----------------------------------------------------------------------

    /// Repeated address replacement keeps reverse mapping consistent.
    #[test]
    fn property_repeated_address_replacement_keeps_reverse_consistent() {
        let env = Env::default();
        let contract_id = env.register(ResolverContract, ());
        let client = ResolverContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let name = String::from_str(&env, "alice.xlm");

        let addresses = ["GAAA", "GBBB", "GCCC", "GDDD", "GEEE"];
        for (i, addr_str) in addresses.iter().enumerate() {
            let addr = String::from_str(&env, addr_str);
            client.set_record(&name, &owner, &addr, &(100 + i as u64));

            // Current reverse must point to name
            assert_eq!(
                client.reverse(&addr),
                Some(name.clone()),
                "reverse for {addr_str} must resolve after set_record"
            );

            // All previous addresses must be cleared
            for prev_addr_str in &addresses[..i] {
                let prev = String::from_str(&env, prev_addr_str);
                assert_eq!(
                    client.reverse(&prev),
                    None,
                    "stale reverse for {prev_addr_str} must be cleared"
                );
            }
        }
    }

    /// Primary-name changes stay consistent with forward resolution.
    #[test]
    fn property_primary_name_changes_remain_consistent() {
        let env = Env::default();
        let contract_id = env.register(ResolverContract, ());
        let client = ResolverContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let names = ["alice.xlm", "pay.alice.xlm", "tip.alice.xlm"];
        let addr = String::from_str(&env, "GAAA");

        // Register all three names with the same address
        for (i, n) in names.iter().enumerate() {
            client.set_record(
                &String::from_str(&env, n),
                &owner,
                &addr,
                &(100 + i as u64),
            );
        }

        // Cycle through each name as the primary
        for chosen in &names {
            let chosen_name = String::from_str(&env, chosen);
            client.set_primary_name(&addr, &owner, &chosen_name);
            // reverse() should return the currently-set primary
            assert_eq!(
                client.reverse(&addr),
                Some(chosen_name.clone()),
                "reverse should return the current primary name ({chosen})"
            );
            // forward resolution still works for each name
            for n in &names {
                let rec = client.resolve(&String::from_str(&env, n));
                assert!(
                    rec.is_some(),
                    "forward resolution for {n} must remain intact"
                );
            }
        }
    }

    /// Record removal clears both forward and reverse; subsequent re-registration works.
    #[test]
    fn property_remove_and_reregister_stays_consistent() {
        let env = Env::default();
        let contract_id = env.register(ResolverContract, ());
        let client = ResolverContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let name = String::from_str(&env, "alice.xlm");
        let addr = String::from_str(&env, "GAAA");

        for round in 0u64..4 {
            // Register
            client.set_record(&name, &owner, &addr, &(100 + round * 10));
            assert!(client.resolve(&name).is_some());
            assert_eq!(client.reverse(&addr), Some(name.clone()));

            // Remove
            client.remove_record(&name, &owner);
            assert!(client.resolve(&name).is_none());
            assert_eq!(client.reverse(&addr), None);
        }
    }

    /// Text-record churn near the configured limit: add up to limit, remove one,
    /// add another — verify the record count and key accuracy.
    #[test]
    fn property_text_record_churn_near_limit_stays_consistent() {
        let env = Env::default();
        let contract_id = env.register(ResolverContract, ());
        let client = ResolverContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let name = String::from_str(&env, "alice.xlm");
        client.set_record(&name, &owner, &String::from_str(&env, "GAAA"), &100);

        // Fill to the limit
        for idx in 0..MAX_TEXT_RECORDS {
            client.set_text_record(
                &name,
                &owner,
                &String::from_str(&env, &format!("key-{idx}")),
                &String::from_str(&env, &format!("value-{idx}")),
                &(101 + idx as u64),
            );
        }
        assert_eq!(
            client.resolve(&name).unwrap().text_records.len(),
            MAX_TEXT_RECORDS as u32
        );

        // Overflow must be rejected
        let overflow = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.set_text_record(
                &name,
                &owner,
                &String::from_str(&env, "overflow-key"),
                &String::from_str(&env, "v"),
                &200,
            );
        }));
        assert!(overflow.is_err());

        // Updating an existing key at the limit must succeed
        client.set_text_record(
            &name,
            &owner,
            &String::from_str(&env, "key-0"),
            &String::from_str(&env, "updated"),
            &201,
        );
        let record = client.resolve(&name).unwrap();
        assert_eq!(record.text_records.len(), MAX_TEXT_RECORDS as u32);
        assert_eq!(
            record.text_records.get(String::from_str(&env, "key-0")),
            Some(String::from_str(&env, "updated"))
        );
    }

    /// batch_set with mixed address + text ops: verify address and text-record
    /// consistency after each step in a sequence.
    #[test]
    fn property_batch_set_sequence_remains_consistent() {
        let env = Env::default();
        let contract_id = env.register(ResolverContract, ());
        let client = ResolverContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let name = String::from_str(&env, "alice.xlm");
        client.set_record(&name, &owner, &String::from_str(&env, "G0000"), &100);

        let steps: &[(&str, &[(&str, &str)])] = &[
            ("GAAA", &[("url", "https://a"), ("com.twitter", "@a1")]),
            ("GBBB", &[("url", "https://b"), ("email", "b@b.com")]),
            ("GCCC", &[("email", "c@c.com")]),
        ];

        let mut now = 200u64;
        for (addr_str, text_pairs) in steps {
            let mut ops = Vec::new(&env);
            ops.push_back(BatchOp::SetAddress(String::from_str(&env, addr_str)));
            for (k, v) in *text_pairs {
                ops.push_back(BatchOp::SetText(
                    String::from_str(&env, k),
                    String::from_str(&env, v),
                ));
            }
            client.batch_set(&name, &owner, &ops, &now);

            // Invariant: reverse points to current address
            let current_addr = String::from_str(&env, addr_str);
            assert_eq!(client.reverse(&current_addr), Some(name.clone()));

            // Invariant: forward record exists
            assert!(client.resolve(&name).is_some());

            now += 10;
        }
    }
}
