# macOS Platform Integration for Lumen (Sizing, Traversal, Change Tracking, Inventory, Privilege, TCC, Reversible Cleanup)

> Researched: 2026-10-05 · Scope: Foundation/BSD/CoreServices/ServiceManagement/XPC/TCC APIs, APFS semantics, launchd/BTM inventory, privilege split and distribution for a safety-first storage analysis and reversible cleanup tool on macOS 13 Ventura through macOS 27 Golden Gate.

## Summary

- **Platform baseline has moved.** macOS 27 "Golden Gate" shipped 2026-09-14 and is at **27.0.1** (2026-09-28). It is **Apple-silicon only**; Intel Macs top out at macOS 26 Tahoe, whose latest update is the security-only 26.7 (2026-09-14; Apple security page 149042). Lumen should target **macOS 13+** (`SMAppService`, BTM) and test on 13, 14, 15, 26 (both architectures) and 27 ([Wikipedia: macOS Golden Gate](https://en.wikipedia.org/wiki/MacOS_Golden_Gate), [MacRumors 26.6](https://www.macrumors.com/2026/07/27/apple-releases-macos-tahoe-26-6/)).
- **On APFS, one file has three different sizes, and Lumen must report them separately.**
  - Logical size: `fileSize` / `ATTR_FILE_TOTALSIZE`.
  - Allocated size: `totalFileAllocatedSize` / `ATTR_FILE_ALLOCSIZE`.
  - Bytes actually freed by deletion: `ATTR_CMNEXT_PRIVATESIZE`, "bytes that are **not** trapped inside a clone or snapshot, and which would be freed immediately if the file were deleted".

  Clones (`clonefile`, `cp -c`, Finder Duplicate, pnpm/uv-style stores) and local Time Machine snapshots make naive `du`-style totals overstate reclaimable space, sometimes by 10× or more ([getattrlist(2)](https://keith.github.io/xcode-man-pages/getattrlist.2.html), [clonefile(2)](https://keith.github.io/xcode-man-pages/clonefile.2.html)).
- **`getattrlistbulk` is the only bulk API that returns clone and sharing metadata.** That metadata is `ATTR_CMNEXT_CLONEID`, `CLONE_REFCNT`, `EXT_FLAGS` (`EF_MAY_SHARE_BLOCKS`, `EF_SHARES_ALL_BLOCKS`, `EF_IS_SPARSE`, `EF_IS_PURGEABLE`, `EF_IS_SYNC_ROOT`) and `NOFIRMLINKPATH`. Apple DTS advises most apps not to build on `getattrlistbulk` because buffer handling is tricky, and recommends `FileManager.enumerator(at:includingPropertiesForKeys: [])` or `fts`. A Rust core that needs clone-aware accounting should still wrap `getattrlistbulk` behind one well-tested adapter ([Apple forums 760256](https://developer.apple.com/forums/thread/760256)).
- **Firmlinks show Data-volume content twice** (`/Users` ≡ `/System/Volumes/Data/Users`, and likewise `/Applications`, `/Library`, `/private`, `/usr/local`, `/opt`). Walk the Data volume once, de-duplicate by `(fsid, fileid)`, and present user-facing paths. Treat the sealed System volume (SSV, macOS 11+) as one opaque, read-only "macOS" figure ([Eclectic Light: firmlinks](https://eclecticlight.co/2023/07/22/how-macos-depends-on-firmlinks/), [Apple: SSV](https://support.apple.com/guide/security/signed-system-volume-security-secd698747c9/web)).
- **Free space has no single answer.** `statfs`/`df` report unconditional free space. `volumeAvailableCapacityForImportantUsage` and `...ForOpportunisticUsage` include purgeable space as estimated by the CacheDelete daemon (`deleted`). Finder and Storage settings are known to be inaccurate, and snapshot purgeability got *worse* in Tahoe ([Apple: Checking volume storage capacity](https://developer.apple.com/documentation/foundation/checking-volume-storage-capacity), [Eclectic Light 2026-08](https://eclecticlight.co/2026/08/24/arent-snapshots-purgeable/)).
- **FSEvents can drive incremental rescans, at directory granularity.** Event IDs persist across reboots. Use per-device streams (`FSEventStreamCreateRelativeToDevice`) and validate the volume with `FSEventsCopyUUIDForDevice`. `kFSEventStreamEventFlagMustScanSubDirs` (also set on User/KernelDropped) means "rescan this subtree". Apple says to treat the stream as **advisory** and still do periodic full sweeps ([FSEvents guide](https://developer.apple.com/library/archive/documentation/Darwin/Conceptual/FSEvents_ProgGuide/UsingtheFSEventsFramework/UsingtheFSEventsFramework.html)). The `notify` crate (stable **8.2.0**; 9.0.0-rc.5 on 2026-08-30) is fine for live UI watching. Persistent resumable indexing needs `sinceWhen` and device UUIDs, so use `fsevent-sys` 5.2.0 / `objc2-core-services` 0.3.2 for that.
- **TCC is the main "can we see it" constraint, and there is no API to check Full Disk Access.** Apple DTS: "do what you're really trying to do and then handle errors". Containers (macOS 14+), Group Containers (macOS 15+) and, per a 2026 third-party analysis, selected apps' `~/Library/Application Support` folders on macOS 27 now carry a separate write protection (`kTCCServiceSystemPolicyAppDataDetailed`) that, per that analysis, Full Disk Access does not override. Lumen must model **permission-denied as "unknown", never "empty"** ([Apple forums 114452](https://developer.apple.com/forums/thread/114452), [Eclectic Light: Containers](https://eclecticlight.co/2024/08/05/what-are-all-those-containers/), [Regula: macOS 27 AppData](https://wojciechregula.blog/post/golden-gate-appdata-protection/)).
- **Distribute with Developer ID, notarization and Hardened Runtime. Do not ship the full product through the Mac App Store.** Guideline 2.4.5 requires the sandbox and forbids root escalation and "code run automatically at startup or login without consent". A sandboxed app cannot inventory other apps' caches or LaunchAgents. Since Sequoia, users can no longer Control-click past Gatekeeper, so notarization is effectively mandatory ([App Review Guidelines](https://developer.apple.com/app-store/review/guidelines/), [Apple news: Sequoia runtime protection](https://developer.apple.com/news/?id=saqachfa)).
- **Privileged work goes through `SMAppService.daemon(plistName:)` plus an XPC Mach service.** `SMJobBless` has been deprecated since macOS 13: "Please use SMAppService instead". The daemon requires user approval in System Settings. Authenticate peers with `xpc_connection_set_peer_code_signing_requirement` (macOS 12+), `NSXPCConnection.setCodeSigningRequirement` (macOS 13+), or `XPCPeerRequirement.isFromSameTeam(andMatchesSigningIdentifier:)` (macOS 26+) ([SMJobBless](https://developer.apple.com/documentation/servicemanagement/smjobbless(_:_:_:_:)), [XPCPeerRequirement](https://developer.apple.com/documentation/xpc/xpcpeerrequirement)).
- **Login and background item inventory has no public cross-app API.**
  - Parse launchd plists in the five standard directories.
  - Optionally read BTM through `sudo sfltool dumpbtm`. Its backing file `/private/var/db/com.apple.backgroundtaskmanagement/BackgroundItems-v*.btm` is private and changes version (v4 on 13.0, v13 on 15.2, v16 on 26).
  - To disable an item, send the user to System Settings with `SMAppService.openSystemSettingsLoginItems()`. Never edit the BTM database ([Apple Platform Deployment: login items](https://support.apple.com/guide/deployment/manage-login-items-background-tasks-mac-depdca572563/web), [Eclectic Light 2026-02](https://eclecticlight.co/2026/02/20/in-the-background-identification/)).
- **Do not use the Trash as Lumen's quarantine.** `FileManager.trashItem(at:resultingItemURL:)` works, but Put Back can silently go missing for programmatically trashed items. Apple DTS acknowledges this as bug r. 23153124, "known for at least 10 years", and suspects a `.DS_Store` race with Finder. There is no restore API, and the user may have "empty after 30 days" enabled. Quarantine by **same-volume `rename`** into a Lumen-owned store, which keeps clones, xattrs, ACLs and file IDs. Use the Trash only as an optional user-visible step ([Apple forums 773997](https://developer.apple.com/forums/thread/773997)).
- **SIP and the SSV are hard KEEP boundaries.** Hard-exclude `/System`, `/usr` (except `/usr/local`), `/bin`, `/sbin`, `/var`, preinstalled apps, and anything flagged `SF_RESTRICTED` (0x00080000) or `SF_NOUNLINK`. Never suggest disabling SIP ([Apple: SIP](https://support.apple.com/en-us/102149), [xnu `sys/stat.h`](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/sys/stat.h)).
- **Running-process evidence:**
  - `NSWorkspace.runningApplications` returns GUI apps with bundle IDs, but not daemons.
  - `libproc` (`proc_listpids`, `proc_pidpath`, `proc_pidinfo(PROC_PIDLISTFDS)`) covers all processes. Without root, open-file inspection works only for the user's own processes.
  - Use `pkgutil` (not raw `/var/db/receipts`) for installer receipts. Apple: receipt locations "are subject to change".
- **Apple's Storage settings is the bar to beat on honesty, not on reach.** It treats "System Data" and "macOS" as unmanageable. It offers "Store in iCloud", "Optimize Storage" and "Empty Trash Automatically" (30 days). It delegates cache purging to CacheDelete. Lumen should add evidence-backed explanations and clone- and snapshot-aware "really freed" numbers, and should defer to macOS for purgeable categories ([Apple: Storage settings](https://support.apple.com/guide/mac-help/change-storage-settings-mchl3d437fbc/mac)).

## Findings

### 1. Platform baseline (as of 2026-10-05)

| Item | Current | Notes / source |
| --- | --- | --- |
| macOS 27 "Golden Gate" | **27.0.1** (2026-09-28); 27.0 released 2026-09-14; 27.2 beta 3 on 2026-10-05 | **Apple silicon only.** Last release with full Rosetta 2. AFP removed ([Wikipedia](https://en.wikipedia.org/wiki/MacOS_Golden_Gate), [UPenn ISC](https://isc.upenn.edu/news/macos-27-golden-gate-released-9142026)) |
| macOS 26 Tahoe | **26.7** (build 25G229, 2026-09-14, security-only; confirmed by Apple security-content page 149042 and MacRumors). 26.6 shipped 2026-07-27, 26.6.1 2026-08-06, 26.6.2 2026-08-17. macOS Sequoia **15.8** shipped the same day | Final line for Intel Macs ([Apple: security content of 26.7](https://support.apple.com/en-us/149042), [MacRumors 26.7 / 15.8](https://www.macrumors.com/2026/09/14/apple-releases-macos-tahoe-26-7/), [MacRumors 26.6](https://www.macrumors.com/2026/07/27/apple-releases-macos-tahoe-26-6/)) |
| Dev machine used for this research | macOS 26.6.2 (25G83) | local `sw_vers` |
| Minimum Lumen target | **macOS 13.0** | Needed for `SMAppService`, `NSXPCConnection.setCodeSigningRequirement` and BTM. Consistent with [01-desktop-architecture.md](./01-desktop-architecture.md) |

**Rust crates relevant to this document** (crates.io API, queried 2026-10-05):

| Crate | Max stable | Last updated | Use in Lumen |
| --- | --- | --- | --- |
| `notify` | **8.2.0** (newest pre-release 9.0.0-rc.5, 2026-08-30) | 2026-08-30 | Live UI watching only |
| `fsevent-sys` | 5.2.0 | 2025-11-17 | Raw FSEvents FFI for persistent incremental index |
| `objc2` | 0.6.5 | 2026-10-05 | ObjC runtime bindings |
| `objc2-foundation` / `objc2-service-management` / `objc2-core-services` | 0.3.2 | 2025-10-04 | `FileManager`, `SMAppService`, FSEvents/LaunchServices |
| `libproc` | 0.14.11 | 2025-10-01 | `proc_listpids`, `proc_pidpath`, fd lists |
| `sysinfo` | 0.39.6 | 2026-07-09 | Cross-platform process list (coarser) |
| `plist` | 1.10.1 | 2026-09-06 | launchd plists, Info.plist, receipts plists |
| `xattr` | 1.6.1 | 2025-09-21 | `com.apple.quarantine`, `com.apple.metadata:*` |
| `trash` | 5.2.9 | 2026-09-13 | **Not** recommended as quarantine (see §12) |
| `security-framework` | 3.7.0 | 2026-02-20 | `SecCode`/`SecStaticCode` signature checks |
| `core-foundation` | 0.10.1 | 2025-05-26 | CF types |
| `jwalk` / `walkdir` | 0.9.0 / 2.5.0 | 2026-08-05 / 2024-03-01 | Rejected for primary scan (no clone metadata) |

### 2. Size accounting: URL resource keys, volume capacity, purgeable space

**Per-file keys (Foundation `URLResourceKey`)**

- `fileSizeKey` / `totalFileSizeKey`: logical size. The "total" variant includes resource forks.
- `fileAllocatedSizeKey`: "the total allocated size on-disk for the file" ([docs](https://developer.apple.com/documentation/foundation/urlresourcekey/fileallocatedsizekey)).
- `totalFileAllocatedSizeKey`: "The allocated size in bytes may include space that metadata uses. This can be less than the value that `totalFileSize` returns … if it's a compressed resource." It is nil if unavailable and applies only to regular files ([docs](https://developer.apple.com/documentation/foundation/urlresourcevalues/totalfileallocatedsize)).
- None of the `URLResourceKey` size keys say anything about **clones or snapshots**. An allocated size over-counts every clone in full.

**BSD equivalents and the clone-aware extras (`getattrlist(2)` / `getattrlistbulk(2)`)** ([man page](https://keith.github.io/xcode-man-pages/getattrlist.2.html)):

| Attribute | Meaning (verbatim where quoted) |
| --- | --- |
| `ATTR_FILE_TOTALSIZE` / `ATTR_FILE_DATALENGTH` | Logical size |
| `ATTR_FILE_ALLOCSIZE` | "bytes on disk used by all of the file's forks (the physical size)" |
| `ATTR_FILE_DATAALLOCSIZE` | Physical size of data fork |
| `ATTR_CMNEXT_PRIVATESIZE` | "number of bytes that are **not** trapped inside a clone or snapshot, and which would be freed immediately if the file were deleted" |
| `ATTR_CMNEXT_CLONEID` | "uniquely identifies the data stream … Useful for finding which files are pure clones of each other (as they will have the same clone-id)" |
| `ATTR_CMNEXT_CLONE_REFCNT` | "number of full clones (each shares all of its blocks with this file)" |
| `ATTR_CMNEXT_EXT_FLAGS` | `EF_MAY_SHARE_BLOCKS`, `EF_SHARES_ALL_BLOCKS`, `EF_IS_SPARSE`, `EF_IS_PURGEABLE` ("can be deleted by the file system when asked to free space"), `EF_IS_SYNC_ROOT`, `EF_IS_SYNTHETIC` |
| `ATTR_CMNEXT_NOFIRMLINKPATH` | Path "that does not have firmlinks" |
| `ATTR_CMNEXT_REALDEVID`, `ATTR_CMNEXT_LINKID`, `ATTR_CMN_FILEID` | Identity for de-duplication (hard links, firmlinks) |

The constants are defined in xnu [`bsd/sys/attr.h`](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/sys/attr.h) (`ATTR_CMNEXT_PRIVATESIZE 0x8`, `CLONEID 0x100`, `EXT_FLAGS 0x200`, `CLONE_REFCNT 0x1000`). The constant values above were re-checked against xnu `main` and the macOS 26 SDK header (2026-10-05). Note that the `EF_*` flag values live in `bsd/sys/stat.h`, not `attr.h` (`EF_MAY_SHARE_BLOCKS 0x1`, `EF_NO_XATTRS 0x2`, `EF_IS_SYNC_ROOT 0x4`, `EF_IS_PURGEABLE 0x8`, `EF_IS_SPARSE 0x10`, `EF_IS_SYNTHETIC 0x20`, `EF_SHARES_ALL_BLOCKS 0x40`). The minimum OS for each `ATTR_CMNEXT_*` attribute is **(unverified)**. Probe at runtime with `FSOPT_PACK_INVAL_ATTRS` plus `ATTR_CMN_RETURNED_ATTRS`, and fall back gracefully.

**Per-volume capability gate (omitted from the original draft).** The man page and `attr.h` tie `ATTR_CMNEXT_CLONEID` and `ATTR_CMNEXT_CLONE_REFCNT` to `VOL_CAP_FMT_CLONE_MAPPING` (0x04000000): "If this bit is set, the volume format supports full clone tracking." Lumen must read `ATTR_VOL_CAPABILITIES` once per volume with `getattrlist` on the mount root (volume attributes cannot be requested through `getattrlistbulk`) and treat clone IDs and clone ref-counts as **meaningless** on volumes without this bit. On such volumes, fall back to `PRIVATESIZE` or to allocated size flagged "may over-count shared blocks", and never merge clone groups by `CLONEID`. Also check `VOL_CAP_INT_CLONE` before assuming clones exist at all.

Relevant `st_flags` bits from xnu [`sys/stat.h`](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/sys/stat.h):
- `UF_COMPRESSED` (0x20): decmpfs-compressed. Allocated size is less than logical size.
- `SF_RESTRICTED` (0x00080000): "entitlement required for writing". This is the SIP flag.
- `SF_NOUNLINK` (0x00100000).
- `SF_FIRMLINK` (0x00800000).
- `SF_DATALESS` (0x40000000): "file is dataless object". This covers iCloud and File Provider placeholders. **Reading their content triggers a download.**

**Volume capacity** ([Apple article](https://developer.apple.com/documentation/foundation/checking-volume-storage-capacity)):
- `volumeTotalCapacityKey`, `volumeAvailableCapacityKey`.
- `volumeAvailableCapacityForImportantUsageKey`: "for storing important resources … based on a user request or resources the app requires to function properly".
- `volumeAvailableCapacityForOpportunisticUsageKey`: "downloading data in a more predictive manner".
- These keys are declared "required reason" APIs for fingerprinting purposes. A `PrivacyInfo.xcprivacy` declaration is needed when the code ships in an App Store app, and is harmless for Developer ID.
- Per [Eclectic Light (2023)](https://eclecticlight.co/2023/04/27/where-does-macos-get-its-volume-free-space-figures-from/), `statfs`/`df` report only unconditionally free space. Disk Utility gets its purgeable and available figures from **CacheDelete** (`deleted`) through `volumeAvailableCapacityForImportantUsage`-type calls. CacheDelete evaluates files with the purgeable inode flag and polls about 39 services registered via plists in `/System/Library/CacheDelete`.

**Purgeable space and snapshots**
- Apple's formula, as quoted by Eclectic Light: Capacity = Available + (Used − Purgeable). `tmutil localsnapshot` docs say local snapshots "are considered purgeable and may be removed at any time by deleted(8)".
- In 2026 testing, macOS "failed to recognise the purgeability of the huge snapshot", Finder omits purgeable space, and Storage settings does not disclose snapshot data. The problems "only got worse" by Tahoe ([Eclectic Light 2026-08-24](https://eclecticlight.co/2026/08/24/arent-snapshots-purgeable/)).
- Purge order (Ventura analysis): volume caches, then subsystem caches (QuickLook thumbnails, Asset Cache, Mail), then service caches and extensions, then **Time Machine snapshots last**, at urgency levels 1–3 ([Eclectic Light](https://eclecticlight.co/2023/04/19/ventura-space-management-what-gets-purged-and-how/)).

### 3. APFS semantics that distort size accounting

- **Clones.** `clonefile(2)` creates copy-on-write copies: "Subsequent writes to either the original or cloned file are private to the file being modified". Source and destination must be on the same filesystem (`EXDEV` otherwise), and the volume must advertise `VOL_CAP_INT_CLONE` ([clonefile(2)](https://keith.github.io/xcode-man-pages/clonefile.2.html)).
  - APFS has no dedup and "uses clone files to minimize data storage and data duplication" ([APFS FAQ](https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/APFS_Guide/FAQ/FAQ.html)).
  - Consequence: deleting a clone frees only its *private* blocks. Deleting *all* clones of an extent frees the shared blocks once.
- **Partial clones.** After a clone is modified, the files share *some* blocks (`EF_MAY_SHARE_BLOCKS` set, `EF_SHARES_ALL_BLOCKS` clear). Clone ID equality no longer identifies them. Only `PRIVATESIZE` is exact per file, and there is no public API for "which other file shares these extents".
- **Cost of `PRIVATESIZE`.** Several 2026 open-source APFS analyzers report that requesting `ATTR_CMNEXT_PRIVATESIZE` for every file made scans about **5× slower**, because the kernel walks extents. They request it only for files with `EF_MAY_SHARE_BLOCKS`, and only above a size threshold such as 64 KiB (**third-party reports, unverified**; e.g. [cheapsteak/duh](https://github.com/cheapsteak/duh), [rustClean PR #32](https://github.com/hzrbasaran/rustClean/pull/32)).
- **Local snapshots.** Blocks referenced by a local Time Machine or other APFS snapshot are not freed by deleting the live file. The same third-party analyzers report `PRIVATESIZE = 0` for files older than the newest snapshot (**unverified**, but consistent with the man-page definition "not trapped inside a clone or snapshot").
  - Apple: Time Machine takes a local snapshot about every hour and keeps it for 24 h, **but also "keeps an additional snapshot of your last successful Time Machine backup until space is needed"**. So some snapshot-held blocks are not freed at the 24 h mark; they are freed only under space pressure, or never if TM backups stop succeeding. Third-party backup tools' snapshots follow their own retention ([Apple 102154](https://support.apple.com/en-us/102154), [Eclectic Light](https://eclecticlight.co/2025/02/11/what-is-system-data-in-storage-settings/)).
  - Tools: `tmutil listlocalsnapshots <mount>`, `tmutil listlocalsnapshotdates`, `tmutil deletelocalsnapshots {mount|date}`, `tmutil thinlocalsnapshots <mount> [bytes] [urgency 1-4]` ([ss64 tmutil](https://ss64.com/mac/tmutil.html)).
- **Sparse files.** Allocated size is much smaller than logical size. Use `EF_IS_SPARSE` or allocated < logical. Docker/VM disk images are typical, and "reclaim" there means compacting inside the guest, not deleting.
- **Compressed files** (`UF_COMPRESSED`, decmpfs) also have allocated < logical.
- **Hard links.** Files can have `st_nlink > 1` (directory hard links are not supported on APFS; HFS+ conversions became symlinks ([APFS FAQ](https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/APFS_Guide/FAQ/FAQ.html))). De-duplicate by `(dev, fileid)`.
- **Firmlinks and the volume group.** Since Catalina the boot disk is a System + Data volume group.
  - `/usr/share/firmlinks` maps `/Applications`, `/Library`, `/Users`, `/Volumes`, `/private`, `/opt`, `/usr/local`, `/System/Library/Caches`, `/System/Library/Assets`, `/System/Library/Speech` and others onto the Data volume, mounted at `/System/Volumes/Data` ([Eclectic Light](https://eclecticlight.co/2023/07/22/how-macos-depends-on-firmlinks/)).
  - A naive walk of `/` that also descends into `/System/Volumes/Data` **double counts**.
  - Firmlinks are marked `SF_FIRMLINK`, and `ATTR_CMNEXT_NOFIRMLINKPATH` returns canonical paths.
- **Sealed System Volume (macOS 11+).** The System volume is an APFS snapshot sealed with a Merkle-tree hash, and boot halts if verification fails ([Apple Platform Security](https://support.apple.com/guide/security/signed-system-volume-security-secd698747c9/web)). Nothing on it is user-reclaimable. Data-volume paths that *look* like system paths (for example `/System/Library/Caches` via firmlink) are **not** covered by the seal.
- **Fast directory sizing.** APFS can precompute directory sizes ("quickly compute the total space used by a directory hierarchy"), but it is opt-in per directory and not a general API ([APFS Guide: Features](https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/APFS_Guide/Features/Features.html)). Don't rely on it.

### 4. Low-level traversal: getattrlistbulk, fts, readdir, FileManager enumerator

**`getattrlistbulk(2)`** ([man](https://keith.github.io/xcode-man-pages/getattrlistbulk.2.html)):
- `ATTR_CMN_NAME` and `ATTR_CMN_RETURNED_ATTRS` are mandatory.
- Records are packed with a `uint32_t` length prefix and 8-byte alignment.
- It returns the number of entries, 0 at the end, -1 with `errno` on failure.
- `FSOPT_PACK_INVAL_ATTRS` fills unsupported attributes with defaults. `ATTR_CMN_ERROR` gives a per-entry error.
- Volume attributes cannot be requested, and "the order … is not specified".
- It is one syscall per batch instead of N+1 (`readdir` + `lstat`).

**Apple DTS guidance** ([forums 760256](https://developer.apple.com/forums/thread/760256)): counting about 357k files took:

| Method | Time |
| --- | --- |
| `fts` (`FTS_PHYSICAL\|FTS_NOCHDIR\|FTS_NOSTAT`) | 4.5 s |
| `enumeratorAtURL` with `@[]` keys | 4.8 s |
| `enumeratorAtURL` with `nil` keys (default set) | 9.0 s |
| `enumeratorAtPath` | 13.6–14.4 s |

- "Low level doesn't mean faster". Both `fts` and the URL enumerator use `getattrlistbulk` underneath.
- "getattrlistbulk is not an API I'd recommend building on … The trickiest part is managing its memory buffers".
- An `@autoreleasepool` per iteration cut peak memory from about 491 MB to 6.3 MB.
- Parallel enumeration helps on APFS ("highly parallel") but not on SMB.

**Independent benchmark (2019):** `fts` was usually fastest on HFS+ and APFS, `readdir` was fastest when no attributes are needed, `getattrlistbulk` was best over AFP, and on SMB it depends on the need. The author recommends choosing by filesystem ([Tempel](http://blog.tempel.org/2019/04/dir-read-performance.html)). AFP is gone in macOS 27.

**Assessment for Lumen.** Lumen's core is Rust, so the Foundation enumerator means ObjC bridging per entry, and `fts` returns `stat` but **no clone, sharing or private-size data**. Lumen's differentiator is honest reclaimable bytes, so the scanner should call `getattrlistbulk` directly. Design points:
- Batch buffer of about 256 KiB.
- Request `NAME | RETURNED_ATTRS | ERROR | OBJTYPE | FILEID | DEVID/FSID | FLAGS | MODTIME/ACCTIME | FILE_ALLOCSIZE | FILE_TOTALSIZE | FILE_LINKCOUNT | CMNEXT_CLONEID | CMNEXT_EXT_FLAGS`, plus `CMNEXT_PRIVATESIZE` only in a second pass on candidates.
- Open directories with `O_DIRECTORY|O_NOFOLLOW`. Never follow symlinks, and stay on one device.
- Parallelize at directory granularity with a work-stealing pool (Rayon/Tokio blocking pool).
- Fuzz-test the parser.

### 5. Spotlight: NSMetadataQuery, mdfind, kMDItemLastUsedDate

- `kMDItemLastUsedDate` is "updated automatically by LaunchServices every time a file is opened by double-clicking it or when LaunchServices is asked to open a file" ([docs](https://developer.apple.com/documentation/coreservices/kmditemlastuseddate)).
  - It does **not** reflect programmatic opens (`open(2)`), CLI tools or app-internal use.
  - Apple forum reports show it sometimes fails to update even for Finder opens ([forums 20639](https://developer.apple.com/forums/thread/20639)).
  - It is weak evidence for "unused" files and usable evidence for apps (last launched via LaunchServices).
- Spotlight does not index hidden paths, package contents, excluded folders or unmounted volumes, and `mdfind` "cannot see what was never indexed" ([summary of mdfind guides](https://everywherefast.com/blog/mdfind-command-guide)). So **Spotlight cannot inventory `~/Library/Caches`** or similar.
- Use it for:
  - fast lookups of app bundles by `kMDItemCFBundleIdentifier`, to resolve "is the owning app installed anywhere, including `/Volumes/*`?";
  - large-file discovery in user documents (`kMDItemFSSize`);
  - `kMDItemWhereFroms` evidence for downloads.
- `NSMetadataQuery` needs a run loop. From Rust, shelling out to `mdfind -0 'kMDItemCFBundleIdentifier == "com.x.y"'` is simpler; treat it as a best-effort accelerator with a filesystem fallback.
- 27.0 release notes mention Spotlight changes ("Better search in Spotlight"), and Tahoe 26.6 "optimizes Spotlight for the release of macOS 27". Re-test index coverage on 27 **(behavioral changes unverified)** ([9to5Mac](https://9to5mac.com/2026/09/09/macos-27-golden-gate-here-are-apples-full-release-notes/)).

### 6. FSEvents and the `notify` crate

**API facts** ([FSEventStreamCreate](https://developer.apple.com/documentation/coreservices/1443980-fseventstreamcreate), [FSEvents guide](https://developer.apple.com/library/archive/documentation/Darwin/Conceptual/FSEvents_ProgGuide/UsingtheFSEventsFramework/UsingtheFSEventsFramework.html), [flags](https://developer.apple.com/documentation/coreservices/1455361-fseventstreameventflags)):

- **History and resumption.** Event IDs persist across reboots. Pass the stored ID as `sinceWhen` ("Do not pass zero … unless you want … every directory modified since 'the beginning of time'"). `kFSEventStreamEventFlagHistoryDone` marks the end of replay.
- **Per-device streams** (`FSEventStreamCreateRelativeToDevice`) are recommended for persistence: IDs only increase per disk and paths are relative to the volume root. Check identity with `FSEventsCopyUUIDForDevice`. Rescan everything if:
  - the UUID changed (reformat or different disk), or
  - the stored ID is above the current one (restore, purge, wrap).
- **Granularity.** The model is directory-level: "For each event, you should scan the directory at the specified path". `kFSEventStreamCreateFlagFileEvents` adds per-file flags (`ItemCreated/Removed/Renamed/Modified/Cloned/IsHardlink…`), but coalescing still applies.
- **Coalescing and drops.** `MustScanSubDirs` is set when events coalesce or are dropped ("When an event is dropped, the `kFSEventStreamEventFlagMustScanSubDirs` flag is also set"). The `latency` parameter trades responsiveness for coalescing.
- **Other flags.**
  - `RootChanged` (with `kFSEventStreamCreateFlagWatchRoot`): the root moved; use `fcntl(F_GETPATH)`.
  - `EventIdsWrapped`.
  - `Mount`/`Unmount`.
  - `OwnEvent` (needs `MarkSelf`), useful for ignoring Lumen's own quarantine moves.
- **Race-free bootstrap.** "you must start monitoring the directory *before* you start scanning it", and rescan any subdirectory modified during the scan.
- **Advisory only.** "you should treat the events list as advisory rather than a definitive list of all changes to the volume". Disks can be modified elsewhere, so do periodic full sweeps.

**`notify` crate** ([docs.rs](https://docs.rs/notify/latest/notify/), [crates.io](https://crates.io/crates/notify)):
- Stable 8.2.0 (2025-08-03). The 9.0.0 RCs ran rc.1 (2026-01-25) to rc.5 (2026-08-30), with no 9.0 final.
- `macos_fsevent` is the default and `macos_kqueue` is optional.
- Documented caveat: "Due to the inner security model of FSEvents … some events cannot be observed easily when trying to follow files that do not belong to you", in which case the fallback is `PollWatcher`.
- `notify`'s `Watcher` API has no way to resume from a persisted FSEvents ID or select per-device streams. Confirmed against `notify/src/fsevent.rs` on the `main` branch (2026-10-05): `since_when` is hard-coded to `kFSEventStreamEventIdSinceNow`, the stream is created with `FSEventStreamCreate` (not `...RelativeToDevice`), and `Config` exposes only `with_fsevent_latency` for this backend.
- Use `notify` for live "folder being viewed changed" UX. Use direct FSEvents (`fsevent-sys` 5.2.0 or `objc2-core-services`) for the persistent incremental index.

### 7. ~/Library locations: safe vs dangerous

Apple's definitions ([File System Programming Guide, Library details](https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/FileSystemProgrammingGuide/MacOSXDirectories/MacOSXDirectories.html)):

- **Caches**: "cached data that can be regenerated as needed. Apps should never rely on the existence of cache files". Named by bundle ID by convention.
- **Application Support**: "files that your app creates and manages on behalf of the user and can include files that contain user data".
- **Preferences**: "never create files in this directory yourself", so use `NSUserDefaults`. Deleting a plist while the app or `cfprefsd` has it cached is ineffective or racy.
- **Logs**: console and service logs.
- **Containers**: "home directories for any sandboxed apps".

| Location | Default Lumen class | Rationale / caveats |
| --- | --- | --- |
| `~/Library/Caches/<bundle-id>` | QUARANTINE candidate (with conditions) | Apple contract says it can be regenerated. Conditions: owning app not running, not inside a sync root, not `SF_DATALESS`, and per-app overrides apply. Some apps misuse Caches for state (e.g. offline downloads). Use Jev/evidence plus rules per bundle ID |
| `~/Library/Caches/com.apple.*`, `CloudKit`, `com.apple.Safari*` | REVIEW / prefer system purge | Apple's CacheDelete manages these. Some are TCC-protected |
| `~/Library/Logs`, `/Library/Logs`, `DiagnosticReports` | QUARANTINE candidate after age threshold | Low risk, but crash reports have support value. Keep the newest N |
| `~/Library/Application Support/*` | REVIEW only | May contain the only copy of user data. macOS 27 adds TCC protection for some apps' folders here (§10) |
| `~/Library/Containers/*`, `Group Containers/*` | REVIEW only, never auto | "If in doubt, leave Containers well alone". There is no auto-removal on uninstall, and they may hold the only copy of documents ([Eclectic Light](https://eclecticlight.co/2024/08/05/what-are-all-those-containers/)). Access prompts on 14+/15+ |
| `~/Library/Saved Application State/*.savedState` | QUARANTINE candidate | Window-restoration state only (well-established behavior; no Apple primary source fetched, **unverified**). Lumen-style tools commonly treat it as low risk. Skip for running apps |
| `~/Library/Preferences/*.plist` | KEEP (orphans → REVIEW) | Tiny. Settings loss annoys users and saves nothing |
| `~/Library/Mail`, `Messages`, `Safari`, `Photos` libraries, `Mobile Documents` (iCloud) | KEEP / route to the owning app | Data vaults and user data. FDA-gated. Use Apple's own Storage-settings flows |
| `~/Library/Developer/Xcode/DerivedData`, `iOS DeviceSupport`, simulator runtimes | QUARANTINE candidate / REVIEW | Regenerable. Apple's Storage "Developer" category offers the same ([Apple](https://support.apple.com/guide/mac-help/change-storage-settings-mchl3d437fbc/mac)) |
| `~/Library/Application Support/MobileSync/Backup` | REVIEW only | iOS backups: may be the only backup |
| `/Library/Caches`, `/private/var/folders/*` (per-user temp/cache via `confstr(_CS_DARWIN_USER_CACHE_DIR)`) | `/var/folders` KEEP; `/Library/Caches` REVIEW | `/var/folders` is live OS state; let macOS manage it |
| Anything under SIP paths, the SSV, or flagged `SF_RESTRICTED` | **Hard KEEP** | See §11 |

### 8. launchd, SMAppService, BTM: enumerating login and background items

**Classic launchd locations** ([Daemons and Services guide](https://developer.apple.com/library/archive/documentation/MacOSX/Conceptual/BPSystemStartup/Chapters/CreatingLaunchdJobs.html)):
- `~/Library/LaunchAgents` (per user).
- `/Library/LaunchAgents` (all users, third party).
- `/Library/LaunchDaemons` (system, third party).
- `/System/Library/LaunchAgents|Daemons` (Apple, SSV, read-only).

Key plist fields: `Label` (required), `Program`/`ProgramArguments`, `RunAtLoad`, `KeepAlive`, `StartInterval`, `StartCalendarInterval`, `WatchPaths`, `MachServices`, plus `BundleProgram` and `AssociatedBundleIdentifiers` (below). All of these are readable by a normal user and parse with the `plist` crate.

**SMAppService (macOS 13+)** ([class](https://developer.apple.com/documentation/servicemanagement/smappservice), [migration guide](https://developer.apple.com/documentation/servicemanagement/updating-helper-executables-from-earlier-versions-of-macos)):
- Constructors: `.mainApp`, `.loginItem(identifier:)`, `.agent(plistName:)`, `.daemon(plistName:)`.
- Calls: `register()`, `unregister()` / `unregister(completionHandler:)`, `status` (`notRegistered`, `enabled`, `requiresApproval`, `notFound`), `openSystemSettingsLoginItems()`, `statusForLegacyPlist(at:)`.
- Plists live **inside the app bundle**: `Contents/Library/LaunchAgents/`, `Contents/Library/LaunchDaemons/` and `Contents/Library/LoginItems/`. They use `BundleProgram` (a bundle-relative path, for example `Contents/Resources/mydaemon`) instead of `Program`. This replaces "manually installing property lists in `~/Library/LaunchAgents` or `/Library/LaunchAgents`".
- Register daemons first: "They require user authentication but authorize all other helper executables in the app bundle simultaneously".
- Legacy plists can carry `AssociatedBundleIdentifiers` (the Team ID must match). Otherwise System Settings shows the certificate's organization name instead of the app.

**Background Task Management (BTM)** ([Apple Platform Deployment](https://support.apple.com/guide/deployment/manage-login-items-background-tasks-mac-depdca572563/web)):
- macOS 13+ tracks login items, agents and daemons, and notifies the user once ("managed items are being installed and can be viewed in System Settings"), with 24 h suppression.
- MDM rules can match by BundleIdentifier(Prefix), TeamIdentifier and Label(Prefix).
- Diagnostics: `sfltool dumpbtm` ("Prints the current status of login and background items"), `sfltool resetbtm`, and `log stream --predicate "subsystem = 'com.apple.backgroundtaskmanagement'"`.
- Store: `/private/var/db/com.apple.backgroundtaskmanagement/BackgroundItems-v*.btm`, a private keyed-archive format.
- The version changes across releases: v4/v7 on Ventura ([DumpBTM](https://github.com/objective-see/DumpBTM)), v13 on 15.2 ([search summary of mac_apt blog](http://www.swiftforensics.com/2025/01/macapt-update-to-btm-processing.html)), v16 on 26 ([Eclectic Light 2026-02](https://eclecticlight.co/2026/02/20/in-the-background-identification/)).
- `sudo sfltool dumpbtm` lists per-UID items with UUID, name, type, developer name, Team ID, disposition (enabled/allowed/notified), URL, executable path, bundle ID and parent.
- `attributions.plist` inside `BackgroundTaskManagement.framework` maps thousands of helpers to vendors.
- Objective-See's GPL-3.0 DumpBTM parses the file directly and needs FDA.

**What this means for enumeration**
1. **Unprivileged baseline.** Parse plists in the five dirs plus `Contents/Library/{LaunchAgents,LaunchDaemons,LoginItems}` inside every installed app bundle. Resolve `Program`/`BundleProgram` to a path, code-sign it (Team ID via `SecStaticCodeCopySigningInformation`), and link it to the owning app via the bundle and `AssociatedBundleIdentifiers`. This builds the evidence-graph edges app → launch agent → executable.
2. **Loaded state.** Use `launchctl print gui/<uid>` and `launchctl print system` (system domain details need root for some fields), or `launchctl list` for a coarse view. The output format is not API, so parse defensively **(format stability unverified)**.
3. **BTM view (optional, privileged).** Run `sfltool dumpbtm` through the privileged helper and parse it as evidence only, tolerating format drift. Do **not** ship a parser for the `.btm` file itself: it is private, versioned and GPL in the only open implementation.
4. **Changes.** To disable a modern SMAppService item, call `SMAppService.openSystemSettingsLoginItems()` and let the user toggle it. That is reversible and Apple-owned. For a legacy third-party plist, run `launchctl bootout` and then quarantine-move the plist; restore is a move back plus `launchctl bootstrap`. This is REVIEW tier, and a Team ID that matches an installed app is evidence for KEEP.

### 9. Privileged operations: SMJobBless deprecation, SMAppService.daemon, XPC

- **`SMJobBless`** has been deprecated since macOS 13.0: "Please use SMAppService instead" ([docs](https://developer.apple.com/documentation/servicemanagement/smjobbless(_:_:_:_:))). Don't use it for new code.
- **`SMAppService.daemon(plistName:)`** registers a root launchd daemon from `Contents/Library/LaunchDaemons/<plist>`. The plist declares `MachServices` so clients can connect via XPC. On first `register()` the status is `requiresApproval`, and the user enables it in **System Settings › General › Login Items & Extensions** (the app can deep-link with `openSystemSettingsLoginItems()`). Forum guidance says SMAppService "cannot prompt users for password approval on daemon installation" (third-party summary; consistent with Apple's migration doc).
- **XPC peer validation** (both directions):
  - C API: `xpc_connection_set_peer_code_signing_requirement` (macOS 12+), plus `..._team_identity_requirement`, `..._platform_identity_requirement`, `..._entitlement_exists_requirement`, `..._lightweight_code_requirement` ([docs](https://developer.apple.com/documentation/xpc/xpc_connection_set_peer_code_signing_requirement(_:_:))).
  - Foundation: `NSXPCConnection.setCodeSigningRequirement(_:)` (macOS 13+). It must be called before `resume()`. A malformed requirement is fatal in Swift or an exception in ObjC. On mismatch "the connection becomes invalidated" with `NSXPCConnectionCodeSigningRequirementFailure` ([docs](https://developer.apple.com/documentation/foundation/nsxpcconnection/setcodesigningrequirement(_:))).
  - Swift (macOS 26+): `XPCPeerRequirement` with `.isFromSameTeam(andMatchesSigningIdentifier:)`, `.hasEntitlement(_:)`, `.entitlement(_:matches:)`, `.isPlatformCode(…)`, `.codeRequirement(_:)`, passed to `XPCListener`/`XPCSession` ([docs](https://developer.apple.com/documentation/xpc/xpcpeerrequirement)).
  - Recommended requirement string: `anchor apple generic and certificate leaf[subject.OU] = "<TEAMID>" and identifier "com.lumen.app"`. Both sides should check; the helper should also verify the client on every message, not only on connect.
- **Authorization.** For per-operation admin consent inside a long-lived daemon, the established pattern is the app obtaining an `AuthorizationRef` with a custom right (`AuthorizationCopyRights`), externalizing it (`AuthorizationMakeExternalForm`) and sending it over XPC for the helper to re-check (see the [SwiftAuthorizationSample](https://github.com/trilemma-dev/SwiftAuthorizationSample) pattern; details **unverified** against current Apple docs). For Lumen, user approval of the daemon plus a plan-ID-only command surface is the main control. AuthorizationServices is an optional second factor for system-scope deletes.
- **TCC still applies to root.** A root daemon does not bypass TCC for protected user data. Quinn (DTS, Apr 2024) confirms that FDA granted to an app extends to items it installs with `SMAppService`: "One of the key reasons we introduced `SMAppService` was to make it easier for us to track the responsibility relationship between disparate code items" ([forums 750484](https://developer.apple.com/forums/thread/750484)). This does **not** hold for legacy helpers: the older "Rules for Full Disk Access" thread says privileged helpers installed into `/Library/PrivilegedHelperTools` (the SMJobBless model) and bare command-line tools do not inherit FDA from the parent app, and helper tools should carry a child bundle ID embedded in the binary ([forums 107546](https://developer.apple.com/forums/thread/107546?page=1)). Legacy launchd plists not installed via SMAppService need `AssociatedBundleIdentifiers` for attribution.

### 10. TCC, Full Disk Access, App Sandbox, Hardened Runtime, notarization

**What FDA covers**: "all files on your computer, including data from other apps (for example, Mail, Messages, Safari, and Home), data from Time Machine backups, and certain administrative settings for all users on this Mac" ([Apple](https://support.apple.com/guide/mac-help/change-privacy-security-settings-on-mac-mchl211c911f/mac)). **App Management**: "Allow apps to update or delete other apps on your Mac".

**Layered protections a scanner hits:**

| Protection | Since | Trigger | Grant path |
| --- | --- | --- | --- |
| Data vaults / FDA (`kTCCServiceSystemPolicyAllFiles`) | 10.14 | `~/Library/Mail`, `Messages`, `Safari`, Time Machine, other users' homes, TCC.db | User adds app in Privacy & Security › Full Disk Access |
| App Management (`kTCCServiceSystemPolicyAppBundles`) | 13 | Modifying the *contents* of another developer's signed app bundle (Lapcat's analysis covers modification only). Apple's settings text says "update or delete other apps", but whether moving/renaming a whole `.app` out of `/Applications` triggers it is **unverified** (see Risks). Same-team updates "just work" | Privacy & Security › App Management ([Lapcat](https://lapcatsoftware.com/articles/AppManagement.html)) |
| App data (`kTCCServiceSystemPolicyAppData`) | 14 | Reading another app's `~/Library/Containers/<id>` | Consent prompt ("would like to access data from other apps"), or FDA. Prompt text from `NSAppDataUsageDescription` |
| Group Containers | 15 | `~/Library/Group Containers/*` | Eclectic Light describes this as SIP-backed protection under which Group Containers "can only be accessed by apps identified as being members of that group". Whether a consent prompt or FDA unlocks read access for a non-member scanner like Lumen is **unverified; test on 15/26/27** ([Eclectic Light](https://eclecticlight.co/2024/08/05/what-are-all-those-containers/)) |
| `kTCCServiceSystemPolicyAppDataDetailed` | 27 (third-party analysis) | Selected non-sandboxed apps' `~/Library/Application Support/<name>` (Chrome, Brave, Edge, Firefox, Discord, crypto wallets), enforced by kernel `com.apple.macl` xattrs and a `sandboxd` allowlist (`defaultRules-2`) updated via XProtect (`AppProtectionRules.plist`). Per the analysis it blocks **writes** (create/modify) by other code; reads appear to still work; delete/rename behavior is not stated | **No grant path found: the analysis reports that Full Disk Access does *not* bypass it** (Terminal with FDA still got "Operation not permitted"). Expect quarantine moves of these folders to fail with `EPERM`. **(Third-party analysis; unverified against Apple docs)** ([Regula](https://wojciechregula.blog/post/golden-gate-appdata-protection/)) |

**Detecting FDA.**
- There is no API. Quinn: "the location and permissions of the TCC database is not considered API"; "do what you're really trying to do and then handle errors". The permission-error list is long: FDA, Data Vaults, SIP, sandboxing, ACLs, UNIX permissions ([forums 114452](https://developer.apple.com/forums/thread/114452)).
- The pragmatic probe is to try to open a known FDA-gated file that exists, such as `~/Library/Safari/Bookmarks.plist` or `/Library/Preferences/com.apple.TimeMachine.plist`. `EPERM` with the file present means "no FDA". This is not Apple-endorsed.
- Deep link to the pane: `x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_AllFiles` (Ventura+), with the legacy `com.apple.preference.security?Privacy_AllFiles` form. **Undocumented, unverified on 27** ([community list](https://github.com/bvanpeski/SystemPreferences/blob/main/macos_preferencepanes-Ventura.md)).

**Known 2025–26 bugs.** On macOS 26.1 and 26.2, command-line binaries vanished from the FDA list UI. Grants still worked, Quinn confirmed it was a bug, and it was fixed in 26.3 beta 1 ([forums 806187](https://developer.apple.com/forums/thread/806187), [809549](https://developer.apple.com/forums/thread/809549)). This is another reason to keep every Lumen executable inside the `.app` with a child bundle ID.

**App Sandbox / Mac App Store.** Guideline 2.4.5:
- (i) "must be appropriately sandboxed".
- (iii) no auto-launch "without consent".
- (iv) no downloading code.
- (v) "may not request escalation to root privileges".
- (vii) updates only via MAS.

Guideline 2.3.1 also prohibits misleading marketing ([guidelines](https://developer.apple.com/app-store/review/guidelines/)). A sandboxed app sees only its container and user-selected folders (security-scoped bookmarks), so it cannot do cross-app cache attribution or launchd inventory.

**Hardened Runtime and notarization** ([Apple](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution)):
- Developer ID certificate, Hardened Runtime on all executables, a secure timestamp, and no `get-task-allow`.
- Upload with `notarytool` ("no longer accepts uploads from `altool`" since 2023-11-01), then `stapler`.
- Gatekeeper checks all software on first open, uses app translocation for quarantined apps ([Platform Security](https://support.apple.com/guide/security/gatekeeper-and-runtime-protection-sec5599b66df/web)), and since Ventura checks notarized-app integrity beyond quarantined apps ([Lapcat](https://lapcatsoftware.com/articles/AppManagement.html)).

### 11. SIP, Gatekeeper, com.apple.quarantine

- **SIP-protected:** `/System`, `/usr`, `/bin`, `/sbin`, `/var`, and "Apps that are pre-installed with the Mac operating system". Third parties may write `/Applications`, `/Library` and `/usr/local`. SIP "restricts the root user account" ([Apple 102149](https://support.apple.com/en-us/102149)). Note that `/var` → `/private/var`, and parts of `/private/var` (e.g. `/private/var/folders`) are writable per-user. Use `SF_RESTRICTED` on each object as the authoritative per-item signal rather than path prefixes alone.
- **Never disable SIP.** Lumen must never instruct users to run `csrutil disable`. If SIP is reported off (`csrutil status`), Lumen should *tighten*, not loosen, its rules and show a warning.
- **Gatekeeper / quarantine xattr.**
  - Downloaded files carry `com.apple.quarantine` (`flags;hex-timestamp;agent-name;UUID`; the format is undocumented but long-stable, **unverified**).
  - `com.apple.metadata:kMDItemWhereFroms` records the URL.
  - These are good evidence for "old downloaded installer (.dmg/.pkg/.zip) in ~/Downloads already installed".
  - Lumen must **never remove `com.apple.quarantine`** from other software; that is a Gatekeeper bypass.
  - Since macOS 15, the Control-click override is gone. Users must use System Settings › Privacy & Security › "Open Anyway" ([Apple news](https://developer.apple.com/news/?id=saqachfa)).

### 12. Reversible deletion: Trash semantics

- `FileManager.trashItem(at:resultingItemURL:)` (macOS 10.8+) moves an item to the trash. "The actual name of the item may be changed when moving it to the trash, so use this URL" ([docs](https://developer.apple.com/documentation/foundation/filemanager/trashitem(at:resultingitemurl:))). Apple's docs say nothing about volumes without a trash.
- **Per-volume Trash.** The boot volume uses `~/.Trash`; other local volumes use `<vol>/.Trashes/<uid>/`. Put Back metadata lives in the `.DS_Store` of that trash folder (`ptbL`/`ptbN` records) ([DLEAPP PR](https://github.com/abrignoni/DLEAPP/pull/334), third-party forensic description). Network and some removable volumes may have no trash, so `trashItem` fails or deletes immediately (**unverified; test per FS**).
- **Put Back is unreliable programmatically.** Apple DTS confirmed the missing-Put-Back bug (r. 23153124, "known for at least 10 years"). Quinn *suspects* (not confirmed) that FileManager and Finder "are bumping into each other" over `.DS_Store`; in his 15.2 test it reproduced only while Finder's Trash window was open, and an empirical ~2 s delay between calls avoided it. There is no supported workaround ([forums 773997](https://developer.apple.com/forums/thread/773997)).
- The `trash` crate's macOS default is `DeleteMethod::Finder` (an AppleScript to Finder: Automation TCC prompt, sound, Put Back). `NsFileManager` is faster and needs no prompt, but "Does *not* show the 'Put Back' option on some systems". There are no list/restore functions on macOS ([source](https://github.com/Byron/trash-rs/blob/master/src/macos/mod.rs)).
- The user's "Remove items from the Trash after 30 days" setting can permanently delete trashed items ([Apple](https://support.apple.com/guide/mac-help/free-up-storage-space-on-mac-sysp4ee93ca4/mac)).
- **Space reality.** Neither Trash nor rename-quarantine frees space until purge, and with a local snapshot even purge may free nothing until the snapshot expires (§3).

### 13. Running processes, open files, app identification, receipts

- **GUI apps.** `NSWorkspace.shared.runningApplications` (KVO-observable, thread-safe) returns `NSRunningApplication` with `bundleIdentifier`, `bundleURL`, `executableURL` and `processIdentifier`. It covers "running applications only — does not include background daemons" ([docs](https://developer.apple.com/documentation/appkit/nsworkspace/runningapplications)).
- **All processes.** libproc `proc_listpids` / `proc_listallpids`, `proc_pidpath` (path), and `proc_pidinfo(PROC_PIDTBSDINFO)` (uid, ppid, start time). Rust: `libproc` 0.14.11 ([docs.rs](https://docs.rs/libproc/latest/libproc/)) or `sysinfo` 0.39.6.
- **Open files.** `proc_pidinfo(pid, PROC_PIDLISTFDS)` then `proc_pidfdinfo(PROC_PIDFDVNODEPATHINFO)` gives paths. Without root this only works for the **user's own processes**; others return `EPERM`, which is why `lsof` needs `sudo` (community-confirmed, e.g. [psutil #883](https://github.com/giampaolo/psutil/issues/883)). Memory-mapped files need `PROC_PIDREGIONPATHINFO` **(unverified detail)**.
- **App identity.**
  - LaunchServices: `NSWorkspace.urlForApplication(withBundleIdentifier:)`, `urlsForApplications(withBundleIdentifier:)` on macOS 12+ (confirmed in Apple's documentation metadata, 2026-10-05).
  - Spotlight: `kMDItemCFBundleIdentifier`.
  - `Info.plist`: `CFBundleIdentifier`, `CFBundleShortVersionString`.
  - Code signature: Team ID via `SecStaticCodeCreateWithPath` + `SecCodeCopySigningInformation` (`security-framework` 3.7.0).
- **Receipts.** `pkgutil --pkgs`, `--pkg-info`, `--files <id>`, `--only-files`/`--only-dirs`, and `--forget` (removes the receipt only). Receipts are a plist plus a `.bom` per package in `<vol>/var/db/receipts`, but "The files and directories where receipts are stored are subject to change. Always use pkgutil" ([ss64 pkgutil](https://ss64.com/mac/pkgutil.html)). A receipt's file list is strong evidence linking `/Library/LaunchDaemons/*.plist`, `/Library/Application Support/<vendor>` and similar paths to an installer package. Directory entries like `Library` appear too, so never treat "listed in BOM" as "safe to delete".

### 14. Apple's own Storage settings (comparison)

- **Location:** System Settings › General › Storage.
- **Recommendations:** Store in iCloud, Optimize Storage (watched TV and old mail attachments), Empty Trash automatically (30 days) ([Apple](https://support.apple.com/guide/mac-help/free-up-storage-space-on-mac-sysp4ee93ca4/mac)). macOS also "clears caches and logs that are safe to delete, including temporary database files, interrupted downloads, staged macOS and app updates, Safari website data, and more" when space is needed.
- **Categories** ([Apple](https://support.apple.com/guide/mac-help/change-storage-settings-mchl3d437fbc/mac)): Applications, Documents (sortable by Last Accessed and Size), iCloud Drive, iOS Files, TV/Music/Books/Podcasts, Mail, Messages, Music Creation, Photos, Trash, Developer. **Other Users & Shared, macOS and System Data are view-only.**
- **System Data** is a remainder bucket. It includes snapshots, VM swap, caches, Time Machine data and mis-categorized bytes; an Eclectic Light test found 50 of 100 GB of Music counted as System Data ([Eclectic Light](https://eclecticlight.co/2025/02/11/what-is-system-data-in-storage-settings/)).
- **What Lumen can do better:**
  - per-item evidence ("why is this here, who owns it, is it running");
  - clone- and snapshot-aware "freed now / freed after snapshot expiry" numbers;
  - attribution of System Data;
  - a reversible quarantine.
- **What Lumen should not try to beat:** CacheDelete's purge of Apple subsystems. Lumen should *explain* purgeable space and offer "let macOS reclaim it" guidance rather than deleting Apple caches itself.

## Implications for Lumen

1. **Distribution: Developer ID + Hardened Runtime + notarization, macOS 13.0 minimum, universal binary while macOS 26 Intel is supported.**
   - Rationale: full-reach scanning needs non-sandboxed access, a root daemon and login items, all disallowed or constrained by MAS 2.4.5.
   - Rejected: a MAS-only build. A later "Lumen Lite" MAS analyzer limited to user-chosen folders is possible but out of scope.
   - Drop the x86_64 slice only when Lumen drops macOS 26 support.
2. **Scanner adapter: a hand-written `getattrlistbulk` walker in a `lumen-platform-macos` crate.**
   - Pass 1 requests cheap attributes (`FILEID`, `FSID/DEVID`, `FLAGS`, `LINKCOUNT`, `ALLOCSIZE`, `TOTALSIZE`, times, `CLONEID`, `EXT_FLAGS`).
   - Pass 2 requests `PRIVATESIZE` only for `EF_MAY_SHARE_BLOCKS` items above a threshold, and for every cleanup candidate before a plan is shown.
   - Rationale: it is the only bulk source of clone and sharing data, and per-file `URLResourceValues` via objc2 would cost a bridge round-trip per item.
   - Rejected: `walkdir`/`jwalk` + `lstat` (N+1 syscalls, no clone data), `FileManager` enumerator via objc2 (bridging cost, no `CMNEXT` attributes), `fts` (no clone data).
   - Gate clone-group logic per volume on `VOL_CAP_FMT_CLONE_MAPPING` (read via `getattrlist` + `ATTR_VOL_CAPABILITIES` on the mount root); without it, do not group by `CLONEID` (§2).
   - Mitigate DTS's "tricky API" warning with a fuzzed parser, `FSOPT_PACK_INVAL_ATTRS`, an `ATTR_CMN_ERROR` per entry, and a slow-path `lstat` fallback.
3. **Report four numbers per node, never one.**
   - **Logical**.
   - **Allocated**.
   - **Freed now**: Σ private size, with each clone group's shared bytes counted once and only if *all* members are in the plan.
   - **Freed after snapshots expire**: the delta, shown with snapshot dates from `tmutil listlocalsnapshotdates`.

   Hard links: count once per `(fsid, fileid)`, and count as freed only if all links are in the plan. Rationale: "safety > reclaimed space" includes not over-promising.
4. **Volume model.**
   - Walk the **Data volume** (`/System/Volumes/Data`), and stay on one `fsid`.
   - Map to user-facing paths with the firmlink table, or `ATTR_CMNEXT_NOFIRMLINKPATH` in reverse.
   - Show the System volume as a single sealed "macOS" bar.
   - Show three free-space figures (`statfs` free, `ImportantUsage`, `OpportunisticUsage`) with a tooltip explaining purgeable space.
   - Rejected: walking `/` with path-prefix excludes, which is fragile when firmlinks change between OS versions.
5. **Snapshots are REVIEW-only and never part of an automatic plan.**
   - Offer `tmutil thinlocalsnapshots` / `deletelocalsnapshots` only as an explicit, clearly **irreversible** action outside the quarantine system.
   - The UI must explain that deleting files may free nothing until snapshots expire. For Time Machine that is usually 24 h, but the snapshot of the last successful backup is kept "until space is needed" (Apple 102154). Show "freed after snapshots expire" with the actual snapshot dates and owners, not a fixed 24 h promise.
6. **Incremental scanning.**
   - Use a per-device FSEvents stream via `fsevent-sys`, with `kFSEventStreamCreateFlagFileEvents | WatchRoot | MarkSelf | IgnoreSelf`.
   - Persist `(volume UUID, last event ID)` in the local index.
   - On UUID mismatch or ID regression, run a full rescan.
   - On `MustScanSubDirs`, invalidate the subtree.
   - Run a scheduled full sweep (e.g. weekly or on idle).
   - Use `notify` 8.2.0 only for live UI. Re-evaluate after 9.0 final.
7. **Permission model: tri-state evidence.**
   - Every directory node carries `Readable | Denied(TCC|SIP|POSIX|Unknown) | NotScanned`. Denied nodes are shown as "unknown size", never 0, and never feed "orphan" heuristics.
   - Detect FDA by probe and re-probe on app activation.
   - Guide the user with a deep link plus screenshot instructions, and a clear statement of what Lumen reads (metadata only).
   - Do **not** touch Containers or Group Containers without explicit user consent; set `NSAppDataUsageDescription`.
   - Treat the macOS 27-protected Application Support folders (browsers, Discord, crypto wallets; list delivered by XProtect and may grow) as **hard KEEP**: per the third-party analysis, even FDA does not allow writes there, so a quarantine move would fail part-way. Read the allowlist from `AppProtectionRules.plist` when present, and classify any `EPERM` on write/rename as `Denied(TCC)`, never as a cleanup error to retry.
8. **Process topology for TCC.** Keep the Rust core in the main Lumen.app process, as 01-desktop-architecture recommends, so FDA granted to "Lumen" applies. Any agent or daemon is an executable **inside the bundle**, registered via `SMAppService`, with a bundle ID child of the app's and matching signing identifier. Rejected: a bare CLI sidecar, which cannot inherit FDA and was invisible in the FDA UI on 26.1/26.2.
9. **Privileged helper only for system-scope cleanup.**
   - Use an `SMAppService.daemon` with an XPC Mach service.
   - Both ends enforce Team ID plus signing-identifier requirements (`xpc_connection_set_peer_code_signing_requirement` on 13–25, `XPCPeerRequirement.isFromSameTeam(andMatchesSigningIdentifier:)` on 26+).
   - Messages carry **plan IDs** referencing a signed, hashed plan the helper re-validates (path still exists, same `(fsid, fileid)`, not `SF_RESTRICTED`, not under SIP or SSV, policy verdict still QUARANTINE).
   - Ship it lazily: most users never need it, and its approval prompt is a trust cost.
   - Rejected: `SMJobBless` (deprecated), and `AuthorizationExecuteWithPrivileges` (long deprecated; **not re-verified**).
10. **Quarantine store: same-volume `rename(2)` (or `renamex_np` with `RENAME_EXCL`).**
    - Move into a Lumen-owned directory per volume: `~/Library/Application Support/Lumen/Quarantine/<plan-id>/` for the Data volume, and `<vol>/.LumenQuarantine/<uid>/` elsewhere.
    - Record the original path, `(fsid, fileid)`, mode, owner, flags, xattr list, and the clone ID and private size at move time.
    - Restore is a reverse rename after verifying the original parent still exists and nothing new occupies the name.
    - Mark the store `isExcludedFromBackup` and exclude it from Spotlight (`.metadata_never_index`, **unverified on 27**).
    - Rationale: atomic, preserves clones, xattrs, ACLs and file IDs, needs no Finder, and gives a verifiable rollback.
    - Rejected as primary: the Trash (Put Back race bug, no restore API, user auto-empty) and `trash` crate's Finder AppleScript (Automation TCC prompt). Offer "move to Trash" as an optional final step at purge time.
11. **Policy hard rules (deterministic, before any Jev evidence):** KEEP if any of the following holds.
    - The item is on the SSV, or `SF_RESTRICTED`/`SF_NOUNLINK`.
    - It sits under `/System`, `/usr` (except `/usr/local`), `/bin`, `/sbin` or `/private/var/db`.
    - It is a data-vault path.
    - It is `SF_DATALESS` or inside a sync root (`EF_IS_SYNC_ROOT`; iCloud `Mobile Documents`, File Provider domains).
    - Its owning app is running or holds an open fd.
    - It is a Preferences plist.
    - It is a macOS 27 AppData-Detailed-protected Application Support folder (per §10; FDA reportedly does not lift the write block).

    REVIEW-max for Containers, Group Containers, Application Support, iOS backups, launchd items and snapshots. QUARANTINE-eligible for `~/Library/Caches/<id>` of non-running apps, aged logs, Saved Application State of non-running apps, and Xcode DerivedData.
12. **Inventory edges for the evidence graph:**
    - App bundle → bundle ID → Team ID.
    - Bundle ID → `Caches/<id>`, `Application Support/<id>`, `Containers/<id>`, `Preferences/<id>.plist`, `Saved Application State/<id>.savedState`, `HTTPStorages/<id>` **(path unverified)**.
    - App → `Contents/Library/LaunchAgents|Daemons` → executable.
    - Legacy plist → `Program` → code signature Team ID → app.
    - pkg receipt → BOM paths.
    - Running process → executable → bundle.
    - Download → `com.apple.quarantine` agent and `kMDItemWhereFroms`.
    - Orphan detection needs *absence of the app on all mounted volumes* (Spotlight plus LaunchServices plus a `/Applications` scan) and still yields REVIEW only.
13. **Defer to Apple where Apple is authoritative.** For Apple-managed caches, iCloud, Photos, Mail and Messages, link to Storage settings or the owning app instead of deleting. For login items, link to Login Items & Extensions.

## Risks and open questions

- **`getattrlistbulk` correctness and cost.** It needs per-OS validation of `ATTR_CMNEXT_*` availability on 13.x, and benchmarks of `PRIVATESIZE` cost on large trees (the third-party "≈5× slower" figure is unverified).
- **Shared-extent attribution.** There is no public API to find *which* files share blocks with a partially cloned file, so "freed now" for partial clones is exact only per file, not per set. Present it as a lower bound.
- **Snapshot interaction.** Is `PRIVATESIZE = 0` for snapshot-held files guaranteed, and does it account for non-Time-Machine snapshots (third-party backup tools)? This needs empirical tests on 26 and 27.
- **macOS 27 changes are under-documented.** The AppData-Detailed protection for Application Support comes from one security researcher's analysis, which reports that FDA does not lift it and that its app list is updated through XProtect. A quarantine of a folder that becomes protected between plan and execution must fail closed and roll back. Spotlight/Siri changes may alter `mdfind`. Verify on 27.0.1 and the 27.2 betas before GA.
- **FDA detection** relies on probe heuristics that Apple says may change. The deep-link URL is undocumented.
- **BTM format drift.** `sfltool dumpbtm` output and `.btm` versions change every release (v4 → v16). Treat them as optional evidence with schema-tolerant parsing, and keep the plist-based inventory as the source of truth.
- **`launchctl print` output is not API.** Parsing may break across releases.
- **Trash on non-APFS and network volumes.** Behavior of `trashItem` on exFAT, SMB and NFS is unverified. Rename-quarantine needs the same-volume guarantee, so cross-volume moves must be refused or done as copy + verify + delete (non-atomic). The default should be refusal.
- **TCC re-prompts.** Reports suggest App Data consent can be per-process-lifetime (01-desktop-architecture notes this as unverified). Repeated prompts would hurt UX and could push Lumen toward requiring FDA.
- **App Management semantics for deletion** (moving another team's `.app` to Trash or quarantine) are not documented by Apple. Test whether `rename` out of `/Applications` is blocked without App Management on 13–27.
- **Intel support window.** Supporting macOS 26 Intel means universal builds and Rosetta-era testing until Lumen drops 26. 27 is the last release with full Rosetta 2.
- **Open:** should Lumen ship the privileged daemon at all in v1, or limit v1 to user scope and show system-scope findings read-only? The recommendation is user-scope only for v1.
- **Open:** do we need AuthorizationServices per-operation consent in addition to daemon approval for system-scope quarantine (defense in depth vs. prompt fatigue)?

## Sources

- [Wikipedia: macOS Golden Gate](https://en.wikipedia.org/wiki/MacOS_Golden_Gate)
- [UPenn ISC: macOS 27 Golden Gate Released (9/14/2026)](https://isc.upenn.edu/news/macos-27-golden-gate-released-9142026)
- [9to5Mac: macOS 27 Golden Gate full release notes](https://9to5mac.com/2026/09/09/macos-27-golden-gate-here-are-apples-full-release-notes/)
- [MacRumors: Apple Releases macOS Tahoe 26.6](https://www.macrumors.com/2026/07/27/apple-releases-macos-tahoe-26-6/)
- [MacRumors: Apple Releases macOS Tahoe 26.6.1](https://www.macrumors.com/2026/08/06/apple-releases-macos-tahoe-26-6-1/)
- [Apple: Checking Volume Storage Capacity](https://developer.apple.com/documentation/foundation/checking-volume-storage-capacity)
- [Apple: volumeAvailableCapacityForImportantUsageKey](https://developer.apple.com/documentation/foundation/urlresourcekey/volumeavailablecapacityforimportantusagekey)
- [Apple: totalFileAllocatedSize](https://developer.apple.com/documentation/foundation/urlresourcevalues/totalfileallocatedsize)
- [Apple: fileAllocatedSizeKey](https://developer.apple.com/documentation/foundation/urlresourcekey/fileallocatedsizekey)
- [Apple: isExcludedFromBackupKey](https://developer.apple.com/documentation/foundation/urlresourcekey/isexcludedfrombackupkey)
- [getattrlist(2) man page](https://keith.github.io/xcode-man-pages/getattrlist.2.html)
- [getattrlistbulk(2) man page](https://keith.github.io/xcode-man-pages/getattrlistbulk.2.html)
- [clonefile(2) man page](https://keith.github.io/xcode-man-pages/clonefile.2.html)
- [xnu bsd/sys/attr.h](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/sys/attr.h)
- [xnu bsd/sys/stat.h](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/sys/stat.h)
- [Apple File System Guide: Features (archived)](https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/APFS_Guide/Features/Features.html)
- [Apple File System Guide: FAQ (archived)](https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/APFS_Guide/FAQ/FAQ.html)
- [Apple Platform Security: Signed system volume security](https://support.apple.com/guide/security/signed-system-volume-security-secd698747c9/web)
- [Eclectic Light: How macOS depends on firmlinks](https://eclecticlight.co/2023/07/22/how-macos-depends-on-firmlinks/)
- [Eclectic Light: Aren't snapshots purgeable? (2026-08-24)](https://eclecticlight.co/2026/08/24/arent-snapshots-purgeable/)
- [Eclectic Light: Where does macOS get its volume free space figures from?](https://eclecticlight.co/2023/04/27/where-does-macos-get-its-volume-free-space-figures-from/)
- [Eclectic Light: Ventura space management: what gets purged and how?](https://eclecticlight.co/2023/04/19/ventura-space-management-what-gets-purged-and-how/)
- [Eclectic Light: What is System Data in Storage settings?](https://eclecticlight.co/2025/02/11/what-is-system-data-in-storage-settings/)
- [Eclectic Light: What are all those Containers?](https://eclecticlight.co/2024/08/05/what-are-all-those-containers/)
- [Eclectic Light: In the background: Identification (2026-02-20)](https://eclecticlight.co/2026/02/20/in-the-background-identification/)
- [GitHub: cheapsteak/duh (APFS-clone-aware du)](https://github.com/cheapsteak/duh)
- [GitHub: rustClean PR #32 "Count APFS clones once"](https://github.com/hzrbasaran/rustClean/pull/32)
- [Apple Developer Forums 760256: recommended way to count files](https://developer.apple.com/forums/thread/760256)
- [Views of a Coder: Performance considerations when reading directories on macOS](http://blog.tempel.org/2019/04/dir-read-performance.html)
- [Apple: kMDItemLastUsedDate](https://developer.apple.com/documentation/coreservices/kmditemlastuseddate)
- [Apple Developer Forums 20639: kMDItemLastUsedDate not updated](https://developer.apple.com/forums/thread/20639)
- [mdfind: Spotlight from the command line](https://everywherefast.com/blog/mdfind-command-guide)
- [Apple: FSEventStreamCreate](https://developer.apple.com/documentation/coreservices/1443980-fseventstreamcreate)
- [Apple: FSEventStreamEventFlags](https://developer.apple.com/documentation/coreservices/1455361-fseventstreameventflags)
- [Apple: File System Events Programming Guide: Using the FSEvents Framework](https://developer.apple.com/library/archive/documentation/Darwin/Conceptual/FSEvents_ProgGuide/UsingtheFSEventsFramework/UsingtheFSEventsFramework.html)
- [crates.io: notify](https://crates.io/crates/notify) and [docs.rs: notify](https://docs.rs/notify/latest/notify/)
- [Apple: File System Programming Guide: macOS Library Directory Details](https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/FileSystemProgrammingGuide/MacOSXDirectories/MacOSXDirectories.html)
- [Apple: Daemons and Services Programming Guide: Creating Launch Daemons and Agents](https://developer.apple.com/library/archive/documentation/MacOSX/Conceptual/BPSystemStartup/Chapters/CreatingLaunchdJobs.html)
- [Apple: SMAppService](https://developer.apple.com/documentation/servicemanagement/smappservice)
- [Apple: Updating helper executables from earlier versions of macOS](https://developer.apple.com/documentation/servicemanagement/updating-helper-executables-from-earlier-versions-of-macos)
- [Apple: SMJobBless (deprecated)](https://developer.apple.com/documentation/servicemanagement/smjobbless(_:_:_:_:))
- [Apple Platform Deployment: Manage login items and background tasks on Mac](https://support.apple.com/guide/deployment/manage-login-items-background-tasks-mac-depdca572563/web)
- [GitHub: objective-see/DumpBTM](https://github.com/objective-see/DumpBTM)
- [Yogesh Khatri: mac_apt update to BTM processing](http://www.swiftforensics.com/2025/01/macapt-update-to-btm-processing.html)
- [Apple: NSXPCConnection.setCodeSigningRequirement(_:)](https://developer.apple.com/documentation/foundation/nsxpcconnection/setcodesigningrequirement(_:))
- [Apple: xpc_connection_set_peer_code_signing_requirement](https://developer.apple.com/documentation/xpc/xpc_connection_set_peer_code_signing_requirement(_:_:))
- [Apple: XPCPeerRequirement](https://developer.apple.com/documentation/xpc/xpcpeerrequirement)
- [GitHub: trilemma-dev/SwiftAuthorizationSample](https://github.com/trilemma-dev/SwiftAuthorizationSample)
- [Apple Support: Change Privacy & Security settings on Mac](https://support.apple.com/guide/mac-help/change-privacy-security-settings-on-mac-mchl211c911f/mac)
- [Apple Developer Forums 114452: Reliable test for Full Disk Access?](https://developer.apple.com/forums/thread/114452)
- [Apple Developer Forums 107546: The Rules for Full Disk Access](https://developer.apple.com/forums/thread/107546?page=1)
- [Apple Developer Forums 806187: macOS 26.1 FDA UI bug (CLI tools)](https://developer.apple.com/forums/thread/806187)
- [Apple Developer Forums 809549: Emerging issue with macOS Tahoe 26.1 FDA](https://developer.apple.com/forums/thread/809549)
- [Wojciech Reguła: Crossing the Golden Gate: macOS's New Application Support Protection](https://wojciechregula.blog/post/golden-gate-appdata-protection/)
- [Lapcat Software: How macOS Ventura App Management works and doesn't work](https://lapcatsoftware.com/articles/AppManagement.html)
- [bvanpeski/SystemPreferences: Ventura pane URLs](https://github.com/bvanpeski/SystemPreferences/blob/main/macos_preferencepanes-Ventura.md)
- [Apple: App Review Guidelines](https://developer.apple.com/app-store/review/guidelines/)
- [Apple: Notarizing macOS software before distribution](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution)
- [Apple Platform Security: Gatekeeper and runtime protection](https://support.apple.com/guide/security/gatekeeper-and-runtime-protection-sec5599b66df/web)
- [Apple Developer News: Updates to runtime protection in macOS Sequoia](https://developer.apple.com/news/?id=saqachfa)
- [Apple Support: About System Integrity Protection on your Mac (102149)](https://support.apple.com/en-us/102149)
- [Apple: FileManager.trashItem(at:resultingItemURL:)](https://developer.apple.com/documentation/foundation/filemanager/trashitem(at:resultingitemurl:))
- [Apple Developer Forums 773997: trashItem, recycle, but no put back option](https://developer.apple.com/forums/thread/773997)
- [GitHub: abrignoni/DLEAPP PR #334 (Trash Put Back .DS_Store)](https://github.com/abrignoni/DLEAPP/pull/334)
- [GitHub: Byron/trash-rs src/macos/mod.rs](https://github.com/Byron/trash-rs/blob/master/src/macos/mod.rs)
- [Apple: NSWorkspace.runningApplications](https://developer.apple.com/documentation/appkit/nsworkspace/runningapplications)
- [docs.rs: libproc](https://docs.rs/libproc/latest/libproc/)
- [GitHub: giampaolo/psutil #883 (proc_pidinfo AccessDenied)](https://github.com/giampaolo/psutil/issues/883)
- [SS64: pkgutil](https://ss64.com/mac/pkgutil.html)
- [SS64: tmutil](https://ss64.com/mac/tmutil.html)
- [Apple Support: Free up storage space on Mac](https://support.apple.com/guide/mac-help/free-up-storage-space-on-mac-sysp4ee93ca4/mac)
- [Apple Support: Change Storage settings on Mac](https://support.apple.com/guide/mac-help/change-storage-settings-mchl3d437fbc/mac)
- [Apple: About the security content of macOS Tahoe 26.7 (149042)](https://support.apple.com/en-us/149042)
- [MacRumors: Apple Releases macOS Tahoe 26.7 and macOS Sequoia 15.8](https://www.macrumors.com/2026/09/14/apple-releases-macos-tahoe-26-7/)
- [MacRumors: Apple Seeds Third macOS Golden Gate 27.2 Beta](https://www.macrumors.com/2026/10/05/apple-seeds-macos-27-2-beta-3/)
- [Apple Support: About Time Machine local snapshots (102154)](https://support.apple.com/en-us/102154)
- [Apple Developer Forums 750484: SMAppService items and Full Disk Access responsibility](https://developer.apple.com/forums/thread/750484)
- [GitHub: notify-rs/notify `notify/src/fsevent.rs` (main)](https://github.com/notify-rs/notify/blob/main/notify/src/fsevent.rs)
- crates.io API (`https://crates.io/api/v1/crates/<name>`) for notify, fsevent-sys, objc2, objc2-foundation, objc2-service-management, objc2-core-services, libproc, sysinfo, plist, xattr, trash, security-framework, core-foundation, jwalk, walkdir (queried 2026-10-05)

## Verification log

Adversarial fact-check performed 2026-10-05/06. Verdicts: **confirmed**, **corrected** (text edited in place), **unverified** (left marked).

| # | Claim | Verdict | Source |
| --- | --- | --- | --- |
| 1 | macOS 27 Golden Gate released 2026-09-14, now 27.0.1 (2026-09-28), Apple silicon only, last release with full Rosetta 2, AFP removed | confirmed | [Wikipedia](https://en.wikipedia.org/wiki/MacOS_Golden_Gate) |
| 2 | 27.2 beta 3 seeded 2026-10-05 (Apple skipped a 27.1 beta) | confirmed | [MacRumors](https://www.macrumors.com/2026/10/05/apple-seeds-macos-27-2-beta-3/) |
| 3 | macOS Tahoe 26.7 released 2026-09-14 (was marked unverified) | corrected: confirmed as security-only build 25G229; Sequoia 15.8 the same day | [Apple 149042](https://support.apple.com/en-us/149042), [MacRumors](https://www.macrumors.com/2026/09/14/apple-releases-macos-tahoe-26-7/) |
| 4 | All 15 crate versions and update dates in §1 (notify 8.2.0 / 9.0.0-rc.5, objc2 0.6.5, objc2-* 0.3.2, libproc 0.14.11, sysinfo 0.39.6, plist 1.10.1, trash 5.2.9, etc.); notify 8.2.0 dated 2025-08-03, rc.1 2026-01-25 | confirmed | crates.io API, queried 2026-10-05 |
| 5 | `notify` cannot resume from a persisted FSEvents ID or use per-device streams (was unverified) | corrected: confirmed in source (`since_when` hard-coded to `SinceNow`, `FSEventStreamCreate`) | [notify fsevent.rs](https://github.com/notify-rs/notify/blob/main/notify/src/fsevent.rs) |
| 6 | FSEvents guide: per-disk streams for persistence, "advisory", MustScanSubDirs set on drops, monitor before scanning, persistent IDs | confirmed (verbatim) | [FSEvents guide](https://developer.apple.com/library/archive/documentation/Darwin/Conceptual/FSEvents_ProgGuide/UsingtheFSEventsFramework/UsingtheFSEventsFramework.html) |
| 7 | `ATTR_CMNEXT_*` constants, `PRIVATESIZE`/`CLONEID`/`CLONE_REFCNT` man-page wording; `st_flags` values (`UF_COMPRESSED`, `SF_RESTRICTED`, `SF_NOUNLINK`, `SF_FIRMLINK`, `SF_DATALESS`) | confirmed; added note that `EF_*` live in `stat.h` | xnu `bsd/sys/attr.h`, `bsd/sys/stat.h`; local `man getattrlist` (macOS 26.6) |
| 8 | Clone metadata usable on any APFS volume | corrected (omission): `CLONEID`/`CLONE_REFCNT` are tied to `VOL_CAP_FMT_CLONE_MAPPING`; gate per volume | `getattrlist(2)` man page, xnu `attr.h` |
| 9 | Minimum OS for each `ATTR_CMNEXT_*` attribute | unverified (no Apple availability data found) | — |
| 10 | DTS: getattrlistbulk "not an API I'd recommend building on"; 357,248-file timings; autoreleasepool 491 MB → 6.3 MB | confirmed | [forums 760256](https://developer.apple.com/forums/thread/760256) |
| 11 | `SMJobBless` deprecated in 13.0 ("Please use SMAppService instead"); `SMAppService` and `openSystemSettingsLoginItems()` 13.0+; `NSXPCConnection.setCodeSigningRequirement` 13.0+; `xpc_connection_set_peer_code_signing_requirement` 12.0+; `XPCPeerRequirement` and `isFromSameTeam(andMatchesSigningIdentifier:)` 26.0+; `urlsForApplications(withBundleIdentifier:)` 12.0+ (was unverified) | confirmed | Apple documentation JSON metadata |
| 12 | SMAppService helpers inherit the app's FDA; bare CLI tools do not | corrected: confirmed for SMAppService with a primary Quinn quote; clarified that legacy `/Library/PrivilegedHelperTools` helpers do not inherit | [forums 750484](https://developer.apple.com/forums/thread/750484), [forums 107546](https://developer.apple.com/forums/thread/107546?page=1) |
| 13 | No API to check FDA; "do what you're really trying to do and then handle errors"; TCC DB location is not API | confirmed | [forums 114452](https://developer.apple.com/forums/thread/114452) |
| 14 | macOS 26.1/26.2 hid CLI tools from the FDA list; grants worked; fixed in 26.3 beta 1 | confirmed | [forums 806187](https://developer.apple.com/forums/thread/806187) |
| 15 | macOS 27 Application Support protection (`kTCCServiceSystemPolicyAppDataDetailed`) | corrected: the source reports it blocks **writes** and that **FDA does not bypass it**; upgraded to hard KEEP in §10 and Implications 7 and 11. Still third-party only, so unverified against Apple docs | [Reguła](https://wojciechregula.blog/post/golden-gate-appdata-protection/) |
| 16 | Group Containers protected since macOS 15 with "same" grant path as Containers | corrected: Eclectic Light describes SIP-backed, group-member-only access; whether consent or FDA unlocks reads for Lumen is unverified | [Eclectic Light](https://eclecticlight.co/2024/08/05/what-are-all-those-containers/) |
| 17 | App Management triggers on deleting another developer's app | corrected: the cited analysis covers modification only; deletion is unverified | [Lapcat](https://lapcatsoftware.com/articles/AppManagement.html) |
| 18 | Guideline 2.4.5 (i) sandbox, (iii) no auto-launch without consent, (v) no root escalation, (vii) MAS-only updates | confirmed | [App Review Guidelines](https://developer.apple.com/app-store/review/guidelines/) |
| 19 | Sequoia removed the Control-click Gatekeeper override | confirmed (Apple news, 2024-08-06) | [Apple Developer News](https://developer.apple.com/news/?id=saqachfa) |
| 20 | SIP protects `/System`, `/usr`, `/bin`, `/sbin`, `/var` and preinstalled apps; third parties may write to `/Applications`, `/Library` and `/usr/local` | confirmed | [Apple 102149](https://support.apple.com/en-us/102149) |
| 21 | Trash Put Back bug r. 23153124; DTS "confirmed a race" | corrected: the bug is confirmed ("known for at least 10 years"), but the `.DS_Store` race is only Quinn's suspicion | [forums 773997](https://developer.apple.com/forums/thread/773997) |
| 22 | Snapshot purgeability got worse by Tahoe (tested on 26.6.2) | confirmed | [Eclectic Light 2026-08-24](https://eclecticlight.co/2026/08/24/arent-snapshots-purgeable/) |
| 23 | Time Machine auto-purges its snapshots after about 24 h | corrected: Apple also keeps the last successful backup's snapshot "until space is needed" | [Apple 102154](https://support.apple.com/en-us/102154) |
| 24 | BTM store is `BackgroundItems-v16.btm` on 26; `sudo sfltool dumpbtm`; `attributions.plist` | confirmed (v4/v7 and v13 only via the cited third parties) | [Eclectic Light 2026-02-20](https://eclecticlight.co/2026/02/20/in-the-background-identification/) |
| 25 | All cited URLs resolve | confirmed: every GitHub, Apple doc/support, forum, ss64, MacRumors, 9to5Mac, UPenn, Lapcat, Tempel and swiftforensics link returned HTTP 200. Eclectic Light returns 403 to curl but loaded through the fetcher | HTTP checks 2026-10-05 |
| 26 | "≈5× slower" `PRIVATESIZE` cost; `PRIVATESIZE = 0` for snapshot-held files; `.metadata_never_index` on 27; `com.apple.quarantine` format; FDA deep-link URL on 27 | unverified (left marked) | — |
