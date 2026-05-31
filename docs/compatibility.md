# Compatibility Matrix

This document tracks which versions of the smart contracts, the `xlm-ns-cli`, the `xlm-ns-sdk`, and the deployed network manifests are known to work together. Because the system is composed of several independent but interacting components, mismatched versions can lead to serialization errors or unauthorized invocations.

## Versioning Policy

`xlm-ns` components track versioning together during the `0.x` pre-release phase:
- **Contracts**: ABI-breaking changes bump the minor version (e.g., `0.1.x` to `0.2.x`).
- **SDK**: Matches contract ABI expectations. A `0.2.x` SDK is guaranteed to work with `0.2.x` contracts.
- **CLI**: Bundles the SDK and behaves identically to the SDK's versioning rules.

## Current Compatibility Matrix

| Contracts | SDK | CLI | Network | Known Good Manifest |
| :--- | :--- | :--- | :--- | :--- |
| `v0.1.x` | `v0.1.x` | `v0.1.x` | Stellar Testnet | `testnet-v0.1.json` |
| `v0.1.x` | `v0.1.x` | `v0.1.x` | Local Sandbox | N/A (ephemeral) |

> *Note: Mainnet deployments have not commenced. Mainnet compatibility will be added in Phase 3.*

## Upgrading and Verification

Before upgrading your integration or operational tooling, check the compatibility table above. The CLI and SDK both enforce interface checks during invocation.

### Checking Component Versions

Since the project uses a single Cargo workspace, all components (Contracts, CLI, SDK) typically share the workspace version defined in `Cargo.toml`. To view the active CLI version locally:

```sh
cargo pkgid -p xlm-ns-cli
```

### Checking SDK Version in Rust

If you depend on the `xlm-ns-sdk` in a Rust project, ensure your `Cargo.toml` aligns with the deployed contract version:

```toml
[dependencies]
xlm-ns-sdk = "0.1.0" # Use a specific version compatible with the contracts
```

### Invoking Contracts with the CLI

You can explicitly point the CLI at a specific network and contract IDs to ensure you are interacting with a compatible deployment. Replace the placeholders with your target contract IDs:

```sh
cargo run -p xlm-ns-cli -- \
  --network testnet \
  --registry-contract-id "<REGISTRY_CONTRACT_ID_PLACEHOLDER>" \
  --resolver-contract-id "<RESOLVER_CONTRACT_ID_PLACEHOLDER>" \
  resolve "alice.xlm"
```

## Relevant Tests and Scripts

The CI pipeline automatically enforces cross-component compatibility. You can reproduce these checks locally to ensure your modifications remain compatible:

- **Contract Specs**: Extracted automatically. If you change a contract, regenerate specs with the commands in `docs/contributing-contracts.md`.
- **Snapshot Tests**: Integration tests verify serialized state representations. Read how to update them in `docs/contributing-contracts.md`.
- **End-to-End Tests**: Run the full workspace test suite to verify the SDK and CLI correctly interact with local contract builds:
  ```sh
  TMPDIR=/tmp cargo test --workspace
  ```
- **Operator Runbook**: See `docs/testnet-operator-runbook.md` for step-by-step CLI usage against testnet deployments.