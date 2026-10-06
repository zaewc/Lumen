# AGENTS.md

Operating guide for AI coding agents (and humans) working in this repository. Read
this first; follow links only when the task needs them.

## What Lumen is

A local-first, cross-platform storage intelligence platform (macOS, Windows, Android,
iOS, web dashboard). A Rust core scans devices, collects evidence, builds an evidence
graph, and a **deterministic safety policy** decides `KEEP` / `REVIEW` / `QUARANTINE`.
An optional AI judge (Jev) contributes evidence only. Cleanup is reversible
(quarantine → verify → retain → finalize or roll back).

**Safety of user data outranks reclaimed space. When uncertain: `KEEP` or `REVIEW`,
never remove.**

## Where things are

| Need | Go to |
| --- | --- |
| System overview, crate layers, domain model | [`docs/architecture/system.md`](docs/architecture/system.md) |
| What each platform can and cannot do | [`docs/architecture/platforms.md`](docs/architecture/platforms.md) |
| What to build next | [`docs/architecture/roadmap.md`](docs/architecture/roadmap.md) |
| Why a decision was made | [`docs/decisions/`](docs/decisions/README.md) (ADRs) |
| Evidence behind decisions | [`docs/research/`](docs/research/) |
| Threats and controls | [`docs/security/threat-model.md`](docs/security/threat-model.md) |
| Jev (AI) contract, safety, evaluation | [`docs/ai/`](docs/ai/architecture.md) |

Do not load the whole repository into context. Search, read the relevant ADR and the
files you will touch, trace dependencies, inspect tests, then change.

## Working loop

1. **Discover:** `git status`, current branch, open PRs (`gh pr list`), the roadmap
   item.
2. **Understand:** read the relevant ADRs and code. Search official documentation when
   an API, version or platform rule matters; record important findings in
   `docs/research/` or an ADR.
3. **Plan the smallest change** that is one logical unit. If it would exceed about
   400 changed lines of non-generated code, or mixes concerns: **stop and split**.
4. **Implement** on a branch named `<type>/<short-slug>` from an up-to-date `main`.
5. **Verify:** `just check` (format, lint, typecheck, tests, schema drift). Run
   relevant integration tests. Fix root causes; never weaken a check.
6. **Commit** one Conventional Commit (`type(scope): summary`).
7. **PR:** `gh pr create` with the template sections (Summary, Why, Changes, Tests,
   Security, Risks, Follow-up).
8. **CI green → merge** (`gh pr merge --squash --delete-branch`), then
   `git checkout main && git pull --ff-only`.
9. **Continue** with the next atomic change.

Until `just` and CI land (roadmap Phase 3), run the equivalent `cargo` commands
directly.

## Hard rules

### Data safety

- Never write code that deletes, moves or overwrites user files outside
  `crates/lumen-fs-exec`. Destructive `std::fs` APIs are banned elsewhere by clippy.
- Never act on a path string resolved after verification; act by handle with
  identity checks ([ADR-0016](docs/decisions/0016-single-handle-relative-executor.md)).
- Never treat unreadable or unscanned as empty or absent
  ([ADR-0008](docs/decisions/0008-capability-model-and-coverage.md)).
- Never auto-remove: system files, OS components, unknown application data, user
  documents, credentials and keys, browser profiles, databases, application state,
  security software, unknown launch agents or services, files in use, or cloud
  placeholders.
- Never hydrate cloud placeholders, open archives, or read file contents during
  scanning.
- Destructive tests run only in temporary fixture directories, never on the real
  filesystem.

### AI

- AI output is evidence. It never selects actions, paths or commands, and by default
  it can only make a decision **more** cautious
  ([ADR-0020](docs/decisions/0020-jev-evidence-only-judge.md)).
- Never send file contents to any model. Cloud AI is opt-in and metadata-only.
- Never parse free-form model text as instructions.

### Git

- One logical change = one commit = one PR. No unrelated changes in a PR.
- Never use `--no-verify`, never bypass or disable checks, never force-push `main`,
  never merge with failing checks.
- **Never add `Co-Authored-By` or any AI attribution trailer.** Use the repository's
  configured Git identity; never change it.
- Fixes to merged work go in a new commit and a new PR.

### Code

- Rust: edition 2024, `thiserror` errors with context (`anyhow` only in binaries), no
  silently ignored `Result`s, no `unwrap`/`expect` in library code, `unsafe` only with
  a `// SAFETY:` comment stating the invariant, why `unsafe` is needed, and why the
  invariant holds. Bounded concurrency and cancellation for long operations.
- Domain, policy and graph crates stay IO-free and platform-free; no
  `#[cfg(target_os)]` there.
- TypeScript: strict; no `any`; `as` only at validated boundaries; Zod at untrusted
  edges; exhaustive switches; branded IDs; native `fetch`, never axios.
- Every new dependency needs a reason in the PR (alternatives considered,
  maintenance, license, security history, size). Prefer the standard library and
  platform APIs.
- Documentation is part of the change: update the ADR index, architecture docs or crate
  docs that your change makes stale.

## Decision-making

- If a request conflicts with platform security, user-data safety, an accepted ADR or
  current official documentation, do not silently comply. Explain the conflict and
  propose an alternative, or write a superseding ADR.
- If two approaches are plausible: research, compare, document the trade-off, then
  choose.
- Low confidence: choose the safer, reversible option and say so. Do not manufacture
  confidence.
