#[cfg(test)]
mod tests {
    use soroban_sdk::{
        testutils::{Address as _, Events as _},
        Address, Env, String,
    };

    use crate::expiry::{expiry_from_now, within_grace_period};
    use crate::pricing::price_for_label_length;
    use crate::{
        can_renew, RegistrarContract, RegistrarContractClient, RegistrarError, RegistrationStatus,
        GRACE_PERIOD_SECONDS,
    };
    use xlm_ns_registry::RegistryContract;

    #[test]
    fn applies_tiered_pricing() {
        assert_eq!(price_for_label_length(3), 1_000_000_000);
        assert_eq!(price_for_label_length(5), 250_000_000);
        assert_eq!(price_for_label_length(12), 100_000_000);
    }

    #[test]
    fn computes_expiry_and_grace_period() {
        let expiry = expiry_from_now(100, 1);
        assert!(within_grace_period(expiry, expiry + 10));
        assert!(can_renew(expiry, expiry + 10).unwrap());
    }

    #[test]
    fn stores_registrations_in_contract_storage() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(RegistrarContract, ());
        let client = RegistrarContractClient::new(&env, &contract_id);

        let registry_id = env.register(RegistryContract, ());
        client.initialize(&registry_id);

        let owner = Address::generate(&env);
        let label = String::from_str(&env, "timmy");
        let name = String::from_str(&env, "timmy.xlm");

        let quote = client.quote_registration(&label, &1, &100);
        client.register(&label, &owner, &1, &quote.fee_stroops, &100);
        assert!(!client.is_available(&label, &101));

        client.renew(&name, &owner, &1, &quote.fee_stroops, &200);

        let record = client.registration(&name).unwrap();
        assert_eq!(record.owner, owner);
        assert!(client.treasury_balance() >= quote.fee_stroops * 2);
    }

    // ==================== Renewal Lifecycle Tests ====================

    #[test]
    fn can_renew_active_registration_before_expiry() {
        let now = 1000;
        let expiry = 2000;
        let result = can_renew(expiry, now);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn can_renew_at_exact_expiry() {
        let now = 2000;
        let expiry = 2000;
        let result = can_renew(expiry, now);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn can_renew_during_grace_period() {
        let expiry = 1000;
        let _grace_end = expiry + GRACE_PERIOD_SECONDS;
        let now = expiry + 100;
        let result = can_renew(expiry, now);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn can_renew_at_grace_period_boundary_minus_one() {
        let expiry = 1000;
        let grace_end = expiry + GRACE_PERIOD_SECONDS;
        let now = grace_end - 1;
        let result = can_renew(expiry, now);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn can_renew_at_exact_grace_period_end() {
        let expiry = 1000;
        let grace_end = expiry + GRACE_PERIOD_SECONDS;
        let now = grace_end;
        let result = can_renew(expiry, now);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn cannot_renew_claimable_registration_after_grace_period() {
        let expiry = 1000;
        let grace_end = expiry + GRACE_PERIOD_SECONDS;
        let now = grace_end + 1;
        let result = can_renew(expiry, now);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), RegistrarError::RegistrationClaimable);
    }

    #[test]
    fn cannot_renew_claimable_registration_far_future() {
        let expiry = 1000;
        let grace_end = expiry + GRACE_PERIOD_SECONDS;
        let now = grace_end + 1000000;
        let result = can_renew(expiry, now);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), RegistrarError::RegistrationClaimable);
    }

    #[test]
    fn renew_fails_for_claimable_registration() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(RegistrarContract, ());
        let client = RegistrarContractClient::new(&env, &contract_id);

        let registry_id = env.register(RegistryContract, ());
        client.initialize(&registry_id);

        let owner = Address::generate(&env);
        let label = String::from_str(&env, "test");
        let name = String::from_str(&env, "test.xlm");

        let quote = client.quote_registration(&label, &1, &100);
        client.register(&label, &owner, &1, &quote.fee_stroops, &100);

        let grace_end = quote.grace_period_ends_at;
        let after_grace = grace_end + 1;

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.renew(&name, &owner, &1, &quote.fee_stroops, &after_grace);
        }));
        assert!(
            result.is_err(),
            "Renewal should fail for claimable registration"
        );
    }

    #[test]
    fn renew_succeeds_at_grace_period_boundary() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(RegistrarContract, ());
        let client = RegistrarContractClient::new(&env, &contract_id);

        let registry_id = env.register(RegistryContract, ());
        client.initialize(&registry_id);

        let owner = Address::generate(&env);
        let label = String::from_str(&env, "boundary");
        let name = String::from_str(&env, "boundary.xlm");

        let quote = client.quote_registration(&label, &1, &100);
        client.register(&label, &owner, &1, &quote.fee_stroops, &100);

        let grace_end = quote.grace_period_ends_at;
        client.renew(&name, &owner, &1, &quote.fee_stroops, &grace_end);

        let record = client.registration(&name).unwrap();
        assert!(record.expires_at > quote.expiry_unix);
    }

    #[test]
    fn declares_that_admin_recovery_is_not_supported() {
        let env = Env::default();
        let contract_id = env.register(RegistrarContract, ());
        let client = RegistrarContractClient::new(&env, &contract_id);

        assert!(!client.supports_admin_recovery());
    }

    #[test]
    fn quote_renewal_reports_current_and_extended_expiry() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(RegistrarContract, ());
        let client = RegistrarContractClient::new(&env, &contract_id);
        let registry_id = env.register(RegistryContract, ());
        client.initialize(&registry_id);

        let owner = Address::generate(&env);
        let label = String::from_str(&env, "alice");
        let name = String::from_str(&env, "alice.xlm");

        let reg_quote = client.quote_registration(&label, &1, &100);
        client.register(&label, &owner, &1, &reg_quote.fee_stroops, &100);

        let renewal = client.quote_renewal(&name, &2, &200);
        assert_eq!(renewal.current_expiry_unix, reg_quote.expiry_unix);
        assert!(renewal.extended_expiry_unix > renewal.current_expiry_unix);
        assert_eq!(
            renewal.grace_period_ends_at,
            renewal.extended_expiry_unix + GRACE_PERIOD_SECONDS
        );
        // 5-char label → 250_000_000 stroops/year × 2 years.
        assert_eq!(renewal.fee_stroops, 500_000_000);
        assert_eq!(renewal.pricing.annual_fee_stroops, 250_000_000);
        assert_eq!(renewal.pricing.duration_years, 2);
    }

    #[test]
    fn quote_renewal_fails_for_unregistered_name() {
        let env = Env::default();
        let contract_id = env.register(RegistrarContract, ());
        let client = RegistrarContractClient::new(&env, &contract_id);
        let registry_id = env.register(RegistryContract, ());
        client.initialize(&registry_id);

        let name = String::from_str(&env, "ghost.xlm");
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.quote_renewal(&name, &1, &100);
        }));
        assert!(
            result.is_err(),
            "quote_renewal should fail for an unregistered name"
        );
    }

    #[test]
    fn pricing_policy_version_is_exposed() {
        let env = Env::default();
        let contract_id = env.register(RegistrarContract, ());
        let client = RegistrarContractClient::new(&env, &contract_id);

        assert_eq!(client.pricing_policy_version(), 1);
    }

    #[test]
    fn quote_includes_pricing_breakdown() {
        let env = Env::default();
        let contract_id = env.register(RegistrarContract, ());
        let client = RegistrarContractClient::new(&env, &contract_id);

        // 5-char label → 250_000_000 stroops/year
        let label = String::from_str(&env, "alice");
        let quote = client.quote_registration(&label, &2, &100);
        assert_eq!(quote.pricing.annual_fee_stroops, 250_000_000);
        assert_eq!(quote.pricing.duration_years, 2);
        assert_eq!(quote.pricing.premium_stroops, 0);
        assert_eq!(quote.fee_stroops, 500_000_000);

        // 3-char label → 1_000_000_000 stroops/year
        let short_label = String::from_str(&env, "foo");
        let short_quote = client.quote_registration(&short_label, &1, &100);
        assert_eq!(short_quote.pricing.annual_fee_stroops, 1_000_000_000);
        assert_eq!(short_quote.pricing.duration_years, 1);
        assert_eq!(short_quote.fee_stroops, 1_000_000_000);

        // 10-char label → 100_000_000 stroops/year
        let long_label = String::from_str(&env, "longerlabel");
        let long_quote = client.quote_registration(&long_label, &3, &100);
        assert_eq!(long_quote.pricing.annual_fee_stroops, 100_000_000);
        assert_eq!(long_quote.pricing.duration_years, 3);
        assert_eq!(long_quote.fee_stroops, 300_000_000);
    }

    #[test]
    fn fee_metrics_track_operations() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(RegistrarContract, ());
        let client = RegistrarContractClient::new(&env, &contract_id);

        let registry_id = env.register(RegistryContract, ());
        client.initialize(&registry_id);

        let owner1 = Address::generate(&env);
        let owner2 = Address::generate(&env);
        let label1 = String::from_str(&env, "alpha");
        let label2 = String::from_str(&env, "delta");
        let name1 = String::from_str(&env, "alpha.xlm");

        let quote1 = client.quote_registration(&label1, &1, &100);
        let quote2 = client.quote_registration(&label2, &1, &100);

        client.register(&label1, &owner1, &1, &quote1.fee_stroops, &100);
        client.register(&label2, &owner2, &1, &quote2.fee_stroops, &100);
        client.renew(&name1, &owner1, &1, &quote1.fee_stroops, &200);

        let metrics = client.fee_metrics();
        assert_eq!(metrics.total_registrations, 2);
        assert_eq!(metrics.total_renewals, 1);
        assert_eq!(
            metrics.treasury_balance,
            quote1.fee_stroops + quote2.fee_stroops + quote1.fee_stroops
        );
        assert_eq!(metrics.treasury_balance, client.treasury_balance());
    }

    // Issue #311 - registration_status lifecycle

    #[test]
    fn status_is_unavailable_for_unknown_name() {
        let env = Env::default();
        let contract_id = env.register(RegistrarContract, ());
        let client = RegistrarContractClient::new(&env, &contract_id);
        let registry_id = env.register(xlm_ns_registry::RegistryContract, ());
        client.initialize(&registry_id);
        assert_eq!(
            client.registration_status(&String::from_str(&env, "ghost"), &1000),
            RegistrationStatus::Unavailable
        );
    }

    #[test]
    fn status_is_reserved_for_reserved_label() {
        let env = Env::default();
        let contract_id = env.register(RegistrarContract, ());
        let client = RegistrarContractClient::new(&env, &contract_id);
        let registry_id = env.register(xlm_ns_registry::RegistryContract, ());
        client.initialize(&registry_id);
        let label = String::from_str(&env, "admin");
        client.reserve_label(&label);
        assert_eq!(
            client.registration_status(&label, &1000),
            RegistrationStatus::Reserved
        );
    }

    #[test]
    fn status_is_active_during_registration_period() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(RegistrarContract, ());
        let client = RegistrarContractClient::new(&env, &contract_id);
        let registry_id = env.register(xlm_ns_registry::RegistryContract, ());
        client.initialize(&registry_id);
        let owner = Address::generate(&env);
        let label = String::from_str(&env, "alive");
        let quote = client.quote_registration(&label, &1, &100);
        client.register(&label, &owner, &1, &quote.fee_stroops, &100);
        assert_eq!(
            client.registration_status(&label, &(quote.expiry_unix - 1)),
            RegistrationStatus::Active
        );
    }

    #[test]
    fn status_is_grace_period_after_expiry() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(RegistrarContract, ());
        let client = RegistrarContractClient::new(&env, &contract_id);
        let registry_id = env.register(xlm_ns_registry::RegistryContract, ());
        client.initialize(&registry_id);
        let owner = Address::generate(&env);
        let label = String::from_str(&env, "gracing");
        let quote = client.quote_registration(&label, &1, &100);
        client.register(&label, &owner, &1, &quote.fee_stroops, &100);
        assert_eq!(
            client.registration_status(&label, &(quote.expiry_unix + 1)),
            RegistrationStatus::GracePeriod
        );
    }

    #[test]
    fn status_is_claimable_after_grace_period() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(RegistrarContract, ());
        let client = RegistrarContractClient::new(&env, &contract_id);
        let registry_id = env.register(xlm_ns_registry::RegistryContract, ());
        client.initialize(&registry_id);
        let owner = Address::generate(&env);
        let label = String::from_str(&env, "expired");
        let quote = client.quote_registration(&label, &1, &100);
        client.register(&label, &owner, &1, &quote.fee_stroops, &100);
        assert_eq!(
            client.registration_status(&label, &(quote.grace_period_ends_at + 1)),
            RegistrationStatus::Claimable
        );
    }

    // Issue #310 - payment reconciliation

    #[test]
    fn treasury_accumulates_exact_fees_across_registrations() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(RegistrarContract, ());
        let client = RegistrarContractClient::new(&env, &contract_id);
        let registry_id = env.register(xlm_ns_registry::RegistryContract, ());
        client.initialize(&registry_id);
        let owner1 = Address::generate(&env);
        let owner2 = Address::generate(&env);
        let label1 = String::from_str(&env, "pay1");
        let label2 = String::from_str(&env, "pay2");
        let q1 = client.quote_registration(&label1, &1, &100);
        let q2 = client.quote_registration(&label2, &2, &100);
        client.register(&label1, &owner1, &1, &q1.fee_stroops, &100);
        client.register(&label2, &owner2, &2, &q2.fee_stroops, &100);
        let expected = q1.fee_stroops + q2.fee_stroops;
        assert_eq!(client.treasury_balance(), expected);
        let report = client.accounting_report();
        assert_eq!(report.treasury_balance, expected);
        assert_eq!(report.total_registrations, 2);
        assert_eq!(report.total_renewals, 0);
    }

    #[test]
    fn treasury_accumulates_overpayment_stroops() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(RegistrarContract, ());
        let client = RegistrarContractClient::new(&env, &contract_id);
        let registry_id = env.register(xlm_ns_registry::RegistryContract, ());
        client.initialize(&registry_id);
        let owner = Address::generate(&env);
        let label = String::from_str(&env, "over");
        let quote = client.quote_registration(&label, &1, &100);
        let overpay = quote.fee_stroops + 9_999;
        client.register(&label, &owner, &1, &overpay, &100);
        assert_eq!(client.treasury_balance(), overpay);
        assert_eq!(client.accounting_report().treasury_balance, overpay);
    }

    #[test]
    fn registration_fails_on_insufficient_payment() {
        let env = Env::default();
        let contract_id = env.register(RegistrarContract, ());
        let client = RegistrarContractClient::new(&env, &contract_id);
        let registry_id = env.register(xlm_ns_registry::RegistryContract, ());
        client.initialize(&registry_id);
        let owner = Address::generate(&env);
        let label = String::from_str(&env, "cheap");
        let quote = client.quote_registration(&label, &1, &100);
        let underpay = quote.fee_stroops - 1;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.register(&label, &owner, &1, &underpay, &100);
        }));
        assert!(result.is_err(), "insufficient payment must be rejected");
        assert_eq!(client.treasury_balance(), 0);
        assert_eq!(client.accounting_report().total_registrations, 0);
    }

    #[test]
    fn renewal_count_and_treasury_update_correctly() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(RegistrarContract, ());
        let client = RegistrarContractClient::new(&env, &contract_id);
        let registry_id = env.register(xlm_ns_registry::RegistryContract, ());
        client.initialize(&registry_id);
        let owner = Address::generate(&env);
        let label = String::from_str(&env, "renew");
        let name = String::from_str(&env, "renew.xlm");
        let q = client.quote_registration(&label, &1, &100);
        client.register(&label, &owner, &1, &q.fee_stroops, &100);
        client.renew(&name, &owner, &1, &q.fee_stroops, &200);
        client.renew(&name, &owner, &1, &q.fee_stroops, &300);
        let report = client.accounting_report();
        assert_eq!(report.total_registrations, 1);
        assert_eq!(report.total_renewals, 2);
        assert_eq!(report.treasury_balance, q.fee_stroops * 3);
        assert_eq!(report.treasury_balance, client.treasury_balance());
    }

    // Issue #142 — registrar event emission

    #[test]
    fn register_emits_registered_event() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(RegistrarContract, ());
        let client = RegistrarContractClient::new(&env, &contract_id);
        let registry_id = env.register(xlm_ns_registry::RegistryContract, ());
        client.initialize(&registry_id);

        let owner = Address::generate(&env);
        let label = String::from_str(&env, "events");
        let quote = client.quote_registration(&label, &1, &100);
        client.register(&label, &owner, &1, &quote.fee_stroops, &100);

        assert!(
            !env.events().all().events().is_empty(),
            "register() must emit at least one event"
        );
    }

    #[test]
    fn renew_emits_renewed_event() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(RegistrarContract, ());
        let client = RegistrarContractClient::new(&env, &contract_id);
        let registry_id = env.register(xlm_ns_registry::RegistryContract, ());
        client.initialize(&registry_id);

        let owner = Address::generate(&env);
        let label = String::from_str(&env, "renev");
        let name = String::from_str(&env, "renev.xlm");
        let quote = client.quote_registration(&label, &1, &100);
        client.register(&label, &owner, &1, &quote.fee_stroops, &100);

        client.renew(&name, &owner, &1, &quote.fee_stroops, &200);

        assert!(
            !env.events().all().events().is_empty(),
            "renew() must emit at least one event"
        );
    }

    #[test]
    fn accounting_report_matches_fee_metrics() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(RegistrarContract, ());
        let client = RegistrarContractClient::new(&env, &contract_id);
        let registry_id = env.register(xlm_ns_registry::RegistryContract, ());
        client.initialize(&registry_id);
        let owner = Address::generate(&env);
        let label = String::from_str(&env, "match");
        let q = client.quote_registration(&label, &1, &100);
        client.register(&label, &owner, &1, &q.fee_stroops, &100);
        let metrics = client.fee_metrics();
        let report = client.accounting_report();
        assert_eq!(metrics.treasury_balance, report.treasury_balance);
        assert_eq!(metrics.total_registrations, report.total_registrations);
        assert_eq!(metrics.total_renewals, report.total_renewals);
    }

    // ==================== Rate Limiting Tests ====================

    #[test]
    fn rate_limit_config_initialized_with_defaults() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(RegistrarContract, ());
        let client = RegistrarContractClient::new(&env, &contract_id);
        let registry_id = env.register(RegistryContract, ());
        client.initialize(&registry_id);

        let config = client.get_rate_limit_config();
        assert_eq!(config.window_size_seconds, 86400); // 24 hours
        assert_eq!(config.max_registrations_per_window, 5);
    }

    #[test]
    fn can_register_up_to_limit_within_window() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(RegistrarContract, ());
        let client = RegistrarContractClient::new(&env, &contract_id);
        let registry_id = env.register(RegistryContract, ());
        client.initialize(&registry_id);

        let owner = Address::generate(&env);
        let now = 1000u64;

        // Register 5 names within the same time window (should all succeed)
        for i in 0..5 {
            let label = String::from_str(&env, &format!("name{}", i));
            let quote = client.quote_registration(&label, &1, &now);
            client.register(&label, &owner, &1, &quote.fee_stroops, &now);
        }

        // Verify we have 5 registrations
        let metrics = client.fee_metrics();
        assert_eq!(metrics.total_registrations, 5);
    }

    #[test]
    fn rate_limit_exceeded_on_sixth_registration_in_window() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(RegistrarContract, ());
        let client = RegistrarContractClient::new(&env, &contract_id);
        let registry_id = env.register(RegistryContract, ());
        client.initialize(&registry_id);

        let owner = Address::generate(&env);
        let now = 1000u64;

        // Register 5 names successfully
        for i in 0..5 {
            let label = String::from_str(&env, &format!("limit{}", i));
            let quote = client.quote_registration(&label, &1, &now);
            client.register(&label, &owner, &1, &quote.fee_stroops, &now);
        }

        // Attempt 6th registration - should fail with rate limit error
        let label6 = String::from_str(&env, "limit5");
        let quote6 = client.quote_registration(&label6, &1, &now);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.register(&label6, &owner, &1, &quote6.fee_stroops, &now);
        }));
        assert!(
            result.is_err(),
            "6th registration should fail with rate limit exceeded"
        );
    }

    #[test]
    fn whitelisted_address_bypasses_rate_limit() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(RegistrarContract, ());
        let client = RegistrarContractClient::new(&env, &contract_id);
        let registry_id = env.register(RegistryContract, ());
        client.initialize(&registry_id);

        let owner = Address::generate(&env);
        let now = 1000u64;

        // Whitelist the owner
        client.whitelist_address(&owner);
        assert!(client.is_whitelisted(&owner));

        // Should be able to register more than 5 names
        for i in 0..10 {
            let label = String::from_str(&env, &format!("white{}", i));
            let quote = client.quote_registration(&label, &1, &now);
            client.register(&label, &owner, &1, &quote.fee_stroops, &now);
        }

        let metrics = client.fee_metrics();
        assert_eq!(metrics.total_registrations, 10);
    }

    #[test]
    fn remove_whitelist_applies_rate_limit() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(RegistrarContract, ());
        let client = RegistrarContractClient::new(&env, &contract_id);
        let registry_id = env.register(RegistryContract, ());
        client.initialize(&registry_id);

        let owner = Address::generate(&env);
        let now = 1000u64;

        // Whitelist and register 3 names
        client.whitelist_address(&owner);
        for i in 0..3 {
            let label = String::from_str(&env, &format!("rmwhite{}", i));
            let quote = client.quote_registration(&label, &1, &now);
            client.register(&label, &owner, &1, &quote.fee_stroops, &now);
        }

        // Remove from whitelist
        client.remove_whitelist_address(&owner);
        assert!(!client.is_whitelisted(&owner));

        // Register 2 more (within the limit of 5)
        for i in 3..5 {
            let label = String::from_str(&env, &format!("rmwhite{}", i));
            let quote = client.quote_registration(&label, &1, &now);
            client.register(&label, &owner, &1, &quote.fee_stroops, &now);
        }

        // 6th registration should fail
        let label6 = String::from_str(&env, "rmwhite5");
        let quote6 = client.quote_registration(&label6, &1, &now);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.register(&label6, &owner, &1, &quote6.fee_stroops, &now);
        }));
        assert!(
            result.is_err(),
            "Should hit rate limit after whitelist removal"
        );
    }

    #[test]
    fn different_addresses_have_independent_rate_limits() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(RegistrarContract, ());
        let client = RegistrarContractClient::new(&env, &contract_id);
        let registry_id = env.register(RegistryContract, ());
        client.initialize(&registry_id);

        let owner1 = Address::generate(&env);
        let owner2 = Address::generate(&env);
        let now = 1000u64;

        // Owner1 registers 5 names
        for i in 0..5 {
            let label = String::from_str(&env, &format!("owner1_{}", i));
            let quote = client.quote_registration(&label, &1, &now);
            client.register(&label, &owner1, &1, &quote.fee_stroops, &now);
        }

        // Owner2 should still be able to register 5 names
        for i in 0..5 {
            let label = String::from_str(&env, &format!("owner2_{}", i));
            let quote = client.quote_registration(&label, &1, &now);
            client.register(&label, &owner2, &1, &quote.fee_stroops, &now);
        }

        let metrics = client.fee_metrics();
        assert_eq!(metrics.total_registrations, 10);
    }

    #[test]
    fn registrations_outside_window_do_not_count_toward_limit() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(RegistrarContract, ());
        let client = RegistrarContractClient::new(&env, &contract_id);
        let registry_id = env.register(RegistryContract, ());
        client.initialize(&registry_id);

        let owner = Address::generate(&env);
        let now = 1000u64;
        let window_size = 86400u64;
        let future_window = now + window_size + 1; // Outside the current window

        // Register 5 names at time now
        for i in 0..5 {
            let label = String::from_str(&env, &format!("window1_{}", i));
            let quote = client.quote_registration(&label, &1, &now);
            client.register(&label, &owner, &1, &quote.fee_stroops, &now);
        }

        // Register 5 more names in a future time window
        for i in 0..5 {
            let label = String::from_str(&env, &format!("window2_{}", i));
            let quote = client.quote_registration(&label, &1, &future_window);
            client.register(&label, &owner, &1, &quote.fee_stroops, &future_window);
        }

        let metrics = client.fee_metrics();
        assert_eq!(metrics.total_registrations, 10);
    }

    #[test]
    fn get_registrations_in_window_returns_count() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(RegistrarContract, ());
        let client = RegistrarContractClient::new(&env, &contract_id);
        let registry_id = env.register(RegistryContract, ());
        client.initialize(&registry_id);

        let owner = Address::generate(&env);
        let now = 1000u64;

        // Initially should be 0
        let initial_count = client.get_registrations_in_window(&owner, &now);
        assert_eq!(initial_count, 0);

        // Register 3 names
        for i in 0..3 {
            let label = String::from_str(&env, &format!("count{}", i));
            let quote = client.quote_registration(&label, &1, &now);
            client.register(&label, &owner, &1, &quote.fee_stroops, &now);
        }

        // Should now be 3
        let count_after = client.get_registrations_in_window(&owner, &now);
        assert_eq!(count_after, 3);
    }

    #[test]
    fn set_rate_limit_config_changes_limit() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(RegistrarContract, ());
        let client = RegistrarContractClient::new(&env, &contract_id);
        let registry_id = env.register(RegistryContract, ());
        client.initialize(&registry_id);

        // Change rate limit to 3 per window
        client.set_rate_limit_config(&86400, &3);
        let config = client.get_rate_limit_config();
        assert_eq!(config.max_registrations_per_window, 3);

        let owner = Address::generate(&env);
        let now = 1000u64;

        // Register 3 names successfully
        for i in 0..3 {
            let label = String::from_str(&env, &format!("limited{}", i));
            let quote = client.quote_registration(&label, &1, &now);
            client.register(&label, &owner, &1, &quote.fee_stroops, &now);
        }

        // 4th should fail
        let label4 = String::from_str(&env, "limited3");
        let quote4 = client.quote_registration(&label4, &1, &now);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.register(&label4, &owner, &1, &quote4.fee_stroops, &now);
        }));
        assert!(result.is_err(), "Should fail with new limit of 3");
    }

    #[test]
    fn rate_limit_events_emitted_on_limit_exceeded() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(RegistrarContract, ());
        let client = RegistrarContractClient::new(&env, &contract_id);
        let registry_id = env.register(RegistryContract, ());
        client.initialize(&registry_id);

        let owner = Address::generate(&env);
        let now = 1000u64;

        // Register 5 names
        for i in 0..5 {
            let label = String::from_str(&env, &format!("event{}", i));
            let quote = client.quote_registration(&label, &1, &now);
            client.register(&label, &owner, &1, &quote.fee_stroops, &now);
        }

        // Attempt 6th - should emit rate limit event
        let label6 = String::from_str(&env, "event5");
        let quote6 = client.quote_registration(&label6, &1, &now);
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.register(&label6, &owner, &1, &quote6.fee_stroops, &now);
        }));

        // Check that events were emitted
        let all_events = env.events().all().events();
        assert!(
            !all_events.is_empty(),
            "Rate limit events should have been emitted"
        );
    }
}
