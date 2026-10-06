# iOS Storage APIs and Mobile Framework Choice

> Researched: 2026-10-05 · Scope: What Lumen can and cannot do on iOS (sandbox, capacity, PhotoKit, Files/iCloud, background work, App Review, Foundation Models), and how to build the iOS/Android apps around the Rust core (React Native/Expo vs native vs KMP vs Flutter).

## Summary

- **iOS is a "Photos + user-granted folders + own container" product, not a system cleaner.** An iOS app only sees its own container, so it cannot inspect or clear other apps' caches or system caches ([File System Programming Guide](https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/FileSystemProgrammingGuide/FileSystemOverview/FileSystemOverview.html); [App Review 2.5.2](https://developer.apple.com/app-store/review/guidelines/)). Any claim to do so is both impossible and grounds for rejection.
- **What Lumen can do on iOS:** (1) show volume capacity, (2) analyse and delete photos and videos through PhotoKit (each delete asks the user, and deleted items go to Recently Deleted), (3) scan folders the user picks in Files or iCloud Drive through security-scoped URLs, (4) **evict downloaded iCloud Drive items**. Eviction is a reversible way to free space: the file stays in iCloud and can be downloaded again ([`evictUbiquitousItem(at:)`](https://developer.apple.com/documentation/foundation/filemanager/evictubiquitousitem(at:))). (5) Manage Lumen's own caches.
- **Photo sizes now have a supported API.** `PHAssetResource.dataSize` (`Int?`) is **public from iOS 27.0** (verified in the Apple doc JSON). On iOS 26 and earlier the only route is the undocumented KVC `value(forKey: "fileSize")`, which Lumen should not use in production.
- **No API reaches the system Duplicates album.** `PHAssetCollectionSubtype` has no Duplicates or Recently Deleted case ([docs](https://developer.apple.com/documentation/photos/phassetcollectionsubtype)). Lumen has to find near-duplicates itself using Vision `GenerateImageFeaturePrintRequest` / `FeaturePrintObservation.distance(to:)` (iOS 18+). It should also point users to Apple's own Merge flow.
- **Every `PHPhotoLibrary.performChanges` call shows a system confirmation alert**, so deletes must be batched into one change block ([docs](https://developer.apple.com/documentation/photos/phphotolibrary/performchanges(_:completionhandler:))). Deleted assets stay in Recently Deleted for 30 days ([Apple Support](https://support.apple.com/en-us/104967)). There is no API to restore them, so for photos Lumen's quarantine is the system's Recently Deleted album.
- **A photo delete is not a local-only action.** With iCloud Photos on, "when you use iCloud Photos and delete a photo or video on one device, it gets deleted on all other devices where you're signed in with the same Apple Account" ([Apple Support](https://support.apple.com/en-us/104967)). With Optimize iPhone Storage on (the default), the device holds only space-saving versions, so deleting frees far less local space than the original's size ([Apple Support](https://support.apple.com/en-us/105061)). Space also comes back only when Recently Deleted is purged (after 30 days or when the user empties it). Lumen's confirmation UI and its "reclaimable" numbers must reflect all three facts.
- **`BGContinuedProcessingTask` is real and GA on iOS/iPadOS 26.0+** ([docs](https://developer.apple.com/documentation/backgroundtasks/bgcontinuedprocessingtask)). It must be started by a user action, shows a Live Activity with progress, and the user can cancel it. GPU use needs an entitlement. It suits "Analyze my library" runs. `BGProcessingTask` (iOS 13+) covers opportunistic maintenance while charging.
- **App Review risk is real.** 2.3.1(a) bans marketing features the app does not have ("iOS-based virus and malware scanners" is Apple's own example). 1.1.6 bans "inaccurate device data". 2.4.4 bans suggesting restarts or settings changes. 5.6 bans manipulative practices. "Speeds up your phone", fake scan alarms and junk-GB counters will get the app rejected.
- **Foundation Models (iOS/macOS 26+):** the on-device model's context window is **4,096 tokens** per Apple's context-window article, but WWDC26 says the rebuilt iOS 27 on-device model reports `contextSize` = **8,192**; read `contextSize` at runtime rather than hard-coding either. The framework offers `@Generable`/`@Guide` guided generation with constrained sampling, tool calling, and `SystemLanguageModel.availability` checks. **iOS 27 added `PrivateCloudComputeLanguageModel`** (32K context, server-side, daily quota) and image `Attachment`s. This fits Jev as an *optional evidence contributor*. PCC sends data off the device, so it must be **off by default**.
- **React Native 0.87 is the current stable release** (2026-08-10). 0.88 is due 2026-10-12. The New Architecture has been the only architecture since 0.82, and 0.87 removed the `useTurboModules` flag ([releases](https://reactnative.dev/docs/releases); [0.87](https://reactnative.dev/blog/2026/08/11/react-native-0.87)).
- **Expo SDK 57 is the current stable Expo release** (2026-06-30, RN 0.86). **SDK 58 is in beta** (2026-09-15, RN 0.88 RC, with stable to follow RN 0.88). It adds iOS 27 scene-lifecycle support and **Expo Modules 2.0**, an annotation-based Swift/Kotlin API whose sync calls are 2.5 to 5.6 times faster. Legacy Architecture support ended after SDK 54. The minimum iOS version is 16.4 since SDK 56.
- **`uniffi-bindgen-react-native` is at 0.31.0-6** (2026-09-25) and still calls itself "early development, should not yet be used in production". UniFFI itself is at **0.32.2** (crates.io, 2026-09-23; 0.32.1 was 2026-09-09 UTC), and its Swift and Kotlin bindings are production-grade (they ship in Firefox).
- **Recommendation:** use **Expo (development builds and Continuous Native Generation) with the UI in React Native**. Put all platform work in **local Expo Modules written in Swift and Kotlin** (the hexagonal platform adapters), and have those modules call the **Rust core through UniFFI's Swift and Kotlin bindings**. Do not bind Rust to JS directly. JS gets paged view-models and progress events only. Kotlin Multiplatform and Flutter are rejected. Fully native SwiftUI + Compose is the documented fallback if the RN layer turns out to be a liability.

## Findings

### 1. iOS sandbox: what an app can see

- At install, iOS creates a **bundle container** and a **data container** for each app. The data container holds `Documents/`, `Library/` (with `Library/Caches/`, which is not backed up and which the system may purge) and `tmp/` (which the system purges when the app is not running). An app's file-system access is limited to its own sandbox, apart from public system interfaces such as Photos and Contacts ([File System Programming Guide – File System Basics](https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/FileSystemProgrammingGuide/FileSystemOverview/FileSystemOverview.html)).
- App Review 2.5.2: apps "may not read or write data outside the designated container area" ([guidelines](https://developer.apple.com/app-store/review/guidelines/)). **Lumen cannot:**
  - enumerate other apps' containers or caches;
  - read the per-app usage breakdown in Settings › General › iPhone Storage. No public API exposes it, to my knowledge (unverified, but nothing in Foundation or UIKit provides it);
  - clear "System Data" or "Other";
  - offload apps.
- **What Lumen can measure for itself:**
  - `URLResourceKey.totalFileAllocatedSizeKey` gives total allocated bytes including metadata (iOS 5+); `fileAllocatedSizeKey` and `fileSizeKey` are alternatives ([docs](https://developer.apple.com/documentation/foundation/urlresourcekey/totalfileallocatedsizekey)). Enumerate the container with `FileManager.enumerator(at:includingPropertiesForKeys:)` and prefetch these keys.
  - Use `isExcludedFromBackupKey` for Lumen's own caches and index DB. Apple's note: re-set it on every save, because common file operations reset it ([docs](https://developer.apple.com/documentation/foundation/urlresourcekey/isexcludedfrombackupkey)).
- **Volume capacity keys** (all read-only, on `URL(fileURLWithPath: "/")` or any URL on the volume) ([Checking volume storage capacity](https://developer.apple.com/documentation/foundation/checking-volume-storage-capacity)):

  | Key | Since | Meaning / use |
  |---|---|---|
  | `volumeTotalCapacityKey` | iOS 4.0 | Total bytes |
  | `volumeAvailableCapacityKey` | iOS 4.0 | Raw free bytes (ignores purgeable space; usually understates what is usable) |
  | `volumeAvailableCapacityForImportantUsageKey` | iOS 11 | Capacity for data the user requested or the app needs. **Use this for the headline "available" number.** |
  | `volumeAvailableCapacityForOpportunisticUsageKey` | iOS 11 | Capacity for prefetch or optional data |

  - **These are "Required Reason APIs".** The `volumeAvailableCapacityForImportantUsageKey` page warns about fingerprinting and requires a declaration in `PrivacyInfo.xcprivacy` ([docs](https://developer.apple.com/documentation/foundation/urlresourcekey/volumeavailablecapacityforimportantusagekey); [Describing use of required reason API](https://developer.apple.com/documentation/bundleresources/describing-use-of-required-reason-api)).
  - Lumen needs at least the *Disk space* category and probably the *File timestamp* category (it reads mtimes for staleness evidence). The applicable reason codes are confirmed against Apple's [NSPrivacyAccessedAPITypeReasons](https://developer.apple.com/documentation/bundleresources/app-privacy-configuration/nsprivacyaccessedapitypes/nsprivacyaccessedapitypereasons) page. Disk space: `85F4.1` (display disk space to the user) and `E174.1` (check whether there is enough space to write files, or delete files when space is low). File timestamp: `C617.1` (files inside the app container, app group or CloudKit container), `3B52.1` (files or directories the user specifically granted access to, e.g. via a document picker) and `DDA9.1` if Lumen shows file timestamps to the user. (Codes and wording re-confirmed against the docs JSON on 2026-10-05.) `85F4.1`, `E174.1` and `DDA9.1` each state that the information "or any derived information, may not be sent off-device" (85F4.1 has a narrow exception: with explicit user permission, disk space may be sent over the *local network* to another device of the same person for display, never over the Internet). `C617.1` and `3B52.1` carry no such clause, but Lumen should not rely on that loophole. Together this constrains any future telemetry, cloud sync of scan results, or a phone-to-desktop dashboard: a LAN-only, user-permitted transfer of disk-space figures is the only explicitly allowed path.
  - Apple has rejected uploads that lack approved reasons for required-reason APIs since 1 May 2024, and this includes third-party SDKs ([Upcoming requirements](https://developer.apple.com/news/upcoming-requirements/)). The React Native, Expo and Rust dependencies also count, so audit the merged manifest of the final build. Do not rely only on Lumen's own declarations.
  - The Rust core must not call `statfs`/`getattrlist` directly in ways that bypass this declaration. The declaration covers the binary's use of the API category, not the Swift wrapper, so Rust's use of the same syscalls needs the same reasons declared.

### 2. Photos (PhotoKit)

**Authorization and access levels**
- `PHPhotoLibrary.requestAuthorization(for: .readWrite)` (iOS 14+, async variant available) ([docs](https://developer.apple.com/documentation/photos/phphotolibrary/requestauthorization(for:handler:))). Use `.readWrite`: `.addOnly` cannot read or delete.
- `.limited` (iOS 14+) means the user granted a subset of assets. Re-prompt with `PHPhotoLibrary.presentLimitedLibraryPicker(from:)`. Set `PHPhotoLibraryPreventAutomaticLimitedAccessAlert = YES` in Info.plist to suppress the automatic re-selection prompt ([docs](https://developer.apple.com/documentation/photos/phauthorizationstatus/limited)).
  - Under limited access, every fetch returns only the selected assets. Lumen's "library summary" must say **"of N photos you shared with Lumen"**, never present the result as the whole library.
- `PHPickerViewController` (iOS 14+) works **without library permission**. It is system-rendered and out of process, cannot be subclassed, and ignores touches when not fully opaque ([docs](https://developer.apple.com/documentation/photosui/phpickerviewcontroller)). It suits "check these specific photos". It cannot drive a library-wide analysis, and it returns item providers, not deletable `PHAsset`s, unless initialised with a `PHPhotoLibrary` (and then deletion still needs read/write authorization).

**Fetching**
- Use `PHAsset.fetchAssets(with: PHFetchOptions)`. By default it excludes iTunes-synced and iCloud Shared Album assets; `includeAssetSourceTypes` changes that ([docs](https://developer.apple.com/documentation/photos/phasset/fetchassets(with:))).
- Useful `PHFetchOptions` fields: `includeHiddenAssets`, `fetchLimit`, and **`prefetchAssetExtendedMetadata` (iOS 27+)**. The last one loads `PHAsset.extendedMetadata` (`PHAssetExtendedMetadata`: caption, keywords, originalFilename) in the same fetch ([docs](https://developer.apple.com/documentation/photos/phfetchoptions/prefetchassetextendedmetadata)).
- Smart albums Lumen can use as evidence sources include `smartAlbumScreenshots`, `smartAlbumScreenRecordings`, `smartAlbumBursts`, `smartAlbumVideos`, `smartAlbumLivePhotos`, `smartAlbumRAW`, `smartAlbumSlomoVideos`, `smartAlbumAllHidden` and `smartAlbumUnableToUpload`. **There is no Duplicates, Recently Deleted or large-videos subtype** ([PHAssetCollectionSubtype](https://developer.apple.com/documentation/photos/phassetcollectionsubtype)).
- **Incremental rescans:** `PHPhotoLibrary.fetchPersistentChanges(since: PHPersistentChangeToken)` (iOS 16+) returns inserts, updates and deletes since a stored token, or throws `persistentChangeTokenExpired` ([docs](https://developer.apple.com/documentation/photos/phphotolibrary/fetchpersistentchanges(since:))). Store the token in the Rust evidence store so re-analysis does not need a full scan.

**Resource sizes (the supported way)**
- `PHAssetResource.assetResources(for: PHAsset)` returns the resources behind an asset: original, edited, paired video, adjustment data and so on ([docs](https://developer.apple.com/documentation/photos/phassetresource)).
- **iOS 27.0+: `PHAssetResource.dataSize: Int?`** ("The size of the resource in bytes"; ObjC `NSNumber *dataSize`). iOS 27 also added `filename` and deprecated `originalFilename` (deprecated in 27.0); `uniformTypeIdentifier` is marked deprecated from 27.2, not 27.0. `contentType: UTType` is iOS 26.0+. `PHAssetResource.pixelWidth`/`pixelHeight` are available from iOS 16.0, which helps the size estimate on older OS versions. Availability was verified against the docs JSON, which marks it non-beta.
- **iOS ≤ 26:** there is no public size property. The common `resource.value(forKey: "fileSize")` reads a private property: undocumented, could break at any time, and an App Review risk. The supported alternative, `PHAssetResourceManager.requestData`/`writeData` ([docs](https://developer.apple.com/documentation/photos/phassetresourcemanager)), streams the bytes and may trigger iCloud downloads. That is far too expensive for a scan.
  - **Recommendation:** show exact bytes on iOS 27+ and on earlier OS versions show estimated sizes from `pixelWidth × pixelHeight × codec heuristics` and video `duration`, labelled "estimated". `dataSize` is optional (`nil` is possible, e.g. for cloud-only resources), so the UI must cope with unknown sizes. Do not ship the KVC hack.

**Deletion**
- `PHAssetChangeRequest.deleteAssets(_:)` runs inside `PHPhotoLibrary.shared().performChanges { … }` ([docs](https://developer.apple.com/documentation/photos/phassetchangerequest/deleteassets(_:))).
- Apple: "For each call to this method, iOS shows an alert asking the user for permission to edit the contents of the photo library." Batch all deletions into **one** change block ([docs](https://developer.apple.com/documentation/photos/phphotolibrary/performchanges(_:completionhandler:))).
- Deleted items move to **Recently Deleted** and can be recovered for 30 days ([Apple Support: Delete photos](https://support.apple.com/en-us/104967)). Since iOS 16, viewing Recently Deleted needs Face ID or Touch ID by default (same source), so the restore instructions must mention this.
- **iCloud Photos propagation (safety-critical):** with iCloud Photos on, a delete on this device deletes the item on all the user's other devices signed in to the same Apple Account ([Apple Support](https://support.apple.com/en-us/104967)). The delete syncs right away (the item leaves every device's library and sits in Recently Deleted); it is the *permanent* removal that waits for the 30-day window, not the cross-device propagation. Do not tell users the other devices are unaffected for 30 days. Lumen must find out or ask whether iCloud Photos is on and say "this removes it from all your devices and iCloud" in the confirmation step. *(Unverified: PhotoKit has no documented public flag for "iCloud Photos enabled"; inferring it from the cloud-only state of resources is a heuristic.)*
- **Optimize Storage skews reclaimable bytes:** with Optimize iPhone Storage (on by default), originals live in iCloud and the device keeps space-saving versions ([Apple Support: Manage photo storage](https://support.apple.com/en-us/105061); [Set up iCloud Photos](https://support.apple.com/en-us/108782)). `dataSize` describes the resource and is not known to equal the bytes on the device *(unverified)*. Lumen must not present the sum of `dataSize` as "space you will free on this iPhone"; it would breach 1.1.6. Also, space is freed only when Recently Deleted is purged. PhotoKit cannot read or restore Recently Deleted, so rollback means telling the user to restore from Photos › Recently Deleted. The Lumen ledger records `localIdentifier`s plus feature-print and size evidence for audit.
- A **non-destructive staging step is available**: `PHAssetCollectionChangeRequest` can create a "Lumen – Review" album and add candidates to it. This frees no space but is fully reversible and lets the user review in Photos.

**Duplicates**
- The system **Duplicates** album (Photos › Collections › Utilities, iOS 16+) merges exact and near duplicates, keeps the best quality and metadata, and moves the rest to Recently Deleted ([Apple Support: Merge duplicates](https://support.apple.com/guide/iphone/iph1978d9c23/ios)). PhotoKit does not expose it.
- **Near-duplicate detection in Lumen:**
  - Use Vision's Swift API `GenerateImageFeaturePrintRequest` → `FeaturePrintObservation` (iOS 18+; `data`, `elementCount`, `elementType`). Compare with `distance(to:) -> Double`, where shorter means more similar ([docs](https://developer.apple.com/documentation/vision/generateimagefeatureprintrequest); [FeaturePrintObservation](https://developer.apple.com/documentation/vision/featureprintobservation)). The legacy `VNGenerateImageFeaturePrintRequest` goes back to iOS 13.
  - Pipeline: request small thumbnails via `PHImageManager` (do not download originals from iCloud) → generate feature prints natively → hand the raw vectors to the **Rust core** for clustering (e.g. LSH/HNSW + thresholds + EXIF/time/burst evidence) → keep-best scoring (resolution, edits, favorites, `isFavorite`, recency) → KEEP/REVIEW proposals.
  - Feature-print revisions can change between OS versions, so store the `revision` with each vector and invalidate on change.

### 3. Files app, iCloud Drive, File Provider, security-scoped URLs

- **Picking a folder:** `UIDocumentPickerViewController(forOpeningContentTypes: [.folder])` (iOS 13+) returns a security-scoped URL that grants **recursive** access to the directory and future children ([Providing access to directories](https://developer.apple.com/documentation/uikit/providing-access-to-directories); [UIDocumentPickerViewController](https://developer.apple.com/documentation/uikit/uidocumentpickerviewcontroller)). This works for "On My iPhone" (which includes Downloads), iCloud Drive and third-party File Provider locations.
- **Access protocol:**
  - Call `startAccessingSecurityScopedResource()` and balance each call with `stopAccessingSecurityScopedResource()`. Leaked access leaks kernel resources until the app relaunches ([docs](https://developer.apple.com/documentation/foundation/url/startaccessingsecurityscopedresource())).
  - Use `NSFileCoordinator` for reads and writes.
  - Persist access with `bookmarkData(options: .minimalBookmark…)` and resolve with `URL(resolvingBookmarkData:bookmarkDataIsStale:)`, re-creating the bookmark when it is stale.
  - Do not store the raw URL.
- **iCloud placeholders:** read `ubiquitousItemDownloadingStatusKey` so scans do not trigger downloads and Lumen does not double-count cloud-only items ([docs](https://developer.apple.com/documentation/foundation/urlresourcekey/ubiquitousitemdownloadingstatuskey)).
  - **`FileManager.evictUbiquitousItem(at:)`** "removes only the local version". Deleting via `FileManager` removes the file from iCloud permanently and "can't be undone" ([docs](https://developer.apple.com/documentation/foundation/filemanager/evictubiquitousitem(at:))).
  - So for iCloud Drive, **eviction is Lumen's ideal reversible action**: it frees space, loses nothing and restores on demand.
  - Apple: "Don't use a coordinated write to perform this operation" (same doc). So the `NSFileCoordinator` rule above must exclude the evict call.
  - **Safety precondition (Lumen policy):** before evicting, check `ubiquitousItemIsUploadedKey == true` and that there are no unresolved conflicts (`ubiquitousItemHasUnresolvedConflictsKey`). Evict only items whose current version is already in iCloud. Apple does not document what eviction does to a not-yet-uploaded item *(unverified)*, so treat that case as unsafe. Apple's `ubiquitousItemIsUploadedKey` page warns not to poll the key from inside a coordinated-read block (the system cannot perform the coordinated read it needs until the block returns) and to use `NSMetadataQuery` or an `NSFilePresenter` for status changes ([docs](https://developer.apple.com/documentation/foundation/urlresourcekey/ubiquitousitemisuploadedkey)). Re-read the keys immediately before each evict (not from a stale scan snapshot) and log the values in the ledger.
- **Deletion in user folders:** `FileManager.trashItem(at:resultingItemURL:)` exists on iOS 11+ ([docs](https://developer.apple.com/documentation/foundation/filemanager/trashitem(at:resultingitemurl:))). Whether it lands in the Files app's "Recently Deleted" depends on the provider: **(unverified)**, and it must be tested per provider (local, iCloud Drive, third-party). For reference, Apple Support says that items a *user* deletes in Files from iCloud Drive or On My iPhone go to Files › Browse › Recently Deleted for 30 days, and that deleting from iCloud Drive on one device removes the files from every device signed in to the same Apple Account ([Apple Support: Delete or recover files in Files](https://support.apple.com/en-us/104953)). So a trash or delete of an iCloud Drive item is a cross-device action (same warning as iCloud Photos), and on-device space is only freed once the item leaves Recently Deleted.
  - Lumen's own quarantine (moving into the app container) frees nothing on the same volume and duplicates bytes when crossing providers. Prefer **evict → trash → hard delete** in that order, each with verification.
- **File Provider extensions** (`NSFileProviderReplicatedExtension`) are for apps that *provide* a storage domain. They give Lumen no visibility into other apps' data, so Lumen does not need one.

### 4. Background execution

- **`BGProcessingTaskRequest`** (iOS 13+): discretionary work that can run for minutes. Options are `requiresExternalPower` and `requiresNetworkConnectivity`. Register at launch and list identifiers in `BGTaskSchedulerPermittedIdentifiers` ([docs](https://developer.apple.com/documentation/backgroundtasks/bgprocessingtaskrequest); [WWDC25-227](https://developer.apple.com/videos/play/wwdc2025/227/)). Use it for overnight incremental index refresh, feature-print backfill and evidence-graph compaction.
- **`BGContinuedProcessingTask` / `BGContinuedProcessingTaskRequest`** (verified **iOS/iPadOS/Mac Catalyst 26.0+**, not beta) ([task](https://developer.apple.com/documentation/backgroundtasks/bgcontinuedprocessingtask); [request](https://developer.apple.com/documentation/backgroundtasks/bgcontinuedprocessingtaskrequest); [article](https://developer.apple.com/documentation/backgroundtasks/performing-long-running-tasks-on-ios-and-ipados)):
  - Submission "needs to occur as a result of a person's action, such as tapping a button". It starts immediately (or queues). `strategy = .fail` fails the request if it cannot start immediately.
  - It shows a **Live Activity** with `title`/`subtitle` (`updateTitle(_:subtitle:)`) and progress from `task.progress` (`ProgressReporting`). The user can cancel; on cancel the `expirationHandler` fires.
  - Apple's article names Vision and Core Image processing as example workloads. Background GPU needs `requiredResources = .gpu`, a check of `BGTaskScheduler.supportedResources.contains(.gpu)`, and the `com.apple.developer.background-tasks.continued-processing.gpu` entitlement.
  - Tasks with little progress are terminated first. Swiping the app away cancels without notifying the app, so all work must be **checkpointed and idempotent**.
  - WWDC guidance: do not use it for automatic maintenance, backups or sync.
  - Lumen's "Analyze photo library" button (feature prints for 50k assets) is the textbook use case. A community Expo wrapper exists ([aermes-ai/expo-continued-task](https://github.com/aermes-ai/expo-continued-task), unvetted). Lumen should write its own small Expo module.
- **App Review 2.4.2 / 2.5.4:** 2.4.2 says apps should not "rapidly drain battery, generate excessive heat, or put unnecessary strain on device resources", including "excessive write cycles to the solid state drive", and must not run unrelated background processes. 2.5.4 limits background services to their intended purposes ([guidelines](https://developer.apple.com/app-store/review/guidelines/)). The SSD-write clause matters for Lumen's index and evidence DB: batch writes and avoid rewriting the whole DB on incremental scans.

### 5. App Store Review Guidelines relevant to a "cleaner" (Last Updated: June 8, 2026)

| Guideline | Text (abridged) | Lumen implication |
|---|---|---|
| 2.3.1(a) | No hidden or undocumented features; "marketing your app in a misleading way, such as by promoting content or services that it does not actually offer (e.g. iOS-based virus and malware scanners)" can mean removal and **account termination** | No "virus", "junk", "boost", "speed up", "RAM cleaner" or "system cache" claims on iOS. Describe exactly: photos/videos, selected folders, iCloud downloads. |
| 1.1.6 | "False information and features, including inaccurate device data" | Capacity numbers must come from the documented keys. Never inflate "reclaimable" figures, and label estimates as estimates. |
| 2.4.4 | Never suggest a restart or unrelated settings changes | No "restart to finish cleaning". |
| 2.5.2 | Stay in your container; no downloaded code that changes features | Policy updates ship as data (declarative rules) inside app updates, or at most as signed data feeds. **(Unverified whether remote rule feeds draw scrutiny; the safe default is bundling.)** |
| 2.5.9 | Do not alter standard UI behaviour | — |
| 3.1.2(a) | Subscriptions must give ongoing value | If subscription-based, justify with continuous monitoring/analysis, not one-off "cleans". |
| 5.1.1 / 5.1.2 | Privacy policy, permission justification, no use without consent | Matches Lumen's local-first stance. Fill in the privacy manifest. |
| 5.6 | No preying on users or tricking them into purchases | No fake "danger" dashboards or scare scans before the paywall. |

Fake system-style alerts would violate 1.1.6/2.3.1/5.6. Apple's guidelines do not name "cleaner apps" explicitly; the enforcement pattern is **(unverified, based on press and community reports)**.

### 6. Apple Foundation Models framework

- **Availability:**
  - iOS/iPadOS/macOS/visionOS/Mac Catalyst **26.0+**, watchOS 27.0+ ([framework](https://developer.apple.com/documentation/foundationmodels)).
  - Requires an Apple Intelligence–capable device and a supported region. `SystemLanguageModel.default.availability` returns `.available` or `.unavailable(.deviceNotEligible | .appleIntelligenceNotEnabled | .modelNotReady | …)` ([SystemLanguageModel](https://developer.apple.com/documentation/foundationmodels/systemlanguagemodel)).
  - Apple ships updated models with OS updates; there are three versions so far (26.0–26.3, 26.4, 27.0). WWDC26 confirms a model "rebuilt from the ground up" for the 27 releases, and secondary sources describe a rebuilt model in 26.4 *(26.4 boundary unverified against a primary source)*. Third-party reports also mention a second, larger on-device model for higher-end devices in 27 *(unverified)*. Plan for prompt regressions and re-evaluate on each OS release.
- **Limits:**
  - The on-device context window is **4,096 tokens per session**, covering instructions, prompts, tool schemas, `@Generable` schemas and output ([Managing the context window](https://developer.apple.com/documentation/foundationmodels/managing-the-context-window)).
  - **Correction (verification):** the 4,096 figure is the iOS 26 model's. WWDC26 "What's new in the Foundation Models framework" states the rebuilt on-device model's `model.contextSize` returns 8192 tokens ([WWDC26-241](https://developer.apple.com/videos/play/wwdc2026/241/)); the docs article still says 4096 as of 2026-10-05. Budget from `contextSize`/`tokenCount(for:)` at runtime and keep the 4K budget as the floor for iOS 26 devices.
  - `contextSize` and `tokenCount(for:)` let you budget; `contextSize` is declared `@backDeployed(before: iOS 26.4 …)`, i.e. it shipped in 26.4 and is usable back to 26.0 (the docs list it as 26.0+).
  - Exceeding the window throws `LanguageModelError.contextSizeExceeded`.
- **Guided generation:** `@Generable` (structs, enums, actors) and `@Guide` (stored properties; `.minimum/.maximum`, `.minimumCount/.maximumCount`, descriptions) use **constrained sampling**, so the output is always a valid instance of the Swift type ([guide](https://developer.apple.com/documentation/foundationmodels/generating-swift-data-structures-with-guided-generation)). The `Tool` protocol enables tool calling. `UseCase.contentTagging` is a specialised adapter.
- **New in iOS 27:**
  - `PrivateCloudComputeLanguageModel` is a drop-in `LanguageModel` with a 32K context, `ContextOptions(reasoningLevel: .light/.moderate/.deep)` and a daily quota (`quotaUsage`, `quotaLimitReached`). When users near the quota, Apple's guidance is to show system UI that lets them subscribe to iCloud+ for more access. That is an Apple upsell inside Lumen's UI, which is a product consideration. It requires a **managed entitlement** (`com.apple.developer.private-cloud-compute`; eligibility requirements apply) and a network connection; Apple's article says to fall back to the on-device model when the network is unavailable. Per WWDC26 the PCC model is free for developers below a download threshold (stated as fewer than 2 million first-time downloads; *(unverified against written terms)*) and Apple says no prompts are stored ([WWDC26-241](https://developer.apple.com/videos/play/wwdc2026/241/); [PCC model](https://developer.apple.com/documentation/foundationmodels/privatecloudcomputelanguagemodel); [article](https://developer.apple.com/documentation/foundationmodels/adding-server-side-intelligence-with-private-cloud-compute)).
  - Multimodal `Attachment(image)` in prompts ([Attachment](https://developer.apple.com/documentation/foundationmodels/attachment)).
  - Custom `LanguageModel` providers.
- **Suitability for Jev:**
  - Good for small, **structured, metadata-only judgments**, for example:

    ```swift
    @Generable struct JevOpinion {
      @Guide(description: "keep, review or remove") let leaning: Leaning
      @Guide(.minimum(0), .maximum(1)) let confidence: Double
      let rationale: String
    }
    ```

    The input would be a compact evidence summary such as "Screenshot, 14 months old, OCR text looks like a boarding pass, not favorited, in no album".
  - The 4K (iOS 26) to 8K (iOS 27) window means one item or a small batch per session. The output must feed the deterministic policy as **evidence with a confidence value, never a verdict**, which matches Lumen's design.
  - Gate on availability and fall back to rules-only.
  - PCC must be **opt-in only** and labelled "processed by Apple Private Cloud Compute". It is privacy-preserving, but it is not local, and Lumen's default is that no content leaves the device.
  - Image `Attachment` (27+) lets Jev look at a thumbnail locally. Keep this on-device only.

### 7. React Native: current state

- **Versions** ([Releases overview](https://reactnative.dev/docs/releases)):
  - 0.87.x, released 2026-08-10, Active (latest stable).
  - 0.86.x, Active.
  - 0.85.x, End of Cycle.
  - 0.88 is due **2026-10-12** and 0.89 on 2026-12-07.
  - Only the latest three minors are supported, so expect to upgrade roughly every two months.
- **Architecture:**
  - Since **0.82**, the New Architecture (Fabric renderer, TurboModules, JSI, bridgeless runtime) is the only one. `newArchEnabled=false` and `RCT_NEW_ARCH_ENABLED=0` are ignored, but the interop layers stay "for the foreseeable future" ([0.82 blog](https://reactnative.dev/blog/2025/10/08/react-native-0.82)).
  - 0.85 continued removing legacy classes (e.g. `CatalystInstanceImpl` removed) ([0.85](https://reactnative.dev/blog/2026/04/07/react-native-0.85)).
  - 0.87 removed the `useTurboModules` flag ("TurboModules always enabled") and made the Strict TypeScript API the default (deep imports become type errors; the opt-out works through 0.88 only). It also added experimental, opt-in SwiftPM support *alongside* CocoaPods ("It is opt-in and additive; CocoaPods remains the default and the supported path"; a "do not use it in production yet" line was not found verbatim during verification), first AGP 9 support (with a recommended opt-out of the new Kotlin/DSL behaviour), and Node ≥ 22.13 ([0.87](https://reactnative.dev/blog/2026/08/11/react-native-0.87)).
  - Hermes V1 was experimental/opt-in in 0.82 and has been **the default engine since 0.84** (2026-02-11) ([0.84 blog](https://reactnative.dev/blog/2026/02/11/react-native-0.84)).
- **C++ path:** "Cross-Platform Native Modules (C++)", i.e. pure C++ TurboModules with Codegen specs and `…CxxSpec` classes, registered via `OnLoad.cpp` on Android and a module provider on iOS ([docs](https://reactnative.dev/docs/the-new-architecture/pure-cxx-modules)). In principle this could call a Rust C ABI directly.

### 8. Expo: current state

- **SDK timeline** ([changelog](https://expo.dev/changelog)):

  | SDK | Release | React Native | Notes |
  |---|---|---|---|
  | 55 | 2026-02-25 | 0.83 | **Legacy Architecture dropped** ("SDK 54 is the final release to include Legacy Architecture support"); `newArchEnabled` removed; min iOS 15.1; `expo-brownfield` ([sdk-55](https://expo.dev/changelog/sdk-55)) |
  | 56 | 2026-05-21 | 0.85 | **Min iOS 16.4**, Xcode 26.4. Expo UI (SwiftUI/Jetpack Compose) stable. Expo Router forked from React Navigation. New object-oriented `expo-media-library`. `expo-file-system` tasks with AbortSignal ([sdk-56](https://expo.dev/changelog/sdk-56)) |
  | **57 (current stable)** | 2026-06-30 | 0.86 | No breaking RN changes; `expo-image` cache management. Notes that iOS 27 SDK builds must use the UIKit scene lifecycle ([sdk-57](https://expo.dev/changelog/sdk-57)) |
  | 58 (beta) | beta 2026-09-15 | 0.88 RC | **Expo Modules 2.0** (annotated Swift/Kotlin classes, build-time generation; sync calls 2.5–5.6× faster on iOS). Prebuilt `expo-modules-core` on Android. iOS 27 scene lifecycle. `expo-app-intents` (alpha). R8 on by default. EAS default Xcode 26.6, with Xcode 27 images "coming soon" ([sdk-58-beta](https://expo.dev/changelog/sdk-58-beta)) |

  Per the SDK 57 notes, Expo is *exploring* (not committed to) optional non-breaking releases between major SDKs. SDK 57 also offers opt-in UIKit scene support via `expo-build-properties` `ios.enableSceneSupport`; Expo states "Apps built with the iOS 27 SDK must use the UIKit scene-based life cycle, or they do not launch correctly on iOS 27" ([sdk-57](https://expo.dev/changelog/sdk-57)). Expo Go now requires login (2026-09-03).
- **Expo Modules API:**
  - Swift and Kotlin modules and native views over JSI, supporting the New Architecture. Expo recommends TurboModules instead "if you intend to use C++" ([overview](https://docs.expo.dev/modules/overview/)).
  - Modules 2.0 in SDK 58 swaps the DSL for annotations on plain classes.
- **Config plugins / CNG:**
  - JS functions in `app.json` `plugins` that edit native projects at `npx expo prebuild` through mods such as `withInfoPlist`, `withEntitlementsPlist` and `withAndroidManifest` ([docs](https://docs.expo.dev/config-plugins/introduction/)).
  - Lumen needs them for:
    - the `NSPhotoLibraryUsageDescription` key;
    - `PHPhotoLibraryPreventAutomaticLimitedAccessAlert`;
    - `BGTaskSchedulerPermittedIdentifiers` and background modes;
    - the GPU background entitlement;
    - `PrivacyInfo.xcprivacy` reasons;
    - linking the Rust XCFramework and `.so` files.
- **Development builds vs Expo Go:** a development build is "your own version of Expo Go" in which you can use any native library or configuration ([docs](https://docs.expo.dev/develop/development-builds/introduction/)). Lumen's custom Swift, Kotlin and Rust code means **Expo Go is not an option**; Lumen uses dev builds from day one.
- **EAS Build:** hosted builds for any native project, with `eas build --local` for local or CI builds and managed keystores and provisioning profiles ([docs](https://docs.expo.dev/build/introduction/)). Lumen can use EAS or keep its own CI (local builds) if the Rust toolchain makes cloud images awkward.
- **Expo Router:** file-based routing. Since SDK 55, "all Expo SDK packages use the same major version as the SDK" (e.g. `expo-router@^55`), so pin Router by SDK major, not by a separate "v7" number ([sdk-55](https://expo.dev/changelog/sdk-55)); the "Expo Router v7" label previously cited from an X post is (unverified). SDK 56 forked away from React Navigation ("no longer depends on `react-navigation`"; `@react-navigation/*` code mostly stops working; a codemod exists) ([sdk-56](https://expo.dev/changelog/sdk-56)). That SDK 58 stabilises data loaders and native tabs is (unverified).

### 9. Rust ↔ mobile bridging options

- **UniFFI (mozilla/uniffi-rs) 0.32.2** (crates.io 2026-09-23; 0.32.1 on 2026-09-09, 0.32.0 on 2026-06-30) ([CHANGELOG](https://github.com/mozilla/uniffi-rs/blob/main/CHANGELOG.md)):
  - Official Kotlin, Swift, Python and Ruby bindings. Used "extensively by Mozilla in Firefox mobile and desktop". "ready for production use, but … a long way from a 1.0" ([README](https://github.com/mozilla/uniffi-rs)).
  - 0.32 added zero-copy `&[u8]` arguments, recursive enums and `Box<T>` support. An experimental JNI-based Kotlin bindgen is in the unreleased section.
  - **Breaking in 0.32.0 (affects the Kotlin adapter):** `[ByRef] bytes` / `&[u8]` arguments now require Kotlin call sites to pass a *direct* `java.nio.ByteBuffer` instead of `ByteArray` (migrate with `ByteBuffer.allocateDirect(n).put(arr).flip()`); Swift `Data` call sites are unchanged ([CHANGELOG](https://github.com/mozilla/uniffi-rs/blob/main/CHANGELOG.md)). Note: 0.32.2 is published on crates.io and tagged, but the `main` CHANGELOG lists only up to 0.32.1 (dated 2026-09-08), so check the 0.32.2 tag for its notes.
  - Third-party bindings: React Native/WASM (ubrn), **Kotlin Multiplatform (Gobley)**, Go, C#, Dart, Java and Node.
  - Tooling: `cargo swift` builds an SPM package and the Cargo NDK Gradle plugin builds the Android side.
- **uniffi-bindgen-react-native (ubrn) 0.31.0-6** (2026-09-25) ([releases](https://github.com/jhugman/uniffi-bindgen-react-native/releases); [site](https://jhugman.github.io/uniffi-bindgen-react-native/)):
  - Generates TypeScript + C++/JSI TurboModules for Hermes, plus WASM and Node (N-API) flavours.
  - The latest release adds a "JSI player" runtime (`@ubjs/react-native`) so libraries can ship without per-library C++.
  - It tracks uniffi 0.31; an upgrade to 0.32 is open ([issue #449](https://github.com/jhugman/uniffi-bindgen-react-native/issues/449)).
  - **Its docs still say: "still in early development, and should not yet be used in production."**
- **Native-wrapper approach (recommended):** Rust → UniFFI Swift/Kotlin bindings → Expo Module (Swift/Kotlin) → JS. This adds one hop, but:
  - it uses the most mature bindgen paths;
  - the native layer needs Swift and Kotlin anyway for PhotoKit, Vision, BGTask, MediaStore and StorageStatsManager;
  - the same Swift/Kotlin bindings work unchanged if Lumen later goes fully native.
- **Hand-written C ABI + pure C++ TurboModule** is possible but means owning a hand-maintained FFI. Rejected.

### 10. Alternatives evaluated

| Option | Current state (verified) | Fit for Lumen |
|---|---|---|
| **Expo/RN + Swift/Kotlin Expo Modules + UniFFI** | RN 0.87; Expo SDK 57 (58 beta); Expo UI SwiftUI/Compose stable | Shares TS/React skills, design tokens and domain types with the web dashboard (and with a Tauri desktop UI if Lumen uses one). Heavy work stays native or in Rust. Costs: a fast upgrade treadmill (support window of three minors) and a JS↔native boundary that must stay coarse-grained. |
| **Native SwiftUI + Jetpack Compose, Rust via UniFFI** | UniFFI 0.32.2 | Best platform fidelity, the most direct PhotoKit and Vision integration, and no JS runtime. Costs: two UI codebases, no reuse with the web dashboard, and two native skill sets. **Strong fallback.** |
| **Kotlin Multiplatform / Compose Multiplatform** | Kotlin 2.4.0 (2026-06-03); CMP 1.11.0 (May 2026); **Swift export still Alpha** ([docs](https://kotlinlang.org/docs/native-swift-export.html)) | KMP's main value is shared business logic, which Lumen already has in Rust. That leaves three languages (Rust + Kotlin + Swift), Obj-C-header interop until Swift export matures, and Rust→KMP via Gobley (third-party). Rejected. |
| **Flutter** | 3.47 latest stable ([release notes](https://docs.flutter.dev/release/release-notes)); 3.44 made SwiftPM the iOS default and Impeller the Android default | Good rendering and a mature Rust bridge (flutter_rust_bridge; not evaluated here). But Dart cannot share anything with the TS web dashboard, platform channels are still needed for PhotoKit and Vision, and it adds a third UI stack. Rejected. |
| **Tauri 2 mobile** | Supports iOS and Android ([prereqs](https://v2.tauri.app/start/prerequisites/)) | Tempting if desktop is Tauri (same Rust core, same web UI). But a WebView UI for photo-grid review of thousands of thumbnails is weak, and PhotoKit, Vision and BGTask still need Swift plugins. Revisit only for an Android/iOS "companion" scope. Not recommended. |
| **RN + ubrn (Rust directly in JS)** | 0.31.0-6, self-described pre-production | Rejected for now; re-evaluate when it declares production readiness and tracks uniffi 0.32+. |

**Android note (for adapter design):** `StorageStatsManager` (API 26+) needs no permission for the app's own package. Querying other packages requires `PACKAGE_USAGE_STATS`, "a system-level permission that will not be granted to normal apps", which the user can grant in Settings ([docs](https://developer.android.com/reference/android/app/usage/StorageStatsManager)). Android therefore exposes far more of the system than iOS. The Android adapter surface (MediaStore, SAF, StorageStatsManager, cache-clear intents) is larger than the iOS one, which favours a design where platform capabilities are declared per adapter rather than assumed.

**Android omission added during verification (affects the Android adapter and Play listing):** the "cache-clear intent", `StorageManager.ACTION_CLEAR_APP_CACHE` (API 30+), "doesn't automatically clear cache, but shows a dialog and lets the user decide", clears *external* app cache directories, and **requires `MANAGE_EXTERNAL_STORAGE`** ([StorageManager](https://developer.android.com/reference/android/os/storage/StorageManager#ACTION_CLEAR_APP_CACHE)). Google Play restricts `MANAGE_EXTERNAL_STORAGE` ("All files access") to declared core-purpose categories (file managers whose core purpose includes management "including maintenance" of files outside app-specific storage, backup/restore, anti-virus, document management); "storage cleaner" is not a listed category, and the permission needs a Play Console declaration and review ([Play policy: All files access](https://support.google.com/googleplay/android-developer/answer/10467955)). So Lumen-for-Android must either qualify and declare as a file manager or ship without All files access (and therefore without the system cache-clear intent), using MediaStore/SAF only. Treat this as a capability flag in the Android adapter, not an assumption. `ACTION_MANAGE_STORAGE` (API 25+) opens the system storage-management screen; that it needs no special permission is (unverified).

## Implications for Lumen

1. **Make the iOS product honest by design.** Ship iOS as "Lumen for Photos & Files" with four capability-scoped adapters:
   - `IosVolumeCapacityAdapter`: the important-usage key for display;
   - `PhotoKitAdapter`: fetch, persistent-change tokens, sizes, smart-album evidence and batched delete;
   - `UserFolderAdapter`: bookmarks, coordinated enumeration, iCloud status and evict/trash;
   - `OwnContainerAdapter`: Lumen's own caches.

   The Rust core must expose a **capability model** (`supports_system_caches = false` on iOS) so that policy, UI and marketing never offer actions that do not exist.
2. **Define quarantine semantics per platform.**
   - **iOS Photos:** quarantine = system Recently Deleted (30 days, restore in Photos). An optional pre-stage adds items to a "Lumen – Review" album.
   - **iCloud Drive:** evict first. It is reversible, verifiable via `ubiquitousItemDownloadingStatusKey`, and frees space.
   - **Local Files:** `trashItem` where supported, otherwise REVIEW-only with explicit hard-delete confirmation.

   In every case the ledger records the before/after evidence, and rollback is either automated (evict → re-download) or user-guided (Recently Deleted).
3. **Batch deletes into one `performChanges` per user decision.** That gives one system prompt and one ledger transaction. The system alert is a feature: it is a second confirmation outside Lumen's control, which suits "safety over space".
4. **Make size accuracy explicit.** Use `dataSize` on iOS 27+ and estimated sizes (labelled) on iOS 26 and earlier. No private KVC. Keep minimum iOS at 16.4 to match Expo SDK 56+, and light up `dataSize`, `BGContinuedProcessingTask` and Foundation Models behind `#available`.
5. **Near-duplicates:** compute Vision feature prints natively (thumbnail-sized, no iCloud original downloads) and run clustering and keep-best scoring in Rust, so the same logic serves desktop photo folders. Also offer a deep-link explanation to Photos › Utilities › Duplicates rather than duplicating Apple's merge.
6. **Background work:**
   - "Analyze library" is a `BGContinuedProcessingTask` (user-initiated, Live Activity, checkpointed every N assets, `.queue` strategy, GPU when supported).
   - Nightly incremental work is a `BGProcessingTask` with `requiresExternalPower = true`.
   - Nothing destructive runs in the background, ever.
7. **Jev on Apple platforms:**
   - Add a `FoundationModelsJudge` adapter behind the Jev port: on-device only, `@Generable` output, one item per session, availability-gated, with a timeout. Its output goes into the evidence graph with model-version provenance (the OS-bundled model changes with OS updates).
   - PCC is a separate opt-in adapter, disabled by default and clearly disclosed.
   - Do not let any LLM output bypass the deterministic policy.
8. **Mobile stack (recommended): Expo with development builds and CNG.**
   - **UI:** React Native with Expo Router, using Expo UI's SwiftUI/Compose components where native feel matters (photo review grids should use native views or `expo-image`).
   - **Platform adapters:** local Expo Modules in Swift and Kotlin (`modules/lumen-photos`, `modules/lumen-files`, `modules/lumen-bgtask`, `modules/lumen-android-storage`). Adopt Modules 2.0 annotations once SDK 58 is stable.
   - **Core:** the Rust crate compiled to `LumenCore.xcframework` and Android `.so`s, with UniFFI Swift and Kotlin bindings consumed *inside* the Expo modules. A config plugin wires the build phases.
   - **Boundary rule:** the JS side gets paged, read-only view-models, progress events and command intents (`proposeQuarantine(ids)`), never raw file paths in bulk and never per-file calls in hot loops. Scanning, hashing, feature prints and graph writes stay native/Rust.
   - **Rationale:**
     - It reuses React/TS skills and the domain types and UI tokens shared with the web dashboard.
     - Heavy native APIs need Swift and Kotlin regardless, and UniFFI Swift/Kotlin are the most mature Rust bindings.
     - It keeps a clean escape hatch: if RN becomes a liability, the Swift/Kotlin adapters and the Rust bindings carry over to a fully native SwiftUI/Compose app unchanged.
   - **Rejected:**
     - ubrn direct JS↔Rust: pre-production by its own docs.
     - KMP/CMP: duplicates Rust's role, and Swift export is Alpha.
     - Flutter: no TS reuse, still needs channels.
     - Tauri mobile: WebView UI for big media grids.
     - Expo Go: cannot load custom native code.
   - **Decision gate:** before committing, build a spike that enumerates 50k PHAssets, computes 5k feature prints in a `BGContinuedProcessingTask`, clusters them in Rust and renders a review grid. Success means smooth scrolling at 60 fps or better on a mid-range iPhone and on Android, and JS-thread work under 4 ms per frame. If it fails, fall back to native SwiftUI/Compose on the same Rust and adapter layers.
9. **Upgrade cadence:** pin Expo SDK 57 now (enable `ios.enableSceneSupport` if building with the iOS 27 SDK), move to SDK 58 when it ships (it makes the scene lifecycle the default, which is mandatory for iOS 27 SDK builds; Apple typically starts requiring the newest SDK for App Store uploads the following spring *(unverified for 2027)*), and budget for a planned upgrade every quarter. RN supports only three minors and drops each after about six months.
10. **App Store compliance checklist:**
    - privacy manifest with Required Reason APIs (disk space, file timestamp);
    - accurate metadata (no "virus", "boost" or "system cleaner");
    - exact or labelled-estimate numbers only;
    - no scare UI;
    - review notes that explain the PhotoKit delete flow and the evict action;
    - no downloadable executable policy code.

## Risks and open questions

- **iOS 26 size accuracy:** most users may still be on iOS 26 for a while. Estimated sizes could misrank "biggest items". Should Lumen use `PHAssetResourceManager` byte-streaming for top-N candidates only, when the user opts in (accurate, but it may download from iCloud)?
- **Files "Recently Deleted" via `trashItem` on iOS** across providers (local, iCloud Drive, third-party) is unverified and needs device tests before Lumen can call it reversible.
- **Required Reason codes** (disk space / file timestamp) were re-confirmed against Apple's NSPrivacyAccessedAPITypeReasons docs JSON during verification. Still open: the Rust core's direct syscalls (`stat`/`statfs`/`getattrlist` family via `std::fs`/`libc`) must also be covered by the declaration; Apple's list of exact covered C functions should be checked against what the Rust binary links.
- **App Review interpretation:** cleaner-category scrutiny is real but undocumented beyond the clauses above. Prepare detailed review notes and an honest subscription value proposition.
- **Foundation Models drift:** the model changes with OS releases (three versions to date). Jev's evidence calibration needs a per-model-version evaluation set, and device or region availability fragments the feature.
- **PCC entitlement** is managed and eligibility-gated, and it contradicts "local-first" unless strictly opt-in. Product decision needed: offer it at all?
- **Expo SDK 58 timing:** it depends on RN 0.88 (2026-10-12). Modules 2.0 annotations are new, so the risk of churn in early adoption is unknown.
- **Expo + Rust build integration:** the XCFramework, Android NDK ABIs, 16 KB page alignment for Android 15+ and EAS cloud images with a Rust toolchain all need a config plugin and CI proof. `eas build --local` or self-hosted CI may be simpler.
- **ubrn trajectory:** if it reaches production status with uniffi 0.32+, Lumen could skip the Swift/Kotlin hop for pure-compute calls such as policy evaluation. Re-evaluate in 2027.
- **Android All files access:** the Android system cache-clear intent requires `MANAGE_EXTERNAL_STORAGE`, which Google Play only grants to declared core-purpose categories (cleaners are not listed). Decide whether Lumen-for-Android declares as a file manager or ships MediaStore/SAF-only. *(Added during verification.)*
- **Limited Photos access UX:** Lumen must show clearly that results are partial. Some users will never grant full access, which caps what Lumen can do for them.
- **Vision feature-print thresholds** are not documented by Apple (distance is relative). Lumen needs its own labelled near-duplicate dataset to tune thresholds per revision.
- **(Unverified)** that there is no public API or deep link for Settings › iPhone Storage. Only `UIApplication.openSettingsURLString` (the app's own settings) is documented.

## Sources

- [Releases Overview · React Native](https://reactnative.dev/docs/releases)
- [React Native 0.82 – A New Era (New Architecture only)](https://reactnative.dev/blog/2025/10/08/react-native-0.82)
- [React Native 0.85 blog](https://reactnative.dev/blog/2026/04/07/react-native-0.85)
- [React Native 0.86 blog](https://reactnative.dev/blog/2026/06/11/react-native-0.86) (search result)
- [React Native 0.87 blog](https://reactnative.dev/blog/2026/08/11/react-native-0.87)
- [Cross-Platform Native Modules (C++) · React Native](https://reactnative.dev/docs/the-new-architecture/pure-cxx-modules)
- [Expo Changelog](https://expo.dev/changelog)
- [Expo SDK 55 changelog](https://expo.dev/changelog/sdk-55)
- [Expo SDK 56 changelog](https://expo.dev/changelog/sdk-56)
- [Expo SDK 57 changelog](https://expo.dev/changelog/sdk-57)
- [Expo SDK 58 Beta changelog](https://expo.dev/changelog/sdk-58-beta)
- [Expo Modules API overview](https://docs.expo.dev/modules/overview/)
- [Expo development builds introduction](https://docs.expo.dev/develop/development-builds/introduction/)
- [Expo config plugins introduction](https://docs.expo.dev/config-plugins/introduction/)
- [EAS Build introduction](https://docs.expo.dev/build/introduction/)
- [Expo on X – SDK 55 announcement (Expo Router v7)](https://x.com/expo/status/2026811977990025364) (search result; unverified, could not be fetched, and its "v7" label conflicts with Expo's SDK-major package versioning)
- [uniffi-bindgen-react-native releases](https://github.com/jhugman/uniffi-bindgen-react-native/releases)
- [uniffi-bindgen-react-native documentation](https://jhugman.github.io/uniffi-bindgen-react-native/)
- [ubrn issue #449 – Upgrade to uniffi-rs 0.32](https://github.com/jhugman/uniffi-bindgen-react-native/issues/449) (search result)
- [mozilla/uniffi-rs README](https://github.com/mozilla/uniffi-rs)
- [mozilla/uniffi-rs CHANGELOG](https://github.com/mozilla/uniffi-rs/blob/main/CHANGELOG.md)
- [The UniFFI user guide](https://mozilla.github.io/uniffi-rs/latest/)
- [Kotlin – Swift export (Alpha)](https://kotlinlang.org/docs/native-swift-export.html)
- [Kotlin 2.4.0 Released – JetBrains Blog](https://blog.jetbrains.com/kotlin/2026/06/kotlin-2-4-0-released/) (search result)
- [Compose Multiplatform 1.11.0 – JetBrains Blog](https://blog.jetbrains.com/kotlin/2026/05/compose-multiplatform-1-11-0/) (search result)
- [Flutter release notes](https://docs.flutter.dev/release/release-notes)
- [What's new in Flutter 3.44](https://flutter.dev/blog/whats-new-in-flutter-3-44) (search result)
- [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/)
- [App Review Guidelines (Last Updated June 8, 2026)](https://developer.apple.com/app-store/review/guidelines/)
- [File System Programming Guide – File System Basics](https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/FileSystemProgrammingGuide/FileSystemOverview/FileSystemOverview.html)
- [Checking volume storage capacity](https://developer.apple.com/documentation/foundation/checking-volume-storage-capacity)
- [volumeAvailableCapacityForImportantUsageKey](https://developer.apple.com/documentation/foundation/urlresourcekey/volumeavailablecapacityforimportantusagekey)
- [totalFileAllocatedSizeKey](https://developer.apple.com/documentation/foundation/urlresourcekey/totalfileallocatedsizekey)
- [isExcludedFromBackupKey](https://developer.apple.com/documentation/foundation/urlresourcekey/isexcludedfrombackupkey)
- [Describing use of required reason API](https://developer.apple.com/documentation/bundleresources/describing-use-of-required-reason-api)
- [PHAssetResource](https://developer.apple.com/documentation/photos/phassetresource) (including `dataSize`, `filename`, `contentType` symbol pages via the docs JSON)
- [PHAssetResourceManager](https://developer.apple.com/documentation/photos/phassetresourcemanager)
- [PHAssetChangeRequest.deleteAssets(_:)](https://developer.apple.com/documentation/photos/phassetchangerequest/deleteassets(_:))
- [PHPhotoLibrary.performChanges(_:completionHandler:)](https://developer.apple.com/documentation/photos/phphotolibrary/performchanges(_:completionhandler:))
- [PHPhotoLibrary.requestAuthorization(for:handler:)](https://developer.apple.com/documentation/photos/phphotolibrary/requestauthorization(for:handler:))
- [PHPhotoLibrary.fetchPersistentChanges(since:)](https://developer.apple.com/documentation/photos/phphotolibrary/fetchpersistentchanges(since:))
- [PHAuthorizationStatus.limited](https://developer.apple.com/documentation/photos/phauthorizationstatus/limited)
- [PHAssetCollectionSubtype](https://developer.apple.com/documentation/photos/phassetcollectionsubtype)
- [PHAsset.fetchAssets(with:)](https://developer.apple.com/documentation/photos/phasset/fetchassets(with:))
- [PHFetchOptions.prefetchAssetExtendedMetadata](https://developer.apple.com/documentation/photos/phfetchoptions/prefetchassetextendedmetadata)
- [PHAssetExtendedMetadata](https://developer.apple.com/documentation/photos/phassetextendedmetadata)
- [PHPickerViewController](https://developer.apple.com/documentation/photosui/phpickerviewcontroller)
- [Vision GenerateImageFeaturePrintRequest](https://developer.apple.com/documentation/vision/generateimagefeatureprintrequest)
- [Vision FeaturePrintObservation](https://developer.apple.com/documentation/vision/featureprintobservation)
- [VNGenerateImageFeaturePrintRequest](https://developer.apple.com/documentation/vision/vngenerateimagefeatureprintrequest)
- [UIDocumentPickerViewController](https://developer.apple.com/documentation/uikit/uidocumentpickerviewcontroller)
- [Providing access to directories](https://developer.apple.com/documentation/uikit/providing-access-to-directories)
- [URL.startAccessingSecurityScopedResource()](https://developer.apple.com/documentation/foundation/url/startaccessingsecurityscopedresource())
- [FileManager.evictUbiquitousItem(at:)](https://developer.apple.com/documentation/foundation/filemanager/evictubiquitousitem(at:))
- [ubiquitousItemDownloadingStatusKey](https://developer.apple.com/documentation/foundation/urlresourcekey/ubiquitousitemdownloadingstatuskey)
- [FileManager.trashItem(at:resultingItemURL:)](https://developer.apple.com/documentation/foundation/filemanager/trashitem(at:resultingitemurl:))
- [BGContinuedProcessingTask](https://developer.apple.com/documentation/backgroundtasks/bgcontinuedprocessingtask)
- [BGContinuedProcessingTaskRequest](https://developer.apple.com/documentation/backgroundtasks/bgcontinuedprocessingtaskrequest)
- [Performing long-running tasks on iOS and iPadOS](https://developer.apple.com/documentation/backgroundtasks/performing-long-running-tasks-on-ios-and-ipados)
- [BGProcessingTaskRequest](https://developer.apple.com/documentation/backgroundtasks/bgprocessingtaskrequest)
- [WWDC25 – Finish tasks in the background](https://developer.apple.com/videos/play/wwdc2025/227/)
- [Foundation Models framework](https://developer.apple.com/documentation/foundationmodels)
- [SystemLanguageModel](https://developer.apple.com/documentation/foundationmodels/systemlanguagemodel)
- [SystemLanguageModel.contextSize](https://developer.apple.com/documentation/foundationmodels/systemlanguagemodel/contextsize)
- [Managing the context window](https://developer.apple.com/documentation/foundationmodels/managing-the-context-window)
- [Generating Swift data structures with guided generation](https://developer.apple.com/documentation/foundationmodels/generating-swift-data-structures-with-guided-generation)
- [PrivateCloudComputeLanguageModel](https://developer.apple.com/documentation/foundationmodels/privatecloudcomputelanguagemodel)
- [Adding server-side intelligence with Private Cloud Compute](https://developer.apple.com/documentation/foundationmodels/adding-server-side-intelligence-with-private-cloud-compute)
- [Foundation Models Attachment](https://developer.apple.com/documentation/foundationmodels/attachment)
- [Apple Support – Delete photos on your iPhone or iPad](https://support.apple.com/en-us/104967) (fetched during verification)
- [Apple Support – Manage your photo and video storage](https://support.apple.com/en-us/105061)
- [Apple Support – Delete files or recover deleted files in the Files app](https://support.apple.com/en-us/104953)
- [Apple Support – Merge duplicate photos and videos on iPhone](https://support.apple.com/guide/iphone/iph1978d9c23/ios) (page exists; body text not extracted during verification)
- [Android StorageStatsManager reference](https://developer.android.com/reference/android/app/usage/StorageStatsManager)
- [Android StorageManager reference (ACTION_CLEAR_APP_CACHE)](https://developer.android.com/reference/android/os/storage/StorageManager#ACTION_CLEAR_APP_CACHE)
- [Google Play policy – Use of All files access (MANAGE_EXTERNAL_STORAGE)](https://support.google.com/googleplay/android-developer/answer/10467955)
- [NSPrivacyAccessedAPITypeReasons](https://developer.apple.com/documentation/bundleresources/app-privacy-configuration/nsprivacyaccessedapitypes/nsprivacyaccessedapitypereasons)
- [ubiquitousItemIsUploadedKey](https://developer.apple.com/documentation/foundation/urlresourcekey/ubiquitousitemisuploadedkey)
- [WWDC26 – What's new in the Foundation Models framework](https://developer.apple.com/videos/play/wwdc2026/241/)
- [crates.io – uniffi versions](https://crates.io/crates/uniffi/versions)
- [aermes-ai/expo-continued-task (community module)](https://github.com/aermes-ai/expo-continued-task) (repo exists; 0 stars, last push 2026-09-24; not vetted)

## Verification log

Verified 2026-10-05/06 by an adversarial fact-check pass. Apple API availability was checked against the Apple docs JSON (`developer.apple.com/tutorials/data/documentation/...json`), which is the data behind the doc pages.

| # | Claim | Verdict | Source |
|---|---|---|---|
| 1 | `PHAssetResource.dataSize` (`Int?` / `NSNumber *`) is iOS/macOS 27.0+, non-beta | Confirmed | Apple docs JSON `photos/phassetresource/datasize-5lxva`, `-6cf5k` |
| 2 | iOS 27 added `filename` and deprecated `originalFilename`/`uniformTypeIdentifier`; `contentType` is 26.0+ | Corrected: `originalFilename` is deprecated in 27.0, `uniformTypeIdentifier` in **27.2**. Added: `pixelWidth`/`pixelHeight` on `PHAssetResource` are iOS 16+ | Apple docs JSON for each symbol |
| 3 | `prefetchAssetExtendedMetadata` iOS 27+; `fetchPersistentChanges(since:)` iOS 16+; no Duplicates/Recently Deleted subtype in `PHAssetCollectionSubtype` | Confirmed | Apple docs JSON |
| 4 | `performChanges` shows a permission alert per call, so batch into one change block | Confirmed (exact Apple wording) | Apple docs JSON `performchanges(_:completionhandler:)` |
| 5 | Recently Deleted keeps items 30 days; Face ID/Touch ID needed by default since iOS 16; iCloud Photos deletes propagate to other devices | Confirmed; quote corrected to Apple's actual wording. Corrected: propagation is immediate, not "after the Recently Deleted window" (only permanent removal waits 30 days) | [Apple Support 104967](https://support.apple.com/en-us/104967) |
| 6 | Optimize iPhone Storage is on by default and keeps space-saving copies on device | Confirmed | [Apple Support 105061](https://support.apple.com/en-us/105061) |
| 7 | `evictUbiquitousItem(at:)` removes only the local copy; don't use a coordinated write; deleting from iCloud can't be undone | Confirmed. Added: `ubiquitousItemIsUploadedKey` must not be polled inside a coordinated read | Apple docs JSON |
| 8 | `trashItem` is iOS 11+; landing in Files' Recently Deleted is provider-dependent | Partly confirmed (API availability); Files' Recently Deleted behaviour for user deletes added from Apple Support; `trashItem` behaviour still **unverified** | Apple docs JSON; [Apple Support 104953](https://support.apple.com/en-us/104953) |
| 9 | Volume capacity keys and versions | Corrected: `volumeTotalCapacityKey` and `volumeAvailableCapacityKey` are iOS 4.0 (was "iOS 3-era" / blank); important/opportunistic keys iOS 11 confirmed; fingerprinting/Required Reason warning confirmed | Apple docs JSON |
| 10 | Required Reason codes 85F4.1, E174.1, C617.1, 3B52.1, DDA9.1 and "all forbid sending off-device" | Codes confirmed. Corrected: only 85F4.1, E174.1 and DDA9.1 carry the no-off-device clause (85F4.1 has a LAN-to-own-device exception); C617.1/3B52.1 do not | Apple docs JSON `nsprivacyaccessedapitypereasons` |
| 11 | `BGContinuedProcessingTask`/Request GA on iOS/iPadOS/Mac Catalyst 26.0; user-initiated; Live Activity; cancel calls expiration handler; GPU entitlement; low-progress tasks terminated first; `.queue`/`.fail` strategies | Confirmed | Apple docs JSON for task, request, SubmissionStrategy and the long-running-tasks article |
| 12 | Foundation Models: on-device context 4,096 tokens | Corrected: 4,096 is the iOS 26 figure in the docs article; WWDC26 says the rebuilt iOS 27 on-device model's `contextSize` returns 8,192. Read at runtime | Apple docs JSON; [WWDC26-241](https://developer.apple.com/videos/play/wwdc2026/241/) |
| 13 | `contextSize` "back-deployed to 26.4" | Corrected: `@backDeployed(before: iOS 26.4)`, i.e. it works back to 26.0 | Apple docs JSON |
| 14 | `PrivateCloudComputeLanguageModel` iOS 27+, 32K context, daily quota, iCloud+ upsell UI, managed entitlement, network required; `Attachment` iOS 27+ | Confirmed; added entitlement name, offline fallback guidance, and the WWDC free-tier threshold (threshold unverified in written terms) | Apple docs JSON; WWDC26-241 |
| 15 | App Review Guidelines last updated June 8, 2026; 2.3.1(a) virus-scanner example; 1.1.6; 2.4.2 SSD write cycles; 2.4.4; 2.5.2; 2.5.4; 2.5.9; 3.1.2(a); 5.6 | Confirmed (wording checked against the live page) | [App Review Guidelines](https://developer.apple.com/app-store/review/guidelines/) |
| 16 | React Native 0.87 latest stable (2026-08-10); 0.88 on 2026-10-12, 0.89 on 2026-12-07; 0.85 End of Cycle; latest three minors supported | Confirmed | [RN releases](https://reactnative.dev/docs/releases) |
| 17 | RN 0.87 removed `useTurboModules`, Strict TS API default (opt-out through 0.88), SwiftPM experimental/opt-in, AGP 9, Node ≥ 22.13; Hermes V1 default since 0.84 | Confirmed (the "Do not use it in production yet" fragment was not found verbatim; the confirmed quote is "CocoaPods remains the default and the supported path") | RN 0.87 and 0.84 blogs |
| 18 | Expo SDK 57 current stable (2026-06-30, RN 0.86); SDK 58 beta 2026-09-15 with RN 0.88 RC, Modules 2.0 (2.5–5.6× faster sync calls on iOS), scene lifecycle, R8 default, `expo-app-intents` alpha, EAS Xcode 26.6; Expo Go login 2026-09-03 | Confirmed | [Expo changelog](https://expo.dev/changelog), sdk-57, sdk-58-beta |
| 19 | SDK 55: Legacy Architecture dropped, min iOS 15.1; SDK 56: min iOS 16.4, Xcode 26.4, RN 0.85, Expo UI stable, Router forked from React Navigation | Confirmed | sdk-55, sdk-56 changelogs |
| 20 | "SDK 55 shipped Expo Router v7" | Corrected/unverified: since SDK 55 all Expo packages share the SDK major version; the "v7" label only appears in an unfetchable X post. "SDK 58 stabilises data loaders and native tabs" marked unverified | sdk-55 changelog |
| 21 | Expo "is moving to" optional non-breaking releases | Corrected: SDK 57 notes say Expo is *exploring* it | sdk-57 changelog |
| 22 | UniFFI current version | Corrected: 0.32.2 (crates.io 2026-09-23) is current; Findings and the alternatives table said 0.32.1. Added the 0.32.0 Kotlin `ByteBuffer` breaking change | crates.io API; GitHub tags; uniffi CHANGELOG |
| 23 | ubrn 0.31.0-6 (2026-09-25), "early development … should not yet be used in production", issue #449 open for uniffi 0.32 | Confirmed | GitHub API; ubrn docs site |
| 24 | Kotlin Swift export is Alpha; Flutter 3.47 latest stable | Confirmed | kotlinlang.org; docs.flutter.dev |
| 25 | `StorageStatsManager` needs `PACKAGE_USAGE_STATS` for other packages, "a system-level permission that will not be granted to normal apps" | Confirmed. Added omission: `ACTION_CLEAR_APP_CACHE` requires `MANAGE_EXTERNAL_STORAGE`, which Play restricts (no cleaner category) | Android reference; Google Play policy |
| 26 | Cited URLs (RN blogs, JetBrains, Flutter 3.44 blog, Tauri, Apple Support 108782, community Expo module) | Confirmed to resolve (HTTP 200; reactnative.dev returns 404 for bad paths). The X link could not be checked | curl / GitHub API |
| — | No public API for the iPhone Storage per-app breakdown; no public "iCloud Photos enabled" flag; Rust syscall coverage by Required Reason declarations; App Review enforcement against cleaner apps; remote rule feeds under 2.5.2; second larger on-device model in iOS 27 | Unverified (left marked in text) | — |

