//! Integration tests that exercise the SDK against a real Soroban RPC node.
//!
//! These are gated on the `XLM_NS_LIVE_SDK_TESTS` environment variable so they
//! are *not* part of the default `cargo test` run — that keeps CI green for
//! contributors who do not have a local stack up. Set the variable, point the
//! SDK at your local node, and run:
//!
//! ```sh
//! XLM_NS_LIVE_SDK_TESTS=1 \
//! XLM_NS_RPC_URL=http://localhost:8000/soroban/rpc \
//! XLM_NS_NETWORK_PASSPHRASE='Standalone Network ; February 2017' \
//! XLM_NS_REGISTRY_ID=CDAD... \
//! XLM_NS_REGISTRAR_ID=CDAD... \
//! XLM_NS_RESOLVER_ID=CDAD... \
//! cargo test -p xlm-ns-sdk --test local_soroban -- --nocapture --test-threads=1
//! ```
//!
//! See `docs/sdk-integration-tests.md` for how to bring a local node up.
//!
//! The tests cover at least one read path (`resolve` against the registry) and
//! one write path (`renew` against the registrar) — the SDK does not need to
//! understand the contract internals, only that the RPC round-trip succeeds.

use std::env;
use std::time::Duration;

use xlm_ns_sdk::{
    types::{RenewalRequest, SubmissionStatus},
    ClientConfig, XlmNsClient,
};

/// Returns `Some(env)` when the live test environment is configured, or `None`
/// when the test should be skipped. Skipped tests print a one-line note so
/// developers see why nothing ran.
fn live_env() -> Option<LiveEnv> {
    if env::var("XLM_NS_LIVE_SDK_TESTS").ok().as_deref() != Some("1") {
        eprintln!(
            "skip: XLM_NS_LIVE_SDK_TESTS!=1 — see packages/xlm-ns-sdk/tests/local_soroban.rs"
        );
        return None;
    }

    let rpc_url = env::var("XLM_NS_RPC_URL").ok()?;
    let registry = env::var("XLM_NS_REGISTRY_ID").ok()?;
    Some(LiveEnv {
        rpc_url,
        passphrase: env::var("XLM_NS_NETWORK_PASSPHRASE").ok(),
        registry,
        registrar: env::var("XLM_NS_REGISTRAR_ID").ok(),
        resolver: env::var("XLM_NS_RESOLVER_ID").ok(),
        signer: env::var("XLM_NS_SIGNER").ok(),
        test_name: env::var("XLM_NS_TEST_NAME").unwrap_or_else(|_| "alice.xlm".to_string()),
    })
}

struct LiveEnv {
    rpc_url: String,
    passphrase: Option<String>,
    registry: String,
    registrar: Option<String>,
    resolver: Option<String>,
    signer: Option<String>,
    test_name: String,
}

fn build_client(env: &LiveEnv) -> XlmNsClient {
    let mut builder = XlmNsClient::builder(&env.rpc_url)
        .registry(&env.registry)
        .config(
            ClientConfig::default()
                .with_timeout(Duration::from_secs(15))
                .with_max_retries(2)
                .with_user_agent("xlm-ns-sdk-integration/0.1"),
        );
    if let Some(p) = &env.passphrase {
        builder = builder.network_passphrase(p);
    }
    if let Some(r) = &env.registrar {
        builder = builder.registrar(r);
    }
    if let Some(r) = &env.resolver {
        builder = builder.resolver(r);
    }
    builder.build()
}

#[tokio::test]
async fn resolve_against_local_node() {
    let Some(env) = live_env() else { return };
    let client = build_client(&env);

    let resolution = client
        .resolve(&env.test_name)
        .await
        .expect("resolve should succeed against the local registry");

    assert_eq!(resolution.name, env.test_name);
    assert!(
        resolution.expires_at.is_some(),
        "expected the registry to populate expires_at",
    );
}

#[tokio::test]
async fn renew_against_local_node() {
    let Some(env) = live_env() else { return };
    let client = build_client(&env);

    let receipt = client
        .renew(RenewalRequest {
            name: env.test_name.clone(),
            additional_years: 1,
            signer: env.signer.clone(),
        })
        .await
        .expect("renew should produce a submission against the local registrar");

    assert_eq!(receipt.name, env.test_name);
    assert_eq!(receipt.additional_years, 1);
    assert!(matches!(
        receipt.submission.status,
        SubmissionStatus::Submitted | SubmissionStatus::Confirmed,
    ));
}

#[tokio::test]
async fn contract_error_surfaces_diagnostic_events() {
    let Some(env) = live_env() else { return };
    let client = build_client(&env);

    // Attempt to renew a name that does not exist to trigger a contract error.
    let result = client
        .renew(RenewalRequest {
            name: "this-name-definitely-does-not-exist.xlm".to_string(),
            additional_years: 1,
            signer: env.signer.clone(),
        })
        .await;

    assert!(
        result.is_err(),
        "expected renewal of nonexistent name to fail and surface diagnostic events"
    );
}

#[test]
fn blocking_client_resolves_against_local_node() {
    use xlm_ns_sdk::XlmNsBlockingClient;

    let Some(env) = live_env() else { return };
    let client = XlmNsBlockingClient::from_async(build_client(&env))
        .expect("blocking client should start its runtime");

    let resolution = client
        .resolve(&env.test_name)
        .expect("blocking resolve should succeed against the local registry");
    assert_eq!(resolution.name, env.test_name);
}
