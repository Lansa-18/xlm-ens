use crate::config::MonitorConfig;
use crate::metrics::{MetricSnapshot, MetricsHistory};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub name: String,
    pub description: String,
    pub severity: String,
    pub current_value: String,
    pub threshold: String,
}

pub async fn evaluate_thresholds(
    config: &MonitorConfig,
    history: &MetricsHistory,
    latest: &MetricSnapshot,
) -> Vec<Alert> {
    let mut alerts = Vec::new();

    // 1. Check Failure Rate > 5% (Success rate < 95%)
    let failure_rate = 100.0 - latest.transaction_success_rate;
    if failure_rate > config.thresholds.failure_rate_limit {
        alerts.push(Alert {
            name: "Elevated Transaction Failure Rate".to_string(),
            description: format!(
                "Transaction failure rate is {:.2}%, exceeding the limit of {:.2}%.",
                failure_rate, config.thresholds.failure_rate_limit
            ),
            severity: "CRITICAL".to_string(),
            current_value: format!("{:.2}%", failure_rate),
            threshold: format!("> {:.2}%", config.thresholds.failure_rate_limit),
        });
    }

    // 2. Check Storage > 80% Capacity
    if latest.storage_capacity_pct > config.thresholds.storage_capacity_limit {
        alerts.push(Alert {
            name: "High Storage Utilization".to_string(),
            description: format!(
                "Contract storage is at {:.2}% of capacity (Limit: {:.2}%).",
                latest.storage_capacity_pct, config.thresholds.storage_capacity_limit
            ),
            severity: "WARNING".to_string(),
            current_value: format!("{:.2}%", latest.storage_capacity_pct),
            threshold: format!("> {:.2}%", config.thresholds.storage_capacity_limit),
        });
    }

    // 3. Check Registration Volume Spike > 3x Baseline
    // Compute baseline as the average registration_volume_hour over the last 10 snapshots (minimum 1)
    let baseline: f64 = if history.snapshots.len() >= 3 {
        let count = history.snapshots.len().min(10);
        let sum: u64 = history
            .snapshots
            .iter()
            .rev()
            .take(count)
            .map(|s| s.registration_volume_hour)
            .sum();
        sum as f64 / count as f64
    } else {
        2.0 // Default baseline
    };

    let spike_factor = latest.registration_volume_hour as f64 / baseline.max(1.0);
    if spike_factor > config.thresholds.registration_spike_factor {
        alerts.push(Alert {
            name: "Registration Volume Spike Detected".to_string(),
            description: format!("Registration volume spike is {:.2}x baseline ({:.2} regs/hour vs baseline {:.2} regs/hour).", spike_factor, latest.registration_volume_hour, baseline),
            severity: "WARNING".to_string(),
            current_value: format!("{:.2}x", spike_factor),
            threshold: format!("> {:.2}x", config.thresholds.registration_spike_factor),
        });
    }

    // 4. Check Zero Registrations for > 1 hour
    // In our simulated setup, if we have snapshots spanning 1+ hours and no registrations grew.
    if latest.registration_volume_hour == 0 {
        alerts.push(Alert {
            name: "Zero Registration Activity".to_string(),
            description: "No new registrations have occurred in the last hour.".to_string(),
            severity: "WARNING".to_string(),
            current_value: "0 registrations".to_string(),
            threshold: "0 registrations for > 1 hour".to_string(),
        });
    }

    alerts
}

pub async fn dispatch_alerts(config: &MonitorConfig, alerts: &[Alert]) {
    if alerts.is_empty() {
        return;
    }

    let client = Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap_or_default();

    for alert in alerts {
        println!(
            "[ALERT] [{}] {}: {}",
            alert.severity, alert.name, alert.description
        );

        // Slack Notification
        if let Some(ref url) = config.alert_channels.slack_webhook_url {
            let payload = serde_json::json!({
                "text": format!("*[{}]* *{}*\n{}", alert.severity, alert.name, alert.description)
            });
            let _ = client.post(url).json(&payload).send().await;
        }

        // PagerDuty Notification
        if let Some(ref url) = config.alert_channels.pagerduty_url {
            let payload = serde_json::json!({
                "event_action": "trigger",
                "payload": {
                    "summary": format!("{}: {}", alert.name, alert.description),
                    "severity": alert.severity.to_lowercase(),
                    "source": "xlm-ns-monitor"
                }
            });
            let _ = client.post(url).json(&payload).send().await;
        }

        // Generic Webhook
        if let Some(ref url) = config.alert_channels.generic_webhook_url {
            let _ = client.post(url).json(alert).send().await;
        }

        // Email Alert Simulation
        if let Some(ref recipient) = config.alert_channels.email_recipient {
            println!(
                "[EMAIL SENT to {}] Subject: [{}] {}\n\n{}",
                recipient, alert.severity, alert.name, alert.description
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_no_alerts_when_healthy() {
        let config = MonitorConfig::default();
        let history = MetricsHistory::default();
        let latest = MetricSnapshot {
            timestamp: "2026-06-23T00:00:00Z".to_string(),
            total_registrations: 100,
            registration_volume_hour: 2,
            registration_volume_day: 15,
            transaction_success_rate: 100.0,
            storage_entry_count: 1000,
            storage_capacity_pct: 10.0,
            average_resolution_latency_ms: 50,
            auction_activity: 1,
            contract_balance_xlm: 1000,
        };

        let alerts = evaluate_thresholds(&config, &history, &latest).await;
        // Zero registration warning is expected if hourly volume is 0, but here it is 2.
        assert!(
            alerts.is_empty(),
            "Healthy snapshot should not trigger alerts, got: {:?}",
            alerts
        );
    }

    #[tokio::test]
    async fn test_alerts_triggered_when_unhealthy() {
        let config = MonitorConfig::default();
        let history = MetricsHistory::default();
        let latest = MetricSnapshot {
            timestamp: "2026-06-23T00:00:00Z".to_string(),
            total_registrations: 100,
            registration_volume_hour: 0, // Should trigger zero regs warning
            registration_volume_day: 15,
            transaction_success_rate: 90.0, // Should trigger failure rate > 5% (Success < 95%)
            storage_entry_count: 9000,
            storage_capacity_pct: 90.0, // Should trigger storage capacity > 80%
            average_resolution_latency_ms: 500,
            auction_activity: 1,
            contract_balance_xlm: 1000,
        };

        let alerts = evaluate_thresholds(&config, &history, &latest).await;
        assert_eq!(alerts.len(), 3);
        assert!(alerts
            .iter()
            .any(|a| a.name.contains("Transaction Failure Rate")));
        assert!(alerts
            .iter()
            .any(|a| a.name.contains("High Storage Utilization")));
        assert!(alerts
            .iter()
            .any(|a| a.name.contains("Zero Registration Activity")));
    }
}
