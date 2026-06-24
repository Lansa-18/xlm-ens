#[cfg(test)] ///////
mod tests {
    use soroban_sdk::token;
    use soroban_sdk::{testutils::Address as _, Address, Env, String};

    use crate::{AuctionContract, AuctionContractClient};

    fn setup_token(
        env: &Env,
    ) -> (
        Address,
        token::StellarAssetClient<'static>,
        token::Client<'static>,
    ) {
        let admin = Address::generate(env);
        let contract = env.register_stellar_asset_contract(admin.clone());
        (
            contract.clone(),
            token::StellarAssetClient::new(env, &contract),
            token::Client::new(env, &contract),
        )
    }

    #[test]
    fn stores_auctions_in_contract_storage() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(AuctionContract, ());
        let client = AuctionContractClient::new(&env, &contract_id);

        let (asset, token_admin, token) = setup_token(&env);
        let treasury = Address::generate(&env);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);

        token_admin.mint(&alice, &1000);
        token_admin.mint(&bob, &1000);

        let name = String::from_str(&env, "vip.xlm");

        client.create_auction(&name, &asset, &treasury, &200, &10, &20);
        client.place_bid(&name, &alice, &500, &12);
        client.place_bid(&name, &bob, &300, &13);

        let settlement = client.settle(&name, &21).unwrap();
        assert_eq!(settlement.winner, Some(alice.clone()));
        assert_eq!(settlement.clearing_price, 300);
        assert!(settlement.sold);

        assert_eq!(token.balance(&alice), 1000 - 300);
        assert_eq!(token.balance(&bob), 1000);
        assert_eq!(token.balance(&treasury), 300);
    } //

    #[test]
    fn test_auction_no_bids() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(AuctionContract, ());
        let client = AuctionContractClient::new(&env, &contract_id);

        let (asset, _, _) = setup_token(&env);
        let treasury = Address::generate(&env);
        let name = String::from_str(&env, "ghost.xlm");
        client.create_auction(&name, &asset, &treasury, &100, &10, &20);

        let settlement = client.settle(&name, &21);
        assert!(settlement.is_none());
    }

    #[test]
    fn test_auction_reserve_not_met() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(AuctionContract, ());
        let client = AuctionContractClient::new(&env, &contract_id);

        let (asset, token_admin, token) = setup_token(&env);
        let treasury = Address::generate(&env);
        let alice = Address::generate(&env);

        token_admin.mint(&alice, &1000);
        let name = String::from_str(&env, "cheap.xlm");
        client.create_auction(&name, &asset, &treasury, &1000, &10, &20);
        client.place_bid(&name, &alice, &500, &15);

        let settlement = client.settle(&name, &21).unwrap();
        assert_eq!(settlement.winner, None);
        assert_eq!(settlement.clearing_price, 0);
        assert!(!settlement.sold);

        assert_eq!(token.balance(&alice), 1000);
        assert_eq!(token.balance(&treasury), 0);
    }

    #[test]
    fn test_auction_tie_behavior() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(AuctionContract, ());
        let client = AuctionContractClient::new(&env, &contract_id);

        let (asset, token_admin, token) = setup_token(&env);
        let treasury = Address::generate(&env);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);

        token_admin.mint(&alice, &1000);
        token_admin.mint(&bob, &1000);
        let name = String::from_str(&env, "tie.xlm");
        client.create_auction(&name, &asset, &treasury, &100, &10, &20);

        client.place_bid(&name, &alice, &500, &12);
        client.place_bid(&name, &bob, &500, &13);

        let settlement = client.settle(&name, &21).unwrap();
        // First bidder wins in case of tie in current implementation
        assert_eq!(settlement.winner, Some(alice.clone()));
        assert_eq!(settlement.clearing_price, 500);
        assert!(settlement.sold);

        assert_eq!(token.balance(&alice), 1000 - 500);
        assert_eq!(token.balance(&bob), 1000);
        assert_eq!(token.balance(&treasury), 500);
    }

    #[test]
    fn list_auctions_paginates_in_creation_order() {
        let env = Env::default();
        let contract_id = env.register(AuctionContract, ());
        let client = AuctionContractClient::new(&env, &contract_id);

        // Empty state: every helper returns an empty Vec, count is zero.
        assert_eq!(client.auction_count(), 0);
        assert_eq!(client.list_auctions(&0, &10).len(), 0);
        assert_eq!(client.list_active_auctions(&0, &0, &10).len(), 0);
        assert_eq!(client.list_settled_auctions(&0, &10).len(), 0);

        // Create three auctions with distinct windows so we can drive the
        // active/settled filters independently of one another.
        let alpha = String::from_str(&env, "alpha.xlm");
        let beta = String::from_str(&env, "beta.xlm");
        let gamma = String::from_str(&env, "gamma.xlm");
        let asset = Address::generate(&env);
        let treasury = Address::generate(&env);
        client.create_auction(&alpha, &asset, &treasury, &100, &10, &20);
        client.create_auction(&beta, &asset, &treasury, &100, &30, &40);
        client.create_auction(&gamma, &asset, &treasury, &100, &50, &60);

        assert_eq!(client.auction_count(), 3);

        let page1 = client.list_auctions(&0, &2);
        assert_eq!(page1.len(), 2);
        assert_eq!(page1.get_unchecked(0), alpha);
        assert_eq!(page1.get_unchecked(1), beta);

        let page2 = client.list_auctions(&2, &2);
        assert_eq!(page2.len(), 1);
        assert_eq!(page2.get_unchecked(0), gamma);

        // Offset past the end is empty, not an error.
        assert_eq!(client.list_auctions(&99, &10).len(), 0);
    }

    #[test]
    fn list_active_and_settled_filters_partition_by_state() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(AuctionContract, ());
        let client = AuctionContractClient::new(&env, &contract_id);

        let (asset, token_admin, _) = setup_token(&env);
        let treasury = Address::generate(&env);
        let bidder = Address::generate(&env);
        token_admin.mint(&bidder, &1000);

        let alpha = String::from_str(&env, "alpha.xlm");
        let beta = String::from_str(&env, "beta.xlm");
        let gamma = String::from_str(&env, "gamma.xlm");

        client.create_auction(&alpha, &asset, &treasury, &100, &10, &20);
        client.create_auction(&beta, &asset, &treasury, &100, &30, &40);
        client.create_auction(&gamma, &asset, &treasury, &100, &50, &60);

        // At t=15: only alpha is currently accepting bids. None settled.
        let active = client.list_active_auctions(&15, &0, &10);
        assert_eq!(active.len(), 1);
        assert_eq!(active.get_unchecked(0), alpha);
        assert_eq!(client.list_settled_auctions(&0, &10).len(), 0);

        // Settle alpha at t=21. After settlement it must drop out of the
        // active set even at a time inside its original bidding window.
        client.place_bid(&alpha, &bidder, &200, &12);
        client.settle(&alpha, &21);

        let still_active = client.list_active_auctions(&15, &0, &10);
        assert_eq!(still_active.len(), 0);

        let settled = client.list_settled_auctions(&0, &10);
        assert_eq!(settled.len(), 1);
        assert_eq!(settled.get_unchecked(0), alpha);

        // At t=35: beta is active, alpha is settled, gamma hasn't started.
        let active_mid = client.list_active_auctions(&35, &0, &10);
        assert_eq!(active_mid.len(), 1);
        assert_eq!(active_mid.get_unchecked(0), beta);

        // Pagination on filtered list: offset within matches works.
        let page = client.list_active_auctions(&35, &1, &10);
        assert_eq!(page.len(), 0);
    }

    #[test]
    fn list_helpers_cap_limit_at_max_page_size() {
        let env = Env::default();
        let contract_id = env.register(AuctionContract, ());
        let client = AuctionContractClient::new(&env, &contract_id);
        let asset = Address::generate(&env);
        let treasury = Address::generate(&env);

        // Create a handful of auctions; ask for a huge limit and verify the
        // contract caps it at MAX_PAGE_SIZE instead of returning the full
        // index (which would be unbounded). Label minimum length is 3, so use
        // "namXY.xlm" rather than "nX.xlm".
        for i in 0..5u32 {
            let s = format!("nam{i:02}.xlm");
            let name = String::from_str(&env, &s);
            client.create_auction(&name, &asset, &treasury, &100, &10, &20);
        }
        let huge = client.list_auctions(&0, &u32::MAX);
        assert!(huge.len() <= crate::MAX_PAGE_SIZE);
        assert_eq!(huge.len(), 5);
    }

    #[test]
    fn test_auction_clearing_price_logic() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(AuctionContract, ());
        let client = AuctionContractClient::new(&env, &contract_id);

        let (asset, token_admin, token) = setup_token(&env);
        let treasury = Address::generate(&env);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        let charlie = Address::generate(&env);

        token_admin.mint(&alice, &1000);
        token_admin.mint(&bob, &1000);
        token_admin.mint(&charlie, &1000);
        let name = String::from_str(&env, "multi.xlm");
        client.create_auction(&name, &asset, &treasury, &100, &10, &20);

        client.place_bid(&name, &alice, &1000, &12);
        client.place_bid(&name, &bob, &500, &13);
        client.place_bid(&name, &charlie, &750, &14);

        let settlement = client.settle(&name, &21).unwrap();
        assert_eq!(settlement.winner, Some(alice.clone()));
        assert_eq!(settlement.clearing_price, 750); // Second highest bid
        assert!(settlement.sold);

        assert_eq!(token.balance(&alice), 1000 - 750);
        assert_eq!(token.balance(&bob), 1000);
        assert_eq!(token.balance(&charlie), 1000);
        assert_eq!(token.balance(&treasury), 750);
    }

    // ── #157: auction discovery query helpers ──────────────────────────────

    #[test]
    fn discovery_queries_handle_empty_state() {
        let env = Env::default();
        let client = AuctionContractClient::new(&env, &env.register(AuctionContract, ()));
        assert_eq!(client.auction_names().len(), 0);
        assert_eq!(client.active_auctions(&100).len(), 0);
        assert_eq!(client.settled_auctions().len(), 0);
    }

    #[test]
    fn discovery_queries_filter_active_and_settled() {
        let env = Env::default();
        env.mock_all_auths();
        let (asset, token_admin, _) = setup_token(&env);
        let treasury = Address::generate(&env);
        let alice = Address::generate(&env);
        token_admin.mint(&alice, &1000);
        let client = AuctionContractClient::new(&env, &env.register(AuctionContract, ()));

        let a = String::from_str(&env, "alpha.xlm");
        let b = String::from_str(&env, "bravo.xlm");
        let c = String::from_str(&env, "charlie.xlm");
        client.create_auction(&a, &asset, &treasury, &100, &10, &20);
        client.create_auction(&b, &asset, &treasury, &100, &10, &20);
        client.create_auction(&c, &asset, &treasury, &100, &100, &200);

        // Index records every created auction, in creation order.
        let names = client.auction_names();
        assert_eq!(names.len(), 3);
        assert_eq!(names.get(0), Some(a.clone()));

        // At t=15: a and b are open; c hasn't started.
        let active = client.active_auctions(&15);
        assert_eq!(active.len(), 2);

        // Settle `a`, then it must move out of active and into settled.
        client.place_bid(&a, &alice, &500, &12);
        client.settle(&a, &21).unwrap();

        let active_after = client.active_auctions(&15);
        assert_eq!(active_after.len(), 1); // only b remains active
        assert_eq!(active_after.get(0).unwrap().name, b);

        let settled = client.settled_auctions();
        assert_eq!(settled.len(), 1);
        assert_eq!(settled.get(0).unwrap().name, a);
    }

    #[test]
    fn version_is_exposed() {
        let env = Env::default();
        let contract_id = env.register(AuctionContract, ());
        let client = AuctionContractClient::new(&env, &contract_id);
        assert_eq!(client.version(), 1);
    }

    // ── #433: auction admin cancellation ──────────────────────────────────

    fn setup_with_admin(
        env: &Env,
    ) -> (
        AuctionContractClient<'_>,
        Address,
        Address,
        Address,
        token::StellarAssetClient<'_>,
        Address,
    ) {
        env.mock_all_auths();
        let contract_id = env.register(AuctionContract, ());
        let client = AuctionContractClient::new(env, &contract_id);
        let admin = Address::generate(env);
        client.initialize(&admin);
        let (asset, token_admin, _) = setup_token(env);
        let treasury = Address::generate(env);
        (client, contract_id, admin, asset, token_admin, treasury)
    }

    #[test]
    fn cancel_auction_cancels_zero_bid_auction() {
        let env = Env::default();
        let (client, _, _, asset, _, treasury) = setup_with_admin(&env);

        let name = String::from_str(&env, "rare.xlm");
        let reason = String::from_str(&env, "reserved for partner");
        client.create_auction(&name, &asset, &treasury, &500, &100, &200);

        client.cancel_auction(&name, &reason, &50);

        let record = client.cancellation(&name).expect("cancellation record");
        assert_eq!(record.cancelled_at, 50);
        assert_eq!(record.reason, reason);

        let cancelled = client.list_cancelled_auctions(&0, &10);
        assert_eq!(cancelled.len(), 1);
        assert_eq!(cancelled.get_unchecked(0), name);

        // Cancelled auction must not appear as active.
        let active = client.active_auctions(&150);
        assert_eq!(active.len(), 0);

        let active_list = client.list_active_auctions(&150, &0, &10);
        assert_eq!(active_list.len(), 0);
    }

    #[test]
    fn cancel_auction_rejected_when_bids_exist() {
        let env = Env::default();
        let (client, _, _, asset, token_admin, treasury) = setup_with_admin(&env);

        let bidder = Address::generate(&env);
        token_admin.mint(&bidder, &1000);

        let name = String::from_str(&env, "hotname.xlm");
        let reason = String::from_str(&env, "error");
        client.create_auction(&name, &asset, &treasury, &100, &10, &200);
        client.place_bid(&name, &bidder, &200, &15);

        let result = client.try_cancel_auction(&name, &reason, &50);
        assert!(matches!(
            result,
            Err(Ok(crate::AuctionError::AuctionHasBids))
        ));
    }

    #[test]
    fn cancel_auction_rejected_when_already_cancelled() {
        let env = Env::default();
        let (client, _, _, asset, _, treasury) = setup_with_admin(&env);

        let name = String::from_str(&env, "dupe.xlm");
        let reason = String::from_str(&env, "mistake");
        client.create_auction(&name, &asset, &treasury, &100, &10, &200);
        client.cancel_auction(&name, &reason, &50);

        let result = client.try_cancel_auction(&name, &reason, &51);
        assert!(matches!(
            result,
            Err(Ok(crate::AuctionError::AlreadyCancelled))
        ));
    }

    #[test]
    fn cancel_auction_rejected_when_already_settled() {
        let env = Env::default();
        let (client, _, _, asset, _, treasury) = setup_with_admin(&env);

        let name = String::from_str(&env, "settled.xlm");
        let reason = String::from_str(&env, "too late");
        client.create_auction(&name, &asset, &treasury, &100, &10, &20);
        // settle with no bids (returns None, no Settlement record stored)
        // need a bid to get a settlement record written
        // use reserve_price=0 so any bid wins and settlement is written
        let name2 = String::from_str(&env, "sold.xlm");
        let (asset2, token_admin2, _) = setup_token(&env);
        let bidder = Address::generate(&env);
        token_admin2.mint(&bidder, &1000);
        client.create_auction(&name2, &asset2, &treasury, &0, &10, &20);
        client.place_bid(&name2, &bidder, &100, &15);
        client.settle(&name2, &21);

        let result = client.try_cancel_auction(&name2, &reason, &50);
        assert!(matches!(
            result,
            Err(Ok(crate::AuctionError::AlreadySettled))
        ));
    }

    #[test]
    fn threat_cancel_auction_requires_admin() {
        let env = Env::default();
        // Do NOT call env.mock_all_auths() — auth must be enforced for cancel.
        let contract_id = env.register(AuctionContract, ());
        let client = AuctionContractClient::new(&env, &contract_id);

        // initialize() doesn't require auth — call it without mocking.
        let admin = Address::generate(&env);
        client.initialize(&admin);

        let asset = Address::generate(&env);
        let treasury = Address::generate(&env);
        let name = String::from_str(&env, "secret.xlm");
        client.create_auction(&name, &asset, &treasury, &100, &10, &200);

        // cancel_auction requires admin auth — must panic without it.
        let name2 = name.clone();
        let reason = String::from_str(&env, "unauthorized attempt");
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.cancel_auction(&name2, &reason, &50);
        }));
        assert!(result.is_err(), "cancel_auction must require admin auth");
    }
}
