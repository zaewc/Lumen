# ADR-0005: Run user-scope only in v1; add an agent and a privileged helper later

- Status: Accepted
- Date: 2026-10-06

## Context and problem statement

Some useful cleanup (machine-wide caches, other users' files, fast NTFS MFT
enumeration, USN journal tracking) needs administrator or root privileges. Privileged
cleaners are classic local-privilege-escalation targets through symlink and junction
redirection. Background scanning needs a process that runs without the UI.

## Decision drivers

- Least privilege: no standing privileged component unless users need it.
- macOS TCC: Full Disk Access granted to "Lumen" must cover the process that scans.
- Windows Administrator protection runs elevated processes in a separate profile, so
  an elevated process must never infer "the user" from its own environment.
- A privileged component must not be a confused deputy.

## Considered options

1. v1: one unelevated app process (UI host + core). Later: an optional per-user agent
   and an optional, minimal, on-demand privileged helper that accepts only signed plan
   IDs.
2. A privileged daemon or service from v1.
3. Running the whole application elevated.

## Decision outcome

Chosen option: **1**, staged:

| Stage | Component | Privilege | Mechanism |
| --- | --- | --- | --- |
| v1 | `Lumen.app` / `lumen.exe` (Tauri host + core) | user (`asInvoker`) | — |
| v1.x | `lumen-agent`: headless, same core, scheduled read-only scans and quarantine expiry | user | macOS `SMAppService.agent` (executable inside the bundle so it shares the app's TCC identity); Windows Task Scheduler logon task |
| later, opt-in | `lumen-helper`: verbs `stat`, `quarantine_move`, `restore`, `verify` for system-scope paths only | root / admin | macOS `SMAppService.daemon` + XPC with peer code-signing requirements; Windows on-demand `ShellExecuteEx("runas")` per approved plan with a one-shot hardened pipe |

Rules for any privileged helper:

- It accepts **plan IDs** referencing a signed, hashed plan produced by the core.
  It never accepts raw paths from the UI.
- It re-runs the hard deny-list and identity verification itself
  (same `(device, inode)` / `(volume serial, FileId)`, not SIP- or SSV-protected, verdict still
  `QUARANTINE`).
- It receives the target user's identity from the client token or the unelevated
  core, never from its own `HKCU`, `%TEMP%` or home directory.
- It is written in Rust with a minimal dependency set and gets its own threat-model
  section.

Single-writer rule: when the agent runs it owns the state databases and the UI becomes
its client over local IPC (ADR-0023); otherwise the app owns them.

v1 shows system-scope findings **read-only** with an explanation of why Lumen does
not act on them.

### Consequences

- Good: v1 has no privilege asymmetry, so no confused-deputy surface.
- Good: Full Disk Access granted to the app applies to the scanning process.
- Bad: v1 cannot clean machine-wide locations or use MFT/USN fast paths.
- Bad: the agent introduces a client/server mode for the UI later.

Rejected:

- Option 2 creates a standing attack surface before users need it; `SMJobBless` is
  deprecated, and a LocalSystem service raises Microsoft Store and EDR concerns.
- Option 3 breaks per-user context (HKCU, Recycle Bin, profile separation under
  Administrator protection).

## More information

- [Desktop architecture research, Implications 4–5](../research/01-desktop-architecture.md)
- [macOS research, Implications 8–9](../research/03-macos-platform.md)
- [Windows research, §G](../research/04-windows-platform.md)
- [Security research, §A.5](../research/09-security-devops-quality.md)
