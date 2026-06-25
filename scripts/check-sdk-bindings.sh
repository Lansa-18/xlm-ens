#!/usr/bin/env bash
# Detect drift between deployed Soroban contracts and the xlm-ns-sdk surface.
#
# Reads the JSON spec files produced by `soroban contract spec --output json`
# (the CI `artifacts` job emits them under `artifacts/specs/`) and checks that
# every method the SDK is hardcoded to call still exists on the corresponding
# contract. Exits non-zero on drift so it can wedge CI before a stale SDK
# ships.
#
# Usage:
#   scripts/check-sdk-bindings.sh [path-to-specs]
#
# `path-to-specs` defaults to ./artifacts/specs. Pass an alternative directory
# (e.g. a downloaded CI artifact) to validate without rebuilding locally.

set -euo pipefail

SPECS_DIR="${1:-artifacts/specs}"

if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq is required (brew install jq / apt install jq)" >&2
  exit 2
fi

if [[ ! -d "$SPECS_DIR" ]]; then
  echo "error: spec directory '$SPECS_DIR' does not exist." >&2
  echo "       Build it with the CI 'artifacts' job, or run:" >&2
  echo "         cargo build --release --target wasm32v1-none -p xlm-ns-registry ..." >&2
  echo "         soroban contract spec --wasm <file>.wasm --output json > $SPECS_DIR/<file>.json" >&2
  exit 2
fi

# Map contract -> spec file basename -> required method names.
# Keep this list in sync with the methods the SDK calls in
# packages/xlm-ns-sdk/src/client.rs.
declare -a CHECKS=(
  "registry|xlm_ns_registry|register resolve transfer set_resolver renew names_for_owner"
  "registrar|xlm_ns_registrar|register renew quote_registration"
  "resolver|xlm_ns_resolver|resolve set_record set_text_record set_primary_name remove_record reverse"
  "subdomain|xlm_ns_subdomain|register_parent add_controller create transfer revoke"
  "auction|xlm_ns_auction|create_auction place_bid settle auction"
  "bridge|xlm_ns_bridge|register_chain route build_message"
  "nft|xlm_ns_nft|owner_of token_uri"
)

failures=0

for entry in "${CHECKS[@]}"; do
  IFS='|' read -r label spec_basename required <<<"$entry"
  spec_file="$SPECS_DIR/${spec_basename}.json"

  if [[ ! -f "$spec_file" ]]; then
    echo "skip: $label — no spec at $spec_file (build the wasm artifact first)"
    continue
  fi

  # Soroban spec JSON is an array of entries; each function-shaped entry has
  # `function_v0.name` (newer specs) or `name` at the top level (older shape).
  # Accept both.
  available=$(jq -r '
    . as $root
    | if (type == "array") then $root else [$root] end
    | map(
        if has("function_v0") then .function_v0.name
        elif (.kind // "") == "function" then .name
        elif has("function") then .function.name
        else empty
        end
      )
    | unique
    | .[]
  ' "$spec_file")

  contract_failures=0
  for method in $required; do
    if ! grep -Fxq "$method" <<<"$available"; then
      echo "drift: $label is missing required method '$method' (file: $spec_file)" >&2
      contract_failures=$((contract_failures + 1))
    fi
  done

  if (( contract_failures == 0 )); then
    echo "ok: $label — $(echo "$required" | wc -w | tr -d ' ') method(s) verified"
  fi
  failures=$((failures + contract_failures))
done

if (( failures > 0 )); then
  echo "" >&2
  echo "$failures binding mismatch(es) detected. Update the SDK or the contract." >&2
  exit 1
fi

echo ""
echo "All SDK bindings match the deployed contract specs."
