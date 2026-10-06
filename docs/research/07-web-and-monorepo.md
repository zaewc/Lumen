# Web Dashboard Stack and Polyglot Monorepo Architecture

> Researched: 2026-10-05 · Scope: frontend stack (Next.js/React/TS/Tailwind/TanStack/Zod), fit for a Tauri webview plus a daemon-served local dashboard, Feature-Sliced Design, lint/format, monorepo tooling for Rust + TS (+ Swift/Kotlin), Rust-to-TS type sharing, localhost-daemon web security, and testing.

## Summary

- **Don't use Next.js for Lumen's local UI. Build one Vite 8 + React 19.3 + TanStack Router + TanStack Query 5 single-page app (SPA).** Tauri requires a static/SPA frontend ("Tauri doesn't support server-based solutions"). Next.js `output: 'export'` turns off Server Actions, proxy, rewrites, headers, cookies, ISR, request-reading Route Handlers, and dynamic routes without `generateStaticParams`. That leaves Next's App Router and RSC model as build-time overhead only. Next.js stays an option for a future *hosted* marketing or fleet site.
- **Running a Next.js server on end-user machines adds a lot of attack surface.** In 2025–2026 there were: CVE-2025-55182 "React2Shell" (CVSS 10 pre-auth RCE in RSC, Dec 2025); two Critical RCEs in Aug 2026, one Windows-only; and monthly security releases since July 2026. A static bundle served by the Rust daemon or by Tauri's custom protocol avoids all of this.
- **Current versions (npm/crates.io, 2026-10-05):** Next 16.3.8 (Active LTS), React 19.3.0, TypeScript 7.0.2 (Go-native), Tailwind 4.3.3, TanStack Query 5.104.1, TanStack Router 1.170.x, Zod 4.6.5, Vite 8.3.2 (Rolldown), Vitest 5.0.3, MSW 3.0.2, Playwright 1.63.0, Biome 2.5.15, ESLint 10.12.0, oxlint 1.87.0, pnpm 12.9.1, Turborepo 2.11.7, Nx 23.2.1, moon 2.6.0, Tauri 2.12.1.
- **TypeScript 7 is GA (2026-07-08) and has no programmatic API until 7.1.** typescript-eslint supports only TS `<6.1`, so tools that need the compiler API must use `@typescript/typescript6` side by side. Plan for two TS installs until 7.1 ships (no date announced; the team cites a 3–4-month feature cadence, so roughly late 2026 — unverified). TS 7 also changes defaults (`types: []`, `rootDir: ./`, `strict: true`), so shared tsconfigs must be explicit.
- **Rust is the single source of truth for the API.** Use utoipa 6 to emit OpenAPI (it added 3.2 support), commit the spec, and generate the TS client and Zod schemas (hey-api or openapi-typescript + openapi-fetch). The same spec can feed Swift/Kotlin generators. Use tauri-specta (still `2.0.0-rc.25`) only for the small desktop-shell command surface, pinned to an exact version. ts-rs (types only, TS only) and typeshare (types for TS/Swift/Kotlin, no transport) are supporting options.
- **Use native `fetch`, not axios.** Axios was compromised on 2026-03-31: the malicious versions 1.14.1 and 0.30.4 shipped a RAT through the `plain-crypto-js` dependency and were attributed to a North Korean state actor. Axios also had 2025 SSRF and DoS CVEs. pnpm 11+'s default `minimumReleaseAge: 1440` would have blocked that 3-hour malicious window.
- **Monorepo: pnpm 12 workspaces with catalogs, a root Cargo workspace, and Turborepo for the JS task graph.** Turborepo's Cargo support is still experimental. Swift and Kotlin keep their native builds. A thin `justfile` or `cargo xtask` is the polyglot entry point (Spacedrive uses Cargo + Bun + `just`). Revisit moon v2 (WASM toolchain plugins, Rust-aware) if cross-language caching starts to hurt. Nx is rejected as too heavy, and its Rust support is a community plugin.
- **Linting:** Biome 2.5 for format and lint (500+ rules, GritQL plugins with fixes), Steiger for Feature-Sliced Design (FSD) rules, and dependency-cruiser or Biome import restrictions for package boundaries. Add oxlint `--type-aware` (tsgolint v7, stable 2026-07-22, works with TS 7, 59/61 typescript-eslint type-aware rules) in CI for the promise-safety rules. ESLint 10 is flat-config-only and its TS-7 support is blocked.
- **Adopt FSD v2.1 inside the UI app,** with pages-first slicing, `index.ts` public APIs and `@x` cross-imports only between entities. Steiger is still beta (0.7.0). FSD's documented Next.js workaround is renaming the FSD layers to `_app`/`_pages`. With TanStack Router the routes simply live under the `app` layer and re-export pages.
- **Treat the localhost daemon API as hostile-reachable:**
  - Bind to 127.0.0.1/::1 only.
  - Validate `Host` (defeats DNS rebinding) and `Origin` (including on WebSocket upgrades).
  - Send no CORS headers by default.
  - Require a per-install bearer token in a header, not a cookie.
  - Ship the browser dashboard **off by default**.
  
  Chrome's Local Network Access (shipped in Chrome 142) prompts for public-to-loopback requests but exempts loopback-to-loopback, did not cover WebSockets at launch (reportedly extended in Chrome 147), and can be disabled by enterprise policy. Treat it as defense in depth only.
- **Desktop path:** Tauri custom protocol (not `tauri-plugin-localhost`, which Tauri itself warns about), then IPC to the Tauri Rust core, then a Unix domain socket (in a 0700 per-user directory) or a named pipe (first-instance flag + current-user DACL, remote clients rejected) to the daemon with a peer-credential check. No TCP port is opened for desktop users.
- **Testing:**
  - Vitest 5 (Node ≥ 22.12, Vite ≥ 6.4; unawaited async assertions now fail; Browser Mode trace view).
  - React Testing Library 16.3.
  - MSW 3 (ESM-only, Node 22+, TS ≥ 5.9, `msw/http` entry points, experimental `defineNetwork`).
  - Playwright 1.63 for end-to-end tests against a real daemon running on a fixture filesystem.
- **Tailwind v4 uses CSS-first config** (`@import "tailwindcss"`, `@theme`, `@source`), so a shared `packages/ui` exports a CSS theme file instead of a JS preset. Tailwind Labs announced it is joining Shopify (2026-09-09). There is no technical impact yet; note it as a governance item.

## Findings

### 1. Current versions of the core web stack (verified 2026-10-05)

Versions come from the npm registry `dist-tags.latest` and the crates.io API, queried on 2026-10-05. Release dates come from registry `time` metadata unless a blog post is cited.

| Package | Latest stable | Published | Notes / source |
| --- | --- | --- | --- |
| next | **16.3.8** (Active LTS); 15.5.27 = Maintenance LTS | 2026-09-30 | [Next.js blog](https://nextjs.org/blog), [16.3 release](https://nextjs.org/blog/next-16-3) (2026-08-03) |
| react / react-dom | **19.3.0** | 2026-09-09 | [react.dev/versions](https://react.dev/versions); no React 20 announced |
| babel-plugin-react-compiler | 1.0.0 | — | [React Compiler v1.0](https://react.dev/blog/2025/10/07/react-compiler-1) (2025-10-07) |
| typescript | **7.0.2** (Go-native `tsc`); `@typescript/typescript6` 6.0.2 for API consumers; `next` tag = 7.1.0-dev | 2026-07-08 | [Announcing TypeScript 7.0](https://devblogs.microsoft.com/typescript/announcing-typescript-7-0/) |
| tailwindcss | **4.3.3** (`v3-lts` = 3.4.19) | 2026-07-16 | [Tailwind blog](https://tailwindcss.com/blog) (v4.3 on 2026-05-08) |
| @tanstack/react-query | **5.104.1** | 2026-10-02 | npm (no v6 stable) |
| @tanstack/react-router | **1.170.41** | — | npm, [docs](https://tanstack.com/router/latest/docs/framework/react/overview) |
| zod | **4.6.5** | 2026-09-13 | npm, [JSON Schema docs](https://zod.dev/json-schema) |
| @standard-schema/spec | 1.1.0 | — | [standardschema.dev](https://standardschema.dev/) |
| axios | 1.20.0 | 2026-08-26 | see §8 for the security history |
| vite | **8.3.2** (8.0.0 on 2026-03-12) | 2026-10-01 | [Vite 8 announcement](https://vite.dev/blog/announcing-vite8) |
| @vitejs/plugin-react | 6.1.2 | — | npm |
| @biomejs/biome | **2.5.15** | 2026-09-30 | [Biome v2.5](https://biomejs.dev/blog/biome-v2-5/) (2026-06-05) |
| eslint | **10.12.0** (`maintenance` = 9.39.5) | 2026-10-02 | [ESLint v10.0.0](https://eslint.org/blog/2026/02/eslint-v10.0.0-released/) (2026-02-06) |
| typescript-eslint | 8.71.1 (supports TS `>=4.8.4 <6.1.0`) | 2026-10-05 | [issue #12518](https://github.com/typescript-eslint/typescript-eslint/issues/12518) |
| eslint-plugin-react-hooks | 7.1.1 | — | npm (hosts the React Compiler lint rules) |
| prettier | 3.9.9 | — | npm |
| oxlint | **1.87.0**; tsgolint v7 stable | 2026-10-05 | [Type-aware linting stable](https://oxc.rs/blog/2026-07-22-type-aware-linting-stable) |
| steiger | **0.7.0** (beta) | 2026-09-25 | [steiger repo](https://github.com/feature-sliced/steiger) |
| dependency-cruiser | 18.5.0 | — | npm |
| eslint-plugin-boundaries | 7.2.0 | — | [repo](https://github.com/javierbrea/eslint-plugin-boundaries) |
| pnpm | **12.9.1** (12.0.0 on 2026-08-26, Rust rewrite) | 2026-10-03 | [pnpm 12.0](https://pnpm.io/blog/releases/12.0) |
| turbo | **2.11.7** | 2026-10-02 | [Rust guide](https://turborepo.dev/docs/guides/tools/rust) |
| nx | 23.2.1 | 2026-09-09 | npm |
| @moonrepo/cli | 2.6.0 (2.0.0 on 2026-02-18) | 2026-10-05 | [InfoQ on moon v2](https://www.infoq.com/news/2026/05/moonrepo-2-release/) |
| vitest | **5.0.3** (5.0.0 on 2026-09-03) | 2026-09-30 | [Vitest 5](https://vitest.dev/blog/vitest-5.html) |
| @playwright/test | **1.63.0** | 2026-09-04 | npm |
| msw | **3.0.2** (3.0.0 on 2026-09-28; blog dated 09-30) | 2026-10-03 | [Introducing MSW 3.0](https://mswjs.io/blog/introducing-msw-3.0) |
| @testing-library/react | 16.3.3 | — | npm |
| orval / openapi-typescript / openapi-fetch / @hey-api/openapi-ts | 8.40.0 / 7.13.0 / 0.17.0 / 0.99.0 | openapi-typescript + openapi-fetch last published 2026-02-11; hey-api 0.99.0 on 2026-06-22 | npm; [hey-api: "pin an exact version"](https://heyapi.dev/openapi-ts/get-started) |
| @tauri-apps/api, @tauri-apps/cli, crate `tauri` | **2.12.1** (tauri 3.0.0-alpha.4 published) | 2026-10-01 | npm + crates.io |

Rust crates (crates.io API, `max_stable_version`):

| Crate | Version | Note |
| --- | --- | --- |
| schemars | 1.2.2 | |
| ts-rs | 12.0.1 | MSRV 1.88 ([repo](https://github.com/Aleph-Alpha/ts-rs)) |
| specta | 1.0.5 stable; **2.0.0-rc.25** is what tauri-specta v2 uses | last published 2026-05-07 (crates.io, re-checked 2026-10-06) |
| tauri-specta | 1.0.2 stable (Tauri 1); **2.0.0-rc.25** for Tauri 2 | last published 2026-05-08; the GitHub README still mentions rc.21, so trust crates.io |
| typeshare | 1.0.5 | |
| utoipa | **6.0.0** | 2026-09-22; adds OpenAPI 3.2 (PR #1555), MSRV 1.88; breaking: YAML feature now uses `yaml_serde`, expression-ignore removed. Whether 3.1 emission remains selectable is not stated in the release notes (unverified) ([release](https://github.com/juhaku/utoipa/releases/tag/utoipa-6.0.0)) |
| axum | 0.8.9 | |
| tokio | 1.53.2 | |

### 2. Next.js 16.x, React 19.x, TypeScript 7, Tailwind v4, TanStack, Zod

**Next.js 16 → 16.3**
- **16.0** ([blog](https://nextjs.org/blog/next-16), 2025-10-21):
  - Turbopack is stable and the default bundler (`--webpack` to opt out).
  - Caching is opt-in through **Cache Components** (`cacheComponents: true` + the `'use cache'` directive). It replaces `experimental.ppr`/`dynamicIO`, and "all dynamic code … is executed at request time by default".
  - `middleware.ts` is renamed to `proxy.ts` (Node runtime).
  - `reactCompiler: true` is stable but not on by default.
  - `next lint` is removed ("Use Biome or ESLint directly").
  - Async-only `params`/`cookies()`/`headers()`.
  - Node ≥ 20.9, TS ≥ 5.1.
  - New cache APIs: `revalidateTag(tag, profile)`, `updateTag()`, `refresh()`.
- **16.3** ([blog](https://nextjs.org/blog/next-16-3), 2026-08-03):
  - Up to 90% less dev memory; Turbopack FS cache now on for `next build`.
  - `next build` can type-check with TS 7.
  - Native Node streams for SSR.
  - `catchError` error boundaries, `next/root-params`, `import.meta.glob`.
  - Opt-in **Instant Navigations** (`cacheComponents: true` + `partialPrefetching: true`), which "will become the default in a future major version".
  - Experimental: Rust React Compiler (`experimental.turbopackRustReactCompiler`) and `experimental.useOffline`.
- **Support policy:** an Active LTS (16.3.x) and a Maintenance LTS (15.5.x) receive **pre-announced monthly security releases** ([program post](https://nextjs.org/blog/next-security-release-program), July 2026).
  - The Aug 2026 release (published 2026-08-25, moved forward from its announced date; patched in 16.3.3 and 15.5.24) fixed two **Critical unauthenticated RCEs**: AVIF optimization through libheif/sharp (GHSA-2xp9-vwfh-vxw4; patched releases *disable* AVIF optimization pending upstream), and CVE-2026-75604 on Windows-hosted servers that used both the Pages and App Routers without Cache Components, with "no known workaround" ([Aug 2026 release](https://nextjs.org/blog/august-2026-security-release)). The Windows one is directly relevant to any idea of running a Next server on Windows end-user machines.
  - Sept 2026 brought one High, five Medium and one Low, plus an out-of-band fix for a critical upstream issue ([blog index](https://nextjs.org/blog)).

**React**
- 19.2 (2025-10-01) added `<Activity>`, `useEffectEvent`, `cacheSignal`, Performance Tracks, and partial pre-rendering in React DOM ([React 19.2](https://react.dev/blog/2025/10/01/react-19-2)).
- 19.3.0 shipped on 2026-09-09 ([versions](https://react.dev/versions)). I did not verify its feature list.
- React Compiler 1.0 (2025-10-07) is production-ready. Its lint rules ship in `eslint-plugin-react-hooks` `recommended`, and Vite/Expo/Next templates can enable it ([blog](https://react.dev/blog/2025/10/07/react-compiler-1)).
- **Security precedent:** CVE-2025-55182 ("React2Shell", CVSS 10.0) was a pre-auth RCE caused by unsafe deserialization of RSC Flight payloads. Default `create-next-app` production builds were exploitable. The vulnerable code lives in `react-server-dom-webpack/-parcel/-turbopack` 19.0, 19.1.0, 19.1.1 and 19.2.0; it was fixed in 19.0.1/19.1.2/19.2.1 and Next 15.x/16.0.7 ([React advisory](https://react.dev/blog/2025/12/03/critical-security-vulnerability-in-react-server-components), [Vercel summary](https://vercel.com/changelog/cve-2025-55182), [Wiz](https://www.wiz.io/blog/critical-vulnerability-in-react-cve-2025-55182)). It was followed by further RSC CVEs — CVE-2025-55184 and CVE-2025-67779 (DoS, 7.5), CVE-2025-55183 (source exposure, 5.3) and CVE-2026-23864 (DoS, 7.5, 2026-01-26) — so the first patch level was not sufficient. The React advisory states: "If your app's React code does not use a server, your app is not affected" — a client-only SPA does not deploy an RSC endpoint at all. To keep it that way, Lumen should forbid `react-server-dom-*` packages in the dependency graph (dependency-cruiser/pnpm rule).

**TypeScript 7** ([announcement](https://devblogs.microsoft.com/typescript/announcing-typescript-7-0/), 2026-07-08)
- It is a Go port with 8–12× faster full builds and 6–26% less memory. `npm i -D typescript` now installs the native `tsc`, and the `@typescript/native-preview`/`tsgo` preview names have been retired into `typescript@next`.
- **No programmatic API in 7.0.** A new and different API is expected in 7.1.
- Tools that need the API use the `@typescript/typescript6` compatibility package (`tsc6`). Vue, MDX, Astro, Svelte and Angular template tooling cannot use 7.0 yet.
- Changed defaults (confirmed against the announcement): `strict: true`, `target` = current stable ES, `module: esnext`, `rootDir: ./` (no longer inferred), and **`types: []` (no longer auto-discovers `@types/*`)**; the `es5` target is gone. Lumen's shared `tsconfig` in `packages/config` must therefore list `types` explicitly (e.g. `["vite/client"]`, `["vitest/globals"]`) and set `rootDir`. `baseUrl` deprecation was not re-checked here (unverified). The team recommends migrating to 6.0 first.
- typescript-eslint 8.x supports `<6.1.0`. The workaround is an npm alias such as `"typescript": "npm:@typescript/typescript6@^6.0.2"` alongside native TS 7 ([typescript-eslint #12518](https://github.com/typescript-eslint/typescript-eslint/issues/12518)). The announcement gives no 7.1 date; it only says 7.1 is "on the horizon" and that feature releases are expected every 3–4 months, which would put 7.1 around Oct–Nov 2026 (inference, unverified).

**Tailwind CSS v4**
- v4.0 (2025-01-22) introduced the CSS-first configuration: `@import "tailwindcss";`, design tokens in `@theme { … }`, and extra scan roots via `@source`. The JS config is optional.
- v4.1 added text-shadow and mask utilities. v4.3 (2026-05-08) added first-party scrollbar styling, logical-property, `zoom` and `tab-size` utilities, and better `@variant` ([blog index](https://tailwindcss.com/blog)).
- Monorepo implication: a shared UI package exports a `theme.css` with `@theme` tokens, and each app adds `@source "../../packages/ui/src";`. That mechanism comes from v4 docs as I remember them; check the exact path syntax when implementing.

**TanStack**
- Query is on v5 (5.104.1), with no stable v6 on npm.
- Router 1.x offers fully inferred type-safe routes and file-based routing through the Vite plugin. It treats search params as validated, typed state and has a built-in loader cache that integrates with TanStack Query ([docs](https://tanstack.com/router/latest/docs/framework/react/overview)).

**Zod 4 and Standard Schema**
- Zod 4.6.5. `z.toJSONSchema()` is stable and targets draft-2020-12, draft-07, draft-04 and OpenAPI 3.0.
- **`z.fromJSONSchema()` exists but is explicitly experimental** ("not considered part of Zod's stable API") ([zod.dev/json-schema](https://zod.dev/json-schema)).
- Types such as `bigint`, `date`, `map`, `set` and transforms are unrepresentable in JSON Schema. That matters for u64 byte counts (see Risks).
- Standard Schema (`StandardSchemaV1`, the `~standard` property) is the cross-library validator interface. A sibling spec, **Standard JSON Schema**, standardizes conversion to JSON Schema targets ([standardschema.dev](https://standardschema.dev/)).
- Zod, Valibot and ArkType implement Standard Schema. That comes from the ecosystem README; the spec page itself does not list implementers.

### 3. Is Next.js right for a local UI that is also embedded in Tauri?

**Hard constraints.** Tauri's Next.js guide requires `output: 'export'` with `frontendDist: "../out"` and `images.unoptimized: true`, because "Tauri doesn't support server-based solutions" ([Tauri + Next.js](https://v2.tauri.app/start/frontend/nextjs/)). Next's static-export docs (v16.3.8) list as **unsupported** ([static exports](https://nextjs.org/docs/app/guides/static-exports)):
- dynamic routes with `dynamicParams: true` or without `generateStaticParams()`
- Route Handlers that read `Request`
- cookies, rewrites, redirects, headers
- Proxy
- ISR
- default image optimization
- Draft Mode
- **Server Actions**
- intercepting routes

Server Components still work, but only at **build time**.

What that means for Lumen:
- Lumen's data is per-device and runtime-only (scan results, evidence graph, quarantine state). Every useful screen becomes a Client Component fetching from the daemon, which is exactly the SPA model, minus the dynamic routes.
- `/apps/[bundleId]` or `/scans/[id]` cannot be prerendered because IDs are unknown at build time. They must become `?id=` search params or client-side routing workarounds.
- That leaves Next's App Router, RSC, `'use cache'`, Instant Navigations and Partial Prefetching as unused complexity. Those are the features Next is actively investing in.
- Next also adds framework constraints such as `default.js` for parallel routes, async-only APIs and pre-announced monthly security releases, which bring no benefit here.

**Comparison (desktop webview plus daemon-served dashboard):**

| Criterion | Next.js 16 `output: 'export'` | **Vite 8 + React 19 + TanStack Router** |
| --- | --- | --- |
| Tauri support | Supported with config caveats | First-class (Tauri's default templates are Vite-based) |
| Runtime server | None (export) | None |
| Dynamic routes for runtime IDs | Not supported without `generateStaticParams` | Native (`/scans/$scanId`), type-safe params and search |
| Data fetching model | Client Components + SWR/Query; RSC unused | TanStack Router loaders + TanStack Query |
| Bundler | Turbopack (in-house) | Rolldown (Vite 8), shared with Vitest 5 |
| Test stack alignment | Needs separate Jest/Vitest config | Vitest reuses Vite config |
| Upgrade churn | High (16.x: caching model, proxy rename, prefetch rewrite) | Lower; router 1.x stable |
| Security exposure if someone later runs `next start` | Server RCE history (2025–2026) | N/A |

**Decision:** Use Vite + React + TanStack Router as one SPA (`apps/ui`).
- Build it once and serve it from two hosts:
  - (a) Tauri custom protocol, using a `TauriTransport` adapter.
  - (b) The daemon's embedded static server on loopback, using an `HttpTransport` adapter.
- Keep Next.js only for a possible future *hosted* site (docs, marketing, opt-in cloud fleet console), where SSR and server features matter.

**Sharing UI between the web dashboard and the Tauri frontend:**
- Do share, ideally the same app rather than two apps sharing packages.
- Isolate platform differences behind a frontend port:
  - `shared/api/transport.ts`: `interface LumenTransport { call<Op>(op, input): Promise<Output>; subscribe(topic, cb): Unsubscribe }`
  - Two adapters chosen at bootstrap, e.g. via `window.__TAURI_INTERNALS__` detection or a build-time `VITE_TARGET`.
- Desktop-only capabilities (reveal in Finder/Explorer, OS notifications, Full Disk Access deep links) live in `features/*` behind a `platform` capability check.
- Spacedrive follows the same pattern: a shared "SpaceUI" component system for desktop (Tauri), web and mobile, all talking to a central Rust daemon over JSON-RPC ([Spacedrive repo](https://github.com/spacedriveapp/spacedrive)).

### 4. Feature-Sliced Design (v2.1)

- **Layers, top to bottom:** `app`, (`processes`, deprecated), `pages`, `widgets`, `features`, `entities`, `shared`.
- **Import rule:** a module may import only from layers strictly below it, and slices on the same layer cannot import each other.
- `app` and `shared` contain segments directly. `pages`, `widgets`, `features` and `entities` contain **slices** (business domains), which contain **segments**: `ui`, `api`, `model`, `lib`, `config` ([overview](https://feature-sliced.design/docs/get-started/overview)). The docs present v2.1 as "pages-first", meaning code stays in the page slice until it is genuinely reused.
- **Public API rule** ([reference](https://feature-sliced.design/docs/reference/public-api)):
  - Each slice exposes an `index.ts` contract and avoids wildcard re-exports.
  - Cross-imports between same-layer slices use `entities/A/@x/B.ts`, and only on the entities layer.
  - For `shared/ui` and `shared/lib`, use per-component index files to avoid barrel files hurting tree-shaking and dev-server speed.
  - Index files don't *prevent* deep imports; that is the linter's job.
- **Steiger** ([repo](https://github.com/feature-sliced/steiger)):
  - The FSD architecture linter. Install `steiger` with `@feature-sliced/steiger-plugin` and configure it in `steiger.config.ts` with an ESLint-like flat syntax (`...fsd.configs.recommended`).
  - Rules include `fsd/forbidden-imports`, `fsd/public-api`, `fsd/insignificant-slice`, `fsd/no-segmentless-slices`, `fsd/inconsistent-naming` and about 20 more.
  - It is self-described as **beta**, at 0.7.0, and 0.5.0 already broke config, so pin it.
- **Next.js mapping** ([FSD + Next.js](https://feature-sliced.design/docs/guides/tech/with-nextjs)):
  - Next's root `app/` (and `pages/`) collide with FSD layer names. The documented fix is to rename the FSD layers to `src/_app` and `src/_pages`, keep Next's routing `app/` at the project root, and have route files only re-export: `export { ExamplePage as default, metadata } from '@/_pages/example'`.
  - Add `index.server.ts` public APIs for server-only code.
  - `middleware`/`proxy` and `instrumentation` stay at the root.
- **With TanStack Router (recommended):**
  - There is no naming conflict: `src/app/routes/**` (generated route tree) belongs to the `app` layer, and each route file imports a page from `@/pages/<slice>`.
  - Route `loader`s call `features`/`entities` query options (`queryOptions` from `entities/*/api`).

### 5. Linting and formatting

- **Biome 2.5** (2026-06-05; [blog](https://biomejs.dev/blog/biome-v2-5/)):
  - 500+ rules, 73 promoted to stable.
  - GritQL plugins can now emit fixes (applied as unsafe by default) and be scoped by glob.
  - Cross-file rules driven by the module graph (`noUnusedClasses`, `noUndeclaredClasses`), `--watch`, a `concise` reporter, and `linter.rules.preset`.
  - Type-aware rules such as `noFloatingPromises`, `noMisusedPromises` and `noUnnecessaryConditions` exist; the v2.5 blog only refers generically to "type-aware lint rules", so their stability group and the claimed v2.5 stack-overflow fixes are unverified. `linter.rules.preset` replaces the deprecated `recommended` flag (confirmed).
  - Biome infers types itself rather than running `tsc`, so coverage is narrower than typescript-eslint. I did not verify a coverage percentage.
- **ESLint 10** (2026-02-06; [blog](https://eslint.org/blog/2026/02/eslint-v10.0.0-released/)):
  - eslintrc is gone entirely, along with `ESLINT_USE_FLAT_CONFIG`, `--env`, `--rulesdir` and `.eslintignore`.
  - `eslint.config.*` is now looked up **from each linted file's directory**, which is useful for monorepos.
  - typescript-eslint 8.71 does not support TS 7 (see §2).
- **oxlint** ([type-aware stable](https://oxc.rs/blog/2026-07-22-type-aware-linting-stable), 2026-07-22):
  - 1.x stable.
  - `oxlint --type-aware [--type-check]` with `oxlint-tsgolint@7`, which tracks TS 7.0.2 and covers 59 of typescript-eslint's 61 type-aware rules.
  - Benchmarked 12–18× faster than ESLint + typescript-eslint.
  - Oxfmt is beta and reported to match Prettier's conformance suite ([InfoQ](https://www.infoq.com/news/2026/09/tsgolint-oxlint-typescript/)).
- **Import boundaries:**
  - **eslint-plugin-boundaries** 7.2 (flat config; `boundaries/elements` with element types and dependency rules) ([repo](https://github.com/javierbrea/eslint-plugin-boundaries)), but it requires ESLint.
  - **dependency-cruiser** 18.5 is runner-independent and suits cross-package rules (e.g. "`packages/ui` must not import `apps/*`"; "nothing imports `@tauri-apps/*` outside `shared/api/adapters/tauri`").
  - **Steiger** handles FSD layer and slice rules.

### 6. Monorepo tooling for Rust + TS (+ Swift/Kotlin)

**pnpm 12** ([12.0](https://pnpm.io/blog/releases/12.0)) is a stable **Rust rewrite** that keeps pnpm 11's commands, settings and lockfile format.
- Unknown `pnpm-workspace.yaml` settings now error or warn.
- Git dependencies resolve over HTTPS.
- Lockfiles with cyclic peer dependencies are deterministic.

**Catalogs** ([docs](https://pnpm.io/catalogs)):
- Declare `catalog:` / `catalogs:` in `pnpm-workspace.yaml` and reference them with `"react": "catalog:"`.
- `catalogMode: strict | prefer | manual`.
- `catalogPrune` replaces `cleanupUnusedCatalogs`.
- `catalog:` is rewritten to the concrete range on publish.
- 12.2–12.3 let catalogs use the `workspace:` protocol.

**Supply-chain defaults since pnpm 11** ([supply-chain](https://pnpm.io/supply-chain-security), [11.0](https://pnpm.io/blog/releases/11.0)):
- `minimumReleaseAge: 1440` (one day).
- `blockExoticSubdeps: true`.
- Lifecycle scripts blocked unless listed in `allowBuilds` (blocking since v10; `allowBuilds` map replaces the legacy `onlyBuiltDependencies` options in v11), and `strictDepBuilds: true` by default in v11, so an unapproved build script fails the install rather than being silently skipped.
- pnpm 11+ requires Node.js 22+ (consistent with Vitest 5's Node ≥ 22.12 floor).
- Recommended (opt-in, not a default): `trustPolicy: no-downgrade`.

| Tool | Version | Rust story | Swift/Kotlin | Verdict for Lumen |
| --- | --- | --- | --- | --- |
| **Turborepo** | 2.11.7 | **Experimental** native Cargo workspaces (`futureFlags.experimentalCargoWorkspaces`). Maps build/test/check/lint/format to `cargo … --package --locked`; hashes `Cargo.lock` + `rustc -vV`; caches only final `bin`/`cdylib`/`staticlib` by default ([Rust guide](https://turborepo.dev/docs/guides/tools/rust)) | `package.json` shim per project ([multi-language](https://turborepo.dev/docs/guides/multi-language)) | **Use for the JS/TS graph**; keep Cargo native; optionally add the Cargo flag later |
| Nx | 23.2.1 | Community `@monodon/rust` (Nx 21–23) wraps cargo build/test/clippy ([Nx KB](https://nx.dev/docs/kb/add-rust-to-nx-workspace)) | plugins / shims | Reject: heavier config and plugin model, Rust via third party |
| moon | 2.6.0 | v2 "Phobos" moved to **WASM plugin toolchains**; Rust toolchain support; scaffolds Cargo files for Docker ([InfoQ](https://www.infoq.com/news/2026/05/moonrepo-2-release/)) | via tasks | Strong alternative; re-evaluate if cross-language cached task graphs become necessary |
| just / cargo xtask | — | Native | Native (calls xcodebuild/gradle) | **Use as the human/CI entry point** (Spacedrive pattern) |

**Real-world reference.** Spacedrive v2 has these top-level directories: `apps/`, `core/`, `crates/`, `packages/`, `adapters/`, `extensions/`, `xtask/`, `scripts/`, `docs/`.
- Toolchain: a Rust workspace, a Bun JS workspace and the `just` runner.
- Specta generates **TypeScript and Swift** clients from Rust operation types, and every client talks to one Rust daemon over JSON-RPC.
- A 2.0 beta is planned for 2026-11-01 ([repo](https://github.com/spacedriveapp/spacedrive)).
- Licence: current code is Apache-2.0, but releases before 2026-03-24 remain AGPL-3.0 — copy patterns, and only copy code from post-relicence commits after checking provenance.

This is the closest public analogue to Lumen.

**Proposed Lumen layout:**

```
lumen/
├── Cargo.toml                 # [workspace] members = ["crates/*", "apps/desktop/src-tauri", "apps/daemon"]
├── pnpm-workspace.yaml        # packages: apps/ui, apps/desktop, packages/*; catalog: {...}; minimumReleaseAge, allowBuilds
├── turbo.json                 # JS tasks: build, typecheck, lint, test; inputs include generated/
├── justfile                   # polyglot entry: just gen | just check | just e2e | just mobile-ios
├── crates/
│   ├── lumen-core/            # domain (hexagonal core), policy engine, evidence graph
│   ├── lumen-api/             # transport-neutral DTOs + utoipa ToSchema; emits openapi.json
│   ├── lumen-ports/ …         # ports; platform adapters in crates/adapter-{macos,windows,…}
├── apps/
│   ├── daemon/                # axum on UDS/named pipe (+ optional loopback HTTP), serves apps/ui dist
│   ├── desktop/               # Tauri 2 shell (src-tauri) + loads apps/ui build via custom protocol
│   ├── ui/                    # Vite + React + TanStack Router SPA, FSD inside src/
│   ├── ios/  android/         # Swift / Kotlin native shells (own build systems)
├── packages/
│   ├── api-client/            # GENERATED from openapi.json (types + zod + fetch SDK), committed
│   ├── ui-kit/                # design system (Tailwind v4 theme.css + components)
│   └── config/                # shared tsconfig, biome.json, steiger/depcruise configs
└── schemas/openapi.json       # committed contract; CI fails on drift
```

### 7. Sharing types between Rust and TypeScript (and Swift/Kotlin)

| Approach | Version | What you get | Fit |
| --- | --- | --- | --- |
| **utoipa → OpenAPI → generator** | utoipa 6.0.0 (adds OpenAPI 3.2; MSRV 1.88) | Spec for HTTP routes + schemas; generators: **@hey-api/openapi-ts** 0.99 (SDK, types, Zod/Valibot, TanStack Query plugins, fetch/axios clients; 0.x, *pin exact*) ([hey-api](https://heyapi.dev/openapi-ts/get-started)), **openapi-typescript 7.13 + openapi-fetch 0.17** (types-only + tiny typed fetch), **orval** 8.40 | **Primary for the daemon API**: language-neutral contract also usable by Swift/Kotlin generators |
| **schemars → JSON Schema** | 1.2.2 | JSON Schema for config, policy rules, evidence records; Zod via `z.fromJSONSchema()` (experimental) or `json-schema-to-zod` 2.8.1 | For **on-disk formats** (policy files, quarantine manifests), not as the client generator |
| **specta + tauri-specta** | 2.0.0-rc.25 (both) | `collect_commands!` → `bindings.ts` with typed `invoke` wrappers returning `Result`, plus typed events ([repo](https://github.com/specta-rs/tauri-specta)) | Use only for the **thin Tauri shell command set**; still RC after a long time, so pin `=2.0.0-rc.25` |
| **ts-rs** | 12.0.1 | `#[derive(TS)]` + `#[ts(export)]`, serde-aware, types only, no validation ([repo](https://github.com/Aleph-Alpha/ts-rs)) | Fallback for TS-only internal types |
| **typeshare** (1Password) | 1.0.5 | `#[typeshare]` → TS, **Swift, Kotlin**, Scala stable; Go/Python experimental ([repo](https://github.com/1Password/typeshare)) | Candidate for **mobile FFI DTOs** when UniFFI is not used; no transport |

**Recommendation:**
- Treat `crates/lumen-api` DTOs as the contract. Derive `serde` and `utoipa::ToSchema` on them, emit `schemas/openapi.json` from a `cargo run -p lumen-api --bin dump-openapi` step, and generate `packages/api-client` with one pinned generator.
- Commit the generated code and make CI fail on `git diff --exit-code`. That avoids a hard Turborepo↔Cargo task edge and makes contract changes reviewable.
- Run Zod parsing of daemon responses at the client boundary in dev and test builds, and in production for safety-critical payloads (decision plans, quarantine manifests). The daemon remains the only authority for safety decisions.

### 8. HTTP client: fetch vs axios

- Lumen's UI runs only in modern webviews and browsers, so native `fetch` covers everything. Streaming progress uses SSE through `EventSource`/`fetch` streams or WebSockets. `AbortController` handles cancellation, and TanStack Query passes `signal` automatically.
- **Axios supply-chain incident** ([post-mortem](https://github.com/axios/axios/issues/10636), [Microsoft](https://www.microsoft.com/en-us/security/blog/2026/04/01/mitigating-the-axios-npm-supply-chain-compromise/)):
  - On 2026-03-31, a social-engineered maintainer machine was used to publish **axios 1.14.1 and 0.30.4**. Both added `plain-crypto-js@4.2.1`, which installed a RAT on macOS, Windows and Linux.
  - They were live for about 3 hours (00:21–03:15 UTC).
  - Microsoft attributed it to Sapphire Sleet (DPRK).
  - Axios then moved to OIDC trusted publishing and immutable releases.
- **Earlier CVEs:**
  - CVE-2025-27152: SSRF and credential leakage when an absolute URL overrides `baseURL`, fixed in 1.8.2.
  - CVE-2025-58754: unbounded `data:` URI decoding causing DoS in Node, fixed in 1.12.0.
- **Decision:** reject axios. Use `openapi-fetch` or the hey-api fetch client. Two settings would have neutralized the incident on their own: pnpm's default `minimumReleaseAge` (1 day) and `allowBuilds` (postinstall blocked).

### 9. Local web dashboard to local daemon: security

**Threats:**
- **Any website can make requests to `http://127.0.0.1:<port>`.** Simple requests (GET, form POST) are sent even when CORS blocks the response.
- **DNS rebinding:** an attacker domain re-resolves to 127.0.0.1, so its requests become *same-origin* to the browser. The defense is server-side `Host`/`Origin` validation.
- **"0.0.0.0 Day":** browsers routed `0.0.0.0` to localhost on macOS/Linux. Chrome rolled out blocking gradually from Chromium 128, completing by 133, and WebKit blocks all-zeros destinations (Safari 18 era); Firefox had **no fix** at disclosure (Aug 2024) and its current status is unverified — another reason the daemon must never bind `0.0.0.0` and must validate `Host` itself ([Oligo](https://www.oligo.security/blog/0-0-0-0-day-exploiting-localhost-apis-from-the-browser)).
- **WebSockets are not covered by CORS,** so Cross-Site WebSocket Hijacking needs an `Origin` check on upgrade.

The MCP specification gives a current, normative version of the same guidance for local HTTP servers: "Servers **MUST** validate the `Origin` header … to prevent DNS rebinding", "**SHOULD** bind only to localhost (127.0.0.1) rather than … (0.0.0.0)", and "**SHOULD** implement proper authentication", and (newer revisions) "If the `Origin` header is present and invalid, servers **MUST** respond with HTTP 403 Forbidden". The cited 2025-06-18 revision is superseded; the current revision is **2026-07-28**, which keeps the same three rules ([MCP 2026-07-28 Streamable HTTP](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http); older: [2025-06-18](https://modelcontextprotocol.io/specification/2025-06-18/basic/transports)). Note the MCP rule only applies when `Origin` is *present*; Lumen should go further and reject state-changing requests and WS/SSE upgrades with a missing or `null` Origin unless they carry the bearer token from a non-browser client (CLI).

**Chrome Local Network Access (LNA)** ([Chrome blog](https://developer.chrome.com/blog/local-network-access), [spec](https://wicg.github.io/local-network-access/)):
- Replaces the Private Network Access preflight design.
- Public-to-local and public-to-loopback requests require a **user permission prompt**, shipped in Chrome 142 after testing from Chrome 138 (confirmed, Chrome blog). The single `local-network-access` permission was later split into `local-network` and `loopback-network` (public-to-loopback uses `loopback-network`); the exact alias semantics are reported inconsistently by secondary sources (unverified against a primary source).
- **WebSockets were *not* gated in the initial LNA launch** (crbug 421156866); WebSocket gating was reportedly extended in Chrome 147 (April 2026, secondary sources), and Chrome 154 added a `targetAddressSpace` option to the `WebSocket` constructor ([Chrome 154 release notes](https://chromestatus.com/release-notes/154)). Older Chrome versions and other browsers therefore give **no** browser-side protection for WS/SSE, so the daemon's own `Origin` check on upgrade is mandatory, not optional.
- The `fetch(url, { targetAddressSpace: "local" })` hint exists.
- **Loopback-to-loopback is not restricted**, so a dashboard served by the daemon on 127.0.0.1 calling its own API is unaffected.
- Chromium does not yet enforce local-to-loopback (Chrome blog: planned for "the future").
- Enterprise policies (`LocalNetworkAccessRestrictionsTemporaryOptOut`, `LoopbackNetworkAllowedForUrls`, …) can disable or pre-grant LNA, so managed machines may have it switched off entirely.
- LNA reduces drive-by attacks, but users can click "Allow" and Firefox/Safari behave differently, so it is defense in depth, not a control Lumen relies on.

**Required controls for the daemon's HTTP listener (if enabled):**
1. Bind `127.0.0.1` and `[::1]` only; never `0.0.0.0`. Prefer an ephemeral port written to a user-only (0600) runtime file.
2. **Host allowlist:** reject unless `Host` ∈ {`127.0.0.1:<port>`, `[::1]:<port>`, `localhost:<port>`}. This kills DNS rebinding.
3. **Origin allowlist** on every state-changing request *and* every WebSocket/SSE upgrade:
   - the daemon's own origin
   - the Tauri origins: `tauri://localhost` on macOS/Linux (and iOS); `http://tauri.localhost` on Windows **and Android** by default since Tauri 2.0, or `https://tauri.localhost` when `app.windows[].useHttpsScheme` (added in 2.1.0) is `true` ([tauri 2.1.0 release notes](https://v2.tauri.app/release/tauri/v2.1.0/), [migration guide](https://v2.tauri.app/start/migrate/from-tauri-1/)). Pin `useHttpsScheme` explicitly in `tauri.conf.json` so the allowlist is deterministic; note that changing it later resets the webview's IndexedDB/localStorage on Windows/Android. In the recommended IPC design the webview never calls the daemon over HTTP, so these entries are only needed if a webview ever does.
   - Reject `Origin: null`.
4. **No CORS by default.** Same-origin serving means no `Access-Control-Allow-Origin` is ever needed.
5. **Auth:**
   - Use a per-install random token, at least 256 bits, sent as `Authorization: Bearer`. A non-simple header also forces a CORS preflight from foreign origins.
   - Bootstrap it Jupyter-style: `lumen dashboard` opens `http://127.0.0.1:<port>/#t=<token>`. The fragment is never sent to the server or logged. The SPA moves the token into memory or sessionStorage and strips the URL.
   - Avoid cookie auth. If you use cookies, they must be `HttpOnly; SameSite=Strict` plus a CSRF token.
6. **Destructive operations** (quarantine commit, purge, rollback) require a server-issued plan ID plus content hash that the UI echoes back. Consider making "purge quarantine" desktop-only.
7. **Strict CSP** on the served SPA: `default-src 'self'; connect-src 'self'`; no inline scripts. Add `X-Content-Type-Options`, `Cross-Origin-Opener-Policy`, `Cross-Origin-Resource-Policy: same-origin`, and `frame-ancestors 'none'` against clickjacking a "Quarantine" button.
8. Log rejected Host/Origin attempts as security events through local telemetry.

**Alternative (recommended default):**
- Desktop users never need a TCP port. The Tauri webview loads assets through the **custom protocol**, and Tauri warns that `tauri-plugin-localhost` "brings considerable security risks" ([plugin docs](https://v2.tauri.app/plugin/localhost/)).
- The webview calls Tauri commands, which are gated by Tauri 2 **capabilities/permissions/scopes** ([security](https://v2.tauri.app/security/)). The Tauri Rust core relays them to the daemon over a **Unix domain socket** (macOS) or **named pipe** (Windows), checking peer credentials (UID/SID) and code signature where feasible.
- The browser dashboard becomes an opt-in "advanced/remote-display" mode with every control above.

**IPC-socket hardening (added in verification; the UDS/pipe is now the primary trust boundary for a tool that deletes files):**
- **Windows named pipe:** create the first instance with tokio `ServerOptions::first_pipe_instance(true)` so a squatting process that created the pipe name first causes the daemon to fail closed (`PermissionDenied`) instead of the daemon unknowingly sharing a name; keep `reject_remote_clients(true)` (tokio's default — remote clients are disabled by default); and pass an explicit DACL restricted to the current user SID (and SYSTEM) via `create_with_security_attributes_raw`, because the default pipe DACL is broader than "current user only" ([tokio `ServerOptions`](https://docs.rs/tokio/latest/tokio/net/windows/named_pipe/struct.ServerOptions.html), tokio 1.53.2). The client (Tauri core/CLI) should verify the server's identity (e.g. `GetNamedPipeServerProcessId` → image path/signature) before sending a destructive plan; that last step is standard Win32 practice, not checked against Lumen-specific guidance (unverified).
- **Unix domain socket:** create it inside a per-user directory with mode `0700` (`$XDG_RUNTIME_DIR` on Linux, the app's per-user support/container directory on macOS), never in a world-writable `/tmp` path, and check the peer UID (`UnixStream::peer_cred()` / `getpeereid`) on every connection.

### 10. Testing

- **Vitest 5** (2026-09-03; [blog](https://vitest.dev/blog/vitest-5.html)):
  - Requires Vite ≥ 6.4 and Node ≥ 22.12.
  - Faster: the blog reports 8%–53% improvements depending on configuration (largest in vm pools, Browser Mode and large isolated suites).
  - Browser Mode **trace view**.
  - Nested projects with config inheritance.
  - `vi.when()` for per-argument mock behaviour.
  - Unawaited async assertions now **fail**.
  - `clearMocks` now defaults to `true` (confirmed in the blog). Per the migration notes, locators match text exactly and the WebdriverIO provider moved to community maintenance (not in the release blog; unverified).
- **React Testing Library 16.3.3** for component tests in jsdom/happy-dom. Prefer Vitest Browser Mode with the Playwright provider for components that depend on layout or virtualization (treemaps, evidence-graph canvas).
- **MSW 3** (blog 2026-09-30; [blog](https://mswjs.io/blog/introducing-msw-3.0)):
  - ESM-only.
  - Granular entry points `msw/http`, `msw/graphql`, `msw/sse`, `msw/ws`.
  - Experimental `defineNetwork` (replacing the `setupServer`/`setupWorker` pattern); socket-level interception in Node.
  - Node 22/24/26 supported.
  - Per [migration](https://mswjs.io/docs/migrations/2.x-to-3.x): `onUnhandledRequest` is renamed `onUnhandledFrame`, `msw/native` moved to `@msw/react-native`, and TS ≥ 5.9 is required.
  - Use MSW handlers typed from the generated OpenAPI types so mocks cannot drift from the contract.
- **Playwright 1.63:**
  - End-to-end tests of the daemon-served dashboard against a real daemon on a **fixture filesystem**: synthetic caches, launch agents, and decoy "user data" that must never be quarantined.
  - Assert the KEEP/REVIEW/QUARANTINE outcomes and that rollback restores byte-identical files.
  - Tauri-specific end-to-end tests: per the [Tauri WebDriver docs](https://v2.tauri.app/develop/tests/webdriver/), `tauri-driver` used directly supports **only Windows and Linux** ("macOS has no WKWebView driver tool available"); the recommended WebdriverIO service runs an embedded WebDriver server inside the app and is "how macOS is supported". Since macOS is a primary Lumen target, plan desktop E2E on the WebdriverIO-service path (ensure the embedded driver is compiled only into test builds, never release builds), and keep most safety E2E on the daemon-served dashboard via Playwright. Not re-checked against 2.12 specifically.

## Implications for Lumen

1. **UI framework.** Adopt **Vite 8 + React 19.3 + TanStack Router 1.x + TanStack Query 5 + Tailwind 4.3** as one SPA (`apps/ui`).
   - *Rationale:* it matches Tauri's static-only model, runtime-ID routing is native, it shares a toolchain with Vitest, and there is no server to patch.
   - *Rejected:* Next.js 16 static export. Its distinctive features are disabled, dynamic routes need workarounds, and it brings churn plus a server-side CVE history that matters if anyone ever runs `next start` locally.
   - *Rejected:* Next.js as a server bundled into the daemon. That would mean running a Node server on end-user machines (React2Shell, the Windows RCE).
   - *Kept open:* Next.js for a separate hosted site.
2. **One app, two hosts, a transport port.**
   - Add `LumenTransport` with `TauriTransport` and `HttpTransport` adapters, mirroring the Rust hexagonal design.
   - All daemon calls go through generated, typed operations. UI code never hand-builds URLs.
   - *Rejected:* two separate frontends, which doubles the UI surface that has to be correct about safety-critical state.
3. **The contract lives in Rust.**
   - utoipa 6 builds the OpenAPI spec, which generates `packages/api-client`: types, Zod schemas and a fetch SDK, with the generator version pinned exactly. CI fails on drift.
   - schemars 1.x documents on-disk formats (policy, quarantine manifest) as JSON Schema.
   - tauri-specta rc covers only the shell commands.
   - *Rejected:* ts-rs as the primary tool (TS-only, no runtime validation) and rspc (stale 0.4 / 1.0-rc).
4. **Validation and safety boundary.**
   - Parse responses with Zod 4 at the transport edge.
   - Represent byte counts and inode/file IDs (u64) as **strings or `bigint`-safe encodings** in the API, because JSON numbers lose precision above 2^53 and Zod `bigint` is not representable in JSON Schema.
   - The UI renders decisions; it never makes them. Mutations carry server-issued plan IDs and hashes.
5. **No axios.** Use native fetch through `openapi-fetch` or the hey-api fetch client.
6. **TypeScript.**
   - Use TS 7.0.x for `tsc --noEmit`, which is fast in CI.
   - Alias `typescript` → `@typescript/typescript6` *only* for tools that need the API, until TS 7.1 ships and typescript-eslint, dependency-cruiser and codegen catch up.
   - Track TS 7.1 as a planned upgrade.
7. **Lint and format.**
   - **Biome 2.5** for formatting and the base lint, with `biome.json` in `packages/config` and nested configs per package.
   - **Steiger** (pinned) for FSD layers.
   - **dependency-cruiser** for cross-package and platform-isolation rules.
   - **oxlint `--type-aware`** in CI with only the promise-safety rules (`no-floating-promises`, `no-misused-promises`), because an unawaited quarantine/rollback promise is a correctness bug.
   - *Rejected:* ESLint 10 + typescript-eslint + Prettier as the primary stack. It is slower, needs three tools, and is blocked on TS 7 today. Reconsider only if React Compiler lint rules (in `eslint-plugin-react-hooks`) become essential.
8. **FSD v2.1 in `apps/ui/src`.**
   - `entities`: device, volume, artifact, app, process, launch-agent, evidence, decision, quarantine-item, scan.
   - `features`: start-scan, review-decision, approve-quarantine-plan, restore-item, explain-with-jev (read-only evidence).
   - `widgets`: evidence-graph, storage-treemap, decision-queue, quarantine-ledger.
   - `pages`: overview, scan, review, quarantine, history, settings.
   - `shared`: api (transport, generated client), ui (re-exports from `packages/ui-kit`), lib, config.
   - Routes in `src/app/routes` only re-export pages.
9. **Monorepo.**
   - pnpm 12 workspaces with **catalogs** (`catalogMode: strict`), keeping `minimumReleaseAge`, `blockExoticSubdeps` and `allowBuilds`, plus `trustPolicy: no-downgrade`.
   - Root Cargo workspace.
   - **Turborepo 2.11** for JS tasks.
   - `justfile` as the polyglot entry point (gen → check → test → e2e). Xcode and Gradle keep their native builds.
   - *Rejected:* Nx (complexity, third-party Rust plugin) and Bazel (cost far exceeds team size; not researched in depth).
   - *Watch:* moon v2, and Turborepo's `experimentalCargoWorkspaces` once it is stable.
10. **Local security posture.**
    - Desktop runs over Tauri custom protocol + IPC + UDS or named pipe, with no TCP. Harden the socket itself: UDS in a 0700 per-user dir with peer-UID check; named pipe with `first_pipe_instance(true)`, a current-user-only DACL, and remote clients rejected (see §9).
    - If the webview ever talks to the daemon over HTTP, the Origin allowlist is `tauri://localhost` (macOS/Linux/iOS) and `http://tauri.localhost` (Windows/Android; `https://` if `useHttpsScheme` is set — pin it explicitly).
    - Forbid `react-server-dom-*` in the dependency graph so the SPA can never grow an RSC endpoint.
    - The browser dashboard is **off by default**. When enabled: loopback bind, Host/Origin allowlists (including WS), no CORS, a bearer token bootstrapped through the URL fragment, a strict CSP, plan-hash confirmations, and security-event logging.
    - Never use `tauri-plugin-localhost`.
11. **Testing pyramid.**
    - Vitest 5 unit and component tests (Browser Mode for visual widgets).
    - MSW 3 handlers typed from OpenAPI.
    - Contract test: the daemon's live OpenAPI equals the committed spec.
    - Playwright end-to-end tests against a real daemon on a fixture filesystem, with "never touch user data" assertions as release-blocking tests.
    - Desktop (Tauri) E2E via the WebdriverIO service's embedded WebDriver server, because direct `tauri-driver` does not support macOS; the embedded driver must be excluded from release builds.

## Risks and open questions

- **tauri-specta and specta are still at `2.0.0-rc.25`.** That is a long-lived RC with possible breaking changes. Mitigate by keeping the Tauri command surface tiny, or by routing desktop calls through the same generated OpenAPI client over an IPC transport.
- **TS 7.0 lacks an API.** typescript-eslint, dependency-cruiser's TS resolution and some codegen tools may need `@typescript/typescript6` until 7.1. Running two TS versions risks confusing editors (IDE language-server choice).
- **Biome's type-aware rules** use its own inference. False negatives on promise misuse are possible, which is why the plan adds oxlint type-aware in CI. Is running two linters acceptable to the team?
- **React Compiler adoption:** its diagnostics ship as ESLint rules (`eslint-plugin-react-hooks` 7.x). If we enable the compiler, do we keep a minimal ESLint 10 config just for those rules (which needs a TS parser, so the TS6 alias), or rely on the compiler silently bailing out?
- **Steiger is beta (0.7.0)** and has had config-breaking releases. Pin it and budget for upgrades.
- **Tauri webview origins** — resolved: `tauri://localhost` on macOS/Linux/iOS; `http://tauri.localhost` (or `https://` with `useHttpsScheme`) on Windows/Android (Tauri 2.1+ docs; not re-checked specifically against 2.12).
- **u64 precision across JSON:** decide the API convention (decimal strings vs `{hi, lo}`) before generating clients. It also affects Swift/Kotlin.
- **OpenAPI 3.2 vs generator support:** utoipa 6 can emit 3.2, but generator support for 3.2 is unverified. Pin emission to 3.1 unless the chosen generator confirms 3.2.
- **hey-api is 0.x** with breaking minors. Is openapi-typescript + openapi-fetch (smaller surface) preferable for long-term stability? It needs a separate Zod step.
- **Turborepo Cargo support is experimental.** Do not depend on it for release builds.
- **Chrome LNA behaviour may evolve,** for example enforcing local-to-loopback. Our design must not depend on browser-side protections either way.
- **Tailwind Labs joining Shopify** (2026-09-09): a governance and roadmap watch item only.
- **Whether a hosted, multi-device "fleet" dashboard is ever in scope:** if yes, that would be a separate app where Next.js 16 (Active LTS) is a reasonable candidate, with its own threat model.
- **I did not verify React 19.3 feature details** or a TanStack Query v6 timeline.

## Sources

- [npm registry metadata](https://registry.npmjs.org) (dist-tags and time; queried 2026-10-05)
- [crates.io API](https://crates.io/api/v1/crates) (queried 2026-10-05)
- [Next.js Blog index](https://nextjs.org/blog)
- [Next.js 16](https://nextjs.org/blog/next-16)
- [Next.js 16.3](https://nextjs.org/blog/next-16-3)
- [Next.js: How to create a static export](https://nextjs.org/docs/app/guides/static-exports)
- [Next.js August 2026 Security Release](https://nextjs.org/blog/august-2026-security-release)
- [Tauri v2: Next.js frontend guide](https://v2.tauri.app/start/frontend/nextjs/)
- [Tauri v2: Security](https://v2.tauri.app/security/)
- [Tauri v2: Localhost plugin](https://v2.tauri.app/plugin/localhost/)
- [React Versions](https://react.dev/versions)
- [React 19.2](https://react.dev/blog/2025/10/01/react-19-2) (via search result)
- [React Compiler v1.0](https://react.dev/blog/2025/10/07/react-compiler-1) (via search result)
- [Vercel: Summary of CVE-2025-55182](https://vercel.com/changelog/cve-2025-55182) (via search result)
- [Wiz: React2Shell CVE-2025-55182](https://www.wiz.io/blog/critical-vulnerability-in-react-cve-2025-55182) (via search result)
- [Announcing TypeScript 7.0](https://devblogs.microsoft.com/typescript/announcing-typescript-7-0/)
- [typescript-eslint issue #12518: TypeScript 7.0.2 Support](https://github.com/typescript-eslint/typescript-eslint/issues/12518) (via search result)
- [Tailwind CSS Blog](https://tailwindcss.com/blog)
- [TanStack Query overview](https://tanstack.com/query/latest/docs/framework/react/overview)
- [TanStack Router overview](https://tanstack.com/router/latest/docs/framework/react/overview)
- [Zod: JSON Schema](https://zod.dev/json-schema)
- [Standard Schema](https://standardschema.dev/)
- [Vite 8.0 is out!](https://vite.dev/blog/announcing-vite8) (via search result)
- [FSD: Overview](https://feature-sliced.design/docs/get-started/overview)
- [FSD: Public API](https://feature-sliced.design/docs/reference/public-api)
- [FSD: Usage with Next.js](https://feature-sliced.design/docs/guides/tech/with-nextjs)
- [Steiger (GitHub)](https://github.com/feature-sliced/steiger)
- [Biome v2.5 release blog](https://biomejs.dev/blog/biome-v2-5/)
- [Oxc: Type-Aware Linting Stable](https://oxc.rs/blog/2026-07-22-type-aware-linting-stable)
- [InfoQ: tsgolint reaches stable v7](https://www.infoq.com/news/2026/09/tsgolint-oxlint-typescript/) (via search result)
- [ESLint v10.0.0 released](https://eslint.org/blog/2026/02/eslint-v10.0.0-released/) (via search result)
- [eslint-plugin-boundaries (GitHub)](https://github.com/javierbrea/eslint-plugin-boundaries)
- [pnpm 12.0 release](https://pnpm.io/blog/releases/12.0)
- [pnpm Catalogs](https://pnpm.io/catalogs)
- [pnpm: Mitigating supply chain attacks](https://pnpm.io/supply-chain-security)
- [Turborepo: Rust (Experimental)](https://turborepo.dev/docs/guides/tools/rust)
- [Turborepo: Multi-language support](https://turborepo.dev/docs/guides/multi-language)
- [Nx: Add a Rust application](https://nx.dev/docs/kb/add-rust-to-nx-workspace) (via search result)
- [InfoQ: Moonrepo releases moon v2.0](https://www.infoq.com/news/2026/05/moonrepo-2-release/) (via search result)
- [Spacedrive (GitHub)](https://github.com/spacedriveapp/spacedrive)
- [tauri-specta (GitHub)](https://github.com/specta-rs/tauri-specta)
- [ts-rs (GitHub)](https://github.com/Aleph-Alpha/ts-rs)
- [typeshare (GitHub)](https://github.com/1Password/typeshare)
- [utoipa 6.0.0 release](https://github.com/juhaku/utoipa/releases/tag/utoipa-6.0.0) (via search result)
- [Hey API: openapi-ts get started](https://heyapi.dev/openapi-ts/get-started)
- [Axios post-mortem: npm supply chain compromise (#10636)](https://github.com/axios/axios/issues/10636)
- [Microsoft Security: Mitigating the Axios npm supply chain compromise](https://www.microsoft.com/en-us/security/blog/2026/04/01/mitigating-the-axios-npm-supply-chain-compromise/) (via search result)
- [CVE-2025-27152 axios SSRF (Miggo)](https://www.miggo.io/vulnerability-database/cve/CVE-2025-27152) (via search result)
- [CVE-2025-58754 axios DoS (SentinelOne)](https://www.sentinelone.com/vulnerability-database/cve-2025-58754/) (via search result)
- [Chrome: Local Network Access](https://developer.chrome.com/blog/local-network-access)
- [WICG: Local Network Access spec](https://wicg.github.io/local-network-access/)
- [Oligo: 0.0.0.0 Day](https://www.oligo.security/blog/0-0-0-0-day-exploiting-localhost-apis-from-the-browser) (via search result)
- [MCP spec: Transports (security warning), 2025-06-18 revision](https://modelcontextprotocol.io/specification/2025-06-18/basic/transports)
- [MCP spec 2026-07-28: Streamable HTTP (Security & Endpoint)](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http)
- [Chrome 154 release notes (LNA, WebSocket targetAddressSpace)](https://chromestatus.com/release-notes/154)
- [Tauri 2.1.0 release notes (useHttpsScheme)](https://v2.tauri.app/release/tauri/v2.1.0/)
- [Tauri: Upgrade from Tauri 1.0 (http://tauri.localhost on Windows)](https://v2.tauri.app/start/migrate/from-tauri-1/)
- [Tauri v2: WebDriver testing](https://v2.tauri.app/develop/tests/webdriver/)
- [React: Critical Security Vulnerability in React Server Components](https://react.dev/blog/2025/12/03/critical-security-vulnerability-in-react-server-components)
- [pnpm 11.0 release](https://pnpm.io/blog/releases/11.0)
- [tokio named_pipe::ServerOptions](https://docs.rs/tokio/latest/tokio/net/windows/named_pipe/struct.ServerOptions.html)
- [Vitest 5.0 is out!](https://vitest.dev/blog/vitest-5.html)
- [Introducing MSW 3.0](https://mswjs.io/blog/introducing-msw-3.0)
- [MSW 2.x → 3.x migration](https://mswjs.io/docs/migrations/2.x-to-3.x) (via search result)

## Verification log

Adversarial fact-check performed 2026-10-06 against primary sources (official docs/blogs, npm registry, crates.io API).

| # | Claim | Verdict | Source |
| --- | --- | --- | --- |
| 1 | Package versions in §1 (next 16.3.8 / 15.5.27 backport, react 19.3.0, typescript 7.0.2 + `@typescript/typescript6` 6.0.2, tailwind 4.3.3, react-query 5.104.1, react-router 1.170.41, zod 4.6.5, axios 1.20.0, vite 8.3.2, vitest 5.0.3, msw 3.0.2, playwright 1.63.0, biome 2.5.15, eslint 10.12.0, typescript-eslint 8.71.1, oxlint 1.87.0, steiger 0.7.0, pnpm 12.9.1, turbo 2.11.7, nx 23.2.1, moon 2.6.0, hey-api 0.99.0, openapi-typescript 7.13.0, openapi-fetch 0.17.0, @tauri-apps/api 2.12.1, RTL 16.3.3, dependency-cruiser 18.5.0) | Confirmed (dist-tags.latest); added publish dates for the OpenAPI generators | npm registry, queried 2026-10-06 |
| 2 | Crate versions: tauri 2.12.1 (3.0.0-alpha.4), utoipa 6.0.0, specta/tauri-specta 2.0.0-rc.25 | Confirmed; added last-publish dates (specta May 2026) and note that the tauri-specta README still says rc.21 | crates.io API |
| 3 | typescript-eslint supports TS `>=4.8.4 <6.1.0` only | Confirmed (peerDependencies of 8.71.1; issue #12518 closed as duplicate) | npm registry; GitHub issue #12518 |
| 4 | TS 7.0 GA 2026-07-08, no API until 7.1, `@typescript/typescript6` compat | Confirmed; corrected "7.1 targeted autumn 2026" to "no date, 3–4-month cadence"; added changed defaults (`types: []`, `rootDir`, `module`, `target`) | devblogs.microsoft.com TS 7.0 announcement |
| 5 | Next `output: 'export'` unsupported-feature list | Confirmed verbatim (docs v16.3.8) | nextjs.org static-exports guide |
| 6 | Aug 2026 Next security release: two Critical RCEs (AVIF/libheif, Windows-only) | Confirmed; added date 2026-08-25, patched versions 16.3.3/15.5.24, CVE-2026-75604, "no known workaround" | nextjs.org/blog/august-2026-security-release |
| 7 | CVE-2025-55182 React2Shell, CVSS 10, fixed 19.0.1/19.1.2/19.2.1, client-only apps unaffected | Confirmed; added follow-up RSC CVEs (55183, 55184, 67779, CVE-2026-23864) and a `react-server-dom-*` ban | react.dev advisory 2025-12-03 |
| 8 | Axios 1.14.1/0.30.4 malicious on 2026-03-31, `plain-crypto-js@4.2.1`, ~00:21–03:15 UTC, Sapphire Sleet | Confirmed | axios issue #10636; Microsoft Security blog 2026-04-01 |
| 9 | pnpm 11 defaults: `minimumReleaseAge: 1440`, `blockExoticSubdeps: true`; `allowBuilds`; `trustPolicy` | Confirmed; clarified `trustPolicy: no-downgrade` is opt-in, added `strictDepBuilds: true` default and Node 22+ requirement | pnpm.io 11.0 release; pnpm.io supply-chain-security |
| 10 | pnpm 12 is a Rust rewrite | Confirmed (release date not stated on the page) | pnpm.io/blog/releases/12.0 |
| 11 | Tauri webview origins (`tauri://localhost` macOS/Linux, `http://tauri.localhost` Windows) | Corrected/completed: Android also uses `http://tauri.localhost`; `useHttpsScheme` (2.1.0+) switches to `https://`; open question closed | Tauri 2.1.0 release notes; Tauri v1→v2 migration guide |
| 12 | `tauri-plugin-localhost` "brings considerable security risks" | Confirmed verbatim | v2.tauri.app/plugin/localhost |
| 13 | Chrome LNA shipped in 142; loopback-to-loopback exempt; local-to-loopback not enforced | Confirmed; added that WebSockets were not gated at launch (reportedly from Chrome 147), Chrome 154 WS `targetAddressSpace`, enterprise opt-out policies; permission-alias wording marked unverified | developer.chrome.com LNA blog; chromestatus release notes 154 |
| 14 | MCP spec Origin/localhost/auth guidance | Confirmed but cited revision outdated; updated to current 2026-07-28 revision and added 403-on-invalid-Origin rule | modelcontextprotocol.io spec 2026-07-28 |
| 15 | 0.0.0.0-Day browser fixes | Confirmed for Chrome 128→133 (gradual) and WebKit; added Firefox had no fix at disclosure (current status unverified) | Oligo Security blog |
| 16 | oxlint type-aware stable 2026-07-22, tsgolint v7, TS 7.0.2, 59/61 rules, 12–18x | Confirmed | oxc.rs blog |
| 17 | Turborepo Cargo support experimental via `futureFlags.experimentalCargoWorkspaces` | Confirmed | turborepo.dev Rust guide |
| 18 | utoipa 6.0.0 adds OpenAPI 3.2, MSRV 1.88 | Confirmed (2026-09-22); whether 3.1 emission is selectable is unverified | GitHub release utoipa-6.0.0 |
| 19 | Vitest 5 requirements and features | Confirmed (Node ≥22.12, Vite ≥6.4, unawaited assertions fail, trace view, `vi.when`, `clearMocks` default); corrected speed-up "8–25%" to "8%–53%"; WebdriverIO-provider claim marked unverified | vitest.dev Vitest 5 blog |
| 20 | MSW 3: ESM-only, `msw/*` entry points, experimental `defineNetwork`, Node 22/24/26 | Confirmed; TS ≥ 5.9 not in blog (from migration guide, not re-checked) | mswjs.io MSW 3.0 blog |
| 21 | Zod `toJSONSchema` stable, `fromJSONSchema` experimental, bigint unrepresentable | Confirmed; default for unrepresentable types is to throw | zod.dev/json-schema |
| 22 | FSD Next.js workaround (`_app`/`_pages`, `index.server.ts`) | Confirmed | feature-sliced.design with-nextjs guide |
| 23 | Biome 2.5 (2026-06-05, 500+ rules, GritQL fixes, `--watch`, `linter.rules.preset`) | Confirmed; type-aware rule details and "stack overflow fixes" marked unverified | biomejs.dev Biome v2.5 blog |
| 24 | Spacedrive: Rust daemon + JSON-RPC, Specta TS+Swift clients, `just`, 2.0 beta 2026-11-01 | Confirmed; added licence note (Apache-2.0 now, AGPL-3.0 before 2026-03-24) | github.com/spacedriveapp/spacedrive |
| 25 | Tauri E2E via `tauri-driver` (previously unverified) | Corrected: direct `tauri-driver` supports Windows/Linux only; macOS needs the WebdriverIO service's embedded WebDriver server | v2.tauri.app/develop/tests/webdriver |
| 26 | (Omission) Named-pipe/UDS hardening for the daemon IPC | Added: `first_pipe_instance`, `reject_remote_clients` default, explicit DACL; UDS 0700 dir + peer UID | docs.rs tokio `ServerOptions` (tokio 1.53.2) |

Not re-checked (left as stated, and still treated as lower-confidence): Next 16.3 feature list, React 19.3 features, Tailwind 4.3 features and `@source` path syntax, Tailwind Labs/Shopify announcement, ESLint 10 config-lookup change, Steiger rule list, Nx/moon details, axios 2025 CVE fix versions, MSW migration details, TanStack Router specifics.
