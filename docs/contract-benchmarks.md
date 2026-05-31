# Contract Benchmarks

Storage-growth and hot-path-write benchmarks for the contract workspace.
They exist so we can answer one question on every PR:

> Did this change make a hot write path materially more expensive?

They are not on-chain cost predictions — they are SDK-side budget counters
that scale linearly with the host's accounting. Use them for relative
comparison, not for forecasting testnet/mainnet fees.

## What gets measured

`tests/benches/storage_benchmarks.rs` runs the busiest write paths under
the `soroban-sdk` test environment and reads
`env.cost_estimate().budget()` for each invocation:

| Path | Why it matters |
|---|---|
| `registry::register` | New-name flow; storage grows by one Entry + one OwnerNames update. |
| `registry::renew` | Hot update on an existing entry; should be cheap and flat. |
| `registry::transfer` | Two OwnerNames updates + Entry mutation. |
| `resolver::set_record` (first write) | New Forward + Reverse entry per name. |
| `resolver::set_record` (overwrite) | Same name, different address — measures the overwrite path that real users take. |

For each path the benchmark resets the SDK budget before each call, runs
`N = 25` iterations, and reports:

- average CPU instructions and memory bytes per call;
- the first-call vs. last-call cost (a non-trivial positive delta on
  `register` is expected — the OwnerNames `Vec` grows; an upward trend on
  `renew` or `set_record` overwrite is not).

## Running locally

```bash
scripts/run-benchmarks.sh
```

That wraps:

```bash
cargo test --test storage_benchmarks -- --ignored --nocapture --test-threads=1
```

The default test suite (`cargo test --workspace`) excludes these — the
benchmarks are marked `#[ignore]` so they only run when explicitly asked
for. The report is written to `target/bench-report.txt` for diffing across
branches.

## Reading the output

Each section looks like:

```
=== registry::register (fresh) (25 iterations) ===
  avg cpu:     195567   avg mem:      80989
  first cpu:   118392    last cpu:   261406    delta: +143014
  first mem:    54615    last mem:   107009    delta: +52394
```

- **`avg cpu` / `avg mem`** — call this the "headline number." Compare
  before/after a change.
- **`delta`** — the gap between the first and last iteration. Strongly
  positive means the operation gets more expensive as state grows —
  expected for `register` because of the per-owner index, suspicious for
  `renew`/`set_record` overwrites.

## When to update the benchmarks

- A new write path becomes load-bearing — add it to the matrix.
- A `#[contracttype]` struct on a hot path gains fields that change its
  storage footprint — bumping iterations or adding an explicit
  storage-size assertion may make sense.
- The SDK is upgraded to a new soroban-sdk minor — absolute numbers will
  shift, but the relative shape across paths should stay the same.

## What we deliberately don't do

- We don't gate CI on absolute numbers. Cost counters drift across SDK
  versions, host versions, and rustc updates. Make the comparison on the
  same branch / same toolchain.
- We don't try to convert these to stroops. That conversion lives in the
  Soroban fee schedule and changes outside this repo.
