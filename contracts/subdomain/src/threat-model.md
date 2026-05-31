# Security Assumptions and Threat Model

This document outlines the trust boundaries, security assumptions, and threat model for the `xlm-ns` smart contract system.

## Trust Boundaries and Authentication (Auth)

- **Ownership Authority**: The `Registry` contract is the canonical source of truth for base domain ownership (`*.xlm`). All mutations to resolution records, transfers, and renewals require authorization from the active owner.
- **Subdomain Authority**: The `Subdomain` contract is the canonical source of truth for subdomains. Subdomain creation requires authorization from the parent domain owner or a delegated controller.
- **Contract-to-Contract Auth**: Contracts like `Resolver` and `Subdomain` trust the `Registry` (and each other) for ownership states. Calls must verify the caller's authorization through Soroban's native authentication (`require_auth`) or by checking ownership against the canonical registry.
- **Admin Powers**: The system is designed to minimize centralized control. Admin powers are limited to protocol-level parameter adjustments (e.g., pricing updates in the `Registrar`, registering supported chains in the `Bridge`). Admins do *not* have the power to arbitrarily seize, transfer, or modify user-owned active domains.

## Ownership Drift

Because resolution state is split from the canonical registry, "ownership drift" is a primary concern.

- **Mitigation**: The `Resolver` enforces authorization by querying the `Registry` (or `Subdomain` contract) dynamically. Transferring a name in the `Registry` instantly invalidates the previous owner's ability to mutate records in the `Resolver`.
- **Subdomain Independence**: Subdomains are resolved directly against the `Subdomain` contract's state, isolating their ownership lifecycle from base names in the main `Registry`.

## Replay Assumptions

- **Cross-Chain Replay**: The `Bridge` contract constructs deterministic payloads for external networks. Replay attacks are mitigated by including nonces, chain IDs, and structured payload schemas to ensure a resolution update processed on a destination chain cannot be replayed.
- **Soroban Replay**: Native Soroban transactions handle nonce management and replay protection at the protocol level. Contracts assume that if Soroban's native auth passes, the invocation is fresh and intended.

## Auction Abuse

Premium names are sold via a Vickrey-style (second-price) auction in the `Auction` contract.

- **Bid Hiding and Front-running**: In a public ledger, if bids are submitted in plaintext, front-running and bid manipulation are trivial. A commit-reveal scheme is assumed or required for future iterations to ensure bids remain hidden until the settlement phase.
- **Unsold Outcomes**: Auctions require a reserve price. If the reserve is not met, the name remains unissued and can be auctioned again.
- **Timestamp Manipulation & Sniping**: Soroban limits ledger timestamp manipulation, but validators have slight leeway. Auction bidding windows must be sufficiently large (e.g., multiple days) so that minor timestamp drift does not affect the outcome. To prevent sniper abuse, an anti-sniping extension window is enforced: bids placed near the auction's end automatically extend the end time.

## Bridge Routing Risks

- **Gateway Malfeasance**: Cross-chain resolution relies on external bridge gateways (e.g., Axelar). If the gateway or the destination chain's verification mechanism is compromised, invalid resolution records could be pushed to external networks.
- **Mitigation**: The `Bridge` contract only allows registered and approved destination chains and gateways. Cross-chain updates are strictly deterministic and traceable. Users must trust the consensus of the bridged network.

## Open Risks

- **Subdomain Squatting/Takeover**: If a parent domain expires and is re-registered by a new owner, the status of previously issued subdomains needs explicit handling to prevent the new parent owner from hijacking existing subdomains or the subdomains becoming entirely orphaned.
- **Data Availability Limit**: The `Resolver` stores text records. Excessive amounts of data could hit Soroban's storage limits or bloat state costs. Bounded record sizes and storage rent must be carefully managed.
- **Front-running Standard Registrations**: Standard base names are registered on a first-come, first-served basis. In a mempool-based system, a bot could observe a registration request and front-run it by paying a higher inclusion fee.