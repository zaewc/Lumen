# .github: local guide

CI and release workflows (ADR-0026, ADR-0054). A push to `main` releases to every
installed copy; workflows hold signing and updater secrets. Reviewed by CODEOWNERS.

## Rules for workflows

- `permissions: {}` at the top; each job asks only for what it needs.
- First step `step-security/harden-runner`; `actions/checkout` with
  `persist-credentials: false`.
- Pin every action to a full commit SHA with its version in a comment
  (`uses: owner/action@<sha> # vX.Y.Z`); download prebuilt tools with a checked
  SHA-256, or install Go/Rust/Maven tools at pinned versions from their
  registries with checksum verification enabled.
- Never interpolate `${{ … }}` from events or inputs into `run:`; pass it through
  `env:`.
- Secrets reach only the step that needs them, never a job that runs dependency
  build scripts (ADR-0048, ADR-0055).
- Caches are saved only from `develop` (`save-if`).

## Checks

`actionlint` and `zizmor` run in CI (`workflow-lint.yml`); run them locally if you
have them. The PR template in `pull_request_template.md` is what every PR body
follows.
