# Contributing to Lumen

Thank you for helping. Lumen handles people's files, so the bar for safety and
clarity is high, and the process favours many small changes over large ones.

## Before you start

- Read [`AGENTS.md`](AGENTS.md): it is the operating guide for humans and AI agents
  alike.
- Check the [roadmap](docs/architecture/roadmap.md) and the
  [ADR index](docs/decisions/README.md). If your change contradicts an accepted ADR,
  propose a new ADR first.

## Workflow

1. Branch from an up-to-date `main`: `<type>/<short-slug>`.
2. Make **one logical change**. Split anything larger than about 400 changed lines of
   non-generated code.
3. Run all checks locally (`just check` once available). Never use `--no-verify`.
4. Commit once, using [Conventional Commits](https://www.conventionalcommits.org/):
   `type(scope): summary`. Types: `feat`, `fix`, `docs`, `test`, `refactor`, `perf`,
   `build`, `ci`, `chore`, `revert`. Scopes are crate or area names (`domain`,
   `policy`, `scan`, `fs-exec`, `platform-macos`, `web`, `adr`, …).
5. Open a PR whose title equals the commit subject and whose body has the sections
   **Summary, Why, Changes, Tests, Security, Risks, Follow-up**.
6. PRs are squash-merged after CI passes and review; the branch is deleted.

Fixes to merged work are new PRs; history on `main` is never rewritten. Do not add
`Co-Authored-By` or AI attribution trailers.

## Quality bar

- Tests accompany behaviour changes: unit tests, plus property tests for invariants
  (paths, policy, sizes, state machines) and integration tests on temporary fixture
  filesystems. Never test destructive behaviour on a real filesystem.
- Rust: `rustfmt`, clippy clean with `-D warnings`, typed errors, no unexplained
  `unsafe`.
- TypeScript: strict mode, no `any`, Biome clean.
- New dependencies are justified in the PR description: alternatives, maintenance,
  license, security history, size.
- Docs that your change makes stale are updated in the same PR.

## Safety-critical areas

Changes to `crates/lumen-policy`, `crates/lumen-fs-exec`, IPC code, Jev
prompts/schemas/datasets, and `.github/workflows` require review from CODEOWNERS and,
where they change the attack surface, an update to the
[threat model](docs/security/threat-model.md).

## Reporting security issues

See [`SECURITY.md`](SECURITY.md). Do not open public issues for vulnerabilities.
