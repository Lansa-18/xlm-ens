#!/usr/bin/env bash
# Run the contract storage / hot-path benchmarks and write the report to
# target/bench-report.txt. The benchmarks themselves live in
# tests/benches/storage_benchmarks.rs and are marked #[ignore] so they
# do not run in the default test suite.
#
# Usage:
#   scripts/run-benchmarks.sh
#
# The output is intended for relative comparison ("did this PR change the
# cost of register / renew / transfer / set_record?"), not for predicting
# on-chain fees. See docs/contract-benchmarks.md for how to read the
# numbers.

set -euo pipefail

OUT="${1:-target/bench-report.txt}"
mkdir -p "$(dirname "$OUT")"

echo "Running storage benchmarks..."
cargo test --test storage_benchmarks -- --ignored --nocapture --test-threads=1 2>&1 | tee "$OUT"
echo
echo "Report written to $OUT"
