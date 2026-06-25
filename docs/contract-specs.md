# Contract spec artifacts (#149)

Every contract crate has a reproducible path to a machine-readable interface
spec that downstream tooling (the SDK, CLI codegen, and the binding-coverage
check) can consume.

## Generate

From the workspace root:

```bash
scripts/gen-specs.sh
```

This builds each contract to `wasm32v1-none` (release) and writes one
spec per contract to **`artifacts/specs/<crate>.json`**, e.g.
`artifacts/specs/xlm_ns_registry.json`. Requires the `stellar` (or legacy
`soroban`) CLI and `rustup target add wasm32v1-none`.

Contracts covered: `registry`, `registrar`, `resolver`, `subdomain`, `auction`,
`bridge`, `nft` (artifact basenames use `_`, matching cargo's wasm output names).

## Consume

- **SDK / CLI tooling:** read the JSON spec for a contract to discover its
  functions, parameters, and types (the spec is the array Soroban emits from
  `stellar contract spec --output json`).
- **Binding coverage:** `scripts/check-sdk-bindings.sh` reads the same
  `artifacts/specs/` directory and verifies the SDK client calls every method the
  contracts expose. Run `gen-specs.sh` first, then `check-sdk-bindings.sh`.

The artifacts always live under `artifacts/specs/` relative to the workspace
root, so they're easy to locate from CI or local tooling.
