#[cfg(test)]
mod auction_integration {
    use soroban_sdk::{testutils::Address as _, Address, Env, String};
    use soroban_sdk::token;
    use xlm_ns_auction::{AuctionContract, AuctionContractClient};

    fn setup_env() -> (Env, AuctionContractClient<'static>) {
        let env = Env::default();
        let contract_id = env.register(AuctionContract, ());
        let client = AuctionContractClient::new(&env, &contract_id);
        (env, client)
    }

    fn setup_token(env: &Env) -> (Address, token::StellarAssetClient<'static>, token::Client<'static>) {
        let admin = Address::generate(env);
        let contract = env.register_stellar_asset_contract(admin.clone());
        (contract.clone(), token::StellarAssetClient::new(env, &contract), token::Client::new(env, &contract))
    }

    struct TimeHelper {
        pub now: u64,
    }

    impl TimeHelper {
        pub fn new(start: u64) -> Self {
            Self { now: start }
        }
        pub fn advance(&mut self, seconds: u64) {
            self.now += seconds;
        }
        pub fn future(&self, seconds: u64) -> u64 {
            self.now + seconds
        }
    }

    /// Test covers create, bid, settle, and winner inspection matching Vickrey policy.
    #[test]
    fn test_auction_vickrey_settlement() {
        let (env, client) = setup_env();
        env.mock_all_auths();

        let (asset, token_admin, token) = setup_token(&env);
        let treasury = Address::generate(&env);

        let name = String::from_str(&env, "premium.xlm");
        let reserve_price = 100;
        let mut time = TimeHelper::new(1000);
        let starts_at = time.now;
        let ends_at = time.future(1000);

        // Create auction
        client.create_auction(&name, &asset, &treasury, &reserve_price, &starts_at, &ends_at);

        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        let charlie = Address::generate(&env);

        token_admin.mint(&alice, &1000);
        token_admin.mint(&bob, &1000);
        token_admin.mint(&charlie, &1000);

        // Place bids
        client.place_bid(&name, &alice, &500, &time.future(100));
        client.place_bid(&name, &bob, &800, &time.future(200)); // Highest bid
        client.place_bid(&name, &charlie, &600, &time.future(300)); // Second highest bid

        // Settle auction after ends_at
        time.advance(1001);
        let settlement = client
            .settle(&name, &time.now)
            .expect("settlement expected");

        // Bob should win and pay Charlie's bid amount (Vickrey second-price)
        assert_eq!(settlement.winner, Some(bob));
        assert_eq!(settlement.winning_bid, 800);
        assert_eq!(settlement.clearing_price, 600);
        assert!(settlement.sold);

        assert_eq!(token.balance(&alice), 1000);
        assert_eq!(token.balance(&bob), 1000 - 600);
        assert_eq!(token.balance(&charlie), 1000);
        assert_eq!(token.balance(&treasury), 600);
    }

    /// Unsold behavior is covered when bids do not meet the reserve price.
    #[test]
    fn test_auction_unsold_reserve_not_met() {
        let (env, client) = setup_env();
        env.mock_all_auths();

        let (asset, token_admin, token) = setup_token(&env);
        let treasury = Address::generate(&env);

        let name = String::from_str(&env, "unsold.xlm");
        let reserve_price = 1000;
        let mut time = TimeHelper::new(1000);
        let starts_at = time.now;
        let ends_at = time.future(1000);

        client.create_auction(&name, &asset, &treasury, &reserve_price, &starts_at, &ends_at);

        let alice = Address::generate(&env);
        let bob = Address::generate(&env);

        token_admin.mint(&alice, &1000);
        token_admin.mint(&bob, &1000);

        // Place bids below reserve
        client.place_bid(&name, &alice, &500, &time.future(100));
        client.place_bid(&name, &bob, &900, &time.future(200));

        time.advance(1001);
        let settlement = client
            .settle(&name, &time.now)
            .expect("settlement expected");

        // Auction should not be sold
        assert_eq!(settlement.winner, None);
        assert_eq!(settlement.clearing_price, 0);
        assert_eq!(settlement.winning_bid, 900);
        assert!(!settlement.sold);

        assert_eq!(token.balance(&alice), 1000);
        assert_eq!(token.balance(&bob), 1000);
        assert_eq!(token.balance(&treasury), 0);
    }

    /// A single bid above reserve should clear exactly at the reserve price.
    #[test]
    fn test_auction_single_bid_clears_at_reserve() {
        let (env, client) = setup_env();
        env.mock_all_auths();

        let (asset, token_admin, token) = setup_token(&env);
        let treasury = Address::generate(&env);

        let name = String::from_str(&env, "single.xlm");
        let reserve_price = 500;
        let mut time = TimeHelper::new(1000);
        let starts_at = time.now;
        let ends_at = time.future(1000);
        client.create_auction(&name, &asset, &treasury, &reserve_price, &starts_at, &ends_at);

        let alice = Address::generate(&env);
        token_admin.mint(&alice, &1000);
        client.place_bid(&name, &alice, &1000, &time.future(500));

        time.advance(1001);
        let settlement = client
            .settle(&name, &time.now)
            .expect("settlement expected");
        assert_eq!(settlement.winner, Some(alice));
        assert_eq!(settlement.clearing_price, 500); // Clears at reserve
        assert!(settlement.sold);

        assert_eq!(token.balance(&alice), 1000 - 500);
        assert_eq!(token.balance(&treasury), 500);
    }
}
