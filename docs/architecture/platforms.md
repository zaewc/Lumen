# Platform architecture

> Status: accepted baseline, 2026-10-06. Detailed evidence lives in the research notes
> linked per section. Capability names refer to `PlatformCapabilities`
> ([ADR-0008](../decisions/0008-capability-model-and-coverage.md)).

Lumen promises on each platform only what that platform allows. This document is the
contract between the core, the platform adapters, the UI and product claims.

## Summary matrix

| Capability | macOS | Windows | Android | iOS |
| --- | --- | --- | --- | --- |
| Enumerate user files system-wide | yes (FDA for protected areas) | yes | shared storage only (T3) | no (own container, picked folders) |
| Other apps' caches: inspect | yes | yes | sizes only (T1, `StorageStatsManager`) | no |
| Other apps' caches: clear | quarantine (user scope) | quarantine (user scope) | guided via system settings; external caches all-or-nothing (T3) | no |
| Quarantine by rename | yes (APFS, no-replace) | yes (NTFS, no-replace) | shared storage, same volume (T3) | no |
| Media trash | — | — | `MediaStore.createTrashRequest` (T2) | PhotoKit delete → Recently Deleted |
| Cloud placeholders | detect, never hydrate; evict via provider | detect, never hydrate; dehydrate via provider | — | iCloud Drive evict |
| Inventory apps/services | bundles, receipts, launchd plists, BTM (optional) | Uninstall keys, MSI, AppX, Run keys, Task Scheduler, SCM | packages (T1, `QUERY_ALL_PACKAGES`) | no |
| Change feed | FSEvents (persisted) | `ReadDirectoryChangesW`; USN (helper) | MediaStore generations | PhotoKit change tokens |
| System scope | read-only in v1 | read-only in v1 | no | no |
| On-device Jev | Foundation Models (26+) | ONNX Runtime / llama.cpp (later) | ML Kit Prompt API (foreground, beta) | Foundation Models (26+) |

## macOS

Research: [03-macos-platform](../research/03-macos-platform.md),
[01-desktop-architecture](../research/01-desktop-architecture.md).

**Targets and distribution:**

- macOS 13+ (`SMAppService`).
- Test on 13, 14, 15, 26 (arm64 and x86_64) and 27.
- Universal binary while macOS 26 on Intel is supported.
- Developer ID, Hardened Runtime, notarization; not the Mac App Store
  ([ADR-0006](../decisions/0006-desktop-distribution-and-signing.md)).

**Process topology:**

- v1: `Lumen.app` (Tauri host + core).
- v1.x: `lumen-agent` inside the bundle via `SMAppService.agent`.
- Later: `lumen-helper` via `SMAppService.daemon` + XPC peer requirements
  ([ADR-0005](../decisions/0005-process-topology-and-privilege.md)).

**Scanning (`lumen-platform-macos`):**

- Walk the Data volume (`/System/Volumes/Data`) once, on one `fsid`. De-duplicate
  firmlinks by identity. Show the sealed system volume as one "macOS" figure.
- `getattrlistbulk`, breadth-first, one directory open per task, 32 KB buffers.
  Request `ATTR_CMNEXT_PRIVATESIZE` only for items that may share blocks and for
  candidates.
- Gate clone grouping on `VOL_CAP_FMT_CLONE_MAPPING`. Treat missing attributes on
  non-APFS volumes as unknown.
- `SF_DATALESS` and sync-root items are never opened.
  `IOPOL_MATERIALIZE_DATALESS_FILES_OFF` is set process-wide.
- Four size figures per node: logical, allocated, freed now, and freed after snapshots
  expire (with `tmutil listlocalsnapshotdates`).
- Three free-space figures: `statfs` free, `ImportantUsage`, and `OpportunisticUsage`.

**Permissions:**

- There is no API to check Full Disk Access; Lumen probes and handles errors.
- Directories carry `Readable | Denied(Tcc | Sip | Posix) | NotScanned`.
- Containers and Group Containers need consent (`NSAppDataUsageDescription`) and are
  `REVIEW` at most.
- Application Support folders that macOS 27 protects (AppData-Detailed protection) are
  hard `KEEP`: Full Disk Access reportedly does not lift the write block, so a move
  would fail partway.

**Inventory and evidence:**

- App bundles → bundle ID → Team ID.
- `~/Library/{Caches, Application Support, Containers, Preferences, Saved Application State, HTTPStorages}/<id>`.
- `Contents/Library/LaunchAgents|LaunchDaemons`; launchd plists in the five standard
  directories.
- `pkgutil` receipts.
- `libproc` processes and open files.
- `com.apple.quarantine` and `kMDItemWhereFroms` for downloads.
- BTM (`sfltool dumpbtm`) is optional, schema-tolerant evidence.

**Quarantine:**

- `renamex_np(RENAME_EXCL)` into `~/Library/Application Support/Lumen/Quarantine/`,
  or `<volume>/.LumenQuarantine/<uid>/`.
- Excluded from backups and indexing.
- The Trash is only an optional release step, via `FileManager.trashItem` called
  directly, recording the resulting URL.

**Hard `KEEP`:**

- The sealed system volume, `SF_RESTRICTED` / `SF_NOUNLINK`, `/System`, `/usr` (except
  `/usr/local`), `/bin`, `/sbin`, `/private/var/db`.
- Data vaults, Preferences plists, items of running apps, keychains, and SSH/GPG
  material.

**Defer to Apple:** iCloud, Photos, Mail and Messages storage, and login items. Lumen
links to System Settings instead of acting.

**Snapshots:** `REVIEW` only. Thinning or deleting snapshots is an explicit,
irreversible action outside quarantine.

## Windows

Research: [04-windows-platform](../research/04-windows-platform.md).

**Targets and distribution:**

- Windows 10 22H2 and Windows 11 24H2+.
- Signed MSI/NSIS (`currentUser`) with Azure Artifact Signing; no MSIX-only build.

**Process topology:**

- v1: `lumen.exe` (`asInvoker`).
- v1.x: `lumen-agent` as a Task Scheduler logon task.
- Later: `lumen-elevate.exe`, launched on demand via `runas` with a signed plan file.
  An optional `LumenService` would need its own ADR.
- The elevated side never uses its own `HKCU`, `%TEMP%` or known folders; it gets user
  context from the unelevated core (Administrator protection).

**Bindings:**

- `windows-sys` 0.61 for FFI and `windows` 0.62 for COM/WinRT, isolated in
  `lumen-platform-windows`.
- A migration spike is planned for windows-rs 0.100.
- COM work runs on dedicated STA threads.

**Scanning:**

- Directory handles (`FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT`) and
  `GetFileInformationByHandleEx(FileIdExtdDirectoryInfo)`, with a fallback to
  `FileIdBothDirectoryInfo`.
- `RtlSetProcessPlaceholderCompatibilityMode(PHCM_EXPOSE_PLACEHOLDERS)` at start.
- Reparse points are recorded as edges and never traversed.
- Hard links are de-duplicated by `(volume serial, FileId)`.
- Phase 2 re-verification of candidates by handle: `FileStandardInfo`, `FileIdInfo`,
  `GetCompressedFileSizeW`, and placeholder state.
- MFT/USN fast paths only via the elevated helper (later).

**Cloud files:**

- Sync roots and `RECALL_ON_*` items are `KEEP`.
- The only space action is dehydration, delegated to the provider or Storage Sense.

**In-use gate:** Restart Manager (`RmGetList`), one session per batch, files only.
Lumen never calls `RmShutdown`.

**Quarantine:**

- `SetFileInformationByHandle(FileRenameInfoEx)` on the verified handle into
  `X:\$LumenQuarantine\<UserSID>\` with an owner-only DACL.
- Manifests are kept in the database and as sidecar files.
- Recycle Bin release is optional; Storage Sense may purge it, so the UI does not call
  it recoverable.

**Hard deny:**

- `%WINDIR%\WinSxS`, `System32`, `SysWOW64`, `\Windows\Installer`.
- `pagefile.sys`, `hiberfil.sys`, `swapfile.sys`.
- `System Volume Information`, other users' `$Recycle.Bin`, `$Extend`.
- `SoftwareDistribution\DataStore`, `catroot2`.
- Sync roots, and the install directories of installed apps (`REVIEW`).

**Delegate:** DISM `/StartComponentCleanup` (never `/ResetBase` in one-click flows),
`powercfg` for hibernation, and the Delivery Optimization cmdlet. Disk Cleanup handler
registrations (`VolumeCaches`) are high-trust vendor evidence.

**Inventory:**

- Installed apps: Uninstall keys (three views), MSI (`MsiEnumProductsExW`), AppX
  (`FindPackagesForUser`), and optional winget enrichment.
- Startup: Run/RunOnce, Startup folders, `StartupApproved`, Task Scheduler (hidden
  included), and services and drivers (report-only).
- Disabling uses reversible toggles with per-item consent. Uninstall is never a
  quarantine action.

## Android

Research: [05-android-platform](../research/05-android-platform.md).

**Targets:** `targetSdk 36` (Play requirement since 2026-08-31), `minSdk 30`, and
ABIs `arm64-v8a` and `x86_64` with 16 KB page alignment.

**Capability tiers** (recorded in every coverage report):

| Tier | Grant | Lumen can |
| --- | --- | --- |
| T0 | none | volume totals, own storage, Photo Picker spot checks, `ACTION_MANAGE_STORAGE` |
| T1 | Usage access (`PACKAGE_USAGE_STATS`, + `QUERY_ALL_PACKAGES`) | per-app app/data/cache bytes, last used, unused-app findings, deep links to app settings |
| T2 | `READ_MEDIA_*` (possibly partial) | large and duplicate media, trash-based reversible cleanup |
| T3 | All files access (`MANAGE_EXTERNAL_STORAGE`) | shared-storage tree, residual folders, APKs, `ACTION_CLEAR_APP_CACHE` (external caches, all-or-nothing) |

Rules:

- No app can clear another app's internal cache (since API 23). Lumen offers a
  **guided** flow: rank by `cacheBytes`, deep-link to the app's settings, and verify by
  re-querying.
- No AccessibilityService in the Play build. Play does not count cleaners as
  accessibility tools, forbids autonomous accessibility actions, and Android 17
  Advanced Protection revokes accessibility access.
- Media quarantine uses `createTrashRequest`, within the item's `DATE_EXPIRES`.
- Non-media quarantine (T3) uses a same-volume rename into
  `Documents/Lumen/.quarantine/`, never app-specific storage, which is wiped on
  uninstall.
- Unused apps: `requestArchive` (keeps data) only when the installer is present and
  enabled; the UI shows the restore conditions.
- Background work: periodic WorkManager jobs (idle, charging); user-initiated deep
  scans in a `dataSync` foreground service with `onTimeout` checkpointing; never
  started from boot.
- Private space, work profiles and `Android/data` are out of scope and say so.
  Truncated package lists disable residual-data findings.
- The Play listing is positioned as file management and maintenance; declaration forms
  are prepared early. The product must remain useful at T1/T2 if T3 is denied.

## iOS

Research: [06-ios-and-mobile-framework](../research/06-ios-and-mobile-framework.md).

**Product:** "Lumen for Photos & Files". Lumen does not claim system cleaning.

**Targets:** iOS 16.4+ (Expo SDK 56+), with newer APIs enabled behind `#available`.

Adapters (Expo Modules in Swift, calling the Rust core via UniFFI):

- `IosVolumeCapacityAdapter`: important-usage capacity for display.
- `PhotoKitAdapter`:
  - fetch, persistent change tokens;
  - sizes via `PHAssetResource.dataSize` on iOS 27+, labelled estimates on earlier
    versions, no private KVC;
  - Vision feature prints for near-duplicates;
  - batched `performChanges` deletes, one system prompt per user decision.
- `UserFolderAdapter`: security-scoped bookmarks, coordinated enumeration, iCloud
  status, eviction.
- `OwnContainerAdapter`: Lumen's own caches.

Rules:

- Deleting a photo with iCloud Photos on deletes it on every device, and with Optimize
  Storage it frees less than the original's size. Space returns only after Recently
  Deleted is purged. The confirmation UI states all three.
- Background work: `BGContinuedProcessingTask` (iOS 26+, user-initiated, with progress)
  for library analysis; `BGProcessingTask` while charging. Nothing destructive runs in
  the background.
- App Review: no "virus", "boost" or "cleaner" claims; exact or labelled-estimate
  numbers only; no scare UI; a privacy manifest with Required Reason APIs (disk space,
  file timestamps), covering the Rust core's syscalls too.
- Jev: an on-device `FoundationModelsJudge` (`@Generable`, `contextSize` read at
  runtime). Private Cloud Compute is a cloud-tier opt-in.

## Web dashboard

See [ADR-0010](../decisions/0010-web-dashboard-role.md): the same SPA as the desktop
UI, served by the local core on loopback only when the user enables it, with
`Host`/`Origin` validation, a bearer token, no CORS, and strict CSP. There is no cloud
backend in v1.

## Adding a platform

1. Add `lumen-platform-<name>` implementing the ports it can support.
2. Declare `PlatformCapabilities` honestly; unsupported actions are refused by the
   application layer.
3. Add fixture-based adapter tests and a CI job.
4. Document scope here and add an ADR if the platform needs a new process or
   privilege boundary.

No change to `lumen-domain`, `lumen-policy` or `lumen-graph` should be needed. If one
is, that is a design smell to raise in review.
