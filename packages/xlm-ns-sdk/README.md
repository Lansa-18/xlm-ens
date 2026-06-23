# xlm-ns-sdk

Async + blocking Rust SDK for the xlm-ns name service contracts on Soroban.

## Two surfaces

- **`XlmNsClient`** — async API, the canonical surface. Use this from any
  service already running on a tokio runtime.
- **`XlmNsBlockingClient`** — synchronous wrapper around the async client.
  Owns its own current-thread runtime so CLIs and scripts can use the SDK
  without taking on tokio plumbing.

The blocking client is implemented on top of the async one: every blocking
call drives the same async method through `runtime.block_on`. There is no
duplicated logic — the async path is the source of truth.

## Configuration

Transport-level controls live on `ClientConfig`:

| Field | Default | Purpose |
|---|---|---|
| `timeout` | `30s` | Per-request timeout. Bounds a single RPC call (not the total wall-clock across retries). |
| `retry.max_retries` | `3` | Number of retry attempts on transient transport errors. `0` disables retries. |
| `retry.initial_backoff` | `1s` | Initial delay before the first retry; doubles per attempt. |
| `retry.max_backoff` | `30s` | Cap on the exponential backoff delay. |
| `retry.jitter` | `true` | Randomize each retry delay uniformly in `[0, backoff]`. |
| `user_agent` | `xlm-ns-sdk/<crate-version>` | Sent as the HTTP `User-Agent` so operators can identify SDK traffic in upstream logs. |

Override anything with the chainable setters:

```rust
use std::time::Duration;
use xlm_ns_sdk::{ClientConfig, XlmNsClient};

let client = XlmNsClient::builder("https://soroban-rpc.example")
    .registry("CDAD...REGISTRY")
    .config(
        ClientConfig::default()
            .with_timeout(Duration::from_secs(10))
            .with_max_retries(5)
            .with_user_agent("my-service/1.2.3"),
    )
    .build();
```

## Async usage

```rust
use xlm_ns_sdk::{types::RegistrationRequest, XlmNsClient};

# async fn run() -> Result<(), xlm_ns_sdk::SdkError> {
let client = XlmNsClient::builder("https://soroban-rpc.example")
    .network_passphrase("Test SDF Network ; September 2015")
    .registry("CDAD...REGISTRY")
    .registrar("CDAD...REGISTRAR")
    .build();

let resolution = client.resolve("alice.xlm").await?;
println!("alice.xlm -> {:?}", resolution.address);

let receipt = client.register(RegistrationRequest {
    label: "bob".into(),
    owner: "GDRA...OWNER".into(),
    duration_years: 1,
    signer: Some("treasury".into()),
}).await?;
println!("registered {} for {} years", receipt.name, receipt.duration_years);
# Ok(()) }
```

## Blocking usage

```rust
use xlm_ns_sdk::{XlmNsBlockingClient, XlmNsClient};

# fn run() -> Result<(), xlm_ns_sdk::SdkError> {
let client = XlmNsBlockingClient::from_async(
    XlmNsClient::builder("https://soroban-rpc.example")
        .registry("CDAD...REGISTRY")
        .build(),
)?;

let resolution = client.resolve("alice.xlm")?;
println!("alice.xlm -> {:?}", resolution.address);
# Ok(()) }
```

## Integration tests against a local Soroban node

See [`docs/sdk-integration-tests.md`](../../docs/sdk-integration-tests.md) for
the full local setup. The suite is gated on `XLM_NS_LIVE_SDK_TESTS=1` so it
does not run in default CI; once the env vars are set it covers a read path
(`resolve`) and a write path (`renew`) against deployed contracts.

## Spec drift

`scripts/check-sdk-bindings.sh` validates that every method the SDK calls
still exists on the corresponding contract. CI runs it as part of the
artifacts job; run it locally after rebuilding the WASM artifacts to catch
drift before opening a PR.
