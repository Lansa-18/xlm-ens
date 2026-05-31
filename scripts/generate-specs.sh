#!/usr/bin/env bash
# Generate Soroban contract spec artifacts for every contract crate.
#
# Output layout matches the CI `artifacts` job (see .github/workflows/ci.yml)
# so downstream consumers — the SDK drift checker, IDE tooling, docs
# generators — can read the same paths whether the specs were built locally
# or downloaded from a CI artifact bundle.
#
#   artifacts/wasm/<crate>.wasm
#   artifacts/specs/<crate>.json
#
# Usage:
#   scripts/generate-specs.sh [--out DIR]
#
# Defaults to ./artifacts. The script is idempotent: rerunning it overwrites
# the files in place.

set -euo pipefail

OUT_DIR="artifacts"
if [[ "${1:-}" == "--out" ]]; then
  OUT_DIR="${2:?--out requires a directory}"
fi
if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  sed -n '2,18p' "$0"
  exit 0
fi

if ! command -v soroban >/dev/null 2>&1; then
  echo "error: soroban CLI is required. Install with: cargo install --locked soroban-cli" >&2
  exit 2
fi

# Crate -> wasm filename root (cargo replaces hyphens with underscores).
CRATES=(
  "xlm-ns-registry|xlm_ns_registry"
  "xlm-ns-registrar|xlm_ns_registrar"
  "xlm-ns-resolver|xlm_ns_resolver"
  "xlm-ns-auction|xlm_ns_auction"
  "xlm-ns-subdomain|xlm_ns_subdomain"
  "xlm-ns-nft|xlm_ns_nft"
  "xlm-ns-bridge|xlm_ns_bridge"
)

WASM_DIR="$OUT_DIR/wasm"
SPEC_DIR="$OUT_DIR/specs"
mkdir -p "$WASM_DIR" "$SPEC_DIR"

PKG_ARGS=()
for entry in "${CRATES[@]}"; do
  IFS='|' read -r pkg _ <<<"$entry"
  PKG_ARGS+=(-p "$pkg")
done

echo "== Building WASM (release, wasm32-unknown-unknown) =="
cargo build --release --target wasm32-unknown-unknown "${PKG_ARGS[@]}"

echo "== Collecting WASM and emitting specs into $OUT_DIR =="
for entry in "${CRATES[@]}"; do
  IFS='|' read -r _ wasm_root <<<"$entry"
  src="target/wasm32-unknown-unknown/release/${wasm_root}.wasm"
  dst="$WASM_DIR/${wasm_root}.wasm"
  spec="$SPEC_DIR/${wasm_root}.json"
  if [[ ! -f "$src" ]]; then
    echo "error: expected WASM not found: $src" >&2
    exit 1
  fi
  cp "$src" "$dst"
  soroban contract spec --wasm "$dst" --output json > "$spec"
  printf '  %-22s -> %s (%d bytes wasm)\n' "$wasm_root" "$spec" "$(wc -c <"$dst")"
done

echo "Done. Specs in $SPEC_DIR, wasm in $WASM_DIR."
