# ADR-0024: Use pnpm workspaces, Turborepo and a justfile alongside the Cargo workspace

- Status: Accepted
- Date: 2026-10-06

## Context and problem statement

The repository contains Rust (the Cargo workspace), TypeScript (web app, UI package,
generated API client, mobile JS), Swift and Kotlin (Expo Modules, native glue), plus
AI datasets and schemas. Contributors and agents need one obvious entry point for
generate → check → test.

## Decision drivers

- Native build tools stay in charge of their languages.
- One command surface for humans, agents and CI.
- Supply-chain controls on JavaScript dependencies.

## Considered options

1. Cargo workspace + pnpm workspaces (with catalogs) + Turborepo for JavaScript tasks
   + a `justfile` as the polyglot entry point.
2. Nx.
3. moon v2.
4. Bazel.

## Decision outcome

Chosen option: **1**.

Layout:

```text
apps/web        Vite SPA (desktop webview + browser dashboard)
apps/desktop    Tauri host (src-tauri) embedding the core
apps/mobile     Expo app with local Expo Modules (Swift/Kotlin)
crates/         Rust workspace members (ADR-0003)
packages/ui     design tokens and primitives
packages/api-client  generated from the OpenAPI spec (ADR-0011)
packages/config shared tsconfig and Biome config
schemas/        exported JSON Schemas and OpenAPI spec (generated, committed)
ai/             prompts, schemas, evaluators, fixtures, adversarial, golden
xtask/          Rust automation
docs/           documentation
```

pnpm:

- Exact versions via catalogs (`catalogMode: strict`) and the pnpm version pinned
  through `packageManager`.
- `minimumReleaseAge` raised to 3 days; `blockExoticSubdeps` and `strictDepBuilds`
  kept on; an explicit `allowBuilds` allowlist; `trustPolicy: no-downgrade`.

Tasks:

- Turborepo orchestrates JavaScript tasks only; its Cargo support is experimental.
- `just` recipes: `gen`, `fmt`, `lint`, `typecheck`, `test`, `check` (all
  pre-commit-equivalent checks), and `e2e`.
- Xcode and Gradle keep their native builds.

### Consequences

- Good: each ecosystem uses its native tool; one entry point for agents.
- Bad: no cross-language build cache; revisit moon v2 if that starts to hurt.

Rejected: Nx is heavy and its Rust support is a community plugin; Bazel's cost far
exceeds the team's size.

## More information

- [Web and monorepo research, Implication 9](../research/07-web-and-monorepo.md)
- [Security research, §B](../research/09-security-devops-quality.md)
