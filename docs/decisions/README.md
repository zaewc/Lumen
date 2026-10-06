# Architecture decision records

Each significant architecture decision is recorded as an ADR. See
[ADR-0001](0001-record-architecture-decisions.md) for the format and rules, and
[`0000-template.md`](0000-template.md) to start a new one.

`Proposed` ADRs await the project owner's confirmation; work proceeds on the
recommended option.

| ADR | Title | Status |
| --- | --- | --- |
| [0001](0001-record-architecture-decisions.md) | Record architecture decisions | Accepted |
| [0002](0002-modular-monolith-hexagonal-rust-core.md) | Build a modular monolith with a hexagonal Rust core | Accepted |
| [0003](0003-rust-workspace-and-toolchain.md) | Use a flat Cargo workspace on Rust 2024 with a pinned toolchain | Accepted |
| [0004](0004-desktop-shell-tauri.md) | Use Tauri 2 as the desktop shell with the core in the host process | Accepted |
| [0005](0005-process-topology-and-privilege.md) | Run user-scope only in v1; add an agent and a privileged helper later | Accepted |
| [0006](0006-desktop-distribution-and-signing.md) | Distribute desktop builds outside the app stores, signed and notarized | Proposed |
| [0007](0007-mobile-expo-native-modules-uniffi.md) | Build mobile with Expo, native Expo Modules, and the Rust core via UniFFI | Accepted |
| [0008](0008-capability-model-and-coverage.md) | Model platform capabilities and scan coverage explicitly | Accepted |
| [0009](0009-frontend-vite-react-spa.md) | Build the UI as one Vite + React SPA with Feature-Sliced Design | Accepted |
| [0010](0010-web-dashboard-role.md) | Make the web dashboard local-only and off by default; defer any cloud backend | Proposed |
| [0011](0011-contracts-and-schema-sharing.md) | Make Rust the source of truth for versioned contracts | Accepted |
| [0012](0012-local-persistence-sqlite.md) | Persist locally in SQLite with separate index and ledger databases | Accepted |
| [0013](0013-evidence-graph-relational.md) | Store the evidence graph relationally with immutable, hashed evidence | Accepted |
| [0014](0014-deterministic-safety-policy-engine.md) | Decide every action with a deterministic, versioned safety policy | Accepted |
| [0015](0015-quarantine-and-reversible-cleanup.md) | Quarantine by journaled same-volume rename into a Lumen-owned store | Accepted |
| [0016](0016-single-handle-relative-executor.md) | Route every destructive filesystem operation through one handle-relative executor | Accepted |
| [0017](0017-scan-engine-and-size-accounting.md) | Scan with native batch enumerators on a work-stealing pool and account sizes precisely | Accepted |
| [0018](0018-incremental-scanning-change-feeds.md) | Scan incrementally from persisted change feeds with periodic full sweeps | Accepted |
| [0019](0019-staged-duplicate-detection.md) | Detect duplicates in stages with a cryptographic content ID | Accepted |
| [0020](0020-jev-evidence-only-judge.md) | Integrate Jev as an optional, evidence-only judge behind a port | Accepted |
| [0021](0021-developer-artifact-knowledge-base.md) | Drive Developer Mode from a researched knowledge base that prefers tool-native cleanup | Accepted |
| [0022](0022-privacy-preserving-observability.md) | Observe locally with tracing; make OpenTelemetry export opt-in and content-free | Accepted |
| [0023](0023-ipc-security.md) | Secure every IPC boundary with authenticated peers and plan-ID commands | Accepted |
| [0024](0024-polyglot-monorepo-tooling.md) | Use pnpm workspaces, Turborepo and a justfile alongside the Cargo workspace | Accepted |
