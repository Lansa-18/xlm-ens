#![allow(dead_code)]
use crate::errors::SdkError;
use stellar_rpc_client::Client;

#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
pub(crate) struct GetNetworkResponse {
    pub passphrase: String,
}

#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
pub(crate) struct RpcResponse<T> {
    pub result: T,
}

/// Calls the RPC getNetwork method and returns the server's
/// reported network passphrase.
///
/// # Errors
/// Returns the transport error type if the HTTP request fails or
/// the response cannot be deserialized.
pub(crate) async fn fetch_network_passphrase(
    _rpc_url: &str,
    http_client: &Client,
) -> Result<String, SdkError> {
    let network = http_client
        .get_network()
        .await
        .map_err(|e| SdkError::Transport(format!("failed to get network: {}", e)))?;

    Ok(network.passphrase)
}

/// Calls getNetwork and asserts the RPC-reported passphrase matches
/// the configured passphrase.
///
/// Call this function at the start of any method that signs or
/// submits a write transaction.
///
/// # Errors
/// - Returns NetworkPassphraseMismatch if the passphrases differ.
/// - Returns a transport or deserialization error if the RPC call
///   fails.
pub async fn verify_network_passphrase(
    configured: &str,
    rpc_url: &str,
    http_client: &Client,
) -> Result<(), SdkError> {
    let rpc_reported = fetch_network_passphrase(rpc_url, http_client).await?;
    if configured != rpc_reported {
        return Err(SdkError::NetworkPassphraseMismatch {
            configured: configured.to_owned(),
            rpc_reported,
        });
    }
    Ok(())
}

/// Checks that the passphrase embedded in a transaction XDR string
/// matches the configured passphrase without making any RPC call.
///
/// `tx_network_passphrase` is the passphrase that the transaction
/// was built for (passed in by the caller from their transaction
/// builder, not extracted from raw XDR - we do not depend on
/// stellar-xdr here to keep the SDK dependency surface minimal).
///
/// # Errors
/// Returns TransactionPassphraseMismatch if the passphrases differ.
pub fn verify_transaction_passphrase(
    configured: &str,
    in_transaction: &str,
) -> Result<(), SdkError> {
    if configured != in_transaction {
        return Err(SdkError::TransactionPassphraseMismatch {
            configured: configured.to_owned(),
            in_transaction: in_transaction.to_owned(),
        });
    }
    Ok(())
}
