#!/usr/bin/env bash
set -euo pipefail

echo "Checking for cargo-mutants..."
if ! command -v cargo-mutants &> /dev/null; then
    echo "cargo-mutants not found. Installing..."
    cargo install cargo-mutants
fi

echo "Running mutation tests for xlm-ns-registry..."
cargo mutants -p xlm-ns-registry --no-shuffle -v

echo "Running mutation tests for xlm-ns-registrar..."
cargo mutants -p xlm-ns-registrar --no-shuffle -v

echo "Running mutation tests for xlm-ns-resolver..."
cargo mutants -p xlm-ns-resolver --no-shuffle -v

echo "Mutation tests completed successfully."