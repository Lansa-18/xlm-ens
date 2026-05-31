use serde::Deserialize;
use std::collections::BTreeMap;
use std::env;
use std::fmt;
use std::fs;
use std::path::PathBuf;
use xlm_ns_common::validation::validate_contract_id;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Network {
    Testnet,
    Mainnet,
}

impl Network {
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "testnet" => Some(Self::Testnet),
            "mainnet" => Some(Self::Mainnet),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Testnet => "testnet",
            Self::Mainnet => "mainnet",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractKind {
    Registry,
    Registrar,
    Resolver,
    Auction,
    Bridge,
    Subdomain,
    Nft,
}

impl ContractKind {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Registry => "registry contract",
            Self::Registrar => "registrar contract",
            Self::Resolver => "resolver contract",
            Self::Auction => "auction contract",
            Self::Bridge => "bridge contract",
            Self::Subdomain => "subdomain contract",
            Self::Nft => "nft contract",
        }
    }

    pub fn env_var(&self) -> &'static str {
        match self {
            Self::Registry => "REGISTRY_CONTRACT_ID",
            Self::Registrar => "REGISTRAR_CONTRACT_ID",
            Self::Resolver => "RESOLVER_CONTRACT_ID",
            Self::Auction => "AUCTION_CONTRACT_ID",
            Self::Bridge => "BRIDGE_CONTRACT_ID",
            Self::Subdomain => "SUBDOMAIN_CONTRACT_ID",
            Self::Nft => "NFT_CONTRACT_ID",
        }
    }

    pub fn flag_name(&self) -> &'static str {
        match self {
            Self::Registry => "registry-contract-id",
            Self::Registrar => "registrar-contract-id",
            Self::Resolver => "resolver-contract-id",
            Self::Auction => "auction-contract-id",
            Self::Bridge => "bridge-contract-id",
            Self::Subdomain => "subdomain-contract-id",
            Self::Nft => "nft-contract-id",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ContractOverrides {
    pub registry_contract_id: Option<String>,
    pub registrar_contract_id: Option<String>,
    pub resolver_contract_id: Option<String>,
    pub auction_contract_id: Option<String>,
    pub bridge_contract_id: Option<String>,
    pub subdomain_contract_id: Option<String>,
    pub nft_contract_id: Option<String>,
}

impl ContractOverrides {
    pub fn get(&self, kind: ContractKind) -> Option<&str> {
        match kind {
            ContractKind::Registry => self.registry_contract_id.as_deref(),
            ContractKind::Registrar => self.registrar_contract_id.as_deref(),
            ContractKind::Resolver => self.resolver_contract_id.as_deref(),
            ContractKind::Auction => self.auction_contract_id.as_deref(),
            ContractKind::Bridge => self.bridge_contract_id.as_deref(),
            ContractKind::Subdomain => self.subdomain_contract_id.as_deref(),
            ContractKind::Nft => self.nft_contract_id.as_deref(),
        }
    }

    pub fn provided_kinds(&self) -> Vec<ContractKind> {
        let mut kinds = Vec::new();
        for kind in [
            ContractKind::Registry,
            ContractKind::Registrar,
            ContractKind::Resolver,
            ContractKind::Auction,
            ContractKind::Bridge,
            ContractKind::Subdomain,
            ContractKind::Nft,
        ] {
            if self.get(kind).is_some() {
                kinds.push(kind);
            }
        }
        kinds
    }
}

#[derive(Debug, Clone)]
pub struct NetworkConfig {
    pub network: Network,
    pub rpc_url: String,
    pub network_passphrase: String,
    pub registry_contract_id: Option<String>,
    pub registrar_contract_id: Option<String>,
    pub resolver_contract_id: Option<String>,
    pub auction_contract_id: Option<String>,
    pub bridge_contract_id: Option<String>,
    pub subdomain_contract_id: Option<String>,
    pub nft_contract_id: Option<String>,
    pub config_path: Option<PathBuf>,
}

impl NetworkConfig {
    pub fn contract_id(&self, kind: ContractKind) -> Option<&str> {
        match kind {
            ContractKind::Registry => self.registry_contract_id.as_deref(),
            ContractKind::Registrar => self.registrar_contract_id.as_deref(),
            ContractKind::Resolver => self.resolver_contract_id.as_deref(),
            ContractKind::Auction => self.auction_contract_id.as_deref(),
            ContractKind::Bridge => self.bridge_contract_id.as_deref(),
            ContractKind::Subdomain => self.subdomain_contract_id.as_deref(),
            ContractKind::Nft => self.nft_contract_id.as_deref(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ResolveOptions {
    pub config_path: Option<PathBuf>,
    pub rpc_url: Option<String>,
    pub network_passphrase: Option<String>,
    pub contract_overrides: ContractOverrides,
}

#[derive(Debug)]
pub enum ConfigError {
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    Validation {
        path: Option<PathBuf>,
        messages: Vec<String>,
    },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => {
                write!(f, "failed to read config file {}: {source}", path.display())
            }
            Self::Parse { path, source } => {
                write!(
                    f,
                    "failed to parse config file {}: {source}",
                    path.display()
                )
            }
            Self::Validation { path, messages } => {
                if let Some(path) = path {
                    write!(f, "invalid config {}: {}", path.display(), messages.join("; "))
                } else {
                    write!(f, "invalid config: {}", messages.join("; "))
                }
            }
        }
    }
}

impl std::error::Error for ConfigError {}

pub fn load_config(
    network: Network,
    options: ResolveOptions,
) -> Result<NetworkConfig, ConfigError> {
    let file = load_config_file(options.config_path.clone())?;
    let file_values = file
        .as_ref()
        .map(|(_, config)| config.network(network))
        .unwrap_or_default();
    let env_values = PartialNetworkConfig::from_env();

    let config = NetworkConfig {
        network,
        rpc_url: options
            .rpc_url
            .or(env_values.rpc_url)
            .or(file_values.rpc_url)
            .unwrap_or_else(|| default_rpc_url(network).to_string()),
        network_passphrase: options
            .network_passphrase
            .or(env_values.network_passphrase)
            .or(file_values.network_passphrase)
            .unwrap_or_else(|| default_network_passphrase(network).to_string()),
        registry_contract_id: options
            .contract_overrides
            .registry_contract_id
            .or(env_values.registry_contract_id)
            .or(file_values.registry_contract_id),
        registrar_contract_id: options
            .contract_overrides
            .registrar_contract_id
            .or(env_values.registrar_contract_id)
            .or(file_values.registrar_contract_id),
        resolver_contract_id: options
            .contract_overrides
            .resolver_contract_id
            .or(env_values.resolver_contract_id)
            .or(file_values.resolver_contract_id),
        auction_contract_id: options
            .contract_overrides
            .auction_contract_id
            .or(env_values.auction_contract_id)
            .or(file_values.auction_contract_id),
        bridge_contract_id: options
            .contract_overrides
            .bridge_contract_id
            .or(env_values.bridge_contract_id)
            .or(file_values.bridge_contract_id),
        subdomain_contract_id: options
            .contract_overrides
            .subdomain_contract_id
            .or(env_values.subdomain_contract_id)
            .or(file_values.subdomain_contract_id),
        nft_contract_id: options
            .contract_overrides
            .nft_contract_id
            .or(env_values.nft_contract_id)
            .or(file_values.nft_contract_id),
        config_path: file.map(|(path, _)| path),
    };

    validate_network_config(&config)?;
    Ok(config)
}

pub fn validate_network_config(config: &NetworkConfig) -> Result<(), ConfigError> {
    let mut messages = Vec::new();

    if config.rpc_url.trim().is_empty() {
        messages.push("rpc_url must not be empty".to_string());
    }
    if config.network_passphrase.trim().is_empty() {
        messages.push("network_passphrase must not be empty".to_string());
    }

    for (kind, id) in [
        (ContractKind::Registry, &config.registry_contract_id),
        (ContractKind::Registrar, &config.registrar_contract_id),
        (ContractKind::Resolver, &config.resolver_contract_id),
        (ContractKind::Auction, &config.auction_contract_id),
        (ContractKind::Bridge, &config.bridge_contract_id),
        (ContractKind::Subdomain, &config.subdomain_contract_id),
        (ContractKind::Nft, &config.nft_contract_id),
    ] {
        if let Some(contract_id) = id.as_deref() {
            if let Err(err) = validate_contract_id(contract_id) {
                messages.push(format!("{}: {err}", kind.flag_name()));
            }
        }
    }

    if messages.is_empty() {
        Ok(())
    } else {
        Err(ConfigError::Validation {
            path: config.config_path.clone(),
            messages,
        })
    }
}

pub fn config_template(network: Network) -> String {
    let (rpc_url, passphrase) = match network {
        Network::Testnet => (
            "https://soroban-testnet.stellar.org",
            "Test SDF Network ; September 2015",
        ),
        Network::Mainnet => (
            "https://mainnet.stellar.org:443",
            "Public Global Stellar Network ; October 2015",
        ),
    };

    format!(
        r#"# xlm-ns configuration file
#
# Values in [default] apply to every network unless a per-network section
# overrides them.

[default]
rpc_url = "{rpc_url}"
network_passphrase = "{passphrase}"
registry_contract_id = "C................................................................"
registrar_contract_id = "C................................................................"
resolver_contract_id = "C................................................................"
auction_contract_id = "C................................................................"
bridge_contract_id = "C................................................................"
subdomain_contract_id = "C................................................................"
nft_contract_id = "C................................................................"

[networks.{network}]
"#,
        network = network.as_str(),
    )
}


#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct FileConfig {
    default: PartialNetworkConfig,
    networks: BTreeMap<String, PartialNetworkConfig>,
}

impl FileConfig {
    fn network(&self, network: Network) -> PartialNetworkConfig {
        let mut merged = self.default.clone();
        if let Some(overlay) = self.networks.get(network.as_str()) {
            merged.apply(overlay.clone());
        }
        merged
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct PartialNetworkConfig {
    rpc_url: Option<String>,
    network_passphrase: Option<String>,
    registry_contract_id: Option<String>,
    registrar_contract_id: Option<String>,
    resolver_contract_id: Option<String>,
    auction_contract_id: Option<String>,
    bridge_contract_id: Option<String>,
    subdomain_contract_id: Option<String>,
    nft_contract_id: Option<String>,
}

impl PartialNetworkConfig {
    fn from_env() -> Self {
        Self {
            rpc_url: env::var("SOROBAN_RPC_URL").ok(),
            network_passphrase: env::var("SOROBAN_NETWORK_PASSPHRASE").ok(),
            registry_contract_id: env::var("REGISTRY_CONTRACT_ID").ok(),
            registrar_contract_id: env::var("REGISTRAR_CONTRACT_ID").ok(),
            resolver_contract_id: env::var("RESOLVER_CONTRACT_ID").ok(),
            auction_contract_id: env::var("AUCTION_CONTRACT_ID").ok(),
            bridge_contract_id: env::var("BRIDGE_CONTRACT_ID").ok(),
            subdomain_contract_id: env::var("SUBDOMAIN_CONTRACT_ID").ok(),
            nft_contract_id: env::var("NFT_CONTRACT_ID").ok(),
        }
    }

    fn apply(&mut self, other: Self) {
        if other.rpc_url.is_some() {
            self.rpc_url = other.rpc_url;
        }
        if other.network_passphrase.is_some() {
            self.network_passphrase = other.network_passphrase;
        }
        if other.registry_contract_id.is_some() {
            self.registry_contract_id = other.registry_contract_id;
        }
        if other.registrar_contract_id.is_some() {
            self.registrar_contract_id = other.registrar_contract_id;
        }
        if other.resolver_contract_id.is_some() {
            self.resolver_contract_id = other.resolver_contract_id;
        }
        if other.auction_contract_id.is_some() {
            self.auction_contract_id = other.auction_contract_id;
        }
        if other.bridge_contract_id.is_some() {
            self.bridge_contract_id = other.bridge_contract_id;
        }
        if other.subdomain_contract_id.is_some() {
            self.subdomain_contract_id = other.subdomain_contract_id;
        }
        if other.nft_contract_id.is_some() {
            self.nft_contract_id = other.nft_contract_id;
        }
    }
}

fn load_config_file(
    explicit_path: Option<PathBuf>,
) -> Result<Option<(PathBuf, FileConfig)>, ConfigError> {
    let requested = explicit_path.or_else(config_path_from_env);

    if let Some(path) = requested {
        return read_config_file(path).map(Some);
    }

    for candidate in config_search_paths() {
        if candidate.exists() {
            return read_config_file(candidate).map(Some);
        }
    }

    Ok(None)
}

fn read_config_file(path: PathBuf) -> Result<(PathBuf, FileConfig), ConfigError> {
    let raw = fs::read_to_string(&path).map_err(|source| ConfigError::Read {
        path: path.clone(),
        source,
    })?;
    let parsed = toml::from_str::<FileConfig>(&raw).map_err(|source| ConfigError::Parse {
        path: path.clone(),
        source,
    })?;
    Ok((path, parsed))
}

fn config_path_from_env() -> Option<PathBuf> {
    env::var("XLM_NS_CONFIG").ok().map(PathBuf::from)
}

fn config_search_paths() -> Vec<PathBuf> {
    let mut paths = vec![PathBuf::from(".xlm-ns.toml"), PathBuf::from("xlm-ns.toml")];

    if let Some(xdg_config_home) = env::var_os("XDG_CONFIG_HOME") {
        paths.push(
            PathBuf::from(xdg_config_home)
                .join("xlm-ns")
                .join("config.toml"),
        );
    }

    if let Some(home) = env::var_os("HOME") {
        paths.push(
            PathBuf::from(home)
                .join(".config")
                .join("xlm-ns")
                .join("config.toml"),
        );
    }

    paths
}

fn default_rpc_url(network: Network) -> &'static str {
    match network {
        Network::Testnet => "https://soroban-testnet.stellar.org",
        Network::Mainnet => "https://mainnet.stellar.org:443",
    }
}

fn default_network_passphrase(network: Network) -> &'static str {
    match network {
        Network::Testnet => "Test SDF Network ; September 2015",
        Network::Mainnet => "Public Global Stellar Network ; October 2015",
    }
}
