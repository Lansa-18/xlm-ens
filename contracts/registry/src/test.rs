#[cfg(test)]
mod tests {
    use soroban_sdk::{testutils::Address as _, Address, Env, String};

    use crate::{RegistryContract, RegistryContractClient, RegistryError};

    struct TimeHelper {
        pub now: u64,
    }

    impl TimeHelper {
        pub fn new() -> Self {
            Self { now: 100_000 }
        }
        pub fn future(&self, seconds: u64) -> u64 {
            self.now + seconds
        }
        pub fn past(&self, seconds: u64) -> u64 {
            self.now.saturating_sub(seconds)
        }
    }

    #[test]
    fn stores_registry_entries_in_persistent_storage() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(RegistryContract, ());
        let client = RegistryContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let next_owner = Address::generate(&env);
        let name = String::from_str(&env, "timmy.xlm");
        let target = Some(String::from_str(&env, "GABC"));

        let time = TimeHelper::new();
        let expires_at = time.future(1_000);
        let grace_period_ends_at = time.future(2_000);

        client.register(
            &name,
            &owner,
            &target,
            &None::<String>,
            &time.now,
            &expires_at,
            &grace_period_ends_at,
        );

        let transfer_time = time.future(10);
        client.transfer(&name, &owner, &next_owner, &transfer_time);

        let resolved = client.resolve(&name, &transfer_time);
        assert_eq!(resolved.owner, next_owner);
        assert_eq!(client.names_for_owner(&next_owner).len(), 1);
    }

    #[test]
    fn rejects_registration_with_expiry_before_now() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(RegistryContract, ());
        let client = RegistryContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let name = String::from_str(&env, "timmy.xlm");
        let time = TimeHelper::new();

        let result = client.try_register(
            &name,
            &owner,
            &None::<String>,
            &None::<String>,
            &time.now,
            &time.past(1),
            &time.future(100),
        );

        assert_eq!(result, Ok(Err(RegistryError::InvalidExpiry)));
    }

    #[test]
    fn rejects_registration_with_grace_period_before_expiry() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(RegistryContract, ());
        let client = RegistryContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let name = String::from_str(&env, "timmy.xlm");
        let time = TimeHelper::new();
        let expires_at = time.future(100);

        let result = client.try_register(
            &name,
            &owner,
            &None::<String>,
            &None::<String>,
            &time.now,
            &expires_at,
            &time.future(99),
        );

        assert_eq!(result, Ok(Err(RegistryError::InvalidGracePeriod)));
    }

    #[test]
    fn rejects_renewal_with_malformed_lifecycle_timestamps() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(RegistryContract, ());
        let client = RegistryContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let name = String::from_str(&env, "timmy.xlm");
        let time = TimeHelper::new();
        let expires_at = time.future(100);
        let grace_ends_at = time.future(200);

        client.register(
            &name,
            &owner,
            &None::<String>,
            &None::<String>,
            &time.now,
            &expires_at,
            &grace_ends_at,
        );

        let invalid_expiry =
            client.try_renew(&name, &owner, &time.past(1), &grace_ends_at, &time.now);
        assert_eq!(invalid_expiry, Ok(Err(RegistryError::InvalidExpiry)));

        let invalid_grace_period = client.try_renew(
            &name,
            &owner,
            &time.future(150),
            &time.future(149),
            &time.now,
        );
        assert_eq!(
            invalid_grace_period,
            Ok(Err(RegistryError::InvalidGracePeriod))
        );
    }

    #[test]
    fn rejects_renewal_that_reduces_expiry_or_grace_period() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(RegistryContract, ());
        let client = RegistryContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let name = String::from_str(&env, "timmy.xlm");
        let time = TimeHelper::new();
        let expires_at = time.future(100);
        let grace_ends_at = time.future(200);

        client.register(
            &name,
            &owner,
            &None::<String>,
            &None::<String>,
            &time.now,
            &expires_at,
            &grace_ends_at,
        );

        let reduced_expiry =
            client.try_renew(&name, &owner, &time.future(50), &grace_ends_at, &time.now);
        assert_eq!(reduced_expiry, Ok(Err(RegistryError::InvalidExpiry)));

        let reduced_grace =
            client.try_renew(&name, &owner, &expires_at, &time.future(150), &time.now);
        assert_eq!(reduced_grace, Ok(Err(RegistryError::InvalidGracePeriod)));

        let new_expires_at = time.future(200);
        let new_grace_ends_at = time.future(300);
        client.renew(
            &name,
            &owner,
            &new_expires_at,
            &new_grace_ends_at,
            &time.now,
        );
        let entry = client.resolve(&name, &time.now);
        assert_eq!(entry.expires_at, new_expires_at);
        assert_eq!(entry.grace_period_ends_at, new_grace_ends_at);
    }

    #[test]
    fn threat_unauthorized_actor_cannot_register_without_auth() {
        let env = Env::default();
        let contract_id = env.register(RegistryContract, ());
        let client = RegistryContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let name = String::from_str(&env, "timmy.xlm");
        let time = TimeHelper::new();

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.register(
                &name,
                &owner,
                &None::<String>,
                &None::<String>,
                &time.now,
                &time.future(1_000),
                &time.future(2_000),
            );
        }));

        assert!(result.is_err(), "registration without auth should fail");
        assert_eq!(
            client.try_resolve(&name, &100),
            Ok(Err(RegistryError::NotFound))
        );
    }

    #[test]
    fn threat_unauthorized_actor_cannot_transfer_without_auth() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(RegistryContract, ());
        let client = RegistryContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let next_owner = Address::generate(&env);
        let name = String::from_str(&env, "timmy.xlm");
        let time = TimeHelper::new();

        client.register(
            &name,
            &owner,
            &None::<String>,
            &None::<String>,
            &time.now,
            &time.future(1_000),
            &time.future(2_000),
        );

        env.set_auths(&[]);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.transfer(&name, &owner, &next_owner, &time.future(10));
        }));

        assert!(result.is_err(), "transfer without auth should fail");
        let resolved = client.resolve(&name, &time.future(10));
        assert_eq!(resolved.owner, owner);
        assert_eq!(client.names_for_owner(&next_owner).len(), 0);
    }

    #[test]
    fn threat_actor_cannot_transfer_unowned_name() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(RegistryContract, ());
        let client = RegistryContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let attacker = Address::generate(&env);
        let next_owner = Address::generate(&env);
        let name = String::from_str(&env, "timmy.xlm");
        let time = TimeHelper::new();

        client.register(
            &name,
            &owner,
            &None::<String>,
            &None::<String>,
            &time.now,
            &time.future(1_000),
            &time.future(2_000),
        );

        let result = client.try_transfer(&name, &attacker, &next_owner, &time.future(10));
        assert_eq!(result, Ok(Err(RegistryError::Unauthorized)));
    }

    #[test]
    fn threat_actor_cannot_set_resolver_for_unowned_name() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(RegistryContract, ());
        let client = RegistryContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let attacker = Address::generate(&env);
        let name = String::from_str(&env, "timmy.xlm");
        let time = TimeHelper::new();

        client.register(
            &name,
            &owner,
            &None::<String>,
            &None::<String>,
            &time.now,
            &time.future(1_000),
            &time.future(2_000),
        );

        let resolver = Some(String::from_str(&env, "resolver_address"));
        let result = client.try_set_resolver(&name, &attacker, &resolver, &time.future(10));
        assert_eq!(result, Ok(Err(RegistryError::Unauthorized)));
    }

    #[test]
    fn threat_actor_cannot_set_target_address_for_unowned_name() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(RegistryContract, ());
        let client = RegistryContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let attacker = Address::generate(&env);
        let name = String::from_str(&env, "timmy.xlm");
        let time = TimeHelper::new();

        client.register(
            &name,
            &owner,
            &None::<String>,
            &None::<String>,
            &time.now,
            &time.future(1_000),
            &time.future(2_000),
        );

        let target = Some(String::from_str(&env, "target_address"));
        let result = client.try_set_target_address(&name, &attacker, &target, &time.future(10));
        assert_eq!(result, Ok(Err(RegistryError::Unauthorized)));
    }

    #[test]
    fn threat_actor_cannot_set_metadata_for_unowned_name() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(RegistryContract, ());
        let client = RegistryContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let attacker = Address::generate(&env);
        let name = String::from_str(&env, "timmy.xlm");
        let time = TimeHelper::new();

        client.register(
            &name,
            &owner,
            &None::<String>,
            &None::<String>,
            &time.now,
            &time.future(1_000),
            &time.future(2_000),
        );

        let metadata = Some(String::from_str(&env, "ipfs://hash"));
        let result = client.try_set_metadata(&name, &attacker, &metadata, &time.future(10));
        assert_eq!(result, Ok(Err(RegistryError::Unauthorized)));
    }

    #[test]
    fn threat_actor_cannot_renew_unowned_name() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(RegistryContract, ());
        let client = RegistryContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let attacker = Address::generate(&env);
        let name = String::from_str(&env, "timmy.xlm");
        let time = TimeHelper::new();

        client.register(
            &name,
            &owner,
            &None::<String>,
            &None::<String>,
            &time.now,
            &time.future(1_000),
            &time.future(2_000),
        );

        let result = client.try_renew(
            &name,
            &attacker,
            &time.future(1500),
            &time.future(2500),
            &time.future(10),
        );
        assert_eq!(result, Ok(Err(RegistryError::Unauthorized)));
    }

    #[test]
    fn declares_that_admin_recovery_is_not_supported() {
        let env = Env::default();
        let contract_id = env.register(RegistryContract, ());
        let client = RegistryContractClient::new(&env, &contract_id);

        assert!(!client.supports_admin_recovery());
    }
}
