# ADR-0017: Scan with native batch enumerators on a work-stealing pool and account sizes precisely

- Status: Accepted (macOS `PRIVATESIZE` pass amended by [ADR-0028](0028-macos-private-size-in-bulk-pass.md);
  scheduling and cancellation amended below)
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

Scheduling (as decided; the amendment below records what was built instead):

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

### Amendment: the scheduler uses scoped threads and Lumen's own token (2026-11-02)

The scheduling above named `rayon`, Tokio and `tokio-util`'s `CancellationToken`. No
`Cargo.toml` in the workspace depends on any of them; the engine was built with the
standard library instead, and this records what it does:

- **Traversal.** `lumen_scan::scan` (`crates/lumen-scan/src/scheduler.rs`) runs
  `ScanConfig::workers` scoped `std` threads (default: `available_parallelism`). They
  take one directory at a time from a shared queue and send results through a bounded
  `sync_channel` (default capacity 4096), so a slow consumer slows the workers. Work
  items are whole directories, so a shared queue gives the same parallelism as a
  work-stealing pool without a dependency (the module docs say why; the benchmarks in
  roadmap 5.19 revisit it). There are no per-volume semaphores, priority bands or stall
  watchdog yet.
- **Consumer and store writer.** The calling thread receives the results, aggregates
  coverage and sizes, and emits progress (`crates/lumen-app/src/scan.rs`). Records for
  `index.db` go through a second bounded channel to one dedicated writer thread that
  commits in batches (`crates/lumen-store-sqlite/src/writer.rs`).
- **Cancellation.** `lumen_application::CancelToken`
  (`crates/lumen-application/src/cancel.rs`) is a chain of `AtomicBool` flags: clones
  share cancellation, and `child()` tokens are cancelled with their parent but not the
  reverse. Callers create one token per operation; no caller makes per-volume children
  yet. Workers check the token before each directory and on every entry, and the
  calling thread polls it every 50 ms while waiting, so cancellation takes effect even
  while a worker is blocked in a slow listing. Inventory sources check it between
  items (`InventorySource::collect`).
- **Async ports.** `JudgeModel` and `Describer` return boxed futures
  (`crates/lumen-application/src/ports.rs`). There is no async runtime:
  `lumen_app::block_on` (`crates/lumen-app/src/runtime.rs`) polls the future on the
  calling thread and parks it until woken. The OpenRouter adapter runs each blocking
  `ureq` request on its own thread and wakes the future when the answer arrives
  (`crates/lumen-jev/src/openrouter.rs`).

The decision itself (native batch enumerators run as synchronous directory tasks, not
`tokio::fs`) is unchanged. Adding Tokio or `rayon` later needs its own change with the
dependency reasoning `AGENTS.md` asks for.

### Amendment: Windows candidates are measured by handle when assessed (2026-11-04)

A Windows directory record has no hard-link count, so the Windows enumerator marks
every listed file `may_share_blocks`, and its reclaim estimate is a lower bound of
zero (`SizeFacts::whole_file_reclaim`). `CandidateVerifier` was meant to make it
exact, but nothing called it while assessing, and it re-reads only the object it is
given: a directory candidate kept the listing's figures for everything below it.
Every Windows directory candidate therefore reported 0 B, and `eligible.frees-nothing`
kept it, so on Windows Lumen proposed nothing (reproduced 2026-11-04: a 2 MB
`node_modules` beside `package-lock.json` was `KEEP`, "removing it would free no
space").

Decision: when a candidate's part of the scan is read for a decision, at assessment
and again at confirmation, the Windows composition root re-reads the files below it
through `CandidateVerifier` before the estimate is made
(`lumen_application::measure`). A verified file replaces its listed size facts only
when it is still the same object of the same kind; otherwise the listing's figures
stay. Cloud placeholders and dataless files are never opened (they free nothing
anyway), and nothing is opened for data. At most 100,000 files are re-read per
candidate; the rest keep the listing's lower bound, so the estimate never
over-promises and stays marked inexact.

The verified facts are exact only where the verifier says so: NTFS or FAT, not
deduplicated, and a compressed size read for compressed or sparse files. A file with
another link outside the candidate still frees nothing (pnpm stores link into
`node_modules` with hard links on Windows), and ReFS keeps the lower bound. The
`eligible.frees-nothing` gate is unchanged ([research 24](../research/24-space-recovery-experiment.md)).
macOS already measures `PRIVATESIZE` in the listing and is not re-read; the portable
`std::fs` adapter cannot rule out clone sharing even by handle, so Linux keeps its
lower bound.

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
