//! Storage-growth and hot-path-write benchmarks for the contract workspace.
//!
//! These tests are marked `#[ignore]` so they are excluded from default
//! `cargo test` runs. Invoke them explicitly:
//!
//!     cargo test --test storage_benchmarks -- --ignored --nocapture
//!
//! or via `scripts/run-benchmarks.sh`, which sets the right flags and writes
//! the report to `target/bench-report.txt`.
//!
//! Each benchmark resets the SDK budget tracker, runs N iterations of a
//! single hot-path operation, and prints the per-call CPU/memory cost
//! averaged across iterations. The output also reports the *trend* in cost
//! across the run, so reviewers can spot non-flat storage growth without
//! needing absolute numbers from any particular SDK release.
//!
//! The numbers are not network costs — they are SDK-side budget counters
//! that scale linearly with the host's accounting. They are intended for
//! relative comparison ("did my change make `register` 2x more expensive?")
//! not for predicting on-chain fees.

use soroban_sdk::{testutils::Address as _, Address, Env, String};
use xlm_ns_registry::{RegistryContract, RegistryContractClient};
use xlm_ns_resolver::{ResolverContract, ResolverContractClient};

const ITERATIONS: u32 = 25;

struct CostSample {
    cpu: u64,
    mem: u64,
}

/// Run `op` for ITERATIONS, resetting the SDK budget tracker before each
/// call, and collect per-iteration CPU and memory costs. Returns the raw
/// per-iteration samples; the caller renders the summary.
fn measure<F: FnMut(u32)>(env: &Env, label: &str, mut op: F) -> Vec<CostSample> {
    let mut samples = Vec::with_capacity(ITERATIONS as usize);
    for i in 0..ITERATIONS {
        env.cost_estimate().budget().reset_default();
        op(i);
        let cpu = env.cost_estimate().budget().cpu_instruction_cost();
        let mem = env.cost_estimate().budget().memory_bytes_cost();
        samples.push(CostSample { cpu, mem });
        let _ = i;
    }
    println!("\n=== {} ({} iterations) ===", label, ITERATIONS);
    report(&samples);
    samples
}

fn report(samples: &[CostSample]) {
    let n = samples.len() as u64;
    let cpu_sum: u64 = samples.iter().map(|s| s.cpu).sum();
    let mem_sum: u64 = samples.iter().map(|s| s.mem).sum();
    let cpu_first = samples.first().map(|s| s.cpu).unwrap_or(0);
    let cpu_last = samples.last().map(|s| s.cpu).unwrap_or(0);
    let mem_first = samples.first().map(|s| s.mem).unwrap_or(0);
    let mem_last = samples.last().map(|s| s.mem).unwrap_or(0);
    println!(
        "  avg cpu: {:>10}   avg mem: {:>10}",
        cpu_sum / n.max(1),
        mem_sum / n.max(1)
    );
    println!(
        "  first cpu: {:>8}    last cpu: {:>8}    delta: {:+}",
        cpu_first,
        cpu_last,
        cpu_last as i64 - cpu_first as i64
    );
    println!(
        "  first mem: {:>8}    last mem: {:>8}    delta: {:+}",
        mem_first,
        mem_last,
        mem_last as i64 - mem_first as i64
    );
}

fn label_name(env: &Env, i: u32) -> String {
    // Labels must be lowercase ASCII / digits, length >= 3.
    String::from_str(env, &format!("bench{i:04}.xlm"))
}

#[test]
#[ignore]
fn bench_registry_register_renew_transfer() {
    let env = Env::default();
    env.mock_all_auths();

    let registry_id = env.register(RegistryContract, ());
    let registry = RegistryContractClient::new(&env, &registry_id);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    // Pre-register N names so the per-call cost of register / renew /
    // transfer reflects realistic state, not a cold contract.
    let now: u64 = 1_000_000;
    let expires_at = now + 100_000;
    let grace_end = expires_at + 1_000;

    measure(&env, "registry::register (fresh)", |i| {
        let name = label_name(&env, i);
        registry.register(&name, &alice, &None, &None, &now, &expires_at, &grace_end);
    });

    measure(&env, "registry::renew", |i| {
        let name = label_name(&env, i);
        // Extend the expiry window in-place. Signature is
        // (name, caller, expires_at, grace_period_ends_at, now_unix).
        let new_expiry = expires_at + 200_000;
        let new_grace = new_expiry + 1_000;
        registry.renew(&name, &alice, &new_expiry, &new_grace, &now);
    });

    measure(&env, "registry::transfer (alice -> bob)", |i| {
        let name = label_name(&env, i);
        registry.transfer(&name, &alice, &bob, &now);
    });
}

#[test]
#[ignore]
fn bench_resolver_set_record_mutation() {
    let env = Env::default();
    env.mock_all_auths();

    // Resolver mutations require a backing registry entry so the
    // unauthorized-owner branch is not exercised.
    let registry_id = env.register(RegistryContract, ());
    let registry = RegistryContractClient::new(&env, &registry_id);
    let resolver_id = env.register(ResolverContract, ());
    let resolver = ResolverContractClient::new(&env, &resolver_id);
    resolver.initialize(&registry_id);

    let owner = Address::generate(&env);
    let now: u64 = 1_000_000;
    let expires_at = now + 100_000;
    let grace_end = expires_at + 1_000;

    for i in 0..ITERATIONS {
        let name = label_name(&env, i);
        registry.register(&name, &owner, &None, &None, &now, &expires_at, &grace_end);
    }

    measure(&env, "resolver::set_record (first write)", |i| {
        let name = label_name(&env, i);
        let address = String::from_str(
            &env,
            "GAAA000000000000000000000000000000000000000000000000000000",
        );
        resolver.set_record(&name, &owner, &address, &now);
    });

    measure(&env, "resolver::set_record (overwrite)", |i| {
        let name = label_name(&env, i);
        let address = String::from_str(
            &env,
            "GBBB000000000000000000000000000000000000000000000000000000",
        );
        resolver.set_record(&name, &owner, &address, &now);
    });
}
