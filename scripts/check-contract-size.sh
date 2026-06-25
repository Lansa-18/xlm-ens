#!/usr/bin/env bash
set -euo pipefail

CONFIG_FILE="${1:-.github/contract-size-budget.json}"
WASM_DIR="${2:-target/wasm32v1-none/release}"
MARKDOWN_FILE="${3:-artifacts/contract-size-report.md}"

echo "Checking contract WASM sizes against budget..."
echo "Config: $CONFIG_FILE"
echo "WASM directory: $WASM_DIR"
echo "Markdown report will be written to: $MARKDOWN_FILE"

# Ensure jq is available
if ! command -v jq &> /dev/null; then
    echo "Error: jq is required but not installed." >&2
    exit 1
fi

# Read config into arrays
CONTRACTS=()
while IFS= read -r line; do
    CONTRACTS+=("$line")
done < <(jq -r '.[] | .name' "$CONFIG_FILE")

MAX_SIZES=()
while IFS= read -r line; do
    MAX_SIZES+=("$line")
done < <(jq -r '.[] | .max_size_bytes' "$CONFIG_FILE")

# Ensure arrays have same length
if [ "${#CONTRACTS[@]}" -ne "${#MAX_SIZES[@]}" ]; then
    echo "Error: mismatched lengths in config" >&2
    exit 1
fi

# Ensure output directory exists
mkdir -p "$(dirname "$MARKDOWN_FILE")"

# Markdown report
{
    echo "| Contract | Size (bytes) | Budget (bytes) | Status |"
    echo "|----------|--------------|----------------|--------|"
    FAILED=0
    for i in "${!CONTRACTS[@]}"; do
        contract="${CONTRACTS[$i]}"
        max="${MAX_SIZES[$i]}"
        wasm_file="${WASM_DIR}/${contract}.wasm"
        if [[ ! -f "$wasm_file" ]]; then
            echo "| $contract | MISSING | $max | ❌ Missing |"
            FAILED=1
            continue
        fi
        size=$(wc -c < "$wasm_file")
        if (( size > max )); then
            status="❌ OVER ($((size - max)) bytes over)"
            FAILED=1
        else
            status="✅ OK"
        fi
        printf "| %s | %'d | %'d | %s |\n" "$contract" "$size" "$max" "$status"
    done
} > "$MARKDOWN_FILE"

echo "Report written to $MARKDOWN_FILE"
cat "$MARKDOWN_FILE"

if (( FAILED )); then
    echo "::error::One or more contracts exceed the WASM size budget."
    exit 1
else
    echo "All contracts within size limits."
fi