#!/usr/bin/env bash
# Generate Soroban contract spec artifacts for every contract crate (#149).
#
# Builds each contract to wasm and emits its machine-readable interface spec
# (JSON) under artifacts/specs/, where downstream tooling (the SDK binding
# check in scripts/check-sdk-bindings.sh, and CLI codegen) consumes them.
#
# Usage:   scripts/gen-specs.sh
# Output:  artifacts/specs/<crate>.json   (one per contract, e.g. xlm_ns_registry.json)
#
# Requires: the `stellar` (or legacy `soroban`) CLI on PATH, and the
# wasm32-unknown-unknown target (`rustup target add wasm32-unknown-unknown`).
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

SPECS_DIR="artifacts/specs"
WASM_DIR="target/wasm32-unknown-unknown/release"
mkdir -p "$SPECS_DIR"

# Prefer the modern `stellar contract`, fall back to legacy `soroban contract`.
if command -v stellar >/dev/null 2>&1; then
  SPEC_CMD=(stellar contract)
elif command -v soroban >/dev/null 2>&1; then
  SPEC_CMD=(soroban contract)
else
  echo "error: neither 'stellar' nor 'soroban' CLI is on PATH." >&2
  exit 2
fi

# pkg-name -> wasm/spec basename (cargo replaces '-' with '_' in artifact names).
CONTRACTS=(
  "xlm-ns-registry:xlm_ns_registry"
  "xlm-ns-registrar:xlm_ns_registrar"
  "xlm-ns-resolver:xlm_ns_resolver"
  "xlm-ns-subdomain:xlm_ns_subdomain"
  "xlm-ns-auction:xlm_ns_auction"
  "xlm-ns-bridge:xlm_ns_bridge"
  "xlm-ns-nft:xlm_ns_nft"
)

for entry in "${CONTRACTS[@]}"; do
  pkg="${entry%%:*}"
  base="${entry##*:}"
  echo "==> building $pkg"
  cargo build --release --target wasm32-unknown-unknown -p "$pkg"
  echo "==> spec $base"
  "${SPEC_CMD[@]}" spec --wasm "$WASM_DIR/${base}.wasm" --output json > "$SPECS_DIR/${base}.json"
done

echo "✓ specs written to $SPECS_DIR/ (run scripts/check-sdk-bindings.sh to verify SDK coverage)"
