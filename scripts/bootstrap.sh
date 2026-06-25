#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  scripts/bootstrap.sh [--install]

Checks the local toolchain needed for this repo. With --install, it attempts to
install missing pieces in a safe, rerunnable way.

What it checks:
  - rustup / cargo (Rust toolchain)
  - wasm32 target
  - soroban CLI
EOF
}

INSTALL=false
if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi
if [[ "${1:-}" == "--install" ]]; then
  INSTALL=true
fi

need_cmd() {
  command -v "$1" >/dev/null 2>&1
}

header() {
  printf '\n== %s ==\n' "$1"
}

header "Rust"
if need_cmd rustup; then
  rustup --version
else
  echo "Missing: rustup"
  echo "Install: https://rustup.rs/"
  exit 1
fi

if need_cmd cargo; then
  cargo --version
else
  echo "Missing: cargo (should come with rustup)"
  exit 1
fi

header "Wasm target"
if rustup target list --installed | grep -q '^wasm32v1-none$'; then
  echo "wasm32v1-none: installed"
else
  echo "wasm32v1-none: missing"
  if [[ "$INSTALL" == "true" ]]; then
    rustup target add wasm32v1-none
  else
    echo "Install with: rustup target add wasm32v1-none"
  fi
fi

header "Soroban CLI"
if need_cmd soroban; then
  soroban --version || true
else
  echo "Missing: soroban"
  if [[ "$INSTALL" == "true" ]]; then
    cargo install --locked soroban-cli
  else
    echo "Install with: cargo install --locked soroban-cli"
  fi
fi

header "Done"
echo "Bootstrap checks complete."

