# Polyglot task entry point (ADR-0024). Run `just` to list recipes.
# Prerequisites: rustup (toolchain from rust-toolchain.toml), cargo-nextest,
# cargo-deny; Node.js 24 with Corepack enabled (`corepack enable`) for the
# JavaScript workspace (pnpm comes from `packageManager`).
# `cargo xtask` is an alias defined in .cargo/config.toml.

set shell := ["bash", "-euo", "pipefail", "-c"]

# Turborepo sends anonymous usage data unless told not to (ADR-0022).
export TURBO_TELEMETRY_DISABLED := "1"

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

# Run the criterion benchmarks. Not part of `check`: results depend on the
# machine. Baselines: docs/architecture/performance.md.
bench:
    cargo bench --workspace --locked

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

# Install the JavaScript workspace exactly as locked (ADR-0024).
js-install:
    pnpm install --frozen-lockfile

# Format, lint and type-check the JavaScript workspace (ADR-0025).
js-check: js-install
    pnpm run check

# Build the on-device judge helper for `lumen-jev-eval record --judge apple-fm`
# (ADR-0044). macOS 26 with Xcode 26 or later; not part of `check`, which also
# runs on Linux and Windows. CI builds and tests it on macOS (fm-judge.yml).
fm-judge:
    swift build -c release --package-path apps/fm-judge

# Lint and test the on-device judge helper; its tests need no model.
fm-judge-test:
    xcrun swift-format lint --strict -r apps/fm-judge/Package.swift apps/fm-judge/Sources apps/fm-judge/Tests
    swift test --package-path apps/fm-judge

# Everything CI checks; run before every commit.
check: fmt-check lint test deny schema-check js-check
