# Cloud sync folders

- Date: 2026-10-31
- Question: where do cloud providers keep their synced folders on macOS and Windows,
  and what can Lumen detect without new I/O? Moving an item out of a sync folder makes
  the provider delete it in the cloud and on every other device. Security audit FS-2
  ([public-beta readiness](../security/public-beta-readiness.md)) found that only an
  item's own placeholder or sync-root flags kept it.
- Outcome: two layers.
  1. Scan flags (application layer): everything at or below a placeholder, dataless or
     sync-root directory is cloud-backed, and so is every directory holding a flagged
     item (`lumen_application::cloud`).
  2. Static protected locations (`kind: cloud` in
     `crates/lumen-policy/data/protected-locations.json`, policy 0.16.0): each
     provider's documented default folder, with everything below it, is `KEEP`
     (`protect.location.<id>`, risk `cloud_backed`).

  Neither layer opens, hydrates or reads any file.

## Sources

Read 2026-10-31.

**macOS File Provider (Dropbox, Google Drive, OneDrive, Box and others).**

- Google, [Use Drive for desktop on macOS](https://support.google.com/drive/answer/12178485):
  with File Provider, files are in "~/Library/CloudStorage" or
  "/Users/$USER/Library/CloudStorage" by default. "You can't change this location."
- Dropbox, [Dropbox for macOS on File Provider](https://help.dropbox.com/installs/macos-support-for-expected-changes):
  "Your Dropbox folder will be moved to ~/Library/CloudStorage." "Changing the
  location of your Dropbox folder isn't supported by Dropbox on File Provider."
  Moving it to a compatible external drive is being rolled out to some users.
- Microsoft, [Missing files and autosave issues with OneDrive on macOS](https://support.microsoft.com/office/missing-files-and-autosave-issues-with-onedrive-on-macos-7fc55219-7fb5-4885-b117-1c8cb53d2db5):
  "OneDrive's path has moved from [username] > OneDrive to a new location in Library
  > Cloud Storage > OneDrive". "A shortcut or alias has also been created in its
  previous location."
- Apple documents File Provider domains, not their folder. `NSFileProviderManager`
  lists only the calling app's own domains, so no public API enumerates every
  provider's domain. `~/Library/CloudStorage` is therefore protected as a location.

**iCloud Drive on macOS.**

- Apple documents ubiquity containers through
  [`FileManager.url(forUbiquityContainerIdentifier:)`](https://developer.apple.com/documentation/foundation/filemanager/url(forubiquitycontaineridentifier:))
  and does not document their folder. The folder `~/Library/Mobile Documents` was
  measured instead: a scan left its dataless files dataless, and no directory there
  reported `EF_IS_SYNC_ROOT` ([research 03](03-macos-platform.md)). Scan flags alone
  therefore cannot find a materialised item in iCloud Drive, so the folder is a
  protected location.
- Apple, [Add your Desktop and Documents files to iCloud Drive](https://support.apple.com/en-us/109344):
  "When you turn on Desktop & Documents Folders, your Desktop folder and Documents
  folder are stored in iCloud Drive." When it is turned off, "a new Desktop folder and
  Documents folder are created locally on your Mac in the home folder."

**Windows.**

- Microsoft, [Build a cloud sync engine](https://learn.microsoft.com/en-us/windows/win32/cfapi/build-a-cloud-file-sync-engine):
  sync engines register sync roots, and placeholders are reparse points of the
  `cldflt.sys` minifilter. Files are placeholder, full, or pinned full. Lumen already
  exposes the placeholders and flags the recall, pinned and unpinned attributes and
  cloud reparse tags ([research 04](04-windows-platform.md)). The scan flags therefore
  cover any Cloud Files sync root they reach, wherever it is.
- Microsoft, [`KNOWNFOLDERID`](https://learn.microsoft.com/en-us/windows/win32/shell/knownfolderid):
  `FOLDERID_SkyDrive`, display name OneDrive, default path `%USERPROFILE%\OneDrive`.
- Microsoft, [OneDrive policies: Set a custom name for the OneDrive folder](https://learn.microsoft.com/en-us/sharepoint/use-group-policy#set-a-custom-name-for-the-onedrive-folder):
  "By default, the folder name is *OneDrive - {organization name}*". For example, the
  path is `C:\Users\{alias}\OneDrive - {organization name}`. The default location "is
  under %userprofile%", and the `DefaultRootDir` policy and a custom name change it.
- Microsoft, [Redirect and move Windows known folders to OneDrive](https://learn.microsoft.com/en-us/sharepoint/redirect-known-folders):
  Known Folder Move moves Desktop, Documents, Pictures, Screenshots and Camera Roll
  into OneDrive. Those folders then lie inside the OneDrive folder and are covered by
  it.
- Apple, [Set up iCloud Drive on your Windows computer](https://support.apple.com/guide/icloud-windows/icw0144825a5/icloud):
  "Your files and folders are stored in C:\Users\[user name]\iCloud Drive by
  default." iCloud for Windows 14 and later can use another folder. Older versions
  used `%USERPROFILE%\iCloudDrive` (Apple Community and setup guides). It is kept too,
  because a wrong match only makes Lumen keep more.

**Dropbox outside File Provider (Windows, Linux, macOS opt-out).**

- Dropbox, [Locate the Dropbox folder](https://help.dropbox.com/installs/locate-dropbox-folder):
  "By default, the Dropbox folder is located in your user folder". With a personal and
  a business account, the folders are `Dropbox (Personal)` and
  `Dropbox (<business name>)`.

**Google Drive for desktop outside File Provider.**

- Google, [Stream & mirror files with Drive for desktop](https://support.google.com/drive/answer/13401938):
  streamed files are "on a virtual Drive on your computer". When mirroring, "My Drive
  files download to the folder you select". No default folder is documented.

## What is protected

| Location | Id | Platforms |
| --- | --- | --- |
| `~/Library/CloudStorage` (every File Provider domain) | `macos-cloud-storage` | macOS |
| `~/Library/Mobile Documents` (iCloud Drive and app containers) | `icloud-drive-macos` | macOS |
| `~/OneDrive` | `onedrive-windows`, `onedrive-macos-legacy` | Windows, macOS (legacy client; now an alias) |
| `~/OneDrive - *` | `onedrive-organization` | Windows, macOS (legacy client) |
| `~/Dropbox`, `~/Dropbox (*` | `dropbox`, `dropbox-named` | macOS, Windows, Linux |
| `~/iCloud Drive`, `~/iCloudDrive` | `icloud-drive-windows`, `icloud-drive-windows-legacy` | Windows |

Names compare without case on macOS and Windows. A last name ending in `*` matches
every name with that prefix. Anything else at or below a flagged directory, or holding
a flagged item, is cloud-backed through the scan flags.

## What is not protected yet (follow-ups)

- **iCloud Desktop & Documents.** With it on, `~/Desktop` and `~/Documents` are
  stored in iCloud Drive. No Mac available for this research had it on, so nothing was
  measured. Apple documents no flag or attribute that marks these folders, and a
  materialised folder there (a `node_modules` in `~/Documents/project`) is found only
  when a dataless item sits below it. Protecting `~/Desktop` and `~/Documents` outright
  would be a guess with a large cost, so it waits for a measurement of what the scan
  reports there (`EF_IS_SYNC_ROOT`, dataless items, extended attributes) on a Mac with
  the feature on.
- **Windows sync roots outside the defaults:** a OneDrive folder moved with
  `DefaultRootDir` or renamed with the custom name policy, Google Drive mirror folders,
  Box Drive, and Dropbox moved by the user. The scan flags cover them when their items
  carry the Cloud Files attributes or tags, but nothing protects them statically.
  `StorageProviderSyncRootManager.GetCurrentSyncRoots()` would list every registered
  root. That is new WinRT I/O for the Windows platform crate, so it is a follow-up.
- **Google Drive's streamed drive on Windows** is its own volume, and a scan does not
  cross volumes. A mirror folder has no documented default.
- **A scan root inside a sync root** (scanning `~/Library/CloudStorage/Dropbox/x`
  directly) sees no flagged ancestor. The static locations cover the defaults.
