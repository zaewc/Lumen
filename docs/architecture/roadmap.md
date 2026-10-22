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
| 6.15+ | `feat(devkb): add <tool> entry` | one PR per tool, each with sources. Landed: Cargo, Gradle, Go build and module caches, npm, pnpm, uv, Homebrew, Claude Code, Xcode DerivedData, iOS DeviceSupport and simulator devices, CocoaPods download cache, JetBrains system directory (`not_regenerable` guard: it holds Local History), Docker Desktop disk image (`not_regenerable`). Open: simulator runtimes (no home-relative location), Docker objects via `docker system prune`, Yarn (Yarn 1 has no documented path; Berry's `cache clean --mirror` is a no-op under the default global cache and plain `cache clean` depends on the working directory), VS Code (no first-party source) |
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

## Phase 9: Quarantine (complete)

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
| 9.9a | `test(fs-exec): carry hostile names through the lifecycle` | NFC/NFD and case twins, RTLO, newline, control, leading-dash, CJK and non-UTF-8 names through quarantine, restore and finalize; twins restore correctly or are refused, never overwritten |
| 9.9b | `test(fs-exec): add race suites` | symlink and directory swaps between prepare and execute, leaf replacement, hard links, read-only directories |
| 9.9c | `test(application): crash at every journal step` | a model of the item's location and a journal that crashes after each entry: after restart, reconciliation (quarantine) and finalization (restore) leave the journal replaying to where the item physically is |
| 9.9d | `test(fs-exec): inject faults at every system call` | ADR-0032 test-only fault points after each flush, post-move check, sidecar write and finalize unlink; every reported location (`left`, `removed`) matches the filesystem afterwards |
| 9.10 | `docs(adr): decide the Windows executor mechanics` | ADR-0033: relative `NtCreateFile` per component with `FILE_OPEN_REPARSE_POINT`, `FileRenameInfo` without replace, `FileDispositionInfoEx`, store DACL, `unsafe` confined to `win_sys` |
| 9.10a | `feat(fs-exec): verify plan items on Windows` | `win_sys` (relative open, queries, `NTSTATUS` mapping); handle walk refusing reparse points and other volumes; identity, kind, size, time and link comparison |
| 9.10b | `feat(fs-exec): open Windows quarantine stores` | `$LumenQuarantine\<UserSID>` created with owner and protected DACL, not content-indexed; foreign or loose stores refused |
| 9.10c | `feat(fs-exec): move and restore on Windows` | `FileRenameInfo` without replace between verified handles, both directions; post-rename checks; exclusive sidecars |
| 9.10c2 | `feat(fs-exec): finalize on Windows` | directory listing by handle, checked-first tree removal with `FileDispositionInfoEx`, links removed as links |
| 9.10d | `feat(fs-exec): implement the Windows executor` | `QuarantineExecutor` on Windows over verify, store, move, restore, sidecar and finalize; local drive roots only (verbatim `\\?\`), network roots refused |
| 9.10e | `test(fs-exec): run the name and race suites on Windows` | the lifecycle with hostile names, junction swaps, replaced items and held-open files through the Windows executor |
| 9.11 | `chore(fs-exec): protect the executor` | CODEOWNERS for `win_sys` and fault points, executor review checklist, threat-model executor section and residual risks updated |

## Phase 10: macOS end to end

Every piece from scanner to executor exists as a library; nothing past the scan is
wired yet. This phase connects them in the CLI composition root on macOS (Linux CI
runs the same path), following the pipeline in [system.md](system.md):
collect evidence → build relationships → policy (optional Jev) → explain → plan →
user confirms → quarantine → verify → retain → finalize or roll back. Destructive
commands arrive only with an explicit, hash-bound confirmation (10.9), and the CLI
test that forbids them today changes in the same PR.

| # | PR | Scope |
| --- | --- | --- |
| 10.1 | `feat(cli): add the system clock and ID adapters` | `Clock` and `IdGenerator` (UUIDv7) for the composition root; the scan command uses them |
| 10.2 | `feat(application): collect filesystem evidence` | `Protected` (always, for the hard-protection stage), `CloudBacked`, `LastModified`/`LastAccessed` from stored entries; nothing from unread metadata |
| 10.2b | `feat(scan): collect ownership evidence on Unix` | `OwnershipProbe` port (ownership producers deferred by 7.11): the owner is not in scanned entries, so it is read when assessing; Unix probe (`lstat`, scanner identity rule, effective user ID); `ownership_evidence` |
| 10.2c | `feat(platform-windows): probe ownership on Windows` | owner SID of the object, opened as itself and identity-checked, against the process token's user |
| 10.3 | `feat(application): collect inventory evidence` | `ApplicationPresent` (with `Basis::Absence` only from complete sources) from `InventorySource`s; `Context.complete_sources` from their status |
| 10.4 | `feat(application): collect in-use evidence` | `InUseProbe` answers as `InUse` / `NotInUseByVisibleProcesses` evidence (ADR-0030) |
| 10.5 | `feat(devkb): map safety classes to knowledge classes` | `SafetyClass` → `KnowledgeClass` for `PolicyInput.knowledge` |
| 10.6 | `feat(application): gather a scan's evidence` | producers' evidence and relationships (devkb matches, support locations) combined with scanned-entry facts and owners' installation state, unique by ID; recording in `index.db` and loading the graph back happen in the composition root (10.13) |
| 10.7 | `feat(store): read coverage and reclaim back` | listing completeness per directory and a per-subject `ReclaimEstimate` for `Observed` |
| 10.8 | `feat(application): select candidates and assemble their policy input` | candidates are entries some evidence identifies (knowledge-base match, app data location, declared cache), outermost only; `PolicyInput` with the bundle (relationships within two hops, evidence about every reached subject, fresh probe evidence) and its hash, paths, context, `Observed` |
| 10.8b | `feat(application): decide and explain candidates` | probe in use and ownership, `decide` with an optional judge and its request, traces stored, explanations |
| 10.9 | `docs(adr): confirm cleanup plans from the command line` | ADR-0034: `lumen plan` shows QUARANTINE items and the plan hash; `lumen quarantine --plan --confirm <hash>` executes only that plan, re-deciding every item on fresh evidence; stale plans and root refused |
| 10.10 | `feat(domain): add cleanup plans` | `CleanupPlan`, `PlanHash` computation, `ConfirmationSurface::CommandLine` |
| 10.11 | `feat(store): persist plans and decisions` | plans, items and their decisions in `index.db` |
| 10.12 | `feat(application): confirm a plan` | re-verify each item, re-run the policy, prepare it, build its `quarantine.manifest/1`, allocate operation IDs, produce `ConfirmedItem`s |
| 10.13 | `feat(store): find the latest completed scan and its roots` | assessment reads only completed scans; roots split item paths for the executor |
| 10.13b | `feat(cli): assess the latest scan` | composition: inventory, devkb and support-location producers, gather, record and load the graph, select, decide; read-only report with reasons |
| 10.13c | `feat(cli): plan` | QUARANTINE items into a stored plan, printed with reasons and the plan hash (ADR-0034) |
| 10.14a | `refactor(cli): reuse the assessment for confirmation` | the assessment's context (roots, entries, listings, evidence, graph, policy context) built once and reused; `StdFsEnumerator` verifies candidates |
| 10.14b | `feat(cli): quarantine a confirmed plan` | composition root: hash, age and privilege checks; executor and store (Time Machine exclusion); ledger journal; reconciliation on every start; confirm then quarantine each item |
| 10.15a | `feat(store): replay every quarantine operation` | `operations()` with each operation's replayed state; the executors name a prepared item's original path |
| 10.15b | `feat(cli): list, restore and finalize` | quarantined items with their original paths and ages, restore with conflicts reported, finalize on request (retention scheduling later) |
| 10.16 | `test(cli): run the lifecycle end to end` | covered on macOS by the 10.15b CLI test (scan → plan → quarantine → list → restore → re-plan → quarantine → finalize). Linux gains an in-use probe on `/proc` (open descriptors, working directory, executable and mapped files; other users' processes count as unseen) |
| 10.17a | `feat(cli): manage the Jev API key` | `lumen jev key set/clear/status`: the key read from standard input, checked, kept in the macOS keychain or Windows Credential Manager, never printed |
| 10.17b | `feat(cli): consult Jev on request` | `--jev` per run on `assess`/`plan`: a notice of what is sent, the request built from each candidate's metadata only, traces in the ledger |
| 10.18 | `feat(cli): quarantine, list, restore and finalize on Windows` | the same commands through the Windows executor; store in `%LOCALAPPDATA%\Lumen\Quarantine` (ADR-0015 amended), so items on other volumes stay in place; an elevated token is refused (ADR-0005) |
| 10.19 | `fix(application): probe in-use state before finalizing` | the executors name the held object (path and identity in the enumerators' form); finalization probes it there first and keeps anything `Yes`, `Unknown` or unnamed (ADR-0030 clarified) |
| 10.20 | `feat(cli): finalize items past their retention` | `lumen finalize --expired`: every retained item quarantined 30 or more days ago (ADR-0015's default), each probed for use first; items held after a failure wait for the user. A configurable period and scheduled runs come with the agent (Phase 11) |
| 10.21 | `fix(fs-exec): forget the sidecars of restored items` | once `Restore` is journaled, the executor removes the operation's manifest sidecar from the store, and only while nothing in the store has the item's name; a sidecar that stays is harmless, because the journal is the record |
| 10.22 | `feat(fs-exec): record Unix mode and owner in manifests` | the Unix executor keeps `st_mode`, `st_uid` and `st_gid` from the stat it prepares from, and the manifest's `unix` field carries them (ADR-0015); records from before still parse |
| 10.23 | `feat(ffi): build the mobile core for Android` | `lumen-ffi` (UniFFI; MPL-2.0 approved for the `uniffi*` crates) exports `core_version`; `cargo xtask android` builds it for `arm64-v8a` and `x86_64` through the NDK's clang (r28+, `minSdk 30`), fails unless every `LOAD` segment is 16 KB-aligned, and generates the Kotlin bindings; CI runs it |
| 10.24 | `feat(ffi): build the mobile core for iOS` | `cargo xtask ios` builds `lumen-ffi` for devices and the Apple-silicon simulator (iOS 16.4+), packages `LumenCore.xcframework` with the header and module map, generates the Swift bindings and proves them by calling `coreVersion()` from Swift on the host; CI runs it on macOS |

Windows end to end follows the same items with the Windows adapters (10.18). Android
comes next: the core builds for Android (10.23) and iOS (10.24), then the Expo app
and the performance spike that gates mobile UI work (ADR-0007).

## Phase 11: UI and mobile

| # | PR | Scope |
| --- | --- | --- |
| 11.1 | `build(js): bootstrap the JavaScript workspace` | pnpm 12 workspace (`apps/*`, `packages/*`) with a strict catalog and the ADR-0024 supply-chain settings, Biome, Turborepo for the JS task graph, TypeScript 7 with an explicit shared `tsconfig` (`packages/config`); `just check`, the pre-commit hook and a `js` CI workflow run Biome and typecheck |
| 11.2 | `refactor(app): move the scan composition into lumen-app` | a `lumen-app` library crate (system.md §5.1) takes the platform enumerator choice, clock, ID generator and recording of a scan from `lumen-cli`; it returns structured summaries and `thiserror` errors, and the CLI only formats them |
| 11.3 | `refactor(app): move assessment and planning into lumen-app` | the assessment context, probes, platform contributions and plan creation, with typed results; Jev stays opt-in per call |
| 11.4 | `refactor(app): move quarantine, restore and finalize into lumen-app` | the privilege, hash and age checks, store location, reconciliation and the held-item operations, unchanged in behaviour; `lumen-cli` becomes a thin shell over `lumen-app` |
| 11.5 | `feat(app): describe the local API in OpenAPI` | view models for scans, candidates, plans and held items (ADR-0011 encoding: 64-bit numbers as decimal strings, RFC 3339 times, display paths plus opaque handles); `utoipa` emits `schemas/openapi.json`, and `cargo xtask schema --check` fails on drift |
| 11.6 | `build(js): generate the API client` | `packages/api-client` (types, Zod schemas, `fetch` SDK) generated from the committed spec with an exactly pinned generator; CI fails on drift |
| 11.7 | `feat(web): bootstrap the web app` | `apps/web` (Vite, React, TanStack Router and Query, Tailwind, Zod; ADR-0009) in Feature-Sliced Design with Steiger; the `LumenTransport` port with an in-memory adapter for development and tests |
| 11.8 | `feat(desktop): host the core in Tauri` | `apps/desktop` (Tauri 2.12.x, ADR-0004): the core in the host process, one command per read or intent (selection IDs, plan ID plus confirmation; never a path for mutation), per-window capabilities, strict CSP, isolation pattern, navigation and permission requests denied; `TauriTransport` |
| 11.9 | `feat(web): show scans and assessments` | start a scan of chosen roots, progress over a channel, coverage, and candidates with verdicts and reasons (read-only) |
| 11.10 | `feat(web): plan and quarantine with confirmation` | build a plan from `QUARANTINE` candidates, show it with its hash, confirm explicitly, execute, show each outcome |
| 11.11 | `feat(web): restore and finalize held items` | the held items with age and retention, restore with conflicts explained, finalize only on explicit request |
| 11.12 | `test(desktop): run the desktop flow end to end` | the scan → plan → quarantine → restore → finalize flow through the real host on a fixture directory |
| 11.13 | `feat(mobile): bootstrap the Expo app` | `apps/mobile` on Expo SDK 58 (React Native 0.88, New Architecture), continuous native generation (no committed `ios/`/`android/`); typecheck and a JavaScript bundle export for both platforms in `pnpm run check`; Expo telemetry off |
| 11.14 | `feat(mobile): load the Rust core on Android` | a local Expo module (`lumen-core`) wrapping the UniFFI Kotlin bindings and the 16 KB-aligned `.so` files from `cargo xtask android`; the app shows `coreVersion()` on an emulator |
| 11.15 | `feat(mobile): load the Rust core on iOS` | the same module in Swift over `LumenCore.xcframework` from `cargo xtask ios`; verified on the simulator |
| 11.16a | `feat(cluster): group near-duplicate feature prints` | `lumen-cluster` (pure): single-linkage groups of vectors within a Euclidean threshold, all-pairs on scoped threads with early exit, at most 20,000 vectors; exported as `group_feature_prints` from `lumen-ffi`; 5,000 × 768 in about 33 ms on an M-series Mac |
| 11.16b | `feat(mobile): build the photo review grid spike` | the photo library enumerated natively (PhotoKit, MediaStore) into paged view models, a thumbnail grid in React Native, an on-screen frame-time probe |
| 11.16c | `feat(mobile): group photos in the spike` | Vision feature prints (iOS) in a user-started background task, a perceptual-hash fallback (Android), grouping in Rust, groups shown in the grid |
| 11.16d | `docs(research): measure the mobile spike` | the measurement protocol and the owner's results on mid-range iPhone and Android devices: ≥ 60 fps scrolling and < 4 ms JS per frame decide React Native vs native screens (ADR-0007) |

## Phase 12: Hardening

| # | PR | Scope |
| --- | --- | --- |
| 12.1 | `feat(jev-eval): run evaluations from the command line` | `lumen-jev-eval run`: a dataset version (manifest-verified) against the mock judge or a replay cassette; the gate report as text and JSON; a non-zero exit when a gate fails (docs/ai/evaluation.md) |
| 12.2 | `ci: gate Jev changes on the evaluation` | a workflow on changes to `ai/`, `crates/lumen-jev*`, `crates/lumen-policy` and the Jev contract: the golden and adversarial sets replayed, failing on any gate |
| 12.3 | `feat(telemetry): add local, redacted telemetry` | `lumen-telemetry` (ADR-0022): `tracing` with a redaction layer (no paths, names or content hashes at `info` and above), a rolling JSON log in the app data folder and an in-memory ring buffer the user can review and export; nothing leaves the machine |
| 12.4 | `feat(app): instrument scans, decisions and quarantine` | spans and aggregate counters in `lumen-app` (files/s, decisions by verdict, quarantine outcomes, rollbacks, Jev latency and validation failures); the CLI and desktop host install the subscriber |
| 12.5 | `docs(operations): list every signal` | `docs/operations/privacy.md`: each span, event and metric, what it carries and where it goes (ADR-0022: no hidden telemetry) |
| 12.6 | `perf: record benchmark baselines` | the scan, policy and grouping benchmarks in `performance.md`, and a manual workflow that runs them on CI hardware for comparison |
| 12.7a | `build(release): build release artifacts on tags` | a tag-triggered workflow: the CLI for macOS (arm64), Windows and Linux (x86_64), with SHA-256 checksums, a CycloneDX SBOM per target and build-provenance attestations, published as a draft release (docs/operations/release.md) |
| 12.7b | `build(release): bundle the desktop app` | unsigned desktop bundles (universal `.dmg`, per-user NSIS installer, `.deb`/`.AppImage`) and their SBOMs added to the same release, with icons for every platform |
| 12.8 | `build(release): sign and notarize` | macOS Developer ID, Hardened Runtime and notarization, and Windows Azure Artifact Signing (ADR-0006), once the owner provides the accounts as protected-environment secrets |

## Phase 13: First use

Found by the owner trying the CLI and the desktop app (2026-10-19). Done:

| # | PR | Scope |
| --- | --- | --- |
| 13.1 | `feat(cli): log scan progress as it runs` (#302) | a live, log-style view on standard error; `lumen_app::ScanEvent` |
| 13.2 | `feat(app): never fill the disk when recording` (#303) | `lumen_app::space`: 2 GB always left free; scans stop with `out_of_space`, assessments check first |
| 13.3 | `feat(api): carry the rule behind each reason` (#304) | `ReasonView { rule, text }` |
| 13.4 | `fix(app): record a scan's evidence once` (#305) | scan facts dated by the scan, so re-assessing adds nothing |
| 13.5 | `feat(web): add a Korean locale` (#306) | typed en/ko catalogs, no dependency |
| 13.6 | `feat(cli): scan the whole disk when no path is given` (#307) | `lumen_app::roots::whole_disk` |
| 13.7 | `feat(cli): show sizes in readable units` (#308) | |
| 13.8 | `perf(application): gather scan facts only where a decision can reach` (#309) | 790 k → 30 evidence records for 198 k entries; identical decisions |
| 13.9 | `feat(web): explain terms with info buttons` (#310) | `InfoTip`, a bilingual glossary |
| 13.10 | `feat(desktop): …` (#312–#314) | whole-disk button, live scan progress, the out-of-space reason |
| 13.11 | `feat(telemetry): keep only the last 14 days of logs` (#315) | |
| 13.12 | `feat(web): localize dates and keep tips on screen` (#316) | |
| 13.13 | `fix(macos): name privacy and integrity refusals` (#317) | `EPERM` → TCC or SIP by `SF_RESTRICTED` |
| 13.14 | `perf(store): drop the unused entry identity index` (#318) | scan database −39% |
| 13.15 | `ci: compile the generated Kotlin bindings` (#319) | pinned Kotlin and JNA |
| 13.16 | `perf: stream the assessment` (#322–#324) | paged reads, chunked knowledge-base matching, per-candidate subtrees: whole-disk `assess` 5.0 GB → 1.0 GB, 36 s → 25 s, identical output |
| 13.17 | `perf(store): store each directory path once` (#329) | `index.db` schema v6: a directory table plus names; whole disk 1,777 → 978 MB, scan 100 → 71 s, identical decisions; existing scans must be rescanned after the upgrade |
| 13.18 | `feat(web): one guided clean-up flow` (#325) | two tabs (Clean up, Quarantine), scan → "you can free X" → Clean up → confirm dialog → done |
| 13.19 | `feat(quarantine): show how much space the quarantine holds` (#326) | per-item size and total, from an append-only `quarantine_sizes` ledger table |
| 13.20 | `feat(web): follow the system's dark mode` (#327) | |
| 13.21 | `feat(desktop): give Lumen a real icon` (#328) | |
| 13.22 | `feat(store): keep only the latest scan's rows` (#330) | earlier and abandoned scans pruned after a completed scan, `VACUUM` when 256 MB+ is free (the owner's database had grown to 11.3 GB) |
| 13.23 | `feat: show a tool's own cleanup command` (#332, #333) | ADR-0035: an entry may carry its tool's command (e.g. `docker system prune`), shown with "Lumen does not run it", never executed; allowed on `not_regenerable` entries |
| 13.24 | `feat(jev): ask Jeb on the school AI Hub` (#334, #335) | ADR-0036: Jeb is the default judge; asked only about proposed quarantines (≈170 → 0–25 requests per assessment) |
| 13.25 | `feat: keep AI review on once turned on` (#337–#340) | ADR-0037: one consent, kept in `ai-review.json`; `lumen ai on/off/status`; the app's switch and consent dialog; no keychain prompt to show the setting |
| 13.26 | `feat(api): group candidates and report disk usage` (#341) | `CandidateView.group` (developer, app cache, app data, other; with the tool), `DiskView` |
| 13.27 | `feat(web): the macOS-utility redesign` (#344–#355) | a sidebar (Overview, Clean up, Quarantine, Settings); a disk usage overview; results grouped by kind and tool, a detail sheet, a verdict filter and a sticky clean-up bar; AI review, language and permissions in Settings |
| 13.28 | `feat(desktop): open the Full Disk Access settings from the app` (#356) | a fixed `x-apple.systempreferences` address through `/usr/bin/open`; no input from the window |

Open:

| # | Scope | Needs |
| --- | --- | --- |
| 13.a | Per-volume quarantine stores (`X:\$LumenQuarantine\<SID>\` on Windows, per-volume on macOS) so items on other drives can be quarantined; today they stay where they are (ADR-0015) | an executor review |
| 13.d | Pin the release desktop job's egress and switch it to `block`; tighten the attest and release jobs after the first release | the first release's egress report |
| 13.e | release-please for versioning (ADR-0026) | |
| 13.f | Gate CI on replaying the Jeb cassette (12.2). Recorded 2026-10-22: `ai/cassettes/jev-decisions/1/jeb-2026.10.0.json`, report in `ai/reports/jev-decisions/1/` | |
| 13.g | Signing and notarization (12.8), the first tagged release, mobile device measurements (11.16d) | the owner |

## Later phases (outline)

- **Phase 11, UI:** JavaScript bootstrap (pnpm, Biome, TS 7, Turborepo), `apps/web`
  (FSD), `apps/desktop` (Tauri), `apps/mobile` (Expo).
- **Phase 12, Hardening:** benchmarks, observability, evaluation gates, release
  pipeline, signing.
- **Phase 13, First use:** what trying the product showed; see above.
