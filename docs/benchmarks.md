# Contract benchmarks (#165)

A repeatable way to track the storage/code cost of the contracts before the
workspace moves further toward testnet and mainnet.

## Run

```bash
scripts/bench.sh
```

It builds every contract (release, wasm32) and reports each compiled wasm's byte
size plus the total. On Soroban, **deploy and footprint cost scale with wasm
size**, so this is a stable, reproducible proxy for storage/code-cost trends:
changes to the busy write paths (`register`, `renew`, `transfer`, resolver
mutation) surface as size deltas. Rerun after a change and diff the output to see
relative trends.

## Behavioural cost under the test budget

The per-contract unit suites exercise the write paths inside the Soroban test
budget. Run them with:

```bash
cargo test --workspace
```

to validate behaviour alongside the size report. (A future enhancement can print
`env.cost_estimate()` CPU/memory per write path directly from a dedicated bench
test; the size report + budgeted tests give the cost-trend signal today.)
