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
