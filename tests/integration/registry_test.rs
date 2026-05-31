use soroban_sdk::{testutils::Address as _, Address, Env, String};
use xlm_ns_registry::{RegistryContract, RegistryContractClient};
use xlm_ns_resolver::{ResolverContract, ResolverContractClient};

#[test]
fn test_reclaim_and_burn_flow() {
    let env = Env::default();
    env.mock_all_auths();

    let registry_id = env.register(RegistryContract, ());
    let registry = RegistryContractClient::new(&env, &registry_id);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let name = String::from_str(&env, "alice.xlm");

    let now = 1_000_000;
    let expires_at = now + 1000;
    let grace_period_ends_at = expires_at + 1000;

    // 1. Alice registers a name
    registry.register(
        &name,
        &alice,
        &None,
        &None,
        &now,
        &expires_at,
        &grace_period_ends_at,
    );

    let entry = registry.resolve(&name, &now);
    assert_eq!(entry.owner, alice);

    // 2. Name expires and goes past grace period
    let future_now = grace_period_ends_at + 1;

    // 3. Bob reclaims the name, this should emit a burn event for Alice
    // and register for Bob.
    let new_expires_at = future_now + 1000;
    let new_grace_period = new_expires_at + 1000;
    registry.register(
        &name,
        &bob,
        &None,
        &None,
        &future_now,
        &new_expires_at,
        &new_grace_period,
    );

    let reclaimed_entry = registry.resolve(&name, &future_now);
    assert_eq!(reclaimed_entry.owner, bob);

    // 4. Bob burns his active name
    registry.burn(&name, &bob, &future_now);

    // Resolving should now fail
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        registry.resolve(&name, &future_now);
    }));
    assert!(result.is_err(), "resolve should fail after burn");

    // 5. Test that anyone can burn a claimable name
    let charlie = Address::generate(&env);
    let dave = Address::generate(&env);
    registry.register(
        &name,
        &charlie,
        &None,
        &None,
        &future_now,
        &new_expires_at,
        &new_grace_period,
    );

    let claimable_now = new_grace_period + 1;

    // Dave burns Charlie's claimable name
    registry.burn(&name, &dave, &claimable_now);

    let result2 = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        registry.resolve(&name, &claimable_now);
    }));
    assert!(result2.is_err(), "resolve should fail after claimable burn");
}

#[test]
fn test_transfer_invalidates_resolver_authority() {
    let env = Env::default();
    env.mock_all_auths();

    let registry_id = env.register(RegistryContract, ());
    let resolver_id = env.register(ResolverContract, ());

    let registry = RegistryContractClient::new(&env, &registry_id);
    let resolver = ResolverContractClient::new(&env, &resolver_id);

    // Wire the resolver to the registry for live canonical ownership checks
    resolver.initialize(&registry_id);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let name = String::from_str(&env, "alice.xlm");
    let address_a = String::from_str(&env, "G_ALICE");
    let address_b = String::from_str(&env, "G_BOB");
    let now = 1_000_000;

    registry.register(
        &name,
        &alice,
        &None,
        &None,
        &now,
        &(now + 10_000),
        &(now + 20_000),
    );

    // Alice can set the record
    resolver.set_record(&name, &alice, &address_a, &now);

    // Transfer ownership to Bob
    registry.transfer(&name, &alice, &bob, &now);

    // Alice's stale authority should be instantly invalidated
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        resolver.set_record(&name, &alice, &address_a, &now);
    }));
    assert!(
        result.is_err(),
        "stale owner should not be able to mutate resolver"
    );

    // Bob can successfully mutate the record with derived canonical authority
    resolver.set_record(&name, &bob, &address_b, &now);
}
