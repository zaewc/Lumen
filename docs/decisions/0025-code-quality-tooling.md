# ADR-0025: Enforce code quality with rustfmt/clippy/nextest and Biome/oxlint/Steiger on TypeScript 7

- Status: Accepted
- Date: 2026-10-06

## Context and problem statement

Agents write most of the code. Lint, format, type and test gates are the first line of
review and must be fast, strict and consistent.

TypeScript 7 (the native Go port) is GA but has no programmatic API until 7.1.
typescript-eslint supports only TypeScript < 6.1, and ESLint 10 is blocked on
TypeScript 7.

## Decision drivers

- Speed and strictness.
- Few tools, each with a clear job.
- Promise-safety checks, because an unawaited quarantine or rollback promise is a
  correctness bug.

## Considered options

1. Rust: rustfmt, clippy (workspace lints, `-D warnings`), cargo-nextest + doctests,
   proptest, insta, cargo-mutants (nightly), cargo-fuzz (nightly).
   TypeScript: TypeScript 7 strict (`tsc --noEmit`), Biome 2.x (format + lint), oxlint
   `--type-aware` restricted to promise-safety rules, Steiger for FSD,
   dependency-cruiser for package boundaries, Vitest, Playwright.
2. ESLint 10 + typescript-eslint + Prettier.

## Decision outcome

Chosen option: **1**.

Rust:

- `cargo fmt --check`.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` with workspace lints
  (ADR-0003).
- `cargo nextest run --workspace --locked` and `cargo test --doc`.
- Property tests (proptest) for path normalisation, policy invariants, size
  aggregation, graph invariants, serialisation round-trips and quarantine state
  transitions.
- Snapshot tests (insta) for schemas, decisions and explanations.
- Benchmarks with criterion and stored baselines; fuzzing with cargo-fuzz on every
  parser of untrusted on-disk data.

TypeScript:

- `strict: true` plus `noUncheckedIndexedAccess`, `exactOptionalPropertyTypes` and
  `noImplicitOverride`. Every tsconfig option is explicit, because TS 7 changed
  defaults.
- `any` is banned by lint. Type assertions are allowed only in validated boundary
  modules. Exhaustive `switch` over discriminated unions; branded IDs; `readonly`
  data.
- Tools that need the compiler API use `@typescript/typescript6` side by side until
  TS 7.1.
- Biome formats and lints; oxlint type-aware runs `no-floating-promises` and
  `no-misused-promises` in CI; Steiger and dependency-cruiser enforce architecture
  boundaries; Vitest with MSW and React Testing Library covers units and components;
  Playwright runs end-to-end tests against a real core on a fixture filesystem.

Git hooks (lefthook):

- pre-commit: format and lint staged files, gitleaks.
- commit-msg: Conventional Commits check.
- pre-push: clippy and cargo-deny.

Hooks are never bypassed (no `--no-verify`).

### Consequences

- Good: fast checks; architecture rules are machine-enforced.
- Bad: two TypeScript installs until 7.1; two linters (Biome, oxlint) with distinct
  scopes.

Rejected: option 2 is slower, needs three tools, and is blocked on TypeScript 7.
Revisit if React Compiler lint rules become essential.

## More information

- [Rust ecosystem research, R7](../research/02-rust-ecosystem.md)
- [Web and monorepo research, Implications 6–7 and 11](../research/07-web-and-monorepo.md)
- [Security research, §B](../research/09-security-devops-quality.md)
