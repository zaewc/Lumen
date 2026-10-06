# ADR-0018: Scan incrementally from persisted change feeds with periodic full sweeps

- Status: Accepted
- Date: 2026-10-06

## Context and problem statement

Full scans of large disks are expensive and should not repeat. Operating systems
provide change journals, but all of them can drop events, and Apple advises treating
FSEvents as advisory.

## Decision drivers

- Avoid repeated full scans.
- Correctness when events are lost, coalesced, or the volume changes.
- Unprivileged operation in v1 (ADR-0005).

## Considered options

1. A `ChangeFeed` port with native adapters persisting resume points, invalidating
   subtrees, and backed by periodic full sweeps.
2. The `notify` crate as the source of truth.
3. Periodic full rescans only.

## Decision outcome

Chosen option: **1**.

| Platform | Adapter | Persisted resume point | Fallback |
| --- | --- | --- | --- |
| macOS | per-device FSEvents stream (`FSEventStreamCreateRelativeToDevice`, `fsevent-sys` / `objc2-core-services`) | `(volume UUID, last event ID)` | full rescan on UUID mismatch or ID regression |
| Windows (privileged helper) | USN change journal | `(JournalID, NextUsn)` | full rescan on journal ID change or deleted range |
| Windows (unprivileged) | `ReadDirectoryChangesW` on hot roots; `ReadDirectoryChangesExW` only on NTFS | none | scheduled rescan of hot roots |
| Android | MediaStore generation numbers; scheduled WorkManager jobs | generation | rescan |
| Linux (development) | inotify on hot roots | none | rescan |

Rules:

- Start the feed **before** a full scan, so no change falls between the scan and the
  feed.
- Coalescing flags (`MustScanSubDirs`, dropped events, `ERROR_NOTIFY_ENUM_DIR`)
  invalidate the subtree for rescan.
- A scheduled full sweep (e.g. weekly, on idle, or on volume UUID change) reconciles
  drift.
- Before any plan is proposed or executed, candidates are re-verified directly
  (ADR-0016); the change feed is never trusted for safety.
- `notify` (8.x stable) is used only for live UI refresh of focused folders. It cannot
  replay FSEvents from a saved event ID.

### Consequences

- Good: rescans are proportional to change.
- Bad: per-platform adapters with persisted state that must be versioned.

## More information

- [macOS research, Implication 6](../research/03-macos-platform.md)
- [Windows research, §C](../research/04-windows-platform.md)
- [Scanning research, Implication 5](../research/10-filesystem-scanning-quarantine-duplicates.md)
