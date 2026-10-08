# Polyglot task entry point (ADR-0024). Run `just` to list recipes.
# Prerequisites: rustup (toolchain from rust-toolchain.toml), cargo-nextest,
# cargo-deny. JavaScript recipes are added with the first TypeScript code.
# `cargo xtask` is an alias defined in .cargo/config.toml.

set shell := ["bash", "-euo", "pipefail", "-c"]

# List available recipes.
default:
    @just --list

# Format all Rust code.
fmt:
    cargo fmt --all

# Verify formatting without changing files.
fmt-check:
    cargo fmt --all --check

# Lint with clippy; warnings are errors (ADR-0025).
lint:
    cargo clippy --workspace --all-targets --locked -- -D warnings

# Run unit, integration and doc tests.
test:
    cargo nextest run --workspace --locked --no-tests=warn
    cargo test --workspace --doc --locked

# Check dependencies: advisories, licenses, bans, sources (ADR-0026).
deny:
    cargo deny --all-features --locked check

# Regenerate committed artifacts (JSON Schemas, ADR-0011).
gen:
    cargo xtask schema

# Fail if committed schemas differ from the Rust types.
schema-check:
    cargo xtask schema --check

# Everything CI checks for Rust; run before every commit.
check: fmt-check lint test deny schema-check
