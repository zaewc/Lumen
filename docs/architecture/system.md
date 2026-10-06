# Lumen system architecture

> Status: accepted architecture baseline, 2026-10-06. Decisions are recorded in
> [`docs/decisions/`](../decisions/README.md); research in
> [`docs/research/`](../research/). This document summarises how the pieces fit;
> when it disagrees with an ADR, the ADR wins and this document must be fixed.

## 1. Purpose

Lumen answers two questions on every device it runs on:

1. *What is consuming my storage?*
2. *What can I safely remove, and why?*

It answers them by collecting evidence, building relationships, and applying a
deterministic safety policy. Every removal is reversible where the platform allows,
and is explained.

Non-goals:

- "Speeding up" devices or registry cleaning.
- Deleting anything without explicit user confirmation of a specific plan.
- Cleaning other apps' data on iOS.
- Any action the platform does not support (see [platforms](platforms.md)).

## 2. Product loop

```text
DISCOVER → SCAN → COLLECT EVIDENCE → BUILD RELATIONSHIPS → CLASSIFY
        → (optional) JEV JUDGMENT → SAFETY POLICY → EXPLAIN
        → USER CONFIRMS PLAN → QUARANTINE → VERIFY → RETAIN → FINALIZE | ROLLBACK
```

Outcomes are `KEEP`, `REVIEW` or `QUARANTINE`. There is no `REMOVE` verdict. Permanent
deletion happens only when quarantine is finalized, after retention or on explicit
request.

## 3. Principles

| Principle | Mechanism |
| --- | --- |
| Safety over reclaimed space | Hard-protection stage in the policy; unknown → `KEEP`/`REVIEW` ([ADR-0014](../decisions/0014-deterministic-safety-policy-engine.md)) |
| AI is evidence, never authority | `JudgeModel` port, closed vocabulary, monotone-safety contract ([ADR-0020](../decisions/0020-jev-evidence-only-judge.md)) |
| Reversible by default | Journaled same-volume quarantine ([ADR-0015](../decisions/0015-quarantine-and-reversible-cleanup.md)) |
| No path confusion | Single handle-relative executor ([ADR-0016](../decisions/0016-single-handle-relative-executor.md)) |
| Blind spots are visible | Capability model and coverage reports ([ADR-0008](../decisions/0008-capability-model-and-coverage.md)) |
| Honest numbers | `SizeFacts` and `reclaimable_now` ([ADR-0017](../decisions/0017-scan-engine-and-size-accounting.md)) |
| Local-first, private | No content upload; telemetry opt-in and content-free ([ADR-0022](../decisions/0022-privacy-preserving-observability.md)) |
| Earned complexity | Modular monolith ([ADR-0002](../decisions/0002-modular-monolith-hexagonal-rust-core.md)) |

## 4. Context

```text
                 ┌──────────────────────────────────────────────────────────┐
  User ─────────▶│ Lumen UI (Tauri webview / browser dashboard / Expo app)  │
                 └───────────────┬──────────────────────────────────────────┘
                                 │ view models, plan IDs (ADR-0023)
                 ┌───────────────▼──────────────────────────────────────────┐
                 │ Lumen core (Rust): application use cases                 │
                 │  scan · evidence · graph · policy · plan · quarantine    │
                 └──┬────────────┬──────────────┬───────────────┬───────────┘
                    │ ports      │              │               │ optional, opt-in
        ┌───────────▼───┐ ┌──────▼──────┐ ┌─────▼──────┐ ┌──────▼──────────────┐
        │ OS APIs       │ │ SQLite      │ │ Developer  │ │ Jev providers        │
        │ (fs, inventory│ │ index.db    │ │ tools (CLI │ │ on-device / cloud    │
        │  processes)   │ │ ledger.db   │ │ cleanup)   │ │ (metadata only)      │
        └───────────────┘ └─────────────┘ └────────────┘ └─────────────────────┘
```

## 5. Building blocks

### 5.1 Layers and crates

The full crate map is in [ADR-0003](../decisions/0003-rust-workspace-and-toolchain.md).
Crates are created only when their first code lands.

```text
                      lumen-domain   (types, IDs, invariants, state machines)
                       ▲        ▲
              lumen-policy    lumen-graph     (pure, IO-free)
                       ▲        ▲
                    lumen-application         (use cases + ports)
        ▲            ▲            ▲             ▲              ▲
 lumen-scan   lumen-fs-exec  lumen-store-sqlite  lumen-jev   lumen-platform-*
 lumen-devkb  lumen-telemetry
        ▲            ▲            ▲             ▲              ▲
             lumen-app  ·  lumen-ffi  ·  apps/desktop (Tauri host)  ·  lumen-cli
```

Dependency rules (enforced by Cargo, and checked in CI):

- `lumen-domain`, `lumen-policy` and `lumen-graph` have no IO, no async runtime, no
  platform code and no `#[cfg(target_os)]`.
- `lumen-application` defines ports; it depends on no adapter.
- Adapters depend on `lumen-application` (and on `lumen-scan` for platform
  enumerators), never on each other, except for test support.
- Only composition roots select platform adapters.
- Only `lumen-fs-exec` may call destructive filesystem APIs.

### 5.2 Ports

| Port | Responsibility | Desktop adapters |
| --- | --- | --- |
| `DirEnumerator` | Batched directory metadata → `EntryBatch` with `SizeFacts` and access state | `getattrlistbulk`, `FileIdExtdDirectoryInfo`, `getdents64` + `statx`, `std::fs` fallback |
| `ChangeFeed` | Persisted change streams with resume points | FSEvents, USN, `ReadDirectoryChangesW`, inotify |
| `InventorySource` | Applications, packages, processes, services, startup items | per OS (bundles + `pkgutil`, launchd plists; Uninstall keys, MSI, AppX, Run keys, Task Scheduler, SCM) |
| `InUseProbe` | Is an item open or owned by a running process? | `libproc`; Restart Manager |
| `QuarantineStore` | Per-volume stores, manifests, retention | `lumen-fs-exec` |
| `Executor` | Plan execution by handle with identity verification | `lumen-fs-exec` |
| `ToolRunner` | Run vetted developer-tool cleanup commands as the user | `lumen-devkb` |
| `Store` | Persist scans, graph, decisions, ledger | `lumen-store-sqlite` |
| `JudgeModel` | Optional evidence from AI | `lumen-jev` |
| `Clock`, `IdGenerator` | Deterministic time and IDs in tests | std / fakes |

Ports for filesystem and OS inspection are **synchronous**. They run on the scan pool
and are orchestrated by Tokio. Only network- or model-bound ports (`JudgeModel`) are
async.

## 6. Domain model

All types live in `lumen-domain` and are serialisable with versioned JSON Schemas
([ADR-0011](../decisions/0011-contracts-and-schema-sharing.md)).

### 6.1 Identifiers

Identifiers are typed newtypes, never bare strings or integers:

| Type | Meaning |
| --- | --- |
| `DeviceId` | Random per install |
| `VolumeId` | Volume UUID / serial |
| `FileIdentity` | `(VolumeId, file_id: u128)`: inode or NTFS FileId |
| `NodeId` | Graph node |
| `EvidenceId` | BLAKE3 of the canonical evidence record |
| `EvidenceHash` | Merkle root of an evidence bundle |
| `ScanId`, `SnapshotId`, `PlanId`, `OperationId` | Lifecycle entities |
| `PolicyVersion`, `PromptVersion`, `SchemaVersion` | Semantic versions |

Paths are stored as raw bytes (`OsString` semantics) plus an escaped display form.
Filesystem identity, not path text, is the key for every decision.

### 6.2 Core entities

| Entity | Key fields |
| --- | --- |
| `Device` | id, platform, OS version, capabilities |
| `Platform` | `macos`, `windows`, `android`, `ios`; and `PlatformCapabilities` |
| `Volume` | id, mount points, filesystem, case and normalisation behaviour, supports no-replace rename, supports clones |
| `FilesystemEntry` | identity, parent, name bytes, kind, `SizeFacts`, times, flags, `AccessState` |
| `Application` | bundle / package ID, name, version, vendor, signing team, install locations, sources |
| `Process` | pid, executable identity, owning application, start time |
| `Service` | kind (launch agent/daemon, Windows service, scheduled task, startup entry), definition location, target executable, enabled state |
| `Package` | package manager or installer, identifiers, owned paths |
| `Artifact` | A classified thing worth explaining: one entry or a group (e.g. "Xcode DerivedData for project X"), with a category |
| `Evidence` | Immutable fact with provenance ([§7](#7-evidence-graph)) |
| `Relationship` | Typed, directed edge between nodes |
| `CoverageReport` | Roots and sources observed, denied, truncated |

### 6.3 Decision entities

`CleanupCandidate` is what users review:

```text
CleanupCandidate
 ├── target            Artifact ref (+ FileIdentity set)
 ├── size              SizeFacts aggregate (logical, allocated, reclaimable_now, after_snapshots)
 ├── category          e.g. app_cache, developer_build_output, log, duplicate, orphaned_service
 ├── evidence[]        EvidenceId list
 ├── relationships[]   edges used by the explanation
 ├── regeneratable     Yes | No | Unknown (with evidence)
 ├── currently_in_use  Yes | No | Unknown
 ├── protected         bool + protection rule id
 ├── risk              Low | Medium | High | Critical + factors
 ├── confidence        rule-based confidence; Jev calibrated confidence kept separately
 ├── policy_decision   PolicyDecision
 └── reversible        Reversible(mechanism) | Irreversible(reason)
```

- `PolicyDecision`: verdict, `policy_version`, `evidence_hash`, fired rules (each with
  the evidence IDs it used), Jev effect, explanation.
- `AIJudgment`: the validated `Judgment` plus its `JevTrace` (model, model version,
  prompt version, policy version, schema versions, evidence hash, timestamp, decision,
  confidence).
- `Recommendation`: the user-facing projection of a decision, with explanation and
  available actions.
- `CleanupAction`: one of `QuarantineMove`, `ToolCommand`, `MediaTrash`,
  `PhotoLibraryDelete`, `CloudEvict`, `OpenSystemSettings`, `ArchiveApp`. Each declares
  its reversibility.
- `CleanupPlan`: an ordered set of actions bound to a `PlanId`, with a plan hash, the
  decisions it relies on, and expected identities.

### 6.4 State machines

Scan:

```text
Created → Running ⇄ Paused → Completed | Cancelled | Failed(partial results kept)
```

Quarantine item ([ADR-0015](../decisions/0015-quarantine-and-reversible-cleanup.md)):

```text
Planned → Journaled → Moved → Verified → Retained → Restored | Finalized
   └─────────── Failed(reason): original untouched or restorable ──────────┘
```

Plan:

```text
Draft → Proposed → Confirmed(user, token) → Executing → Completed | PartiallyCompleted
      → RolledBack (optional)
```

Invariants (property-tested):

- An item cannot reach `Moved` without a durable `Journaled` record.
- `Finalized` is reachable only from `Retained`.
- A plan cannot be `Executing` without `Confirmed`.
- `REVIEW` items are never part of a confirmed plan unless the user individually
  promoted them.

## 7. Evidence graph

See [ADR-0013](../decisions/0013-evidence-graph-relational.md). The graph is a typed
property graph stored in `index.db` (nodes, edges and evidence tables) with an
in-memory adjacency index per analysis.

Example chains:

```text
Application ─creates→ CacheDirectory ─opened_by→ Process ─launched_by→ LaunchAgent
Docker Desktop ─stored_in→ VM disk image ─contains→ images ─contains→ layers
Uninstall entry ─installed_by→ MSI product ─owns→ InstallLocation ─contains→ entries
```

Every evidence record carries its source adapter and API, the observation time, and a
coverage reference. Bundles for policy and Jev are canonically serialised, and their
Merkle root (`EvidenceHash`) is recorded with every decision.

## 8. Safety policy

See [ADR-0014](../decisions/0014-deterministic-safety-policy-engine.md). The stages
run in order: hard protections → eligibility → classification → Jev (monotone) → user
confirmation. The policy is IO-free and versioned; decisions are reproducible from
`(policy_version, evidence_hash)`.

## 9. Explainability

Every `Recommendation` answers, from evidence (or states "unknown" with the reason):

| Question | Source |
| --- | --- |
| What is this? | category + knowledge-base entry + node type |
| Why does it exist? / What created it? | `created_by`, `owned_by`, `installed_by` edges |
| Is anything using it? | `opened_by`, running processes, `launches` edges, in-use probe |
| Can it be regenerated? | knowledge base, vendor-declared cache evidence |
| How much space does it use? | `SizeFacts`: logical, allocated, reclaimable now, after snapshots |
| What happens if I remove it? | knowledge-base consequence text + regeneration cost |
| Can I undo it? | `CleanupAction` reversibility + retention window |
| Why did Lumen recommend this? | fired rules with evidence, Jev effect, coverage caveats |

Explanations are generated from templates, never from AI free text. A Jev rationale,
if present, is shown separately and labelled as AI-generated.

## 10. Scanning

See [ADR-0017](../decisions/0017-scan-engine-and-size-accounting.md) and
[ADR-0018](../decisions/0018-incremental-scanning-change-feeds.md).

- Modes: full, incremental (change feed), targeted (subtree or category), background
  (agent, low priority), cancel, and resume (checkpoints).
- Pipeline: enumerator tasks (rayon pool, per-volume semaphores, priority bands) →
  evidence collector → graph builder → single store writer, with bounded channels
  between stages.
- Partial failure: per-directory errors become `Denied` or `Error` nodes; the scan
  continues and the coverage report records them.
- Never hydrate placeholders, never open contents during scanning, never open archives.

## 11. Cleanup execution

1. The user selects candidates. The core builds a `CleanupPlan`, re-runs the policy on
   fresh evidence, and returns `PlanId` plus the plan hash.
2. The user confirms. The UI sends `execute_plan(plan_id, confirmation_token)`.
3. Per item, the executor re-verifies identity by handle, checks the in-use gate,
   writes a journal intent, performs a no-replace rename into the quarantine store,
   verifies, and records completion. A mismatch aborts that item to `REVIEW`.
4. Retention then finalization, or rollback by operation ID.
5. Tool actions (developer caches) and other irreversible actions are executed
   separately, after a dry run, labelled irreversible.

## 12. Specialised subsystems

### 12.1 Orphan Hunter

The Orphan Hunter finds services, startup entries, launch agents, package remnants and
security modules whose owning application appears to be gone. It produces evidence
cards, never deletions:

```text
Detected third-party component.
Associated application: not found (searched: /Applications, Spotlight, LaunchServices, receipts)
Last activity: 184 days ago          System service: yes
Executable: present (unsigned)       Vendor: unknown
Coverage: complete for user scope; system scope read-only
Risk: HIGH                           Recommendation: REVIEW
```

Rules:

- "Absent" requires positive evidence across all applicable sources with complete
  coverage.
- Security software, kernel or system extensions, and unknown executables are always
  `REVIEW` or `KEEP`.
- Actions are delegated to the OS where possible (System Settings › Login Items,
  `StartupApproved` toggles that the user confirms per item). Lumen never edits the
  macOS BTM database or deletes Run values or tasks.

### 12.2 Developer Mode

Developer Mode is driven by the `lumen-devkb` knowledge base
([ADR-0021](../decisions/0021-developer-artifact-knowledge-base.md)): Xcode
DerivedData and simulators, Docker, `node_modules`, pnpm, npm and Yarn caches, Cargo,
Gradle, CocoaPods, Next.js build output, Android build outputs, IDE caches, AI tool
caches and Homebrew. The tool's own cleanup command is preferred; self-managed caches
are shown as informational.

### 12.3 Duplicates

See [ADR-0019](../decisions/0019-staged-duplicate-detection.md). Exact duplicates are
found in stages ending in a BLAKE3 content ID. Near-duplicates are `REVIEW` only.

### 12.4 Storage forecasting

Forecasts use the history of `ScanSnapshot` volume totals:

- They require at least 5 snapshots spanning at least 14 days.
- Growth rate comes from a robust estimator (Theil–Sen) with an interval. Output is
  rounded ("about 3.8 GB/week", "full in about 3–5 weeks"), never shown to false
  precision.
- They report "not enough data" otherwise, and flag discontinuities (large one-off
  changes) instead of extrapolating them.

## 13. Cross-cutting concerns

- **Errors:** typed `thiserror` enums per crate with structured fields (operation,
  path handle, OS code); `anyhow` only in binaries; no silently ignored results
  (`let _ =` requires a justification comment).
- **Concurrency:** one `CancellationToken` per operation; bounded channels; destructive
  steps are uninterruptible journaled units; cancellation happens between items.
- **Observability:** `tracing` with a redaction layer; local sinks; OpenTelemetry
  opt-in and content-free ([ADR-0022](../decisions/0022-privacy-preserving-observability.md)).
- **Privacy:** no file contents leave the device; cloud Jev is opt-in and metadata-only;
  content hashes stay local.
- **Persistence:** `index.db` (rebuildable) and `ledger.db` (durable)
  ([ADR-0012](../decisions/0012-local-persistence-sqlite.md)).
- **Security:** see the [threat model](../security/threat-model.md).

## 14. Failure modes

| Failure | Behaviour |
| --- | --- |
| Directory unreadable (TCC, ACL) | `Denied` node, unknown size, coverage gap; scan continues |
| Change feed drops events | Subtree invalidation; periodic full sweep |
| Identity mismatch at execution | Item aborted to `REVIEW`, reason recorded |
| Crash mid-plan | Journal reconciliation on start; each item is either at its source or in quarantine |
| Ledger DB lost or corrupt | Rebuild from sidecar manifests in quarantine stores |
| Quarantine volume unsupported | Item becomes `REVIEW` (no copy-and-delete) |
| Jev unavailable or invalid | Decision proceeds without Jev evidence |
| Name collision on restore | Ask the user; never overwrite |
| Tool command fails | Recorded; no partial claims of freed space; measured free space after |

## 15. Testing strategy

| Level | What |
| --- | --- |
| Unit | domain invariants, policy rules, size math, explanation templates |
| Property | path normalisation, policy invariants, size aggregation, graph invariants, serialisation round-trips, quarantine and plan state machines |
| Integration | use cases on temporary fixture filesystems with port fakes; fault injection at every executor step |
| Platform adapter | per-OS fixtures in CI (APFS clones, hard links, placeholders, long paths, junctions) |
| Security | symlink/junction race tests, malicious names, IPC peer rejection, injection canaries |
| AI evaluation | golden datasets, cassette replay, calibration and safety gates ([docs/ai](../ai/evaluation.md)) |
| End-to-end | Playwright against a real core on fixture filesystems; "never touch user data" assertions |
| Benchmarks | scan throughput, RSS, syscall counts, incremental rescan time, UI responsiveness |

Destructive tests never run against the real user filesystem: they use temporary
directories created by the test harness, and the executor refuses roots outside a
test-provided sandbox when built with the `test-sandbox` feature.

## 16. Deviations from the initial specification

| Initial suggestion | Decision | Why |
| --- | --- | --- |
| Next.js web app | Vite + React SPA | Tauri needs static frontends; no server on user machines ([ADR-0009](../decisions/0009-frontend-vite-react-spa.md)) |
| Axios | Native `fetch` via the generated client | axios supply-chain compromise, March 2026 |
| `apps/ios`, `apps/android` | `apps/mobile` (Expo, both platforms) | One RN app with native modules ([ADR-0007](../decisions/0007-mobile-expo-native-modules-uniffi.md)) |
| `platforms/*` directories | `crates/lumen-platform-*` + Expo Modules | Platform code lives in adapters next to the ports they implement |
| `packages/schemas` | `schemas/` generated from Rust | Rust is the single source of truth ([ADR-0011](../decisions/0011-contracts-and-schema-sharing.md)) |
| Generic "cleanup" | `QUARANTINE` plus labelled irreversible tool actions | OS trash is not a reliable quarantine ([ADR-0015](../decisions/0015-quarantine-and-reversible-cleanup.md)) |
