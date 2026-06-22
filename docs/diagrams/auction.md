# Auction Flow Diagram (Vickrey Second-Price)

This diagram displays the sequence of steps for auctioning premium names. In public ledgers, commit-reveal schemes are recommended to keep bids secret until the reveal phase.

```mermaid
sequenceDiagram
    autonumber
    actor Operator as "Operator / Admin"
    actor BidderA as "Bidder A"
    actor BidderB as "Bidder B"
    participant Auction as "Auction Contract"
    participant Token as "Token Contract (XLM)"
    participant Registrar as "Registrar Contract"
    participant Registry as "Registry Contract"

    Operator->>Auction: create_auction(name, asset, treasury, reserve_price, starts, ends)
    Note over Auction: Initialize auction state
    Auction-->>Operator: Auction Created

    Note over Auction: Bidding Phase
    BidderA->>Auction: place_bid(name, bidder_a, amount_a, now)
    Auction->>Token: transfer(bidder_a, auction, amount_a)
    Token-->>Auction: Success
    Note over Auction: Record Bidder A's bid
    Auction-->>BidderA: Bid Placed

    BidderB->>Auction: place_bid(name, bidder_b, amount_b, now)
    Auction->>Token: transfer(bidder_b, auction, amount_b)
    Token-->>Auction: Success
    Note over Auction: Record Bidder B's bid (highest)
    Auction-->>BidderB: Bid Placed

    Note over Auction: Settlement Phase (After end_time)
    Operator->>Auction: settle(name, now)
    Note over Auction: Determine winner (Bidder B) and<br/>clearing price (Bidder A's bid)
    
    Auction->>Token: transfer(auction, bidder_a, amount_a) [Refund Loser]
    Token-->>Auction: Success

    Auction->>Token: transfer(auction, bidder_b, amount_b - clearing_price) [Refund Overpayment]
    Token-->>Auction: Success

    Auction->>Token: transfer(auction, treasury, clearing_price) [Payment to Treasury]
    Token-->>Auction: Success
    
    Note over Auction: Mark auction settled
    Auction-->>Operator: Settle Success (Emit event)

    Note over Operator: Name Handover
    Operator->>Registrar: register_settled_name(label, bidder_b, duration_years, ...)
    Registrar->>Registry: register(name, bidder_b, ...)
    Registry-->>Registrar: Success
    Registrar-->>Operator: Handover complete
```

## Detailed Flow Steps

1. **Auction Initialization**: The Operator creates an auction on `AuctionContract` specifying the FQDN, asset used for bidding (e.g. XLM), reserve price, and bidding timeframes.
2. **Escrow Bidding**: Bidders place bids. To commit to the bid, the `AuctionContract` initiates a transfer to escrow the bidding tokens directly into the contract storage.
3. **Settlement Math**: When the bidding ends, the auction is settled. The settlement calculation:
   - Identifies the highest bidder.
   - Calculates the clearing price: the second highest bid or the reserve price, whichever is higher.
4. **Fund Distribution**:
   - Losers receive full refunds of their escrows.
   - The winner is refunded the difference between their bid and the second-price clearing price.
   - The clearing price is transferred to the configured treasury address.
5. **Ownership Handover**: Once settled, the operator registers the FQDN to the winner by invoking the `Registrar` which bypasses the reserved namespace rules and creates the registration entry in the `Registry`.
