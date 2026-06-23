use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::time::Duration;
use tokio::time::sleep;

mod alerts;
mod config;
mod metrics;

use alerts::{dispatch_alerts, evaluate_thresholds};
use config::MonitorConfig;
use metrics::{collect_metrics, MetricsHistory};

#[derive(Parser)]
#[command(name = "xlm-ns-monitor")]
#[command(about = "XLM Name Service Contract Health Monitor", long_about = None)]
struct Cli {
    /// Config file path
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the background health monitoring daemon
    Run {
        /// Run a single poll cycle and exit
        #[arg(long)]
        dry_run: bool,
    },
    /// Print a real-time status and health summary of the contracts
    Status,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Load configuration
    let config = if let Some(path) = cli.config {
        MonitorConfig::load_from_file(path).unwrap_or_else(|err| {
            eprintln!(
                "Warning: Failed to load config, using default values: {}",
                err
            );
            MonitorConfig::default()
        })
    } else {
        MonitorConfig::default()
    };

    match cli.command {
        Commands::Run { dry_run } => {
            println!("Starting XLM-NS Monitor Daemon...");
            println!("RPC URL: {}", config.rpc_url);
            println!("Poll Interval: {} seconds", config.poll_interval_secs);
            println!("Metrics DB Path: {}", config.metrics_db_path);
            println!("--------------------------------------------------");

            let mut history = MetricsHistory::load_from_file(&config.metrics_db_path);

            loop {
                match collect_metrics(&config, &mut history).await {
                    Ok(snapshot) => {
                        println!(
                            "[{}] Collected metrics snapshot: Total Regs: {}, Latency: {}ms, Balance: {} XLM",
                            snapshot.timestamp,
                            snapshot.total_registrations,
                            snapshot.average_resolution_latency_ms,
                            snapshot.contract_balance_xlm
                        );

                        // Check thresholds & trigger alerts
                        let alerts = evaluate_thresholds(&config, &history, &snapshot).await;
                        dispatch_alerts(&config, &alerts).await;

                        // Save snapshot
                        history.snapshots.push(snapshot);
                        if let Err(e) = history.save_to_file(&config.metrics_db_path) {
                            eprintln!("Failed to save metrics to file: {}", e);
                        }
                    }
                    Err(err) => {
                        eprintln!("Error collecting metrics: {}", err);
                        // Trigger a critical transport alert
                        let transport_alert = alerts::Alert {
                            name: "RPC Connectivity Failure".to_string(),
                            description: format!(
                                "Failed to connect or poll from Soroban RPC: {}",
                                err
                            ),
                            severity: "CRITICAL".to_string(),
                            current_value: "Disconnected".to_string(),
                            threshold: "Unreachable".to_string(),
                        };
                        dispatch_alerts(&config, &[transport_alert]).await;
                    }
                }

                if dry_run {
                    println!("Dry run complete. Exiting.");
                    break;
                }

                sleep(Duration::from_secs(config.poll_interval_secs)).await;
            }
        }
        Commands::Status => {
            let history = MetricsHistory::load_from_file(&config.metrics_db_path);
            println!("==================================================");
            println!("           xlm-ns Contract Health Status          ");
            println!("==================================================");

            if let Some(latest) = history.snapshots.last() {
                println!("Last checked:   {}", latest.timestamp);
                println!("RPC URL:        {}", config.rpc_url);
                println!();
                println!("Metrics Summary:");
                println!(
                    "  - Total Registrations:     {}",
                    latest.total_registrations
                );
                println!(
                    "  - Hourly Reg Volume:       {}",
                    latest.registration_volume_hour
                );
                println!(
                    "  - Daily Reg Volume:        {}",
                    latest.registration_volume_day
                );
                println!(
                    "  - Probe Latency:           {} ms",
                    latest.average_resolution_latency_ms
                );
                println!(
                    "  - Success/Failure Rate:    {:.1}% success",
                    latest.transaction_success_rate
                );
                println!(
                    "  - Storage Entry Count:     {}",
                    latest.storage_entry_count
                );
                println!(
                    "  - Storage Capacity Pct:    {:.1}%",
                    latest.storage_capacity_pct
                );
                println!("  - Active Auctions:         {}", latest.auction_activity);
                println!(
                    "  - Contract Treasury Bal:   {} XLM",
                    latest.contract_balance_xlm
                );
                println!();

                // Compute if healthy
                let alerts = evaluate_thresholds(&config, &history, latest).await;
                if alerts.is_empty() {
                    println!("Overall Health: OK");
                } else {
                    println!("Overall Health: DEGRADED");
                    println!("Active Alerts:");
                    for alert in alerts {
                        println!(
                            "  [{}] {}: {}",
                            alert.severity, alert.name, alert.description
                        );
                    }
                }
            } else {
                println!(
                    "No historical metrics found. Run the monitor daemon first to collect data."
                );
            }
            println!("==================================================");
        }
    }

    Ok(())
}
