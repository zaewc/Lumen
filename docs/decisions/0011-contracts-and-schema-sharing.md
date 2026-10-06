# ADR-0011: Make Rust the source of truth for versioned contracts

- Status: Accepted
- Date: 2026-10-06

## Context and problem statement

Domain data crosses many boundaries: Rust ↔ TypeScript (Tauri IPC and the local HTTP
API), Rust ↔ Swift/Kotlin (UniFFI), Rust ↔ disk (SQLite, quarantine manifests,
policy files, golden datasets), and Rust ↔ AI models (Jev request and response
schemas). Hand-maintained duplicates drift, and drift in safety-relevant state
(decisions, plan IDs, sizes) is dangerous.

## Decision drivers

- One definition per type, generated everywhere else.
- Runtime validation at every untrusted edge.
- Explicit, reviewable schema evolution.
- 64-bit integers (byte counts, file IDs) must survive JSON.

## Considered options

1. Rust types as the source of truth: `schemars` JSON Schema for persisted and AI
   formats, `utoipa` OpenAPI for the local API, generated TypeScript client and Zod
   schemas, UniFFI for Swift/Kotlin.
2. `ts-rs` TypeScript types only.
3. `tauri-specta` for everything.
4. Hand-written types per language.

## Decision outcome

Chosen option: **1**.

- **Persisted and AI contracts** (quarantine manifest, operation journal records,
  policy rule files, evidence bundles, Jev request/response, golden dataset cases):
  `schemars` 1.x JSON Schema 2020-12, exported by `xtask` into `schemas/`, carrying an
  explicit `schema_version`, and snapshot-tested with `insta` so any change appears
  as a reviewable diff.
- **Local API**: `utoipa` emits OpenAPI (pinned to 3.1 until generator support for
  3.2 is confirmed). The committed spec generates `packages/api-client` (TypeScript
  types, Zod schemas, `fetch` SDK) with an exactly pinned generator. CI fails on
  drift. The same operations run over Tauri IPC via `TauriTransport`.
- **Mobile**: UniFFI records and enums are exported from `lumen-ffi`.
- `tauri-specta` is used, if at all, only for the few shell-level commands, pinned to
  an exact release candidate.
- **Encoding rules**: `u64` and `i64` values (bytes, file IDs, inode numbers) are
  encoded as decimal strings in JSON. Timestamps are RFC 3339 UTC. Identifiers are
  typed (branded in TypeScript) and never bare strings in domain code. Paths cross
  boundaries as display-escaped strings plus an opaque handle; raw bytes stay in Rust.
- **Evolution**: additive changes bump the minor schema version; breaking changes bump
  the major and require a migration or reader for the previous major. Persisted data
  is never rewritten in place without a migration.

### Consequences

- Good: drift becomes a CI failure.
- Good: Zod validation at the UI edge comes for free.
- Bad: a codegen step in the build (`just gen`).

Rejected: option 2 has no runtime validation and is TypeScript-only; option 3 depends
on a long-running release candidate; option 4 guarantees drift.

## More information

- [Rust ecosystem research, R6](../research/02-rust-ecosystem.md)
- [Web and monorepo research, Implication 3](../research/07-web-and-monorepo.md)
