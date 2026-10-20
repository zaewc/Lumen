# Scanner safety contract

What Lumen's scanner guarantees, where each guarantee is enforced, and which test
proves it. The scanner only **observes**: nothing in it can change a file. Every
later stage (evidence, policy, quarantine) relies on these guarantees, so a change
that weakens one needs an ADR.

Decisions behind it:
[ADR-0008](../decisions/0008-capability-model-and-coverage.md) (coverage),
[ADR-0017](../decisions/0017-scan-engine-and-size-accounting.md) (engine and sizes),
[ADR-0018](../decisions/0018-incremental-scanning-change-feeds.md) (change feeds),
[ADR-0028](../decisions/0028-macos-private-size-in-bulk-pass.md) (`PRIVATESIZE`),
[ADR-0029](../decisions/0029-fsevents-absolute-streams.md) (`FSEvents`).

## Components

| Component | Crate | Role |
| --- | --- | --- |
| `DirEnumerator` port | `lumen-application` | metadata of one object, entries of one directory |
| `CandidateVerifier` port | `lumen-application` | re-read one candidate by handle before any proposal |
| `ChangeFeed` port | `lumen-application` | changes since a resume point |
| Scheduler (`scan`, `resume`) | `lumen-scan` | parallel breadth-first traversal, backpressure, cancellation |
| `CoverageBuilder`, `SizeAggregator` | `lumen-scan` | what was seen, and per-directory sizes |
| `StdFsEnumerator` | `lumen-scan` | portable Unix fallback |
| `BulkEnumerator`, `FsEventsFeed` | `lumen-platform-macos` | `getattrlistbulk`, `FSEvents` |
| `HandleEnumerator`, `DirectoryChangesFeed` | `lumen-platform-windows` | directory handles, `ReadDirectoryChangesW` |
| `IndexStore`, `IndexWriter` | `lumen-store-sqlite` | `index.db` (each directory path once, entries as directory plus name; upgrading to schema v6 requires a rescan, [performance](performance.md#scan-storage)), snapshots, resume frontier |

## Invariants

### 1. Read-only

The scanner never writes, renames, deletes or changes permissions on anything it
scans, and never reads file contents.

- Destructive `std::fs` calls are banned outside `lumen-fs-exec` by clippy
  `disallowed-methods`. The lint also rejects them in tests: a test needing a
  missing file uses a path that never existed.
- macOS opens only directories (`O_DIRECTORY | O_NOFOLLOW`) and otherwise uses
  `getattrlist`. Windows opens with `FILE_LIST_DIRECTORY` or `FILE_READ_ATTRIBUTES`
  only; no data access right is ever requested.
- Proof: `lumen-cli` tests assert scanned files are unchanged; the conformance
  suite's named pipe would block a scanner that opened files (`special_files`).

### 2. Links and reparse points are never followed

Symlinks, junctions, mount-point reparse points and cloud placeholders are reported
as entries of their own kind and never traversed. A "directory" that is a reparse
point is not a directory.

- macOS: `O_NOFOLLOW`, `FSOPT_NOFOLLOW`. Windows: `FILE_FLAG_OPEN_REPARSE_POINT`,
  and `read_dir` refuses a reparse point. Portable: `symlink_metadata`.
- Proof: `conformance::link_loops` (links to `.`, `..`, `/`, cycles) on every
  Unix enumerator; Windows `junction_loops_are_recorded_and_never_followed`.

### 3. Cloud content is never downloaded

- macOS: every enumerator call turns dataless materialization off for its thread
  (`IOPOL_MATERIALIZE_DATALESS_FILES_OFF`, restored afterwards), because the
  process policy is inherited and was measured as `ON`. Dataless directories are
  reported `NotScanned`, not listed.
- Windows: every call exposes placeholders for its thread
  (`PHCM_EXPOSE_PLACEHOLDERS`), so they appear as reparse points and are never
  opened.
- Dataless, recall-on-access, pinned and sync-root items are `cloud_placeholder`
  and never count as reclaimable.
- Proof: guard tests on both platforms; the sink observes the guarded mode during
  listing; a manual scan of iCloud Drive left dataless files dataless
  ([research note](../research/03-macos-platform.md)).

### 4. Identity before trust

- A directory is listed only if its identity, checked on the opened handle, equals
  the identity it had when discovered; otherwise `IdentityChanged`. This catches a
  directory swapped for a link between discovery and listing.
- Each directory identity is listed once, so firmlinks (`/Users` and
  `/System/Volumes/Data/Users`), bind mounts and overlapping roots neither
  double-count nor loop; later paths are reported as `Skipped::Alias`.
- Firmlinks and mount points are re-read so identities match `lstat`.
- Proof: `refuses_*_swapped_*` tests per enumerator;
  `each_directory_is_listed_once_and_alias_loops_end`;
  `identities_match_lstat_across_firmlinks_and_mount_points`.

### 5. Unknown is never empty

A directory that could not be read is `Denied(reason)`, `Failed(kind)` or
`NotScanned`, never an empty listing. An entry that cannot be parsed or mapped
makes its listing incomplete. Coverage reports every such gap, and sizes over an
incomplete subtree are lower bounds.

- Proof: `unreadable_directory_is_denied_not_empty` (portable and macOS);
  `a_missing_path_is_an_incomplete_scan_not_an_empty_one` (CLI, both platforms).

### 6. Untrusted input stays bounded and escaped

- Kernel buffers (`getattrlistbulk`, `FILE_ID_EXTD_DIR_INFO`,
  `FILE_NOTIFY_INFORMATION`) are parsed by safe, bounds-checked code with property
  tests that never panic on arbitrary or mutated bytes, and are tested on every CI
  platform.
- Names must be one path component (no `/`, `\`, NUL, `.` or `..`).
- Display strings escape control, bidirectional and invisible characters.
- Proof: parser property tests; `conformance::hostile_names` (newlines, ANSI
  escapes, right-to-left override, shell metacharacters, 255-byte names).

### 7. Sizes never over-promise

- Usage counts each file identity once. A file is reclaimable from a directory only
  if all of its links are inside it.
- macOS measures `PRIVATESIZE` (bytes freed by deletion, clone- and snapshot-aware).
  A file without it counts as sharing blocks, so as reclaiming nothing exactly.
- Windows directory records have no link count, so listings report a lower bound;
  `CandidateVerifier` reads the exact link count, compressed size and filesystem
  (exact only on NTFS and FAT, never for deduplicated or ReFS files).
- Proof: aggregator tests; `verification_reads_exact_link_counts_on_ntfs`;
  `verification_measures_compressed_files`.

### 8. Bounded resources and prompt cancellation

- Bounded channels between workers and consumer; a cancel token is checked during
  listing.
- Memory: single-link files are aggregated as they stream; only hard-linked files
  and per-directory totals stay in memory (1.3 M files: 201 MB peak,
  [performance.md](performance.md)).
- Proof: scheduler cancellation and backpressure tests; benchmarks assert entry
  counts.
- Disk: recording a scan or an assessment never leaves less than 2 GB free on the
  database's volume (`lumen_app::space`). A scan checks before it starts and at
  every progress snapshot, and if space runs short it stops with partial,
  incomplete results (`out_of_space`). An assessment counts the records it is
  about to write (1 kB each, a wide margin) and checks before writing them. Free
  space that cannot be measured counts as too little.
- Proof: `lumen-app` `scan::tests::recording_*` with an injected disk that fills.

### 9. Change feeds are advisory

Feeds only decide what to rescan. Resume points are checked (`FSEvents`: same
volume UUID, no ID regression; `ReadDirectoryChangesW`: no history, so always a
rescan). Coalesced, dropped or unparseable events widen the rescan. Nothing acts on
a feed without verification by handle.

## The whole disk

With no folder given, `lumen scan` scans the startup disk (`lumen_app::roots`):

- macOS: each data-volume firmlink in `/usr/share/firmlinks` (`/Users`,
  `/Applications`, `/Library`, `/private`, `/opt`, `/usr/local` and the system's
  asset and cache folders), but not `/Volumes`. The sealed system volume holds
  nothing that can be cleaned.
- Linux: `/`, and the home folder if it is on another file system.
- Windows: the system drive.

Each root stays on its own volume, so other disks and network or cloud mounts are
never entered. On a 461 GB Mac it read 4.9 M entries (431 GB allocated) in 62 s
without a database. Folders macOS keeps private without Full Disk Access are
reported as denied, never as empty.

## Where `unsafe` lives

Every block has a `// SAFETY:` comment stating the invariant, why `unsafe` is
needed, and why the invariant holds. All other crates forbid or deny `unsafe`.

| File | What |
| --- | --- |
| `lumen-platform-macos/src/sys.rs` | `getattrlistbulk`, `getattrlistat`, `getattrlist`; `getiopolicy_np`/`setiopolicy_np` |
| `lumen-platform-macos/src/feed.rs` | `FSEvents` stream lifecycle, event-store UUID, callback arrays |
| `lumen-platform-windows/src/sys.rs` | `CreateFileW`, `CloseHandle`, `GetFileInformationByHandleEx`, `GetVolumeInformationByHandleW`, thread placeholder mode, overlapped `ReadDirectoryChangesW` |

Asynchronous I/O (`Watch`) keeps its buffer and `OVERLAPPED` at fixed heap
addresses and cancels and drains a pending read before freeing them. The `FSEvents`
stream is released before the inbox its callback writes to.

## Adding an enumerator

1. Implement `DirEnumerator` and `CandidateVerifier`. Put buffer parsing and
   mapping in safe modules compiled on every platform, and keep the system calls
   in one `sys` module.
2. Run `lumen_scan::conformance` (feature `conformance`) against it, or the
   platform's equivalent (Windows: `tests/adversarial.rs`).
3. Add benchmarks and record a baseline in [performance.md](performance.md).
4. Update the platform matrix in [platforms.md](platforms.md) and this document.

## Known limits

- One volume per `FSEvents` feed; one thread per root for `ReadDirectoryChangesW`.
- `FileIdExtdDirectoryInfo` support on FAT/exFAT is unverified; failures are
  reported as incomplete listings.
- Clone IDs are unused: APFS does not set `VOL_CAP_FMT_CLONE_MAPPING`.
- Sync-root status is per directory; descendants inherit it in the evidence graph
  (Phase 6).
