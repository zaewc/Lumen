# ADR-0003: Use a flat Cargo workspace on Rust 2024 with a pinned toolchain

- Status: Accepted
- Date: 2026-10-06

## Context and problem statement

The Rust core is shared by every host. Its workspace layout, toolchain and lint policy
determine build speed, how strongly dependency direction is enforced, and how easily
agents can find code.

## Decision drivers

- Enforce the hexagonal dependency direction (ADR-0002) at compile time.
- Reproducible builds across macOS, Windows, Linux CI, Android NDK and iOS targets.
- Strict, consistent linting without fighting noisy lints.
- Compatibility with Tauri 2.12 (MSRV 1.90) and mobile toolchains.

## Considered options

1. Flat `crates/` directory, virtual manifest, crate folder name = crate name, `xtask`.
2. Nested hierarchy (`crates/platform/macos/...`).
3. A single crate with modules.

## Decision outcome

Chosen option: **1**, the layout used by rust-analyzer, uv and ruff.

- Virtual root manifest with `resolver = "3"` set explicitly (a virtual manifest has no
  edition to infer it from). Resolver 3 enables MSRV-aware dependency selection.
- `[workspace.package]`: `edition = "2024"`, `version = "0.0.0"`, `publish = false`,
  a single `rust-version`.
- `rust-toolchain.toml` pins the toolchain (`1.99.0` at bootstrap) with `clippy`,
  `rustfmt`, and the Android and iOS targets. Upgrades are their own PRs.
- MSRV is set deliberately at bootstrap (initial floor 1.95, driven by
  `rusqlite_migration` 2.6 and the upcoming `windows` 0.100) and tested in CI with
  `cargo hack --rust-version`.
- `[workspace.dependencies]` lists every external and internal crate once; members use
  `{ workspace = true }`.
- `[workspace.lints]`: `unsafe_code = "deny"` (platform crates opt in per module, with
  `clippy::undocumented_unsafe_blocks` required), clippy `pedantic` at warn with a
  curated allow-list, and cherry-picked `restriction` lints (`unwrap_used`,
  `expect_used`, `panic`, `dbg_macro`, `print_stdout`, `print_stderr`, `exit`). CI
  runs clippy with `-D warnings`. `restriction` and `nursery` are never enabled
  wholesale.
- `clippy::disallowed_methods` bans `std::fs::remove_*`, `std::fs::rename` and similar
  everywhere except `lumen-fs-exec` (ADR-0016).
- `Cargo.lock` is committed; all builds use `--locked`.
- `xtask` hosts repository automation (schema export, codegen, release checks).

Initial crate map (crates are created only when their first code lands):

| Crate | Layer | May depend on |
| --- | --- | --- |
| `lumen-domain` | domain | std, serde, schemars, thiserror |
| `lumen-policy` | domain service | `lumen-domain` |
| `lumen-graph` | domain service | `lumen-domain` |
| `lumen-application` | application (use cases + ports) | domain, policy, graph |
| `lumen-testkit` | test support | application (fakes, proptest strategies, fixture trees) |
| `lumen-scan` | adapter (scan engine) | application |
| `lumen-fs-exec` | adapter (sole destructive executor) | application, rustix / windows-sys |
| `lumen-store-sqlite` | adapter | application, rusqlite |
| `lumen-platform-{macos,windows,android,ios}` | adapters | application, scan |
| `lumen-devkb` | adapter (developer-artifact knowledge base) | application |
| `lumen-jev`, `lumen-jev-eval` | adapter, tool | application |
| `lumen-telemetry` | adapter | tracing |
| `lumen-ffi` | composition root (UniFFI) | application + adapters |
| `lumen-app` | composition root | everything above |
| `lumen-cli` | developer CLI | `lumen-app` |

### Consequences

- Good: the compiler proves the policy crate cannot perform IO.
- Good: incremental builds stay fast; crate boundaries are review boundaries.
- Bad: more `Cargo.toml` files; mitigated by workspace inheritance.

## More information

- [Rust ecosystem research, R1, R8](../research/02-rust-ecosystem.md)
- [Security research, §A.1](../research/09-security-devops-quality.md)
