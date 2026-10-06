# ADR-0002: Build a modular monolith with a hexagonal Rust core

- Status: Accepted
- Date: 2026-10-06

## Context and problem statement

Lumen must run the same safety-critical logic (evidence collection, graph building,
policy evaluation, quarantine bookkeeping) on macOS, Windows, Android, and iOS, with
different UIs and very different platform capabilities. The logic that decides whether
a user's file may be moved must be identical everywhere, testable without a real
filesystem, and impossible to bypass from a UI or from AI output.

## Decision drivers

- One implementation of the safety policy for every platform.
- Platform details (APFS clones, NTFS placeholders, MediaStore, PhotoKit) must not leak
  into the domain model.
- The policy must be provably IO-free and deterministic.
- Complexity must be earned: no distributed system for a single-device product.
- Small crates with enforced dependency direction, so AI agents can change one piece
  at a time.

## Considered options

1. Modular monolith: Rust core organised as hexagonal layers (domain → application
   ports → adapters), compiled into each host (desktop shell, agent, mobile library).
2. Per-platform native implementations (Swift, C#, Kotlin) sharing only a spec.
3. Local microservices (scanner service, policy service, AI service) talking over IPC.

## Decision outcome

Chosen option: **1**.

- **Domain** (`lumen-domain`): pure types, identifiers, value objects, invariants and
  state machines. No IO, no async runtime, no platform code.
- **Policy** (`lumen-policy`) and **evidence graph** (`lumen-graph`): depend only on
  the domain. The policy crate cannot touch the filesystem by construction, because it
  has no dependency that can.
- **Application** (`lumen-application`): use cases (scan, plan, quarantine, verify,
  rollback) and the **ports** (traits) they need: `DirEnumerator`, `ChangeFeed`,
  `InventorySource`, `InUseProbe`, `QuarantineStore`, `Store`, `JudgeModel`, `Clock`.
- **Adapters**: platform crates, the SQLite store, the destructive-operation executor,
  Jev providers, and telemetry implement those ports.
- **Composition roots** (`lumen-app`, `lumen-ffi`, the Tauri host) wire adapters to use
  cases. `#[cfg(target_os)]` appears only in adapters and composition roots, never in
  the domain, policy, graph or application crates.

Process boundaries exist only where a trust or privilege boundary requires them
(see ADR-0005), not as a modularity tool.

### Consequences

- Good: one audited policy implementation; exhaustive property tests run on any host.
- Good: adapters are replaceable (e.g. a native SwiftUI shell later) without touching
  the core.
- Good: fakes for every port make use cases testable on a temporary filesystem.
- Bad: mobile and desktop UIs reach the core through bindings (UniFFI, Tauri IPC),
  which adds a contract layer to maintain (ADR-0011).
- Bad: Rust expertise is required for core changes.

Rejected:

- Option 2 forks the safety logic into three or four languages and guarantees drift.
- Option 3 adds IPC, versioning and failure modes with no benefit on a single device.

## More information

- [Rust ecosystem research, R1–R2](../research/02-rust-ecosystem.md)
- [Desktop architecture research](../research/01-desktop-architecture.md)
- ADR-0003 (workspace layout), ADR-0005 (process topology)
