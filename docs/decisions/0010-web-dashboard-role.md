# ADR-0010: Make the web dashboard local-only and off by default; defer any cloud backend

- Status: Proposed
- Date: 2026-10-06

## Context and problem statement

The project calls for a "web dashboard / management UI". It could be:

- a local browser UI for the same device;
- a hosted, multi-device fleet console, which needs accounts, a server, sync and a
  cloud database;
- both.

A hosted console contradicts Lumen's local-first default unless carefully scoped, and
it adds a server, authentication, and a new threat model. A localhost HTTP API is
reachable from any web page the user visits (DNS rebinding, CSRF) unless it is
hardened.

This decision is **Proposed** because whether a multi-device cloud product exists is
a business decision.

## Decision drivers

- Local-first and private by default.
- No network listener unless the user enables one.
- Earned complexity: no server-side product before it is needed.

## Considered options

1. v1: local-only. The same SPA (ADR-0009) runs in Tauri; the browser dashboard is an
   opt-in mode served by the local core on loopback. No cloud backend.
2. Hosted multi-device console in v1.
3. No browser dashboard at all.

## Decision outcome

Recommended option: **1**.

- Desktop uses the Tauri custom protocol and IPC; no TCP port is opened.
- When the user enables the browser dashboard, the local server:
  - binds to `127.0.0.1` and `::1` only;
  - validates `Host` (DNS rebinding) and `Origin`, including on WebSocket upgrades;
  - sends no CORS headers;
  - requires a per-install bearer token in a header, bootstrapped through a URL
    fragment (never a cookie);
  - serves a strict CSP and logs security events;
  - requires plan-hash confirmation for every mutation.
- Chrome's Local Network Access prompts are treated as defense in depth only.
- A hosted fleet console, if the owner wants one, gets its own ADR, threat model and
  privacy review. It would use metadata-only sync and would be a separate app
  (Next.js is a reasonable candidate there).

### Consequences

- Good: no listening socket by default; no server-side data custody.
- Bad: no cross-device view in v1.
- Bad: the "web" target is initially the same SPA, not a separate product.

## More information

- [Web and monorepo research, Implications 10](../research/07-web-and-monorepo.md)
- [Security research](../research/09-security-devops-quality.md)
- Owner question: is a hosted multi-device console in scope, and when?
