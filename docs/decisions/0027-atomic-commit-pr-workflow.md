# ADR-0027: Ship every atomic change as one Conventional Commit in one pull request

- Status: Accepted
- Date: 2026-10-06

## Context and problem statement

Lumen will be developed over a long period, largely by AI coding agents. Large,
mixed-purpose changes are hard to review, hard to revert, and hide safety regressions.

## Decision drivers

- Every change independently understandable, reviewable and revertible.
- History that reads as a changelog.
- No bypassing of quality gates.

## Considered options

1. One logical change = one commit = one PR, squash-merged with a Conventional Commits
   title.
2. Feature branches with many commits per PR.

## Decision outcome

Chosen option: **1**.

Loop: inspect state and documentation → smallest change → format → lint → typecheck →
unit and relevant integration tests → commit → push branch → `gh pr create` → CI →
review → squash merge with branch deletion → update local `main` → next change.

Rules:

- PR titles and commit subjects follow Conventional Commits: `type(scope): summary`.
  Types are `feat`, `fix`, `docs`, `test`, `refactor`, `perf`, `build`, `ci`, `chore`
  and `revert`. Scopes are crate or area names (`policy`, `scanner`, `fs-exec`,
  `web`, `adr`, `research`, …).
- PR bodies use the sections Summary, Why, Changes, Tests, Security, Risks, Follow-up
  (`.github/pull_request_template.md`).
- A fix to a merged change is a new commit in a new PR. History is never rewritten on
  `main`.
- Never `--no-verify`, never `--admin` merges past failing checks.
- No `Co-Authored-By` or AI attribution trailers. Commits use the repository's
  configured Git identity, never an impersonated one.
- Automation: `gh pr merge --auto --squash --delete-branch --match-head-commit <sha>`
  once required checks exist.
- The very first commit (README) went directly to `main` because an empty repository
  has no base branch; this is the only exception.

### Consequences

- Good: clean, bisectable history; small reviews.
- Bad: more PRs, and sequential dependencies between them; tolerable with automation.

## More information

- [Security research, governance and PR automation](../research/09-security-devops-quality.md)
- `CONTRIBUTING.md`, `AGENTS.md`
