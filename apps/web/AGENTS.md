# apps/web: local guide

One Vite + React SPA (ADR-0009) used by the desktop app's webview and the local
dashboard. Feature-Sliced Design: `app` → `pages` → `widgets` → `entities` →
`shared`; a layer imports only from layers below it (Steiger checks this).

## Rules

- TypeScript is strict: no `any`, `as` only at validated boundaries, Zod at untrusted
  edges, exhaustive `switch`es, branded IDs, native `fetch`.
- Talk to the host only through `src/shared/api/`. Types for the local API come from
  `@lumen/api-client`, generated from `schemas/openapi.json`: after a Rust API change
  run `just gen`, then `pnpm --filter @lumen/api-client generate`; never edit
  `packages/api-client/src/gen/` by hand.
- Every user-facing string is in `src/shared/i18n/`, with the same keys in every
  locale (`en`, `ko`, `ja`, `zh-CN`, `es`, `ru`, `fr`); the tests fail otherwise. Words and
  tone follow [`docs/design/ux-writing.md`](../../docs/design/ux-writing.md)
  (Korean: 해요체), and screens follow
  [`docs/design/ui-principles.md`](../../docs/design/ui-principles.md).
- Show uncertainty honestly: unknown is never shown as zero or empty, and AI notes
  are labelled as AI.

## Checks

Node 24 (`.node-version`) with Corepack. From the repository root:
`pnpm --filter @lumen/web typecheck`, `pnpm --filter @lumen/web test`,
`pnpm --filter @lumen/web fsd`, `pnpm exec biome check apps/web`; all of it with
`just js-check`.
