#[cfg(test)]
mod tests {
    use soroban_sdk::{
        symbol_short,
        testutils::{Address as _, Events},
        Address, Env, IntoVal, String,
    };

    use crate::{NftContract, NftContractClient};

    #[test]
    fn stores_metadata_and_query_methods_after_mint() {
        let env = Env::default();
        let contract_id = env.register(NftContract, ());
        let client = NftContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let token_id = String::from_str(&env, "timmy.xlm");
        let metadata_uri = String::from_str(&env, "ipfs://timmy");

        client.mint(&token_id, &owner, &Some(metadata_uri.clone()));

        assert_eq!(client.owner_of(&token_id), Some(owner.clone()));
        assert_eq!(client.total_supply(), 1);
        assert_eq!(client.balance_of(&owner), 1);
        assert_eq!(client.token_by_index(&0), Some(token_id.clone()));
        assert_eq!(
            client.token_of_owner_by_index(&owner, &0),
            Some(token_id.clone())
        );
        assert_eq!(client.token_uri(&token_id), Some(metadata_uri.clone()));

        let token = client.token(&token_id).unwrap();
        assert_eq!(token.owner, owner);
        assert_eq!(token.approved, None);
        assert_eq!(token.metadata_uri, Some(metadata_uri));
    }

    #[test]
    fn rejects_duplicate_mint_for_existing_token_id() {
        let env = Env::default();
        let contract_id = env.register(NftContract, ());
        let client = NftContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let other_owner = Address::generate(&env);
        let token_id = String::from_str(&env, "timmy.xlm");

        client.mint(&token_id, &owner, &None::<String>);

        let duplicate_mint = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.mint(
                &token_id,
                &other_owner,
                &Some(String::from_str(&env, "ipfs://other")),
            );
        }));

        assert!(duplicate_mint.is_err(), "duplicate mint should fail");
        let token = client.token(&token_id).unwrap();
        assert_eq!(token.owner, owner);
        assert_eq!(token.metadata_uri, None);
    }

    #[test]
    fn stores_approval_and_allows_approved_transfer() {
        let env = Env::default();
        let contract_id = env.register(NftContract, ());
        let client = NftContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let approved = Address::generate(&env);
        let new_owner = Address::generate(&env);
        let token_id = String::from_str(&env, "timmy.xlm");

        client.mint(
            &token_id,
            &owner,
            &Some(String::from_str(&env, "ipfs://timmy")),
        );
        client.approve(&token_id, &owner, &approved);

        let approved_token = client.token(&token_id).unwrap();
        assert_eq!(approved_token.owner, owner);
        assert_eq!(approved_token.approved, Some(approved.clone()));

        client.transfer(&token_id, &approved, &new_owner);

        assert_eq!(client.owner_of(&token_id), Some(new_owner.clone()));

        let transferred_token = client.token(&token_id).unwrap();
        assert_eq!(transferred_token.owner, new_owner);
        assert_eq!(transferred_token.approved, None);
        assert_eq!(
            transferred_token.metadata_uri,
            Some(String::from_str(&env, "ipfs://timmy"))
        );
    }

    #[test]
    fn rejects_transfer_from_unauthorized_caller() {
        let env = Env::default();
        let contract_id = env.register(NftContract, ());
        let client = NftContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let intruder = Address::generate(&env);
        let new_owner = Address::generate(&env);
        let token_id = String::from_str(&env, "timmy.xlm");

        client.mint(&token_id, &owner, &None::<String>);

        let unauthorized_transfer = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.transfer(&token_id, &intruder, &new_owner);
        }));

        assert!(
            unauthorized_transfer.is_err(),
            "unauthorized transfer should fail"
        );
        assert_eq!(client.owner_of(&token_id), Some(owner.clone()));

        let token = client.token(&token_id).unwrap();
        assert_eq!(token.owner, owner);
        assert_eq!(token.approved, None);
    }

    #[test]
    fn updates_query_methods_after_owner_transfer() {
        let env = Env::default();
        let contract_id = env.register(NftContract, ());
        let client = NftContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let new_owner = Address::generate(&env);
        let approved = Address::generate(&env);
        let token_id = String::from_str(&env, "timmy.xlm");

        client.mint(
            &token_id,
            &owner,
            &Some(String::from_str(&env, "ipfs://timmy")),
        );
        client.approve(&token_id, &owner, &approved);
        client.transfer(&token_id, &owner, &new_owner);

        assert_eq!(client.owner_of(&token_id), Some(new_owner.clone()));
        assert_eq!(client.total_supply(), 1);
        assert_eq!(client.balance_of(&owner), 0);
        assert_eq!(client.balance_of(&new_owner), 1);
        assert_eq!(client.token_by_index(&0), Some(token_id.clone()));
        assert_eq!(client.token_of_owner_by_index(&owner, &0), None);
        assert_eq!(
            client.token_of_owner_by_index(&new_owner, &0),
            Some(token_id.clone())
        );
        assert_eq!(
            client.token_uri(&token_id),
            Some(String::from_str(&env, "ipfs://timmy"))
        );

        let token = client.token(&token_id).unwrap();
        assert_eq!(token.owner, new_owner);
        assert_eq!(token.approved, None);
        assert_eq!(
            token.metadata_uri,
            Some(String::from_str(&env, "ipfs://timmy"))
        );
    }

    #[test]
    fn enumerates_global_and_owner_tokens_across_multiple_mints() {
        let env = Env::default();
        let contract_id = env.register(NftContract, ());
        let client = NftContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let other_owner = Address::generate(&env);
        let first_token = String::from_str(&env, "alpha.xlm");
        let second_token = String::from_str(&env, "beta.xlm");
        let third_token = String::from_str(&env, "gamma.xlm");

        client.mint(
            &first_token,
            &owner,
            &Some(String::from_str(&env, "ipfs://alpha")),
        );
        client.mint(&second_token, &owner, &None::<String>);
        client.mint(
            &third_token,
            &other_owner,
            &Some(String::from_str(&env, "ipfs://gamma")),
        );

        assert_eq!(client.total_supply(), 3);
        assert_eq!(client.balance_of(&owner), 2);
        assert_eq!(client.balance_of(&other_owner), 1);

        assert_eq!(client.token_by_index(&0), Some(first_token.clone()));
        assert_eq!(client.token_by_index(&1), Some(second_token.clone()));
        assert_eq!(client.token_by_index(&2), Some(third_token.clone()));
        assert_eq!(client.token_by_index(&3), None);

        assert_eq!(
            client.token_of_owner_by_index(&owner, &0),
            Some(first_token)
        );
        assert_eq!(
            client.token_of_owner_by_index(&owner, &1),
            Some(second_token)
        );
        assert_eq!(client.token_of_owner_by_index(&owner, &2), None);
        assert_eq!(
            client.token_of_owner_by_index(&other_owner, &0),
            Some(third_token.clone())
        );
        assert_eq!(
            client.token_uri(&third_token),
            Some(String::from_str(&env, "ipfs://gamma"))
        );
    }

    /// Walk both enumeration surfaces (global `token_by_index` and per-owner
    /// `token_of_owner_by_index`) and assert the four invariants that have to
    /// hold simultaneously for the NFT to be consistent:
    ///
    /// 1. `total_supply` equals the number of entries reachable through
    ///    `token_by_index` (the global list is dense and bounded).
    /// 2. Every globally-enumerated token resolves through `owner_of` to a
    ///    real owner.
    /// 3. For every owner, the tokens reachable through
    ///    `token_of_owner_by_index` are exactly the tokens whose `owner_of`
    ///    points back at that owner (per-owner list is in sync with the
    ///    canonical owner field).
    /// 4. No owner list contains the same token twice.
    fn assert_enumeration_consistent(client: &NftContractClient<'_>, owners: &[Address]) {
        let total = client.total_supply();

        let mut global_tokens: std::vec::Vec<String> = std::vec::Vec::new();
        for i in 0..total {
            let token = client
                .token_by_index(&i)
                .unwrap_or_else(|| panic!("token_by_index({}) missing inside total_supply", i));
            global_tokens.push(token);
        }
        // Global list is dense: nothing beyond total_supply.
        assert!(client.token_by_index(&total).is_none());

        // Every globally-listed token has an owner.
        for token in &global_tokens {
            assert!(
                client.owner_of(token).is_some(),
                "globally-enumerated token has no owner"
            );
        }

        for owner in owners {
            let balance = client.balance_of(owner);

            let mut per_owner: std::vec::Vec<String> = std::vec::Vec::new();
            for i in 0..balance {
                let token = client
                    .token_of_owner_by_index(owner, &i)
                    .unwrap_or_else(|| {
                        panic!("token_of_owner_by_index({}) missing inside balance_of", i)
                    });
                per_owner.push(token);
            }
            assert!(client.token_of_owner_by_index(owner, &balance).is_none());

            // Per-owner list matches owner_of: every entry resolves back, and
            // every token whose owner is this address shows up exactly once.
            for token in &per_owner {
                assert_eq!(
                    client.owner_of(token).as_ref(),
                    Some(owner),
                    "owner list contains a token whose owner_of disagrees"
                );
            }
            let owned_via_global: std::vec::Vec<&String> = global_tokens
                .iter()
                .filter(|t| client.owner_of(t).as_ref() == Some(owner))
                .collect();
            assert_eq!(
                owned_via_global.len() as u32,
                balance,
                "balance_of disagrees with the count of tokens whose owner_of points here"
            );

            // No duplicate owner-token entries.
            let mut seen: std::vec::Vec<&String> = std::vec::Vec::new();
            for token in &per_owner {
                assert!(
                    !seen.contains(&token),
                    "duplicate owner-token entry detected"
                );
                seen.push(token);
            }
        }
    }

    #[test]
    fn invariants_hold_after_mint_approve_transfer_sequence() {
        let env = Env::default();
        let contract_id = env.register(NftContract, ());
        let client = NftContractClient::new(&env, &contract_id);

        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        let carol = Address::generate(&env);
        let owners = [alice.clone(), bob.clone(), carol.clone()];

        let alpha = String::from_str(&env, "alpha.xlm");
        let beta = String::from_str(&env, "beta.xlm");
        let gamma = String::from_str(&env, "gamma.xlm");

        client.mint(&alpha, &alice, &None::<String>);
        client.mint(&beta, &alice, &None::<String>);
        client.mint(&gamma, &bob, &None::<String>);
        assert_enumeration_consistent(&client, &owners);

        // Direct owner transfer.
        client.transfer(&alpha, &alice, &bob);
        assert_enumeration_consistent(&client, &owners);

        // Approval then approved-transfer must not double-list or lose tokens.
        client.approve(&beta, &alice, &carol);
        client.transfer(&beta, &carol, &carol);
        assert_enumeration_consistent(&client, &owners);

        // Re-mint must not allow the second mint of the same id (covered
        // elsewhere) and must leave invariants intact.
        let delta = String::from_str(&env, "delta.xlm");
        client.mint(
            &delta,
            &alice,
            &Some(String::from_str(&env, "ipfs://delta")),
        );
        assert_enumeration_consistent(&client, &owners);

        // Transfer back to the original owner.
        client.transfer(&alpha, &bob, &alice);
        assert_enumeration_consistent(&client, &owners);
    }

    #[test]
    fn no_op_transfer_to_same_owner_is_idempotent_and_keeps_invariants() {
        let env = Env::default();
        let contract_id = env.register(NftContract, ());
        let client = NftContractClient::new(&env, &contract_id);

        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        let owners = [alice.clone(), bob.clone()];

        let alpha = String::from_str(&env, "alpha.xlm");
        let beta = String::from_str(&env, "beta.xlm");

        client.mint(&alpha, &alice, &None::<String>);
        client.mint(&beta, &bob, &None::<String>);

        let alice_balance_before = client.balance_of(&alice);
        let supply_before = client.total_supply();
        let token_before = client.token(&alpha).unwrap();

        // Set then clear an approval and transfer alice -> alice. The
        // approved field must be cleared, balances unchanged, and the
        // per-owner list must contain alpha exactly once.
        let carol = Address::generate(&env);
        client.approve(&alpha, &alice, &carol);
        client.transfer(&alpha, &alice, &alice);

        assert_eq!(client.owner_of(&alpha), Some(alice.clone()));
        assert_eq!(client.balance_of(&alice), alice_balance_before);
        assert_eq!(client.total_supply(), supply_before);

        let token_after = client.token(&alpha).unwrap();
        assert_eq!(token_after.owner, token_before.owner);
        assert_eq!(token_after.approved, None);

        // Run the full consistency walk — duplicate detection in particular
        // would catch a self-transfer that pushed alpha onto alice's list a
        // second time.
        assert_enumeration_consistent(&client, &owners);
    }

    #[test]
    fn approval_changes_do_not_change_enumeration_queries() {
        let env = Env::default();
        let contract_id = env.register(NftContract, ());
        let client = NftContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let approved = Address::generate(&env);
        let token_id = String::from_str(&env, "timmy.xlm");

        client.mint(
            &token_id,
            &owner,
            &Some(String::from_str(&env, "ipfs://timmy")),
        );
        client.approve(&token_id, &owner, &approved);

        assert_eq!(client.total_supply(), 1);
        assert_eq!(client.balance_of(&owner), 1);
        assert_eq!(client.token_by_index(&0), Some(token_id.clone()));
        assert_eq!(
            client.token_of_owner_by_index(&owner, &0),
            Some(token_id.clone())
        );
        assert_eq!(
            client.token_uri(&token_id),
            Some(String::from_str(&env, "ipfs://timmy"))
        );

        let token = client.token(&token_id).unwrap();
        assert_eq!(token.approved, Some(approved));
    }

    #[test]
    fn test_mint_emits_event() {
        let env = Env::default();
        let contract_id = env.register(NftContract, ());
        let client = NftContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let token_id = String::from_str(&env, "timmy.xlm");

        client.mint(&token_id, &owner, &None::<String>);

        let events = env.events().all();
        assert_eq!(events.len(), 1);
        let (_contract_id, topics, data) = events.get(0).unwrap();
        assert_eq!(topics.get(0).unwrap(), symbol_short!("mint").into_val(&env));
        assert_eq!(data, token_id.into_val(&env));
    }

    #[test]
    fn test_approve_emits_event() {
        let env = Env::default();
        let contract_id = env.register(NftContract, ());
        let client = NftContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let approved = Address::generate(&env);
        let token_id = String::from_str(&env, "timmy.xlm");

        client.mint(&token_id, &owner, &None::<String>);
        client.approve(&token_id, &owner, &approved);

        let events = env.events().all();
        assert_eq!(events.len(), 2);
        let (_contract_id, topics, data) = events.get(1).unwrap();
        assert_eq!(
            topics.get(0).unwrap(),
            symbol_short!("approve").into_val(&env)
        );
        assert_eq!(data, token_id.into_val(&env));
    }

    #[test]
    fn test_approve_clear_emits_event() {
        let env = Env::default();
        let contract_id = env.register(NftContract, ());
        let client = NftContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let approved = Address::generate(&env);
        let token_id = String::from_str(&env, "timmy.xlm");

        client.mint(&token_id, &owner, &None::<String>);
        client.approve(&token_id, &owner, &approved);
        client.approve_clear(&token_id, &owner);

        let events = env.events().all();
        let (_contract_id, topics, data) = events.get(events.len() - 1).unwrap();
        assert_eq!(
            topics.get(0).unwrap(),
            symbol_short!("appr_clr").into_val(&env)
        );
        assert_eq!(data, token_id.into_val(&env));
    }

    #[test]
    fn test_transfer_owner_emits_event() {
        let env = Env::default();
        let contract_id = env.register(NftContract, ());
        let client = NftContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let new_owner = Address::generate(&env);
        let token_id = String::from_str(&env, "timmy.xlm");

        client.mint(&token_id, &owner, &None::<String>);
        client.transfer(&token_id, &owner, &new_owner);

        let events = env.events().all();
        let (_contract_id, topics, data) = events.get(events.len() - 1).unwrap();
        assert_eq!(
            topics.get(0).unwrap(),
            symbol_short!("transfer").into_val(&env)
        );
        assert_eq!(topics.get(1).unwrap(), owner.into_val(&env));
        assert_eq!(topics.get(2).unwrap(), new_owner.into_val(&env));
        assert_eq!(data, token_id.into_val(&env));
    }

    #[test]
    fn test_transfer_from_emits_event() {
        let env = Env::default();
        let contract_id = env.register(NftContract, ());
        let client = NftContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let approved = Address::generate(&env);
        let recipient = Address::generate(&env);
        let token_id = String::from_str(&env, "timmy.xlm");

        client.mint(&token_id, &owner, &None::<String>);
        client.approve(&token_id, &owner, &approved);
        client.transfer_from(&approved, &owner, &recipient, &token_id);

        let events = env.events().all();
        let (_contract_id, topics, data) = events.get(events.len() - 1).unwrap();
        assert_eq!(
            topics.get(0).unwrap(),
            symbol_short!("transfer").into_val(&env)
        );
        assert_eq!(topics.get(1).unwrap(), owner.into_val(&env));
        assert_eq!(topics.get(2).unwrap(), recipient.into_val(&env));
        assert_eq!(data, token_id.into_val(&env));
    // ── #151: enumeration-consistency invariants ───────────────────────────
    // The owner token list, owner_of, and the per-owner index must stay aligned
    // after transfers/approvals, with no duplicate owner-token entries.

    use soroban_sdk::Vec as SdkVec;

    fn assert_owner_enumeration_consistent(
        env: &Env,
        client: &NftContractClient,
        owner: &Address,
    ) {
        let balance = client.balance_of(owner);
        let mut seen: SdkVec<String> = SdkVec::new(env);
        for i in 0..balance {
            let token_id = client
                .token_of_owner_by_index(owner, &i)
                .expect("every index below balance_of must resolve to a token");
            assert!(
                !seen.contains(&token_id),
                "duplicate owner-token entry detected in enumeration"
            );
            assert_eq!(
                client.owner_of(&token_id),
                Some(owner.clone()),
                "owner_of must agree with the owner token list"
            );
            seen.push_back(token_id);
        }
        // Nothing past the reported balance.
        assert_eq!(client.token_of_owner_by_index(owner, &balance), None);
    }

    #[test]
    fn enumeration_consistent_after_owner_transfer() {
        let env = Env::default();
        env.mock_all_auths();
        let client = NftContractClient::new(&env, &env.register(NftContract, ()));
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        let t1 = String::from_str(&env, "a.xlm");
        let t2 = String::from_str(&env, "b.xlm");
        client.mint(&t1, &alice, &None::<String>);
        client.mint(&t2, &alice, &None::<String>);

        client.transfer(&t1, &alice, &bob);

        assert_eq!(client.owner_of(&t1), Some(bob.clone()));
        assert_eq!(client.balance_of(&alice), 1);
        assert_eq!(client.balance_of(&bob), 1);
        assert_eq!(client.total_supply(), 2);
        assert_owner_enumeration_consistent(&env, &client, &alice);
        assert_owner_enumeration_consistent(&env, &client, &bob);
    }

    #[test]
    fn enumeration_consistent_after_approved_transfer() {
        let env = Env::default();
        env.mock_all_auths();
        let client = NftContractClient::new(&env, &env.register(NftContract, ()));
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        let operator = Address::generate(&env);
        let t1 = String::from_str(&env, "a.xlm");
        client.mint(&t1, &alice, &None::<String>);

        client.approve(&t1, &alice, &operator);
        client.transfer(&t1, &operator, &bob); // performed by the approved operator

        assert_eq!(client.owner_of(&t1), Some(bob.clone()));
        assert_eq!(client.balance_of(&alice), 0);
        assert_eq!(client.balance_of(&bob), 1);
        // Approval is cleared on transfer.
        assert_eq!(client.token(&t1).unwrap().approved, None);
        assert_owner_enumeration_consistent(&env, &client, &alice);
        assert_owner_enumeration_consistent(&env, &client, &bob);
    }

    #[test]
    fn no_op_self_transfer_does_not_duplicate_owner_token() {
        let env = Env::default();
        env.mock_all_auths();
        let client = NftContractClient::new(&env, &env.register(NftContract, ()));
        let alice = Address::generate(&env);
        let t1 = String::from_str(&env, "a.xlm");
        client.mint(&t1, &alice, &None::<String>);

        client.transfer(&t1, &alice, &alice); // owner unchanged

        assert_eq!(client.owner_of(&t1), Some(alice.clone()));
        assert_eq!(client.balance_of(&alice), 1);
        // Key invariant: a no-op owner change must not create a duplicate entry.
        assert_owner_enumeration_consistent(&env, &client, &alice);
    }
}
