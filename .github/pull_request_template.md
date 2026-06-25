## Summary

<!-- Describe what this PR changes and why. -->

Closes #<!-- issue number -->

## Change type

- [ ] Bug fix
- [ ] New feature
- [ ] Refactor / cleanup
- [ ] Documentation
- [ ] CI / tooling
- [ ] Contract change

## Areas affected

- [ ] CLI (`cli/`)
- [ ] Contracts (`contracts/`)
- [ ] SDK (`packages/xlm-ns-sdk/`)
- [ ] Common types (`packages/xlm-ns-common/`)
- [ ] CI / GitHub Actions
- [ ] Docs

## Testing

- [ ] `cargo test --workspace` passes
- [ ] `cargo fmt --all --check` passes
- [ ] `cargo clippy --workspace -- -D warnings` passes
- [ ] Contract changes: `cargo build --release --target wasm32v1-none` succeeds
- [ ] New functionality covered by tests or snapshots
- [ ] Manually verified on testnet (for contract / CLI changes)

## Rollout notes

<!-- Anything reviewers or deployers should know: breaking changes, migration steps, env var changes. -->
<!-- Delete this section if not applicable. -->

## Checklist

- [ ] Self-reviewed the diff
- [ ] No secrets or credentials committed
- [ ] PR title is descriptive
- [ ] Linked the issue above
