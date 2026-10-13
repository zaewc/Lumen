# ADR-0029: Watch absolute paths with FSEvents and pin resume points to the volume UUID

- Status: Accepted
- Date: 2026-10-13

## Context and problem statement

[ADR-0018](0018-incremental-scanning-change-feeds.md) chose a per-device FSEvents
stream (`FSEventStreamCreateRelativeToDevice`) with a persisted `(volume UUID, last
event ID)` resume point. On macOS 26 the user's data lives on the Data volume and is
reached through firmlinks: `/Users` is `/System/Volumes/Data/Users`. A device-relative
stream takes and reports paths relative to the Data volume's root, so every watched
root and every reported path would have to be translated between the firmlink
namespace and the volume namespace (`ATTR_CMNEXT_NOFIRMLINKPATH` one way, the
firmlink table the other).

Measured on 2026-10-13 (macOS 26, APFS, C probe):

- `FSEventStreamCreate` on `~/probe` reports `/Users/<me>/probe/` and
  `/Users/<me>/probe/sub/`: firmlink paths, directory granularity, trailing slash.
- `FSEventsCopyUUIDForDevice(st_dev)` returns the Data volume's FSEvents UUID for
  that root.
- Restarting with `sinceWhen` = an earlier ID replays the history, then sends
  `kFSEventStreamEventFlagHistoryDone`. A repeat change to an already-reported
  directory was folded into the history-done event.
- A `sinceWhen` above `FSEventsGetCurrentEventId()` is not an error: the stream just
  sends history-done. A stored ID that moved backwards must be detected by Lumen.

## Decision drivers

- Paths in the namespace users and the scanner see (scan roots are firmlink paths).
- Persistent, verifiable resume points; never trust a replay blindly.
- The least FFI and translation code.

## Considered options

1. `FSEventStreamCreateRelativeToDevice` with path translation (ADR-0018 as written).
2. `FSEventStreamCreate` on absolute paths, with the resume point pinned to the root
   volume's FSEvents UUID.

## Decision outcome

Chosen option: **2**. The resume point stays `(volume UUID, last event ID)`, as
ADR-0018 requires, and is checked on every start:

- the UUID of each root's device must equal the stored UUID;
- the stored event ID must not exceed `FSEventsGetCurrentEventId()`.

Otherwise the feed reports a stale resume point and the caller runs a full rescan.
Coalescing flags (`MustScanSubDirs`, `UserDropped`, `KernelDropped`) invalidate the
subtree, and `RootChanged` or `EventIdsWrapped` invalidates the volume. Streams are
delivered on a dispatch queue (`FSEventStreamSetDispatchQueue`), because run-loop
scheduling is deprecated.

### Consequences

- Good: no path translation; events match scan paths directly.
- Good: one stream for all roots, with a single persisted ID for the volume group.
- Bad: event IDs are host-wide rather than per device. The UUID check catches a
  replaced or reformatted volume, and periodic full sweeps (ADR-0018) catch anything
  folded away during replay.

## More information

- [FSEvents Programming Guide](https://developer.apple.com/library/archive/documentation/Darwin/Conceptual/FSEvents_ProgGuide/UsingtheFSEventsFramework/UsingtheFSEventsFramework.html)
- Bindings: `objc2-core-services` 0.3.2 (`FSEvents`, `libc`, `dispatch2` features).
- Amends [ADR-0018](0018-incremental-scanning-change-feeds.md) (macOS adapter only).
