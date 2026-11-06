# AWS Pick - Agent Prompt

> Purpose: rules for developing and maintaining the `awspick` CLI. Users must
> be able to switch AWS profiles quickly.

---

## Rules

1. Update README.md whenever behaviour changes.
2. Every commit passes `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D warnings` and `cargo test --locked`.
3. Use Conventional Commits (e.g. `docs: update README.md`). release-please derives the version and CHANGELOG from them.
4. Keep the dependency set small; justify each new crate. `cargo deny check` must pass (deny.toml).
5. awspick writes to users' rc files. Changes there need a test that runs the binary against a throwaway HOME (tests/cli.rs, tests/golden.rs), never against the real one.
6. Behaviour that matches awspick 2.0.2 (Python) is pinned by tests/golden. A change that alters a golden case must say why in docs/rust-migration.md.

## Environment

- Rust stable (edition 2024, rust-version in Cargo.toml)
- Tests: `cargo test`. Golden fixtures are regenerated with `uv run scripts/gen_golden.py` (needs uv; runs PyPI aws-profile-pick 2.0.2).

## Release Management

- release-please (`release-type: rust`, `include-component-in-tag: false`) in `.github/workflows/release-please.yml` owns the version in Cargo.toml and Cargo.lock and creates the `v<version>` tag and GitHub release.
- The tag push runs `.github/workflows/release.yml`: binaries for four targets, attestations, crates.io (Trusted Publishing, environment `release`) and the Homebrew formula in KKamJi98/homebrew-tap.
- Do not tag by hand and do not edit the version by hand.
