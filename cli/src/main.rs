mod commands;
mod config;
mod output;
mod signer;

use anyhow::Context;
use clap::{Parser, Subcommand};
use clap_complete::Shell;
use config::{load_config, ContractKind, ContractOverrides, Network, ResolveOptions};
use output::OutputFormat;
use signer::{load_profile, SignerProfile};
use std::path::PathBuf;
use std::process;

const BIN_NAME: &str = "xlm-ns";

#[derive(Parser)]
#[command(name = BIN_NAME)]
#[command(about = "XLM Name Service CLI", long_about = None)]
struct Cli {
    /// Network to use (`testnet` or `mainnet`)
    #[arg(short, long, default_value = "testnet", global = true)]
    network: String,

    /// Config file path. Falls back to `XLM_NS_CONFIG`, then the documented search path.
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    /// Output format for command results.
    #[arg(long, value_enum, default_value_t = OutputFormat::Human, global = true)]
    output: OutputFormat,

    /// Simulate the transaction without submitting it
    #[arg(long, global = true)]
    dry_run: bool,

    /// Override the Soroban RPC URL.
    #[arg(long, global = true)]
    rpc_url: Option<String>,

    /// Override the Soroban network passphrase.
    #[arg(long, global = true)]
    network_passphrase: Option<String>,

    #[arg(long, global = true)]
    registry_contract_id: Option<String>,

    #[arg(long, global = true)]
    registrar_contract_id: Option<String>,

    #[arg(long, global = true)]
    resolver_contract_id: Option<String>,

    #[arg(long, global = true)]
    auction_contract_id: Option<String>,

    #[arg(long, global = true)]
    bridge_contract_id: Option<String>,

    #[arg(long, global = true)]
    subdomain_contract_id: Option<String>,

    #[arg(long, global = true)]
    nft_contract_id: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Register a new name.
    Register {
        /// Name to register
        name: String,
        /// Owner address
        owner: String,
        /// Signer profile to use for submission
        #[arg(long)]
        signer: Option<String>,
    },
    /// Resolve a name to an address.
    Resolve {
        /// Name to resolve
        name: String,
    },
    /// Reverse-resolve an address to its primary name.
    #[command(alias = "reverse-lookup")]
    ReverseResolve {
        /// Address to reverse-resolve
        address: String,
    },
    /// Read or mutate resolver text records.
    #[command(subcommand)]
    Text(TextCommand),
    /// Transfer ownership of a name.
    Transfer {
        /// Name to transfer
        name: String,
        /// New owner address
        new_owner: String,
        /// Signer profile to use for submission
        #[arg(long)]
        signer: Option<String>,
    },
    /// Renew a name registration.
    Renew {
        /// Name to renew
        name: String,
        /// Additional years to renew for
        #[arg(default_value_t = 1)]
        years: u64,
        /// Signer profile to use for submission
        #[arg(long)]
        signer: Option<String>,
    },
    /// Manage auctions for names
    #[command(subcommand)]
    Auction(AuctionCommands),
    /// Generate a shell completion script.
    Completions {
        /// Target shell
        #[arg(value_enum)]
        shell: Shell,
    },
    /// Bridge management commands.
    #[command(subcommand)]
    Bridge(BridgeCommands),
    /// Subdomain management commands
    #[command(subcommand)]
    Subdomain(SubdomainCommands),
    /// Inspect NFT ownership metadata.
    #[command(subcommand)]
    Nft(NftCommands),
    /// Manage configuration files and validation.
    #[command(subcommand)]
    Config(ConfigCommands),
    /// Show registration details for a single name.
    Whois {
        /// Name to inspect
        name: String,
    },
    /// List names owned by an address.
    Portfolio {
        /// Owner address to inspect
        owner: String,
    },
    /// Fetch a registration price quote without submitting a transaction (read-only).
    ///
    /// Use this to inspect the full fee breakdown and lifecycle timestamps before
    /// deciding whether to register a name.
    Quote {
        /// Name label to quote (without the .xlm suffix)
        name: String,
        /// Number of years to quote for
        #[arg(default_value_t = 1)]
        years: u32,
    },
    /// Check whether a name is available for registration (read-only).
    ///
    /// Outputs the availability status: available, active, grace-period, or claimable.
    /// No transaction is submitted.
    Availability {
        /// Name to check (e.g. `alice.xlm` or just `alice`)
        name: String,
    },
    /// Verify RPC connectivity, network passphrase, and configured contract IDs (read-only).
    ///
    /// Exits with a non-zero status when any check fails so the command can be
    /// used in health-probe scripts and CI pipelines.
    Healthcheck,
}

#[derive(Subcommand)]
enum AuctionCommands {
    /// Create a new auction for a name
    Create {
        /// Name to auction
        name: String,
        /// Reserve price in XLM
        #[arg(long, default_value_t = 0)]
        reserve: u64,
        /// Auction duration in seconds
        #[arg(long, default_value_t = 86400)]
        duration: u64,
        /// Signer profile
        #[arg(long)]
        signer: Option<String>,
    },
    /// Place a bid on an active auction
    Bid {
        /// Name under auction
        name: String,
        /// Bid amount in XLM
        amount: u64,
        /// Signer profile
        #[arg(long)]
        signer: Option<String>,
    },
    /// Inspect the state of an auction
    Inspect {
        /// Name to inspect
        name: String,
    },
    /// Settle a completed auction
    Settle {
        /// Name to settle
        name: String,
        /// Signer profile
        #[arg(long)]
        signer: Option<String>,
    },
    /// Export all text records for a name.
    Export {
        /// Name to export text records for
        name: String,
        /// File to write records to (JSON). Prints to stdout if omitted.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Import text records for a name from a file.
    Import {
        /// Name to import text records to
        name: String,
        /// File containing records to import (JSON)
        file: PathBuf,
        /// Signer profile
        #[arg(long)]
        signer: Option<String>,
    },
}

#[derive(Subcommand)]
enum SubdomainCommands {
    /// Register a parent domain for subdomain management
    /// This enables the parent domain owner to create and manage subdomains
    RegisterParent {
        /// Parent domain name (e.g., example.xlm)
        parent: String,
        /// Owner address for the parent domain
        owner: String,
    },
    /// Add a controller to a parent domain
    /// Controllers can create subdomains under the parent domain
    AddController {
        /// Parent domain name
        parent: String,
        /// Controller address to add (must be called by parent owner)
        controller: String,
    },
    /// Create a subdomain under a registered parent
    /// Can be called by parent owner or authorized controllers
    Create {
        /// Subdomain label (e.g., 'sub' for sub.example.xlm)
        label: String,
        /// Parent domain name
        parent: String,
        /// Owner address for the new subdomain
        owner: String,
    },
    /// Transfer ownership of a subdomain
    /// Can only be called by the current subdomain owner
    Transfer {
        /// Full subdomain name (e.g., sub.example.xlm)
        fqdn: String,
        /// New owner address
        new_owner: String,
    },
}

#[derive(Subcommand)]
enum BridgeCommands {
    /// Register a bridge route for a supported chain
    Register {
        /// Chain name (base, ethereum, arbitrum)
        chain: String,
    },
    /// Inspect bridge route for a chain
    Inspect {
        /// Chain name to inspect
        chain: String,
    },
    /// Generate payload for cross-chain resolution
    Payload {
        /// Name to resolve
        name: String,
        /// Target chain
        chain: String,
    },
    /// Publish bridge payload test vectors for EVM resolver consumption
    TestVectors,
}

#[derive(Subcommand)]
enum NftCommands {
    /// Inspect the owner and metadata for a token id.
    Inspect { token_id: String },
}

#[derive(Subcommand)]
enum ConfigCommands {
    /// Create a config file template.
    Init {
        /// Config file path. Defaults to the CLI search path's first entry.
        #[arg(long)]
        path: Option<PathBuf>,
        /// Network profile to render into the template.
        #[arg(long, default_value = "testnet")]
        network: String,
        /// Overwrite an existing file.
        #[arg(long)]
        force: bool,
    },
    /// Open the config file in the user's editor.
    Edit {
        /// Config file path. Defaults to the CLI search path's first entry.
        #[arg(long)]
        path: Option<PathBuf>,
        /// Network profile to render when creating a new file.
        #[arg(long, default_value = "testnet")]
        network: String,
    },
    /// Validate a config file without invoking any contract RPCs.
    Validate {
        /// Config file path. Falls back to the configured search path.
        #[arg(long)]
        path: Option<PathBuf>,
        /// Network to validate against.
        #[arg(long, default_value = "testnet")]
        network: String,
    },
}

#[derive(Subcommand)]
enum TextCommand {
    /// Read a text record value for a name.
    Get { name: String, key: String },
    /// Write a text record value on a name.
    Set {
        name: String,
        key: String,
        value: Option<String>,
        #[arg(long)]
        signer: Option<String>,
    },
}

fn resolve_signer(name: Option<String>) -> anyhow::Result<Option<SignerProfile>> {
    let name = match name {
        Some(n) => n,
        None => return Ok(None),
    };
    load_profile(&name)
        .map(Some)
        .context("failed to load signer profile")
}

async fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();

    if let Commands::Completions { shell } = cli.command {
        commands::completions::run_completions::<Cli>(shell, BIN_NAME);
        return Ok(());
    }

    let network = Network::parse(&cli.network)
        .with_context(|| format!("invalid network '{}'", cli.network))?;

    let contract_overrides = ContractOverrides {
        registry_contract_id: cli.registry_contract_id.clone(),
        registrar_contract_id: cli.registrar_contract_id.clone(),
        resolver_contract_id: cli.resolver_contract_id.clone(),
        auction_contract_id: cli.auction_contract_id.clone(),
        bridge_contract_id: cli.bridge_contract_id.clone(),
        subdomain_contract_id: cli.subdomain_contract_id.clone(),
        nft_contract_id: cli.nft_contract_id.clone(),
    };

    let config = load_config(
        network,
        ResolveOptions {
            config_path: cli.config.clone(),
            rpc_url: cli.rpc_url.clone(),
            network_passphrase: cli.network_passphrase.clone(),
            contract_overrides: contract_overrides.clone(),
        },
    )
    .context("failed to load configuration")?;

    if let Err(err) = validate_contract_policy(&cli.command, &contract_overrides, &config) {
        return Err(anyhow::anyhow!(err));
    }

    match cli.command {
        Commands::Register {
            name,
            owner,
            signer,
        } => commands::register::run_register(config, &name, &owner, resolve_signer(signer)?).await,
        Commands::Resolve { name } => commands::resolve::run_resolve(config, &name).await,
        Commands::ReverseResolve { address } => {
            commands::reverse::run_reverse(config, &address).await
        }
        Commands::Text(sub) => match sub {
            TextCommand::Get { name, key } => commands::text::run_get(config, &name, &key).await,
            TextCommand::Set {
                name,
                key,
                value,
                signer,
            } => commands::text::run_set(config, &name, &key, value, resolve_signer(signer)?).await,
        },
        Commands::Transfer {
            name,
            new_owner,
            signer,
        } => {
            commands::transfer::run_transfer(config, &name, &new_owner, resolve_signer(signer)?)
                .await
        }
        Commands::Renew {
            name,
            years,
            signer,
        } => commands::renew::run_renew(config, &name, years, resolve_signer(signer)?).await,
        Commands::Auction(sub) => match sub {
            AuctionCommands::Create {
                name,
                reserve,
                duration,
                signer,
            } => {
                commands::auction::run_create(
                    config,
                    &name,
                    reserve,
                    duration,
                    resolve_signer(signer)?,
                )
                .await
            }
            AuctionCommands::Bid {
                name,
                amount,
                signer,
            } => commands::auction::run_bid(config, &name, amount, resolve_signer(signer)?).await,
            AuctionCommands::Inspect { name } => {
                commands::auction::run_inspect(config, &name).await
            }
            AuctionCommands::Settle { name, signer } => {
                commands::auction::run_settle(config, &name, resolve_signer(signer)?).await
            }
            AuctionCommands::Export { .. } | AuctionCommands::Import { .. } => {
                Err(anyhow::anyhow!("auction text import/export is not implemented"))
            }
        },
        Commands::Bridge(command) => match command {
            BridgeCommands::Register { chain } => {
                commands::bridge::run_register_chain(config, &chain).await
            }
            BridgeCommands::Inspect { chain } => {
                commands::bridge::run_inspect_route(config, &chain).await
            }
            BridgeCommands::Payload { name, chain } => {
                commands::bridge::run_generate_payload(config, &name, &chain).await
            }
            BridgeCommands::TestVectors => {
                Err(anyhow::anyhow!("bridge test vector export is not implemented"))
            }
        },
        Commands::Subdomain(command) => match command {
            SubdomainCommands::RegisterParent { parent, owner } => {
                commands::subdomain::run_register_parent(config, &parent, &owner).await
            }
            SubdomainCommands::AddController { parent, controller } => {
                commands::subdomain::run_add_controller(config, &parent, &controller).await
            }
            SubdomainCommands::Create {
                label,
                parent,
                owner,
            } => commands::subdomain::run_create_subdomain(config, &label, &parent, &owner).await,
            SubdomainCommands::Transfer { fqdn, new_owner } => {
                commands::subdomain::run_transfer_subdomain(config, &fqdn, &new_owner).await
            }
        },
        Commands::Nft(command) => match command {
            NftCommands::Inspect { token_id } => {
                commands::nft::run_inspect(config, cli.output, &token_id).await
            }
        },
        Commands::Config(command) => match command {
            ConfigCommands::Init {
                path,
                network,
                force,
            } => commands::config::run_init(path.or(cli.config.clone()), &network, force).await,
            ConfigCommands::Edit { path, network } => {
                commands::config::run_edit(path.or(cli.config.clone()), &network).await
            }
            ConfigCommands::Validate { path, network } => {
                commands::config::run_validate(path.or(cli.config.clone()), &network, cli.output)
                    .await
            }
        },
        Commands::Whois { name } => commands::whois::run_whois(config, cli.output, &name).await,
        Commands::Portfolio { owner } => {
            commands::portfolio::run_portfolio(config, cli.output, &owner).await
        }
        Commands::Quote { name, years } => {
            commands::quote::run_quote(config, cli.output, &name, years).await
        }
        Commands::Availability { name } => {
            commands::quote::run_availability(config, cli.output, &name).await
        }
        Commands::Healthcheck => {
            commands::healthcheck::run_healthcheck(config, cli.output).await
        }
        Commands::Completions { .. } => unreachable!("handled above"),
    }
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("Error: {:?}", e);
        process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use config::{Network, NetworkConfig};

    fn config_with_all_contracts() -> NetworkConfig {
        NetworkConfig {
            network: Network::Testnet,
            rpc_url: "https://soroban-testnet.stellar.org".to_string(),
            network_passphrase: "Test SDF Network ; September 2015".to_string(),
            registry_contract_id: Some("REGISTRY111".to_string()),
            registrar_contract_id: Some("REGISTRAR111".to_string()),
            resolver_contract_id: Some("RESOLVER111".to_string()),
            auction_contract_id: Some("AUCTION111".to_string()),
            bridge_contract_id: Some("BRIDGE111".to_string()),
            subdomain_contract_id: Some("SUBDOMAIN111".to_string()),
            nft_contract_id: Some("NFT111".to_string()),
            config_path: None,
        }
    }

    fn config_with_no_contracts() -> NetworkConfig {
        NetworkConfig {
            network: Network::Testnet,
            rpc_url: "https://soroban-testnet.stellar.org".to_string(),
            network_passphrase: "Test SDF Network ; September 2015".to_string(),
            registry_contract_id: None,
            registrar_contract_id: None,
            resolver_contract_id: None,
            auction_contract_id: None,
            bridge_contract_id: None,
            subdomain_contract_id: None,
            nft_contract_id: None,
            config_path: None,
        }
    }

    // --- register ---

    #[test]
    fn register_rejects_irrelevant_resolver_flag() {
        let cmd = Commands::Register {
            name: "test.xlm".to_string(),
            owner: "GDRA111".to_string(),
            signer: None,
        };
        let overrides = ContractOverrides {
            resolver_contract_id: Some("RESOLVER111".to_string()),
            ..Default::default()
        };
        let result = validate_contract_policy(&cmd, &overrides, &config_with_all_contracts());
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(
            msg.contains("resolver-contract-id"),
            "expected resolver-contract-id in: {msg}"
        );
        assert!(msg.contains("register"), "expected 'register' in: {msg}");
    }

    #[test]
    fn register_rejects_irrelevant_registry_flag() {
        let cmd = Commands::Register {
            name: "test.xlm".to_string(),
            owner: "GDRA111".to_string(),
            signer: None,
        };
        let overrides = ContractOverrides {
            registry_contract_id: Some("REGISTRY111".to_string()),
            ..Default::default()
        };
        let result = validate_contract_policy(&cmd, &overrides, &config_with_all_contracts());
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(
            msg.contains("registry-contract-id"),
            "expected registry-contract-id in: {msg}"
        );
    }

    #[test]
    fn register_fails_when_registrar_is_missing() {
        let cmd = Commands::Register {
            name: "test.xlm".to_string(),
            owner: "GDRA111".to_string(),
            signer: None,
        };
        let result =
            validate_contract_policy(&cmd, &ContractOverrides::default(), &config_with_no_contracts());
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(
            msg.contains("registrar-contract-id"),
            "expected registrar-contract-id in: {msg}"
        );
    }

    #[test]
    fn register_passes_with_only_registrar_flag() {
        let cmd = Commands::Register {
            name: "test.xlm".to_string(),
            owner: "GDRA111".to_string(),
            signer: None,
        };
        let overrides = ContractOverrides {
            registrar_contract_id: Some("REGISTRAR111".to_string()),
            ..Default::default()
        };
        let mut cfg = config_with_no_contracts();
        cfg.registrar_contract_id = Some("REGISTRAR111".to_string());
        let result = validate_contract_policy(&cmd, &overrides, &cfg);
        assert!(result.is_ok(), "unexpected error: {:?}", result.err());
    }

    // --- resolve ---

    #[test]
    fn resolve_rejects_irrelevant_registry_flag() {
        let cmd = Commands::Resolve {
            name: "test.xlm".to_string(),
        };
        let overrides = ContractOverrides {
            registry_contract_id: Some("REGISTRY111".to_string()),
            ..Default::default()
        };
        let result = validate_contract_policy(&cmd, &overrides, &config_with_all_contracts());
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(
            msg.contains("registry-contract-id"),
            "expected registry-contract-id in: {msg}"
        );
        assert!(msg.contains("resolve"), "expected 'resolve' in: {msg}");
    }

    #[test]
    fn resolve_fails_when_resolver_is_missing() {
        let cmd = Commands::Resolve {
            name: "test.xlm".to_string(),
        };
        let result =
            validate_contract_policy(&cmd, &ContractOverrides::default(), &config_with_no_contracts());
        assert!(result.is_err());
    }

    // --- transfer ---

    #[test]
    fn transfer_rejects_irrelevant_registrar_flag() {
        let cmd = Commands::Transfer {
            name: "test.xlm".to_string(),
            new_owner: "GDRANEW".to_string(),
            signer: None,
        };
        let overrides = ContractOverrides {
            registrar_contract_id: Some("REGISTRAR111".to_string()),
            ..Default::default()
        };
        let result = validate_contract_policy(&cmd, &overrides, &config_with_all_contracts());
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(
            msg.contains("registrar-contract-id"),
            "expected registrar-contract-id in: {msg}"
        );
        assert!(msg.contains("transfer"), "expected 'transfer' in: {msg}");
    }

    #[test]
    fn transfer_fails_when_registry_is_missing() {
        let cmd = Commands::Transfer {
            name: "test.xlm".to_string(),
            new_owner: "GDRANEW".to_string(),
            signer: None,
        };
        let result =
            validate_contract_policy(&cmd, &ContractOverrides::default(), &config_with_no_contracts());
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(
            msg.contains("registry-contract-id"),
            "expected registry-contract-id in: {msg}"
        );
    }

    // --- quote ---

    #[test]
    fn quote_rejects_irrelevant_registry_flag() {
        let cmd = Commands::Quote {
            name: "test".to_string(),
            years: 1,
        };
        let overrides = ContractOverrides {
            registry_contract_id: Some("REGISTRY111".to_string()),
            ..Default::default()
        };
        let result = validate_contract_policy(&cmd, &overrides, &config_with_all_contracts());
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(
            msg.contains("registry-contract-id"),
            "expected registry-contract-id in: {msg}"
        );
        assert!(msg.contains("quote"), "expected 'quote' in: {msg}");
    }

    // --- availability ---

    #[test]
    fn availability_rejects_irrelevant_registrar_flag() {
        let cmd = Commands::Availability {
            name: "test.xlm".to_string(),
        };
        let overrides = ContractOverrides {
            registrar_contract_id: Some("REGISTRAR111".to_string()),
            ..Default::default()
        };
        let result = validate_contract_policy(&cmd, &overrides, &config_with_all_contracts());
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(
            msg.contains("registrar-contract-id"),
            "expected registrar-contract-id in: {msg}"
        );
    }

    #[test]
    fn availability_passes_with_no_contracts_configured() {
        let cmd = Commands::Availability {
            name: "test.xlm".to_string(),
        };
        let result = validate_contract_policy(
            &cmd,
            &ContractOverrides::default(),
            &config_with_no_contracts(),
        );
        assert!(
            result.is_ok(),
            "availability needs no required contracts: {:?}",
            result.err()
        );
    }

    // --- healthcheck ---

    #[test]
    fn healthcheck_allows_all_contract_flags() {
        let cmd = Commands::Healthcheck;
        let overrides = ContractOverrides {
            registry_contract_id: Some("REGISTRY111".to_string()),
            registrar_contract_id: Some("REGISTRAR111".to_string()),
            resolver_contract_id: Some("RESOLVER111".to_string()),
            auction_contract_id: Some("AUCTION111".to_string()),
            bridge_contract_id: Some("BRIDGE111".to_string()),
            subdomain_contract_id: Some("SUBDOMAIN111".to_string()),
            nft_contract_id: Some("NFT111".to_string()),
        };
        let result = validate_contract_policy(&cmd, &overrides, &config_with_no_contracts());
        assert!(
            result.is_ok(),
            "healthcheck should accept any contract flag: {:?}",
            result.err()
        );
    }

    #[test]
    fn healthcheck_passes_with_no_contracts_configured() {
        let cmd = Commands::Healthcheck;
        let result = validate_contract_policy(
            &cmd,
            &ContractOverrides::default(),
            &config_with_no_contracts(),
        );
        assert!(
            result.is_ok(),
            "healthcheck requires no contracts: {:?}",
            result.err()
        );
    }
}

fn validate_contract_policy(
    command: &Commands,
    overrides: &ContractOverrides,
    config: &config::NetworkConfig,
) -> Result<(), String> {
    let (command_name, allowed, required): (&str, &[ContractKind], &[ContractKind]) = match command
    {
        Commands::Register { .. } => (
            "register",
            &[ContractKind::Registrar],
            &[ContractKind::Registrar],
        ),
        Commands::Resolve { .. } => (
            "resolve",
            &[ContractKind::Resolver],
            &[ContractKind::Resolver],
        ),
        Commands::ReverseResolve { .. } => (
            "reverse-resolve",
            &[ContractKind::Resolver],
            &[ContractKind::Resolver],
        ),
        Commands::Text(_) => ("text", &[ContractKind::Resolver], &[ContractKind::Resolver]),
        Commands::Transfer { .. } => (
            "transfer",
            &[ContractKind::Registry],
            &[ContractKind::Registry],
        ),
        Commands::Renew { .. } => (
            "renew",
            &[ContractKind::Registrar],
            &[ContractKind::Registrar],
        ),
        Commands::Auction(_) => (
            "auction",
            &[ContractKind::Auction],
            &[ContractKind::Auction],
        ),
        Commands::Completions { .. } => ("completions", &[], &[]),
        Commands::Bridge(_) => ("bridge", &[ContractKind::Bridge], &[ContractKind::Bridge]),
        Commands::Subdomain(_) => (
            "subdomain",
            &[ContractKind::Subdomain],
            &[ContractKind::Subdomain],
        ),
        Commands::Nft(_) => ("nft", &[ContractKind::Nft], &[ContractKind::Nft]),
        Commands::Config(_) => ("config", &[], &[]),
        Commands::Whois { .. } => (
            "whois",
            &[ContractKind::Registry, ContractKind::Resolver],
            &[ContractKind::Registry],
        ),
        Commands::Portfolio { .. } => (
            "portfolio",
            &[ContractKind::Registry, ContractKind::Resolver],
            &[ContractKind::Registry],
        ),
        // Quote and Availability are read-only; registrar is needed for pricing.
        Commands::Quote { .. } => (
            "quote",
            &[ContractKind::Registrar],
            &[ContractKind::Registrar],
        ),
        Commands::Availability { .. } => ("availability", &[ContractKind::Registry], &[]),
        // Healthcheck is purely informational: all contract flags are allowed
        // (they are reflected in the output) and none are required.
        Commands::Healthcheck => (
            "healthcheck",
            &[
                ContractKind::Registry,
                ContractKind::Registrar,
                ContractKind::Resolver,
                ContractKind::Auction,
                ContractKind::Bridge,
                ContractKind::Subdomain,
                ContractKind::Nft,
            ],
            &[],
        ),
    };

    for kind in overrides.provided_kinds() {
        if !allowed.contains(&kind) {
            return Err(format!(
                "`--{}` cannot be used with `{command_name}`",
                kind.flag_name()
            ));
        }
    }

    for kind in required {
        if config.contract_id(*kind).is_none() {
            return Err(format!(
                "`{command_name}` requires {}. Set `--{}`, `{}`, or the config file value.",
                kind.display_name(),
                kind.flag_name(),
                kind.env_var()
            ));
        }
    }

    Ok(())
}
