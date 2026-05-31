#!/usr/bin/env bash
# Repeatable contract benchmark for storage growth and write-path cost (#165).
#
# Soroban deploy + footprint cost scales with compiled wasm size, so per-contract
# wasm byte size is a stable, reproducible proxy for storage/code cost trends to
# track as the workspace moves toward testnet/mainnet. This script builds every
# contract in release mode and reports each wasm's size (and total), so changes
# to the busy write paths (register / renew / transfer / resolver mutation) show
# up as size deltas across commits.
#
# Usage:   scripts/bench.sh
# Rerun after changes and diff the output to see relative cost trends.
#
# Requires: wasm32-unknown-unknown target.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

WASM_DIR="target/wasm32-unknown-unknown/release"
CONTRACTS=(xlm_ns_registry xlm_ns_registrar xlm_ns_resolver xlm_ns_subdomain xlm_ns_auction xlm_ns_bridge xlm_ns_nft)

echo "==> building all contracts (release, wasm32)"
cargo build --release --target wasm32-unknown-unknown \
  -p xlm-ns-registry -p xlm-ns-registrar -p xlm-ns-resolver \
  -p xlm-ns-subdomain -p xlm-ns-auction -p xlm-ns-bridge -p xlm-ns-nft

printf '\n%-22s %12s\n' "contract" "wasm_bytes"
printf '%-22s %12s\n' "--------" "----------"
total=0
for c in "${CONTRACTS[@]}"; do
  f="$WASM_DIR/${c}.wasm"
  if [[ -f "$f" ]]; then
    size=$(wc -c < "$f" | tr -d ' ')
    total=$((total + size))
    printf '%-22s %12s\n' "$c" "$size"
  else
    printf '%-22s %12s\n' "$c" "MISSING"
  fi
done
printf '%-22s %12s\n' "TOTAL" "$total"

echo
echo "Tip: the contract unit-test suites also exercise the write paths under the"
echo "Soroban test budget — run 'cargo test --workspace' to validate behaviour,"
echo "and compare this size report against a previous run to spot cost regressions."
