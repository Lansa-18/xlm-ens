# Mutation Testing

To ensure our test suite actually enforces critical invariants—like `Address::require_auth()` calls, lifecycle constraints, and fee calculations—we use cargo-mutants.

Mutation testing injects small faults (mutations) into the contract source code (e.g., removing a check, flipping a `<` to `<=`, returning early) and runs the test suite. If the tests still pass, the mutation was "missed", highlighting a gap in our coverage.

## Running Locally

You can run the automated mutation test suite using the provided script:

```sh
./scripts/mutants.sh
```

This script will automatically install `cargo-mutants` if it isn't already available and run it against the critical contract crates (`xlm-ns-registry`, `xlm-ns-registrar`, and `xlm-ns-resolver`).

## Interpreting Results

- **Caught**: The test suite correctly identified the injected fault.
- **Missed**: The test suite passed despite the fault. This means a critical guard or invariant check is not adequately covered by tests. You must add a test to cover this specific scenario.
- **Timeout**: The mutation caused an infinite loop or other hang.
- **Unviable**: The mutation caused a compilation error.

## Adding Tests for Missed Mutations

When a mutation is missed, you need to add a test that fails when that specific mutation is applied. For example, if removing `owner.require_auth()` in `set_text_record` is missed, you should add a test that explicitly verifies `set_text_record` requires authorization.

Use deterministic fixtures (like those in `tests/fixtures/accounts.json`) and avoid production credentials when writing these tests to ensure they are repeatable across environments. Ensure failure messages clearly indicate whether the contract, SDK, or CLI layer is responsible.