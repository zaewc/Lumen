# Implementation roadmap

> Each numbered item is intended to be **one commit and one pull request**
> ([ADR-0027](../decisions/0027-atomic-commit-pr-workflow.md)). If an item turns out
> larger than roughly 400 changed lines of non-generated code, split it before
> starting. Items within a phase are ordered by dependency.

Phases 1 (research), 2 (architecture) and 3 (repository bootstrap) are complete:
see [`docs/research/`](../research/) and [`docs/decisions/`](../decisions/README.md).

## Phase 3: Repository bootstrap (complete)

Items 3.3 and 3.4 shipped as one PR (#57): Cargo rejects a virtual workspace whose
member glob matches nothing, so the workspace and its first member are indivisible.
The `gen` recipe from 3.12 is deferred until `xtask` exists (4.16).

| # | PR title | Content |
| --- | --- | --- |
| 3.1 | `chore: add editorconfig, gitattributes and gitignore` | line endings, binary attributes, ignores for Rust, Node, Xcode, Gradle |
| 3.2 | `docs: add pull request template` | `.github/pull_request_template.md` with Summary/Why/Changes/Tests/Security/Risks/Follow-up |
| 3.3 | `build(rust): add Cargo workspace and toolchain pin` | root `Cargo.toml` (resolver 3, workspace package/lints), `rust-toolchain.toml`, `rustfmt.toml`, `clippy.toml` (incl. `disallowed-methods`) |
| 3.4 | `feat(domain): add lumen-domain crate skeleton` | first member crate with `lib.rs`, crate docs, one smoke test; proves the workspace builds |
| 3.5 | `ci: add Rust format, lint and test workflow` | `.github/workflows/rust.yml`: fmt, clippy `-D warnings`, nextest, doctests on ubuntu-24.04 / windows-2025 / macos-26; SHA-pinned actions; `permissions: {}` |
| 3.6 | `build(rust): add cargo-deny configuration` | `deny.toml` (advisories, licenses allow-list, bans, sources) |
| 3.7 | `ci: add supply-chain workflow` | cargo-deny + OSV-Scanner jobs |
| 3.8 | `ci: add workflow linting` | zizmor + actionlint jobs |
| 3.9 | `ci: add secret scanning` | gitleaks job + config |
| 3.10 | `ci: add CodeQL analysis` | advanced setup for `rust` and `actions` (`javascript-typescript` added with the first TS code) |
| 3.11 | `ci: enforce Conventional Commits PR titles` | PR-title check on `pull_request` |
| 3.12 | `build: add justfile` | `just fmt`, `lint`, `test`, `check`, `gen` |
| 3.13 | `build: add lefthook git hooks` | pre-commit (fmt, gitleaks), commit-msg, pre-push (clippy, deny) |
| 3.14 | `chore: add Renovate configuration` | `renovate.json` (release age, action digest pinning, grouping) |
| 3.15 | `chore: add CODEOWNERS` | safety paths: `crates/lumen-policy`, `crates/lumen-fs-exec`, IPC, `.github/workflows`, `ai/` |
| 3.16 | `ci: add MSRV check` | `cargo hack --rust-version` job |

Repository settings (rulesets requiring PRs and checks, linear history, no force push)
are applied by the owner once the required check names exist. Those are outward
settings, not code.

JavaScript tooling (pnpm workspace, Biome, TypeScript config, Turborepo) is
bootstrapped at the start of the UI phase, so that it lands with the first real
TypeScript code.

## Phase 4: Domain (`lumen-domain`)

| # | PR title | Content |
| --- | --- | --- |
| 4.1 | `feat(domain): add typed identifiers` | `DeviceId`, `VolumeId`, `FileIdentity`, `NodeId`, `ScanId`, `PlanId`, `OperationId` newtypes + serde/schemars + tests |
| 4.2 | `feat(domain): add version types` | `PolicyVersion`, `PromptVersion`, `SchemaVersion` |
| 4.3 | `feat(domain): add raw path and display escaping` | byte-preserving path type; display escaping of control/bidi characters; proptest |
| 4.4 | `feat(domain): add size facts` | `SizeFacts`, flags, aggregation with identity de-duplication; proptest for aggregation invariants |
| 4.5 | `feat(domain): add access state and coverage report` | `AccessState`, `DenialReason`, `CoverageReport` |
| 4.6 | `feat(domain): add platform capabilities` | `Platform`, `PlatformCapabilities` |
| 4.7 | `feat(domain): add filesystem entry model` | `Volume`, `FilesystemEntry`, `EntryKind` |
| 4.8 | `feat(domain): add inventory entities` | `Application`, `Process`, `Service`, `Package` |
| 4.9 | `feat(domain): add evidence record` | `Evidence`, `EvidenceKind`, provenance; canonical serialisation + BLAKE3 `EvidenceId`; golden hash tests |
| 4.10 | `feat(domain): add relationship model` | typed edges |
| 4.11 | `feat(domain): add verdict, risk and policy decision types` | `Verdict`, `Risk`, `FiredRule`, `PolicyDecision` |
| 4.12 | `feat(domain): add cleanup candidate and actions` | `CleanupCandidate`, `CleanupAction`, `Reversibility` |
| 4.13 | `feat(domain): add quarantine item state machine` | states + transition function + proptest of invariants |
| 4.14 | `feat(domain): add scan and plan state machines` | |
| 4.15 | `feat(domain): add judgment types` | `Judgment`, `JudgeDescriptor`, `JevTrace` (types only) |
| 4.16 | `build(xtask): export JSON Schemas` | `xtask schema` writes `schemas/`; insta snapshot test; CI drift check |
| 4.17 | `docs(domain): document domain model` | crate docs + `docs/architecture/domain.md` |

## Phase 5: Scanner

| # | PR title | Content |
| --- | --- | --- |
| 5.1 | `feat(application): add lumen-application crate with scan ports` | `DirEnumerator`, `ChangeFeed`, `Clock` traits; no implementations |
| 5.2 | `test(testkit): add fixture filesystem builder` | `lumen-testkit`: declarative fixture trees in tempdirs (files, dirs, symlinks, hard links, sizes, times) |
| 5.3 | `feat(scan): add portable std::fs enumerator` | fallback adapter, no symlink following, access-state mapping |
| 5.4 | `feat(scan): add scan scheduler` | rayon pool, per-volume semaphore, priority bands, bounded channels |
| 5.5 | `feat(scan): add cancellation and progress reporting` | `CancellationToken` integration, progress events |
| 5.6 | `feat(scan): handle partial failure` | denied/error nodes, coverage report |
| 5.7 | `feat(scan): aggregate sizes per directory` | identity-de-duplicated aggregates, `reclaimable_now` |
| 5.8 | `feat(store): add lumen-store-sqlite with migrations` | `index.db` schema v1 (STRICT), single writer thread, batch inserts |
| 5.9 | `feat(scan): persist scan snapshots and checkpoints` | resumable scans |
| 5.10 | `feat(cli): add lumen-cli scan command` | read-only scan of a given root, JSON output; for development and the harness |
| 5.11 | `feat(platform-macos): add getattrlistbulk enumerator` | breadth-first, 32 KB buffer, attribute parsing (fuzz target), `PRIVATESIZE` second pass |
| 5.12 | `feat(platform-macos): detect clones, dataless and sync roots` | `VOL_CAP` gating; never-hydrate backstop |
| 5.13 | `feat(platform-macos): map firmlinks and data volume` | |
| 5.14 | `feat(platform-windows): add directory-handle enumerator` | `FileIdExtdDirectoryInfo`, placeholder mode, reparse edges |
| 5.15 | `feat(platform-windows): verify candidates by handle` | phase-2 verification |
| 5.16 | `feat(platform-macos): add FSEvents change feed` | persisted event ID + device UUID |
| 5.17 | `feat(platform-windows): add ReadDirectoryChangesW change feed` | hot roots |
| 5.18 | `test(scan): add adversarial scanner fixtures` | symlink loops, malicious names, huge flat dirs, deep trees |
| 5.19 | `perf(scan): add criterion benchmarks and baselines` | entries/s, RSS |
| 5.20 | `docs(scan): document scanner safety contract` | `docs/architecture/scanner.md` |

## Phase 6: Evidence and graph

| # | PR title | Content |
| --- | --- | --- |
| 6.1 | `feat(graph): add lumen-graph node and edge store abstraction` | in-memory adjacency index, typed traversal API |
| 6.2 | `feat(graph): add graph invariants and property tests` | no dangling edges, acyclic `contains`, identity uniqueness |
| 6.3 | `feat(store): persist nodes, edges and evidence` | `index.db` migration v2 |
| 6.4 | `feat(graph): build evidence bundles with Merkle hash` | canonical bundle + `EvidenceHash`; golden tests |
| 6.5 | `feat(application): add InventorySource and InUseProbe ports` | |
| 6.6 | `feat(platform-macos): inventory applications and bundles` | bundles → bundle ID → Team ID |
| 6.7 | `feat(platform-macos): inventory launchd items` | plists (fuzzed parser), program → signature |
| 6.8 | `feat(platform-macos): add running process and open-file probe` | `libproc` |
| 6.9 | `feat(platform-macos): link app support locations` | Caches, Application Support, Containers, Preferences |
| 6.10 | `feat(platform-windows): inventory installed applications` | Uninstall keys, MSI, AppX |
| 6.11 | `feat(platform-windows): inventory startup items and services` | Run keys, StartupApproved, Task Scheduler, SCM (report-only) |
| 6.12 | `feat(platform-windows): add Restart Manager in-use probe` | |
| 6.13 | `feat(platform-windows): read Disk Cleanup handler declarations` | vendor-declared cache edges |
| 6.14 | `feat(devkb): add knowledge-base schema and loader` | `lumen-devkb` entry schema, validation, `last_verified` check |
| 6.15+ | `feat(devkb): add <tool> entry` | one PR per tool (Xcode DerivedData, simulators, Docker, npm, pnpm, Yarn, Cargo, Gradle, CocoaPods, Homebrew, JetBrains, VS Code, uv, Go, Claude Code), each with sources |
| 6.x | `docs(graph): document evidence graph` | `docs/architecture/evidence-graph.md` |

## Later phases (outline)

- **Phase 7, Policy:** `lumen-policy` stages, rule data, explanation templates,
  property and mutation tests, Orphan Hunter queries.
- **Phase 8, Jev:** `JudgeModel` port, `NoopJudge`/`MockJudge`, request builder,
  validator, trace, `lumen-jev-eval` with replay, datasets v2026.10.0, then
  `AnthropicJudge`.
- **Phase 9, Quarantine:** `lumen-fs-exec` (macOS, then Windows), journal, manifests,
  verification, rollback, retention, race and fault-injection suites.
- **Phase 10, Platform integrations:** macOS end to end → Windows → Android
  (performance spike first) → iOS.
- **Phase 11, UI:** JavaScript bootstrap (pnpm, Biome, TS 7, Turborepo), `apps/web`
  (FSD), `apps/desktop` (Tauri), `apps/mobile` (Expo).
- **Phase 12, Hardening:** benchmarks, observability, evaluation gates, release
  pipeline, signing.
