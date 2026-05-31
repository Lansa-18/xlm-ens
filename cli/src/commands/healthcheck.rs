use crate::config::{ContractKind, NetworkConfig};
use crate::output::{emit, OutputFormat};
use serde_json::json;
use xlm_ns_sdk::client::XlmNsClient;
use xlm_ns_sdk::errors::SdkError;

pub async fn run_healthcheck(config: NetworkConfig, output: OutputFormat) -> anyhow::Result<()> {
    let mut checks: Vec<(&'static str, bool, String)> = Vec::new();

    // Configuration loaded — always true when we reach this handler.
    checks.push((
        "config",
        true,
        format!("network={}", config.network.as_str()),
    ));

    // Network passphrase must be non-empty.
    let passphrase_ok = !config.network_passphrase.is_empty();
    checks.push((
        "passphrase",
        passphrase_ok,
        if passphrase_ok {
            config.network_passphrase.clone()
        } else {
            "empty — network passphrase is not set".to_string()
        },
    ));

    // RPC connectivity: trigger an actual get_network() call by resolving a
    // probe name. A placeholder registry ID is used when none is configured;
    // the underlying mock ignores the ID and only the network round-trip matters.
    let probe_registry = config
        .registry_contract_id
        .clone()
        .unwrap_or_else(|| "HEALTHCHECK_PROBE".to_string());

    let probe_client = XlmNsClient::new(
        config.rpc_url.clone(),
        Some(config.network_passphrase.clone()),
        Some(probe_registry),
        config.subdomain_contract_id.clone(),
        config.bridge_contract_id.clone(),
        config.auction_contract_id.clone(),
    );

    let (rpc_ok, rpc_detail) = match probe_client.get_registration("healthcheck-probe.xlm").await {
        Ok(_) => (true, format!("reachable — {}", config.rpc_url)),
        Err(SdkError::Transport(msg)) => (false, format!("unreachable — {msg}")),
        Err(err) => (true, format!("reachable ({})", err)),
    };
    checks.push(("rpc", rpc_ok, rpc_detail));

    let all_ok = checks.iter().all(|(_, ok, _)| *ok);
    let status_label = if all_ok { "OK" } else { "DEGRADED" };

    // Ordered list of contract kinds and their configured values.
    let contract_entries: &[(ContractKind, &Option<String>)] = &[
        (ContractKind::Registry, &config.registry_contract_id),
        (ContractKind::Registrar, &config.registrar_contract_id),
        (ContractKind::Resolver, &config.resolver_contract_id),
        (ContractKind::Auction, &config.auction_contract_id),
        (ContractKind::Bridge, &config.bridge_contract_id),
        (ContractKind::Subdomain, &config.subdomain_contract_id),
        (ContractKind::Nft, &config.nft_contract_id),
    ];

    // Human output
    let mut lines = vec![
        format!(
            "Healthcheck [{status_label}] — network={}",
            config.network.as_str()
        ),
        String::new(),
        "  Checks:".to_string(),
    ];
    for (name, ok, detail) in &checks {
        let mark = if *ok { "PASS" } else { "FAIL" };
        lines.push(format!("    [{mark}]  {name:<12}  {detail}"));
    }
    lines.push(String::new());
    lines.push("  Contracts:".to_string());
    for (kind, id) in contract_entries {
        let val = id.as_deref().unwrap_or("[not configured]");
        lines.push(format!("    {:<22}  {val}", kind.flag_name()));
    }

    // JSON output
    let checks_json: Vec<serde_json::Value> = checks
        .iter()
        .map(|(name, ok, detail)| json!({"name": name, "ok": ok, "detail": detail}))
        .collect();

    let contracts_json: serde_json::Map<String, serde_json::Value> = contract_entries
        .iter()
        .map(|(kind, id)| (kind.flag_name().to_string(), json!(id)))
        .collect();

    emit(
        output,
        &lines.join("\n"),
        json!({
            "status": if all_ok { "ok" } else { "degraded" },
            "network": config.network.as_str(),
            "rpc_url": config.rpc_url,
            "checks": checks_json,
            "contracts": contracts_json,
        }),
    );

    if !all_ok {
        return Err(anyhow::anyhow!("healthcheck reported a degraded status"));
    }
    Ok(())
}
