# Implementation roadmap

> Each numbered item is intended to be **one commit and one pull request**
> ([ADR-0027](../decisions/0027-atomic-commit-pr-workflow.md)). If an item turns out
> larger than roughly 400 changed lines of non-generated code, split it before
> starting. Items within a phase are ordered by dependency.

Phases 1 (research), 2 (architecture), 3 (repository bootstrap), 4 (domain),
5 (scanner) and 6 (evidence and graph) are complete: see [`docs/research/`](../research/),
[`docs/decisions/`](../decisions/README.md) and the
[scanner safety contract](scanner.md).

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

## Phase 4: Domain (`lumen-domain`) (complete)

Shipped as #71–#90; see [domain.md](domain.md). Changes from the plan: a UTC
`Timestamp` type (#77) and `UntrustedText` (#79) were added as prerequisites for
entries and inventory; `EvidenceHash`, `PlanHash` and `PayloadHash` digests were added
with the decision, plan and judgment types; 4.16 was split into JSON Schema
implementations (#88) and the xtask exporter with a CI drift check (#89).

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

## Phase 5: Scanner (complete)

Several items shipped as more than one PR to stay atomic: parsers and mappings
landed separately from the system calls that use them (5.11, 5.14, 5.16, 5.17),
and measurements led to two ADRs (0028, 0029), a candidate-verification port
(`CandidateVerifier`, macOS and Windows), directory-identity de-duplication for
firmlinks, and streaming size aggregation. 5.13's single "macOS" figure for the
sealed system volume moves to the volume inventory.

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
| 5.11 | `feat(platform-macos): add getattrlistbulk enumerator` | breadth-first, 32 KB buffer, attribute parsing (fuzz target), `PRIVATESIZE` in the bulk pass (ADR-0028) |
| 5.12 | `feat(platform-macos): detect clones, dataless and sync roots` | `VOL_CAP` gating (APFS lacks clone mapping: clone IDs unused); never-hydrate backstop; descendants of sync roots inherit cloud-backed status in the evidence graph (Phase 6) |
| 5.13 | `feat(platform-macos): map firmlinks and data volume` | scheduler lists each directory identity once (firmlinks, bind mounts, overlapping roots); system-volume figure with the volume inventory |
| 5.14 | `feat(platform-windows): add directory-handle enumerator` | `FileIdExtdDirectoryInfo`, placeholder mode, reparse edges |
| 5.15 | `feat(platform-windows): verify candidates by handle` | phase-2 verification |
| 5.16 | `feat(platform-macos): add FSEvents change feed` | persisted event ID + volume UUID; absolute-path streams (ADR-0029) |
| 5.17 | `feat(platform-windows): add ReadDirectoryChangesW change feed` | hot roots |
| 5.18 | `test(scan): add adversarial scanner fixtures` | symlink loops, malicious names, huge flat dirs, deep trees |
| 5.19 | `perf(scan): add criterion benchmarks and baselines` | entries/s, RSS ([performance.md](performance.md)); streaming aggregation cut peak RSS 1.32 GB → 201 MB |
| 5.20 | `docs(scan): document scanner safety contract` | `docs/architecture/scanner.md` |

## Phase 6: Evidence and graph (complete)

Deferred to Phase 10 (platform integrations), each needing an API Lumen does not
call yet:

- AppX/MSIX packages (WinRT `PackageManager`);
- scheduled tasks (Task Scheduler COM);
- `StartupApproved` state (undocumented format);
- the devkb entries marked open in 6.15+.

| # | PR title | Content |
| --- | --- | --- |
| 6.1 | `feat(graph): add lumen-graph node and edge store abstraction` | in-memory adjacency index, typed traversal API |
| 6.2 | `feat(graph): add graph invariants and property tests` | no dangling edges, acyclic `contains`, identity uniqueness |
| 6.3 | `feat(store): persist nodes, edges and evidence` | `index.db` migration v3 (v2 holds snapshots) |
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
| 6.14 | `feat(devkb): add knowledge-base schema and loader` | `lumen-devkb` entry schema (`schemas/devkb-entry.schema.json`), validation (safe relative paths, bare program names, https sources, class/action consistency), `last_verified` check |
| 6.14a | `ci(devkb): flag stale entries on a schedule` | `xtask devkb --max-age-days`; a scheduled workflow, so time passing never fails unrelated PRs |
| 6.15+ | `feat(devkb): add <tool> entry` | one PR per tool, each with sources. Landed: Cargo, Gradle, Go build and module caches, npm, pnpm, uv, Homebrew, Claude Code, Xcode DerivedData and iOS DeviceSupport. Open: simulators (`simctl` has no web reference), Docker, Yarn (needs a tool-version constraint: Yarn 1 and Berry differ), CocoaPods (cache path unverified), JetBrains (per-version directory pattern), VS Code (no first-party source) |
| 6.16 | `feat(devkb): match entries against scanned entries` | resolve entry locations per platform and environment; `KnowledgeBaseMatch` evidence |
| 6.17 | `docs(graph): document evidence graph` | `docs/architecture/evidence-graph.md` |

## Phase 7: Policy (complete)

Open items, each waiting on sources or evidence, are noted in 7.3b, 7.4 and 7.11.

`lumen-policy` turns an evidence bundle into a `PolicyDecision`
([ADR-0014](../decisions/0014-deterministic-safety-policy-engine.md)). It is IO-free,
depends only on `lumen-domain` (knowledge-base classes arrive as input data, so the
policy never depends on an adapter), and when uncertain it answers `REVIEW` or
`KEEP`.

| # | PR title | Content |
| --- | --- | --- |
| 7.1 | `feat(policy): add staged policy evaluator` | `PolicyInput` (subject facts, bundle, coverage, in-use answer, capabilities), the four stages in order, default `REVIEW`, fired-rule trace; determinism property test |
| 7.2 | `feat(policy): keep items protected by observed facts` | OS-protected flags, Windows system files, cloud-backed, in use (`Yes`, `Unknown`, and `Partial` outside ADR-0030's conditions, see 7.11) → `KEEP`; coverage checks landed with 7.4; identity is re-verified by handle in the executor (ADR-0016) |
| 7.3 | `feat(policy): keep protected system locations` | embedded, validated location data with official sources (SIP paths; WinSxS, System32, Installer cache, page and hibernation files); case-folded per platform; fails closed |
| 7.3b | `feat(policy): keep credentials, keys and browser profiles` | home-relative sensitive locations with official sources: SSH, GnuPG, macOS keychains, AWS CLI, Chrome profiles. Open: Firefox (support page not machine-readable), password managers, wallets, databases and security software (need per-product sources or signature evidence) |
| 7.4 | `feat(policy): add eligibility rules` | unobserved or partly read items → `KEEP`; quarantine unsupported, frees nothing now (hard links, clones, snapshots, dataless), links and special files → at most `REVIEW`. Open: per-volume no-replace rename support (needs volume capability evidence) |
| 7.5 | `feat(policy): classify knowledge-base and vendor-declared caches` | devkb safety class → `QUARANTINE` (regenerable), `REVIEW` (tool action, not regenerable, unknown class), `KEEP` (self-managed); vendor-declared caches → `REVIEW` until candidates can target a folder's contents |
| 7.5b | `feat(policy): classify application caches` | an owned `AppDataLocation` of kind caches → `QUARANTINE` (Apple: apps must re-create caches); containers, application support, preferences → `REVIEW`; the in-use gate covers running apps |
| 7.6 | `feat(policy): add monotone Jev stage` | `QUARANTINE` → `REVIEW` on likely needed, uncertain or suspected injection; no promotion path exists until ADR-0020's calibration gates are met |
| 7.7 | `feat(policy): add explanation templates` | fired rules → fixed sentences, never untrusted text; every rule has a template; golden-string tests |
| 7.8 | `test(policy): add safety property tests` | random inputs over every fact, coverage state, capability, path and judgment: protected items always `KEEP`; evidence order never matters; Jev never lowers caution; `QUARANTINE` only through a regenerable classification with no cap |
| 7.9 | `ci(policy): run cargo-mutants nightly` | scheduled workflow on `lumen-policy`; missed mutants fail the run (baseline: 0 missed after #179) |
| 7.10 | `feat(policy): review orphaned leftovers` | owner's absence trusted only from completely read inventories → `REVIEW` only (`classify.orphan`); finding candidates across the graph is application-layer work (Phase 10) |
| 7.11 | `feat(policy): accept partial in-use answers under ADR-0030` | below the user's home, `OwnedByCurrentUser(true)` and nothing contradicting it, quarantine rename available; recorded as `eligible.in-use-partial`; the executor's re-probe lands with Phase 9; ownership producers with Phase 10 |

## Phase 8: Jev (complete)

Live calibration and the consent UI wait for the composition root (Phase 10/11).

Jev contributes evidence only ([ADR-0020](../decisions/0020-jev-evidence-only-judge.md),
[docs/ai](../ai/architecture.md)): the policy consumes a validated `Judgment` and can
only become more cautious (7.6). Lumen works fully with `NoopJudge`.

| # | PR title | Content |
| --- | --- | --- |
| 8.1 | `feat(domain): add Jev request type` | `jev.request/1` (candidate summary, evidence items, coverage, `untrusted_strings`); schema export |
| 8.2 | `feat(application): add JudgeModel port` | async `judge`, `descriptor`, `JudgeError`; `NoopJudge`; `MockJudge` in testkit |
| 8.3 | `feat(jev): add request builder` | `lumen-jev`: bundle → request; path redaction (`~`, user names), escaped untrusted strings, decimal-string integers, no contents |
| 8.4 | `feat(jev): add response validator` | stop reason, strict JSON (unknown fields rejected), enums, `Judgment::validate_for`, pinned model |
| 8.5 | `feat(application): consult Jev for unprotected candidates` | only `REVIEW`/`QUARANTINE` candidates after stages 1–3; errors → proceed without Jev; trace |
| 8.6 | `feat(store): persist Jev traces` | `ledger.db` `jev_traces` |
| 8.7 | `feat(jev-eval): add cassettes and replay judge` | `jev.cassette/1` (one judge configuration, entries keyed by request hash), `ReplayJudge` through the real validator; mock mode is `MockJudge` |
| 8.7b | `feat(jev-eval): add evaluation runs and gate report` | cases → requests → judge → policy; metrics and gate verdict per docs/ai/evaluation.md |
| 8.7c | `feat(jev-eval): add golden dataset v2026.10.0` | seed cases and injection canaries with twins; manifest hashes verified on load; canary-flip gate |
| 8.7d | `test(policy): add protected-fixture gate` | protected strata as policy inputs with an adversarial judge: 0 policy violations |
| 8.8 | `feat(jev): map Jev decisions to judgments` | ADR-0031: Decisions API request (state + `assessment`/`reason`/`injection` questions) and strict answer mapping onto `Judgment`, tested offline |
| 8.9 | `feat(jev): add OpenRouter transport` | `JevJudge` over HTTPS (ureq + rustls with the OS trust store), 30 s timeout, 64 KiB answer cap; the caller supplies the API key |
| 8.10 | `feat(application): add SecretStore port` | fixed secret names, redacted `Secret`, `MemorySecretStore` in testkit |
| 8.10b | `feat(platform-macos): store secrets in the keychain` | Security framework generic-password items; real-keychain test opt-in |
| 8.10c | `feat(platform-windows): store secrets in Credential Manager` | `CredWriteW`/`CredReadW`/`CredDeleteW` generic credentials |

## Phase 9: Quarantine

`lumen-fs-exec` is the only crate that may move or delete user files
([ADR-0015](../decisions/0015-quarantine-and-reversible-cleanup.md),
[ADR-0016](../decisions/0016-single-handle-relative-executor.md)). It acts on plan
items, not paths: it walks from a verified root handle without following links,
re-verifies identity, and uses no-replace renames only. Any mismatch fails closed to
`REVIEW`.

| # | PR title | Content |
| --- | --- | --- |
| 9.1 | `feat(fs-exec): verify plan items by handle` | crate; `PlanItem`; Unix handle walk (`openat` `O_NOFOLLOW`/`O_NOFOLLOW_ANY`, `O_DIRECTORY`), leaf `fstatat(AT_SYMLINK_NOFOLLOW)`; identity, type, size and time comparison; read-only |
| 9.2 | `feat(fs-exec): open per-volume quarantine stores` | created by handle with mode 0700, never through links; owner, permission and same-volume checks; planted directories refused, never repaired |
| 9.2b | `feat(fs-exec): exclude stores from backups and indexing` | macOS: sticky Time Machine exclusion of the store by its handle's path, with identity checks before and after (`lumen-platform-macos`); Spotlight has no documented programmatic directory exclusion, recorded as a gap. Called by the composition root (Phase 10) |
| 9.3 | `feat(fs-exec): move items into quarantine` | `renameat_with(NOREPLACE)` between verified handles, directory `fsync`, post-move verification |
| 9.4 | `feat(store): journal quarantine operations` | `ledger.db` intent and completion records (`synchronous=FULL`) |
| 9.5 | `feat(domain): add quarantine manifest type` | `quarantine.manifest/1`: operation, plan, device, raw original path, observed entry, Unix ownership, decision, confirmation, Jev link; schema export |
| 9.5b | `feat(fs-exec): write manifest sidecars` | exclusive create (mode 0600, no links) next to the item in its store, `fsync`; never overwritten |
| 9.6 | `feat(application): quarantine confirmed items` | `QuarantineExecutor` and `QuarantineJournal` ports; intent → in-use re-probe (ADR-0030 condition 4) → execute → journal outcome |
| 9.6b | `feat(fs-exec): implement the quarantine executor` | `prepare` / `quarantine` / `locate` over verify, move and sidecar (Unix); ledger journal adapter |
| 9.6c | `feat(application): reconcile unfinished operations` | on startup, resolve journaled/moved/verified operations by locating each item: at the source fails, moved but unverified fails in the store (rolled back by restore), verified is retained; unknown or contradicting stays untouched for the user |
| 9.7 | `feat(fs-exec): restore quarantined items` | the item must still match its record; the original parent is reached by handle without links on the store's volume (a recreated parent is reported); no-replace rename back, `fsync`, verification; a taken name is a conflict for the user, never overwritten |
| 9.7b | `feat(application): restore quarantined items` | `QuarantineExecutor::restore` port and executor method; journal `Restore` for retained items and items failed in the store (reconciliation's roll-backs) |
| 9.8 | `feat(fs-exec): finalize after retention` | permanent removal only inside the store, by handle: a file must match in full, a directory by identity and kind (so an interrupted finalization can finish); the whole tree is checked first (same volume, no mount points, at most 256 deep, links removed as links), then removed bottom-up; sidecar last |
| 9.8b | `feat(application): finalize retained items` | port and use case: only `Retained` items after retention or on request; locate first: in the store → finalize and journal `Finalize`; at the source → journal `Restore` (a restore whose journal entry was lost); unknown → nothing |
| 9.9 | `test(fs-exec): add race, name and fault-injection suites` | symlink flips mid-execution, NFC/NFD and case twins, RTLO/newline names, hard links, read-only, open files, faults at every step |
| 9.10 | `feat(fs-exec): add the Windows executor` | handle-relative walk, `FileRenameInfoEx` without replace, `FILE_FLAG_OPEN_REPARSE_POINT` |
| 9.11 | `chore(fs-exec): protect the executor` | CODEOWNERS and threat-model section |

## Later phases (outline)

- **Phase 10, Platform integrations:** macOS end to end → Windows → Android
  (performance spike first) → iOS.
- **Phase 11, UI:** JavaScript bootstrap (pnpm, Biome, TS 7, Turborepo), `apps/web`
  (FSD), `apps/desktop` (Tauri), `apps/mobile` (Expo).
- **Phase 12, Hardening:** benchmarks, observability, evaluation gates, release
  pipeline, signing.
