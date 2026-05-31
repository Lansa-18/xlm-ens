mod test;

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, token, Address, Env, String, Vec,
};
use xlm_ns_common::soroban::validate_fqdn_soroban;
use xlm_ns_common::time::is_time_window_open;

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct Bid {
    pub bidder: Address,
    pub amount: u64,
    pub placed_at: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct Settlement {
    pub winner: Option<Address>,
    pub clearing_price: u64,
    pub winning_bid: u64,
    pub settled_at: u64,
    pub sold: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct Auction {
    pub name: String,
    pub reserve_price: u64,
    pub starts_at: u64,
    pub ends_at: u64,
    pub bids: Vec<Bid>,
    pub asset: Address,
    pub treasury: Address,
}

#[derive(Clone)]
#[contracttype]
enum DataKey {
    Auction(String),
    Settlement(String),
    /// Append-only index of created auction names, enabling discovery queries
    /// (#157) without callers needing to know storage keys.
    AuctionNames,
}

/// Bounded result window for auction discovery queries (#157).
const MAX_AUCTION_RESULTS: u32 = 100;
const MAX_PAGE_SIZE: u32 = 100;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum AuctionError {
    Validation = 1,
    AlreadyExists = 2,
    NotFound = 3,
    AuctionClosed = 4,
    AuctionNotStarted = 5,
    AuctionNotEnded = 6,
    AlreadySettled = 7,
    InvalidBid = 8,
}

#[contract]
pub struct AuctionContract;

#[contractimpl]
impl AuctionContract {
    pub fn create_auction(
        env: Env,
        name: String,
        asset: Address,
        treasury: Address,
        reserve_price: u64,
        starts_at: u64,
        ends_at: u64,
    ) -> Result<(), AuctionError> {
        validate_fqdn_soroban(&name).map_err(|_| AuctionError::Validation)?;
        let key = DataKey::Auction(name.clone());
        if env.storage().persistent().has(&key) {
            return Err(AuctionError::AlreadyExists);
        }

        let auction = Auction {
            name: name.clone(),
            reserve_price,
            starts_at,
            ends_at,
            bids: Vec::new(&env),
            asset,
            treasury,
        };
        env.storage().persistent().set(&key, &auction);

        // Record the name in the discovery index (#157).
        let mut names: Vec<String> = env
            .storage()
            .persistent()
            .get(&DataKey::AuctionNames)
            .unwrap_or_else(|| Vec::new(&env));
        names.push_back(name.clone());
        env.storage()
            .persistent()
            .set(&DataKey::AuctionNames, &names);
        Ok(())
    }

    /// Names of all auctions ever created, in creation order. Bounded to at most
    /// [`MAX_AUCTION_RESULTS`] entries (oldest first) so the call can't return an
    /// unbounded result set (#157).
    pub fn auction_names(env: Env) -> Vec<String> {
        let names: Vec<String> = env
            .storage()
            .persistent()
            .get(&DataKey::AuctionNames)
            .unwrap_or_else(|| Vec::new(&env));
        let mut out = Vec::new(&env);
        for name in names.iter().take(MAX_AUCTION_RESULTS as usize) {
            out.push_back(name);
        }
        out
    }

    /// Auctions that are currently open at `now_unix` — started, not yet ended,
    /// and not settled — in creation order, bounded to [`MAX_AUCTION_RESULTS`]
    /// (#157).
    pub fn active_auctions(env: Env, now_unix: u64) -> Vec<Auction> {
        let mut out = Vec::new(&env);
        for name in Self::auction_names(env.clone()).iter() {
            if env
                .storage()
                .persistent()
                .has(&DataKey::Settlement(name.clone()))
            {
                continue;
            }
            if let Some(auction) = Self::auction(env.clone(), name.clone()) {
                if is_time_window_open(now_unix, auction.starts_at, auction.ends_at) {
                    out.push_back(auction);
                }
            }
        }
        out
    }

    /// Auctions that have been settled, in creation order, bounded to
    /// [`MAX_AUCTION_RESULTS`] (#157).
    pub fn settled_auctions(env: Env) -> Vec<Auction> {
        let mut out = Vec::new(&env);
        for name in Self::auction_names(env.clone()).iter() {
            if env
                .storage()
                .persistent()
                .has(&DataKey::Settlement(name.clone()))
            {
                if let Some(auction) = Self::auction(env.clone(), name.clone()) {
                    out.push_back(auction);
                }
            }
        }
        out
    }

    pub fn place_bid(
        env: Env,
        name: String,
        bidder: Address,
        amount: u64,
        now_unix: u64,
    ) -> Result<(), AuctionError> {
        bidder.require_auth();
        if amount == 0 {
            return Err(AuctionError::InvalidBid);
        }
        let mut auction = get_auction(&env, &name)?;
        if env
            .storage()
            .persistent()
            .has(&DataKey::Settlement(name.clone()))
        {
            return Err(AuctionError::AlreadySettled);
        }
        if !is_time_window_open(now_unix, auction.starts_at, auction.ends_at) {
            if now_unix < auction.starts_at {
                return Err(AuctionError::AuctionNotStarted);
            }
            return Err(AuctionError::AuctionClosed);
        }

        let token = token::Client::new(&env, &auction.asset);
        token.transfer(&bidder, &env.current_contract_address(), &(amount as i128));

        auction.bids.push_back(Bid {
            bidder,
            amount,
            placed_at: now_unix,
        });
        put_auction(&env, &name, &auction);
        Ok(())
    }

    pub fn settle(
        env: Env,
        name: String,
        now_unix: u64,
    ) -> Result<Option<Settlement>, AuctionError> {
        let auction = get_auction(&env, &name)?;
        if env
            .storage()
            .persistent()
            .has(&DataKey::Settlement(name.clone()))
        {
            return Err(AuctionError::AlreadySettled);
        }
        if now_unix < auction.ends_at {
            return Err(AuctionError::AuctionNotEnded);
        }

        let settlement = settle_vickrey(&auction, now_unix);
        if let Some(ref finalized) = settlement {
            env.storage()
                .persistent()
                .set(&DataKey::Settlement(name.clone()), finalized);

            let token = token::Client::new(&env, &auction.asset);
            let mut clearing_price_paid = false;

            for bid in auction.bids.iter() {
                if finalized.sold
                    && finalized.winner == Some(bid.bidder.clone())
                    && bid.amount == finalized.winning_bid
                    && !clearing_price_paid
                {
                    clearing_price_paid = true;
                    let overpay = bid.amount.saturating_sub(finalized.clearing_price);
                    if overpay > 0 {
                        token.transfer(
                            &env.current_contract_address(),
                            &bid.bidder,
                            &(overpay as i128),
                        );
                    }
                    if finalized.clearing_price > 0 {
                        token.transfer(
                            &env.current_contract_address(),
                            &auction.treasury,
                            &(finalized.clearing_price as i128),
                        );
                    }
                } else {
                    token.transfer(
                        &env.current_contract_address(),
                        &bid.bidder,
                        &(bid.amount as i128),
                    );
                }
            }
        }
        Ok(settlement)
    }

    pub fn auction(env: Env, name: String) -> Option<Auction> {
        env.storage().persistent().get(&DataKey::Auction(name))
    }

    /// Total number of auctions ever created. Useful for clients that want
    /// to size a paging UI before fetching pages.
    pub fn auction_count(env: Env) -> u32 {
        auction_index(&env).len()
    }

    /// Paginated read-only listing of every auction name, in creation order.
    /// Bounded by `MAX_PAGE_SIZE`; `limit` is clamped if larger.
    pub fn list_auctions(env: Env, offset: u32, limit: u32) -> Vec<String> {
        slice_index(&env, &auction_index(&env), offset, limit)
    }

    /// Filter: names of auctions currently accepting bids at `now_unix`
    /// (i.e. `starts_at <= now_unix <= ends_at`) and not yet settled.
    /// Ordering: creation order. Bounded by `MAX_PAGE_SIZE`.
    pub fn list_active_auctions(env: Env, now_unix: u64, offset: u32, limit: u32) -> Vec<String> {
        filter_index(&env, offset, limit, |env, name| {
            if env
                .storage()
                .persistent()
                .has(&DataKey::Settlement(name.clone()))
            {
                return false;
            }
            match env
                .storage()
                .persistent()
                .get::<_, Auction>(&DataKey::Auction(name.clone()))
            {
                Some(a) => a.starts_at <= now_unix && now_unix <= a.ends_at,
                None => false,
            }
        })
    }

    /// Filter: names of auctions that have a recorded `Settlement`.
    /// Ordering: creation order. Bounded by `MAX_PAGE_SIZE`.
    pub fn list_settled_auctions(env: Env, offset: u32, limit: u32) -> Vec<String> {
        filter_index(&env, offset, limit, |env, name| {
            env.storage()
                .persistent()
                .has(&DataKey::Settlement(name.clone()))
        })
    }
}

fn auction_index(env: &Env) -> Vec<String> {
    env.storage()
        .persistent()
        .get(&DataKey::AuctionNames)
        .unwrap_or_else(|| Vec::new(env))
}

#[allow(dead_code)]
fn append_auction_name(env: &Env, name: &String) {
    let mut index = auction_index(env);
    index.push_back(name.clone());
    env.storage()
        .persistent()
        .set(&DataKey::AuctionNames, &index);
}

fn slice_index(env: &Env, index: &Vec<String>, offset: u32, limit: u32) -> Vec<String> {
    let mut out = Vec::new(env);
    let total = index.len();
    if offset >= total {
        return out;
    }
    let capped_limit = if limit > MAX_PAGE_SIZE {
        MAX_PAGE_SIZE
    } else {
        limit
    };
    let mut i = offset;
    while i < total && (i - offset) < capped_limit {
        if let Some(name) = index.get(i) {
            out.push_back(name);
        }
        i += 1;
    }
    out
}

fn filter_index(
    env: &Env,
    offset: u32,
    limit: u32,
    keep: impl Fn(&Env, &String) -> bool,
) -> Vec<String> {
    let index = auction_index(env);
    let capped_limit = if limit > MAX_PAGE_SIZE {
        MAX_PAGE_SIZE
    } else {
        limit
    };
    let mut matched = 0u32;
    let mut emitted = Vec::new(env);
    for (i, name) in index.iter().enumerate() {
        if !keep(env, &name) {
            continue;
        }
        if matched >= offset && (emitted.len() as u32) < capped_limit {
            emitted.push_back(name);
        }
        matched += 1;
        if (emitted.len() as u32) >= capped_limit {
            break;
        }
        let _ = i;
    }
    emitted
}

fn get_auction(env: &Env, name: &String) -> Result<Auction, AuctionError> {
    env.storage()
        .persistent()
        .get(&DataKey::Auction(name.clone()))
        .ok_or(AuctionError::NotFound)
}

fn put_auction(env: &Env, name: &String, auction: &Auction) {
    env.storage()
        .persistent()
        .set(&DataKey::Auction(name.clone()), auction);
}

fn settle_vickrey(auction: &Auction, settled_at: u64) -> Option<Settlement> {
    if auction.bids.is_empty() {
        return None;
    }

    let mut highest: Option<Bid> = None;
    let mut second_highest = 0u64;

    for bid in auction.bids.iter() {
        if highest
            .as_ref()
            .map(|current| bid.amount > current.amount)
            .unwrap_or(true)
        {
            second_highest = highest.as_ref().map(|current| current.amount).unwrap_or(0);
            highest = Some(bid);
        } else if bid.amount > second_highest {
            second_highest = bid.amount;
        }
    }

    let winning_bid = highest.as_ref()?.amount;
    if winning_bid < auction.reserve_price {
        return Some(Settlement {
            winner: None,
            clearing_price: 0,
            winning_bid,
            settled_at,
            sold: false,
        });
    }

    Some(Settlement {
        winner: highest.map(|bid| bid.bidder),
        clearing_price: if second_highest > auction.reserve_price {
            second_highest
        } else {
            auction.reserve_price
        },
        winning_bid,
        settled_at,
        sold: true,
    })
}
