# ADR-0026: Gate merges on hardened GitHub Actions CI with supply-chain checks

- Status: Accepted
- Date: 2026-10-06

## Context and problem statement

Pull requests must be blocked when quality or security checks fail. Supply-chain
attacks in 2026 are concrete:

- Trivy/TeamPCP (March 2026) started from `pull_request_target` and spread through
  force-pushed action tags.
- axios was compromised (March 2026).
- `arrayref` and related crates were compromised on crates.io (August 2026).

The repository is public, so CodeQL, secret scanning with push protection, and
artifact attestations are available at no cost.

## Decision drivers

- No unverified code or dependency reaches `main`.
- Least-privilege workflows.
- Release provenance users can verify.

## Considered options

1. GitHub Actions with SHA-pinned actions, minimal permissions and required checks:
   CodeQL, cargo-deny, OSV-Scanner, gitleaks, zizmor, actionlint and the per-OS test
   matrix. Renovate for updates; release-please for versioning; attestations on
   releases.
2. Minimal CI (build and test only) with Dependabot.

## Decision outcome

Chosen option: **1**. Workflows land incrementally during bootstrap, one per PR.

Workflow hygiene:

- `permissions: {}` at the top of every workflow, granted per job.
- Every action pinned to a full commit SHA, with the version in a comment.
- `actions/checkout` uses `persist-credentials: false`.
- No `pull_request_target`, and no `workflow_run` that consumes PR artifacts.
- Caches are saved only on `main`, and never restored in release jobs.
- `step-security/harden-runner` with egress audit (block in release jobs).
- Explicit runner labels (`ubuntu-24.04`, `windows-2025`, `macos-26`; plus `macos-15`
  for minimum-OS checks), never `-latest`.

Required checks:

- Rust format, clippy, tests and MSRV.
- TypeScript format, lint, typecheck and tests, once JavaScript exists.
- Schema and codegen drift.
- `cargo deny check` and OSV-Scanner.
- gitleaks.
- zizmor and actionlint.
- CodeQL advanced setup for `rust`, `javascript-typescript` and `actions` with
  `security-extended`.
- A Conventional Commits PR-title check, run on `pull_request`.

Custom Opengrep/Semgrep rules (own rules only) ban raw filesystem deletion outside
the executor and unsafe FFI patterns.

Dependencies:

- Renovate with `minimumReleaseAge` (3–7 days), digest pinning of actions, and grouped
  non-major updates.
- Dependabot security alerts on.
- `cargo-vet` for filesystem, IPC and crypto crates once audits are seeded.
- Any new crate or package in a lockfile diff is a review item.

Governance:

- Repository rulesets on `main`: PR required, required checks, linear history, no force
  push or deletion, and CODEOWNERS review for safety paths (`lumen-policy`,
  `lumen-fs-exec`, IPC, workflows).
- A merge queue once there are multiple concurrent contributors.

Releases:

- release-please v4 manifest mode.
- Builds run in a reusable workflow with `actions/attest` provenance and an SBOM.
- Immutable releases; signing and notarization in a protected `release` environment.

### Consequences

- Good: defense in depth against compromised dependencies and workflows.
- Bad: more CI minutes and more required checks to keep green.

Rejected: option 2 leaves the repository exposed to the 2026 attack patterns.

## More information

- [Security research, §B–C](../research/09-security-devops-quality.md)
- [Rust ecosystem research, R8](../research/02-rust-ecosystem.md)
