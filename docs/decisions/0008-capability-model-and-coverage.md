# ADR-0008: Model platform capabilities and scan coverage explicitly

- Status: Accepted
- Date: 2026-10-06

## Context and problem statement

What Lumen can observe and do differs radically by platform and by permission:

- macOS: Full Disk Access cannot be queried, only probed. Containers, Group Containers
  and (per a third-party analysis) some macOS 27 Application Support folders carry
  extra protection.
- Windows: reading the MFT and USN journal needs elevation; cloud placeholders must not
  be hydrated.
- Android: no app can clear another app's internal cache (since API 23). Access comes
  in tiers: Usage access, media permissions, All files access.
- iOS: the app sees only its own container, PhotoKit, and folders the user picks.

A scanner that reports an unreadable directory as empty, or a missing package as
absent, turns a blind spot into a false "orphan" and invites unsafe deletion.

## Decision drivers

- "Not observed" must never become "absent" or "zero".
- Policy, UI and marketing must never offer an action the platform cannot perform.
- Store review (App Review 2.3.1, Play permissions policy) punishes over-claiming.

## Considered options

1. A capability model plus per-scan coverage reports as first-class domain data.
2. Platform checks scattered through adapters and UI code.

## Decision outcome

Chosen option: **1**.

- `PlatformCapabilities` (domain type) declares what the running host can do, e.g.
  `can_enumerate_other_apps_files`, `can_quarantine_by_rename`,
  `can_trash_media`, `can_clear_other_app_caches = false`, `can_evict_cloud_items`.
  The application layer refuses to build plans that use an unsupported action.
- Every directory and inventory observation carries an access state:
  `Readable | Denied(reason: Tcc | Sip | Posix | Acl | Scope | Unknown) | NotScanned`.
  Denied and not-scanned nodes have **unknown** size, never 0, and never feed orphan
  or "unused" heuristics.
- Every scan produces a `CoverageReport` (roots covered, tiers granted, blind spots,
  suspected truncation such as an implausibly short Android package list). The policy
  engine treats evidence derived from incomplete coverage as weaker; orphan findings
  require positive evidence of absence across all observable sources and still yield
  at most `REVIEW`.
- Cloud placeholders and dataless files are a distinct state (`CloudOnly`) and are
  never opened, hashed or moved (ADR-0017).

Per-platform product scope (detailed in `docs/architecture/platforms.md`):

| Platform | Scope |
| --- | --- |
| macOS | Full user-scope analysis and quarantine; system scope read-only in v1 |
| Windows | Full user-scope analysis and quarantine; system scope read-only in v1 |
| Android | Tiered: T0 basics, T1 Usage access (per-app sizes, last use), T2 media (trash-based reversible cleanup), T3 All files access (shared storage); guided per-app cache clearing via system settings |
| iOS | "Photos and Files": PhotoKit analysis and batched delete into Recently Deleted, user-picked folders, iCloud Drive eviction, Lumen's own cache |

### Consequences

- Good: blind spots are visible and can never cause a deletion.
- Good: one domain vocabulary for very different platforms.
- Bad: every adapter must report access state precisely, which adds test burden.

## More information

- [macOS research, Implication 7](../research/03-macos-platform.md)
- [Android research, Implications 2 and 8](../research/05-android-platform.md)
- [iOS research, Implication 1](../research/06-ios-and-mobile-framework.md)
- [Windows research, §E](../research/04-windows-platform.md)
