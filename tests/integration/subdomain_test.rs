use soroban_sdk::{testutils::Address as _, Address, Env, String};

use xlm_ns_resolver::ResolverContract;
use xlm_ns_subdomain::SubdomainContract;

#[test]
fn subdomain_flow_covers_controller_delegation_transfer_and_resolution() {
    let env = Env::default();

    let subdomain_contract_id = env.register(SubdomainContract, ());
    let resolver_contract_id = env.register(ResolverContract, ());

    let subdomain = xlm_ns_subdomain::SubdomainContractClient::new(&env, &subdomain_contract_id);
    let resolver = xlm_ns_resolver::ResolverContractClient::new(&env, &resolver_contract_id);

    let parent_owner = Address::generate(&env);
    let controller = Address::generate(&env);
    let intruder = Address::generate(&env);
    let subdomain_owner = Address::generate(&env);
    let next_owner = Address::generate(&env);

    let parent = String::from_str(&env, "timmy.xlm");
    let label = String::from_str(&env, "pay");
    let fqdn = String::from_str(&env, "pay.timmy.xlm");
    let first_address = String::from_str(&env, "GABC");
    let second_address = String::from_str(&env, "GDEF");

    subdomain.register_parent(&parent, &parent_owner);

    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            subdomain.add_controller(&parent, &intruder, &controller);
        }))
        .is_err(),
        "non-owner should not be able to add a controller"
    );

    subdomain.add_controller(&parent, &parent_owner, &controller);

    let parent_record = subdomain.parent(&parent).unwrap();
    assert_eq!(parent_record.owner, parent_owner);
    assert!(parent_record.controllers.contains(&controller));

    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            subdomain.create(&label, &parent, &intruder, &subdomain_owner, &100);
        }))
        .is_err(),
        "unauthorized caller should not be able to create a subdomain"
    );

    let created_name = subdomain.create(&label, &parent, &controller, &subdomain_owner, &101);
    assert_eq!(created_name, fqdn);
    assert!(subdomain.exists(&fqdn));

    let created_record = subdomain.record(&fqdn).unwrap();
    assert_eq!(created_record.parent, parent);
    assert_eq!(created_record.owner, subdomain_owner);
    assert_eq!(created_record.created_at, 101);

    resolver.set_record(&fqdn, &subdomain_owner, &first_address, &102);
    assert!(resolver.has_record(&fqdn));
    resolver.set_primary_name(&first_address, &subdomain_owner, &fqdn);

    let resolved_before_transfer = resolver.resolve(&fqdn).unwrap();
    assert_eq!(resolved_before_transfer.owner, subdomain_owner);
    assert_eq!(resolved_before_transfer.address, first_address);
    assert_eq!(resolver.reverse(&first_address), Some(fqdn.clone()));

    // Transfer subdomain ownership, then update resolver ownership explicitly.
    subdomain.transfer(&fqdn, &subdomain_owner, &next_owner);
    resolver.transfer_record_owner(&fqdn, &subdomain_owner, &next_owner);

    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // Old resolver record owner (subdomain_owner) should no longer be able to transfer resolver record ownership
            resolver.transfer_record_owner(&fqdn, &subdomain_owner, &controller);
        }))
        .is_err(),
        "previous owner should not be able to transfer after ownership changes"
    );

    let transferred_record = subdomain.record(&fqdn).unwrap();
    assert_eq!(transferred_record.owner, next_owner);

    // Verify old subdomain owner cannot update resolver record
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            resolver.set_record(&fqdn, &subdomain_owner, &second_address, &103);
        }))
        .is_err(),
        "old subdomain owner should not be able to update resolver record after transfer"
    );

    // New subdomain owner can now update resolver record
    resolver.set_record(&fqdn, &next_owner, &second_address, &103);
    resolver.set_primary_name(&second_address, &next_owner, &fqdn);

    let resolved_after_transfer = resolver.resolve(&fqdn).unwrap();
    assert_eq!(resolved_after_transfer.owner, next_owner);
    assert_eq!(resolved_after_transfer.address, second_address);
    assert_eq!(resolver.reverse(&second_address), Some(fqdn.clone()));

    // Test deletion of subdomain and its effect on resolver (none, resolver record persists)
    subdomain.delete(&fqdn, &next_owner);
    assert!(!subdomain.exists(&fqdn));
    // Resolver record should still exist, and only the last owner (next_owner) can modify it.
    assert!(resolver.has_record(&fqdn));
    assert_eq!(resolver.resolve(&fqdn).unwrap().owner, next_owner);
}
