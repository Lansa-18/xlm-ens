use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorConfig {
    pub rpc_url: String,
    pub network_passphrase: String,
    pub poll_interval_secs: u64,
    pub metrics_db_path: String,
    pub contracts: ContractConfig,
    pub thresholds: ThresholdConfig,
    pub alert_channels: AlertChannelConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractConfig {
    pub registry_contract_id: Option<String>,
    pub registrar_contract_id: Option<String>,
    pub resolver_contract_id: Option<String>,
    pub auction_contract_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThresholdConfig {
    pub failure_rate_limit: f64,
    pub storage_capacity_limit: f64,
    pub registration_spike_factor: f64,
    pub zero_registration_hours: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertChannelConfig {
    pub slack_webhook_url: Option<String>,
    pub pagerduty_url: Option<String>,
    pub email_recipient: Option<String>,
    pub generic_webhook_url: Option<String>,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            rpc_url: "https://soroban-testnet.stellar.org".to_string(),
            network_passphrase: "Test SDF Network ; September 2015".to_string(),
            poll_interval_secs: 60,
            metrics_db_path: "xlm-ns-metrics.json".to_string(),
            contracts: ContractConfig {
                registry_contract_id: Some("CDAD...REGISTRY".to_string()),
                registrar_contract_id: Some("CDAD...REGISTRAR".to_string()),
                resolver_contract_id: Some("CDAD...RESOLVER".to_string()),
                auction_contract_id: Some("CDAD...AUCTION".to_string()),
            },
            thresholds: ThresholdConfig {
                failure_rate_limit: 5.0,
                storage_capacity_limit: 80.0,
                registration_spike_factor: 3.0,
                zero_registration_hours: 1.0,
            },
            alert_channels: AlertChannelConfig {
                slack_webhook_url: None,
                pagerduty_url: None,
                email_recipient: None,
                generic_webhook_url: None,
            },
        }
    }
}

impl MonitorConfig {
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let content = fs::read_to_string(path)?;
        let config: MonitorConfig = serde_json::from_str(&content)?;
        Ok(config)
    }
}
