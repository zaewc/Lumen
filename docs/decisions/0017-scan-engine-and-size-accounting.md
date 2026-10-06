# ADR-0017: Scan with native batch enumerators on a work-stealing pool and account sizes precisely

- Status: Accepted
- Date: 2026-10-06

## Context and problem statement

Storage scanning is dominated by syscalls, not CPU: one profiled macOS scanner spent
about 91% of its time in syscalls. Reclaim estimates are easy to get wrong, because:

- deleting one APFS clone, one hard link, or a snapshot-held file frees nothing;
- pnpm and uv stores hard-link or clone files into `node_modules` and virtualenvs;
- cloud placeholders report sizes for bytes that are not local, and reading them
  triggers hydration.

The engine must support full, targeted, background and resumable scans with
cancellation, bounded resources and partial failure.

## Decision drivers

- Throughput through batched metadata syscalls.
- Correct "reclaimable now" figures; never over-promise.
- Never hydrate cloud files or open file contents during scanning.
- Cancellation, backpressure, and progress; one inaccessible directory never fails the
  scan.

## Considered options

1. `lumen-scan`: a `DirEnumerator` port with native batch adapters, run as synchronous
   directory tasks on a work-stealing blocking pool and orchestrated by Tokio.
2. Build on `jwalk` or `ignore` with `std::fs::Metadata`.
3. `tokio::fs` async traversal.

## Decision outcome

Chosen option: **1**.

Enumerators:

- macOS: `getattrlistbulk`, breadth-first with one directory open per task and a 32 KB
  buffer. It returns `FILEID`, `LINKCOUNT`, `ALLOCSIZE`, `TOTALSIZE`, `CLONEID`,
  `EXT_FLAGS` and flags. `PRIVATESIZE` is requested only for items that may share
  blocks and for candidates before a plan is shown.
- Windows: directory handles with
  `GetFileInformationByHandleEx(FileIdExtdDirectoryInfo)` (fallback
  `FileIdBothDirectoryInfo`), placeholder compatibility mode set explicitly, and
  reparse points emitted as edges, never traversed. MFT/USN fast paths need the
  privileged helper (ADR-0005).
- Linux (CI, development): `getdents64` + `statx` with a minimal mask.
- Portable `std::fs` fallback for tests and mobile app containers.

Scheduling:

- Directory tasks on a sized `rayon` pool (cores − 1) with per-volume semaphores,
  priority bands (targeted > interactive > background), bounded channels between
  enumerator → evidence collector → graph builder → store writer, batched transactions,
  and a stall watchdog.
- Tokio orchestrates operations with one `CancellationToken` per scan and child
  tokens per volume. Walkers check cancellation per directory. Tokio's blocking pool is
  capped explicitly.
- Per-directory errors become `Denied(...)` or `Error(...)` nodes; the scan continues.
- Checkpoints in `index.db` make scans resumable; resumed results are labelled
  approximate until reconciled.

Size accounting (`SizeFacts` per entry):
`{logical, allocated, private?, clone_id?, file_identity, link_count, flags{sparse, compressed, dataless, placeholder, may_share_blocks}}`.

- `reclaimable_now` = the sum of `private` (APFS) or `allocated`, counted once per
  unique file identity, and only when all links or clone members are in the selection.
- The UI shows logical size, allocated size, reclaimable now, and "after snapshots
  expire" separately, never a single "size".
- On macOS, walk the Data volume once and de-duplicate firmlinks by identity; present
  the sealed system volume as one opaque "macOS" figure.

Never hydrate: placeholder and dataless files (`SF_DATALESS`,
`FILE_ATTRIBUTE_RECALL_ON_*`) are never opened or hashed. On macOS,
`IOPOL_MATERIALIZE_DATALESS_FILES_OFF` is set process-wide as a backstop. Archives are
never opened.

### Consequences

- Good: near-native scan speed with correct reclaim math.
- Bad: three native enumerators to maintain, each needing a fuzzed parser and fixture
  tests.

Rejected: option 2 cannot return clone, private-size or placeholder data; option 3
costs a thread-pool hop per syscall.

## More information

- [Scanning research, Implications 1–4](../research/10-filesystem-scanning-quarantine-duplicates.md)
- [macOS research, Implications 2–4](../research/03-macos-platform.md)
- [Windows research, §B](../research/04-windows-platform.md)
- [Rust ecosystem research, R3](../research/02-rust-ecosystem.md)
