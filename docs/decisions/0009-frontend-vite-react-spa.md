# ADR-0009: Build the UI as one Vite + React SPA with Feature-Sliced Design

- Status: Accepted
- Date: 2026-10-06

## Context and problem statement

The owner's initial direction suggested Next.js, React, strict TypeScript, Tailwind,
TanStack Query, Zod, Axios and Feature-Sliced Design (FSD). The UI must run inside the
Tauri webview (ADR-0004) and, optionally, in a browser as a local dashboard
(ADR-0010).

Research findings:

- Tauri supports only static or SPA frontends.
- Next.js `output: 'export'` disables Server Actions, rewrites, headers, cookies,
  request-reading route handlers and dynamic routes without `generateStaticParams`.
  App Router and React Server Components become build-time overhead only.
- Running a Next.js server on end-user machines would import its server-side
  vulnerability history (e.g. CVE-2025-55182, a CVSS 10 pre-auth RCE in RSC, and two
  critical RCEs in August 2026).
- Axios was compromised on 2026-03-31: versions 1.14.1 and 0.30.4 shipped a remote
  access trojan.

## Decision drivers

- Compatibility with Tauri's static model.
- No server runtime on user machines.
- One UI codebase for desktop and browser.
- Supply-chain minimalism.

## Considered options

1. Vite + React + TanStack Router + TanStack Query + Tailwind CSS v4 + Zod, native
   `fetch`, FSD.
2. Next.js with static export.
3. Next.js server bundled with the local daemon.

## Decision outcome

Chosen option: **1**. This deliberately deviates from the suggested Next.js and Axios.

- `apps/web`: a single SPA (Vite, React 19.x, TanStack Router, TanStack Query 5,
  Tailwind 4 with CSS-first config, Zod 4), exact versions pinned at bootstrap.
- HTTP uses native `fetch` through the generated client (ADR-0011). Axios is not
  used.
- **Feature-Sliced Design v2.1** inside `apps/web/src`: `app`, `pages`, `widgets`,
  `features`, `entities`, `shared`; slices expose public APIs via `index.ts`;
  cross-entity imports only through `@x`. TanStack Router route files live in the
  `app` layer and only re-export pages. Steiger enforces FSD rules.
- A `LumenTransport` port in `shared/api` has two adapters: `TauriTransport`
  (IPC) and `HttpTransport` (browser dashboard). UI code never builds URLs or calls
  Tauri directly.
- The UI renders decisions; it never makes them. Mutations carry server-issued plan
  IDs and hashes.
- TypeScript is strict everywhere; `any` is banned by lint; type assertions are
  allowed only at validated trust boundaries.
- `react-server-dom-*` is forbidden in the dependency graph.
- Shared design tokens and primitives live in `packages/ui`; mobile reuses tokens,
  not components.

Next.js remains a candidate for a separately hosted marketing or fleet site, which
would get its own ADR and threat model.

### Consequences

- Good: no server to patch on user machines; one UI for two hosts.
- Good: runtime-ID routing is native to an SPA.
- Bad: no SSR; acceptable for a local, authenticated tool.

## More information

- [Web and monorepo research](../research/07-web-and-monorepo.md)
- ADR-0010, ADR-0011, ADR-0025
