use core::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractErrorCode {
    NameNotFound = 1,
    NotOwner = 2,
    Expired = 3,
    InvalidLabel = 4,
    Other = 99,
}

#[derive(Debug)]
pub enum SdkError {
    InvalidRequest(String),
    Transport(String),
    Ingestion(String),
    ContractError(ContractErrorCode),
    /// The network passphrase returned by the RPC server does not
    /// match the passphrase configured in the SDK client.
    NetworkPassphraseMismatch {
        configured: String,
        rpc_reported: String,
    },
    /// The network passphrase embedded in a transaction does not
    /// match the passphrase configured in the SDK client.
    TransactionPassphraseMismatch {
        configured: String,
        in_transaction: String,
    },
    ContractInvocationFailed {
        operation: &'static str,
        reason: String,
        tx_hash: Option<String>,
    },
    SimulationFailed {
        operation: &'static str,
        reason: String,
    },
    InsufficientFee {
        operation: &'static str,
        required: i64,
        available: i64,
    },
    TransactionTimeout {
        operation: &'static str,
        ledger_submitted: u32,
    },
    SigningFailed {
        operation: &'static str,
        source: SigningError,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SigningError {
    Rejected { reason: String },
    InvalidKey { reason: String },
    ExternalFailure { reason: String },
    MalformedEnvelope { reason: String },
}

impl fmt::Display for SdkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRequest(message) => write!(f, "invalid request: {message}"),
            Self::Transport(message) => write!(f, "transport error: {message}"),
            Self::Ingestion(message) => write!(f, "ingestion error: {message}"),
            Self::ContractError(code) => write!(f, "contract error: {code:?}"),
            Self::NetworkPassphraseMismatch {
                configured,
                rpc_reported,
            } => write!(
                f,
                "network passphrase mismatch: configured={configured:?}, rpc_reported={rpc_reported:?}"
            ),
            Self::TransactionPassphraseMismatch {
                configured,
                in_transaction,
            } => write!(
                f,
                "transaction passphrase mismatch: configured={configured:?}, in_transaction={in_transaction:?}"
            ),
            Self::ContractInvocationFailed {
                operation,
                reason,
                tx_hash,
            } => {
                if let Some(tx_hash) = tx_hash {
                    write!(f, "{operation} failed: {reason} (tx: {tx_hash})")
                } else {
                    write!(f, "{operation} failed: {reason}")
                }
            }
            Self::SimulationFailed { operation, reason } => {
                write!(f, "{operation} simulation failed: {reason}")
            }
            Self::InsufficientFee {
                operation,
                required,
                available,
            } => write!(
                f,
                "{operation} has insufficient fee: required {required}, available {available}"
            ),
            Self::TransactionTimeout {
                operation,
                ledger_submitted,
            } => write!(
                f,
                "{operation} timed out after submission at ledger {ledger_submitted}"
            ),
            Self::SigningFailed { operation, source } => {
                write!(f, "{operation} signing failed: {source}")
            }
        }
    }
}

impl fmt::Display for SigningError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rejected { reason } => write!(f, "signing rejected: {reason}"),
            Self::InvalidKey { reason } => write!(f, "invalid signing key: {reason}"),
            Self::ExternalFailure { reason } => write!(f, "external signer failed: {reason}"),
            Self::MalformedEnvelope { reason } => {
                write!(f, "malformed transaction envelope: {reason}")
            }
        }
    }
}

impl std::error::Error for SdkError {}
impl std::error::Error for SigningError {}

pub fn decode_error(code: u32) -> ContractErrorCode {
    match code {
        1 => ContractErrorCode::NameNotFound,
        2 => ContractErrorCode::NotOwner,
        3 => ContractErrorCode::Expired,
        4 => ContractErrorCode::InvalidLabel,
        _ => ContractErrorCode::Other,
    }
}

/// Returns `true` when an RPC call may be retried with exponential backoff.
pub fn is_retryable(err: &SdkError) -> bool {
    match err {
        SdkError::TransactionTimeout { .. } => true,
        SdkError::Transport(message) => transport_is_retryable(message),
        SdkError::InvalidRequest(_)
        | SdkError::Ingestion(_)
        | SdkError::ContractError(_)
        | SdkError::NetworkPassphraseMismatch { .. }
        | SdkError::TransactionPassphraseMismatch { .. }
        | SdkError::ContractInvocationFailed { .. }
        | SdkError::SimulationFailed { .. }
        | SdkError::InsufficientFee { .. }
        | SdkError::SigningFailed { .. } => false,
    }
}

fn transport_is_retryable(message: &str) -> bool {
    let msg = message.to_ascii_lowercase();

    // Configuration / parsing failures are permanent.
    if msg.contains("invalid rpc url")
        || msg.contains("json decoding error")
        || msg.contains("invalid response from server")
    {
        return false;
    }

    // HTTP rate limiting and server-side transient failures.
    if msg.contains("429")
        || msg.contains("too many requests")
        || msg.contains("rate limit")
        || msg.contains("500")
        || msg.contains("502")
        || msg.contains("503")
        || msg.contains("504")
        || msg.contains("service unavailable")
    {
        return true;
    }

    // Network-level blips.
    for marker in [
        "timeout",
        "timed out",
        "connection refused",
        "connection reset",
        "connection closed",
        "broken pipe",
        "network unreachable",
        "dns error",
        "temporary failure",
        "failed to send",
        "error sending request",
        "error trying to connect",
    ] {
        if msg.contains(marker) {
            return true;
        }
    }

    // Unclassified transport errors default to retryable.
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_retryable_only_for_transient_errors() {
        assert!(is_retryable(&SdkError::Transport("timeout".into())));
        assert!(is_retryable(&SdkError::Transport(
            "http error: status 503 service unavailable".into()
        )));
        assert!(is_retryable(&SdkError::Transport(
            "too many requests (429)".into()
        )));
        assert!(is_retryable(&SdkError::Transport(
            "connection refused".into()
        )));
        assert!(!is_retryable(&SdkError::Transport(
            "invalid rpc url: bad://".into()
        )));
        assert!(is_retryable(&SdkError::TransactionTimeout {
            operation: "register",
            ledger_submitted: 42,
        }));
        assert!(!is_retryable(&SdkError::InvalidRequest("bad input".into())));
        assert!(!is_retryable(&SdkError::ContractError(
            ContractErrorCode::NameNotFound
        )));
        assert!(!is_retryable(&SdkError::NetworkPassphraseMismatch {
            configured: "a".into(),
            rpc_reported: "b".into(),
        }));
        assert!(!is_retryable(&SdkError::SimulationFailed {
            operation: "register",
            reason: "revert".into(),
        }));
        assert!(!is_retryable(&SdkError::ContractInvocationFailed {
            operation: "register",
            reason: "revert".into(),
            tx_hash: None,
        }));
    }
}
