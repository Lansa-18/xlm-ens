# SDK integration tests

`packages/xlm-ns-sdk/tests/local_soroban.rs` exercises the SDK against a real
Soroban RPC node. The tests are skipped by default — they only run when
`XLM_NS_LIVE_SDK_TESTS=1` is set in the environment, so contributors without a
local stack do not need to wait for them.

The suite covers at least one read path (`resolve`) and one write path
(`renew`) using both the async and blocking client surfaces.

## Bringing a local node up

We use `soroban-cli` (now `stellar-cli`) to run a local sandbox. With a recent
toolchain installed (`scripts/bootstrap.sh --install`):

```sh
stellar network start standalone
```

This starts a local Stellar node with a Soroban RPC endpoint at
`http://localhost:8000/soroban/rpc` and the standalone passphrase
`Standalone Network ; February 2017`.

## Deploying the contracts the SDK expects

Build the contracts and deploy each one. From the workspace root:

```sh
cargo build --release --target wasm32v1-none \
  -p xlm-ns-registry \
  -p xlm-ns-registrar \
  -p xlm-ns-resolver

REGISTRY_ID=$(stellar contract deploy \
  --wasm target/wasm32v1-none/release/xlm_ns_registry.wasm \
  --network standalone --source default)

REGISTRAR_ID=$(stellar contract deploy \
  --wasm target/wasm32v1-none/release/xlm_ns_registrar.wasm \
  --network standalone --source default)

RESOLVER_ID=$(stellar contract deploy \
  --wasm target/wasm32v1-none/release/xlm_ns_resolver.wasm \
  --network standalone --source default)
```

Pre-register at least one name so the read-path test has data:

```sh
stellar contract invoke \
  --id "$REGISTRY_ID" --network standalone --source default -- \
  register --name alice.xlm --owner default --duration_years 1
```

## Running the suite

```sh
XLM_NS_LIVE_SDK_TESTS=1 \
XLM_NS_RPC_URL=http://localhost:8000/soroban/rpc \
XLM_NS_NETWORK_PASSPHRASE='Standalone Network ; February 2017' \
XLM_NS_REGISTRY_ID="$REGISTRY_ID" \
XLM_NS_REGISTRAR_ID="$REGISTRAR_ID" \
XLM_NS_RESOLVER_ID="$RESOLVER_ID" \
XLM_NS_SIGNER=default \
XLM_NS_TEST_NAME=alice.xlm \
cargo test -p xlm-ns-sdk --test local_soroban -- --nocapture --test-threads=1
```

Use `--test-threads=1` so the read- and write-path tests do not race on the
same name.

## CI

The workspace `cargo test` step in `.github/workflows/ci.yml` does not set
`XLM_NS_LIVE_SDK_TESTS`, so the live tests are skipped there. To run them on a
self-hosted runner with a Soroban node, set the same env vars in the workflow
and add `--test local_soroban` to the test invocation.

## Spec drift check

`scripts/check-sdk-bindings.sh` is the partner of these integration tests: it
parses the JSON specs produced by `soroban contract spec --output json` and
fails CI when the SDK references a method the contract no longer exposes. Run
it locally after rebuilding the WASM artifacts:

```sh
cargo build --release --target wasm32v1-none \
  -p xlm-ns-registry -p xlm-ns-registrar -p xlm-ns-resolver \
  -p xlm-ns-auction -p xlm-ns-subdomain -p xlm-ns-nft -p xlm-ns-bridge

mkdir -p artifacts/wasm artifacts/specs
cp target/wasm32v1-none/release/xlm_ns_*.wasm artifacts/wasm/
for wasm in artifacts/wasm/*.wasm; do
  base="$(basename "${wasm%.wasm}")"
  soroban contract spec --wasm "$wasm" --output json > "artifacts/specs/${base}.json"
done

scripts/check-sdk-bindings.sh
```
