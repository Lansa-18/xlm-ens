#!/usr/bin/env bash
set -euo pipefail

RPC_URL="${SOROBAN_RPC_URL:-https://soroban-testnet.stellar.org}"
NETWORK="${CANARY_NETWORK:-testnet}"
LOG_FILE="${CANARY_LOG_FILE:-canary-results.ndjson}"

# Emit one JSON entry and append it to the log file.
# Usage: record_result <test_name> <pass|fail> <duration_ms> [error_msg]
record_result() {
  local name="$1" result="$2" duration="$3" error="${4:-}"
  local ts
  ts="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  local entry
  entry=$(printf '{"timestamp":"%s","network":"%s","test":"%s","result":"%s","duration_ms":%s,"error":%s}' \
    "$ts" "$NETWORK" "$name" "$result" "$duration" \
    "$([ -n "$error" ] && printf '"%s"' "${error//\"/\\\"}" || echo 'null')")
  echo "$entry" | tee -a "$LOG_FILE"
}

# Run a single test, measure duration, record outcome.
run_test() {
  local name="$1"; shift
  local t0 t1 duration output rc
  t0=$(date +%s%3N)
  output=$("$@" 2>&1) && rc=0 || rc=$?
  t1=$(date +%s%3N)
  duration=$(( t1 - t0 ))
  if [ "$rc" -eq 0 ]; then
    record_result "$name" "pass" "$duration"
  else
    record_result "$name" "fail" "$duration" "$output"
  fi
  return "$rc"
}

jsonrpc() {
  local method="$1"
  curl -fsS --max-time 15 -H 'content-type: application/json' \
    --data "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"${method}\"}" \
    "${RPC_URL}"
}

echo "Canary: RPC health check  network=${NETWORK}  url=${RPC_URL}  log=${LOG_FILE}" >&2

overall=0
run_test "rpc_getHealth"   jsonrpc getHealth   || overall=1
run_test "rpc_getNetwork"  jsonrpc getNetwork  || overall=1

[ "$overall" -eq 0 ] && echo "OK" >&2 || { echo "FAILED" >&2; exit 1; }
