# ADR-0023: Secure every IPC boundary with authenticated peers and plan-ID commands

- Status: Accepted
- Date: 2026-10-06

## Context and problem statement

Lumen has, or will have, these boundaries:

- webview ↔ Tauri host;
- browser dashboard ↔ local core over loopback HTTP (opt-in, ADR-0010);
- app ↔ agent;
- core ↔ privileged helper (later, ADR-0005).

Defaults are unsafe. A Windows named pipe created with NULL security attributes grants
read to Everyone and anonymous users. Localhost servers are reachable from web pages.
A privileged helper that accepts paths is a confused deputy.

## Decision drivers

- Every message source is authenticated.
- Mutations name server-issued plans, never raw paths.
- Version skew between components is detected.

## Considered options

1. Per-boundary hardened transports, a shared versioned message schema, and a
   plan-ID-only mutation protocol.
2. A single localhost HTTP API for everything.

## Decision outcome

Chosen option: **1**.

| Boundary | Transport | Authentication |
| --- | --- | --- |
| Webview → host | Tauri commands and channels | per-window capability listing only Lumen commands; isolation pattern; strict CSP; navigation and permission requests denied |
| Browser dashboard → core | loopback HTTP + WebSocket | `Host` and `Origin` allowlists, header bearer token, no CORS |
| App ↔ agent (macOS) | Unix domain socket in a `0700` per-user directory, or XPC | `getpeereid` plus code-signature check of the peer (Team ID + identifier) |
| App ↔ agent (Windows) | named pipe | `FILE_FLAG_FIRST_PIPE_INSTANCE`, `PIPE_REJECT_REMOTE_CLIENTS`, explicit DACL (current user or logon SID, no `FILE_CREATE_PIPE_INSTANCE` for clients), client image signature check |
| Core → helper (macOS) | XPC Mach service from `SMAppService.daemon` | `xpc_connection_set_peer_code_signing_requirement` (macOS 13–25) or `XPCPeerRequirement` (26+) on both ends |
| Core → helper (Windows) | one-shot pipe with a random per-launch name passed at elevation | DACL as above, plus a signed plan file |

Protocol rules:

- Messages are versioned schemas (ADR-0011) with a handshake that exchanges component
  versions and refuses skew.
- Reads return view models. Mutations are `propose_plan(selection_ids) → plan_id`,
  then `execute_plan(plan_id, confirmation_token)`, then `rollback(operation_id)`.
- Plans are hashed and signed by the core. Executors re-verify the plan, the deny-list
  and identities themselves.
- Peer-credential failures, `Origin` mismatches and handshake failures are security
  events, logged locally.

### Consequences

- Good: no unauthenticated mutation path exists.
- Bad: per-OS transport code; mitigated by keeping it in adapter crates with shared
  tests.

## More information

- [Security research, §A.5](../research/09-security-devops-quality.md)
- [Desktop research, Summary](../research/01-desktop-architecture.md)
- [Web and monorepo research, Implication 10](../research/07-web-and-monorepo.md)
