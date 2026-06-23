//! Transport-level configuration for the SDK client.
//!
//! Production integrations need to override the SDK's transport behavior
//! (timeouts when an RPC node is slow, retries for transient network blips,
//! a custom `User-Agent` so operators can identify their traffic in upstream
//! logs). `ClientConfig` is the single place those knobs live.
//!
//! Defaults are tuned for interactive workloads:
//! - 30 s request timeout
//! - 3 retry attempts on transient transport failures
//! - 1 s initial backoff with exponential growth (capped at 30 s)
//! - 60 s transaction polling window for write-path hydration
//! - User-agent of `xlm-ns-sdk/<crate-version>`
//!
//! Override individual fields with the chainable setters; everything is
//! immutable once it lands inside the client.

use std::time::Duration;

use rand::Rng;

/// Default per-request timeout when calling Soroban RPC.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Default number of retry attempts for transient transport errors.
pub const DEFAULT_MAX_RETRIES: u32 = 3;

/// Default initial backoff delay between retries.
pub const DEFAULT_INITIAL_BACKOFF: Duration = Duration::from_secs(1);

/// Default upper bound on the exponential backoff delay.
pub const DEFAULT_MAX_BACKOFF: Duration = Duration::from_secs(30);

/// Default amount of time the SDK will wait for a submitted transaction to
/// reach a terminal state when final-status hydration is enabled.
pub const DEFAULT_TRANSACTION_POLL_TIMEOUT: Duration = Duration::from_secs(60);

/// Returns the default `User-Agent` string identifying this SDK build.
pub fn default_user_agent() -> String {
    format!("xlm-ns-sdk/{}", env!("CARGO_PKG_VERSION"))
}

/// Behavior for retrying transient transport errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryConfig {
    /// Maximum number of retry attempts after the first failed call. Set to
    /// zero to disable retries entirely.
    pub max_retries: u32,
    /// Initial delay between the failed call and the first retry. Subsequent
    /// retries double this delay until [`max_backoff`](Self::max_backoff).
    pub initial_backoff: Duration,
    /// Upper bound on the backoff delay between retries.
    pub max_backoff: Duration,
    /// When true, apply full jitter: sleep uniformly in `[0, backoff]`.
    pub jitter: bool,
}

impl RetryConfig {
    /// A retry policy that does not retry. Useful for test paths and for
    /// callers that prefer to manage retries themselves.
    pub const fn disabled() -> Self {
        Self {
            max_retries: 0,
            initial_backoff: Duration::from_millis(0),
            max_backoff: Duration::from_millis(0),
            jitter: false,
        }
    }

    /// Returns the base backoff for retry attempt `attempt` (0-indexed), capped
    /// at [`max_backoff`](Self::max_backoff). `attempt = 0` is the delay
    /// before the first retry.
    pub fn backoff_for(&self, attempt: u32) -> Duration {
        if self.max_retries == 0 {
            return Duration::from_millis(0);
        }
        let factor = 1u64 << attempt.min(16);
        let delay = self
            .initial_backoff
            .checked_mul(factor.try_into().unwrap_or(u32::MAX))
            .unwrap_or(self.max_backoff);
        delay.min(self.max_backoff)
    }

    /// Returns the duration to sleep before the next retry. With jitter enabled
    /// this uses full jitter: a uniform random delay in `[0, backoff_for(attempt)]`.
    pub fn sleep_duration(&self, attempt: u32) -> Duration {
        let base = self.backoff_for(attempt);
        if !self.jitter || base.is_zero() {
            return base;
        }
        let millis = base.as_millis().min(u128::from(u64::MAX)) as u64;
        if millis == 0 {
            return Duration::ZERO;
        }
        // Full jitter: uniform in [0, base].
        Duration::from_millis(rand::thread_rng().gen_range(0..=millis))
    }
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: DEFAULT_MAX_RETRIES,
            initial_backoff: DEFAULT_INITIAL_BACKOFF,
            max_backoff: DEFAULT_MAX_BACKOFF,
            jitter: true,
        }
    }
}

/// Transport-level configuration shared between the async and blocking SDK
/// clients.
///
/// Construct with [`ClientConfig::default`] and override fields with the
/// chainable setters:
///
/// ```
/// use std::time::Duration;
/// use xlm_ns_sdk::config::ClientConfig;
///
/// let config = ClientConfig::default()
///     .with_timeout(Duration::from_secs(10))
///     .with_max_retries(5)
///     .with_user_agent("my-service/1.2.3");
///
/// assert_eq!(config.timeout, Duration::from_secs(10));
/// assert_eq!(config.retry.max_retries, 5);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientConfig {
    /// Per-request timeout. The SDK aborts a single RPC call once this elapses
    /// (it does not bound the total wall-clock time across retries).
    pub timeout: Duration,
    /// Retry policy applied to transient transport errors.
    pub retry: RetryConfig,
    /// Value sent in the HTTP `User-Agent` header on every request.
    pub user_agent: String,
    /// When true, write-path helpers poll RPC for the terminal transaction
    /// status before returning a submission result.
    pub poll_final_status: bool,
    /// Maximum time spent waiting for the transaction to settle.
    pub transaction_poll_timeout: Duration,
}

impl ClientConfig {
    /// Override [`timeout`](Self::timeout).
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Override [`retry`](Self::retry) wholesale.
    pub fn with_retry(mut self, retry: RetryConfig) -> Self {
        self.retry = retry;
        self
    }

    /// Override [`RetryConfig::max_retries`].
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.retry.max_retries = max_retries;
        self
    }

    pub fn with_initial_backoff(mut self, initial_backoff: Duration) -> Self {
        self.retry.initial_backoff = initial_backoff;
        self
    }

    pub fn with_max_backoff(mut self, max_backoff: Duration) -> Self {
        self.retry.max_backoff = max_backoff;
        self
    }

    pub fn with_jitter(mut self, enabled: bool) -> Self {
        self.retry.jitter = enabled;
        self
    }

    /// Override [`user_agent`](Self::user_agent).
    pub fn with_user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = user_agent.into();
        self
    }

    /// Enable or disable post-submission polling.
    pub fn with_poll_final_status(mut self, poll_final_status: bool) -> Self {
        self.poll_final_status = poll_final_status;
        self
    }

    /// Override the transaction polling timeout.
    pub fn with_transaction_poll_timeout(mut self, timeout: Duration) -> Self {
        self.transaction_poll_timeout = timeout;
        self
    }
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_TIMEOUT,
            retry: RetryConfig::default(),
            user_agent: default_user_agent(),
            poll_final_status: true,
            transaction_poll_timeout: DEFAULT_TRANSACTION_POLL_TIMEOUT,
        }
    }
}

/// Identifies a well-known Stellar network and provides the matching RPC URL
/// and network passphrase. Pass to [`XlmNsClientBuilder::from_preset`] to
/// avoid hard-coding these values in application code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkPreset {
    Testnet,
    Mainnet,
}

impl NetworkPreset {
    /// Soroban RPC endpoint for this network.
    pub fn rpc_url(self) -> &'static str {
        match self {
            Self::Testnet => "https://soroban-testnet.stellar.org",
            Self::Mainnet => "https://soroban.stellar.org",
        }
    }

    /// Network passphrase required for transaction signing.
    pub fn passphrase(self) -> &'static str {
        match self {
            Self::Testnet => "Test SDF Network ; September 2015",
            Self::Mainnet => "Public Global Stellar Network ; September 2015",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_documented_values() {
        let config = ClientConfig::default();
        assert_eq!(config.timeout, DEFAULT_TIMEOUT);
        assert_eq!(config.retry.max_retries, DEFAULT_MAX_RETRIES);
        assert_eq!(config.retry.initial_backoff, DEFAULT_INITIAL_BACKOFF);
        assert_eq!(config.retry.max_backoff, DEFAULT_MAX_BACKOFF);
        assert!(config.retry.jitter);
        assert!(config.user_agent.starts_with("xlm-ns-sdk/"));
        assert!(config.poll_final_status);
        assert_eq!(
            config.transaction_poll_timeout,
            DEFAULT_TRANSACTION_POLL_TIMEOUT
        );
    }

    #[test]
    fn chainable_setters_override_individual_fields() {
        let config = ClientConfig::default()
            .with_timeout(Duration::from_secs(30))
            .with_max_retries(7)
            .with_user_agent("svc/0.1")
            .with_poll_final_status(false)
            .with_transaction_poll_timeout(Duration::from_secs(3));

        assert_eq!(config.timeout, Duration::from_secs(30));
        assert_eq!(config.retry.max_retries, 7);
        assert_eq!(config.user_agent, "svc/0.1");
        assert!(!config.poll_final_status);
        assert_eq!(config.transaction_poll_timeout, Duration::from_secs(3));
    }

    #[test]
    fn retry_disabled_returns_zero_backoff() {
        let policy = RetryConfig::disabled();
        assert_eq!(policy.backoff_for(0), Duration::from_millis(0));
        assert_eq!(policy.backoff_for(5), Duration::from_millis(0));
    }

    #[test]
    fn retry_backoff_grows_exponentially_then_caps() {
        let policy = RetryConfig {
            max_retries: 8,
            initial_backoff: Duration::from_millis(100),
            max_backoff: Duration::from_millis(1_000),
            jitter: false,
        };

        assert_eq!(policy.backoff_for(0), Duration::from_millis(100));
        assert_eq!(policy.backoff_for(1), Duration::from_millis(200));
        assert_eq!(policy.backoff_for(2), Duration::from_millis(400));
        assert_eq!(policy.backoff_for(3), Duration::from_millis(800));
        // capped at max_backoff
        assert_eq!(policy.backoff_for(4), Duration::from_millis(1_000));
        assert_eq!(policy.backoff_for(20), Duration::from_millis(1_000));
    }

    #[test]
    fn sleep_duration_without_jitter_matches_backoff() {
        let policy = RetryConfig {
            max_retries: 3,
            initial_backoff: Duration::from_millis(200),
            max_backoff: Duration::from_secs(5),
            jitter: false,
        };
        assert_eq!(policy.sleep_duration(1), Duration::from_millis(400));
    }

    #[test]
    fn sleep_duration_with_jitter_stays_within_bounds() {
        let policy = RetryConfig {
            max_retries: 3,
            initial_backoff: Duration::from_millis(500),
            max_backoff: Duration::from_secs(5),
            jitter: true,
        };
        for _ in 0..32 {
            let delay = policy.sleep_duration(0);
            assert!(delay <= Duration::from_millis(500));
        }
        // Full jitter can be zero.
        assert!(policy.sleep_duration(0) <= Duration::from_millis(500));
    }
}
