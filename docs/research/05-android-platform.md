# Android Platform: Storage APIs, Restrictions and Play Policy for Lumen

> Researched: 2026-10-05 · Scope: Android 11–17 storage, usage, media, background-work and native-code constraints for a local-first storage intelligence app distributed on Google Play (targetSdk 36+), plus how competitors work around them.

## Summary

- **Current platform:** Android 17 (API 37) is the current stable release. It shipped on 2026-06-16 according to secondary sources, and the docs already list QPR1 (API 37.1) and QPR2 beta (37.2). **Play requirement now in force:** since **2026-08-31**, new apps and updates must target **API 36 (Android 16)**. Developers can request an extension to **2026-11-01**. Existing apps must target ≥ API 35 to stay visible to new users on newer OS versions ([Play target API](https://developer.android.com/google/play/requirements/target-sdk)).
- **No third-party app can clear another app's *internal* cache.** `CLEAR_APP_CACHE` went from `dangerous` (Android 5.1.1) to `signature|privileged` in **Android 6.0 (API 23)**. I checked this against the AOSP manifests. Since then the only routes are system UI, root, ADB/Shizuku, or Accessibility-driven taps on "Clear cache".
- **`StorageManager.ACTION_CLEAR_APP_CACHE` (API 30) only clears *external* caches.** In practice that is `/sdcard/Android/data/*/cache`, per the AOSP MediaProvider `FileUtils.clearAppCacheDirectories()`. The caller must hold **`MANAGE_EXTERNAL_STORAGE`**. The action is all-or-nothing and shows a system confirmation dialog.
- **Per-app sizing is possible with "Usage access".** `StorageStatsManager.queryStatsForPackage/queryStatsForUid` (API 26) return app, data and cache bytes per package; `getExternalCacheBytes()` was added in API 31. The data comes from the OS, so Lumen does not need filesystem access to other apps. Querying other packages requires the user-granted special access `PACKAGE_USAGE_STATS`, which also unlocks `UsageStatsManager` last-used times.
- **Shared storage scanning options:** MediaStore (with `READ_MEDIA_*`, partial access on 14+), SAF tree grants, or **All files access (`MANAGE_EXTERNAL_STORAGE`)**. SAF cannot grant the volume root, `Download/`, `Android/data/` or `Android/obb/` (Android 11+). Even All files access **never** reaches other apps' `Android/data` (Android 13 closed the last SAF loophole).
- **Play gates `MANAGE_EXTERNAL_STORAGE` behind a Permissions Declaration Form.** The listed permitted uses include "File management … (including maintenance)", on-device search, backup, antivirus and others. "Media files access" alone is explicitly an **invalid** use. `QUERY_ALL_PACKAGES` is likewise declaration-gated; its permitted uses include file managers, antivirus and device search.
- **Reversible media deletion is built in.** `MediaStore.createTrashRequest()` (API 30) sets `IS_TRASHED`. Trashed items expire after about **30 days** (`DATE_EXPIRES`), and **pending** items after about 7 days. `createDeleteRequest()` deletes permanently. `MANAGE_MEDIA` (API 31) only suppresses the per-batch confirmation dialog.
- **Accessibility-based cache cleaning (SD Maid SE's approach) is a poor fit for Lumen.** Play states that "cleaners" are **not** accessibility tools, so the app would need a declaration plus prominent disclosure. Play now also **prohibits any Accessibility use that lets an app "autonomously initiate, plan, and execute actions or decisions"**, which matters for the AI judge (Jev). With Advanced Protection Mode on (the `AdvancedProtectionManager` API exists since Android 16 / API 36), Android 17 restricts AccessibilityService access to apps verified and categorised as Accessibility Tools (`isAccessibilityTool="true"`); Google confirmed this in its security blog on 2026-10-01 ([Google blog](https://blog.google/security/android-advanced-protection-updates/)).
- **Background limits for user-initiated deep scans:** a deep scan needs a foreground service, most plausibly `dataSync`, whose type list includes "Local file processing". On API 35+ that type has a **6 h per 24 h** budget and a `Service.onTimeout()` callback, and it cannot be started from `BOOT_COMPLETED`. Android 16 makes WorkManager long-running workers count against **job runtime quota**. Android 17 adds **RAM-based per-app memory limits** (`MemoryLimiter:AnonSwap`).
- **Native code:** all apps targeting API 35+ must support **16 KB pages**. The Play docs now say updates without 16 KB support are blocked from **2027-02-01**. Lumen should treat 16 KB support as required now (see §1). NDK r28+ aligns to 16 KB by default. Current tools: NDK **r30**, AGP **9.4.x** (9.4.0 release notes; 9.4.1 patch is on Google Maven; default NDK 28.2), cargo-ndk **4.1.2**, UniFFI **0.32.2**, Rust **1.99.0**. UniFFI's Kotlin bindings depend on JNA, which needs **≥ 5.17.0** for the 16 KB fixes (latest is 5.19.1).
- **Android 15 app archiving is a reversible way to reclaim space from unused apps.** `PackageInstaller.requestArchive()` (API 35) removes the APK and cache but keeps user data. It requires `REQUEST_DELETE_PACKAGES`. The app is restored through its installer, so reversal fails if that installer is uninstalled or disabled, or if there is no network or not enough space.
- **Coverage must be explicit.** Private space (Android 15), work profiles, `Android/data` and other apps' internal storage are invisible to Lumen, and some OEMs silently truncate the app list. Lumen must record "not observed" as **unknown**, never as "absent". This is the same guard SD Maid SE applies with its "Invalid app list" check.

## Findings

### 1. Platform baseline and Play deadlines (as of 2026-10-05)

| Item | Status | Source |
|---|---|---|
| Android 17 (API 37) | Stable. Docs list QPR1 = API 37.1 and QPR2 beta = 37.2. General availability was 2026-06-16 (Wikipedia, citing Google's "What's new in Android 17" post of that date). | [Android 17 overview](https://developer.android.com/about/versions/17), [Wikipedia: Android 17](https://en.wikipedia.org/wiki/Android_17) |
| Play target API for new apps and updates | **API 36 from 2026-08-31.** Extension available to 2026-11-01. | [Target API requirements](https://developer.android.com/google/play/requirements/target-sdk), [Play Console Help 11926878](https://support.google.com/googleplay/android-developer/answer/11926878?hl=en) |
| Existing apps (visibility to new users) | Must target ≥ API 35 | same |
| 16 KB page support | Required for apps targeting 35+. Updates blocked from **2027-02-01** (doc last updated 2026-09-16). The current page no longer mentions the 2025-11-01 requirement announced in 2025 (extensions to 2026-05-31). Whether that earlier rule was formally withdrawn or just dropped from the page is **(unverified)**, so Lumen should ship 16 KB-aligned binaries from day one. | [Support 16 KB page sizes](https://developer.android.com/guide/practices/page-sizes) |
| Next targetSdk bump (API 37) | Expected around Aug 2027, following the yearly cadence **(unverified)** | — |

Android 17 changes that matter to Lumen ([all apps](https://developer.android.com/about/versions/17/behavior-changes-all), [targeting 37](https://developer.android.com/about/versions/17/behavior-changes-17)):
- **App memory limits based on total device RAM.** A killed process shows `REASON_OTHER` with `"MemoryLimiter:AnonSwap"` in `ApplicationExitInfo.getDescription()`. You can test with `adb shell am memory-limiter …`. The Rust scanner must bound its memory and stream results.
- **Safer native DCL (targeting 37).** Native files loaded with `System.load()` must be read-only, otherwise the call throws `UnsatisfiedLinkError`. Load libraries only from the APK with `System.loadLibrary` and never download `.so` files.
- **Implicit URI grants for `ACTION_SEND`/`ACTION_IMAGE_CAPTURE` stop in Android 18.** Add `FLAG_GRANT_READ_URI_PERMISSION` explicitly when sharing reports or exports.
- **Android Advanced Protection Mode (AAPM)** is opt-in. It blocks sideloading and mandates Play Protect. `AdvancedProtectionManager` (added in **API 36**, not 37; `isAdvancedProtectionEnabled()` requires `QUERY_ADVANCED_PROTECTION_MODE`) lets apps detect it. With AAPM on, Android 17 "restricts AccessibilityService access exclusively to verified applications categorized as Accessibility Tools", and access already granted to other apps is revoked. Google's Security blog confirmed this on 2026-10-01 ([Google blog](https://blog.google/security/android-advanced-protection-updates/)). It was first reported from Beta 2 ([The Hacker News, Mar 2026](https://thehackernews.com/2026/03/android-17-blocks-non-accessibility.html); [Oct 2026 follow-up](https://thehackernews.com/2026/10/android-17-advanced-protection-locks.html)). No developer.android.com behaviour-change page lists it yet.
- **New `JobDebugInfo` APIs** (`getPendingJobReasonStats()`) for diagnosing jobs that don't run.

Android 16 ([all apps](https://developer.android.com/about/versions/16/behavior-changes-all), [targeting 36](https://developer.android.com/about/versions/16/behavior-changes-16)):
- **JobScheduler quota changes.** Jobs that start while the app is visible and continue after it goes to the background, and jobs running alongside a foreground service, now count against the runtime quota. This "impacts tasks scheduled using WorkManager, JobScheduler, and DownloadManager". Use `WorkInfo.getStopReason()` and `JobScheduler#getPendingJobReasonsHistory`.
- **16 KB compatibility mode.** 4 KB-aligned apps can still run on 16 KB devices, but the user sees a warning dialog.
- **`MediaStore#getVersion()` is now unique per app** for apps targeting 36 (anti-fingerprinting). Don't parse it; only compare it for equality.

Android 15 ([all apps](https://developer.android.com/about/versions/15/behavior-changes-all), [targeting 35](https://developer.android.com/about/versions/15/behavior-changes-15), [features](https://developer.android.com/about/versions/15/features)):
- **`dataSync` FGS timeout:** 6 h total per 24 h, shared by all of an app's `dataSync` services. After that the system calls `Service.onTimeout(int,int)` and the app has seconds to call `stopSelf()`, or it gets `RemoteServiceException`. The timer resets when the user brings the app to the foreground. **`mediaProcessing`** is a new type with its own separate 6 h budget.
- No `dataSync` FGS may start from a `BOOT_COMPLETED` receiver.
- **Private space:** a separate user profile. Apps in it are stopped when it is locked, and apps outside it can't see it.
- **App archiving** OS support (see §9).
- **16 KB page devices** are supported in AOSP.

### 2. App-specific storage, caches and `StorageManager`

**Own-app storage** ([App-specific storage](https://developer.android.com/training/data-storage/app-specific)):
- Internal: `getFilesDir()`, `getCacheDir()`, `getCodeCacheDir()` (under `/data/data/<pkg>/` or `/data/user/<u>/<pkg>/`).
- External app-specific: `getExternalFilesDir()`, `getExternalCacheDir()`, `getExternalMediaDirs()`, `getObbDir()`.
- All of these are **deleted on uninstall**. The OS may delete cache files under storage pressure, so apps must check the files still exist before using them.

**StorageManager** ([reference](https://developer.android.com/reference/android/os/storage/StorageManager)):

| API | Level | Notes |
|---|---|---|
| `getAllocatableBytes(UUID)` | 26 | Free space plus cache the OS would evict. Run it on a worker thread because it can take seconds. Avoid calling it more than once every 30 s. |
| `allocateBytes(UUID\|FileDescriptor, long)` | 26 | Asks the OS to evict other apps' caches to satisfy the allocation. This is the **only** sanctioned way for a normal app to cause *other apps' internal caches* to be trimmed, and only up to the system's cache quota. Lumen must not abuse it as a "cache cleaner". |
| `getCacheQuotaBytes(UUID)` / `getCacheSizeBytes(UUID)` | 26 | Own app only. Covers `cacheDir`, `codeCacheDir`, and `externalCacheDir` when it is on the same volume. |
| `setCacheBehaviorGroup(File,bool)` / `setCacheBehaviorTombstone(File,bool)` | 26 | Controls how the OS evicts your own cache. Useful for Lumen's own index cache. |
| `ACTION_MANAGE_STORAGE` + `EXTRA_UUID` / `EXTRA_REQUESTED_BYTES` | 25/26 | Opens the system "free up space" UI. Returns `RESULT_OK` if the space became available. |
| **`ACTION_CLEAR_APP_CACHE`** | **30** | Doc: "Allows the user to free up space by clearing app **external** cache directories… shows a dialog and lets the user decide." **Requires `MANAGE_EXTERNAL_STORAGE`.** Returns `RESULT_OK`, `OsConstants.EIO` or `RESULT_CANCELED`. The app-specific storage guide warns it "can substantially affect device battery life and might remove a large number of files". |
| `getManageSpaceActivityIntent(pkg, requestCode)` | 31 | Needs `MANAGE_EXTERNAL_STORAGE` (the guide says it also needs `QUERY_ALL_PACKAGES`). Launches another app's `android:manageSpaceActivity` even when that activity isn't exported. This is a good "let the app clean itself" path. |

**What `ACTION_CLEAR_APP_CACHE` actually does:** it is handled by MediaProvider's [`CacheClearingActivity`](https://android.googlesource.com/platform/packages/providers/MediaProvider/+/master/src/com/android/providers/media/CacheClearingActivity.java). The activity first checks `MediaProvider.hasPermissionToClearCaches()`, which is effectively `checkPermissionManager`, meaning the All files access app-op. It then calls `FileUtils.clearAppCacheDirectories()`. That function lists `<ExternalStorage>/Android/data/*`, runs `deleteContents()` on each `cache/` subfolder, and returns `EIO` on any failure. So:
- It never touches internal `/data/data/*/cache`.
- It is all apps at once. You cannot select or exclude individual apps.
- It cannot be quarantined or rolled back. Lumen has to treat it as a "regenerable, system-mediated" action.

**Clearing other apps' caches: history.**
- In Android 5.1.1 `CLEAR_APP_CACHE` was `protectionLevel="dangerous"`. In Android 6.0 it became `"signature|privileged"`, which killed the old `PackageManager.freeStorageAndNotify()` reflection trick. Verified against the AOSP `core/res/AndroidManifest.xml` at tags `android-5.1.1_r1` and `android-6.0.0_r1`.
- The current reference still lists `CLEAR_APP_CACHE` as `signature|privileged`, and describes `DELETE_CACHE_FILES` as "Old permission … no longer used" ([Manifest.permission](https://developer.android.com/reference/android/Manifest.permission)).
- Android 11 then removed cross-app access to `Android/data` ([Android 11 storage](https://developer.android.com/about/versions/11/privacy/storage)).

**Sanctioned routes for the user to clear one app's internal cache:**
1. Deep-link to `Settings.ACTION_APPLICATION_DETAILS_SETTINGS` (`package:<pkg>`) and let the user tap "Storage › Clear cache".
2. Launch the app's own manage-space activity via `getManageSpaceActivityIntent` (needs All files access).
3. `ACTION_MANAGE_STORAGE` for the system storage UI.

### 3. `StorageStatsManager` and usage access

From the [StorageStatsManager reference](https://developer.android.com/reference/android/app/usage/StorageStatsManager), all API 26:
- `queryStatsForPackage(UUID, String, UserHandle)` and `queryStatsForUid(UUID, int)`. Quote: "no permissions are required when calling this API for your own package. However, requesting details for any other package requires `PACKAGE_USAGE_STATS`… an end user can then choose to grant this permission through the Settings application."
  - Packages with a shared UID force a slower manual calculation, so **prefer `queryStatsForUid`**.
  - Both can take seconds, so call them on a worker thread.
- `queryStatsForUser(UUID, UserHandle)` gives totals for a user. `queryExternalStatsForUser(UUID, UserHandle)` returns `ExternalStorageStats` (audio, video, image, app, OBB bytes) and needs `PACKAGE_USAGE_STATS`.
- `getTotalBytes(UUID)` and `getFreeBytes(UUID)` need no permission.

`StorageStats` fields ([reference](https://developer.android.com/reference/android/app/usage/StorageStats)):
- `getAppBytes()`: APK, odex/compiler output, native libraries, plus OBB when it is on the same volume.
- `getDataBytes()`: dataDir plus caches plus external files/cache/media on the same volume.
- `getCacheBytes()`: `cacheDir`, `codeCacheDir`, plus `externalCacheDir` on the same volume.
- `getExternalCacheBytes()`: API **31**.
- `getAppBytesByDataType(int)`: API **35**. Breaks down APK, DM, lib and similar.

`PACKAGE_USAGE_STATS` has protection level `signature|privileged|development|appop|retailDemo` ([Manifest.permission](https://developer.android.com/reference/android/Manifest.permission)). It is an **app-op special access**:
- Send the user to `Settings.ACTION_USAGE_ACCESS_SETTINGS`.
- Check the grant with `AppOpsManager.unsafeCheckOpNoThrow(OPSTR_GET_USAGE_STATS, …)`.
- I found no specific Play declaration form for it **(unverified)**. It is still personal data under the User Data policy, so it needs prominent disclosure.

`UsageStatsManager` ([reference](https://developer.android.com/reference/android/app/usage/UsageStatsManager), [UsageStats](https://developer.android.com/reference/android/app/usage/UsageStats)):
- `queryUsageStats(INTERVAL_DAILY|WEEKLY|MONTHLY|YEARLY|BEST, begin, end)` returns `UsageStats` objects with `getLastTimeUsed()` (API 21), `getLastTimeVisible()` (29), `getLastTimeForegroundServiceUsed()` (29) and `getTotalTimeInForeground()`.
- `isAppInactive(pkg)` (23) needs `PACKAGE_USAGE_STATS` for other apps.
- **Retention:** daily buckets are coarse and kept for a limited window. The exact retention per interval is OEM/system-defined **(unverified)**. Treat "last used > N days" as *evidence with a confidence*, not as fact.
- Results are only useful for packages Lumen can see, which leads to package visibility.

**Package visibility** ([overview](https://developer.android.com/training/package-visibility), [automatic visibility](https://developer.android.com/training/package-visibility/automatic)):
- Targeting 30+ filters `getInstalledApplications()`, `getPackageInfo()` and similar.
- `<queries>` covers specific intents or packages. `QUERY_ALL_PACKAGES` is a `normal` permission, but Play restricts it (§6).
- Since Android 14, `MediaColumns.OWNER_PACKAGE_NAME` results are also filtered by package visibility ([MediaColumns](https://developer.android.com/reference/android/provider/MediaStore.MediaColumns)).
- SD Maid SE reports that some Android 14+ OEM builds don't grant `QUERY_ALL_PACKAGES` by default, or let the user revoke it. Others gate the list behind an OEM runtime permission (`GET_INSTALLED_APPS`, the "App list" permission) and still return incomplete lists even when it is granted. SD Maid then disables app-linked features entirely ([SD Maid SE Setup wiki](https://github.com/d4rken-org/sdmaid-se/wiki/Setup)). **Lumen should request the OEM `GET_INSTALLED_APPS` permission where it exists, and should run the truncation check (§11) regardless of what the grant state says.**
- **Not usable by Lumen:** `UsageStatsManager.queryAppUsageDuration()` and its `QUERY_APP_USAGE` permission, new in API 37.2 (QPR2 beta), have protection level `internal|role`. They are not an alternative to `PACKAGE_USAGE_STATS` for third-party apps. The API is also limited to the last 30 days ([UsageStatsManager](https://developer.android.com/reference/android/app/usage/UsageStatsManager), [Manifest.permission](https://developer.android.com/reference/android/Manifest.permission)).

### 4. MediaStore: large files, duplicates, trash

Permissions ([Manifest.permission](https://developer.android.com/reference/android/Manifest.permission), [partial access](https://developer.android.com/about/versions/14/changes/partial-photo-video-access)):
- `READ_EXTERNAL_STORAGE` has "no effect" from API 33. Use `READ_MEDIA_IMAGES`, `READ_MEDIA_VIDEO` and `READ_MEDIA_AUDIO` (all `dangerous`).
- **`READ_MEDIA_VISUAL_USER_SELECTED`** (API 34) enables Selected Photos Access when targeting 34+. It is added to the manifest automatically if the app requests the image/video permissions. Without it the app runs in compat mode.
- A storage-intelligence app must detect partial grants and label the scan "partial" (coverage evidence). Otherwise duplicate and large-file results are silently incomplete.

Play Photo/Video policy ([Play help 14115180](https://support.google.com/googleplay/android-developer/answer/14115180?hl=en)):
- `READ_MEDIA_IMAGES` and `READ_MEDIA_VIDEO` are allowed only when "core functionality revolves around broad access", for example "managing and maintaining all of users' photos/videos".
- A declaration is required, and enforcement has been mandatory since 2025-05-28.
- The Photo Picker (`PickVisualMedia` / `PickMultipleVisualMedia`, androidx.activity ≥ 1.7.0, falling back to `ACTION_OPEN_DOCUMENT`) is the default alternative ([Photo picker](https://developer.android.com/training/data-storage/shared/photopicker)).
- The picker is useless for whole-library analysis.
- The same page announces a new **Contacts** policy (READ_CONTACTS, targeting 37+, mandatory 2027-01-27). It doesn't affect Lumen, but it shows the "picker first" direction.

Querying for large media and duplicates ([MediaStore](https://developer.android.com/reference/android/provider/MediaStore), [MediaColumns](https://developer.android.com/reference/android/provider/MediaStore.MediaColumns), [Shared media guide](https://developer.android.com/training/data-storage/shared/media)):
- Collections: `MediaStore.Images|Video|Audio.Media.getContentUri(VOLUME_EXTERNAL)`, plus `MediaStore.Downloads` and `MediaStore.Files`. With All files access, `MediaStore.Files` covers everything indexed.
- Useful columns: `_ID`, `SIZE`, `MIME_TYPE`, `DATE_ADDED`/`DATE_MODIFIED`/`DATE_TAKEN`, `RELATIVE_PATH` (API 29, organizational only), `OWNER_PACKAGE_NAME` (29), `IS_PENDING`, `IS_TRASHED` (30), `IS_DOWNLOAD` (30), `IS_DRM` (30), `DATE_EXPIRES` (29), `GENERATION_ADDED`/`GENERATION_MODIFIED` (30), `DURATION`, `WIDTH`/`HEIGHT`.
- **Incremental indexing:** `MediaStore.getVersion(ctx, volume)` and `getGeneration(ctx, volume)` (API 30). Re-sync fully when the version changes. Otherwise query `GENERATION_MODIFIED > lastSeen`. This is more robust than timestamps, which apps can set with `setLastModified`.
- **No content-hash column is public.** Duplicate detection must compute hashes:
  - Kotlin opens `openFileDescriptor(uri,"r")`, calls `detachFd()`, and hands the fd to Rust.
  - Rust uses a staged approach: group by size, then hash the head and tail, then do a full BLAKE3, with perceptual hashes for "similar" photos.
- `QUERY_ARG_MATCH_TRASHED` / `QUERY_ARG_MATCH_PENDING` (`MATCH_INCLUDE|EXCLUDE|ONLY`) control whether trashed and pending items appear. They are excluded by default.
- `QUERY_ARG_MEDIA_STANDARD_SORT_ORDER` (API 36, and on API 30+ devices with R Extensions 15) sorts by `INFERRED_DATE`, descending. It overrides any other sort argument.

Write and delete operations:
- **`createTrashRequest(resolver, uris, true/false)` (API 30)** sets or clears `IS_TRASHED`. Trashed items are kept until `DATE_EXPIRES`, "default trashed expiration is typically **30 days**". The pending default is typically **7 days**. Expired items are purged at the next idle period.
- **`createDeleteRequest(resolver, uris)` (API 30)** deletes permanently, after user confirmation.
- `createWriteRequest` and `createFavoriteRequest` also exist.
- Each returns a `PendingIntent`. Launch it with `startIntentSenderForResult`, and treat `RESULT_OK` as meaning the operation has completed.
- The default gallery app can set `IS_TRASHED` directly with no dialog. Others always get a dialog unless they hold **`MANAGE_MEDIA`**.
- **`MANAGE_MEDIA` (API 31)** is `signature|appop|preinstalled`. Request it with `Settings.ACTION_REQUEST_MANAGE_MEDIA` and check with `MediaStore.canManageMedia()`.
  - It "doesn't give read or write access directly. It only prevents the user confirmation dialog."
  - It requires `READ_EXTERNAL_STORAGE` or `MANAGE_EXTERNAL_STORAGE` to take effect. How this interacts with `READ_MEDIA_*` on API 33+ is **(unverified)**.
  - Without `ACCESS_MEDIA_LOCATION`, write requests still show a dialog.
- Updating another app's item without a batch request throws `RecoverableSecurityException`, whose `userAction.actionIntent` you can launch to ask the user.
- `IS_PENDING=1` hides an item you are writing. Lumen should use it when restoring files into MediaStore so that half-restored files never appear.
- The maximum batch size per `create*Request` isn't documented **(unverified)**. Chunk batches, for example to a few hundred URIs.

Direct paths: since Android 11, File API and `fopen()` work for shared-storage media the app can read through MediaStore permissions ([Shared media guide](https://developer.android.com/training/data-storage/shared/media)). The Rust core can therefore `stat`/`read` paths, but the authoritative listing should come from MediaStore.

### 5. Storage Access Framework restrictions

From the [SAF guide](https://developer.android.com/training/data-storage/shared/documents-files) and [Android 11 storage](https://developer.android.com/about/versions/11/privacy/storage). For apps targeting 30+:
- `ACTION_OPEN_DOCUMENT_TREE` **cannot** grant:
  - the root of internal storage
  - the root of each "reliable" SD card volume
  - the **`Download/`** directory
- Neither `ACTION_OPEN_DOCUMENT_TREE` nor `ACTION_OPEN_DOCUMENT` can select files inside **`Android/data/`** or **`Android/obb/`** (or their subdirectories).
- Grants persist via `takePersistableUriPermission`. Traverse with `DocumentsContract.buildChildDocumentsUriUsingTree()` and query `Document.COLUMN_SIZE`, `COLUMN_LAST_MODIFIED`, `COLUMN_MIME_TYPE`, `COLUMN_FLAGS`. Delete with `DocumentsContract.deleteDocument()`, move with `moveDocument()` (API 24, provider-dependent).
- The guide warns that iterating large trees via SAF is slow.
- SAF can be used *in addition* to All files access, but "you can only access a file or directory if you can do so without having the MANAGE_EXTERNAL_STORAGE permission" ([Manage all files](https://developer.android.com/training/data-storage/manage-all-files)).

**The `Android/data` loophole is closed.**
- On Android 11/12, file managers could pre-seed `EXTRA_INITIAL_URI` to `Android/data`. Android 13 updated `ExternalStorageProvider.shouldBlockFromTree` to block that ([Esper](https://www.esper.io/blog/android-dessert-bites-28-file-manager-loophole-closed-73891524)).
- SD Maid SE notes that newer DocumentsUI builds (targeting 34, shipped via Google Play system updates) also block it on Android 11/12 devices ([SD Maid SE Setup](https://github.com/d4rken-org/sdmaid-se/wiki/Setup)).
- On current devices only **root, ADB or Shizuku** can read other apps' `Android/data` and `Android/obb`.

### 6. All files access, QUERY_ALL_PACKAGES and their Play policies

**`MANAGE_EXTERNAL_STORAGE`** (API 30, `signature|appop|preinstalled`) ([Manage all files](https://developer.android.com/training/data-storage/manage-all-files)):
- Request it by sending the user to `Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION` (or the generic `ACTION_MANAGE_ALL_FILES_ACCESS_PERMISSION`). Check it with `Environment.isExternalStorageManager()`.
- It grants:
  - read/write of all shared storage, including `/sdcard/Android/media`
  - the `MediaStore.Files` table
  - USB OTG and SD card roots
  - direct-path access
- It does **not** grant `/Android/data/`, `/sdcard/Android` or most of its subdirectories, or other apps' app-specific dirs.
- Testing: `adb shell appops set --uid <pkg> MANAGE_EXTERNAL_STORAGE allow`.

**Play policy** ([Play help 10467955](https://support.google.com/googleplay/android-developer/answer/10467955?hl=en)):
- The **Permissions Declaration Form** is mandatory and needs Play approval. Usage must be tied to *core functionality* that is "prominently documented and promoted in the app's description".
- Permitted uses:
  - **"File management – App's core purpose involves the access, editing, and management (including maintenance) of files and folders outside of its app-specific storage space"**
  - backup and restore
  - antivirus
  - document management (must justify why SAF isn't sufficient)
  - on-device search
  - disk/folder encryption
  - device migration
- **Invalid uses:**
  - "Media Files access", for which MediaStore is the alternative
  - "Any file selection activity where the user manually selects individual files", for which SAF is the alternative
- Temporary exceptions are possible if alternatives have a "substantially detrimental impact".
- Re-submit the form whenever usage changes. "Deceptive and undeclared uses … may result in a suspension … and/or termination of your developer account."

**`QUERY_ALL_PACKAGES`** ([Play help 10158779](https://support.google.com/googleplay/android-developer/answer/10158779?hl=en)):
- Play treats the installed-app inventory as personal and sensitive data. The permission is only allowed when the core user-facing purpose needs broad visibility.
- Permitted uses: **device search, antivirus apps, file managers, browsers**.
- Requirements: a declaration form, prominent disclosure and consent. Selling or sharing the inventory for analytics or ads is invalid.
- The policy requires that "a less broad app-visibility method" can't do the job. The app must also "have a core purpose to search for all apps on the device" and "adequately justify why a less intrusive method of app visibility will not sufficiently enable" its core functionality. This wording is stricter than the permitted-uses list suggests.
- Lumen's case: attributing storage, cache and residual folders to installed apps needs the full list. That should fit "file manager/antivirus-like" maintenance, but approval is discretionary and the "core purpose" bar is high. Lumen's store listing must state per-app storage attribution as a core feature. **Approval is not guaranteed (unverified until a test-track submission).**

**Deceptive behaviour:** store listing claims must match what the app really does. Play explicitly cites apps claiming impossible features ([Play Deceptive Behavior](https://support.google.com/googleplay/android-developer/answer/17006354?hl=en)). "Boost/speed up" or "clears all caches" claims are high-risk for an app that cannot touch internal caches.

### 7. `Android/data` and `Android/obb`

- Android 11: "apps can no longer access files in any other app's dedicated, app-specific directory within external storage" ([Android 11 storage](https://developer.android.com/about/versions/11/privacy/storage)).
- All files access does not lift this ([Manage all files](https://developer.android.com/training/data-storage/manage-all-files)), and SAF blocks it (§5).
- Only system and privileged components such as MediaProvider (via `ACTION_CLEAR_APP_CACHE`), root, or ADB-level identity (Shizuku) can enumerate them.
- `StorageStats` still reports **sizes** that include external app data and caches. Lumen can show "App X: 2.1 GB data, 640 MB cache (of which ~N MB external)" without seeing any files. Compare `getExternalCacheBytes()` on API 31+ with `getCacheBytes()`.
- Residual ("orphan") folders of uninstalled apps under `Android/data` are normally removed by the OS on uninstall. Leftovers in **shared** storage (`/sdcard/<AppName>/`, `Download/`, `Pictures/<App>/`) are visible with All files access. This is CorpseFinder-style evidence: match folder name to package or label, plus `OWNER_PACKAGE_NAME` for MediaStore items.

### 8. Background analysis: WorkManager, jobs, foreground services

Versions:
- **WorkManager 2.12.0** (stable, 2026-09-23) raises `minSdk` from 23 to **24**. It adds an experimental `androidx.work:work-analytics` artifact (WorkMetrics) and execution/scheduling event listeners ([WorkManager releases](https://developer.android.com/jetpack/androidx/releases/work)).

Long-running workers ([guide](https://developer.android.com/develop/background-work/background-tasks/persistent/how-to/long-running)):
- `CoroutineWorker.setForeground(ForegroundInfo(id, notif, FOREGROUND_SERVICE_TYPE_DATA_SYNC))` lets work run longer than 10 minutes.
- Declare the merged `androidx.work.impl.foreground.SystemForegroundService` with `android:foregroundServiceType="dataSync"` and the `FOREGROUND_SERVICE_DATA_SYNC` permission.
- **Starting in Android 16 these can exhaust the job quota.** The guide suggests launching the FGS directly if that happens.

FGS types ([types](https://developer.android.com/develop/background-work/services/fgs/service-types), [timeout](https://developer.android.com/develop/background-work/services/fgs/timeout)):
- `dataSync`: description includes "Import or export operations… **Local file processing**". No runtime prerequisites. 6 h per 24 h on API 35+. Cannot start from `BOOT_COMPLETED`.
- `mediaProcessing` (API 35): "time-consuming operations on media assets, like converting media". It has its own separate 6 h budget. It could plausibly cover perceptual hashing or thumbnailing, but justifying it for *scanning* is weak.
- `specialUse` needs a `PROPERTY_SPECIAL_USE_FGS_SUBTYPE` explanation, and Play reviews it.
- `shortService` has a much shorter limit.

Play FGS declaration ([Play help 13392821](https://support.google.com/googleplay/android-developer/answer/13392821?hl=en)):
- Required for each type on Play Console › App content, with a description, the user impact of deferral or interruption, and a **video**.
- The `TYPE_DATA_SYNC` "Local processing: Other" use case reads: "**Use for specifically user-initiated work and not for regular system or server-initiated tasks**."
- So a *scheduled* nightly deep scan must not use a `dataSync` FGS. Use plain WorkManager constraints (idle, charging, battery not low) and chunked, resumable work instead.

User-initiated data transfer jobs ([UIDT](https://developer.android.com/develop/background-work/background-tasks/uidt)):
- Available from API 34, with the `RUN_USER_INITIATED_JOBS` permission and `setUserInitiated(true)`.
- They are exempt from ordinary quotas, but are meant for **network** transfers (they take a network request and an estimated payload). They don't fit local scanning **(inference)**.

### 9. Other reclaim levers: archiving, uninstall, manage-space

- **`PackageInstaller.requestArchive(pkg, statusReceiver)`** (API 35): "the app's APKs and cache are removed from the device while the user data is kept". Unarchiving happens through the responsible installer, and archived apps remain visible in launchers.
  - Requires `DELETE_PACKAGES` or `REQUEST_DELETE_PACKAGES` ([PackageInstaller](https://developer.android.com/reference/android/content/pm/PackageInstaller), [Android 15 features](https://developer.android.com/about/versions/15/features)).
  - This is the **most reversible app-level reclaim action** available to Lumen, but reversal is **conditional**. Unarchiving re-downloads the APK through the responsible installer, and `PackageInstaller` defines failure states for exactly that path: `UNARCHIVAL_ERROR_INSTALLER_UNINSTALLED`, `UNARCHIVAL_ERROR_INSTALLER_DISABLED`, `UNARCHIVAL_ERROR_NO_CONNECTIVITY` and `UNARCHIVAL_ERROR_INSUFFICIENT_STORAGE`. An app that has been delisted from its store, or that was sideloaded, may not be restorable. Lumen should label archive "reversible if the installer can re-supply the app", record the installer package (`getInstallSourceInfo`) as evidence, and never archive apps whose installer is missing.
  - Still to confirm: whether a user confirmation dialog is shown for non-installer callers, whether Play restricts `REQUEST_DELETE_PACKAGES`, and whether archiving works for non-Play-installed apps **(unverified)**.
- **Uninstall:** `Intent.ACTION_DELETE` or `PackageInstaller.uninstall()` with `REQUEST_DELETE_PACKAGES`. These always ask the user and are irreversible, so policy should rank them below archive.
- **`getManageSpaceActivityIntent`** (§2) defers cleanup to the app's own UI. It is safe because the owning app decides what is expendable.

### 10. Running the Rust core on Android

Tooling verified on 2026-10-05:

| Tool | Version | Source |
|---|---|---|
| Rust stable | 1.99.0 (2026-09-28 build, released 2026-10-01) | [channel-rust-stable.toml](https://static.rust-lang.org/dist/channel-rust-stable.toml) |
| cargo-ndk | 4.1.2 (MSRV 1.86) | [crates.io](https://crates.io/crates/cargo-ndk), [GitHub](https://github.com/bbqsrc/cargo-ndk) |
| UniFFI | 0.32.2 (2026-09-23) | [crates.io](https://crates.io/crates/uniffi), [uniffi-rs](https://github.com/mozilla/uniffi-rs) |
| Android NDK | r30 (2026-09-08) | [android/ndk releases](https://github.com/android/ndk/releases) |
| AGP | 9.4.0 (Sept 2026). Max API 37, Gradle 9.6.0, default NDK 28.2.13676358, JDK 17. AGP 10 makes the new Variant API mandatory. | [AGP release notes](https://developer.android.com/build/releases/gradle-plugin) |
| JNA (UniFFI Kotlin runtime) | 5.19.1 latest on Maven Central. 16 KB fixes in **5.16.0 and 5.17.0** (#1618, #1647). **Avoid 5.19.0.** In 5.19.1, #1730 replaced `MethodHandle` usage with reflection "to restore support for older Android releases". Which Android levels 5.19.0 broke is **(unverified)**. | [JNA CHANGES.md](https://github.com/java-native-access/jna/blob/master/CHANGES.md) |
| `jni` crate (if hand-writing JNI) | 0.22.4 | [crates.io](https://crates.io/crates/jni) |

Build:
- `rustup target add aarch64-linux-android x86_64-linux-android` (add `armv7-linux-androideabi` only if you support 32-bit devices).
- `cargo ndk -t arm64-v8a -t x86_64 --platform <minSdk> -o app/src/main/jniLibs build --release`.
- Generate Kotlin with `uniffi-bindgen generate --language kotlin` (library mode) as a Gradle task.
- UniFFI's Kotlin backend calls through **JNA**: `implementation "net.java.dev.jna:jna:<ver>@aar"` ([UniFFI Gradle docs](https://github.com/mozilla/uniffi-rs/blob/main/docs/manual/src/kotlin/gradle.md)).

16 KB pages ([page-sizes guide](https://developer.android.com/guide/practices/page-sizes)):
- NDK r28+ produces 16 KB-aligned output by default. For older NDKs use `-Wl,-z,max-page-size=16384 -Wl,-z,common-page-size=16384`.
- AGP ≥ 8.5.1 with uncompressed `.so` handles zip alignment.
- Verify with `zipalign -v -c -P 16 4 app.apk`, `bundletool dump config | grep alignment` (expect `PAGE_ALIGNMENT_16K`), and `llvm-readelf -l` (expect LOAD align `2**14`).
- cargo-ndk links through the NDK clang, so r28+/r30 defaults should apply. Do not rely on that: add the linker args explicitly in `.cargo/config.toml` or `build.rs` and **gate CI on the zipalign check** (whether rustc's Android targets default to 16 KB is **unverified**).
- Older JNA AARs ship 4 KB-aligned `libjnidispatch.so` and would fail the check.
- **Don't build with `-C prefer-dynamic`.** As of Rust 1.99 the `*-linux-android` target specs add no page-size linker args. The prebuilt `libstd.so` that rustup ships for `aarch64-linux-android` is 4 KB-aligned, so a dynamically linked Rust runtime fails 16 KB checks ([waterui#156](https://github.com/water-rs/waterui/issues/156), secondary). The default static linking into the `cdylib` is fine when the final link goes through NDK r28+ clang plus the explicit flags above.

Runtime rules for the Rust core:
- **No filesystem assumptions.** Kotlin adapters supply content URIs or file descriptors via `ParcelFileDescriptor.detachFd()`, and Rust takes ownership (`File::from_raw_fd`). Direct paths are used only when All files access is granted.
- **Memory:** Android 17 memory limits mean scanning should stream, keep hash state compact, and have a configurable budget. Check `ApplicationExitInfo` for `MemoryLimiter` kills and report them through telemetry.
- **No downloaded native code** (Android 17 safer native DCL). All `.so` files ship in the AAB.
- Tokio works on Android, but use a small `current_thread` runtime or a bounded worker pool. Cancellation must be wired to `Worker.isStopped` and `onTimeout` **(design inference)**.
- UI layer: if Lumen's mobile shell is Expo/React Native (npm `expo` 57.0.26, `react-native` 0.87.1 latest on 2026-10-05), wrap the UniFFI Kotlin module in an Expo native module. Android-specific system intents (`createTrashRequest`, `ACTION_CLEAR_APP_CACHE`, settings deep links) must live in Kotlin either way **(recommendation)**.

### 11. Competitors: what they can and cannot do

**SD Maid SE** (`eu.darken.sdmse`, GPLv3; latest pre-release v2.2.0-rc0, 2026-09-29) ([README](https://github.com/d4rken-org/sdmaid-se), [FAQ](https://github.com/d4rken-org/sdmaid-se/wiki/FAQ), [AppCleaner](https://github.com/d4rken-org/sdmaid-se/wiki/AppCleaner), [Setup](https://github.com/d4rken-org/sdmaid-se/wiki/Setup)):
- Tools: CorpseFinder (residuals), AppCleaner (expendable files), SystemCleaner (filters), Deduplicator, StorageAnalyzer, AppControl, Scheduler, Media Squeeze, Swiper.
- Distributed on Google Play, F-Droid and GitHub.
- Access is layered. It uses the best method available and retries with the next: root > Shizuku/ADB > Accessibility > All files access + SAF + Usage stats + `QUERY_ALL_PACKAGES`.
- **Internal caches:** "On modern Android, there is no API for apps to clear another app's cache directly. Instead, SD Maid SE navigates to each app's settings page and taps the 'Clear Cache' button" via an **AccessibilityService**, or does it directly with Shizuku or root.
- Known problems:
  - the service gets disabled on update or crash
  - OEM settings differ (MIUI "Security Center")
  - "Some devices block the Accessibility Service" on Android 16
- **Safety pattern worth copying:** if the app list looks incomplete (for example the `android` package is missing), SD Maid shows "Invalid app list" and disables CorpseFinder, AppCleaner and AppControl, "Continuing anyway could mean deleting data that belongs to an app SD Maid simply couldn't see."
- "Manage storage" on Android 13+ "still grants access to all public storage, but not to sub-directories under `Android/data`".

**Files by Google** (`com.google.android.apps.nbu.files`):
- The Clean tab suggests junk files (app temporary files), duplicates, large files, old screenshots and unused apps. Users review suggestions, and "junk files" deletion is permanent ([Files Help](https://support.google.com/files/answer/9713869?hl=en)).
- It is preinstalled on many devices. I could not find public documentation of how it clears app caches, or whether it holds privileged permissions on GMS devices **(unverified)**. Lumen should not assume parity.

**Accessibility as a cleaning mechanism: policy assessment** ([Play help 10964491](https://support.google.com/googleplay/android-developer/answer/10964491?hl=en)):
- Apps targeting API 31+ with an AccessibilityService must file a declaration.
- Non-tool apps must show an in-app **prominent disclosure** with affirmative consent, and provide a **video** of it.
- "Other examples of apps that are not accessibility tools are: antivirus software, automation tools, assistants, monitoring apps, **cleaners**…"
- **New rule:** "Any use of the Accessibility API that enables an app to **autonomously initiate, plan, and execute actions or decisions is strictly prohibited**." Only "deterministic, rule-based automation… static, human-defined script" is allowed.
- Android 13+ "restricted settings" stop sideloaded apps from enabling accessibility easily ([Android Help](https://support.google.com/android/answer/12623953?hl=en)). AAPM (Android 17) revokes it from non-tool apps (confirmed by the Google Security blog, 2026-10-01).
- **Shizuku** (v13.6.0, 2025-05-25) requires a one-time ADB pairing or wireless debugging and must be restarted after reboot. It is a power-user path with no Play declaration of its own. Whether Play scrutinises apps that call Shizuku is **(unverified)**.

## Implications for Lumen

1. **Target and minimum SDK.** Ship with `targetSdk 36` now and plan for 37 in 2027. Use `minSdk 30` (Android 11), or 31 to get `getExternalCacheBytes`, `MANAGE_MEDIA` and `getManageSpaceActivityIntent` without branching.
   - *Why:* below 30 there is no `createTrashRequest`, `IS_TRASHED`, `ACTION_CLEAR_APP_CACHE` or generation-based MediaStore sync, and the legacy storage model doubles the adapter surface.
   - *Rejected:* minSdk 24 (WorkManager's floor). Too much dual-path code for little remaining reach **(market share unverified)**.

2. **Model Android access as tiered capabilities.** Each scan's evidence graph should record its tier and blind spots:

   | Tier | Grants | Lumen can |
   |---|---|---|
   | T0 (no special access) | — | `getTotalBytes`/`getFreeBytes`, own storage, Photo Picker-based spot checks, `ACTION_MANAGE_STORAGE` |
   | T1 Usage access | `PACKAGE_USAGE_STATS` (+ `QUERY_ALL_PACKAGES`) | per-app app/data/cache sizes, last-used, standby-bucket evidence → "unused app" and "cache-heavy app" findings; deep links to app settings |
   | T2 Media | `READ_MEDIA_*`, possibly partial | large media, duplicate/similar media, trash-based reversible cleanup |
   | T3 All files access | `MANAGE_EXTERNAL_STORAGE` | full shared-storage tree (incl. `Download/`, residual folders, `.thumbnails`, APKs), `ACTION_CLEAR_APP_CACHE`, `getManageSpaceActivityIntent` |

   - *Why:* the policy engine must not turn "not observed" into "absent". The Rust core's port should take a `CoverageReport` alongside the observations.

3. **Play listing strategy.** Position Lumen in the store listing as a **file management and maintenance** tool with on-device search and analysis. That is the permitted category for `MANAGE_EXTERNAL_STORAGE` and `QUERY_ALL_PACKAGES`.
   - Prepare both declaration forms, the FGS declaration video and prominent-disclosure screens before the first submission.
   - Don't use All files access for media-only features. MediaStore has to work on its own.
   - *Rejected:* a "media-only" Play variant that declares `MANAGE_EXTERNAL_STORAGE`. That is explicitly invalid.

4. **No AccessibilityService in the Play build.** Never let Jev drive device actions. Internal caches get a **guided** flow instead:
   - Rank apps by `StorageStats.cacheBytes` and deep-link one at a time to `ACTION_APPLICATION_DETAILS_SETTINGS`, or to the app's manage-space activity where available.
   - Verify afterwards by re-querying `queryStatsForUid`.
   - *Why:*
     - Play says cleaners aren't accessibility tools.
     - The autonomy prohibition clashes with any AI-in-the-loop design.
     - AAPM revokes the access.
     - OEM UI differences make the automation brittle.
     - Tapping "Clear cache" can't be undone.
   - *Rejected:* SD Maid-style Accessibility automation. A Shizuku adapter is acceptable later as an **opt-in power-user adapter** behind the same port. It belongs in a non-Play or advanced build, with deterministic, user-confirmed, per-app actions only.

5. **Cleanup by artifact type, mapped to verdicts.** The reversibility differs per artifact, so the verdicts differ too:
   - **MediaStore items:** QUARANTINE = `createTrashRequest(…, true)`. Rollback = `createTrashRequest(…, false)`. Verify by re-querying with `QUERY_ARG_MATCH_TRASHED=MATCH_ONLY`.
     - Lumen's rollback window must be shorter than `DATE_EXPIRES` (about 30 days, system-defined). Read the actual `DATE_EXPIRES` and show it.
     - Trashed items survive a Lumen uninstall, which is a safety plus. Trashing doesn't change the item's owner, and on uninstall MediaProvider's `onPackageOrphaned()` only nulls `OWNER_PACKAGE_NAME` for the removed package. The one exception is missing-file `Android/media/<pkg>` rows, which it deletes (AOSP `MediaProvider.java`). The flip side is that Lumen loses the ability to un-trash after it is uninstalled, and items then expire on the system schedule.
     - Optional `MANAGE_MEDIA` removes the dialog for each batch.
   - **Non-media files (T3):** quarantine by **renaming within the same volume** into a Lumen quarantine folder in *shared* storage. Use `Documents/Lumen/.quarantine/<txn>/` with `.nomedia` and a manifest holding the original path, size, hash and mtime.
     - Do **not** quarantine into `getExternalFilesDir()`. App-specific dirs are wiped on uninstall, so quarantined files would be lost along with Lumen.
     - Whether renaming across shared and app-specific trees is atomic under FUSE is **(unverified)**. Treat a non-atomic move as copy, then verify the hash, then delete.
   - **External caches of all apps (`ACTION_CLEAR_APP_CACHE`):** not quarantinable and all-or-nothing. Allow it only as an explicit user action labelled "system cache clean (irreversible, regenerable)", never as an automatic step. Measure before and after with `queryExternalStatsForUser`.
   - **Unused apps:** prefer `requestArchive` (keeps data) over uninstall. Evidence comes from `UsageStats` last-used, `isAppInactive` and `StorageStats`.
     - Archive is only conditionally reversible because restoring needs the responsible installer, connectivity and free space (§9). Offer it only when `getInstallSourceInfo()` names an installer that is still installed and enabled, and show "restore needs `{installer}` and network" in the confirmation.

6. **Background design.**
   - Periodic WorkManager job: incremental MediaStore sync via generation numbers plus StorageStats snapshots. Constraints: idle, charging, battery not low. Make it chunked and resumable, with checkpoints in Lumen's DB.
   - User-initiated deep scan: a WorkManager long-running worker with a `dataSync` FGS. Implement `onTimeout` and checkpoint state. Never start it from boot. Log `getStopReason()` to OpenTelemetry.
   - *Rejected:*
     - always-on FGS (Play policy and battery)
     - `specialUse` (review risk)
     - UIDT jobs (network-oriented)

7. **Native packaging.** Keep one Rust crate for the core with UniFFI 0.32.x plus JNA ≥ 5.17 (pin 5.19.1). Build with cargo-ndk 4.1.2 and NDK r30, ABIs `arm64-v8a` and `x86_64`.
   - CI gates: `zipalign -P 16` and `llvm-readelf` alignment checks, plus an Android 17 emulator run with `am memory-limiter manual` to test low memory limits.
   - *Rejected:*
     - hand-written JNI (more unsafe surface)
     - shipping `armeabi-v7a` by default (size, plus a 32-bit memory ceiling for hashing). Revisit if analytics show demand.

8. **Safety guards copied from the field.**
   - Disable residual-data detection and app-attributed deletions when the visible package list looks truncated. Examples: no `android` package, or the count is far below the `StorageStats` user totals.
   - Treat private-space and work-profile data as out of scope and say so in the UI.

## Risks and open questions

- **Play approval risk:** `MANAGE_EXTERNAL_STORAGE` and `QUERY_ALL_PACKAGES` approvals are discretionary. Lumen needs a degraded but useful T1/T2 product if T3 is rejected. Run a test-track submission early.
- **`REQUEST_DELETE_PACKAGES` and `requestArchive`:** the permission is `normal`. It isn't among the 12 pages under Play Console Help's "Use of app permissions and APIs" section (checked 2026-10-06), so no dedicated declaration form is known. Still to confirm whether Play restricts it in review, what confirmation UX non-installer callers get, and whether archiving works for apps that weren't installed from Play. **(unverified)**
- **`MANAGE_MEDIA` prerequisite on API 33+:** the doc still says "`READ_EXTERNAL_STORAGE` or `MANAGE_EXTERNAL_STORAGE`". Test whether `READ_MEDIA_*` satisfies it. **(unverified)**
- **`DATE_EXPIRES` for trash is "typically" 30 days.** OEMs may differ. Read the per-item value and never promise a longer rollback window than it allows.
- **Quarantine semantics under FUSE:** atomicity of `rename()` between shared-storage directories, and the performance of large moves on SD cards (separate volumes need copy plus delete).
- **Rust/rustc 16 KB defaults:** a third-party report says the `*-linux-android` target specs set no page-size args and that rustup's prebuilt `libstd.so` is 4 KB-aligned. This hasn't been confirmed from an official Rust source. Link statically (never `prefer-dynamic`), pass the linker flags explicitly, and rely on CI verification.
- **Android 17 AAPM accessibility revocation** has been confirmed by Google's Security blog (2026-10-01), but no developer.android.com behaviour-change page documents it yet. It doesn't affect Lumen if Recommendation 4 is followed.
- **Files by Google internals** (privileged permissions, cache-clearing mechanism) are undocumented, so competitive parity claims are uncertain.
- **UsageStats retention and accuracy** vary by OEM. "Unused for N days" must remain REVIEW-level evidence, never auto-QUARANTINE on its own.
- **Next Play deadlines:** targetSdk 37 (likely Aug 2027) and 16 KB enforcement (2027-02-01). Also track whether `dataSync` gets further restrictions in Android 18.
- **Expo/RN vs native Kotlin UI** for the Android shell is undecided. It affects how system intents and `startIntentSenderForResult` flows get wired.

## Sources

- [Target API level requirements (Android Developers)](https://developer.android.com/google/play/requirements/target-sdk)
- [Target API level requirements for Google Play apps (Play Console Help 11926878)](https://support.google.com/googleplay/android-developer/answer/11926878?hl=en)
- [StorageManager reference](https://developer.android.com/reference/android/os/storage/StorageManager)
- [StorageStatsManager reference](https://developer.android.com/reference/android/app/usage/StorageStatsManager)
- [StorageStats reference](https://developer.android.com/reference/android/app/usage/StorageStats)
- [UsageStatsManager reference](https://developer.android.com/reference/android/app/usage/UsageStatsManager)
- [UsageStats reference](https://developer.android.com/reference/android/app/usage/UsageStats)
- [Manifest.permission reference](https://developer.android.com/reference/android/Manifest.permission)
- [MediaStore reference](https://developer.android.com/reference/android/provider/MediaStore)
- [MediaStore.MediaColumns reference](https://developer.android.com/reference/android/provider/MediaStore.MediaColumns)
- [PackageInstaller reference](https://developer.android.com/reference/android/content/pm/PackageInstaller)
- [Access app-specific files](https://developer.android.com/training/data-storage/app-specific)
- [Access media files from shared storage](https://developer.android.com/training/data-storage/shared/media)
- [Access documents and other files from shared storage (SAF)](https://developer.android.com/training/data-storage/shared/documents-files)
- [Manage all files on a storage device](https://developer.android.com/training/data-storage/manage-all-files)
- [Storage updates in Android 11](https://developer.android.com/about/versions/11/privacy/storage)
- [Grant partial access to photos and videos (Android 14)](https://developer.android.com/about/versions/14/changes/partial-photo-video-access)
- [Photo picker](https://developer.android.com/training/data-storage/shared/photopicker)
- [Package visibility filtering](https://developer.android.com/training/package-visibility)
- [Packages visible automatically](https://developer.android.com/training/package-visibility/automatic)
- [Foreground service types](https://developer.android.com/develop/background-work/services/fgs/service-types)
- [Foreground service timeouts](https://developer.android.com/develop/background-work/services/fgs/timeout)
- [Support for long-running workers (WorkManager)](https://developer.android.com/develop/background-work/background-tasks/persistent/how-to/long-running)
- [User-initiated data transfer jobs](https://developer.android.com/develop/background-work/background-tasks/uidt)
- [WorkManager releases](https://developer.android.com/jetpack/androidx/releases/work)
- [Android Gradle plugin release notes](https://developer.android.com/build/releases/gradle-plugin)
- [Support 16 KB page sizes](https://developer.android.com/guide/practices/page-sizes)
- [Android 17 overview](https://developer.android.com/about/versions/17)
- [Android 17 features](https://developer.android.com/about/versions/17/features)
- [Android 17 summary of changes](https://developer.android.com/about/versions/17/summary)
- [Behavior changes: all apps (Android 17)](https://developer.android.com/about/versions/17/behavior-changes-all)
- [Behavior changes: apps targeting Android 17](https://developer.android.com/about/versions/17/behavior-changes-17)
- [Behavior changes: all apps (Android 16)](https://developer.android.com/about/versions/16/behavior-changes-all)
- [Behavior changes: apps targeting Android 16](https://developer.android.com/about/versions/16/behavior-changes-16)
- [Behavior changes: all apps (Android 15)](https://developer.android.com/about/versions/15/behavior-changes-all)
- [Behavior changes: apps targeting Android 15](https://developer.android.com/about/versions/15/behavior-changes-15)
- [Android 15 features (app archiving)](https://developer.android.com/about/versions/15/features)
- [Use of All files access (MANAGE_EXTERNAL_STORAGE) permission – Play Console Help](https://support.google.com/googleplay/android-developer/answer/10467955?hl=en)
- [Use of the broad package visibility (QUERY_ALL_PACKAGES) permission – Play Console Help](https://support.google.com/googleplay/android-developer/answer/10158779?hl=en)
- [Use of the AccessibilityService API – Play Console Help](https://support.google.com/googleplay/android-developer/answer/10964491?hl=en)
- [Understanding foreground service and full-screen intent requirements – Play Console Help](https://support.google.com/googleplay/android-developer/answer/13392821?hl=en)
- [Understanding Restricted Permissions with minimum scope alternatives (Photos/Videos, Contacts) – Play Console Help](https://support.google.com/googleplay/android-developer/answer/14115180?hl=en)
- [Deceptive Behavior – Play Console Help](https://support.google.com/googleplay/android-developer/answer/17006354?hl=en)
- [Restricted settings – Android Help](https://support.google.com/android/answer/12623953?hl=en)
- [Android Advanced Protection Mode (developer guide)](https://developer.android.com/privacy-and-security/advanced-protection-mode)
- [WorkManager 2.12.0 release notes](https://developer.android.com/jetpack/androidx/releases/work#2.12.0)
- [Clear your junk files – Files by Google Help](https://support.google.com/files/answer/9713869?hl=en)
- [AOSP MediaProvider CacheClearingActivity.java](https://android.googlesource.com/platform/packages/providers/MediaProvider/+/master/src/com/android/providers/media/CacheClearingActivity.java)
- [AOSP MediaProvider util/FileUtils.java (clearAppCacheDirectories)](https://android.googlesource.com/platform/packages/providers/MediaProvider/+/refs/heads/main/src/com/android/providers/media/util/FileUtils.java)
- [AOSP MediaProvider MediaProvider.java (hasPermissionToClearCaches)](https://android.googlesource.com/platform/packages/providers/MediaProvider/+/refs/heads/main/src/com/android/providers/media/MediaProvider.java)
- [AOSP frameworks/base core/res/AndroidManifest.xml @ android-5.1.1_r1](https://android.googlesource.com/platform/frameworks/base/+/refs/tags/android-5.1.1_r1/core/res/AndroidManifest.xml)
- [AOSP frameworks/base core/res/AndroidManifest.xml @ android-6.0.0_r1](https://android.googlesource.com/platform/frameworks/base/+/refs/tags/android-6.0.0_r1/core/res/AndroidManifest.xml)
- [Esper: Android 13 closes file manager loophole](https://www.esper.io/blog/android-dessert-bites-28-file-manager-loophole-closed-73891524)
- [SD Maid SE – GitHub README](https://github.com/d4rken-org/sdmaid-se)
- [SD Maid SE wiki – FAQ](https://github.com/d4rken-org/sdmaid-se/wiki/FAQ)
- [SD Maid SE wiki – AppCleaner](https://github.com/d4rken-org/sdmaid-se/wiki/AppCleaner)
- [SD Maid SE wiki – Setup](https://github.com/d4rken-org/sdmaid-se/wiki/Setup)
- [The Hacker News: Android 17 blocks non-accessibility apps from Accessibility API (AAPM)](https://thehackernews.com/2026/03/android-17-blocks-non-accessibility.html)
- [The Hacker News: Android 17 Advanced Protection locks Accessibility Services to verified Accessibility Tools (2026-10-02)](https://thehackernews.com/2026/10/android-17-advanced-protection-locks.html)
- [Google blog: Android Advanced Protection updates (2026-10-01)](https://blog.google/security/android-advanced-protection-updates/)
- [Wikipedia: Android 17](https://en.wikipedia.org/wiki/Android_17)
- [cargo-ndk (GitHub)](https://github.com/bbqsrc/cargo-ndk) / [crates.io](https://crates.io/crates/cargo-ndk)
- [uniffi-rs (GitHub)](https://github.com/mozilla/uniffi-rs) / [crates.io](https://crates.io/crates/uniffi)
- [UniFFI manual – Integrating with Gradle (Kotlin, JNA)](https://github.com/mozilla/uniffi-rs/blob/main/docs/manual/src/kotlin/gradle.md)
- [JNA CHANGES.md](https://github.com/java-native-access/jna/blob/master/CHANGES.md)
- [Android NDK releases (GitHub)](https://github.com/android/ndk/releases)
- [Rust stable channel manifest](https://static.rust-lang.org/dist/channel-rust-stable.toml)
- [jni crate (crates.io)](https://crates.io/crates/jni)
- [Shizuku releases (GitHub)](https://github.com/RikkaApps/Shizuku)
- [waterui#156: rustup's prebuilt Android libstd dylib is 4 KB-aligned](https://github.com/water-rs/waterui/issues/156)

## Verification log

Adversarial fact-check run on 2026-10-06. Each claim was checked against the primary source; "confirmed" means the live page or source text matched.

| # | Claim | Verdict | Source |
|---|---|---|---|
| 1 | Play: new apps and updates target API 36 from 2026-08-31, extension to 2026-11-01; existing apps ≥ API 35 | Confirmed | [Target API requirements](https://developer.android.com/google/play/requirements/target-sdk) |
| 2 | 16 KB: required for targetSdk 35+, updates blocked from 2027-02-01; NDK r28+ default; AGP ≥ 8.5.1; zipalign/bundletool checks (page updated 2026-09-16) | Confirmed | [Page sizes guide](https://developer.android.com/guide/practices/page-sizes) |
| 3 | `CLEAR_APP_CACHE` went from `dangerous` (5.1.1) to `signature\|privileged` (6.0); `DELETE_CACHE_FILES` "no longer used" | Confirmed | AOSP manifests at `android-5.1.1_r1` / `android-6.0.0_r1`; [Manifest.permission](https://developer.android.com/reference/android/Manifest.permission) |
| 4 | `ACTION_CLEAR_APP_CACHE` clears *external* caches only, needs `MANAGE_EXTERNAL_STORAGE`, shows a dialog, returns OK/EIO/CANCELED | Confirmed | [StorageManager](https://developer.android.com/reference/android/os/storage/StorageManager) |
| 5 | `getManageSpaceActivityIntent` (API 31) needs `MANAGE_EXTERNAL_STORAGE` (the guide adds `QUERY_ALL_PACKAGES`) | Confirmed | StorageManager; [Manage all files](https://developer.android.com/training/data-storage/manage-all-files) |
| 6 | Protection levels: `PACKAGE_USAGE_STATS` = `signature\|privileged\|development\|appop\|retailDemo`; `MANAGE_EXTERNAL_STORAGE` and `MANAGE_MEDIA` = `signature\|appop\|preinstalled`; `QUERY_ADVANCED_PROTECTION_MODE` API 36 | Confirmed | Manifest.permission |
| 7 | `MANAGE_MEDIA` prerequisite text still says `READ_EXTERNAL_STORAGE` or `MANAGE_EXTERNAL_STORAGE` (no mention of `READ_MEDIA_*`) | Confirmed (interaction with `READ_MEDIA_*` remains unverified) | Manifest.permission |
| 8 | Play All files access: permitted uses (file management incl. maintenance, backup, AV, document mgmt, on-device search, encryption, migration); invalid uses (media files, manual file selection) | Confirmed | [Play 10467955](https://support.google.com/googleplay/android-developer/answer/10467955?hl=en) |
| 9 | Play `QUERY_ALL_PACKAGES`: permitted uses are device search, antivirus, file managers, browsers | Confirmed, with an addition: the policy also says the app must "have a core purpose to search for all apps on the device" (added to §6) | [Play 10158779](https://support.google.com/googleplay/android-developer/answer/10158779?hl=en) |
| 10 | Play Accessibility: "cleaners" are not accessibility tools; autonomy prohibition; declaration + video | Confirmed | [Play 10964491](https://support.google.com/googleplay/android-developer/answer/10964491?hl=en) |
| 11 | Android 17 AAPM revokes accessibility from non-`isAccessibilityTool` apps | **Corrected:** was marked "secondary/unverified", now officially confirmed by the Google blog (2026-10-01) | [Google blog](https://blog.google/security/android-advanced-protection-updates/) |
| 12 | Android 17 stable on 2026-06-16; QPR1 = 37.1, QPR2 beta = 37.2 | Confirmed (date via Wikipedia citing Google's launch post; QPRs on the overview page) | [Android 17 overview](https://developer.android.com/about/versions/17), [Wikipedia](https://en.wikipedia.org/wiki/Android_17) |
| 13 | Android 17 memory limits (`MemoryLimiter:AnonSwap`, `am memory-limiter`) and read-only `System.load()` (targeting 37) | Confirmed | [A17 all apps](https://developer.android.com/about/versions/17/behavior-changes-all), [A17 targeting](https://developer.android.com/about/versions/17/behavior-changes-17) |
| 14 | `dataSync`/`mediaProcessing` 6 h/24 h with separate budgets, `onTimeout`, timer resets on foreground; `dataSync` blocked from `BOOT_COMPLETED` | Confirmed | [FGS timeout](https://developer.android.com/develop/background-work/services/fgs/timeout), [A15 targeting](https://developer.android.com/about/versions/15/behavior-changes-15) |
| 15 | Play FGS "Local processing: Other" = "specifically user-initiated work"; UIDT recommended for network transfers | Confirmed | [Play 13392821](https://support.google.com/googleplay/android-developer/answer/13392821?hl=en) |
| 16 | Android 16 job quota now applies to jobs that continue after the app leaves the foreground or run alongside an FGS (WorkManager/JobScheduler/DownloadManager); `MediaStore#getVersion` unique per app; 16 KB compat mode | Confirmed | [A16 all apps](https://developer.android.com/about/versions/16/behavior-changes-all), [A16 targeting](https://developer.android.com/about/versions/16/behavior-changes-16) |
| 17 | MediaStore pending expiry ~7 days, trashed ~30 days (`DATE_EXPIRES`); `QUERY_ARG_MEDIA_STANDARD_SORT_ORDER` API 36 | Confirmed; added that the sort arg is also in R Extensions 15 | [MediaColumns](https://developer.android.com/reference/android/provider/MediaStore.MediaColumns), [MediaStore](https://developer.android.com/reference/android/provider/MediaStore) |
| 18 | Trashed items survive a Lumen uninstall | Confirmed from source: `onPackageOrphaned` only nulls the owner | AOSP `MediaProvider.java` |
| 19 | Photo/Video policy enforcement mandatory since 2025-05-28; Contacts policy 2027-01-27 for targetSdk 37+ | Confirmed | [Play 14115180](https://support.google.com/googleplay/android-developer/answer/14115180?hl=en) |
| 20 | SAF tree cannot grant volume root, reliable SD root, `Download/`; no file selection in `Android/data`/`Android/obb`; All files access excludes `Android/data` | Confirmed | [SAF guide](https://developer.android.com/training/data-storage/shared/documents-files), [Manage all files](https://developer.android.com/training/data-storage/manage-all-files) |
| 21 | Android 13 closed the `EXTRA_INITIAL_URI` `Android/data` loophole via `shouldBlockFromTree` | Confirmed (secondary) | [Esper](https://www.esper.io/blog/android-dessert-bites-28-file-manager-loophole-closed-73891524) |
| 22 | `requestArchive` (API 35) needs `DELETE_PACKAGES` or `REQUEST_DELETE_PACKAGES`; keeps user data | Confirmed. **Added** that reversal depends on the installer (`UNARCHIVAL_ERROR_INSTALLER_UNINSTALLED`/`_DISABLED`/`_NO_CONNECTIVITY`/`_INSUFFICIENT_STORAGE`) | [PackageInstaller](https://developer.android.com/reference/android/content/pm/PackageInstaller), [A15 features](https://developer.android.com/about/versions/15/features) |
| 23 | WorkManager 2.12.0 stable, minSdk 23→24, experimental `work-analytics` | Confirmed | [WorkManager releases](https://developer.android.com/jetpack/androidx/releases/work) |
| 24 | AGP 9.4.0 (Sept 2026): max API 37, Gradle 9.6.0, default NDK 28.2.13676358, JDK 17; 9.4.1 on Google Maven | Confirmed | [AGP notes](https://developer.android.com/build/releases/gradle-plugin), Google Maven metadata |
| 25 | Tooling: Rust 1.99.0 (2026-10-01), NDK r30 (2026-09-08), UniFFI 0.32.2 (2026-09-23), cargo-ndk 4.1.2, jni 0.22.4, expo 57.0.26, react-native 0.87.1, Shizuku v13.6.0, SD Maid SE v2.2.0-rc0 (2026-09-29) | Confirmed | Rust channel manifest, GitHub releases API, crates.io API, npm registry |
| 26 | JNA 5.19.1 latest; 16 KB fixes in 5.16.0 (#1618) and 5.17.0 (#1647) | Confirmed; added "avoid 5.19.0" (#1730) | [JNA CHANGES.md](https://github.com/java-native-access/jna/blob/master/CHANGES.md), Maven Central metadata |
| 27 | Whether rustc Android targets default to 16 KB | Still unverified officially. Third-party report says no, and that the prebuilt `libstd.so` is 4 KB-aligned; added a "no `prefer-dynamic`" rule | [waterui#156](https://github.com/water-rs/waterui/issues/156) |
| 28 | SD Maid SE "Invalid app list" guard; OEM `GET_INSTALLED_APPS`; AAPM blocks its accessibility service on Android 17 | Confirmed (`GET_INSTALLED_APPS` added to §3) | [SD Maid SE Setup wiki](https://github.com/d4rken-org/sdmaid-se/wiki/Setup) |
| 29 | New API 37.2 `queryAppUsageDuration` / `QUERY_APP_USAGE` | **Added:** `internal\|role`, so not usable by Lumen | [UsageStatsManager](https://developer.android.com/reference/android/app/usage/UsageStatsManager) |
| 30 | Cited URLs (The Hacker News Mar 2026, Esper, Wikipedia, Files Help, Restricted settings Help, SD Maid wiki, UniFFI Gradle doc, UIDT, package-visibility automatic, Play Deceptive Behavior 17006354) | All resolve (HTTP 200 with matching titles) | curl |

Still unverified: `MANAGE_MEDIA` combined with `READ_MEDIA_*` on API 33+; the `create*Request` batch limit; UsageStats retention per interval; FUSE rename atomicity; Files by Google internals; whether Play scrutinises Shizuku-calling apps; the archive confirmation UX for non-installer callers; and the targetSdk 37 deadline (expected Aug 2027).
