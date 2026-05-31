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
