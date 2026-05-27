# Contract Spec Artifacts

Every contract crate in this workspace publishes a Soroban contract spec —
a JSON description of its function signatures, types, and errors — so SDKs,
CLIs, and IDE tooling can call the contracts without reading the source.

## Where they come from

Specs are produced by `soroban contract spec --wasm <crate>.wasm --output
json` after the WASM is built for `wasm32-unknown-unknown` in release mode.
Two code paths produce the same artifacts:

| Path | Trigger | Output |
|---|---|---|
| **CI** | `.github/workflows/ci.yml`, `artifacts` job, every push to `main` and every PR | uploaded as `soroban-contract-artifacts-<sha>` (90-day retention) |
| **Local** | `scripts/generate-specs.sh` | `./artifacts/wasm/`, `./artifacts/specs/` |

The local script is a reproducible mirror of the CI step. Rerun it
whenever you change a contract surface so your local SDK / tooling
checks see the new shape.

## Generating locally

```bash
# One-time toolchain setup
rustup target add wasm32-unknown-unknown
cargo install --locked soroban-cli

# Build wasm + emit specs for every contract crate
scripts/generate-specs.sh

# Or write to a custom directory
scripts/generate-specs.sh --out /tmp/xlm-specs
```

Output layout (mirrors CI):

```
artifacts/
  wasm/
    xlm_ns_registry.wasm
    xlm_ns_resolver.wasm
    ...
  specs/
    xlm_ns_registry.json
    xlm_ns_resolver.json
    ...
```

## How tooling consumes them

### Rust SDK (`packages/xlm-ns-sdk`)

`scripts/check-sdk-bindings.sh artifacts/specs` reads every spec JSON and
verifies that the method names hardcoded in the SDK client still exist on
the on-chain contract. CI runs this check on every PR; run it locally with
the same invocation against either a fresh `./artifacts/specs` or a
downloaded CI artifact bundle. The check exits non-zero on drift, so a
stale SDK cannot ship without an obvious failure.

### Soroban CLI

The CLI's `--wasm <file>.wasm` and `--spec-json <file>.json` invocations
both accept the artifacts produced here. The `--spec-json` form lets you
call deployed contracts without re-downloading the contract spec from the
network on every call:

```bash
soroban contract invoke \
  --network testnet \
  --id <CONTRACT_ID> \
  --spec-json artifacts/specs/xlm_ns_registry.json \
  -- register --name "<name>" --owner <ADDR> --duration_secs 31536000
```

### IDE / editor extensions

Most Soroban-aware editor extensions can read a spec JSON to enable
autocomplete and parameter hints. Point them at the relevant file under
`artifacts/specs/`.

### Generating other-language bindings

`soroban contract bindings` accepts the same WASM and can produce TypeScript
or Python bindings; the workspace ships specs primarily so external
integrators can use whichever generator suits their stack.

## When to regenerate

Regenerate specs any time you:

- add, remove, or rename a `#[contractimpl]` method;
- change the shape of a `#[contracttype]` struct or enum that appears on
  the public surface;
- add or remove a `#[contracterror]` variant.

Pure-internal changes (private helpers, storage layout that's still
serialized the same way) do not require regeneration, but rerunning the
script is cheap and harmless.
