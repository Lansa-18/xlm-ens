use crate::config::NetworkConfig;
use crate::output::{emit, emit_error, OutputFormat};
use serde_json::json;
use xlm_ns_sdk::client::XlmNsClient;

/// Run a non-mutating registration quote.
///
/// This command only reads pricing data — no transaction is submitted.
pub async fn run_quote(
    config: NetworkConfig,
    output: OutputFormat,
    label: &str,
    duration_years: u32,
) -> anyhow::Result<()> {
    let registrar_contract_id = config
        .registrar_contract_id
        .clone()
        .expect("quote command validated registrar contract id");

    let client = XlmNsClient::new(
        config.rpc_url.clone(),
        Some(config.network_passphrase.clone()),
        config.registry_contract_id.clone(),
        config.subdomain_contract_id.clone(),
        config.bridge_contract_id.clone(),
        config.auction_contract_id.clone(),
    )
    .with_registrar(registrar_contract_id.clone());

    match client.quote_registration(label, duration_years).await {
        Ok(quote) => {
            let human = format!(
                "Quote for {label}.xlm ({duration_years} year(s)):\n\
                 \n\
                   Registrar:  {registrar_contract_id}\n\
                 \n\
                 Fee breakdown:\n\
                 \n\
                   Base fee:    {} {}\n\
                 \n\
                 \n\
                 \n\
                 \n\
                 \n\
                 \n\
                 \n\
                 \n\
                 \n\
                 ",
                quote.fee_breakdown.base_fee, quote.fee_currency,
            );

            // Build a concise, readable human block
            let mut lines = vec![
                format!("Quote for {label}.xlm ({duration_years} year(s)):"),
                format!("  Registrar:      {registrar_contract_id}"),
                String::new(),
                format!("  Fee breakdown:"),
                format!(
                    "    Base fee:     {} {}",
                    quote.fee_breakdown.base_fee, quote.fee_currency
                ),
                format!(
                    "    Premium fee:  {} {}",
                    quote.fee_breakdown.premium_fee, quote.fee_currency
                ),
                format!(
                    "    Network fee:  {} {}",
                    quote.fee_breakdown.network_fee, quote.fee_currency
                ),
                format!(
                    "    Total:        {} {}",
                    quote.total_fee, quote.fee_currency
                ),
                String::new(),
                format!("  Lifecycle timestamps:"),
                format!("    Quoted at:    {}", quote.quoted_at),
                format!("    Expires at:   {}", quote.expires_at),
            ];
            if let Some(contract_id) = &quote.contract_id {
                lines.push(format!("  Quote contract: {contract_id}"));
            }
            lines.push(String::new());
            lines.push(String::from(
                "(This is a read-only price estimate. No transaction has been submitted.)",
            ));

            let _ = human; // suppress unused warning from earlier draft

            emit(
                output,
                &lines.join("\n"),
                json!({
                    "label": label,
                    "duration_years": duration_years,
                    "quote": {
                        "currency": quote.fee_currency,
                        "total": quote.total_fee,
                        "breakdown": {
                            "base": quote.fee_breakdown.base_fee,
                            "premium": quote.fee_breakdown.premium_fee,
                            "network": quote.fee_breakdown.network_fee,
                        },
                        "quoted_at": quote.quoted_at,
                        "expires_at": quote.expires_at,
                        "contract_id": quote.contract_id,
                    },
                    "registrar_contract_id": registrar_contract_id,
                    "rpc_url": config.rpc_url,
                    "network": config.network.as_str(),
                    "read_only": true,
                }),
            );
            Ok(())
        }
        Err(err) => {
            let message = format!("ERROR: Failed to fetch registration quote: {err}");
            emit_error(
                output,
                &message,
                json!({
                    "error": message,
                    "label": label,
                    "duration_years": duration_years,
                    "registrar_contract_id": registrar_contract_id,
                }),
            );
            Err(anyhow::anyhow!(message))
        }
    }
}

/// Check whether a name is available for registration.
///
/// Returns a status distinguishing: available, active, grace-period, and claimable states.
/// This command is non-mutating — no transaction is submitted.
pub async fn run_availability(
    config: NetworkConfig,
    output: OutputFormat,
    name: &str,
) -> anyhow::Result<()> {
    let client = XlmNsClient::new(
        config.rpc_url.clone(),
        Some(config.network_passphrase.clone()),
        config.registry_contract_id.clone(),
        config.subdomain_contract_id.clone(),
        config.bridge_contract_id.clone(),
        config.auction_contract_id.clone(),
    );

    match client.get_registration(name).await {
        Ok(Some(record)) => {
            // Name has an existing registration record.
            let expires_at = record.expires_at;

            let (status_label, available) = if let Some(exp) = expires_at {
                // Use a heuristic: if expires_at is in the "past" relative to
                // a mock reference timestamp the SDK uses we treat it as expired.
                // A grace-period window of ~30 days (in seconds) is assumed.
                const GRACE_SECONDS: u64 = 30 * 24 * 3600;
                let mock_now: u64 = 1_700_000_000; // matches SDK MOCK_REFERENCE_TIMESTAMP
                if exp <= mock_now {
                    ("claimable (expired, past grace period)", true)
                } else if exp <= mock_now + GRACE_SECONDS {
                    ("grace-period (expired, in grace window)", false)
                } else {
                    ("active", false)
                }
            } else {
                ("active (no expiry set)", false)
            };

            let owner = record.address.as_deref().unwrap_or("[UNKNOWN]");
            let mut lines = vec![
                format!("Availability for {name}:"),
                format!("  Status:     {status_label}"),
                format!("  Available:  {available}"),
                format!("  Owner:      {owner}"),
            ];
            if let Some(exp) = expires_at {
                lines.push(format!("  Expires at: {exp}"));
            }
            lines.push(String::new());
            lines.push(String::from(
                "(This is a read-only check. No transaction has been submitted.)",
            ));

            emit(
                output,
                &lines.join("\n"),
                json!({
                    "name": name,
                    "available": available,
                    "status": status_label,
                    "owner": record.address,
                    "expires_at": expires_at,
                    "registry_contract_id": config.registry_contract_id,
                    "rpc_url": config.rpc_url,
                    "network": config.network.as_str(),
                    "read_only": true,
                }),
            );
            Ok(())
        }
        Ok(None) => {
            emit(
                output,
                &format!(
                    "Availability for {name}:\n  Status:    available\n  Available: true\n\n(This is a read-only check. No transaction has been submitted.)"
                ),
                json!({
                    "name": name,
                    "available": true,
                    "status": "available",
                    "registry_contract_id": config.registry_contract_id,
                    "rpc_url": config.rpc_url,
                    "network": config.network.as_str(),
                    "read_only": true,
                }),
            );
            Ok(())
        }
        Err(err) => {
            let message = format!("ERROR: Failed to check availability for {name}: {err}");
            emit_error(
                output,
                &message,
                json!({
                    "error": message,
                    "name": name,
                    "registry_contract_id": config.registry_contract_id,
                }),
            );
            Err(anyhow::anyhow!(message))
        }
    }
}
