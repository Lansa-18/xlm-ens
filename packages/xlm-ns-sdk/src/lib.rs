//! Async + blocking SDK for the xlm-ns name service contracts on Soroban.
//!
//! Two surfaces are exposed:
//! - [`XlmNsClient`] — the canonical async API. Prefer this in services
//!   already running on a tokio runtime.
//! - [`XlmNsBlockingClient`] — synchronous wrapper around the async client.
//!   Owns its own current-thread runtime, so callers in scripts, CLIs, or
//!   non-async services can use the SDK without taking on tokio plumbing.
//!
//! Both surfaces share [`config::ClientConfig`] for transport-level controls
//! (timeout, retry, user-agent). See `examples/` for end-to-end snippets.

pub mod blocking;
pub mod client;
pub mod config;
pub mod errors;
mod network;
#[cfg(test)]
mod tests;
pub mod types;

pub use blocking::XlmNsBlockingClient;
pub use client::{XlmNsClient, XlmNsClientBuilder};
pub use config::{ClientConfig, RetryConfig, DEFAULT_TRANSACTION_POLL_TIMEOUT};
pub use errors::SdkError;
pub use network::verify_network_passphrase;
pub use network::verify_transaction_passphrase;
pub use types::{RegisterResult, RegistrationReceipt, RenewResult, RenewalReceipt};
