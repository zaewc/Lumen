# Windows Platform Integration for Lumen (Scanning, Change Tracking, Cleanup, Inventory, Privilege, Distribution)

> Researched: 2026-10-05 · Scope: Win32/WinRT/COM APIs, Rust bindings, OS cleanup facilities, installed-app/startup inventory, privilege split, packaging/signing for a safety-first storage analysis and reversible cleanup tool on Windows 10 22H2 / Windows 11 24H2–26H2.

## Summary

- **Rust bindings are mid-transition.** Latest published `windows` crate is still **0.62.2** and `windows-sys` **0.61.2** (both 2025-10-06), but `windows-core` **0.100.0** shipped 2026-09-03 as part of a breaking "0.100" reset. That reset moves to new in-house metadata, Rust 2024, MSRV 1.95, and lowercase header-based features. The maintainers say `windows`/`windows-sys` 0.100 "may initially" ship late. **Pin 0.62/0.61 now and keep every Win32 call behind a Lumen-owned adapter crate** so the 0.100 migration stays in one place.
- **Default scan path needs no admin and reads only metadata:** open each directory with `FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT`, then call `GetFileInformationByHandleEx(FileIdExtdDirectoryInfo / FileIdBothDirectoryInfo)`. That returns logical size (`EndOfFile`), size on disk (`AllocationSize`), attributes, the file ID and (Extd) the reparse tag with no per-file opens. No file opens means no OneDrive hydration and no AV scan storms.
- **Fast MFT/USN scanning needs administrator rights.** `FSCTL_ENUM_USN_DATA` and `FSCTL_READ_USN_JOURNAL` need a `\\.\X:` volume handle on NTFS. Everything and WizTree both fall back to slow enumeration, or ship an elevated service, for exactly this reason. USN records carry names, parents, attributes and 128-bit IDs (V3) but **no file sizes**, so they are a tree and change source, not a size source.
- **Hard links make naive totals wrong.** Dedupe by `(volume serial, FileId)` while enumerating, so that bytes freed are only counted when the last link goes. Microsoft's WinSxS docs show Explorer over-reporting because of hard links.
- **OneDrive and other cloud placeholders are the biggest data-loss trap.** Detect them from directory data alone (`FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS`, `RECALL_ON_OPEN`, `OFFLINE`, `IO_REPARSE_TAG_CLOUD_*`, `CfGetPlaceholderStateFromFindData`). Never read their content. Never move or delete inside a sync root: a deletion syncs to the cloud. Freeing space means "dehydrate", not "delete".
- **Do not use the Recycle Bin as Lumen's quarantine.** There is no documented restore API: restore means invoking the shell `undelete` verb, which may show UI. Files can be permanently deleted when the bin can't take them. Lumen should quarantine by **same-volume atomic rename** into a Lumen-owned per-volume store. The Recycle Bin (`IFileOperation` + `FOFX_RECYCLEONDELETE`, Win8+) is an optional, user-visible final step only.
- **The OS cleanup surfaces are mostly "delegate, don't touch":**
  - WinSxS: never delete files. Use DISM `/StartComponentCleanup`. `/ResetBase` is irreversible.
  - SoftwareDistribution: Microsoft only documents stop-service-then-rename/delete flows.
  - Delivery Optimization: self-managed by default (3-day max age, 20% max size); `Delete-DeliveryOptimizationCache` exists.
  - hiberfil.sys: use `powercfg /hibernate` (off, `/type reduced`).
  - pagefile.sys: never.
- **Disk Cleanup handler registrations are free, vendor-authored evidence.** `HKLM\...\Explorer\VolumeCaches` entries (DataDrivenCleaner `Folder`, `FileList`, `LastAccess`, `Flags`) are each vendor's own declaration of which files are safe to delete. Feed them into the evidence graph as high-trust "vendor-declared cache" edges. Disk Cleanup is still not on Microsoft's deprecated-features list as of 2026-09-23.
- **Installed apps need four sources unioned:**
  - Uninstall keys: HKLM 64-bit, HKLM WOW6432Node, HKCU, plus other users' hives when elevated.
  - MSI `MsiEnumProductsExW`: all users needs admin.
  - MSIX/AppX `PackageManager.FindPackagesForUser`: another user's SID needs admin.
  - winget (CLI 1.29.380, COM API via `Microsoft.WindowsPackageManager.ComInterop`): optional enrichment only.
- **Startup inventory:**
  - Run/RunOnce (4 documented keys plus WOW64 views).
  - Startup folders.
  - `Explorer\StartupApproved\*`: undocumented but what Task Manager uses, and the reversible way to disable.
  - Task Scheduler `ITaskFolder::GetTasks(TASK_ENUM_HIDDEN)`.
  - Services and drivers via `EnumServicesStatusExW`: silently omits services you can't query.
- **Privilege model:** run UI and core unelevated (`asInvoker`). Do privileged work in a separate elevated component behind a hardened named pipe. The pipe needs an explicit DACL (the default grants Everyone/anonymous read), client PID-to-image-to-signature verification, impersonation for access checks, and a plan-ID-only command surface. **Administrator protection** (Windows 11, KB5120998, Aug 2026, off by default) runs elevated processes in a separate hidden profile with no auto-elevation. An elevated helper must never infer "the user" from its own HKCU, %TEMP% or Recycle Bin.
- **Packaging and signing:** ship a signed MSI/EXE, not MSIX-only. MSIX bans drivers and per-user services, virtualizes HKLM and AppData, and requires `packagedServices`/`localSystemServices` restricted capabilities for services, which Microsoft says it usually won't approve for Store apps. MSIX apps that need elevation are rejected from the Store unless Microsoft approves the restricted `allowElevation` capability, and it grants that only under strict criteria. Store listings of MSI/EXE apps (policy 10.2.9) may show a UAC prompt at install. Policy 10.2.4 generally disallows depending on non-Microsoft NT services, and it forbids changing Windows settings through undocumented APIs. Sign everything with **Azure Artifact Signing** (formerly Trusted Signing): $9.99/month Basic. Organizations can be validated in the US, CA, EU, UK, AU, NZ, JP, KR, SG, CH, NO and IL; individuals only in the US/CA. **Neither EV nor Artifact Signing bypasses SmartScreen any more**; reputation builds over time.
- **WinUI 3 is healthy but not needed.** Windows App SDK stable is **2.5.1** (2026-09-16). 1.8 reached end of servicing on 2026-09-24, so only 2.x is supported now. A cross-platform Lumen UI on WebView2/Tauri loses nothing material by skipping WinUI.

## Findings

### 1. Rust bindings: `windows` / `windows-sys` (status as of 2026-10-05)

| Crate | Latest published | Date | Notes |
|---|---|---|---|
| `windows` | **0.62.2** | 2025-10-06 | Depends on `windows-core ^0.62.2` ([crates.io API](https://crates.io/api/v1/crates/windows), [docs.rs](https://docs.rs/crate/windows/latest)) |
| `windows-sys` | **0.61.2** | 2025-10-06 | `raw-dylib` used unconditionally since 0.61.0 (release 69) ([crates.io API](https://crates.io/api/v1/crates/windows-sys), [releases](https://github.com/microsoft/windows-rs/releases)) |
| `windows-core` | **0.100.0** | 2026-09-03 | First crate of the 0.100 reset ([crates.io API](https://crates.io/api/v1/crates/windows-core)) |

The "Rust for Windows – August 2026" tracking issue ([windows-rs#4867](https://github.com/microsoft/windows-rs/issues/4867)) states:
- The supported crates move to a common **0.100.0**, Rust 2024, with **MSRV Rust 1.95**. Exceptions: `windows-sys` keeps MSRV 1.88 and `windows-link` 1.85.
- Win32Metadata is replaced by an in-house pipeline: "Windows headers → RDL → winmd → Rust bindings".
- **Breaking:** module paths become lowercase header-style names. `windows-sys` replaces all `Win32_*` features with lowercase header-based features. Generated enum/ownership wrappers are removed where the SDK declares primitives.
- "We may initially publish the release without `windows` and `windows-sys`."
- Libraries are advised to avoid the umbrella crates and use focused crates or local `windows-bindgen`.

A downstream issue confirms that, at present, no published `windows` release uses `windows-core` 0.100. Mixing `windows-future` 0.100 with `windows` 0.62 does not compile ([example](https://github.com/PeterShanxin/Meowcal-Sub/issues/232)). Release 74 (3 Sep) adds new crates such as `windows-reference`, `windows-time`, `windows-canvas` and `windows-reactor` ([release 74](https://github.com/microsoft/windows-rs/releases/tag/74)).

Verified 0.62 feature names Lumen needs ([docs.rs features](https://docs.rs/crate/windows/0.62.2/features)):

| Feature | Covers |
|---|---|
| `Win32_Storage_FileSystem` | `FindFirstFileExW`, `GetFileInformationByHandleEx`, `GetCompressedFileSizeW` |
| `Win32_System_IO` | `DeviceIoControl` |
| `Win32_System_Ioctl` | `FSCTL_*`, `USN_RECORD_*` |
| `Win32_UI_Shell` | `IFileOperation`, `SHQueryRecycleBinW` |
| `Win32_System_RestartManager` | Restart Manager |
| `Win32_System_TaskScheduler` | `ITaskService` |
| `Win32_System_Services` | Service control manager |
| `Win32_System_ApplicationInstallationAndServicing` | MSI |
| `Win32_Storage_CloudFilters` | cfapi |
| `Win32_System_Pipes` | Named pipes |
| `Win32_Security`, `Win32_Security_Authorization` | Tokens, DACLs |
| `Win32_System_Registry` | Registry |
| `Win32_System_Com` | COM |
| `Win32_System_Threading` | Processes, threads |
| `Wdk_Storage_FileSystem` | `NtQueryDirectoryFileEx` and `RtlSetProcessPlaceholderCompatibilityMode`. Both were found in `windows-sys` 0.61.2 under `Wdk/Storage/FileSystem`; placement in `windows` 0.62 is assumed to match. |
| `Management_Deployment` | WinRT `PackageManager` |

### 2. Filesystem enumeration

**`FindFirstFileExW`** ([docs](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-findfirstfileexw)):
- `FindExInfoBasic` skips 8.3 short names.
- `FIND_FIRST_EX_LARGE_FETCH` (value 2, Win7+) "uses a larger buffer for directory queries".
- `FIND_FIRST_EX_ON_DISK_ENTRIES_ONLY` (4) "limits the results to files that are physically on disk … only relevant when a file virtualization filter is present".
- On a symlink, `WIN32_FIND_DATA` describes the link, not the target.
- The reparse tag is in `dwReserved0` when `FILE_ATTRIBUTE_REPARSE_POINT` is set ([reparse tags](https://learn.microsoft.com/en-us/windows/win32/fileio/reparse-point-tags)).
- Gives `nFileSizeHigh/Low` (logical) but **no allocation size**.
- Microsoft warns attribute info "may not be current" on loaded NTFS systems.

**`GetFileInformationByHandleEx` directory classes** ([class enum](https://learn.microsoft.com/en-us/windows/win32/api/minwinbase/ne-minwinbase-file_info_by_handle_class)):
- `FileIdBothDirectoryInfo` (Vista+) returns `EndOfFile`, `AllocationSize`, `FileAttributes`, `EaSize`, a 64-bit `FileId` and the name. "No specific access rights are required to query this information" ([FILE_ID_BOTH_DIR_INFO](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_id_both_dir_info)).
- `FileIdExtdDirectoryInfo` (Win8/2012+) adds `ReparsePointTag` and a 128-bit `FILE_ID_128` (needed for ReFS) ([FILE_ID_EXTD_DIR_INFO](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_id_extd_dir_info)). Oddly, the doc lists "Minimum supported client: None supported", so feature-detect at runtime and fall back to `FileIdBothDirectoryInfo`.
- Enumeration continues on the same handle until exhausted. The `*RestartInfo` variants rewind.

**`NtQueryDirectoryFileEx`** (ntdll, Win10 1709+, [WDK doc](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/nf-ntifs-ntquerydirectoryfileex)) adds query flags:

| Flag | Meaning |
|---|---|
| `SL_RESTART_SCAN` | Start from the first entry |
| `SL_RETURN_SINGLE_ENTRY` | One entry per call |
| `SL_RETURN_ON_DISK_ENTRIES_ONLY` | Bypass virtualization filters (not all file systems) |
| `SL_NO_CURSOR_UPDATE_QUERY` | Parallel queries on one handle (not all file systems) |

It supports `FileIdExtdBothDirectoryInformation`. It is semi-documented (WDK), so treat it as an optional fast path.

**Size on disk vs. logical size:**
- `AllocationSize` is "usually a multiple of the sector or cluster size".
- `GetCompressedFileSizeW` returns "the actual number of bytes of disk storage used", meaning compressed size for compressed files and "sparse size" for sparse files. On non-compressed, non-sparse files it equals the logical size. On a symlink path it reports the target ([docs](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-getcompressedfilesizew)).
- Unverified, needs testing:
  - Whether directory-entry `AllocationSize` reflects NTFS compression and WOF/CompactOS compression (`IO_REPARSE_TAG_WOF`) exactly.
  - Whether MFT-resident tiny files report 0.
- Hard-link caveat: "the directory entry size and attribute information of the file are *visibly* updated only at the link through which the change was made" ([hard links](https://learn.microsoft.com/en-us/windows/win32/fileio/hard-links-and-junctions)). Directory-scan sizes can be stale, so **re-verify every cleanup candidate through a handle** (`FileStandardInfo`, `GetCompressedFileSizeW`).

**Hard links, junctions, symlinks, reparse points:**
- Hard links: files only, same volume, deletable in any order ([docs](https://learn.microsoft.com/en-us/windows/win32/fileio/hard-links-and-junctions)). `FindFirstFileNameW` / `FindNextFileNameW` enumerate all names of a file ([docs](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-findfirstfilenamew)).
- WinSxS is the canonical example. Files "appear to be stored in more than one place … the rest of the copies are actually hard links" ([WinSxS size](https://learn.microsoft.com/en-us/windows-hardware/manufacture/desktop/determine-the-actual-size-of-the-winsxs-folder)).
- Junctions link directories, can cross local volumes, and are reparse points.
- Tag bits: `M` = Microsoft, `N` = name surrogate (`IsReparseTagNameSurrogate`).
- Tags to classify: `MOUNT_POINT` (0xA0000003), `SYMLINK` (0xA000000C), `CLOUD*`, `ONEDRIVE`, `APPEXECLINK`, `WOF`, `DEDUP`, `WCI*`, `PROJFS`, `LX_SYMLINK`, `AF_UNIX` ([reparse tags](https://learn.microsoft.com/en-us/windows/win32/fileio/reparse-point-tags)).
- `CreateFileW` + `FILE_FLAG_OPEN_REPARSE_POINT` opens the reparse point itself rather than its target. `FILE_FLAG_BACKUP_SEMANTICS` is required for directory handles. `dwDesiredAccess = 0` allows metadata queries "without accessing that file" ([CreateFileW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew)).

**OneDrive Files On-Demand and cloud files API** ([sync engine overview](https://learn.microsoft.com/en-us/windows/win32/cfapi/build-a-cloud-file-sync-engine)):
- Placeholders are implemented by the `cldflt.sys` minifilter, **NTFS only**.
- Three states: placeholder, full (may be dehydrated by the system), and pinned full.
- "Whether you use file system APIs, the Command Prompt, or a desktop or a UWP app to access a placeholder file, the file will hydrate". Background hydration triggers an interactive toast, and the user can block the app.
- Hydration policy is fixed at open time. Cloud reparse points are hidden from apps "except for sync engines and processes whose main image resides under %systemroot%" unless the process opts in via `RtlSetProcessPlaceholderCompatibilityMode`.
- The WDK page for that function ([docs](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/nf-ntifs-rtlsetprocessplaceholdercompatibilitymode)) says "most Windows applications see exposed placeholders by default". This conflicts with the overview, so **explicitly call it with `PHCM_EXPOSE_PLACEHOLDERS` (2)**. Exports: ntdll/ntoskrnl, Win10 1803+.
- Attribute signals ([constants](https://learn.microsoft.com/en-us/windows/win32/fileio/file-attribute-constants)):

| Attribute | Value | Meaning |
|---|---|---|
| `RECALL_ON_OPEN` | 0x40000 | Enumeration-only; "no physical representation" |
| `RECALL_ON_DATA_ACCESS` | 0x400000 | Reading will fetch from remote |
| `PINNED` | 0x80000 | User wants it kept local |
| `UNPINNED` | 0x100000 | May be freed when not accessed |
| `OFFLINE` | 0x1000 | Data moved to offline storage |

- `CfGetPlaceholderStateFromFindData` (cldapi.dll, Win10 1709+) maps find data to `CF_PLACEHOLDER_STATE`: `PLACEHOLDER`, `SYNC_ROOT`, `IN_SYNC`, `PARTIAL`, `PARTIALLY_ON_DISK` ([docs](https://learn.microsoft.com/en-us/windows/win32/api/cfapi/nf-cfapi-cfgetplaceholderstatefromfinddata), [enum](https://learn.microsoft.com/en-us/windows/win32/api/cfapi/ne-cfapi-cf_placeholder_state)).
- Sync roots can be listed with WinRT `StorageProviderSyncRootManager.GetCurrentSyncRoots()` (1709+), which includes legacy roots ([docs](https://learn.microsoft.com/en-us/uwp/api/windows.storage.provider.storageprovidersyncrootmanager.getcurrentsyncroots)).
- `FILE_FLAG_OPEN_NO_RECALL` is described as "for use by remote storage systems". Don't rely on it to avoid hydration.

**MFT-speed enumeration (WizTree, Everything):**
- `FSCTL_ENUM_USN_DATA` on a `\\.\X:` handle ("The volume must be NTFS") iterates `MFT_ENUM_DATA` from `StartFileReferenceNumber = 0` ([docs](https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ni-winioctl-fsctl_enum_usn_data)).
- Records are V2, V3 or V4. V3 carries 128-bit file and parent reference numbers, `Reason`, `SourceInfo`, `FileAttributes` and the name, but **no size**. Always parse with `RecordLength`/`FileNameOffset` at runtime and refuse unknown major versions ([USN_RECORD_V3](https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ns-winioctl-usn_record_v3)).
- voidtools says normal users "don't have access to the MFT and USN journal – you have to be an administrator or install the Everything service". Everything's service "is just a wrapper" so the GUI and database stay unelevated ([voidtools forum, secondary](https://voidtools.com/forum/viewtopic.php?t=12779)).
- WizTree needs admin for direct MFT reads and falls back to standard enumeration otherwise. WizTree's own site was unreachable, so this rests on a secondary report ([windowsforum, secondary](https://windowsforum.com/news/wiztree-fast-scan-requires-ntfs-and-admin-rights-on-windows-11.445718/)).
- Sizes in those tools come from parsing raw `$MFT` records (unverified implementation detail).

### 3. Change tracking

**USN change journal** ([overview](https://learn.microsoft.com/en-us/windows/win32/fileio/change-journals), [FSCTL_READ_USN_JOURNAL](https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ni-winioctl-fsctl_read_usn_journal), [volume handle](https://learn.microsoft.com/en-us/windows/win32/fileio/obtaining-a-volume-handle-for-change-journal-operations)):
- Per-volume, NTFS (ReFS uses V3 records). Requires a volume handle, which in practice means admin (see §2).
- Persist per volume: `{UsnJournalID, NextUsn}` from `FSCTL_QUERY_USN_JOURNAL`.
- On restart, read from the saved USN. If the journal ID changed or entries were deleted (journal wrap), do a full rescan.
- A rename produces two records (old parent, new parent). A final record carries `USN_REASON_CLOSE`.
- `SourceInfo` flags such as `USN_SOURCE_DATA_MANAGEMENT` and `USN_SOURCE_CLIENT_REPLICATION_MANAGEMENT` let Lumen ignore HSM and cloud-sync noise ([USN_RECORD_V3](https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ns-winioctl-usn_record_v3)).
- Lumen should **never create or delete a journal** (`FSCTL_CREATE/DELETE_USN_JOURNAL`), because that changes volume state.
- `FSCTL_READ_UNPRIVILEGED_USN_JOURNAL` exists in the SDK: `windows-sys` 0.61.2 defines it in `Win32_System_Ioctl` as 590763 (0x903AB, `FILE_ANY_ACCESS`). No Learn page documents it, though. Whether a non-admin can use it, which handle type it needs and what it filters are all **(unverified)**. Lumen must not depend on it without testing it on Windows.

**`ReadDirectoryChangesW`** ([docs](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-readdirectorychangesw)):
- Needs `FILE_LIST_DIRECTORY` and `FILE_FLAG_BACKUP_SEMANTICS`. Can watch a subtree.
- On buffer overflow it "will still return true, but the entire contents of the buffer are discarded and lpBytesReturned … zero". `ERROR_NOTIFY_ENUM_DIR` also means "compute the changes by enumerating".
- Over the network the buffer must be ≤ 64 KB. The buffer must be DWORD-aligned.
- Size and last-write changes are only reported once flushed from cache.
- Use IOCP via `FILE_FLAG_OVERLAPPED`.
- `ReadDirectoryChangesExW` (Win10 1709+) with `ReadDirectoryNotifyExtendedInformation` returns `FILE_NOTIFY_EXTENDED_INFORMATION` records. The Learn page says it is "currently supported only for the NTFS file system", so on ReFS/Dev Drive, FAT or network shares Lumen must fall back to `ReadDirectoryChangesW` ([docs](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-readdirectorychangesexw)).

### 4. Recycle Bin semantics

- **`IFileOperation::SetOperationFlags`** ([docs](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ifileoperation-setoperationflags)):
  - Default flags are `FOF_ALLOWUNDO | FOF_NOCONFIRMMKDIR`.
  - **`FOFX_RECYCLEONDELETE` (0x80000, Win8+)**: "send it to the Recycle Bin rather than permanently deleting it".
  - `FOFX_ADDUNDORECORD` (Win8+) is "preferred to FOF_ALLOWUNDO".
  - `FOF_ALLOWUNDO` "is ignored" without fully qualified paths.
  - `FOF_NOERRORUI` + `FOFX_EARLYFAILURE` stops the whole batch on the first error.
  - `FOF_WANTNUKEWARNING` warns when an item "is being destroyed … rather than recycled".
- **`SHFileOperationW`** "has been replaced in Windows Vista by IFileOperation". It can return 0 even if the user cancelled (check `fAnyOperationsAborted`), and relative paths aren't thread-safe ([docs](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shfileoperationw)).
- **Restore:** there is no documented restore function. The supported route is to enumerate `FOLDERID_RecycleBinFolder` through `IShellItem` and invoke the `undelete` verb through `IContextMenu`. Raymond Chen notes the Recycle Bin, at least up to Windows 7, "ignores the `CMIC_MASK_FLAG_NO_UI` flag", so confirmation UI can appear ([Old New Thing](https://devblogs.microsoft.com/oldnewthing/20110901-00/?p=9753)).
- **Size/count:** `SHQueryRecycleBinW(root, &SHQUERYRBINFO)` per drive. NULL root fails on Win2000+ ([docs](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shqueryrecyclebinw)).
- **Per-volume layout (unverified, undocumented format):** `X:\$Recycle.Bin\<UserSID>\$I…` holds metadata and `$R…` holds content. Bins are per user, per volume. Removable media and network paths, and items larger than the bin quota, may be permanently deleted. Treat the format as private.
- **Under Administrator protection**, an elevated process runs as a separate system-managed account with its own profile. Recycling from an elevated context would land in a different SID's bin (inferred from [admin protection](https://learn.microsoft.com/en-us/windows/security/application-security/application-control/administrator-protection)). Always recycle from the unelevated user agent.

### 5. OS-managed cleanup surfaces

| Area | Official facts | Lumen classification |
|---|---|---|
| **Storage Sense** | Enabled by default, runs when disk is low. Cleans temp files, Recycle Bin items older than N days, Downloads older than N days, and dehydrates unused cloud content (thresholds 0–365; cadence 0/1/7/30). Policy CSP `./Device/Vendor/MSFT/Policy/Config/Storage/*` ([docs](https://learn.microsoft.com/en-us/windows/configuration/storage/storage-sense)). | Read its policy and settings as evidence. Recommend enabling it rather than re-implementing. No public "run Storage Sense now" API was found (unverified). |
| **Disk Cleanup (cleanmgr)** | Handlers are COM `IEmptyVolumeCache(2)` registered under `HKLM\Software\Microsoft\Windows\CurrentVersion\Explorer\VolumeCaches\<name>`. DataDrivenCleaner CLSID `{C0E13E61-0CC6-11d1-BBB6-0060978B2AE6}` with `Folder`, `FileList`, `Flags` (`DDEVCF_*`), `LastAccess` (days), `CSIDL`, `StateFlagsNNNN`. `cleanmgr /sageset:n` and `/sagerun:n` profiles ([docs](https://learn.microsoft.com/en-us/windows/win32/lwef/disk-cleanup)). Not listed in [deprecated features](https://learn.microsoft.com/en-us/windows/whats-new/deprecated-features) (page dated 2026-09-23). | **Evidence source:** parse `VolumeCaches` to learn vendor-declared deletable folders and patterns. Optionally delegate system items via `/sagerun` (elevated, opaque, irreversible) with REVIEW and explicit consent. |
| **WinSxS** | "Deleting files from the WinSxS folder … may severely damage your system … might not boot". Use the `StartComponentCleanup` task (30-day grace, 1-h timeout), `Dism /Online /Cleanup-Image /StartComponentCleanup` (immediate), or `/ResetBase` ("existing update packages can't be uninstalled"). `/AnalyzeComponentStore` reports the real overhead ([cleanup](https://learn.microsoft.com/en-us/windows-hardware/manufacture/desktop/clean-up-the-winsxs-folder), [size](https://learn.microsoft.com/en-us/windows-hardware/manufacture/desktop/determine-the-actual-size-of-the-winsxs-folder)). | **Never touch files.** Show the AnalyzeComponentStore overhead. Offer StartComponentCleanup as a delegated, non-reversible system action. Exclude `/ResetBase` from one-click flows. |
| **Windows Update cache** | Microsoft's last-resort reset is `net stop wuauserv`, `rd /s /q %systemroot%\SoftwareDistribution`, `net start wuauserv`. The manual reset renames `SoftwareDistribution\DataStore`, `\Download` and `catroot2` to `.bak` ([docs](https://learn.microsoft.com/en-us/troubleshoot/windows-client/installing-updates-features-roles/additional-resources-for-windows-update)). | Classify as REVIEW. Never act while `wuauserv`/`bits` run. Never touch DataStore (update history) or catroot2. Prefer delegating to Disk Cleanup's update cleanup. |
| **Delivery Optimization** | Max cache age default 259,200 s (3 days); max cache size default 20%; cache on `%SYSTEMDRIVE%` unless `DOModifyCacheDrive` ([reference](https://learn.microsoft.com/en-us/windows/deployment/do/waas-delivery-optimization-reference)). Cmdlet `Delete-DeliveryOptimizationCache [-Force] [-IncludePinnedFiles]` ([cmdlet](https://learn.microsoft.com/en-us/powershell/module/deliveryoptimization/delete-deliveryoptimizationcache)). | Self-managing, so low value. Report size; delegate to the cmdlet only on explicit request. |
| **hiberfil.sys** | `powercfg /hibernate off`; `/size` (≥50% default); `/type reduced` (hiberboot only) ([powercfg](https://learn.microsoft.com/en-us/windows-hardware/design/device-experiences/powercfg-command-line-options)). | **Never delete the file.** Offer a reversible setting change with an explanation (Fast Startup is lost when off). |
| **pagefile.sys / swapfile.sys** | OS-managed virtual memory. | **Never.** KEEP, hard-coded deny. |
| **Temp folders** | Storage Sense deletes "temporary files that are not in use". | QUARANTINE candidates when age > threshold, not in use (Restart Manager), not reparse, and not inside a sync root. |

### 6. Installed applications

- **Uninstall registry key:** values come from MSI properties under `HKLM\Software\Microsoft\Windows\CurrentVersion\Uninstall\<ProductCode>`. Fields include `DisplayName`, `DisplayVersion`, `Publisher`, `InstallDate`, `InstallLocation`, `InstallSource`, `EstimatedSize` (set by the installer; unit commonly KB, unverified), `UninstallString`, `ModifyPath` ([docs](https://learn.microsoft.com/en-us/windows/win32/msi/uninstall-registry-key)).
  - Read `HKLM` 64-bit view (`KEY_WOW64_64KEY`), `HKLM\...\WOW6432Node` (`KEY_WOW64_32KEY`) and `HKCU`.
  - Conventions such as `SystemComponent`, `ParentKeyName` and `WindowsInstaller=1` are widely used but not all are on that page (unverified).
- **MSI:** `MsiEnumProductsExW(szProductCode, szUserSid, dwContext, dwIndex, …)`.
  - `"s-1-1-0"` enumerates all users; `NULL` means the current user.
  - `MSIINSTALLCONTEXT_MACHINE` requires `szUserSid = NULL`.
  - "A user must have administrator privileges to enumerate products across all user accounts". Each call must be made from the same thread.
  - Use `MsiGetProductInfoExW` for details ([docs](https://learn.microsoft.com/en-us/windows/win32/api/msi/nf-msi-msienumproductsexw)).
- **MSIX/AppX:** `PackageManager.FindPackagesForUser("")` covers the current user. Another user's SID "require[s] administrative privileges". The listed app capability is `packageQuery`, which applies to packaged callers; an unpackaged full-trust caller should be able to call it (unverified) ([docs](https://learn.microsoft.com/en-us/uwp/api/windows.management.deployment.packagemanager.findpackagesforuser)).
- **winget:** latest CLI release **1.29.380** (published 2026-09-21 per the GitHub API) ([release](https://github.com/microsoft/winget-cli/releases/latest)). The COM API (`Microsoft.Management.Deployment`) is out-of-proc, activated via the `Microsoft.WindowsPackageManager.ComInterop` NuGet artifacts, and runs as the regular user. Use winget for "available upgrade / source" enrichment only. It is absent on some SKUs and versions vary.

### 7. Startup items, tasks, services, drivers

- **Run/RunOnce** ([docs](https://learn.microsoft.com/en-us/windows/win32/setupapi/run-and-runonce-registry-keys)): four keys (HKLM/HKCU × Run/RunOnce).
  - Command lines are ≤ 260 chars and run in indeterminate order.
  - `!` prefix defers RunOnce deletion; `*` runs in Safe Mode.
  - HKLM RunOnce runs only when an admin logs on. The system may delay Run and Startup-group programs.
  - Also inspect the WOW6432Node views (unverified completeness).
- **Startup folders:** `FOLDERID_Startup` (per-user) and `FOLDERID_CommonStartup`.
- **StartupApproved** (undocumented): `HKCU|HKLM\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\{Run,Run32,StartupFolder}`. The first byte `02` means enabled, `03` disabled; Task Manager writes these (community sources, e.g. [ElevenForum](https://www.elevenforum.com/t/enable-or-disable-startup-apps-in-windows-11.699/)). **(unverified)** Format is not contractual, so read defensively and write only the leading byte.
- **Task Scheduler:** `ITaskService::Connect`, `GetFolder("\\")`, `ITaskFolder::GetTasks(TASK_ENUM_HIDDEN, …)` to include hidden tasks, recursing with `GetFolders` ([docs](https://learn.microsoft.com/en-us/windows/win32/api/taskschd/nf-taskschd-itaskfolder-gettasks)). Visibility of other principals' tasks without admin is (unverified).
- **Services and drivers:** `EnumServicesStatusExW(SC_ENUM_PROCESS_INFO, SERVICE_WIN32 | SERVICE_DRIVER, SERVICE_STATE_ALL, …)`.
  - The buffer maximum is 256 KB; loop on `ERROR_MORE_DATA` with the resume handle.
  - Services without `SERVICE_QUERY_STATUS` "are silently omitted" ([docs](https://learn.microsoft.com/en-us/windows/win32/api/winsvc/nf-winsvc-enumservicesstatusexw)).
  - Follow up with `QueryServiceConfigW` and `QueryServiceConfig2W`.
- **Driver trust:** Windows 11 26H2 removes default trust for cross-signed drivers ([26H2 what's new](https://learn.microsoft.com/en-us/windows/whats-new/whats-new-windows-11-version-26h2)). Lumen must never ship or modify drivers.

### 8. In-use detection: Restart Manager

- `RmStartSession`: "A maximum of 64 Restart Manager sessions per user session" (`ERROR_MAX_SESSIONS_REACHED`) ([docs](https://learn.microsoft.com/en-us/windows/win32/api/restartmanager/nf-restartmanager-rmstartsession)).
- `RmRegisterResources` takes file paths, services or processes. `RmGetList` returns `RM_PROCESS_INFO[]` plus reboot reasons. Loop on `ERROR_MORE_DATA`.
- **`ERROR_ACCESS_DENIED` means "a path registered … is a directory"**, so register files, not folders ([RmGetList](https://learn.microsoft.com/en-us/windows/win32/api/restartmanager/nf-restartmanager-rmgetlist)).
- Always call `RmEndSession`. Never call `RmShutdown` automatically.
- Ground truth remains the atomic action itself: a rename fails with a sharing violation if the file is in use.

### 9. Privilege, IPC, packaging, signing, UI stack

**UAC and Administrator protection:**
- With UAC, an admin gets a split token and Explorer runs with the standard token. Child processes inherit the parent token. Elevation prompts show on the secure desktop ([UAC](https://learn.microsoft.com/en-us/windows/security/application-security/application-control/user-account-control/how-it-works)).
- **Administrator protection** is available via KB5120998 (Aug 2026), off by default. It brings just-in-time elevation, "hidden, system-generated, profile-separated user accounts", "No auto-elevations", and Windows Hello authorization.
- Known effects: "Settings data for applications don't carry over across the regular (unelevated) and the elevated profiles". Network drives are inaccessible from elevated apps. Elevation is logged as ETW events 15031/15032 under Microsoft-Windows-LUA ([docs](https://learn.microsoft.com/en-us/windows/security/application-security/application-control/administrator-protection)).

**Named-pipe security:**
- The default pipe DACL grants full control to LocalSystem, Administrators and creator owner, and **read to Everyone and anonymous**.
- To exclude remote and other-session users, put the logon SID in the DACL.
- `FILE_GENERIC_WRITE` implies `FILE_CREATE_PIPE_INSTANCE`, which enables pipe squatting, so grant individual rights ([docs](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights)).
- Caution: a Microsoft Q&A answer to exactly this architecture (an MSIX disk visualizer with an elevated MFT helper) suggests granting `WorldSid` read on the pipe ([Q&A, secondary](https://learn.microsoft.com/en-gb/answers/questions/5944062/msix-win32-app-reading-raw-c-mft-only-when-run-as)). **Do not copy that.** Use the logon-SID DACL described above.
- `GetNamedPipeClientProcessId(pipe, &pid)` (Vista+) returns the client PID ([docs](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getnamedpipeclientprocessid)). Then `OpenProcess` → `QueryFullProcessImageNameW` → Authenticode check, plus `ImpersonateNamedPipeClient` for per-request access checks. The latter is standard Win32; specific doc pages weren't fetched here.

**MSIX constraints** ([prepare to package](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-prepare)):
- "MSIX doesn't support Windows drivers".
- No per-user services; session-0 services are allowed under LocalSystem, LocalService or NetworkService.
- HKLM writes fail or are virtualized; AppData is redirected; no in-proc shell extensions.
- "apps that require elevation for any part of their functionality won't be accepted into the Store".
- Packaged services (`desktop6:Service`) need the `packagedServices` / `localSystemServices` restricted capabilities and Windows 10 1903+ ([schema doc, via search](https://github.com/MicrosoftDocs/winrt-related/blob/docs/winrt-related-src/schemas/appxpackage/uapmanifestschema/element-desktop6-service.md)). For `packagedServices`, Microsoft says: "We don't recommend that you declare this capability in applications that you submit to the Microsoft Store. In most cases, the use of this capability won't be approved" ([capabilities](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/app-capability-declarations)).
- The restricted `allowElevation` capability lets a packaged desktop app elevate, either at launch or later at runtime. For the Store it is "subject to approval under strict criteria", and Microsoft asks developers to email `reportapp@microsoft.com` in advance. That makes an elevated MFT helper inside a Store MSIX possible in principle, but it is not something Lumen can plan around.

**Microsoft Store policy (v7.20, page dated 2026-09-14)** ([policies](https://learn.microsoft.com/en-us/windows/apps/publish/store-policies)):
- **10.2.9:** MSI/EXE listings must be standalone (not web stubs), install silently ("a User Account Control (UAC) dialog is allowed"), and use a versioned URL. Every PE file must be signed with a certificate that chains to the Microsoft Trusted Root Program.
- **10.2.4:** "dependency on non-Microsoft provided drivers or NT services is not allowed but may be considered case by case", and must be disclosed in certification notes. That affects the optional v2 `LumenService` in any Store listing.
- **10.2.4 also says:** apps "must obtain user consent to change any user's Windows settings". Prohibited unsupported methods include "undocumented or unsupported APIs in unsupported ways". Writing the undocumented `StartupApproved` values is a certification risk for a Store listing. Keep the toggle user-initiated, or route the user to Task Manager or Settings > Apps > Startup in Store builds.

**Code signing and SmartScreen** ([options](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/code-signing-options), [reputation](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation), [Artifact Signing](https://azure.microsoft.com/en-us/products/artifact-signing)):
- Artifact Signing (renamed from Trusted Signing; "remains the same in functionality"):
  - Basic: $9.99/month for 5,000 signatures. Premium: $99.99/month for 100,000. Overage is $0.005 per signature.
  - Public Trust organizations: US, Canada, EU, UK, Australia, New Zealand, Japan, South Korea, Singapore, Switzerland, Norway, Israel. Individuals: US/Canada only ([quickstart](https://learn.microsoft.com/en-us/azure/artifact-signing/quickstart)).
  - Identity validation takes 1–20 business days. A Microsoft Q&A thread reports that organizations need about 3 years of verifiable history (unverified on an official page). No hardware token; CI/CD integration.
- "EV certificates no longer bypass SmartScreen … removed in 2024".
- Reputation "can take several weeks and hundreds of clean installs". Signing consecutive releases with the same identity lets reputation carry over.
- Smart App Control "will block execution of unsigned files unless the file has a positive reputation". It can now be toggled without a reinstall (26H2).
- Store MSIX is re-signed by Microsoft. Store MSI/EXE must be publisher-signed.

**Windows App SDK / WinUI 3:**
- Stable **2.5.1** (2026-09-16); 2.0 was released 2026-04-29 on SemVer, and servicing of the 2.x line runs to 2027-04-29.
- 1.8 is listed as "Maintenance" with end of servicing **2026-09-24**, a date that has already passed ([release channels](https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/release-channels)).
- Experimental 2.5.4-experimental (2026-09-29) ([downloads](https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/downloads)).

**OS baseline:**
- Windows 10 support ended 2025-10-14. ESU requires 22H2; commercial ESU runs up to three years ([ESU](https://learn.microsoft.com/en-us/windows/whats-new/extended-security-updates)). Microsoft's consumer ESU page says consumer coverage and enrollment run "through October 12, 2027" ([consumer ESU](https://www.microsoft.com/windows/extended-security-updates)).
- Windows 11 26H2 (build 26300) reached general availability on 2026-09-29 as an enablement package on 24H2/25H2. It includes Point-in-time restore and built-in Sysmon (off by default) ([26H2](https://learn.microsoft.com/en-us/windows/whats-new/whats-new-windows-11-version-26h2)).
- Servicing ends for **Windows 11 24H2 Home/Pro on 2026-10-13** and Enterprise/Education on 2027-10-12. For 25H2 it ends 2027-10-12 (Home/Pro) and 2028-10-10 (Enterprise). For 26H2 it ends 2028-10-10 (Home/Pro) and 2029-10-09 (Enterprise). LTSC 2024 (24H2 base) is serviced until 2029-10-09 ([release info](https://learn.microsoft.com/en-us/windows/release-health/windows11-release-information)).
- Windows 11 26H1 ships only on new devices and is not offered as an in-place update from 24H2/25H2. Its build lineage is (unverified), so feature-detect APIs rather than comparing build numbers.

## Implications for Lumen

### A. Crate and adapter strategy
- **Do:** add a `lumen-platform-windows` adapter crate implementing Lumen's ports (`VolumeScanner`, `ChangeFeed`, `QuarantineStore`, `InUseProbe`, `AppInventory`, `StartupInventory`, `PrivilegedExecutor`). Inside it:
  - Use `windows-sys = "0.61"` for plain FFI (raw-dylib, fast builds).
  - Use `windows = "0.62"` only for COM/WinRT: `IFileOperation`, `ITaskService`, `IShellItem`, `PackageManager`, `StorageProviderSyncRootManager`.
  - Pin exact versions in `Cargo.lock`. Track windows-rs#4867. Budget a migration spike when `windows` 0.100 publishes (features are renamed and MSRV becomes 1.95).
- **Rejected:**
  - Upgrading to the 0.100 family now: incompatible with published `windows`.
  - Third-party wrapper crates such as `winapi`: unmaintained metadata.
  - Hand-written `extern` blocks: error-prone layouts. The exception is the few ntdll exports (`NtQueryDirectoryFileEx`, `RtlSetProcessPlaceholderCompatibilityMode`), via `Wdk_*` features or a tiny local bindgen.
- Run COM work on dedicated STA threads (`CoInitializeEx(COINIT_APARTMENTTHREADED)`), bridged to Tokio by channels. That `IFileOperation` requires STA is (unverified) but is the safe default.

### B. Scanning pipeline (two phases)
1. **Phase 1, fast metadata scan, unelevated by default:**
   - Parallel per-directory handles with `FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT`, `dwDesiredAccess = FILE_LIST_DIRECTORY | SYNCHRONIZE`, all share modes.
   - `GetFileInformationByHandleEx(FileIdExtdDirectoryInfo)` with ~64 KB buffers, falling back to `FileIdBothDirectoryInfo` + `FindFirstFileExW(FindExInfoBasic, LARGE_FETCH)` for the reparse tag.
   - Record `EndOfFile`, `AllocationSize`, attributes, `FileId`, reparse tag.
   - Call `RtlSetProcessPlaceholderCompatibilityMode(PHCM_EXPOSE_PLACEHOLDERS)` at scanner start.
   - **Never open file data in Phase 1.** No hashing and no content sniffing in the default path; this also matches the local-first privacy promise.
   - Rejected: per-file `CreateFile` + `GetFileInformationByHandle`. It is 10–100× more syscalls, triggers AV and filter overhead, and risks hydration.
2. **Do not traverse reparse points by default.** Emit them as graph edges (`junction → target`, `symlink → target`, `cloud-placeholder`, `wof-compressed`, `appexeclink`). This prevents loops and double counting, for example legacy profile junctions.
3. **Hard-link dedupe:** keep a per-volume `FileId → first-seen` map. Bytes reclaimable for an item = `AllocationSize` only if *all* its links are inside the action set. Otherwise mark as "shared, 0 reclaimable".
4. **Phase 2, candidate verification just before any proposal or action:**
   - Open each candidate by handle with `FILE_FLAG_OPEN_REPARSE_POINT`.
   - Re-read `FileStandardInfo` (allocation, link count), `FileIdInfo` and attributes, and `GetCompressedFileSizeW` for compressed or sparse files.
   - Re-check placeholder state. Compare with the Phase 1 snapshot and drop anything that changed.
5. **Elevated fast mode (opt-in):** `FSCTL_ENUM_USN_DATA` to build the tree, then sizes via Phase 1 queries on directories of interest. Raw `$MFT` parsing is a later, read-only optimization; it is NTFS-only, format-sensitive, and needs fuzz tests.

### C. Change feed
- With the elevated helper: USN journal per NTFS volume, persisting `{JournalID, NextUsn}`. On ID mismatch or deleted entries, do a full rescan. Filter `SourceInfo` HSM and replication noise. Never create journals.
- Without it: `ReadDirectoryChangesW` (IOCP) on hot roots only: Downloads, %TEMP%, known cache roots discovered from evidence. Treat zero bytes or `ERROR_NOTIFY_ENUM_DIR` as "rescan subtree". `ReadDirectoryChangesExW` with extended info is NTFS-only, so use it only after checking the file system name.
- Rejected: periodic full rescans as the only mechanism (too costly), and whole-volume `ReadDirectoryChangesW` (overflows).

### D. Quarantine, verification, rollback (Windows adapter)
- **Quarantine = same-volume atomic rename** into `X:\$LumenQuarantine\<UserSID>\<planId>\<itemId>`.
  - The per-user subfolder gets an owner-only DACL; the root is hidden and system-attributed.
  - Rename via `SetFileInformationByHandle(FileRenameInfo[Ex])` on the *verified handle* from Phase 2. This keeps it TOCTOU-safe against junction swaps; also compare `GetFinalPathNameByHandleW`.
  - This preserves FileId, ACLs, ADS, sparse and compression state, and costs zero bytes of copying.
- **Manifest per item:** original path, FileId (128-bit), volume serial, size, attributes, reparse tag, timestamps, policy decision and evidence IDs. Store it in Lumen's local DB and also as a sidecar file in the quarantine folder, so a lost DB can be recovered.
- **Verification:** after the rename, check that the source path is gone, the destination opens with the same FileId, and the link count is unchanged.
- **Rollback:** rename back. Recreate missing parent directories with the recorded ACL inheritance. On name collision, never overwrite; ask the user.
- **Expiry:** after the retention window, purge permanently, or optionally "release to Recycle Bin" via `IFileOperation` with `FOFX_RECYCLEONDELETE | FOF_NOCONFIRMATION | FOF_SILENT | FOF_NOERRORUI | FOFX_EARLYFAILURE`. Only do this when the volume is fixed and local, and the item is smaller than the bin quota (`SHQueryRecycleBinW` plus bin-size settings). Otherwise refuse. Storage Sense is enabled by default, and its Recycle Bin threshold (`ConfigStorageSenseRecycleBinCleanupThreshold`) may permanently purge released items without Lumen being involved. The UI must not present "released to Recycle Bin" as still recoverable by Lumen. Watch the progress sink for items that were not recycled (unverified mechanism: `PostDeleteItem`'s new-item pointer).
- **Hard deny-list, enforced in the deterministic policy engine and again in the privileged executor:**
  - `%WINDIR%\WinSxS`, `%WINDIR%\System32` / `SysWOW64`, `\Windows\Installer`.
  - `pagefile.sys`, `hiberfil.sys`, `swapfile.sys`.
  - `System Volume Information`, `$Recycle.Bin` (other SIDs), `$Extend`.
  - `SoftwareDistribution\DataStore`, `catroot2`.
  - Any sync-root subtree, any `RECALL_ON_*` item, any `PINNED` item.
  - Program install dirs of installed apps (REVIEW only).
- **Cross-volume moves: rejected.** They are a copy+delete that is not atomic and loses ADS, ACL or sparse state.
- **Recycle-Bin-as-quarantine: rejected.** It has no programmatic restore, can permanently delete, and lands in the wrong SID under Administrator protection.

### E. Cloud files policy
- Inside sync roots, from `GetCurrentSyncRoots()` and placeholder attributes, the default is **KEEP**, with size shown as "cloud-backed" vs "local".
- The only space action is *dehydrate / free up space*. Delegate it to the provider or Storage Sense (`ConfigStorageSenseCloudContentDehydrationThreshold`). A Lumen-native unpin (`FILE_ATTRIBUTE_UNPINNED`) is (unverified) and should be gated behind testing with OneDrive.
- **Moving or deleting = cloud deletion.** Never auto-propose it.
- Jev (the AI judge) must never receive content or hashes of placeholder files. It may receive metadata only.

### F. In-use gate
- Before every quarantine batch, create one Restart Manager session per batch (≤ 64 concurrent per user session), register **files only**, and call `RmGetList`.
- Any hit means status "in use by <app>", so the item is skipped or the user is asked to close the app. Never `RmShutdown`.
- A rename sharing violation is treated as authoritative "in use".

### G. Process topology and privilege
- **Recommended v1:**
  1. `lumen.exe`: UI (cross-platform shell, e.g. WebView2/Tauri), `asInvoker`.
  2. `lumen-agent.exe`: unelevated core with policy, graph, user-scope scan, quarantine of user-owned files, Recycle Bin, HKCU/startup, AppX current user.
  3. `lumen-elevate.exe`: `requireAdministrator` helper launched on demand via `ShellExecuteEx("runas")` per approved plan.
     - Handles MFT/USN reads, machine-scope temp, all-users inventory, DISM, cleanmgr, DO cmdlet delegation.
     - Receives a **signed plan file** (plan ID + item list + policy digest), re-runs the deny-list and Phase 2 verification itself, and exits.
- **Optional v2:** a `LumenService` (LocalSystem, `SERVICE_DEMAND_START` or delayed auto) for continuous USN tracking, avoiding repeated prompts. Under Administrator protection each `runas` needs Windows Hello.
  - Pipe `\\.\pipe\Lumen.<random-per-install>` created with `FILE_FLAG_FIRST_PIPE_INSTANCE` and `PIPE_REJECT_REMOTE_CLIENTS`.
  - Explicit SDDL: SYSTEM full; the interactive user's logon SID `FILE_READ_DATA | FILE_WRITE_DATA` (no `FILE_CREATE_PIPE_INSTANCE`); no Everyone or anonymous.
  - Verify client PID → image path → Authenticode signer equals Lumen's signer. Impersonate the client for path access checks. Accept only typed requests (`ScanVolume`, `ReadJournal`, `ExecutePlan(planId)`), never raw paths to delete.
- **Rejected:**
  - Running the whole app elevated: breaks per-user context, HKCU and Recycle Bin under profile separation, and drag-and-drop; Store-ineligible.
  - Always-on SYSTEM service in v1: largest attack surface for a v1 product. Cleaners are classic LPE targets via junction/symlink redirection (general industry pattern).
- **Identity rule:** the elevated side must get the target user's SID and profile paths from the unelevated agent or the client token. It must never use its own `HKCU`, `%TEMP%` or `FOLDERID_*`.

### H. Inventory adapters
- **Installed apps:** union Uninstall keys (3 views, plus `HKU\<SID>` when elevated), MSI (`MsiEnumProductsExW` on one thread), AppX (`FindPackagesForUser("")`) and optional winget COM enrichment.
  - Normalize into one `InstalledApp` node with `source[]`, then link `InstallLocation` and package dirs to evidence-graph caches.
  - Uninstall is **never** a quarantine action. Launch the vendor uninstaller or `PackageManager.RemovePackageAsync` only on explicit request, and mark it irreversible.
- **Startup:** Run/RunOnce (4 keys + WOW64), Startup folders, StartupApproved state, scheduled tasks (`TASK_ENUM_HIDDEN`), services and drivers.
  - **Disable by toggling StartupApproved** (Task Manager-compatible, reversible) or `IRegisteredTask::put_Enabled(FALSE)`. Never delete Run values or tasks.
  - Services and drivers are report-only in v1.

### I. Distribution
- Ship a signed MSI (or NSIS EXE) built in CI and signed with **Azure Artifact Signing** (Basic tier is enough) with RFC 3161 timestamps. Sign every PE: UI, agent, helper, service, DLLs.
- Keep one signing identity forever to accumulate SmartScreen reputation. Avoid PUA-like behavior (no scare scans, no "registry cleaning") so the certificate doesn't gain negative reputation.
- Optionally list the MSI/EXE in the Store under policy 10.2.9 (publisher-signed, silent install, UAC allowed).
  - A Store build must not ship the v2 `LumenService` without case-by-case approval (10.2.4).
  - It should not write the undocumented `StartupApproved` values automatically; that toggle needs explicit per-item user consent, or it should be omitted in Store builds.
- **MSIX-only is rejected** because of the service, driver, HKLM and elevation constraints. `allowElevation` and `packagedServices` are restricted capabilities that are rarely approved.
- **Minimum OS:** Windows 10 22H2 (ESU) and Windows 11 24H2+. All chosen APIs are Win10 1709+, except `FOFX_RECYCLEONDELETE` (Win8+) and Administrator-protection-aware behavior.
  - Note that 24H2 Home/Pro leaves servicing on 2026-10-13. The practical consumer baseline is soon 25H2/26H2, while 24H2 Enterprise and LTSC 2024 remain supported.
  - The CI matrix should cover Win10 22H2, Win11 24H2 LTSC/Enterprise, 25H2 and 26H2.

## Risks and open questions

1. **windows-rs 0.100 timing and churn.** Feature names and module paths change and MSRV rises to 1.95. If `windows`/`windows-sys` 0.100 lag, COM/WinRT stays on 0.62 while new focused crates move ahead. Plan to isolate via adapters, or generate bindings locally with `windows-bindgen`.
2. **Directory-entry size accuracy** for NTFS-compressed, WOF/CompactOS, deduplicated (`IO_REPARSE_TAG_DEDUP`) and MFT-resident files is not fully specified. Build a Windows fixture corpus and compare against `GetCompressedFileSizeW` and `FileStandardInfo`.
3. **The `FILE_ID_EXTD_DIR_INFO` "client: None supported"** doc anomaly needs runtime validation on Win10/11 clients.
4. **Placeholder exposure defaults conflict** between the cfapi overview and the `RtlSetProcessPlaceholderCompatibilityMode` page. Test with OneDrive, Dropbox and Google Drive: confirm no hydration toasts during a full scan, and confirm attribute and tag visibility.
5. **Unprivileged USN reading** (`FSCTL_READ_UNPRIVILEGED_USN_JOURNAL`): the control code is defined in the SDK and in `windows-sys`, but its semantics are undocumented, so how it behaves is (unverified). If a Windows spike shows it is usable without admin, change tracking could work unelevated. Until then, design for the elevated helper or `ReadDirectoryChangesW`.
6. **Recycle Bin behavior:** confirm how to detect "permanently deleted instead of recycled" with `IFileOperation` (progress sink), the quota checks, and behavior on removable and ReFS volumes. The `$I/$R` format is undocumented; never parse it for decisions.
7. **StartupApproved format** is undocumented and could change; Task Manager is the reference behavior. Add an integrity check: only touch the first byte, and verify by reading back.
8. **Administrator protection** is still rolling out.
    - It is available via KB5120998 on 24H2/25H2 and is listed in the 26H2 feature notes, off by default. The Windows Security toggle is still in preview.
    - Elevated-helper UX (Hello prompt per `runas`), network-path inaccessibility and profile separation need testing on KB5120998+ builds.
    - Microsoft says not to enable the feature "if you're using apps that access shared files across profiles". It also lists Start-menu and update-blocking issues fixed in KB5124010 (2026-09). Test on builds both with and without KB5124010.
9. **Restart Manager limits:** visibility of other users' or services' handles without elevation, and the 64-session cap under parallel batches, are (unverified) and need measuring.
10. **Disk Cleanup and Storage Sense:** cleanmgr isn't on the deprecated list, but Microsoft steers users to Settings. No public API to trigger Storage Sense was found. Keep delegation optional and degrade gracefully.
11. **AV interaction:** mass renames into a hidden quarantine and `$MFT` reads may trigger EDR heuristics. Engage the Microsoft Defender false-positive submission process before launch, and document the behavior for enterprise allow-listing.
12. **Artifact Signing eligibility** (region, organization validation) affects who can sign.
    - The fallback is an OV certificate with its key on an HSM or token, as CA/B Forum rules require. Microsoft's SmartScreen page treats OV and EV the same: "flagged as unrecognized until reputation accumulates".
    - Under CA/B Forum Ballot CSC-31, OV certificates issued on or after 2026-03-01 last at most 460 days, so budget for yearly re-issuance and keep the same subject so reputation carries over ([CSC-31](https://cabforum.org/2025/11/17/ballot-csc-31-maximum-validity-reduction/)).
    - The "$150–300/year" price is (unverified).
13. **ReFS / Dev Drive volumes:** USN V3 records and 128-bit IDs are required. cldflt placeholders are NTFS-only. Validate the scanner on ReFS.
14. **winget availability and COM activation** in unpackaged processes needs a smoke test. Treat it as best-effort enrichment.

## Sources

- [crates.io API – windows](https://crates.io/api/v1/crates/windows) · [versions](https://crates.io/api/v1/crates/windows/versions)
- [crates.io API – windows-sys](https://crates.io/api/v1/crates/windows-sys)
- [crates.io API – windows-core](https://crates.io/api/v1/crates/windows-core)
- [docs.rs – windows latest](https://docs.rs/crate/windows/latest) · [windows 0.62.2 features](https://docs.rs/crate/windows/0.62.2/features) · [windows-sys latest](https://docs.rs/crate/windows-sys/latest)
- [GitHub – microsoft/windows-rs releases](https://github.com/microsoft/windows-rs/releases) · [Release 74](https://github.com/microsoft/windows-rs/releases/tag/74) · [Release 73](https://github.com/microsoft/windows-rs/releases/tag/73)
- [windows-rs #4867 – Rust for Windows, August 2026 (0.100 plan)](https://github.com/microsoft/windows-rs/issues/4867)
- [Downstream issue: windows-future 0.100 vs windows 0.62 (Meowcal-Sub #232)](https://github.com/PeterShanxin/Meowcal-Sub/issues/232)
- [FindFirstFileExW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-findfirstfileexw)
- [FILE_INFO_BY_HANDLE_CLASS](https://learn.microsoft.com/en-us/windows/win32/api/minwinbase/ne-minwinbase-file_info_by_handle_class)
- [FILE_ID_BOTH_DIR_INFO](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_id_both_dir_info)
- [FILE_ID_EXTD_DIR_INFO](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_id_extd_dir_info)
- [NtQueryDirectoryFileEx (WDK)](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/nf-ntifs-ntquerydirectoryfileex)
- [GetCompressedFileSizeW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-getcompressedfilesizew)
- [CreateFileW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew)
- [File Attribute Constants](https://learn.microsoft.com/en-us/windows/win32/fileio/file-attribute-constants)
- [Reparse Point Tags](https://learn.microsoft.com/en-us/windows/win32/fileio/reparse-point-tags)
- [Hard Links and Junctions](https://learn.microsoft.com/en-us/windows/win32/fileio/hard-links-and-junctions)
- [FindFirstFileNameW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-findfirstfilenamew)
- [Build a Cloud Sync Engine that Supports Placeholder Files](https://learn.microsoft.com/en-us/windows/win32/cfapi/build-a-cloud-file-sync-engine)
- [RtlSetProcessPlaceholderCompatibilityMode](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/nf-ntifs-rtlsetprocessplaceholdercompatibilitymode)
- [CfGetPlaceholderStateFromFindData](https://learn.microsoft.com/en-us/windows/win32/api/cfapi/nf-cfapi-cfgetplaceholderstatefromfinddata)
- [CF_PLACEHOLDER_STATE](https://learn.microsoft.com/en-us/windows/win32/api/cfapi/ne-cfapi-cf_placeholder_state)
- [StorageProviderSyncRootManager.GetCurrentSyncRoots](https://learn.microsoft.com/en-us/uwp/api/windows.storage.provider.storageprovidersyncrootmanager.getcurrentsyncroots)
- [FSCTL_ENUM_USN_DATA](https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ni-winioctl-fsctl_enum_usn_data)
- [FSCTL_READ_USN_JOURNAL](https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ni-winioctl-fsctl_read_usn_journal)
- [USN_RECORD_V3](https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ns-winioctl-usn_record_v3)
- [Change Journals](https://learn.microsoft.com/en-us/windows/win32/fileio/change-journals)
- [Obtaining a Volume Handle for Change Journal Operations](https://learn.microsoft.com/en-us/windows/win32/fileio/obtaining-a-volume-handle-for-change-journal-operations)
- [ReadDirectoryChangesW](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-readdirectorychangesw)
- [IFileOperation::SetOperationFlags](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ifileoperation-setoperationflags)
- [SHFileOperationW](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shfileoperationw)
- [SHQueryRecycleBinW](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shqueryrecyclebinw)
- [The Old New Thing – Invoking commands on items in the Recycle Bin](https://devblogs.microsoft.com/oldnewthing/20110901-00/?p=9753)
- [Configure Storage Sense in Windows](https://learn.microsoft.com/en-us/windows/configuration/storage/storage-sense)
- [Creating a Disk Cleanup Handler](https://learn.microsoft.com/en-us/windows/win32/lwef/disk-cleanup)
- [Deprecated features in the Windows client](https://learn.microsoft.com/en-us/windows/whats-new/deprecated-features)
- [Clean Up the WinSxS Folder](https://learn.microsoft.com/en-us/windows-hardware/manufacture/desktop/clean-up-the-winsxs-folder)
- [Determine the Actual Size of the WinSxS Folder](https://learn.microsoft.com/en-us/windows-hardware/manufacture/desktop/determine-the-actual-size-of-the-winsxs-folder)
- [Additional resources for Windows Update (reset components)](https://learn.microsoft.com/en-us/troubleshoot/windows-client/installing-updates-features-roles/additional-resources-for-windows-update)
- [Delivery Optimization reference](https://learn.microsoft.com/en-us/windows/deployment/do/waas-delivery-optimization-reference)
- [Delete-DeliveryOptimizationCache](https://learn.microsoft.com/en-us/powershell/module/deliveryoptimization/delete-deliveryoptimizationcache)
- [Powercfg command-line options](https://learn.microsoft.com/en-us/windows-hardware/design/device-experiences/powercfg-command-line-options)
- [Windows Installer Properties for the Uninstall Registry Key](https://learn.microsoft.com/en-us/windows/win32/msi/uninstall-registry-key)
- [MsiEnumProductsExW](https://learn.microsoft.com/en-us/windows/win32/api/msi/nf-msi-msienumproductsexw)
- [PackageManager.FindPackagesForUser](https://learn.microsoft.com/en-us/uwp/api/windows.management.deployment.packagemanager.findpackagesforuser)
- [winget-cli latest release (1.29.380)](https://github.com/microsoft/winget-cli/releases/latest)
- [Run and RunOnce Registry Keys](https://learn.microsoft.com/en-us/windows/win32/setupapi/run-and-runonce-registry-keys)
- [ElevenForum – Enable or Disable Startup Apps (StartupApproved, community source)](https://www.elevenforum.com/t/enable-or-disable-startup-apps-in-windows-11.699/)
- [ITaskFolder::GetTasks](https://learn.microsoft.com/en-us/windows/win32/api/taskschd/nf-taskschd-itaskfolder-gettasks)
- [EnumServicesStatusExW](https://learn.microsoft.com/en-us/windows/win32/api/winsvc/nf-winsvc-enumservicesstatusexw)
- [RmGetList](https://learn.microsoft.com/en-us/windows/win32/api/restartmanager/nf-restartmanager-rmgetlist)
- [RmStartSession](https://learn.microsoft.com/en-us/windows/win32/api/restartmanager/nf-restartmanager-rmstartsession)
- [How User Account Control works](https://learn.microsoft.com/en-us/windows/security/application-security/application-control/user-account-control/how-it-works)
- [Administrator protection](https://learn.microsoft.com/en-us/windows/security/application-security/application-control/administrator-protection)
- [Named Pipe Security and Access Rights](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights)
- [GetNamedPipeClientProcessId](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getnamedpipeclientprocessid)
- [Prepare to package a desktop application (MSIX)](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-prepare)
- [desktop6:Service schema doc (GitHub, via search)](https://github.com/MicrosoftDocs/winrt-related/blob/docs/winrt-related-src/schemas/appxpackage/uapmanifestschema/element-desktop6-service.md)
- [Code signing options for Windows app developers](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/code-signing-options)
- [SmartScreen reputation for Windows app developers](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation)
- [Azure Artifact Signing (formerly Trusted Signing)](https://azure.microsoft.com/en-us/products/artifact-signing)
- [Latest Windows App SDK downloads](https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/downloads)
- [What's new in Windows 11, version 26H2](https://learn.microsoft.com/en-us/windows/whats-new/whats-new-windows-11-version-26h2)
- [Extended Security Updates (ESU) program for Windows 10](https://learn.microsoft.com/en-us/windows/whats-new/extended-security-updates)
- [voidtools forum – Understanding Indexing and USN Journal (secondary)](https://voidtools.com/forum/viewtopic.php?t=12779)
- [WindowsForum – WizTree fast scan requires NTFS and admin (secondary; returns HTTP 403 to automated clients, not re-verified 2026-10-05)](https://windowsforum.com/news/wiztree-fast-scan-requires-ntfs-and-admin-rights-on-windows-11.445718/)
- [ReadDirectoryChangesExW](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-readdirectorychangesexw)
- [Windows App SDK release channels and servicing lifecycle](https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/release-channels)
- [Windows 11 release information (servicing dates)](https://learn.microsoft.com/en-us/windows/release-health/windows11-release-information)
- [Windows 10 Consumer ESU program](https://www.microsoft.com/windows/extended-security-updates)
- [Microsoft Store Policies v7.20](https://learn.microsoft.com/en-us/windows/apps/publish/store-policies)
- [App capability declarations (allowElevation, packagedServices, localSystemServices)](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/app-capability-declarations)
- [Quickstart: Set up Artifact Signing (eligibility)](https://learn.microsoft.com/en-us/azure/artifact-signing/quickstart)
- [CA/B Forum Ballot CSC-31 – code signing maximum validity reduction](https://cabforum.org/2025/11/17/ballot-csc-31-maximum-validity-reduction/)
- [Microsoft Q&A – MSIX app reading MFT only when elevated (secondary)](https://learn.microsoft.com/en-gb/answers/questions/5944062/msix-win32-app-reading-raw-c-mft-only-when-run-as)
- [GitHub API – winget-cli latest release](https://api.github.com/repos/microsoft/winget-cli/releases/latest) · [windows-rs Release 69 (windows-sys 0.61 → windows-link/raw-dylib)](https://github.com/microsoft/windows-rs/releases/tag/69)

## Verification log

Fact-check pass on 2026-10-05. Sources are primary (Microsoft Learn, crates.io, GitHub) unless marked secondary. Every Microsoft Learn, GitHub and crates.io URL in this document returned HTTP 200. The ElevenForum and WindowsForum links return 403 to automated clients; they were kept as labelled secondary sources but not re-verified.

| # | Claim | Verdict | Source |
|---|---|---|---|
| 1 | `windows` 0.62.2 / `windows-sys` 0.61.2 (2025-10-06) are the latest; `windows-core` 0.100.0 published 2026-09-03 (MSRV 1.95) | Confirmed | crates.io API (queried live) |
| 2 | windows-rs #4867: 0.100.0, MSRV 1.95 (`windows-sys` 1.88, `windows-link` 1.85), lowercase header-based features, "may initially publish … without `windows` and `windows-sys`" | Confirmed | [#4867](https://github.com/microsoft/windows-rs/issues/4867) (opened 2026-09-02, now closed) |
| 3 | Release 74 adds `windows-reference`, `windows-time`, `windows-canvas`, `windows-reactor`; `windows-sys` uses raw-dylib since 0.61.0 (release 69) | Confirmed | GitHub releases API |
| 4 | `Wdk_Storage_FileSystem` placement of `NtQueryDirectoryFileEx` / `RtlSetProcessPlaceholderCompatibilityMode` | Corrected (was "unverified") | `windows-sys` 0.61.2 crate source |
| 5 | `FSCTL_READ_UNPRIVILEGED_USN_JOURNAL` "no Learn page" | Corrected/narrowed: the symbol exists (0x903AB), still undocumented; behavior unverified | `windows-sys` 0.61.2 `Win32_System_Ioctl`; Learn search |
| 6 | `FILE_ID_EXTD_DIR_INFO` "Minimum supported client: None supported" | Confirmed (doc anomaly real) | [Learn](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_id_extd_dir_info) |
| 7 | `RtlSetProcessPlaceholderCompatibilityMode`: Win10 1803, PHCM_EXPOSE_PLACEHOLDERS = 2, "Most Windows applications see exposed placeholders by default" | Confirmed | [WDK](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/nf-ntifs-rtlsetprocessplaceholdercompatibilitymode) |
| 8 | `FOFX_RECYCLEONDELETE` = 0x80000, Win8+; default flags `FOF_ALLOWUNDO \| FOF_NOCONFIRMMKDIR`; `FOFX_EARLYFAILURE` semantics | Confirmed | [SetOperationFlags](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nf-shobjidl_core-ifileoperation-setoperationflags) |
| 9 | Recycle Bin ignores `CMIC_MASK_FLAG_NO_UI` "at least up until Windows 7"; restore via `undelete` verb | Confirmed (2011 article; current behavior unverified) | [Old New Thing](https://devblogs.microsoft.com/oldnewthing/20110901-00/?p=9753) |
| 10 | Restart Manager: max 64 sessions per user session | Confirmed | [RmStartSession](https://learn.microsoft.com/en-us/windows/win32/api/restartmanager/nf-restartmanager-rmstartsession) |
| 11 | `ReadDirectoryChangesExW` extended info "unverified details" | Corrected: Win10 1709+, `FILE_NOTIFY_EXTENDED_INFORMATION`, **NTFS only** | [Learn](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-readdirectorychangesexw) |
| 12 | Named-pipe default DACL grants read to Everyone and anonymous; `FILE_GENERIC_WRITE` includes `FILE_CREATE_PIPE_INSTANCE`; logon SID guidance | Confirmed | [Named pipe security](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights) |
| 13 | Administrator protection: KB5120998 (Aug 2026), off by default, profile separation, no auto-elevation, settings don't carry over, network drives inaccessible, ETW 15031/15032 | Confirmed; added KB5124010 fixes and "shared files across profiles" caveat | [Learn](https://learn.microsoft.com/en-us/windows/security/application-security/application-control/administrator-protection) (2026-09-23) |
| 14 | Storage Sense enabled by default, runs on low disk; CSP names; thresholds 0–365; cadence 0/1/7/30 | Confirmed | [Storage Sense](https://learn.microsoft.com/en-us/windows/configuration/storage/storage-sense) |
| 15 | Disk Cleanup not on deprecated-features list (page 2026-09-23) | Confirmed (WMIC removal is the only cleanup-adjacent change) | [Deprecated features](https://learn.microsoft.com/en-us/windows/whats-new/deprecated-features) |
| 16 | Delivery Optimization: max cache age 259,200 s, max cache size 20%, `%SYSTEMDRIVE%` default | Confirmed | [DO reference](https://learn.microsoft.com/en-us/windows/deployment/do/waas-delivery-optimization-reference) |
| 17 | MSIX: no drivers, no per-user services, HKLM writes fail, AppData redirected, elevation blocks Store acceptance | Confirmed, with nuance added: restricted `allowElevation` exists (strict approval); `packagedServices` "in most cases … won't be approved" | [Prepare to package](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-prepare), [capabilities](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/app-capability-declarations) |
| 18 | Store accepts publisher-signed MSI/EXE | Confirmed and expanded: 10.2.9 (silent, UAC allowed, Trusted Root chain); **added** 10.2.4 NT-service dependency and undocumented-API restrictions | [Store policies v7.20](https://learn.microsoft.com/en-us/windows/apps/publish/store-policies) |
| 19 | Artifact Signing: $9.99 Basic / $99.99 Premium, $0.005 overage; orgs "US/CA/EU/UK" | Pricing confirmed; **eligibility corrected** (now also AU, NZ, JP, KR, SG, CH, NO, IL for orgs; individuals US/CA) | [Product page](https://azure.microsoft.com/en-us/products/artifact-signing), [quickstart](https://learn.microsoft.com/en-us/azure/artifact-signing/quickstart) |
| 20 | EV no longer bypasses SmartScreen; reputation "several weeks and hundreds of clean installs"; Smart App Control blocks unsigned files | Confirmed | [SmartScreen reputation](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation) |
| 21 | OV fallback "$150–300/year, equivalent for SmartScreen" | Equivalence confirmed; price unverified; **added** 460-day max validity (CSC-31, from 2026-03-01) | SmartScreen page; [CSC-31](https://cabforum.org/2025/11/17/ballot-csc-31-maximum-validity-reduction/) |
| 22 | Windows App SDK stable 2.5.1 (2026-09-16); "1.8 is in maintenance" | 2.5.1 confirmed; **1.8 corrected**: end of servicing 2026-09-24 (passed) | [Downloads](https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/downloads), [release channels](https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/release-channels) |
| 23 | winget 1.29.380, 21 Sep "(year inferred)" | Confirmed: published 2026-09-21 | GitHub API |
| 24 | 26H2: enablement package on 24H2/25H2, cross-signed driver trust removed, Sysmon built in, Point-in-time restore, Smart App Control toggle without reinstall | Confirmed; added GA 2026-09-29, build 26300 | [26H2 what's new](https://learn.microsoft.com/en-us/windows/whats-new/whats-new-windows-11-version-26h2), [release info](https://learn.microsoft.com/en-us/windows/release-health/windows11-release-information) |
| 25 | Windows 10 EoS 2025-10-14, ESU needs 22H2, commercial up to 3 years | Confirmed; **added** consumer ESU through 2027-10-12 and Win11 24H2 Home/Pro end of servicing 2026-10-13 | [ESU](https://learn.microsoft.com/en-us/windows/whats-new/extended-security-updates), [consumer ESU](https://www.microsoft.com/windows/extended-security-updates) |

Not re-verified in this pass (they stay as written, with their existing hedges): the WizTree and Everything implementation details, the `StartupApproved` byte format, MSI/AppX enumeration privilege nuances, `EstimatedSize` units, Task Scheduler visibility without admin, and the Windows Update reset procedure text.
