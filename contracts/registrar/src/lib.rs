pub mod expiry;
pub mod pricing;
mod test;

use expiry::expiry_from_now;
use pricing::price_for_label_length;
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env, IntoVal,
    String, Symbol, Vec,
};
use xlm_ns_common::soroban::{
    build_xlm_name, extract_label_soroban, validate_label_soroban,
    validate_registration_years_soroban,
};
use xlm_ns_common::time::grace_period_ends_at;

pub const ADMIN_RECOVERY_SUPPORTED: bool = false;

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

#[derive(Clone)]
#[contracttype]
enum DataKey {
    Registration(String),
    Reserved(String),
    Treasury,
    Registry,
    RegistrationCount,
    RenewalCount,
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
}

#[contract]
pub struct RegistrarContract;

#[contractimpl]
impl RegistrarContract {
    pub fn initialize(env: Env, registry: Address) {
        env.storage().instance().set(&DataKey::Registry, &registry);
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
                owner,
                Option::<String>::None,
                Option::<String>::None,
                now_unix,
                record.expires_at,
                record.grace_period_ends_at,
            )
                .into_val(&env),
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
                name,
                caller,
                record.expires_at,
                record.grace_period_ends_at,
                now_unix,
            )
                .into_val(&env),
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

    Ok(now_unix <= grace_period_end)
}
