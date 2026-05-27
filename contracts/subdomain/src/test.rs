#[cfg(test)]
mod tests {
    use soroban_sdk::{testutils::{Address as _, Events as _}, Address, Env, String};

    use crate::{SubdomainContract, SubdomainContractClient};

    #[test]
    fn register_parent_emits_event() {
        let env = Env::default();
        let contract_id = env.register(SubdomainContract, ());
        let client = SubdomainContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let parent = String::from_str(&env, "timmy.xlm");

        client.register_parent(&parent, &owner);

        assert_eq!(env.events().all().events().len(), 1);
    }

    #[test]
    fn create_subdomain_emits_event() {
        let env = Env::default();
        let contract_id = env.register(SubdomainContract, ());
        let client = SubdomainContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let sub_owner = Address::generate(&env);
        let parent = String::from_str(&env, "timmy.xlm");

        client.register_parent(&parent, &owner);
        let fqdn = client.create(&String::from_str(&env, "pay"), &parent, &owner, &sub_owner, &100);

        assert_eq!(fqdn, String::from_str(&env, "pay.timmy.xlm"));
        assert_eq!(env.events().all().events().len(), 1);
    }

    #[test]
    fn transfer_emits_event() {
        let env = Env::default();
        let contract_id = env.register(SubdomainContract, ());
        let client = SubdomainContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let sub_owner = Address::generate(&env);
        let new_owner = Address::generate(&env);
        let parent = String::from_str(&env, "timmy.xlm");

        client.register_parent(&parent, &owner);
        let fqdn = client.create(&String::from_str(&env, "pay"), &parent, &owner, &sub_owner, &100);

        client.transfer(&fqdn, &sub_owner, &new_owner);

        assert_eq!(env.events().all().events().len(), 1);
        assert_eq!(client.record(&fqdn).unwrap().owner, new_owner);
    }

    #[test]
    fn revoke_emits_event() {
        let env = Env::default();
        let contract_id = env.register(SubdomainContract, ());
        let client = SubdomainContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let sub_owner = Address::generate(&env);
        let parent = String::from_str(&env, "timmy.xlm");

        client.register_parent(&parent, &owner);
        let fqdn = client.create(&String::from_str(&env, "pay"), &parent, &owner, &sub_owner, &100);

        client.revoke(&fqdn, &sub_owner);

        assert_eq!(env.events().all().events().len(), 1);
        assert!(!client.exists(&fqdn));
    }

    #[test]
    fn add_and_remove_controller_emit_events() {
        let env = Env::default();
        let contract_id = env.register(SubdomainContract, ());
        let client = SubdomainContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let controller = Address::generate(&env);
        let parent = String::from_str(&env, "timmy.xlm");

        client.register_parent(&parent, &owner);

        client.add_controller(&parent, &owner, &controller);
        assert_eq!(env.events().all().events().len(), 1);

        client.remove_controller(&parent, &owner, &controller);
        assert_eq!(env.events().all().events().len(), 1);
    }

    #[test]
    fn stores_subdomain_records_in_contract_storage() {
        let env = Env::default();
        let contract_id = env.register(SubdomainContract, ());
        let client = SubdomainContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let controller = Address::generate(&env);
        let sub_owner = Address::generate(&env);
        let parent = String::from_str(&env, "timmy.xlm");

        client.register_parent(&parent, &owner);
        client.add_controller(&parent, &owner, &controller);

        let fqdn = client.create(
            &String::from_str(&env, "pay"),
            &parent,
            &controller,
            &sub_owner,
            &100,
        );

        assert_eq!(fqdn, String::from_str(&env, "pay.timmy.xlm"));
        assert!(client.exists(&fqdn));
        assert_eq!(client.record(&fqdn).unwrap().owner, sub_owner);
    }

    #[test]
    fn removes_controller_and_revokes_authority() {
        let env = Env::default();
        let contract_id = env.register(SubdomainContract, ());
        let client = SubdomainContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let controller = Address::generate(&env);
        let sub_owner = Address::generate(&env);
        let parent = String::from_str(&env, "timmy.xlm");

        client.register_parent(&parent, &owner);

        // Add controller
        client.add_controller(&parent, &owner, &controller);

        // Remove controller
        client.remove_controller(&parent, &owner, &controller);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.create(
                &String::from_str(&env, "pay"),
                &parent,
                &controller,
                &sub_owner,
                &100,
            );
        }));
        assert!(result.is_err(), "post-removal create should fail");
    }

    #[test]
    fn prevents_parent_takeover() {
        let env = Env::default();
        let contract_id = env.register(SubdomainContract, ());
        let client = SubdomainContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let intruder = Address::generate(&env);
        let parent = String::from_str(&env, "timmy.xlm");

        client.register_parent(&parent, &owner);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.register_parent(&parent, &intruder);
        }));

        assert!(result.is_err(), "intruder parent registration should fail");

        let parent_record = client.parent(&parent).unwrap();
        assert_eq!(
            parent_record.owner, owner,
            "original owner should be preserved"
        );
    }

    #[test]
    fn subdomain_owner_can_revoke() {
        let env = Env::default();
        let contract_id = env.register(SubdomainContract, ());
        let client = SubdomainContractClient::new(&env, &contract_id);

        let parent_owner = Address::generate(&env);
        let sub_owner = Address::generate(&env);
        let parent = String::from_str(&env, "timmy.xlm");

        client.register_parent(&parent, &parent_owner);
        let fqdn = client.create(
            &String::from_str(&env, "pay"),
            &parent,
            &parent_owner,
            &sub_owner,
            &100,
        );

        assert!(client.exists(&fqdn));
        client.revoke(&fqdn, &sub_owner);
        assert!(!client.exists(&fqdn));
    }

    #[test]
    fn parent_owner_can_revoke() {
        let env = Env::default();
        let contract_id = env.register(SubdomainContract, ());
        let client = SubdomainContractClient::new(&env, &contract_id);

        let parent_owner = Address::generate(&env);
        let sub_owner = Address::generate(&env);
        let parent = String::from_str(&env, "timmy.xlm");

        client.register_parent(&parent, &parent_owner);
        let fqdn = client.create(
            &String::from_str(&env, "pay"),
            &parent,
            &parent_owner,
            &sub_owner,
            &100,
        );

        assert!(client.exists(&fqdn));
        client.revoke(&fqdn, &parent_owner);
        assert!(!client.exists(&fqdn));
    }

    #[test]
    fn parent_controller_can_revoke() {
        let env = Env::default();
        let contract_id = env.register(SubdomainContract, ());
        let client = SubdomainContractClient::new(&env, &contract_id);

        let parent_owner = Address::generate(&env);
        let controller = Address::generate(&env);
        let sub_owner = Address::generate(&env);
        let parent = String::from_str(&env, "timmy.xlm");

        client.register_parent(&parent, &parent_owner);
        client.add_controller(&parent, &parent_owner, &controller);

        let fqdn = client.create(
            &String::from_str(&env, "pay"),
            &parent,
            &controller,
            &sub_owner,
            &100,
        );

        assert!(client.exists(&fqdn));
        client.revoke(&fqdn, &controller);
        assert!(!client.exists(&fqdn));
    }

    #[test]
    fn unauthorized_caller_cannot_revoke() {
        let env = Env::default();
        let contract_id = env.register(SubdomainContract, ());
        let client = SubdomainContractClient::new(&env, &contract_id);

        let parent_owner = Address::generate(&env);
        let sub_owner = Address::generate(&env);
        let intruder = Address::generate(&env);
        let parent = String::from_str(&env, "timmy.xlm");

        client.register_parent(&parent, &parent_owner);
        let fqdn = client.create(
            &String::from_str(&env, "pay"),
            &parent,
            &parent_owner,
            &sub_owner,
            &100,
        );

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.revoke(&fqdn, &intruder);
        }));
        assert!(result.is_err(), "unauthorized revocation should fail");
        assert!(client.exists(&fqdn));
    }

    #[test]
    fn rejects_duplicate_parent_registration() {
        let env = Env::default();
        let contract_id = env.register(SubdomainContract, ());
        let client = SubdomainContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let parent = String::from_str(&env, "timmy.xlm");

        client.register_parent(&parent, &owner);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.register_parent(&parent, &owner);
        }));
        assert!(result.is_err(), "duplicate parent registration should fail");
    }

    #[test]
    fn rejects_unauthorized_subdomain_creation() {
        let env = Env::default();
        let contract_id = env.register(SubdomainContract, ());
        let client = SubdomainContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let intruder = Address::generate(&env);
        let sub_owner = Address::generate(&env);
        let parent = String::from_str(&env, "timmy.xlm");

        client.register_parent(&parent, &owner);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.create(
                &String::from_str(&env, "pay"),
                &parent,
                &intruder,
                &sub_owner,
                &100,
            );
        }));
        assert!(result.is_err(), "unauthorized create should fail");
    }

    #[test]
    fn rejects_unauthorized_controller_addition() {
        let env = Env::default();
        let contract_id = env.register(SubdomainContract, ());
        let client = SubdomainContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let intruder = Address::generate(&env);
        let controller = Address::generate(&env);
        let parent = String::from_str(&env, "timmy.xlm");

        client.register_parent(&parent, &owner);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.add_controller(&parent, &intruder, &controller);
        }));
        assert!(
            result.is_err(),
            "intruder should not be able to add a controller"
        );

        let parent_record = client.parent(&parent).unwrap();
        assert!(!parent_record.controllers.contains(&controller));
    }

    #[test]
    fn transfers_subdomain_ownership_and_queries_existence() {
        let env = Env::default();
        let contract_id = env.register(SubdomainContract, ());
        let client = SubdomainContractClient::new(&env, &contract_id);

        let owner = Address::generate(&env);
        let sub_owner = Address::generate(&env);
        let new_sub_owner = Address::generate(&env);
        let parent = String::from_str(&env, "timmy.xlm");

        client.register_parent(&parent, &owner);

        let fqdn = client.create(
            &String::from_str(&env, "pay"),
            &parent,
            &owner,
            &sub_owner,
            &100,
        );

        assert!(client.exists(&fqdn));
        assert_eq!(client.record(&fqdn).unwrap().owner, sub_owner);

        client.transfer(&fqdn, &sub_owner, &new_sub_owner);
        assert_eq!(client.record(&fqdn).unwrap().owner, new_sub_owner);

        let intruder = Address::generate(&env);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.transfer(&fqdn, &intruder, &new_sub_owner);
        }));
        assert!(result.is_err(), "unauthorized transfer should fail");

        assert_eq!(client.record(&fqdn).unwrap().owner, new_sub_owner);
    }
}
