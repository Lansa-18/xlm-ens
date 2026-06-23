pub mod expiry;
pub mod pricing;
mod test;

use expiry::expiry_from_now;
use pricing::price_for_label_length;
use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, symbol_short, Address,
    Bytes, Env, IntoVal, String, Symbol, Vec,
};
use xlm_ns_common::soroban::{
    build_xlm_name, extract_label_soroban, validate_label_soroban,
    validate_registration_years_soroban,
};
use xlm_ns_common::time::grace_period_ends_at;
pub use xlm_ns_common::GRACE_PERIOD_SECONDS;

pub const ADMIN_RECOVERY_SUPPORTED: bool = false;
pub const CONTRACT_VERSION: u32 = 1;

#[contractevent]
#[contracttype]
pub struct ContractUpgraded {
    pub old_version: u32,
    pub new_version: u32,
    pub admin: Address,
}

// Default rate limit: 5 registrations per 24 hours (86400 seconds)
pub const DEFAULT_RATE_LIMIT_WINDOW_SECONDS: u64 = 86400;
pub const DEFAULT_MAX_REGISTRATIONS_PER_WINDOW: u64 = 5;

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct PricingBreakdown {
    pub annual_fee_stroops: u64,
    pub duration_years: u64,
    pub premium_stroops: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct RegistrationQuote {
    pub fee_stroops: u64,
    pub expiry_unix: u64,
    pub grace_period_ends_at: u64,
    pub pricing: PricingBreakdown,
}

/// Issue #220: Renewal-specific quote. Unlike [`RegistrationQuote`] this is for
/// an already-registered name and reports both the current and the post-renewal
/// expiry so callers can see exactly how a renewal extends the lifecycle.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct RenewalQuote {
    pub fee_stroops: u64,
    pub current_expiry_unix: u64,
    pub extended_expiry_unix: u64,
    pub grace_period_ends_at: u64,
    pub pricing: PricingBreakdown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct RegistrarMetrics {
    pub treasury_balance: u64,
    pub total_registrations: u64,
    pub total_renewals: u64,
}

/// Issue #311: Lifecycle status for a name from the registrar's perspective.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum RegistrationStatus {
    /// Never registered or already re-claimable (past grace period).
    Unavailable,
    /// Actively registered and not yet expired.
    Active,
    /// Expired but still within the grace period — only the current owner may renew.
    GracePeriod,
    /// Past the grace period; anyone may register the name.
    Claimable,
    /// Blocked by the reserved-label list; cannot be registered at all.
    Reserved,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct RegistrationRecord {
    pub name: String,
    pub owner: Address,
    pub registered_at: u64,
    pub expires_at: u64,
    pub grace_period_ends_at: u64,
    pub fee_paid: u64,
    pub renewed_at: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct RateLimitConfig {
    pub window_size_seconds: u64,
    pub max_registrations_per_window: u64,
}

#[derive(Clone)]
#[contracttype]
enum DataKey {
    Registration(String),
    Reserved(String),
    Treasury,
    Registry,
    RegistrationCount,
    RenewalCount,
    RateLimitConfig,
    WhitelistedAddress(Address),
    RegistrationWindow(Address, u64),
    Admin,
    ContractVersion,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum RegistrarError {
    InsufficientFee = 1,
    NotFound = 2,
    NotRenewable = 3,
    AlreadyRegistered = 4,
    Reserved = 5,
    Unauthorized = 6,
    Validation = 7,
    RegistrationClaimable = 8,
    NotInitialized = 9,
    AlreadyInitialized = 10,
    RateLimitExceeded = 11,
    UpgradeFailed = 12,
}

#[contract]
pub struct RegistrarContract;

#[contractimpl]
impl RegistrarContract {
    pub fn initialize(env: Env, registry: Address, admin: Address) -> Result<(), RegistrarError> {
        if env.storage().instance().has(&DataKey::Registry) {
            return Err(RegistrarError::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Registry, &registry);
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .persistent()
            .set(&DataKey::ContractVersion, &CONTRACT_VERSION);

        // Initialize rate limit config with defaults if not already set
        if !env.storage().persistent().has(&DataKey::RateLimitConfig) {
            let config = RateLimitConfig {
                window_size_seconds: DEFAULT_RATE_LIMIT_WINDOW_SECONDS,
                max_registrations_per_window: DEFAULT_MAX_REGISTRATIONS_PER_WINDOW,
            };
            env.storage()
                .persistent()
                .set(&DataKey::RateLimitConfig, &config);
        }
        Ok(())
    }

    pub fn version(_env: Env) -> u32 {
        CONTRACT_VERSION
    }

    pub fn get_version(env: Env) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::ContractVersion)
            .unwrap_or(CONTRACT_VERSION)
    }

    pub fn upgrade(
        env: Env,
        new_wasm_hash: BytesN<32>,
        migration_data: Bytes,
    ) -> Result<(), RegistrarError> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(RegistrarError::UpgradeFailed)?;
        admin.require_auth();

        let current_version = Self::get_version(env.clone());
        let target_version = decode_target_version(&migration_data);

        for v in current_version..target_version {
            migrate(v, v + 1, &migration_data);
        }

        env.storage()
            .persistent()
            .set(&DataKey::ContractVersion, &target_version);

        env.events().publish(
            (symbol_short!("registrar"), symbol_short!("upgraded")),
            ContractUpgraded {
                old_version: current_version,
                new_version: target_version,
                admin,
            },
        );

        env.deployer().update_current_contract_wasm(new_wasm_hash.to_bytes());

        Ok(())
    }

    // Release policy: registrations are only released through the normal
    // expiry-plus-grace lifecycle. This contract does not expose an admin
    // recovery or forced-release override.
    pub fn reserve_label(env: Env, label: String) -> Result<(), RegistrarError> {
        validate_label_soroban(&label).map_err(|_| RegistrarError::Validation)?;
        let key = DataKey::Reserved(label.clone());
        if env
            .storage()
            .persistent()
            .get::<_, bool>(&key)
            .unwrap_or(false)
        {
            env.events()
                .publish((symbol_short!("reserved"), symbol_short!("skipped")), label);
        } else {
            env.storage().persistent().set(&key, &true);
            env.events()
                .publish((symbol_short!("reserved"), symbol_short!("added")), label);
        }
        Ok(())
    }

    pub fn load_reserved_manifest(env: Env, labels: Vec<String>) -> Result<u32, RegistrarError> {
        let mut added_count = 0;
        for label in labels.iter() {
            if validate_label_soroban(&label).is_ok() {
                let key = DataKey::Reserved(label.clone());
                if env
                    .storage()
                    .persistent()
                    .get::<_, bool>(&key)
                    .unwrap_or(false)
                {
                    env.events().publish(
                        (symbol_short!("reserved"), symbol_short!("skipped")),
                        label.clone(),
                    );
                } else {
                    env.storage().persistent().set(&key, &true);
                    env.events().publish(
                        (symbol_short!("reserved"), symbol_short!("added")),
                        label.clone(),
                    );
                    added_count += 1;
                }
            } else {
                env.events().publish(
                    (symbol_short!("reserved"), symbol_short!("skipped")),
                    label.clone(),
                );
            }
        }
        Ok(added_count)
    }

    pub fn quote_registration(
        _env: Env,
        label: String,
        years: u64,
        now_unix: u64,
    ) -> Result<RegistrationQuote, RegistrarError> {
        validate_label_soroban(&label).map_err(|_| RegistrarError::Validation)?;
        validate_registration_years_soroban(years).map_err(|_| RegistrarError::Validation)?;
        Ok(build_quote(&label, years, now_unix))
    }

    /// Issue #220: Read-only renewal quote for an existing registration.
    ///
    /// Mirrors the fee and expiry math in [`renew`] (renewing from the later of
    /// the current expiry or `now`) without mutating state or requiring auth.
    /// Returns [`RegistrarError::NotFound`] if the name has never been registered.
    pub fn quote_renewal(
        env: Env,
        name: String,
        years: u64,
        now_unix: u64,
    ) -> Result<RenewalQuote, RegistrarError> {
        let label = extract_label_soroban(&env, &name).map_err(|_| RegistrarError::Validation)?;
        validate_registration_years_soroban(years).map_err(|_| RegistrarError::Validation)?;

        let record = env
            .storage()
            .persistent()
            .get::<_, RegistrationRecord>(&DataKey::Registration(name))
            .ok_or(RegistrarError::NotFound)?;

        // Renewal extends from the later of the current expiry or now, matching `renew`.
        let base_time = if record.expires_at > now_unix {
            record.expires_at
        } else {
            now_unix
        };
        let extended_expiry_unix = expiry_from_now(base_time, years);
        let annual_fee = price_for_label_length(label.len() as usize);

        Ok(RenewalQuote {
            fee_stroops: annual_fee.saturating_mul(years),
            current_expiry_unix: record.expires_at,
            extended_expiry_unix,
            grace_period_ends_at: grace_period_ends_at(extended_expiry_unix),
            pricing: PricingBreakdown {
                annual_fee_stroops: annual_fee,
                duration_years: years,
                premium_stroops: 0,
            },
        })
    }

    /// Issue #217: Read-only version of the pricing policy table so clients can
    /// detect quote-policy changes without diffing individual quotes.
    pub fn pricing_policy_version(_env: Env) -> u32 {
        pricing::PRICING_POLICY_VERSION
    }

    pub fn register(
        env: Env,
        label: String,
        owner: Address,
        years: u64,
        payment_stroops: u64,
        now_unix: u64,
    ) -> Result<(), RegistrarError> {
        owner.require_auth();

        validate_label_soroban(&label).map_err(|_| RegistrarError::Validation)?;
        validate_registration_years_soroban(years).map_err(|_| RegistrarError::Validation)?;

        if env
            .storage()
            .persistent()
            .get::<_, bool>(&DataKey::Reserved(label.clone()))
            .unwrap_or(false)
        {
            return Err(RegistrarError::Reserved);
        }

        // Check rate limit before proceeding with registration
        check_rate_limit(&env, &owner, now_unix)?;

        let quote = build_quote(&label, years, now_unix);
        if payment_stroops < quote.fee_stroops {
            return Err(RegistrarError::InsufficientFee);
        }

        let name = build_xlm_name(&env, &label).map_err(|_| RegistrarError::Validation)?;
        if let Some(existing) = env
            .storage()
            .persistent()
            .get::<_, RegistrationRecord>(&DataKey::Registration(name.clone()))
        {
            if now_unix <= existing.grace_period_ends_at {
                return Err(RegistrarError::AlreadyRegistered);
            }
        }

        let record = RegistrationRecord {
            name: name.clone(),
            owner: owner.clone(),
            registered_at: now_unix,
            expires_at: quote.expiry_unix,
            grace_period_ends_at: quote.grace_period_ends_at,
            fee_paid: payment_stroops,
            renewed_at: now_unix,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Registration(name.clone()), &record);

        // Record this registration for rate limit tracking
        record_registration(&env, &owner, now_unix)?;

        let treasury = env
            .storage()
            .persistent()
            .get::<_, u64>(&DataKey::Treasury)
            .unwrap_or(0);
        env.storage().persistent().set(
            &DataKey::Treasury,
            &treasury.saturating_add(payment_stroops),
        );
        let reg_count = env
            .storage()
            .persistent()
            .get::<_, u64>(&DataKey::RegistrationCount)
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&DataKey::RegistrationCount, &reg_count.saturating_add(1));

        let registry: Address = env
            .storage()
            .instance()
            .get(&DataKey::Registry)
            .ok_or(RegistrarError::NotInitialized)?;

        env.invoke_contract::<()>(
            &registry,
            &Symbol::new(&env, "register"),
            (
                name,
                owner.clone(),
                Option::<String>::None,
                Option::<String>::None,
                now_unix,
                record.expires_at,
                record.grace_period_ends_at,
            )
                .into_val(&env),
        );

        env.events().publish(
            (symbol_short!("registrar"), symbol_short!("reg")),
            (
                label,
                owner,
                payment_stroops,
                record.expires_at,
                record.grace_period_ends_at,
            ),
        );

        Ok(())
    }

    pub fn renew(
        env: Env,
        name: String,
        caller: Address,
        years: u64,
        payment_stroops: u64,
        now_unix: u64,
    ) -> Result<(), RegistrarError> {
        caller.require_auth();

        let label = extract_label_soroban(&env, &name).map_err(|_| RegistrarError::Validation)?;
        validate_registration_years_soroban(years).map_err(|_| RegistrarError::Validation)?;

        let mut record = env
            .storage()
            .persistent()
            .get::<_, RegistrationRecord>(&DataKey::Registration(name.clone()))
            .ok_or(RegistrarError::NotFound)?;
        if record.owner != caller {
            return Err(RegistrarError::Unauthorized);
        }
        match can_renew(record.expires_at, now_unix) {
            Ok(true) => {}
            Ok(false) => return Err(RegistrarError::NotRenewable),
            Err(e) => return Err(e),
        }

        let fee_due = price_for_label_length(label.len() as usize).saturating_mul(years);
        if payment_stroops < fee_due {
            return Err(RegistrarError::InsufficientFee);
        }

        let base_time = if record.expires_at > now_unix {
            record.expires_at
        } else {
            now_unix
        };
        let expires_at = expiry_from_now(base_time, years);
        record.expires_at = expires_at;
        record.grace_period_ends_at = grace_period_ends_at(expires_at);
        record.renewed_at = now_unix;
        record.fee_paid = record.fee_paid.saturating_add(payment_stroops);
        env.storage()
            .persistent()
            .set(&DataKey::Registration(name.clone()), &record);

        let treasury = env
            .storage()
            .persistent()
            .get::<_, u64>(&DataKey::Treasury)
            .unwrap_or(0);
        env.storage().persistent().set(
            &DataKey::Treasury,
            &treasury.saturating_add(payment_stroops),
        );
        let renew_count = env
            .storage()
            .persistent()
            .get::<_, u64>(&DataKey::RenewalCount)
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&DataKey::RenewalCount, &renew_count.saturating_add(1));

        let registry: Address = env
            .storage()
            .instance()
            .get(&DataKey::Registry)
            .ok_or(RegistrarError::NotInitialized)?;

        env.invoke_contract::<()>(
            &registry,
            &Symbol::new(&env, "renew"),
            (
                name.clone(),
                caller.clone(),
                record.expires_at,
                record.grace_period_ends_at,
                now_unix,
            )
                .into_val(&env),
        );

        env.events().publish(
            (symbol_short!("registrar"), symbol_short!("renewed")),
            (
                name,
                caller,
                payment_stroops,
                record.expires_at,
                record.grace_period_ends_at,
            ),
        );

        Ok(())
    }

    pub fn registration(env: Env, name: String) -> Option<RegistrationRecord> {
        env.storage().persistent().get(&DataKey::Registration(name))
    }

    pub fn is_available(env: Env, label: String, now_unix: u64) -> bool {
        let name = match build_xlm_name(&env, &label) {
            Ok(name) => name,
            Err(_) => return false,
        };
        env.storage()
            .persistent()
            .get::<_, RegistrationRecord>(&DataKey::Registration(name))
            .map(|record| now_unix > record.grace_period_ends_at)
            .unwrap_or(true)
    }

    pub fn treasury_balance(env: Env) -> u64 {
        env.storage()
            .persistent()
            .get(&DataKey::Treasury)
            .unwrap_or(0)
    }

    pub fn fee_metrics(env: Env) -> RegistrarMetrics {
        RegistrarMetrics {
            treasury_balance: env
                .storage()
                .persistent()
                .get(&DataKey::Treasury)
                .unwrap_or(0),
            total_registrations: env
                .storage()
                .persistent()
                .get(&DataKey::RegistrationCount)
                .unwrap_or(0),
            total_renewals: env
                .storage()
                .persistent()
                .get(&DataKey::RenewalCount)
                .unwrap_or(0),
        }
    }

    pub fn supports_admin_recovery(_env: Env) -> bool {
        ADMIN_RECOVERY_SUPPORTED
    }

    /// Issue #311: Return the lifecycle status of a name.
    pub fn registration_status(env: Env, label: String, now_unix: u64) -> RegistrationStatus {
        // Check reserved first
        if env
            .storage()
            .persistent()
            .get::<_, bool>(&DataKey::Reserved(label.clone()))
            .unwrap_or(false)
        {
            return RegistrationStatus::Reserved;
        }

        let name = match build_xlm_name(&env, &label) {
            Ok(n) => n,
            Err(_) => return RegistrationStatus::Unavailable,
        };

        let record = match env
            .storage()
            .persistent()
            .get::<_, RegistrationRecord>(&DataKey::Registration(name))
        {
            Some(r) => r,
            None => return RegistrationStatus::Unavailable,
        };

        if now_unix <= record.expires_at {
            RegistrationStatus::Active
        } else if now_unix <= record.grace_period_ends_at {
            RegistrationStatus::GracePeriod
        } else {
            RegistrationStatus::Claimable
        }
    }

    /// Issue #313: Read-only aggregate accounting report for operator reconciliation.
    /// Returns the same data as fee_metrics() with an intent-revealing name.
    pub fn accounting_report(env: Env) -> RegistrarMetrics {
        RegistrarMetrics {
            treasury_balance: env
                .storage()
                .persistent()
                .get(&DataKey::Treasury)
                .unwrap_or(0),
            total_registrations: env
                .storage()
                .persistent()
                .get(&DataKey::RegistrationCount)
                .unwrap_or(0),
            total_renewals: env
                .storage()
                .persistent()
                .get(&DataKey::RenewalCount)
                .unwrap_or(0),
        }
    }

    /// Governance function: Set rate limit configuration
    pub fn set_rate_limit_config(
        env: Env,
        window_size_seconds: u64,
        max_registrations_per_window: u64,
    ) -> Result<(), RegistrarError> {
        let config = RateLimitConfig {
            window_size_seconds,
            max_registrations_per_window,
        };
        env.storage()
            .persistent()
            .set(&DataKey::RateLimitConfig, &config);

        env.events().publish(
            (symbol_short!("registrar"), symbol_short!("rate")),
            (window_size_seconds, max_registrations_per_window),
        );

        Ok(())
    }

    /// Governance function: Get current rate limit configuration
    pub fn get_rate_limit_config(env: Env) -> RateLimitConfig {
        env.storage()
            .persistent()
            .get(&DataKey::RateLimitConfig)
            .unwrap_or(RateLimitConfig {
                window_size_seconds: DEFAULT_RATE_LIMIT_WINDOW_SECONDS,
                max_registrations_per_window: DEFAULT_MAX_REGISTRATIONS_PER_WINDOW,
            })
    }

    /// Governance function: Whitelist an address to bypass rate limiting
    pub fn whitelist_address(env: Env, address: Address) -> Result<(), RegistrarError> {
        env.storage()
            .persistent()
            .set(&DataKey::WhitelistedAddress(address.clone()), &true);

        env.events().publish(
            (symbol_short!("registrar"), symbol_short!("wlist")),
            address,
        );

        Ok(())
    }

    /// Governance function: Remove address from whitelist
    pub fn remove_whitelist_address(env: Env, address: Address) -> Result<(), RegistrarError> {
        let key = DataKey::WhitelistedAddress(address.clone());
        env.storage().persistent().remove(&key);

        env.events().publish(
            (symbol_short!("registrar"), symbol_short!("unwlist")),
            address,
        );

        Ok(())
    }

    /// Check if an address is whitelisted
    pub fn is_whitelisted(env: Env, address: Address) -> bool {
        env.storage()
            .persistent()
            .get::<_, bool>(&DataKey::WhitelistedAddress(address))
            .unwrap_or(false)
    }

    /// Get rate limit status for an address (read-only)
    pub fn get_registrations_in_window(env: Env, address: Address, now_unix: u64) -> u64 {
        let config = Self::get_rate_limit_config(&env);
        let window_start = now_unix.saturating_sub(config.window_size_seconds);
        let key = DataKey::RegistrationWindow(address, window_start);
        env.storage().persistent().get::<_, u64>(&key).unwrap_or(0)
    }
}

/// Check if an address is within rate limits for a given time window
fn check_rate_limit(env: &Env, address: &Address, now_unix: u64) -> Result<(), RegistrarError> {
    // Check if address is whitelisted (bypass rate limit)
    if env
        .storage()
        .persistent()
        .get::<_, bool>(&DataKey::WhitelistedAddress(address.clone()))
        .unwrap_or(false)
    {
        return Ok(());
    }

    // Get rate limit config
    let config = env
        .storage()
        .persistent()
        .get::<_, RateLimitConfig>(&DataKey::RateLimitConfig)
        .unwrap_or(RateLimitConfig {
            window_size_seconds: DEFAULT_RATE_LIMIT_WINDOW_SECONDS,
            max_registrations_per_window: DEFAULT_MAX_REGISTRATIONS_PER_WINDOW,
        });

    let window_start = now_unix.saturating_sub(config.window_size_seconds);
    let key = DataKey::RegistrationWindow(address.clone(), window_start);

    let count = env.storage().persistent().get::<_, u64>(&key).unwrap_or(0);

    // Check if we've exceeded the limit
    if count >= config.max_registrations_per_window {
        env.events().publish(
            (symbol_short!("registrar"), symbol_short!("limit")),
            (address.clone(), count),
        );
        return Err(RegistrarError::RateLimitExceeded);
    }

    Ok(())
}

/// Record a registration for an address within the current window
fn record_registration(env: &Env, address: &Address, now_unix: u64) -> Result<(), RegistrarError> {
    // Get rate limit config
    let config = env
        .storage()
        .persistent()
        .get::<_, RateLimitConfig>(&DataKey::RateLimitConfig)
        .unwrap_or(RateLimitConfig {
            window_size_seconds: DEFAULT_RATE_LIMIT_WINDOW_SECONDS,
            max_registrations_per_window: DEFAULT_MAX_REGISTRATIONS_PER_WINDOW,
        });

    let window_start = now_unix.saturating_sub(config.window_size_seconds);
    let key = DataKey::RegistrationWindow(address.clone(), window_start);

    let count = env.storage().persistent().get::<_, u64>(&key).unwrap_or(0);

    env.storage()
        .persistent()
        .set(&key, &count.saturating_add(1));

    Ok(())
}

fn build_quote(label: &String, years: u64, now_unix: u64) -> RegistrationQuote {
    let annual_fee = price_for_label_length(label.len() as usize);
    let expiry_unix = expiry_from_now(now_unix, years);

    RegistrationQuote {
        fee_stroops: annual_fee.saturating_mul(years),
        expiry_unix,
        grace_period_ends_at: grace_period_ends_at(expiry_unix),
        pricing: PricingBreakdown {
            annual_fee_stroops: annual_fee,
            duration_years: years,
            premium_stroops: 0,
        },
    }
}

pub fn can_renew(expiry_unix: u64, now_unix: u64) -> Result<bool, RegistrarError> {
    let grace_period_end = grace_period_ends_at(expiry_unix);

    if now_unix > grace_period_end {
        return Err(RegistrarError::RegistrationClaimable);
    }

    Ok(true)
}

fn migrate(from_version: u32, to_version: u32, _data: &Bytes) {
    let _ = (from_version, to_version);
}

fn decode_target_version(data: &Bytes) -> u32 {
    if data.len() < 4 {
        return CONTRACT_VERSION + 1;
    }
    let b0 = data.get(0).unwrap_or(0);
    let b1 = data.get(1).unwrap_or(0);
    let b2 = data.get(2).unwrap_or(0);
    let b3 = data.get(3).unwrap_or(0);
    u32::from_be_bytes([b0, b1, b2, b3])
}
