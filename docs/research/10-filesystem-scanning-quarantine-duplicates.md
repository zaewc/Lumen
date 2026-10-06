# Filesystem Scanning Engine, Incremental Scanning, Quarantine, Duplicate Detection and Developer-Artifact Knowledge Base

> Researched: 2026-10-05 · Scope: cross-platform Rust scan engine (traversal, size accounting, change tracking, engine mechanics), reversible delete/quarantine, duplicate detection, and a per-tool developer-artifact cleanup knowledge base for Lumen

Companion documents: `01-desktop-architecture.md` (process model, quarantine executor placement), `04-windows-platform.md` (MFT/USN, `NtQueryDirectoryFileEx`, Recycle Bin, Windows quarantine adapter), `06-ios-and-mobile-framework.md` (iOS `trashItem`, Photos). This document deliberately does not repeat their Windows-specific depth. It covers the cross-platform engine and the parts the others leave open.

## Summary

- **Syscalls dominate scan time, not CPU.** A profiled macOS scanner spent about 91% of its time in syscalls ([healeycodes](https://healeycodes.com/maybe-the-fastest-disk-usage-program-on-macos)). The levers that matter are batch metadata syscalls and parallelism at the directory level: `getattrlistbulk` on macOS, `getdents64` plus a minimal `statx` mask on Linux, and `NtQueryDirectoryFileEx` or the MFT on Windows. Async I/O does not help here. The existing fast tools (dust, dua, fd/`ignore`, jwalk) all use a pool of blocking threads that steal work at directory granularity.
- **Use breadth-first, one-directory-at-a-time enumeration on macOS.** A developer reported that nested `getattrlistbulk` enumeration loops forever on some SMB volumes (a Windows Storage Spaces server) in macOS 15. Apple DTS did not confirm a macOS bug: it suggested the SMB server or an Endpoint Security client could be the cause, and said any fix would likely wait for a major release. It did recommend flattening to breadth-first iteration with one directory open at a time, and a 32 KB buffer like `fts` uses. DTS also pointed out that the URL-based `FileManager` enumeration APIs handle such edge cases ([Apple forums](https://developer.apple.com/forums/thread/766035)).
- **Record five sizes per file, not one:** logical, allocated, private (APFS `ATTR_CMNEXT_PRIVATESIZE`, "bytes … which would be freed immediately if the file were deleted"), clone ID, and file ID/link count ([getattrlist(2)](https://keith.github.io/xcode-man-pages/getattrlist.2.html)). Show "reclaimable now" separately from "total size". Deleting one APFS clone, one hard link, or a file still held by a snapshot frees nothing.
- **Never hydrate cloud placeholders.** Detect them from metadata only. On macOS that is `SF_DATALESS` in `st_flags` ([chflags(2)](https://keith.github.io/xcode-man-pages/chflags.2.html)). On Windows it is `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS` / `RECALL_ON_OPEN` ([Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/fileio/file-attribute-constants)). Duplicate hashing must skip placeholders.
- **Incremental scanning = snapshot DB + change journal + periodic full sweep.** The change journal is FSEvents (persist event ID and per-device UUID), USN (persist JournalID and NextUsn), or fanotify/inotify on Linux. Apple's own guidance is to start the event stream *before* the scan, rescan on `MustScanSubDirs`, and still do periodic full sweeps ([FSEvents guide](https://developer.apple.com/library/archive/documentation/Darwin/Conceptual/FSEvents_ProgGuide/UsingtheFSEventsFramework/UsingtheFSEventsFramework.html)).
- **`notify` is fine for live UI watches but is not a source of truth.** The latest stable release is 8.2.0 (2025-08-03); 9.0.0-rc.5 is a pre-release (2026-08-30) ([crates.io](https://crates.io/crates/notify)). Its docs list drops on large trees, inotify limits, NFS blindness, and FSEvents trouble with files the user does not own ([docs.rs](https://docs.rs/notify/latest/notify/)). It does not expose FSEvents event-ID replay or the USN journal, so Lumen needs its own adapters for those.
- **Quarantine = same-volume atomic rename into a Lumen-owned store, driven by a write-ahead journal.** The rename keeps the inode, so xattrs, ACLs, mode and mtime survive by construction. Never fall back to copy+delete silently: on `EXDEV`, use a quarantine dir on that volume, or the OS trash, or downgrade to REVIEW. **Quarantine frees no space until finalization**, and the UI must say so.
- **OS trash is a release valve, not the quarantine.** The `trash` crate is at 5.2.9 (2026-09-13). Its `os_limited::{list, restore_all, purge_all}` APIs exist only on Windows and freedesktop and are **not available on macOS** ([docs.rs](https://docs.rs/trash/latest/trash/os_limited/index.html)). macOS `trashItem(at:resultingItemURL:)` returns the new location, which Lumen must record ([Apple](https://developer.apple.com/documentation/foundation/filemanager/trashitem(at:resultingitemurl:))). The `trash` crate discards that URL and defaults to a Finder/`osascript` method that can raise an Automation permission prompt (verified in the 5.2.9 source), so Lumen should call `trashItem` directly on macOS. Windows has no documented restore API (see doc 04).
- **Duplicate pipeline (fclones model):** size → drop same file ID → head hash → tail hash → full hash. fclones' defaults are a 4 KiB prefix/suffix on SSD and 16 KiB on HDD (stated in the `max_prefix_size`/`max_suffix_size` doc comments in fclones 0.35.0 `src/config.rs`, not in the README; [fclones](https://github.com/pkolaczk/fclones)). fclones' own default hash is 128-bit `metro`, with `xxhash3`, `blake3` and SHA-2/SHA-3 selectable via `--hash-fn`. Use XXH3-128 for the cheap stages and BLAKE3 for the full content ID. xxHash's own docs say it is "not a cryptographic hash function" ([xxHash](https://github.com/Cyan4973/xxHash)), and that matters when a hash collision would delete a file.
- **Near-duplicate images are evidence for REVIEW only.** `image_hasher` 3.1.1 provides Mean/Gradient/DoubleGradient/VertGradient/Blockhash with Hamming distance ([docs.rs](https://docs.rs/image_hasher/latest/image_hasher/)). It should never trigger QUARANTINE on its own.
- **Use the tool's own cleanup command when one exists.** Examples: `docker system prune`, `pnpm store prune`, `uv cache prune`, `go clean -modcache`, `brew cleanup`, `xcrun simctl delete unavailable` / `simctl runtime delete`, `npm cache verify`, `pod cache clean`, `sdkmanager --uninstall` / `android sdk remove`, `claude purge`. uv says outright that "it's never safe to modify the cache directly" ([uv docs](https://docs.astral.sh/uv/concepts/cache/)). Go's module cache is read-only on purpose ([Go modules ref](https://go.dev/ref/mod)). Docker warns against moving `Docker.raw` ([Docker FAQ](https://docs.docker.com/desktop/troubleshoot-and-support/faqs/macfaqs/)).
- **Several caches already clean themselves.** Cargo has automatic GC since Rust 1.88: downloaded items unused for 3 months, local items for 1 month ([Rust 1.88](https://blog.rust-lang.org/2025/06/26/Rust-1.88.0/)). Gradle cleans automatically (30 days downloaded / 7 days created; runs in the background when the daemon stops, by default at most every 24 h, and only for Gradle versions that have run on the machine) ([Gradle 9.8.0](https://docs.gradle.org/current/userguide/directory_layout.html)). JetBrains removes caches of IDE versions unused for 180 days ([JetBrains](https://www.jetbrains.com/help/idea/directories-used-by-the-ide-to-store-settings-caches-plugins-and-logs.html)). Claude Code removes transcripts after `cleanupPeriodDays` (default 30), except Claude Desktop/Cowork transcripts, which have no age limit unless `desktopSessionCleanupPeriodDays` is set, and except when the sweep is paused ([Claude Code docs](https://code.claude.com/docs/en/claude-directory)). Lumen should report these as "self-managed" and not compete with them.
- **Some "developer junk" is not regenerable.** This includes Xcode Archives (dSYMs and submission builds), simulator and AVD app data, named Docker volumes, Yarn zero-install caches committed to git, and AI-tool transcripts (conversation history, which is privacy-sensitive). These default to REVIEW.
- **pnpm/uv hard links distort reclaim estimates.** pnpm's `node_modules` and uv's virtualenvs hard-link or clone files out of a central store. Deleting one `node_modules` can free close to nothing. Reclaim math must count by unique file ID and must include link-count checks.

## Findings

### 1. Parallel traversal

**What the fast tools do**

| Tool | Version (verified 2026-10-05) | Traversal model | Notes |
|---|---|---|---|
| dust (`du-dust`) | 1.2.6 (2026-09-16) | Parallel walker threads; `-T` sets threads, default CPU count | "For high-latency storage like NFS … a higher count can speed up the walk by overlapping more concurrent stat calls"; does not double-count hard links ([README](https://github.com/bootandy/dust)) |
| dua-cli | 2.45.1 (2026-09-30) | jwalk (rayon) parallel; "will max out your SSD" | `--apparent-size`; opt-in `--deduplicate-apfs-clones` counts fully shared clones once; trash or delete with multi-stage confirmation; `dua clean` finds disposable dev dirs but only deletes on confirmation ([README](https://github.com/Byron/dua-cli)) |
| diskus | 0.9.0 (2025-12-06) | Parallel `du -sh` replacement | About 2.6x slower than a `getattrlistbulk` scanner in one warm-cache macOS benchmark ([healeycodes](https://healeycodes.com/maybe-the-fastest-disk-usage-program-on-macos)) |
| gdu | v5.38.0 (2026-10-05) | Go, goroutine-parallel | Only the version was verified (GitHub releases API) |
| fd (`fd-find`) / `ignore` | fd 10.5.0; `ignore` 0.4.33 | `WalkParallel` runs a per-thread visitor (`run`/`visit` with `ParallelVisitorBuilder`); `WalkState::{Continue, Skip, Quit}` | Results come back unordered; per-thread visitors accumulate state and merge on `Drop` ([docs.rs](https://docs.rs/ignore/latest/ignore/struct.WalkParallel.html)) |
| jwalk | 0.9.0 (2026-08-05, first release since 2022) | rayon; parallel per directory; sorted streamed results; `process_read_dir` callback to filter or skip | Parallelism "won't help when reading a single directory with many files" ([README](https://docs.rs/crate/jwalk/latest/source/README.md)) |
| WizTree / Everything | n/a | Read the NTFS MFT and USN journal directly (admin or service) | Covered in doc 04 §2–3 |
| ncdu | n/a | Single-threaded C (1.x) / Zig (2.x) | (unverified for current version; not consulted) |

**Syscall cost is the bottleneck.** dumac, a macOS du clone, replaced per-file `lstat` with `getattrlistbulk(2)`. Into a 128 KB buffer it got 521 ms against 3.33 s for `du -sh` on 409,500 files, warm cache. Its concurrency was Tokio tasks limited by a 64-permit semaphore, and it used a 128-shard inode `HashSet` for hard-link dedup, which cost about 1.5% of runtime. Flamegraphs showed 91% of time in syscalls ([healeycodes](https://healeycodes.com/maybe-the-fastest-disk-usage-program-on-macos)). The lesson: the concurrency framework barely matters. Fewer syscalls and enough of them in flight are what matter.

**Platform batch primitives**
- **macOS:** `getattrlistbulk(int dirfd, struct attrlist*, void* buf, size_t, uint64_t options)`. It needs `ATTR_CMN_NAME` and `ATTR_CMN_RETURNED_ATTRS`. In one call per buffer it can return the type, `ATTR_CMN_FILEID`, `ATTR_FILE_LINKCOUNT`, `ATTR_FILE_TOTALSIZE`/`ALLOCSIZE`, `ATTR_CMN_FLAGS` (covers `SF_DATALESS`), and the extended `ATTR_CMNEXT_PRIVATESIZE`, `ATTR_CMNEXT_CLONEID` and `ATTR_CMNEXT_EXT_FLAGS` (`EF_MAY_SHARE_BLOCKS`, `EF_IS_SPARSE`) ([getattrlist(2)](https://keith.github.io/xcode-man-pages/getattrlist.2.html)). **Known issue (reported, root cause not confirmed by Apple):** on macOS 15, recursive (nested) `getattrlistbulk` enumeration over some SMB shares restarts (`smbfs_fetch_new_entries: Restart enum offset …`) and repeats entries forever. DTS thought the SMB server or an Endpoint Security client might be responsible. Either way the scanner needs a repeat-entry/loop guard (abort that directory and mark it `incomplete` if the enumeration repeats a file ID already seen in the same directory). DTS's workaround is flattened, breadth-first iteration with only one directory open at a time, and a buffer of about 32 KB. Apple's own implementation allocates about 15 KB for `ATTR_CMN_NAME` alone (unverified: not found when re-reading the thread on 2026-10-06) ([Apple forums](https://developer.apple.com/forums/thread/766035)).
- **Linux:** `getdents64` (via `readdir`/`std::fs::read_dir`) gives `d_type` and `d_ino` for free, so the scanner can skip `stat` for directories when only the structure is needed. Then call `statx(dirfd, name, AT_SYMLINK_NOFOLLOW | AT_STATX_DONT_SYNC, mask)` with a minimal mask: `STATX_TYPE|STATX_SIZE|STATX_BLOCKS|STATX_INO|STATX_NLINK|STATX_MTIME`, plus `STATX_MNT_ID` (5.8+) to detect mount crossings. `AT_STATX_DONT_SYNC` avoids network round trips on NFS/CIFS at the cost of slightly stale data. `stx_attributes` exposes `STATX_ATTR_COMPRESSED` and `STATX_ATTR_MOUNT_ROOT`. statx arrived in Linux 4.11 ([statx(2)](https://man7.org/linux/man-pages/man2/statx.2.html)).
- **Windows:** `NtQueryDirectoryFileEx` / `FindFirstFileExW(FindExInfoBasic, FIND_FIRST_EX_LARGE_FETCH)` return size, attributes and reparse tag per entry in batches. For the unelevated and elevated MFT/USN paths, see doc 04.
- **Android / iOS:** no batch primitive is exposed to apps. Scope is limited to app containers, MediaStore and Files-app bookmarks (docs 05/06).

**Work-stealing vs async.** Every metadata call above is blocking. `tokio::fs` runs each operation on the blocking pool (`spawn_blocking`), which adds a hop per syscall. The workable designs are (a) a rayon or crossbeam-deque pool where each task is "enumerate one directory", which is what jwalk, `ignore` and dua do, or (b) Tokio tasks that each own a whole directory batch and use a semaphore, which is dumac's approach. Use (a) for the engine. Tokio 1.53.2 stays in the orchestration and IPC layer only.

**Benchmark methodology.** Use `hyperfine` (v1.21.0, 2026-10-05) with `--warmup 3` for warm cache. For cold cache on Linux, use `--prepare 'sync; echo 3 | sudo tee /proc/sys/vm/drop_caches'`. On macOS there is no reliable cache flush (dumac's author says so too). `sudo purge` is the usual approximation (unverified effectiveness), so a reboot or a freshly mounted APFS disk image is the honest cold case. Measure on fixed fixture trees (deep/narrow, wide/flat, 1M small files, `node_modules`-like, hard-link-heavy, clone-heavy), plus one real home directory. Report wall time, peak RSS, syscall count (`dtruss`/`strace -c`) and entries/s. Gate CI regressions on the fixtures, not on real home directories.

### 2. Size accounting correctness

| Concept | macOS (APFS) | Linux | Windows (NTFS/ReFS) | Lumen rule |
|---|---|---|---|---|
| Logical (apparent) size | `ATTR_FILE_TOTALSIZE` (all forks) / `DATALENGTH` | `stx_size` | `EndOfFile` | Display only |
| Allocated size | `ATTR_FILE_ALLOCSIZE` | `stx_blocks * 512` | `AllocationSize` (compressed or sparse: `GetCompressedFileSizeW`) | Base of "on-disk" |
| Identity | `ATTR_CMN_FILEID` (+ volume UUID); `ATTR_CMNEXT_LINKID` | `(stx_dev, stx_ino)` | 128-bit FileId + volume serial | Count each identity once |
| Hard links | `ATTR_FILE_LINKCOUNT` | `stx_nlink` | `NumberOfLinks` | Reclaim only if **all** links are in the selection |
| Clones / reflinks | `ATTR_CMNEXT_CLONEID`, `EF_MAY_SHARE_BLOCKS`, **`ATTR_CMNEXT_PRIVATESIZE`** | btrfs/XFS reflinks: FIEMAP shared extents (unverified, not consulted) | ReFS block cloning (doc 04) | Reclaimable = private size |
| Sparse | `EF_IS_SPARSE` | allocated < logical | `FILE_ATTRIBUTE_SPARSE_FILE` | Use allocated |
| Compressed | `UF_COMPRESSED` (decmpfs; verified in `chflags(2)` and `<sys/stat.h>`, an internal flag that user space must not set) | `STATX_ATTR_COMPRESSED` | `FILE_ATTRIBUTE_COMPRESSED` | Use allocated |
| Cloud placeholder | `SF_DATALESS` in `st_flags` | FUSE-specific | `RECALL_ON_DATA_ACCESS` / `RECALL_ON_OPEN`, `UNPINNED`, `OFFLINE` | Never open or read; tag as "cloud" |

Key facts:
- `ATTR_CMNEXT_PRIVATESIZE` is "the number of bytes that are **not** trapped inside a clone or snapshot, and which would be freed immediately if the file were deleted" ([getattrlist(2)](https://keith.github.io/xcode-man-pages/getattrlist.2.html)). It is the only first-party per-file reclaim figure on any platform, and it already covers APFS snapshots.
- Deleting one APFS clone frees nothing while the other clone exists: "those components remain, as they're still required by the other clone". Snapshots also retain blocks ([Eclectic Light](https://eclecticlight.co/2024/03/20/apfs-files-and-clones/)). The same article says that before the CLONEID attribute was exposed, "there's no direct way to tell which are clones of one another". `ATTR_CMNEXT_CLONEID` only identifies *pure* clones (identical data streams). Partially diverged clones need PRIVATESIZE.
- Further `getattrlist(2)` attributes (verified in the local macOS 26 man page) that Lumen should request in the same bulk call: `ATTR_CMNEXT_CLONE_REFCNT` (number of full clones sharing all blocks), and `ATTR_CMNEXT_EXT_FLAGS` bits `EF_SHARES_ALL_BLOCKS` (full clone), `EF_IS_PURGEABLE` (the file system itself may delete the item "when asked to free space", so it counts toward purgeable space and is not a Lumen target), `EF_IS_SYNC_ROOT` (directory is a File Provider/cloud sync root, so apply the sync-root policy), and `EF_NO_XATTRS` (skip `listxattr` in the manifest). `ATTR_CMNEXT_NOFIRMLINKPATH` returns a firmlink-free path, which resolves the Data-volume double-count question in §4. `VOL_CAP_FMT_CLONE_MAPPING` says whether a volume supports full clone tracking; on volumes without it, treat CLONEID and CLONE_REFCNT as unknown.
- Dataless files: `SF_DATALESS` means "the system will attempt to materialize it when accessed". Detect it with `stat`/`getattrlist` flags and never with `read` ([chflags(2)](https://keith.github.io/xcode-man-pages/chflags.2.html)). As defense in depth, the scanner and hasher processes should call `setiopolicy_np(IOPOL_TYPE_VFS_MATERIALIZE_DATALESS_FILES, IOPOL_SCOPE_PROCESS, IOPOL_MATERIALIZE_DATALESS_FILES_OFF)`, which "disables materialization of dataless files by the current thread or process" (verified in the local `getiopolicy_np(3)` man page; "new processes inherit the policy of their parent process"). A stray `open`/`read` then fails instead of downloading. Do not rely on the documented system default alone. Modern iCloud Drive evicts in place (`st_blocks == 0`, `st_size > 0`) instead of leaving `.icloud` siblings ([Eclectic Light](https://eclecticlight.co/2023/10/25/macos-sonoma-has-changed-icloud-drive-radically/)). On Windows, `RECALL_ON_DATA_ACCESS` means "reading the file … will cause at least some of the file/directory content to be fetched from a remote store" ([Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/fileio/file-attribute-constants)).
- Purgeable space: Foundation exposes `volumeAvailableCapacityForImportantUsageKey` and `…ForOpportunisticUsageKey` alongside `volumeAvailableCapacityKey`. The "important" figure is generally understood to include space the system can free by purging (unverified: the page consulted does not spell out what is purged) ([Checking volume storage capacity](https://developer.apple.com/documentation/foundation/checking-volume-storage-capacity)). The exact purge contents are not documented. The key also requires a `PrivacyInfo.xcprivacy` required-reason declaration on iOS ([key docs](https://developer.apple.com/documentation/foundation/urlresourcekey/volumeavailablecapacityforimportantusagekey)). Apple's required-reason rules cover apps "on iOS, iPadOS, tvOS, visionOS, or watchOS" (not macOS), and since May 1, 2024 App Store Connect rejects apps that use these APIs without a declared reason ([Describing use of required reason API](https://developer.apple.com/documentation/bundleresources/describing-use-of-required-reason-api)). The disk-space category's exact reason codes were not extracted in this pass (unverified); pick them before the iOS build ships. Lumen should show "free", "free incl. purgeable", and "Lumen-reclaimable" as three distinct numbers.
- Sparse VM disks: `Docker.raw` logical size can be 64 GB while only about 2.3 GB is allocated. Docker itself warns that tools report the maximum size ([Docker FAQ](https://docs.docker.com/desktop/troubleshoot-and-support/faqs/macfaqs/)).
- Hard-linked stores: pnpm hard-links files from its content-addressable store into `node_modules` (see `pnpm store` docs). uv links or clones cache files into venvs, and wants the cache on the same filesystem "to enable efficient file linking" ([uv cache](https://docs.astral.sh/uv/concepts/cache/)). Deleting a pnpm `node_modules` usually frees only the link-count-1 files.

### 3. Incremental scanning

**Change sources per platform**

| Platform | Source | Persisted cursor | Invalidation signals | Granularity |
|---|---|---|---|---|
| macOS | FSEvents per-device stream (`FSEventStreamCreateRelativeToDevice`, `sinceWhen`) | last event ID + `FSEventsCopyUUIDForDevice` UUID | UUID changed → purged/reformatted; ID lower than stored → restored or wrapped; `kFSEventStreamEventFlagMustScanSubDirs` (also set on Kernel/UserDropped); `EventIdsWrapped` | Directory (file-level flag exists, but the guide is written for directory granularity) |
| Windows | USN journal (`FSCTL_QUERY/READ_USN_JOURNAL`) | `{UsnJournalID, NextUsn}` per volume | Journal ID mismatch or wrap → full rescan | File (needs elevation; see doc 04) |
| Linux | fanotify `FAN_MARK_FILESYSTEM` + `FAN_REPORT_DFID_NAME` (directory-entry events with `FAN_REPORT_FID` since 5.1; `FAN_REPORT_DIR_FID`/`FAN_REPORT_NAME`, and so `FAN_REPORT_DFID_NAME`, only since **5.9**) | none (live only) | `FAN_Q_OVERFLOW` | File; needs `CAP_SYS_ADMIN` for filesystem/mount marks. Unprivileged fanotify exists since 5.13 but "the user is limited to only mark inodes" ([fanotify_init(2)](https://man7.org/linux/man-pages/man2/fanotify_init.2.html)), so it is no better than inotify for whole-tree coverage |
| Linux (unprivileged) | inotify (per directory) | none | `IN_Q_OVERFLOW`; `max_user_watches` / `max_user_instances` limits; non-recursive; rename pairs not atomic; no network fs | Directory entry |
| Android / iOS | none for general FS; MediaStore generation (Android), Photos change tokens (iOS) | per doc 05/06 | | |

Sources: [FSEvents guide](https://developer.apple.com/library/archive/documentation/Darwin/Conceptual/FSEvents_ProgGuide/UsingtheFSEventsFramework/UsingtheFSEventsFramework.html), [FSEventStreamCreate](https://developer.apple.com/documentation/coreservices/1443980-fseventstreamcreate), [Change journals](https://learn.microsoft.com/en-us/windows/win32/fileio/change-journals), [inotify(7)](https://man7.org/linux/man-pages/man7/inotify.7.html), [fanotify(7)](https://man7.org/linux/man-pages/man7/fanotify.7.html).

Apple's guidance applies to every platform:
- "To avoid missing changes, you must start monitoring the directory *before* you start scanning it."
- Combine events "with a cached 'snapshot' of the metadata of files within the tree".
- "Backup software should still periodically perform a full sweep."
- "If you are writing software that requires persistence, you should use per-disk streams."

**`notify` crate status**
- Stable 8.2.0, released 2025-08-03. Pre-release 9.0.0-rc.5, released 2026-08-30. `notify-debouncer-full` 0.7.0 is stable; 0.8.0-rc.2 is a pre-release ([crates.io API](https://crates.io/crates/notify)).
- Backends: inotify (Linux/Android), FSEvents or kqueue (macOS), ReadDirectoryChangesW (Windows), kqueue (iOS/BSD), and a `PollWatcher` fallback ([docs.rs](https://docs.rs/crate/notify/latest)).
- Documented problems ([docs.rs](https://docs.rs/notify/latest/notify/)):
  - NFS emits no events.
  - Docker on Apple Silicon fails with "Function not implemented".
  - FSEvents makes it hard to observe files the user does not own.
  - Editors save files in different ways.
  - Deleting a watched folder requires watching its parent.
  - `/proc` and `/sys` do not emit events.
  - inotify limits surface as "No space left on device".
  - It "may fail to receive all events when monitoring very large file sets".
- It does **not** expose FSEvents `sinceWhen` replay across restarts or the USN journal. Lumen therefore needs a `ChangeFeed` port with native adapters. `notify` is acceptable only for live refresh of a few open roots in the UI.

**Merkle-style directory summaries**
- Store, per directory, `dir_digest = H(sorted children (name, type, file_id, size, mtime, child_dir_digest))` using XXH3-128 (not security-relevant). Also store an aggregate `(entries, logical, allocated, private)`.
- When a rescan of a subtree, whether journal-triggered or a sweep, produces the same digest, Lumen skips re-emitting the evidence graph and policy evaluation for that subtree. This is the expensive part, not the walk.
- Caveat: a directory's mtime changes only when its *direct* entries change. A deep edit does not bubble up, so mtime alone cannot prune a walk. Without a journal, the walk must still descend. The digest saves downstream work, not syscalls.
- With a journal, only directories named by events (or their subtrees, on MustScanSubDirs) are re-enumerated. Parent aggregates are then recomputed up the path.

**Resumable scans**
- Persist a checkpoint every N directories or T seconds. The checkpoint holds `scan_id`, `journal_cursor_at_start`, the frontier queue (pending directory IDs and paths), and the set of completed directories with their aggregates. Commit it in one SQLite transaction (rusqlite 0.40.2) or as redb 4.3.0 tables.
- On resume, mark the result `approximate`, finish the frontier, then replay the journal from `journal_cursor_at_start` to repair directories that changed while the scan was paused.
- This depends on starting the change stream before the scan begins, as noted above.

### 4. Scan engine design

```
           ┌─────────── priority frontier (BinaryHeap<DirTask>) ───────────┐
 roots ──▶ │ priority = f(prev_size_hint, is_known_artifact_root, ui_focus) │
           └───────▲──────────────────────────────┬────────────────────────┘
                   │ push children                │ pop (work-stealing pool, N blocking threads)
                   │                              ▼
            ┌──────┴───────┐   bounded chan   ┌────────────┐  bounded chan  ┌─────────────┐
            │ enumerator   │ ───────────────▶ │ aggregator │ ─────────────▶ │ persistence │ (batched txns)
            │ (batch sys-  │  (DirBatch)      │ (sizes,    │  (rows)        │ SQLite/redb │
            │  calls)      │                  │  inode set)│                └─────────────┘
            └──────────────┘                  └─────┬──────┘
                                                    └──▶ progress (coalesced, ≤10 Hz) ──▶ UI
```

- **Unit of work = one directory**, enumerated breadth-first and fully drained before it is closed. This follows the macOS SMB guidance and gives natural checkpoints.
- **Priority:** use the previous snapshot's subtree size as a hint, so big subtrees start first and the UI's "top N" settles early. Known developer-artifact roots go first, then whatever the user is looking at. A strict priority queue can starve deep work. Use priority *bands*: pop from the high band with probability p, otherwise FIFO.
- **Bounded concurrency:** worker count is detected per volume. SSD: about CPU count, capped near 16. HDD: 1–2, and sort by inode or physical placement like fclones does. Network: higher, as dust suggests, but opt-in and with a timeout. Make one `Semaphore` per volume so a slow NAS cannot starve the internal SSD.
- **Backpressure:** bounded crossbeam channels between stages. If persistence falls behind, enumerators block on send, which caps memory. Persist in batches of 1–10k rows per transaction.
- **Memory:** an arena tree (parent index plus interned name, with no full path strings) like fclones' path prefix compression. The hard-link set holds only entries with `nlink > 1`, sharded like dumac's.
- **Cancellation:** a `CancellationToken` (tokio-util) or `AtomicBool`, checked between directory batches. A blocked syscall cannot be cancelled. On network or FUSE mounts the watchdog marks a directory `stalled` after T seconds, abandons that worker (spawning a replacement), and records partial results.
- **Partial failure** is data, not an exception. Record `(path_id, errno/NTSTATUS, phase)` for `EACCES`/`EPERM` (TCC on macOS), `ENOENT` races, `ELOOP`, `ESTALE`, and attribute-unsupported cases. The scan finishes as `completed_with_gaps` and the UI shows coverage.
- **Boundaries:**
  - Never follow symlinks or Windows reparse points.
  - Stay on the root's device (`st_dev` / `STATX_MNT_ID` / volume serial).
  - Skip other users' homes and system-protected areas.
  - On macOS, do not double-count the Data volume through its firmlinked paths: scan by volume and canonicalize paths using `ATTR_CMNEXT_NOFIRMLINKPATH` (attribute verified in `getattrlist(2)`; the exact double-count behaviour still needs a test on a real Data volume).
- **Progress:** the total is unknown. Show entries and bytes discovered, directories pending, and an ETA from the previous scan's entry count or the volume's used bytes. Throttle events, because Tauri events are "not suitable for bigger messages" (doc 01).

### 5. Quarantine and reversible delete

**OS trash facilities**

| Platform | API | Restore programmatically? | Notes |
|---|---|---|---|
| macOS / iOS 11+ | `FileManager.trashItem(at:resultingItemURL:)` | Yes, by moving back from the returned URL. "The actual filename may change", so use the returned URL ([Apple](https://developer.apple.com/documentation/foundation/filemanager/trashitem(at:resultingitemurl:))) | Finder "Put Back" metadata comes only from a Finder-mediated delete. **Verified in the `trash` 5.2.9 source (`src/macos/mod.rs`):** the crate's macOS `DeleteMethod` defaults to `Finder`, which shells out to `osascript` to script Finder ("might ask the user to give additional permissions", i.e. an Automation/Apple Events TCC prompt, and unusable from a sandboxed app). The alternative `NsFileManager` calls `trashItemAtURL:resultingItemURL:error:` with `None` for the resulting URL and has no Put Back. In both cases `delete()` returns `Result<(), Error>`, so **the crate never tells Lumen where the item landed in the Trash**. If Lumen uses the OS trash on macOS, it must call `trashItem(at:resultingItemURL:)` itself (objc2 / Swift shim) and record the URL |
| Windows | `IFileOperation` + `FOFX_RECYCLEONDELETE` | No documented API (shell `undelete` verb only) | Per-SID bins; elevated context goes to the wrong bin. See doc 04 §4 |
| Linux / freedesktop | Trash spec 1.0: `$XDG_DATA_HOME/Trash/{files,info}`, `.trashinfo` with `Path` + `DeletionDate`; `$topdir/.Trash/$uid` (sticky bit required) or `$topdir/.Trash-$uid`; info file created with `O_EXCL` *before* the move ([spec](https://specifications.freedesktop.org/trash/latest/)) | Yes | `$trash/directorysizes` cache: implementations "SHOULD create or update" it since spec 1.0 (a SHOULD, not optional in practice for interop with file managers) |
| `trash` crate 5.2.9 | `delete`, `delete_all`; `os_limited::{list, is_empty, metadata, purge_all, restore_all, trash_folders}` on Windows + freedesktop only; restore errors `RestoreCollision`, `RestoreTwins` | Not on macOS | Docs warn about non-thread-safe libc mount functions on Linux/FreeBSD, guarded by a mutex ([docs.rs](https://docs.rs/trash/latest/trash/)) |

**Lumen-managed quarantine store (recommended primary mechanism)**
1. **Pre-flight (TOCTOU guard):**
   - Open the parent directory fd and re-`lstat` the item.
   - Require that `(file_id, size, mtime, nlink, flags)` match what the policy evaluated. Otherwise, abort the item and re-evaluate it.
   - Check that the item is not in use (Restart Manager on Windows, doc 04; open-file checks on macOS/Linux are best effort).
   - Refuse placeholders and items under protected or sync roots unless the policy explicitly allows them.
2. **Journal intent first:**
   - Append `{op_id, plan_id, item_id, src_path_bytes, src_parent_file_id, file_id, volume_uuid, dst_path, expected_fingerprint, state: INTENT}` to the operation journal. This is the SQLite WAL table plus an fsynced append-only log file in the quarantine root.
   - Fsync before acting, mirroring the freedesktop "write info with `O_EXCL` before move" ordering.
3. **Move:**
   - Use `renameat`-family calls with no-replace semantics: `renameatx_np`/`renamex_np` with `RENAME_EXCL` on macOS (verified in the macOS 26 `rename(2)` man page: returns `EEXIST` if the destination exists, but only "on file systems that support it", so check `VOL_CAP_INT_RENAME_EXCL` per volume and downgrade to REVIEW where it is absent), `renameat2(RENAME_NOREPLACE)` on Linux, and `MoveFileExW` without `MOVEFILE_COPY_ALLOWED` (and without `MOVEFILE_REPLACE_EXISTING`) on Windows.
   - On macOS also pass `RENAME_NOFOLLOW_ANY` (error if any symlink is met while resolving either path) and, with `renameatx_np` relative to the quarantine root fd, `RENAME_RESOLVE_BENEATH`. Both are documented in the same man page and close symlink-swap TOCTOU races on the path components. Linux has no rename flag for this; use `openat2(RESOLVE_NO_SYMLINKS | RESOLVE_BENEATH)` to obtain parent dirfds and rename relative to them.
   - Destination: `<volume quarantine root>/<plan_id>/<item_id>/payload`.
   - On `EXDEV`, never silently copy+delete. Use that volume's own quarantine root, or the OS trash on that volume, or downgrade to REVIEW.
4. **Verify:**
   - `lstat(dst)` shows the same `file_id` and `size`/`mtime`, and `src` is absent (or now a different file ID, which means a race to log). For directories, also compare the immediate entry count.
   - Then `state: MOVED_VERIFIED`. Write a sidecar `manifest.json` next to the payload so the store can be rebuilt if the DB is lost.
5. **Manifest content:**
   - Original path as raw bytes (`OsString`) plus a display form; parent chain with modes.
   - Owner/uid/gid, mode, `st_flags`/attributes, timestamps (btime/mtime/atime; ctime changes on rename), xattr names and sizes (macOS `com.apple.quarantine`, provenance, Finder info), ACL presence, file ID, volume UUID.
   - Policy decision, evidence IDs and Jev evidence references.
   - Because the move is a rename, xattrs, ACLs and resource forks travel with the inode. The manifest is for verification, display and conflict handling, not reconstruction.
6. **Restore:**
   - Rename back with no-replace semantics.
   - If the parent is missing, recreate the chain with the recorded modes.
   - If the original path is occupied, never overwrite. Offer "restore as `name (restored).ext`", "swap" (the current occupant goes to quarantine), or cancel.
   - Journal `RESTORE_INTENT → RESTORED`.
7. **Retention and finalization:**
   - Quarantined bytes stay on the same volume, so **nothing is reclaimed until finalization**.
   - Default retention is N days (suggested 7–30 depending on risk class).
   - Early finalization only with explicit consent (for example, under disk-pressure prompts).
   - Finalize as a journaled `PURGE_INTENT → PURGED`, or optionally "release to OS trash" for one more safety net (doc 04).
8. **Crash recovery:**
   - On start, scan the journal for non-terminal states.
   - `INTENT`: look up the file ID at `src` and at `dst`. Roll forward if it is at `dst`, mark aborted if it is at `src`, and flag for user attention if it is at neither.
   - `PURGE_INTENT`: complete the purge.
   - The journal is idempotent per `op_id`.
9. **APFS `clonefile` as a cheap safety copy:**
   - When Lumen must modify a file in place (for example, removing a login item from a plist), first `clonefile(src, backup, CLONE_NOFOLLOW | CLONE_ACL)`. That costs O(1) and no extra space until the file diverges.
   - Same volume only (`EXDEV` otherwise). "Cloning directories … is strongly discouraged. Use copyfile(3)." Setuid/setgid are cleared, and ownership follows creation rules unless `CLONE_NOOWNERCOPY` is used ([clonefile(2)](https://keith.github.io/xcode-man-pages/clonefile.2.html)).
   - Cross-platform: `reflink-copy` 0.1.30 wraps clonefile, FICLONE and ReFS block cloning (unverified per-OS coverage).

**Tool-driven cleanups are a different kind of operation.** `docker system prune`, `cargo clean`, `brew cleanup` and similar commands cannot be quarantined: the tool deletes the files itself. Model them as `RegenerableIrreversible` actions with:
- a dry-run preview where one exists (`brew cleanup -n`, `simctl runtime delete … --dry-run`, `claude purge` dry run, `docker system df`);
- a regeneration-cost estimate;
- explicit confirmation;
- a post-check of the reclaimed bytes, by re-measuring.

### 6. Duplicate detection

**Staged pipeline** (fclones' documented order: size → remove same-inode → prefix hash → suffix hash → full hash; [fclones](https://github.com/pkolaczk/fclones), [docs.rs](https://docs.rs/fclones/latest/fclones/config/struct.GroupConfig.html)):

1. **Candidate filter.**
   - Regular files ≥ min size (UI default about 1 MiB; configurable).
   - Exclude placeholders and dataless files.
   - Exclude package/bundle internals (`.app`, `.photoslibrary`, `.git/objects`, `node_modules`, VM disks) and tool-managed stores (pnpm/uv/Cargo/Gradle), which are deduplicated by design.
2. **Group by size.**
3. **Collapse identical identities.**
   - Same `(volume, file_id)` = hard links, which are already one copy.
   - On macOS, same `ATTR_CMNEXT_CLONEID` = pure clones, which are already deduplicated, so deleting one frees nothing.
   - fclones by default does not treat hard or symbolic links as duplicates. It offers `--isolate` and `--match-links` to change that.
4. **Head hash.** 4 KiB on SSD, 16 KiB on HDD (fclones defaults), using XXH3-128 (`xxhash-rust` 0.8.19 or `twox-hash` 2.1.5).
5. **Tail hash.** Same size and hash.
6. **Full content hash.** BLAKE3 (`blake3` 1.8.7), streamed. Use `update_rayon` or mmap for very large files on SSD; stay single-threaded on HDD. The 256-bit output becomes Lumen's **content ID** `b3:<hex>`.
7. **Optional byte-for-byte compare** right before a destructive action on the keeper/victim pair, when cheap (same volume, < some size). This defends against changes between hashing and action. Combine it with the TOCTOU fingerprint re-check.

**Hash choice rationale**
- xxHash: "not a cryptographic hash function … Do not use it for … any other purpose that requires resistance to attacks". XXH3 runs at about 59 GB/s with AVX2 ([xxHash](https://github.com/Cyan4973/xxHash)). It is good enough for pre-filters, but files can be attacker-supplied (Downloads), so a deliberately crafted XXH3 collision must not be able to cause a deletion. The final ID must therefore be cryptographic.
- BLAKE3: "much faster than MD5, SHA-1, SHA-2, SHA-3, and BLAKE2", secure against length extension, 256-bit default output, SIMD (SSE2…AVX-512, NEON), Rayon multithreading ([BLAKE3](https://github.com/BLAKE3-team/BLAKE3)). On cold storage, I/O bound dominates, which matches fclones' note that you "won't see much difference unless you're reading from a fast SSD or if file data is cached".

**Hash cache.** Key it on `(volume_uuid, file_id, size, mtime_ns, ctime_ns)`, as fclones' `--cache` does: "Cached hashes are not invalidated by file moves because files are identified by their internal identifiers" ([fclones](https://github.com/pkolaczk/fclones)). Store partial hashes too, so the head/tail stages are free on rescans.

**Acting on duplicates**
- Choose a keeper by rule: inside a managed library (Photos, Music) > user document folders > Downloads/Desktop; older btime > newer; shorter path depth. Quarantine the rest.
- On APFS, offer **dedupe-by-clone** as an alternative. Replace the duplicate with a `clonefile` of the keeper, which keeps both paths and frees the space. Restore metadata from the manifest. This is reversible only in a weak sense, because edits then diverge cleanly. fclones does the same with `dedupe` (reflink; not on Windows).
- Benchmarks (fclones README, SSD, 1.46M files): fclones 34.6 s vs jdupes 5:01 and fdupes 5:46. This is old (fclones 0.12) but shows how much the staged design gains.

**Near-duplicate images**
- `image_hasher` 3.1.1 offers `HashAlg::{Mean, Gradient, VertGradient, DoubleGradient, Blockhash}`, a `HasherConfig` builder (hash size and DCT preprocessing options: unverified, not shown in the page consulted), and `ImageHash::dist` (Hamming) ([docs.rs](https://docs.rs/image_hasher/latest/image_hasher/)).
- czkawka 12.0.2 (2026-09-09; Krokiet is now the primary GUI) uses image_hasher for similar images, supports BLAKE3/CRC32/XXH3 for duplicates, caches hashes, and lets users choose trash or delete ([czkawka](https://github.com/qarmin/czkawka)).
- Thresholds are not documented upstream. Calibrate on a labelled local corpus.
- Use near-duplicates only to *group for review* (burst shots, resized exports). Never quarantine without the user picking.
- Hash decoded thumbnails, locally. Images never leave the device.

### 7. Developer-artifact knowledge base

Principle: **if the tool has a cleanup command, the KB entry's `action` is that command, run as the user. Deleting files directly is a fallback for tools without one.** Versions were verified on 2026-10-05 from crates.io, the npm registry and GitHub releases.

| Artifact | Location (default) | Safe to remove? | Regeneration cost | Official / recommended cleanup | Lumen default |
|---|---|---|---|---|---|
| Xcode DerivedData | `~/Library/Developer/Xcode/DerivedData/<Project-hash>` | Yes: build products, indexes, logs; regenerated on next build | Full rebuild + re-index per project (minutes–hours) | No CLI for the whole dir. Per project: Product › Clean Build Folder / `xcodebuild clean`. Delete per-project subfolders while Xcode is not building. (Apple first-party statement not found; community consensus ([example](https://www.cluttered.dev/blog/xcode-derived-data))) | QUARANTINE-eligible for projects inactive > N days |
| Xcode Archives | `~/Library/Developer/Xcode/Archives/<date>/*.xcarchive` | **No** (not regenerable): contains the dSYMs needed to symbolicate crashes of shipped builds and the submission payload | Irreproducible | Xcode Organizer › Archives (delete by hand) | REVIEW only; never auto |
| iOS/watchOS DeviceSupport | `~/Library/Developer/Xcode/iOS DeviceSupport/<os-build>` | Yes: debug symbols copied from devices; re-copied when that OS version's device next connects | Minutes on reconnect | None (Finder/manual). Community sources only (unverified official) | QUARANTINE versions not seen in > N days |
| CoreSimulator devices | `~/Library/Developer/CoreSimulator/Devices/<UDID>` | Unavailable devices: yes. Available devices may hold test app data | Recreate the simulator plus app state | `xcrun simctl delete unavailable`; `xcrun simctl delete <udid>` (shut down first) | Unavailable: tool command. Others: REVIEW |
| Simulator runtimes | System-managed (disk images mounted by CoreSimulator; some under SIP-protected `/System/Library/AssetsV2` ([forum](https://developer.apple.com/forums/thread/812992))) | Yes if unused | Multi-GB re-download | `xcrun simctl runtime list -j` (JSON); `xcrun simctl runtime delete (<identifier>` or `all` or `--notUsedSinceDays <days>) [--dry-run/-n]`; Xcode › Settings › Components. Verified first-party from `xcrun simctl runtime` help on Xcode 26.1 (17B55): delete works on the "secure storage area", and if the runtime is a disk image "any booted simulators are shutdown and the disk is unmounted first", so warn before running it ([community guide](https://mehmetbaykar.com/posts/how-to-remove-xcode-simulator-runtimes-using-terminal/)) | Tool command only; **never delete image files** |
| Docker Desktop | macOS `~/Library/Containers/com.docker.docker/Data/vms/0/data/Docker.raw`; Windows WSL2 `docker_data.vhdx` | Through Docker only. "Do not move the file directly in Finder" ([FAQ](https://docs.docker.com/desktop/troubleshoot-and-support/faqs/macfaqs/)) | Re-pull images, rebuild cache; volumes may be **irreplaceable data** | `docker system df [-v] [--format json]` to measure ([ref](https://docs.docker.com/reference/cli/docker/system/df/)); `docker system prune` removes stopped containers, unused networks, dangling images, unused build cache; `-a` all unused images; `--volumes` anonymous volumes only; `--filter until=24h` ([ref](https://docs.docker.com/reference/cli/docker/system/prune/)). The raw file is sparse: report allocated, not logical. Never suggest shrinking "Disk image size" in Docker settings as a cleanup: Docker says reducing it "deletes the current image file", taking all containers and images (and volumes stored in it) with it | Measure with `df`; offer prune without `--volumes` by default; named volumes = REVIEW |
| `node_modules` | `<project>/node_modules` | Yes **if** a lockfile exists (`package-lock.json`, `pnpm-lock.yaml`, `yarn.lock`) | `npm ci` / `pnpm install` (network + minutes) | Delete the dir; reinstall from the lockfile | QUARANTINE if lockfile present + project inactive; REVIEW otherwise; reclaim counted by unique inode (pnpm hard links) |
| npm cache | `~/.npm/_cacache`; Windows `%LocalAppData%\npm-cache` | Yes. "npm's cache is self-healing and resistant to data corruption" | Re-download | `npm cache verify` (GC + integrity) preferred; `npm cache clean --force`; npx: `npm cache npx ls/rm/info` ([docs](https://docs.npmjs.com/cli/v12/commands/npm-cache), npm 12 docs). npm 12.2.0 current | Tool command |
| pnpm store | `pnpm store path` | Prune is "not harmful and has no side effects on your projects" | Re-download of pruned packages only | `pnpm store prune` (removes unreferenced packages; avoid running it often) ([docs](https://pnpm.io/cli/store)). pnpm 12.9.1 current | Tool command; never delete the store directly |
| Yarn Berry cache | Global `~/.yarn/berry/cache` (`enableGlobalCache` defaults to `true` per the [yarnrc reference](https://yarnpkg.com/configuration/yarnrc): "Yarn will store the cache files into a folder located within `globalFolder`", default `${HOME}/.yarn/berry`); project `.yarn/cache` exists only when a project opts out (typically for zero-installs), so its presence is itself a signal of intent | Global: yes. Project cache **may be committed to git (zero-installs)** | Re-fetch | `yarn cache clean` (local), `--mirror` (global), `--all` ([docs](https://yarnpkg.com/cli/cache/clean)). Yarn 4.18.1 current | Global: tool command. Project cache tracked by git: never touch |
| Cargo registry/git caches | `~/.cargo/registry/{index,cache,src}`, `~/.cargo/git/{db,checkouts}` | Yes, and **auto-GC since Rust 1.88**: deletes network-downloaded files unused for 3 months and local ones unused for 1 month, at most daily; skipped when `--offline`/`--frozen` ([blog](https://blog.rust-lang.org/2025/06/26/Rust-1.88.0/)) | Re-download | Automatic. Tunable with `cache.auto-clean-frequency` (stable). Manual `cargo clean gc` still requires `-Zgc` on nightly ([unstable](https://doc.rust-lang.org/nightly/cargo/reference/unstable.html)). Never touch `~/.cargo/bin` | Report "self-managed"; act only on explicit request |
| Cargo `target/` | `<project>/target` (or `build-dir`/`CARGO_TARGET_DIR`) | Yes | Full rebuild (often 1–30+ min) | `cargo clean` in the workspace | QUARANTINE-eligible if inactive; prefer the tool command |
| Gradle caches | `~/.gradle/caches`, `~/.gradle/wrapper/dists` | Self-managed. Automatic cleanup every 24 h: downloaded 30 d, created 7 d, unused Gradle versions 30 d (snapshots 7 d), daemon logs 14 d, build cache 7 d. Configure via `init.d` (`Cleanup.DISABLED/ALWAYS`) ([Gradle 9.8.0](https://docs.gradle.org/current/userguide/directory_layout.html)) | Re-download | Automatic. Project: `./gradlew clean` (stop daemons with `./gradlew --stop` first) | Report "self-managed"; project `build/` QUARANTINE-eligible |
| CocoaPods cache | `~/Library/Caches/CocoaPods` (location unverified on the official page); spec repos `~/.cocoapods/repos` | Yes | Re-download | `pod cache list`, `pod cache clean --all` ([docs](https://guides.cocoapods.org/terminal/commands.html)) | Tool command |
| Homebrew | `brew --cache`; old kegs in the Cellar | Yes | Re-download | `brew cleanup` (default: older than 120 days, `HOMEBREW_CLEANUP_MAX_AGE_DAYS`), `-n/--dry-run`, `--prune=all`, `-s/--scrub` ([manpage](https://docs.brew.sh/Manpage)). Partly **self-managed**: unless `HOMEBREW_NO_INSTALL_CLEANUP` is set, `brew install`/`upgrade`/`reinstall` run cleanup for the touched packages and, every `HOMEBREW_CLEANUP_PERIODIC_FULL_DAYS` (default 30), for all packages. Homebrew 7.0.8 current | Tool command with dry-run preview; show as partly self-managed |
| Next.js `.next` | `<project>/.next` (or `distDir`, which "should not leave your project directory") ([docs](https://nextjs.org/docs/app/api-reference/config/next-config-js/distDir)) | Yes when no `next dev`/`next build` is running | Rebuild; `.next/cache` loses incremental speedups | Delete the dir (no official clean command found). Next 16.3.8 current | QUARANTINE-eligible if no process holds it |
| Android build outputs | `<module>/build`, `<project>/.gradle` | Yes | Rebuild | `./gradlew clean` | As Gradle |
| Android SDK system images / platforms | `$ANDROID_HOME/system-images/android-<API>/<variant>/<abi>` | Yes if no AVD uses it | GB-scale re-download | `sdkmanager --uninstall "system-images;android-36;google_apis"` (`sdkmanager` is now marked **deprecated** in favour of the Android CLI `android sdk` install/list/update/remove subcommands) ([docs](https://developer.android.com/tools/sdkmanager)); prefer `android sdk remove` when the Android CLI is installed, else fall back to `sdkmanager` ([docs](https://developer.android.com/tools/agents/android-cli)) | Tool command; check AVD references first |
| Android AVDs | `~/.android/avd/<name>.avd` | Holds emulator user data | Recreate + app state | `avdmanager delete avd -n <name>` (**deprecated** in favour of `android emulator remove`; that command is disabled on Windows) ([docs](https://developer.android.com/tools/avdmanager)) | REVIEW |
| JetBrains IDEs | System (caches) dir: macOS `~/Library/Caches/JetBrains/<product><ver>`, Windows `%LOCALAPPDATA%\JetBrains\…`, Linux `~/.cache/JetBrains/…` | System dir: mostly. JetBrains says it "contains caches **and local history files**", so deleting it also loses the IDE's Local History (user data, not regenerable). **Config/plugins: no** | Re-index; Local History lost | File › Invalidate Caches; Help › Delete Leftover IDE Directories; when a new major version is installed, the IDE "automatically deletes the caches and logs directories for older versions … that have not been updated in the last 180 days" (config and plugins dirs persist) ([docs](https://www.jetbrains.com/help/idea/directories-used-by-the-ide-to-store-settings-caches-plugins-and-logs.html)) | Old-version system dirs QUARANTINE (exclude or warn about `LocalHistory`); current version: suggest Invalidate Caches, never delete directly |
| VS Code | macOS `~/Library/Application Support/Code/{Cache,CachedData,CachedExtensionVSIXs,logs}`; Windows `%APPDATA%\Code\…`; Linux `~/.config/Code/…` | Caches: yes, with VS Code closed. `User/` (settings, Local History) and `workspaceStorage` (extension state) are **not** pure cache | Slower first start | No official command (community sources only, unverified first-party) | Caches QUARANTINE when the app is not running; `User/`, `workspaceStorage` REVIEW |
| pip cache | `pip cache dir` | Yes | Re-download/rebuild wheels | `pip cache purge` / `pip cache remove <pattern>` / `pip cache info` ([pip 26.2.1](https://pip.pypa.io/en/stable/cli/pip_cache/)) | Tool command |
| uv cache | `$XDG_CACHE_HOME/uv` or `~/.cache/uv`; Windows `%LOCALAPPDATA%\uv\cache` | Through uv only: "it's never safe to modify the cache directly" | Re-download | `uv cache prune` ("safe to run periodically"; it now also removes "all centralized project environments", which "are recreated as needed"), `uv cache prune --ci`, `uv cache clean [pkg]` ([docs](https://docs.astral.sh/uv/concepts/cache/)). uv's own reclaimed-space figure is an estimate unless the `cache-physical-space` preview feature is enabled, so Lumen should measure before/after itself. uv 0.12.23 current | Tool command; reclaim estimate adjusted for hard links into venvs |
| Go module + build cache | `GOMODCACHE` (default `$GOPATH/pkg/mod`), `GOCACHE` | Yes. Module-cache files are **read-only**, so `rm -rf` fails without `-modcacherw` | Re-download / rebuild | `go clean -modcache`, `go clean -cache` ([Go ref](https://go.dev/ref/mod)) | Tool command |
| Claude Code | `~/.claude/` (projects transcripts, `file-history/`, `paste-cache/`, `shell-snapshots/`, `debug/`…) | Transcripts, file history and paste cache are **user data** (resume/rewind; contain file contents). Caches and lock files are safe. **Keep** `.credentials.json`, `agent-memory/`, `jobs/`, `daemon/`, settings, CLAUDE.md, skills | Loss of history and checkpoints | Auto-delete after `cleanupPeriodDays` (default 30, min 1; `0` is a validation error). Caveats from the docs: transcripts of Claude Desktop/Cowork sessions are **not** age-limited unless `desktopSessionCleanupPeriodDays` is set (v2.1.248+), and the sweep is skipped or paused in some configurations (e.g. `--bare`, unreadable settings), so "self-managed" is not guaranteed. `claude purge <path> --dry-run` previews; `claude purge <path>` deletes transcripts **and auto memory**, per-session `tasks/`/`debug/`/`file-history/`, matching `history.jsonl` lines and the `~/.claude.json` project entry; `--all` deletes `history.jsonl` outright. The command was `claude project purge` before v2.1.288, so the KB entry needs a `version_range` ([docs](https://code.claude.com/docs/en/claude-directory)) | Report "self-managed"; REVIEW only; treat as privacy-sensitive; never run `claude purge` without a per-project dry-run preview, because auto memory is not regenerable |
| Other AI coding tools (Cursor, Copilot, Codex, Windsurf…) | App-specific | Unknown | Unknown | Not researched in this pass (unverified) | REVIEW until KB entries are verified |

KB entry schema suggestion: `{id, tool, version_range, platforms, path_patterns (with env-var expansion: GOMODCACHE, CARGO_HOME, GRADLE_USER_HOME, UV_CACHE_DIR, ANDROID_HOME, npm_config_cache), detection (marker files, e.g. lockfile, Cargo.toml), safety_class (regenerable | regenerable-with-state | irreplaceable), regeneration_cost (network_bytes_est, cpu_minutes_est), preferred_action (tool_command{argv, dry_run_argv, requires_not_running}), fallback_action (quarantine|review), self_managed (bool + policy), source_urls, last_verified}`. The tool's dry-run output becomes Jev evidence and the UI preview. Lumen never parses it to decide anything.

## Implications for Lumen

1. **`lumen-scan` crate with a `DirEnumerator` port and three native adapters** (macOS `getattrlistbulk`, Linux `getdents64`+`statx`, Windows `NtQueryDirectoryFileEx`/MFT per doc 04), plus a portable `std::fs` fallback used in tests and on Android/iOS.
   - *Rationale:* syscall batching is the dominant cost. The fallback keeps behaviour testable.
   - *Rejected:* building on `jwalk`/`ignore` directly. They use `std::fs::Metadata`, so they cannot return PRIVATESIZE, CLONEID or dataless flags, and jwalk had a three-year release gap. Borrow their scheduling ideas instead.
   - *Rejected:* `tokio::fs`, which costs one blocking-pool hop per syscall.
2. **Breadth-first directory tasks on a work-stealing blocking pool** (rayon 1.12 or crossbeam-deque), with per-volume semaphores, priority bands, bounded channels, batched persistence, and a stall watchdog. One directory is open at a time per task (the macOS SMB bug).
3. **A `SizeFacts` record per file:** `{logical, allocated, private?, clone_id?, file_id, nlink, flags{sparse, compressed, dataless, placeholder, may_share_blocks}}`.
   - The policy and UI use `reclaimable_now` = Σ private (macOS) or allocated over unique file IDs whose links are all in the selection.
   - Show "frees space after retention" for quarantine and "frees nothing (clone/hard link/snapshot)" where it applies.
   - *Rejected:* a single "size" number. It overstates savings for pnpm, uv, APFS clones, Docker.raw and snapshots, which erodes trust.
4. **Never hydrate.** The scanner and the duplicate hasher must not open placeholder or dataless files. On macOS, also set `IOPOL_MATERIALIZE_DATALESS_FILES_OFF` process-wide via `setiopolicy_np` as a backstop. On Windows, also set placeholder compatibility mode (doc 04). Mark these files as `cloud` evidence and add a policy rule: KEEP unless the user explicitly asks for an evict action through the provider.
5. **A `ChangeFeed` port with native adapters:**
   - FSEvents per-device stream (persist event ID + device UUID);
   - USN in the elevated helper;
   - fanotify when privileged (`CAP_SYS_ADMIN`) and the kernel is ≥ 5.9 (needed for `FAN_REPORT_DFID_NAME`), else inotify for hot roots only (unprivileged fanotify, 5.13+, can only mark inodes);
   - polling via scheduled rescans otherwise.
   - Always start the feed before a full scan. Schedule a periodic full sweep (for example, weekly or on volume UUID change).
   - Use the `notify` 8.2 crate only for live UI refresh of focused folders. Do not adopt 9.0 until it is stable.
6. **Snapshot store:** SQLite (rusqlite 0.40.2) with an arena-style `entries(dir_id, name_id, …)` table, per-directory aggregates and an XXH3 digest. Partial hashes and content IDs go in a `hash_cache` keyed by `(volume_uuid, file_id, size, mtime, ctime)`. Checkpoints make scans resumable and are labelled "approximate until reconciled".
7. **Quarantine = journaled same-volume rename with verification and a sidecar manifest.** A per-volume store sits on each writable volume. On `EXDEV`, Lumen falls back to the OS trash on that volume, else to REVIEW, and never silently copies. Restore uses no-replace renames and conflict prompts.
   - *Rejected:* the OS trash as the primary quarantine. It has no restore API on Windows, the `trash` crate cannot list or restore on macOS, the user can empty it at any time, and it frees nothing anyway.
   - When the OS trash is used as a release valve on macOS, call `FileManager.trashItem(at:resultingItemURL:)` directly and persist the resulting URL. Do not use the `trash` crate there: its default `Finder` method runs `osascript` (Automation TCC prompt) and neither method returns the trashed location.
   - Before any rename, check `VOL_CAP_INT_RENAME_EXCL` on the source volume. Where no-replace rename is unsupported, the item goes to REVIEW rather than an unguarded rename.
   - *Rejected:* copy-to-app-container. It doubles I/O and space and loses inode metadata.
8. **Tool commands are first-class "actions" in the KB.** Run them as the user, never elevated. Capture the dry run as evidence, record before/after `df`, and mark them irreversible in the plan UI. Self-managed caches (Cargo, Gradle, JetBrains, Claude Code; Homebrew partly) are shown as informational, with tuning hints, not as cleanup targets. "Self-managed" must be a per-machine observation, not an assumption: the Claude Code sweep can be paused and does not cover Desktop/Cowork transcripts by default, Gradle only cleans for versions that run, and Cargo GC skips `--offline`/`--frozen` use.
9. **Duplicate engine:**
   - stages size → identity collapse → XXH3-128 head/tail → BLAKE3 full;
   - optional byte compare before action;
   - keeper rules;
   - APFS clone-dedupe as an alternative action;
   - near-duplicate images as REVIEW groups only.
   - The `b3:` content ID doubles as the evidence-graph node ID for file content (no content leaves the device; hashes stay local by default too, since hashes of known files can identify content).
10. **Benchmark harness:** generated fixture trees plus `hyperfine` warm runs (and cold runs on Linux CI). Regress on entries/s, peak RSS and syscall counts. Compare against dua, dust and diskus on the same fixtures.

## Risks and open questions

- **macOS TCC and Full Disk Access.** The scan only covers protected areas such as Mail, Messages and other apps' containers when FDA is granted. Coverage gaps must be visible. How this interacts with the sandboxing decision belongs in doc 01/02.
- **`getattrlistbulk` corner cases:** SMB looping (mitigated by breadth-first), FUSE/macFUSE and File Provider volumes, and which `ATTR_CMNEXT_*` attributes are supported on non-APFS volumes (HFS+, exFAT). Query `VOL_CAP` / `ATTR_CMN_RETURNED_ATTRS` per volume and treat missing attributes as unknown, never zero.
- **Partial clones:** PRIVATESIZE covers them on APFS. On btrfs/XFS (FIEMAP shared extents) and ReFS, Lumen needs equivalent per-file "unique bytes" methods (unverified).
- **Snapshots:** local Time Machine snapshots can hold deleted blocks, so "reclaimed" may lag. Should Lumen surface `tmutil` snapshot thinning (unverified API surface)?
- **Same-volume quarantine on read-only or foreign volumes** (exFAT USB, network shares). Permissions or `.Trashes` behaviour may differ. These default to REVIEW until device-tested.
- **No-replace rename flags.** `renamex_np(RENAME_EXCL)` is now verified against the local macOS 26 man page (see §5). It is conditional on `VOL_CAP_INT_RENAME_EXCL`; behaviour on SMB, exFAT and File Provider volumes still needs device tests.
- **Linux fanotify** needs `CAP_SYS_ADMIN` for filesystem-wide marks. Lumen's Linux story (not in the stated platform list, but relevant to the Android kernel and to developers) may be "scheduled rescan + inotify on hot roots".
- **Developer KB gaps:**
  - No Apple first-party pages could be machine-read for DerivedData or DeviceSupport. `simctl runtime` is now verified against the CLI help of Xcode 26.1 (17B55); re-check its flags per Xcode release.
  - The CocoaPods cache path and VS Code cache semantics rest on community sources.
  - AI tools other than Claude Code are unresearched.
  - Resolved: Yarn's current yarnrc reference documents `enableGlobalCache` with default `true` (verified 2026-10-06).
- **Tool commands as subprocesses.** Lumen must resolve binaries safely (no `PATH` hijack: resolve against known install locations and verify signatures where possible). It must handle tool versions whose flags differ (for example, `cargo clean gc` stability changing), set timeouts, and parse localized output.
- **Hash privacy.** Content IDs of common files are fingerprintable. Keep them local and exclude them from any telemetry or sync by default.
- **Version churn.** `notify` 9.0 is in RCs, the Android SDK tools are moving to the new Android CLI, and avdmanager/sdkmanager are deprecated. Re-verify KB entries per release via `last_verified`.
- **Near-duplicate thresholds** for image_hasher are not documented. A labelled corpus and an evaluation like doc 08's are needed before they are shipped as evidence.

## Sources

- [notify — crates.io](https://crates.io/crates/notify) · [notify docs.rs (known problems)](https://docs.rs/notify/latest/notify/) · [notify 8.2.0 crate page](https://docs.rs/crate/notify/latest)
- [trash crate docs](https://docs.rs/trash/latest/trash/) · [trash::os_limited](https://docs.rs/trash/latest/trash/os_limited/index.html) · [trash-rs GitHub](https://github.com/ArturKovacs/trash)
- [crates.io API (versions for notify, trash, jwalk, ignore, rayon, tokio, blake3, xxhash-rust, twox-hash, image_hasher, fclones, czkawka_core, dua-cli, du-dust, diskus, fd-find, rusqlite, redb, reflink-copy)](https://crates.io/)
- [npm registry (pnpm, npm, @yarnpkg/cli-dist, next)](https://registry.npmjs.org/) · GitHub releases API for uv, Homebrew, czkawka, dust, dua-cli, gdu, diskus, fd, hyperfine, fclones
- [jwalk README](https://docs.rs/crate/jwalk/latest/source/README.md) · [ignore::WalkParallel](https://docs.rs/ignore/latest/ignore/struct.WalkParallel.html)
- [dust README](https://github.com/bootandy/dust) · [dua-cli README](https://github.com/Byron/dua-cli)
- [Maybe the Fastest Disk Usage Program on macOS (healeycodes)](https://healeycodes.com/maybe-the-fastest-disk-usage-program-on-macos)
- [Apple Developer Forums: getattrlistbulk lists same files over and over on macOS 15](https://developer.apple.com/forums/thread/766035)
- [getattrlist(2) man page](https://keith.github.io/xcode-man-pages/getattrlist.2.html) · [clonefile(2) man page](https://keith.github.io/xcode-man-pages/clonefile.2.html) · [chflags(2) man page](https://keith.github.io/xcode-man-pages/chflags.2.html)
- [Eclectic Light: APFS files and clones](https://eclecticlight.co/2024/03/20/apfs-files-and-clones/) · [Eclectic Light: macOS Sonoma has changed iCloud Drive radically](https://eclecticlight.co/2023/10/25/macos-sonoma-has-changed-icloud-drive-radically/)
- [FileManager.trashItem(at:resultingItemURL:)](https://developer.apple.com/documentation/foundation/filemanager/trashitem(at:resultingitemurl:))
- [volumeAvailableCapacityForImportantUsageKey](https://developer.apple.com/documentation/foundation/urlresourcekey/volumeavailablecapacityforimportantusagekey) · [Checking volume storage capacity](https://developer.apple.com/documentation/foundation/checking-volume-storage-capacity)
- [FSEvents Programming Guide: Using the File System Events API](https://developer.apple.com/library/archive/documentation/Darwin/Conceptual/FSEvents_ProgGuide/UsingtheFSEventsFramework/UsingtheFSEventsFramework.html) · [FSEventStreamCreate](https://developer.apple.com/documentation/coreservices/1443980-fseventstreamcreate)
- [Microsoft Learn: File Attribute Constants](https://learn.microsoft.com/en-us/windows/win32/fileio/file-attribute-constants) · [Microsoft Learn: Change Journals](https://learn.microsoft.com/en-us/windows/win32/fileio/change-journals)
- [inotify(7)](https://man7.org/linux/man-pages/man7/inotify.7.html) · [fanotify(7)](https://man7.org/linux/man-pages/man7/fanotify.7.html) · [statx(2)](https://man7.org/linux/man-pages/man2/statx.2.html)
- [freedesktop.org Trash specification 1.0](https://specifications.freedesktop.org/trash/latest/)
- [fclones README](https://github.com/pkolaczk/fclones) · [fclones GroupConfig](https://docs.rs/fclones/latest/fclones/config/struct.GroupConfig.html)
- [czkawka README](https://github.com/qarmin/czkawka) · [image_hasher docs](https://docs.rs/image_hasher/latest/image_hasher/)
- [xxHash README](https://github.com/Cyan4973/xxHash) · [BLAKE3 README](https://github.com/BLAKE3-team/BLAKE3)
- [Announcing Rust 1.88.0 (cargo automatic GC)](https://blog.rust-lang.org/2025/06/26/Rust-1.88.0/) · [Cargo unstable features: gc](https://doc.rust-lang.org/nightly/cargo/reference/unstable.html)
- [Docker Desktop for Mac FAQ (Docker.raw)](https://docs.docker.com/desktop/troubleshoot-and-support/faqs/macfaqs/) · [docker system prune](https://docs.docker.com/reference/cli/docker/system/prune/) · [docker system df](https://docs.docker.com/reference/cli/docker/system/df/)
- [Gradle user guide: directory layout & cache cleanup (9.8.0)](https://docs.gradle.org/current/userguide/directory_layout.html)
- [pnpm store](https://pnpm.io/cli/store) · [npm cache (v12)](https://docs.npmjs.com/cli/v12/commands/npm-cache) · [yarn cache clean](https://yarnpkg.com/cli/cache/clean)
- [uv: caching](https://docs.astral.sh/uv/concepts/cache/) · [pip cache](https://pip.pypa.io/en/stable/cli/pip_cache/) · [Go modules reference](https://go.dev/ref/mod)
- [Homebrew manpage (brew cleanup)](https://docs.brew.sh/Manpage) · [CocoaPods command-line reference](https://guides.cocoapods.org/terminal/commands.html)
- [Next.js distDir](https://nextjs.org/docs/app/api-reference/config/next-config-js/distDir)
- [Android sdkmanager](https://developer.android.com/tools/sdkmanager) · [Android avdmanager](https://developer.android.com/tools/avdmanager) · [Android CLI](https://developer.android.com/tools/agents/android-cli)
- [JetBrains: directories used by the IDE](https://www.jetbrains.com/help/idea/directories-used-by-the-ide-to-store-settings-caches-plugins-and-logs.html)
- [Claude Code: Explore the .claude directory](https://code.claude.com/docs/en/claude-directory)
- [Apple Developer Forums: orphaned simulator runtime in /System/Library/AssetsV2](https://developer.apple.com/forums/thread/812992)
- Added during verification (2026-10-06): [fanotify_init(2)](https://man7.org/linux/man-pages/man2/fanotify_init.2.html) · [Yarn yarnrc reference (enableGlobalCache)](https://yarnpkg.com/configuration/yarnrc) · [Apple: Describing use of required reason API](https://developer.apple.com/documentation/bundleresources/describing-use-of-required-reason-api) · [npm cache (v12)](https://docs.npmjs.com/cli/v12/commands/npm-cache) · `trash` 5.2.9 source (`src/lib.rs`, `src/macos/mod.rs`) and `fclones` 0.35.0 source (`src/config.rs`) from the crates.io download endpoint · local macOS 26.6.2 man pages `rename(2)`/`renamex_np`, `getattrlist(2)`, `chflags(2)`, `clonefile(2)`, `getiopolicy_np(3)` · `xcrun simctl runtime` and `simctl help delete` output on Xcode 26.1 (17B55) · `brew cleanup --help` and the [Homebrew manpage](https://docs.brew.sh/Manpage) (`HOMEBREW_CLEANUP_PERIODIC_FULL_DAYS`, `HOMEBREW_NO_INSTALL_CLEANUP`)
- Community (non-authoritative, used where first-party docs were not machine-readable): [Delete Xcode simulators and runtimes with xcrun simctl](https://mehmetbaykar.com/posts/how-to-remove-xcode-simulator-runtimes-using-terminal/) · [Xcode DerivedData guide (cluttered.dev)](https://www.cluttered.dev/blog/xcode-derived-data) · [VS Code cache cleanup (deepclean.app)](https://deepclean.app/blog/vscode-disk-space-mac)

## Verification log

Adversarial fact-check performed 2026-10-06. Local first-party checks were run on macOS 26.6.2 (25G83) with Xcode 26.1 (17B55).

| # | Claim | Verdict | Source |
|---|---|---|---|
| 1 | Crate versions: notify 8.2.0 stable (2025-08-03) / 9.0.0-rc.5 (2026-08-30), notify-debouncer-full 0.7.0 / 0.8.0-rc.2, trash 5.2.9, jwalk 0.9.0, ignore 0.4.33, rayon 1.12.0, tokio 1.53.2, blake3 1.8.7, xxhash-rust 0.8.19, twox-hash 2.1.5, image_hasher 3.1.1, fclones 0.35.0, rusqlite 0.40.2, redb 4.3.0, reflink-copy 0.1.30, dua-cli 2.45.1, du-dust 1.2.6, diskus 0.9.0, fd-find 10.5.0, czkawka_core 12.0.2 | Confirmed | crates.io API (`max_stable_version`, version list) |
| 2 | Tool versions: pnpm 12.9.1, npm 12.2.0, Yarn 4.18.1, Next 16.3.8, uv 0.12.23, Homebrew 7.0.8, hyperfine v1.21.0, gdu v5.38.0, czkawka 12.0.2 | Confirmed | npm registry `/latest`; GitHub releases API |
| 3 | `trash` crate `os_limited` (list/restore_all/purge_all) unavailable on macOS | Confirmed, and **extended**: the default macOS `DeleteMethod::Finder` uses `osascript` (Automation prompt), and neither method returns the trashed URL | `trash` 5.2.9 source `src/lib.rs` L347–350, `src/macos/mod.rs` |
| 4 | macOS no-replace rename flag `renamex_np(RENAME_EXCL)` (previously unverified) | Confirmed, with a caveat added: it depends on `VOL_CAP_INT_RENAME_EXCL`. Added `RENAME_NOFOLLOW_ANY` / `RENAME_RESOLVE_BENEATH` | Local `rename(2)` man page |
| 5 | `ATTR_CMNEXT_PRIVATESIZE`, `CLONEID`, `EF_MAY_SHARE_BLOCKS`, `EF_IS_SPARSE` semantics | Confirmed; added omissions `CLONE_REFCNT`, `EF_SHARES_ALL_BLOCKS`, `EF_IS_PURGEABLE`, `EF_IS_SYNC_ROOT`, `EF_NO_XATTRS`, `NOFIRMLINKPATH`, `VOL_CAP_FMT_CLONE_MAPPING` | Local `getattrlist(2)` man page |
| 6 | `SF_DATALESS` semantics; `UF_COMPRESSED` (previously unverified) | Confirmed; added the `IOPOL_MATERIALIZE_DATALESS_FILES_OFF` backstop | Local `chflags(2)`, `<sys/stat.h>`, `getiopolicy_np(3)` |
| 7 | `clonefile` quotes ("strongly discouraged" for directories, setuid/setgid cleared, `CLONE_NOOWNERCOPY`, `CLONE_ACL`) | Confirmed | Local `clonefile(2)` |
| 8 | "Apple DTS confirmed" `getattrlistbulk` loops forever on SMB in macOS 15; 32 KB buffer | **Corrected**: the bug is user-reported. DTS suggested the SMB server or an ES client might be the cause and did not confirm a macOS bug. The breadth-first and 32 KB advice is confirmed. "15 KB for ATTR_CMN_NAME" marked unverified | [Apple forums 766035](https://developer.apple.com/forums/thread/766035) |
| 9 | fanotify `FAN_REPORT_DFID_NAME` create/delete/move "since 5.1" | **Corrected**: `FAN_REPORT_FID` dates from 5.1, but `FAN_REPORT_DIR_FID`/`FAN_REPORT_NAME` (DFID_NAME) only from 5.9. Unprivileged fanotify (5.13+) can mark inodes only | [fanotify_init(2)](https://man7.org/linux/man-pages/man2/fanotify_init.2.html) |
| 10 | FSEvents guidance quotes (start before scan, snapshot, full sweep, per-disk streams, MustScanSubDirs, Kernel/UserDropped, UUID) | Confirmed (exact quotes) | [FSEvents Programming Guide](https://developer.apple.com/library/archive/documentation/Darwin/Conceptual/FSEvents_ProgGuide/UsingtheFSEventsFramework/UsingtheFSEventsFramework.html) |
| 11 | notify known problems; no FSEvents replay or USN | Confirmed | [docs.rs notify 8.2.0](https://docs.rs/notify/latest/notify/) |
| 12 | dumac figures (128 KB buffer, 409,500 files, 6.39x vs du, 2.58x vs diskus, ~91% syscalls, 64-permit semaphore, 128 shards ≈1.5%, no reliable macOS cache flush) | Confirmed | [healeycodes](https://healeycodes.com/maybe-the-fastest-disk-usage-program-on-macos) |
| 13 | Cargo auto-GC since 1.88 (3 months network / 1 month local; skipped offline/frozen) | Confirmed | [Rust 1.88.0 blog](https://blog.rust-lang.org/2025/06/26/Rust-1.88.0/) |
| 14 | Gradle 9.8.0 retention (30/7/30/7/14/7 days) and 24 h cadence | Confirmed; clarified that cleanup runs in the background when the daemon stops and is configured only via `init.d` scripts | [Gradle directory layout](https://docs.gradle.org/current/userguide/directory_layout.html) |
| 15 | JetBrains removes old-version caches after 180 days; "system dir: yes" safe | **Corrected**: the system dir also holds **Local History** (user data). Auto-removal covers caches and logs of older versions, triggered when a new major version is installed | [JetBrains directories](https://www.jetbrains.com/help/idea/directories-used-by-the-ide-to-store-settings-caches-plugins-and-logs.html) |
| 16 | Claude Code `cleanupPeriodDays` default 30 / min 1; `claude purge` with dry run | Confirmed; **added**: purge also deletes auto memory, Desktop/Cowork transcripts are not age-limited by default, the sweep can pause, and the command was renamed in v2.1.288 | [Claude Code .claude directory](https://code.claude.com/docs/en/claude-directory) |
| 17 | `docker system prune` defaults; `--volumes` = anonymous volumes only; Docker.raw sparse and "do not move in Finder" | Confirmed; added a warning that shrinking the disk image size deletes it | [prune ref](https://docs.docker.com/reference/cli/docker/system/prune/), [Mac FAQ](https://docs.docker.com/desktop/troubleshoot-and-support/faqs/macfaqs/) |
| 18 | fclones 4 KiB SSD / 16 KiB HDD prefix/suffix "per README" | Confirmed, but the source is the `src/config.rs` doc comments, not the README. Added that fclones' default hash is `metro` | fclones 0.35.0 source; [README](https://github.com/pkolaczk/fclones) |
| 19 | `avdmanager` deprecated → `android emulator remove` (disabled on Windows); `android sdk remove` | Confirmed; **added** that `sdkmanager` is also marked deprecated | [avdmanager](https://developer.android.com/tools/avdmanager), [sdkmanager](https://developer.android.com/tools/sdkmanager), [Android CLI](https://developer.android.com/tools/agents/android-cli) |
| 20 | `xcrun simctl runtime delete … --dry-run` (previously community-only) | Confirmed first-party via CLI help; added `--notUsedSinceDays`, `all`, `list -j`, and that it shuts down booted simulators | `xcrun simctl runtime` on Xcode 26.1 |
| 21 | uv "never safe to modify the cache directly"; `uv cache prune` safe periodically | Confirmed; added that prune also removes centralized project environments and that uv's reclaim figure is an estimate | [uv cache docs](https://docs.astral.sh/uv/concepts/cache/) |
| 22 | Yarn ≥4 `enableGlobalCache` default (open question) | Resolved: default `true` | [yarnrc reference](https://yarnpkg.com/configuration/yarnrc) |
| 23 | pnpm `store prune` "not harmful and has no side effects" | Confirmed | [pnpm store](https://pnpm.io/cli/store) |
| 24 | npm cache commands; docs link pointed at v11 | Confirmed; link updated to the v12 docs, added `npx info` | [npm cache v12](https://docs.npmjs.com/cli/v12/commands/npm-cache) |
| 25 | `brew cleanup` 120-day default, `-n`, `--prune=all`, `-s` | Confirmed; **added** periodic automatic cleanup (every 30 days on install/upgrade) | `brew cleanup --help`; [Homebrew manpage](https://docs.brew.sh/Manpage) |
| 26 | freedesktop Trash: info file with `O_EXCL` before move; `directorysizes` "optional" | Ordering confirmed; `directorysizes` corrected to SHOULD | [Trash spec](https://specifications.freedesktop.org/trash/latest/) |
| 27 | Required-reason privacy manifest for disk-space key "on iOS" | Confirmed scope (iOS/iPadOS/tvOS/visionOS/watchOS, not macOS; enforced since 2024-05-01). Disk-space reason codes not extracted (unverified) | [Apple required reason API](https://developer.apple.com/documentation/bundleresources/describing-use-of-required-reason-api) |
| — | Not re-checked in this pass and left as stated: xxHash/BLAKE3 README quotes, Go module cache read-only, Next.js `distDir`, Microsoft file-attribute constants, Eclectic Light APFS/iCloud articles, DerivedData/DeviceSupport (community-sourced), VS Code and CocoaPods paths | Unverified in this pass | — |
