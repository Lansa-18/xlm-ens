use crate::time::{grace_period_ends_at, is_active_at, is_claimable_at, within_grace_period};

pub type NameHash = [u8; 32];

#[cfg(feature = "soroban")]
use soroban_sdk::{contracttype, Address, String as SorobanString};

#[cfg(feature = "soroban")]
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct RegistryEntry {
    pub name: SorobanString,
    pub owner: Address,
    pub resolver: Option<SorobanString>,
    pub target_address: Option<SorobanString>,
    pub metadata_uri: Option<SorobanString>,
    pub ttl_seconds: u64,
    pub registered_at: u64,
    pub expires_at: u64,
    pub grace_period_ends_at: u64,
    pub transfer_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tld {
    Xlm,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameRecord {
    pub label: String,
    pub tld: Tld,
    pub owner: String,
    pub resolver: Option<String>,
    pub target_address: Option<String>,
    pub ttl_seconds: u64,
    pub registered_at: u64,
    pub expires_at: u64,
    pub grace_period_ends_at: u64,
}

impl NameRecord {
    pub fn new(
        label: impl Into<String>,
        owner: impl Into<String>,
        target_address: Option<String>,
        registered_at: u64,
        expires_at: u64,
        grace_period_ends_at: u64,
    ) -> Self {
        Self {
            label: label.into(),
            tld: Tld::Xlm,
            owner: owner.into(),
            resolver: None,
            target_address,
            ttl_seconds: crate::DEFAULT_TTL_SECONDS,
            registered_at,
            expires_at,
            grace_period_ends_at,
        }
    }

    pub fn fqdn(&self) -> String {
        format!("{}.{}", self.label, self.tld.as_str())
    }

    pub fn is_active_at(&self, now_unix: u64) -> bool {
        is_active_at(self.expires_at, now_unix)
    }

    pub fn is_in_grace_period(&self, now_unix: u64) -> bool {
        within_grace_period(self.expires_at, now_unix)
    }

    pub fn is_claimable_at(&self, now_unix: u64) -> bool {
        is_claimable_at(self.grace_period_ends_at, now_unix)
    }

    pub fn set_owner(&mut self, owner: impl Into<String>) {
        self.owner = owner.into();
    }

    pub fn set_resolver(&mut self, resolver: Option<String>) {
        self.resolver = resolver;
    }

    pub fn set_target_address(&mut self, target_address: Option<String>) {
        self.target_address = target_address;
    }

    pub fn extend_expiry(&mut self, expires_at: u64, grace_period_ends_at: u64) {
        self.expires_at = expires_at;
        self.grace_period_ends_at = grace_period_ends_at;
    }

    pub fn next_grace_period_ends_at(expires_at: u64) -> u64 {
        grace_period_ends_at(expires_at)
    }
}

impl Tld {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Xlm => "xlm",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "xlm" => Some(Self::Xlm),
            _ => None,
        }
    }
}
