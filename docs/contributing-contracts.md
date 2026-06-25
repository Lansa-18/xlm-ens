# Contributing: Contract Specs, Snapshots, and Integration Tests

This guide covers the parts of the xlm-ns workspace that require extra care when
you change contracts: generated specs, snapshot test output, and multi-contract
integration flows.  Read this before opening a PR that touches any file under
`contracts/`.

---

## Table of contents

1. [Contract specs](#1-contract-specs)
2. [Snapshot tests](#2-snapshot-tests)
3. [Integration tests](#3-integration-tests)
4. [Required local commands](#4-required-local-commands)
5. [CI expectations](#5-ci-expectations)

---

## 1. Contract specs

### What they are

Each Soroban contract exposes a machine-readable ABI called a *contract spec*.
The CI `artifacts` job (`.github/workflows/ci.yml`) builds every contract to
`wasm32v1-none`, extracts the spec with `soroban contract spec`, and
uploads the JSON files under `artifacts/specs/`.

### When specs change

A spec changes whenever you:

- Add, remove, or rename a public `#[contractimpl]` function.
- Change the type or arity of any function argument or return value.
- Add, remove, or rename a `#[contracttype]` struct, enum, or error enum.
- Change a `#[contracterror]` discriminant value.

Removing or renaming anything that is already in a deployed spec is a **breaking
change** — existing callers and the SDK client will break silently.  Prefer
adding new functions over changing existing ones.

### How to update specs intentionally

```sh
# 1. Build the contracts for the wasm target.
cargo build --release --target wasm32v1-none \
  -p xlm-ns-registry -p xlm-ns-registrar -p xlm-ns-resolver \
  -p xlm-ns-auction  -p xlm-ns-subdomain -p xlm-ns-nft -p xlm-ns-bridge

# 2. Regenerate the spec files.
mkdir -p artifacts/specs
for wasm in target/wasm32v1-none/release/xlm_ns_*.wasm; do
  base="$(basename "${wasm%.wasm}")"
  soroban contract spec --wasm "$wasm" --output json \
    > "artifacts/specs/${base}.json"
done
```

Commit the updated JSON files together with the Rust changes that caused them.
A PR that modifies a contract function but does not update the corresponding
spec file will fail review.

---

## 2. Snapshot tests

### What they are

Snapshot files live in `tests/test_snapshots/`.  Each file captures the
serialized output of one integration-test scenario — typically the full
`NameRecord` or resolution payload returned by a cross-contract flow.  Snapshots
let reviewers see exactly what a change does to on-chain state without reading
through assertions manually.

### When snapshots change

Snapshots must be updated whenever:

- The shape of a `NameRecord`, `ResolutionRecord`, or any other serialized type
  changes.
- A cross-contract flow changes the values it writes (e.g. TTL defaults, grace
  period durations, fee amounts).
- You intentionally add a new snapshot test for a new scenario.

A snapshot that no longer matches the actual output will cause `cargo test` to
fail with a diff showing old vs. new.

### How to update snapshots intentionally

```sh
# Re-run the integration tests in update mode.
UPDATE_SNAPSHOTS=1 cargo test --test registrar_registry_test
```

Review the diff in `tests/test_snapshots/` before committing.  Never commit a
snapshot update without understanding why the output changed.

Snapshot filenames follow this convention:

```
<test_module_name>.<test_number>.json
```

e.g. `subdomain_flow_covers_controller_delegation_transfer_and_resolution.1.json`

---

## 3. Integration tests

### Layout

```
tests/
├── Cargo.toml                  — test crate manifest
├── integration/                — one file per contract-pair or flow
│   ├── registrar_registry_test.rs
│   ├── registrar_test.rs
│   ├── registry_test.rs
│   ├── subdomain_test.rs
│   ├── auction_test.rs
│   └── bridge_test.rs
├── fixtures/                   — shared test data (accounts, name seeds)
│   └── accounts.json
└── test_snapshots/             — serialized scenario outputs (see §2)
```

Each file under `integration/` maps to a `[[test]]` entry in `tests/Cargo.toml`
and is compiled as its own test binary.  This means a build error in one file
does not block other test targets.

### Fixture conventions

`fixtures/accounts.json` contains named Stellar account key-pairs for use in
tests.  Add new fixtures there rather than hardcoding keys inline.  Keep the
file sorted by key name.

Do not use real secret keys in fixtures — use deterministically generated test
keys only (`Address::generate(&env)` for Soroban unit tests; placeholder
`G…` strings for CLI-level tests).

### Writing a new integration test

1. Create `tests/integration/<contract>_<scenario>_test.rs`.
2. Add a `[[test]]` entry to `tests/Cargo.toml`:
   ```toml
   [[test]]
   name = "<contract>_<scenario>_test"
   path = "integration/<contract>_<scenario>_test.rs"
   ```
3. Use `Env::default()` from `soroban-sdk` with the `testutils` feature — do
   not rely on a live network.
4. Wire contracts together with `env.register(…)` and initialize via the client
   (see `registrar_registry_test.rs` for the canonical pattern).
5. Assert on both sides of every cross-contract invariant — if the registrar
   writes to the registry, check *both* the registrar record and the registry
   entry.

### Invariants to verify in every cross-contract test

| Invariant | Where to check |
|-----------|---------------|
| `expires_at` matches between registrar and registry | Both clients |
| `grace_period_ends_at` matches | Both clients |
| Ownership is consistent after transfer | Registry client |
| Subdomain controller list is updated | Subdomain client |
| Auction winner becomes name owner | Registry client |

### Placeholder tests

Files that have not yet been filled in use:

```rust
#[test]
fn <name>_placeholder() {
    assert!(true);
}
```

When you implement a real scenario, replace the placeholder entirely — do not
leave it alongside real tests.

---

## 4. Storage migration strategy

Treat persistent storage as versioned data, even when the current layout is
still at version 1. The pattern in this workspace is:

- Add a version marker or migration hook in the contract that owns the
  persistent state.
- Route lifecycle and timestamp math through shared helpers so future schema
  changes do not duplicate logic.
- Keep compatibility shims small and explicit: if a storage layout changes,
  the upgrade path should rebuild or repair the derived indexes before the new
  version is considered active.

The registry contract is the reference example because it owns both canonical
entries and the owner index. Any future storage upgrade should repair that
index before writing the new storage version marker. The current code exposes
`storage_schema_version()` as the lightweight read-only hook for clients and
upgrade tooling.

---

## 5. Required local commands

Run these before pushing any contract change:

```sh
# Format check (must pass CI)
cargo fmt --all --check

# Full workspace test (includes integration tests)
cargo test --workspace

# Build wasm artifacts and regenerate specs
cargo build --release --target wasm32v1-none \
  -p xlm-ns-registry -p xlm-ns-registrar -p xlm-ns-resolver \
  -p xlm-ns-auction  -p xlm-ns-subdomain -p xlm-ns-nft -p xlm-ns-bridge

mkdir -p artifacts/specs
for wasm in target/wasm32v1-none/release/xlm_ns_*.wasm; do
  base="$(basename "${wasm%.wasm}")"
  soroban contract spec --wasm "$wasm" --output json \
    > "artifacts/specs/${base}.json"
done

# Optional: Run mutation tests to verify invariant coverage
./scripts/mutants.sh
```

If you do not have `soroban-cli` installed:

```sh
cargo install --locked soroban-cli
```

---

## 6. CI expectations

| Job | What it checks | Blocks merge? |
|-----|---------------|---------------|
| `fmt` | `cargo fmt --all --check` | Yes |
| `test` | `cargo test --workspace` | Yes |
| `artifacts` | Contract WASM builds + spec extraction | Yes |

A PR that breaks any of these jobs will not be merged.

If you intentionally change a public contract interface, update the spec JSON
files as described in §1 and add a note in your PR description explaining the
change and any migration considerations for existing deployments.
