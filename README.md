# xlm-ns

![CI Status](https://github.com/0xVida/xlm-ens/actions/workflows/ci.yml/badge.svg)

`xlm-ns` is a Rust workspace for a Stellar name service where names like
`timmy.xlm` behave as user-owned identifiers for accounts, apps, subdomains, and
cross-chain resolution targets.

The repository is organized as a multi-crate system so the core naming logic can
be tested locally before it is wired into Soroban-specific storage, auth, and
deployment flows.

## Vision

The target user experience is straightforward:

- A user registers a base name such as `timmy.xlm`.
- That name resolves to a Stellar address or another delivery target.
- The owner can update resolver data, renew the registration, transfer ownership,
  create subdomains, or bridge the name to external resolver networks.
- Premium names can be sold through auctions instead of first-come-first-served
  issuance.

  

## Architecture

For a detailed breakdown of state ownership, cross-contract flows, and synchronization rules, see the Architecture Documentation.

## Security and Threat Model

For information regarding trust boundaries, admin powers, and open risks, please refer to the Security Assumptions and Threat Model.

## Current status

The workspace now contains real contract-domain logic instead of only placeholder
stubs:

- Shared validation for labels, full names, registration periods, owners, and
  chain identifiers.
- Lifecycle-aware name records with registration, expiry, and grace-period data.
- Stateful registry, registrar, resolver, auction, subdomain, NFT, and bridge
  contract logic.
- Unit tests for all contract crates covering the main happy-path flows.

## Workspace layout

### Contracts

- `contracts/registry`
  Purpose: canonical name ownership state.
  Responsibilities:
  - Stores `NameRecord` ownership and metadata.
  - Enforces active/grace/claimable lifecycle checks.
  - Restricts mutation to the current owner.
  - Supports transfer, resolver updates, target updates, metadata updates, and
    expiry extension.

- `contracts/resolver`
  Purpose: forward and reverse resolution.
  Responsibilities:
  - Maps `name -> resolution record`.
  - Maps `address -> primary name`.
  - Stores bounded text records such as social handles or app metadata.
  - Enforces owner-controlled updates and deletion.

- `contracts/registrar`
  Purpose: registration issuance and renewal policy.
  Responsibilities:
  - Computes quotes from label length and registration duration.
  - Tracks reserved names.
  - Accepts registrations and renewals.
  - Maintains treasury balance accounting in the domain model.
  - Uses explicit expiry and grace-period rules.

- `contracts/auction`
  Purpose: premium-name sale flow.
  Responsibilities:
  - Creates auctions with a reserve price and bidding window.
  - Records bids with timestamps.
  - Settles using a Vickrey-style second-price outcome.
  - Supports unsold outcomes when the reserve is not met.

- `contracts/subdomain`
  Purpose: delegated namespace management.
  Responsibilities:
  - Registers parent domains for subdomain issuance.
  - Supports parent owners and delegated controllers.
  - Creates and transfers owned subdomains such as `pay.timmy.xlm`.

- `contracts/nft`
  Purpose: tokenized representation of name ownership.
  Responsibilities:
  - Mints ownership tokens.
  - Tracks owner, approval, and metadata.
  - Supports approval-based transfers.

- `contracts/bridge`
  Purpose: cross-chain resolution payload construction.
  Responsibilities:
  - Registers supported destination chains.
  - Maps chains to resolver and gateway targets.
  - Builds deterministic Axelar-style payloads for resolution propagation.

### Packages

- `packages/xlm-ns-common`
  Shared constants, errors, types, and validation helpers used by the contract
  crates.

- `packages/xlm-ns-sdk`
  A lightweight Rust SDK surface for future wallet and dapp integration.

### Operator tooling

- `cli/`
  Simple command-line entry points for register, resolve, renew, transfer, and
  auction flows.

- `scripts/`
  Shell helpers for deploy, invoke, and local setup tasks.

#### CLI output modes

All CLI commands accept `--output human` (default) or `--output json` for automation-friendly output.

Examples:

- `cargo run -p xlm-ns-cli -- resolve timmy.xlm --output json`
- `cargo run -p xlm-ns-cli -- whois timmy.xlm --output json`
- `cargo run -p xlm-ns-cli -- portfolio GDRA...OWNER_ADDR --output json`

#### Contract spec artifacts (CI)

CI uploads a `soroban-contract-artifacts` artifact containing built contract WASM files and extracted contract specs (JSON).

- `tests/`
  Placeholders for integration scenarios and test fixtures shared across crates.

## Core domain model

`NameRecord` in `packages/xlm-ns-common` is the shared type used by the main
contract flows. It currently tracks:

- `label`
- `tld`
- `owner`
- `resolver`
- `target_address`
- `ttl_seconds`
- `registered_at`
- `expires_at`
- `grace_period_ends_at`

This matters because the registry and registrar now reason about the same
registration lifecycle:

- Active: `now <= expires_at`
- Grace period: `expires_at < now <= grace_period_ends_at`
- Claimable by a new owner: `now > grace_period_ends_at`

## Release and recovery policy

The current product policy is intentionally conservative:

- Admin recovery is not supported in either the registrar or the registry.
- There is no privileged forced transfer, forced burn, or emergency reassignment
  path for a live name.
- A name only becomes available for a new registrant after its normal expiry and
  grace period have both elapsed.

This keeps ownership and release behavior predictable while the contracts are
still maturing. If a future version introduces an admin recovery mechanism, it
must define explicit Soroban auth requirements, emit an auditable contract
event trail, and document the governance process around who can invoke it.

## Registration flow

The registration flow is now integrated on-chain:

1. Ask the registrar for a quote using the requested label and registration
   duration.
2. Submit payment and create a registration record. The registrar automatically
   materializes the name in the registry with the resulting ownership state.
3. Set resolver records for forward and reverse lookups.
4. Optionally mint an NFT and configure bridge routes or subdomains.

## Registry-Resolver Synchronization

To prevent ownership drift between registry and resolver, resolver operations are authorized against the registry's ownership state rather than the resolver's stored owner field. The resolver contract stores a registry address and queries it for ownership checks during writes.

When a name is transferred in the registry, the resolver's stored owner is updated to maintain consistency and provide accurate ownership information in resolution records.

This ensures a single source of truth for ownership across the system.

## Naming and validation rules

Shared validation currently enforces:

- Minimum and maximum label length.
- Lowercase ASCII letters, digits, and hyphens only.
- No leading or trailing hyphen.
- Explicit `.xlm` TLD parsing for base names.
- Bounded registration durations.
- Non-empty owner and chain identifiers.

## Quickstart

### Prerequisites

- [Rust toolchain](https://rustup.rs/) (latest stable)
- Wasm target: `rustup target add wasm32-unknown-unknown`
- Soroban CLI (`cargo install --locked soroban-cli`)

### Bootstrap (recommended)

To validate (and optionally install) the toolchain in a rerunnable way:

```sh
./scripts/bootstrap.sh --install
```

### Local setup

Clone the repository and format the workspace:

```sh
git clone https://github.com/0xVida/xlm-ens.git
cd xlm-ens
cargo fmt --all
```

Run tests:

```sh
TMPDIR=/tmp cargo test --workspace
```

`TMPDIR=/tmp` is used here because the current sandbox environment does not allow
Rust to create temporary build directories in the default macOS temp location.

## Operator docs

- Testnet operator runbook: `docs/testnet-operator-runbook.md`
- Bridge payload + resolver schema docs: `docs/schemas.md`
- Version compatibility matrix: `docs/compatibility.md`

## Roadmap

The project is currently in active development. The following milestones outline the path to production:

### Phase 1: Foundation (MVP) - **CURRENT**
- [x] Core contract logic (Registry, Registrar, Resolver)
- [x] Auction settlement (Vickrey style)
- [x] Basic SDK stubs and CLI entry points
- [x] Shared validation rules

### Phase 2: Testnet Beta
- [x] **Integration**: Wire CLI through full quote/submit flows (#9)
- [ ] **Testing**: Expand auction and edge-case unit tests (#95)
- [ ] **Automation**: CI/CD for formatting and workspace tests (#99)
- [ ] **SDK**: Full client implementation with Soroban RPC integration

### Phase 3: Mainnet Readiness
- [ ] **Security**: External audits of contract storage and auth patterns
- [ ] **Governance**: Implement treasury controls and fee management
- [ ] **Ecosystem**: Axelar-style bridging and NFT representation (X-NFT)
- [ ] **Docs**: Developer portal and integration cookbook

## Backlog Execution

We use a standardized label taxonomy to manage the 100-issue backlog:
- `area/*`: contracts, sdk, cli, docs, ops
- `type/*`: bug, feature, improvement, security
- `priority/*`: high, medium, low
- `milestone/*`: mvp, testnet-beta, mainnet
