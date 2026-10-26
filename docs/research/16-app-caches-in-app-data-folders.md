# Regenerable caches inside applications' data folders

- Date: 2026-10-26
- Question: which folders inside `~/Library/Application Support/<App>` can Lumen propose
  for quarantine, with sources, and which must it leave alone?
- Outcome: per-app knowledge-base entries for Chromium's caches in six Electron apps
  and for two Visual Studio Code caches; [ADR-0040](../decisions/0040-decide-known-artifacts-inside-app-state-on-their-own.md)
  so they can be decided inside the app's folder. Spotify's cache, Claude's VM bundle,
  partitions, service workers, logs and crash reports are left alone (below).

## What the owner's Mac shows

Sizes measured on 2026-10-26, all inside `~/Library/Application Support/`:

| App | Folder | Size |
| --- | --- | --- |
| Claude (desktop) | `Claude/Cache`, `Code Cache`, `GPUCache` | 718 MB, 103 MB, 5 MB |
| Claude (desktop) | `Claude/vm_bundles` | 11.7 GB |
| Spotify | `Spotify/PersistentCache` | 19.5 GB |
| Visual Studio Code | `Code/Cache`, `CachedData`, `CachedExtensionVSIXs` | 260 MB, 107 MB, 1.95 GB |
| Visual Studio Code | `Code/Crashpad`, `logs`, `Partitions` | 51 MB, 134 MB, 338 MB |
| Discord | `discord/Cache`, `Code Cache`, `logs` | 811 MB, 46 MB, 42 MB |
| Notion | `Notion/Partitions` (of which `notion/Cache` 711 MB, `Code Cache` 86 MB, `Service Worker` 2.1 GB) | 3.0 GB |
| Postman | `Postman/Partitions` (largest: `Cache` 220 MB, `Code Cache` 38 MB, `Service Worker` 187 MB) | 452 MB |

Each of these apps except Spotify ships `Electron Framework.framework` in its signed
bundle in `/Applications` (Claude 2.19675.1, Visual Studio Code 1.142.0, Discord
0.0.416, Notion 7.37.1, Postman 12.22.8, Obsidian 1.12.7; Figma 126.10.10 too). Spotify
1.3.5.3058 ships the Chromium Embedded Framework instead. Each Electron app's folder
holds Chromium's own files beside the caches (`Local State`, `Preferences`,
`Network Persistent State`), and each `Cache` holds Chromium's `Cache_Data`.

## Chromium's caches in an Electron app's data folder

An Electron app's `userData` folder is `~/Library/Application Support/<name>` on macOS,
`%APPDATA%\<name>` on Windows and `$XDG_CONFIG_HOME/<name>` or `~/.config/<name>` on
Linux. Its default session stores "localStorage, cookies, disk cache" there
(`sessionData` "points to `userData`" by default), and Electron warns apps against
naming their own folders after "Chromium's own subdirectories (such as `Cache`,
`GPUCache`, and `Local Storage`)" ([Electron `app.getPath`](https://www.electronjs.org/docs/latest/api/app)).

| Folder | What it is | Source |
| --- | --- | --- |
| `Cache` | The HTTP disk cache: Electron sets `http_cache_directory` to `<session path>/Cache` (`chrome::kCacheDirname`). "The disk cache stores resources fetched from the web so that they can be accessed quickly at a latter time"; Chromium discards and recreates it when it is missing or corrupt. Electron clears it with `ses.clearCache()`. | [network_context_service.cc](https://github.com/electron/electron/blob/main/shell/browser/net/network_context_service.cc), [chrome_constants.h](https://github.com/chromium/chromium/blob/main/chrome/common/chrome_constants.h), [Chromium disk cache](https://www.chromium.org/developers/design-documents/network-stack/disk-cache/), [`ses.clearCache`](https://www.electronjs.org/docs/latest/api/session) |
| `Code Cache` | V8's generated code for scripts and WebAssembly, "one cache per storage partition", at `<partition path>/Code Cache`. | [generated_code_cache.h](https://github.com/chromium/chromium/blob/main/content/browser/code_cache/generated_code_cache.h), [storage_partition_impl.cc](https://github.com/chromium/chromium/blob/main/content/browser/storage_partition_impl.cc) |
| `GPUCache`, `DawnWebGPUCache`, `DawnGraphiteCache` | The GPU process's disk caches (GL shaders, Dawn WebGPU, Dawn Graphite), one per profile directory. Electron clears them as `shadercache`. | [gpu_disk_cache_type.cc](https://github.com/chromium/chromium/blob/main/gpu/ipc/common/gpu_disk_cache_type.cc), [`ses.clearStorageData`](https://www.electronjs.org/docs/latest/api/session) |
| `Partitions/<name>` | A persistent session (`persist:` partition): Electron stores it at `<sessionData>/Partitions/<name>`, with the same layout, so `Partitions/<name>/Cache` and the others above are the same caches. | [electron_browser_context.cc](https://github.com/electron/electron/blob/main/shell/browser/electron_browser_context.cc) |

All five are rebuilt by the app as it runs: nothing in them is the user's data. Each
is open while the app runs, so Lumen's in-use check keeps them then
(`protect.in-use`).

Never proposed, because they hold the user's or the site's data: `Local Storage`,
`Session Storage`, `IndexedDB`, `Cookies`, `File System`, `blob_storage`,
`WebStorage`, `Shared Dictionary` and `Service Worker`. `Service Worker` includes the
Cache Storage that web apps use for offline data: Notion's is 2.1 GB, and removing it
may remove what Notion keeps for offline use. A whole `Partitions/<name>` folder holds
all of these, so only the cache folders inside it are named.

### Why per-app entries, not one generic rule

The knowledge base locates artifacts by path below a known root; marker files apply
only to project roots, which match anywhere on disk. A generic "a `Cache` folder beside
a `Local State` file" rule could not prove that the folder belongs to an Electron app's
data folder, and would match copies and backups anywhere. Each entry therefore names
one app's folder, chosen because the app was observed to be Electron and, where the
vendor documents it, at the vendor's documented location:

| App | Folder | Electron | Vendor source for the folder |
| --- | --- | --- | --- |
| Claude (desktop) | `Claude` | bundle | [MCP: Claude Desktop config](https://modelcontextprotocol.io/docs/develop/connect-local-servers) (`~/Library/Application Support/Claude`, `%APPDATA%\Claude`) |
| Visual Studio Code | `Code` | bundle, [source](https://github.com/microsoft/vscode/blob/main/src/mainImpl.ts) | [Settings file locations](https://code.visualstudio.com/docs/configure/settings#_settings-file-locations) (macOS, Windows, Linux) |
| Discord | `discord` | bundle, [Discord blog](https://discord.com/blog/how-discord-seamlessly-upgraded-millions-of-users-to-64-bit-architecture) | none found: macOS only, as observed |
| Notion | `Notion` | bundle | [Reset Notion](https://www.notion.com/help/reset-notion) (`Application Support`, `AppData\Roaming`) |
| Postman | `Postman` | bundle | none found for macOS: macOS only, as observed |
| Obsidian | `obsidian` | bundle | [How Obsidian stores data](https://obsidian.md/help/data-storage) (macOS, Windows, Linux) |

Not added: Slack and Microsoft Teams (not installed here, so their layout could not be
checked), Figma (its data folder has a different
layout: `Desktop`, `DesktopProfile`).

## Visual Studio Code's own caches

From the VS Code source (`main`, 2026-10-26):

- `CachedData/<commit>`: V8 cached data, created at startup by `src/mainImpl.ts`
  (`path.join(userDataPath, 'CachedData', commit)`). `CodeCacheCleaner` deletes other
  commits' folders after three months. Regenerated at the next start.
- `CachedExtensionVSIXs`: `extensionsDownloadLocation`, where Marketplace downloads are
  saved before they are extracted into the extensions folder. `ExtensionsDownloader`
  keeps the 20 most recent and deletes outdated versions. Extensions installed from a
  local `.vsix` file are read from that file, not copied here. Removing it means the
  next install of the same version downloads it again.
- `logs`: one folder per session; `LogsDataCleaner` keeps the nine most recent besides
  the current one. Not added: logs are never regenerated, they are the only record of
  past sessions, and VS Code already bounds them.
- `Crashpad`: crash reports, including ones not yet sent (`pending`). Not added.

Sources: [environmentService.ts](https://github.com/microsoft/vscode/blob/main/src/vs/platform/environment/common/environmentService.ts),
[extensionDownloader.ts](https://github.com/microsoft/vscode/blob/main/src/vs/platform/extensionManagement/node/extensionDownloader.ts),
[extensionManagementService.ts](https://github.com/microsoft/vscode/blob/main/src/vs/platform/extensionManagement/node/extensionManagementService.ts),
[codeCacheCleaner.ts](https://github.com/microsoft/vscode/blob/main/src/vs/code/electron-utility/sharedProcess/contrib/codeCacheCleaner.ts),
[logsDataCleaner.ts](https://github.com/microsoft/vscode/blob/main/src/vs/code/electron-utility/sharedProcess/contrib/logsDataCleaner.ts).

## Refused

- **Spotify `PersistentCache` (19.5 GB):** Spotify's [storage article](https://support.spotify.com/us/article/storage-information/)
  says the app stores "parts of music and podcasts as cache" and "downloaded music and
  podcasts to play offline", and on desktop tells users to clear the cache in
  Settings → Storage → Clear cache. It does not say where downloads live on desktop or
  that the folder holds only cache. The folder also holds Spotify's own settings and
  an offline index (`offline.bnk`, `Users/`). Not regenerable as far as the evidence
  goes, and the vendor's way to clear it is a setting, not a command, which an entry
  cannot carry (`official_action` is a program and arguments). Candidate for later: a
  knowledge-base field for settings guidance.
- **Claude `vm_bundles` (11.7 GB):** it holds a VM image (`rootfs.img`, `vmlinuz`,
  `initrd`) and `sessiondata.img`, which by name is the VM's session data. No Anthropic
  source says what it holds or that the app downloads it again. Not added; revisit with
  a primary source.
- **`Partitions` as a whole, `Service Worker`, `Local Storage`, `IndexedDB`, `Session
  Storage`, `Cookies`:** the user's and the sites' data (above).
- **`logs` and `Crashpad` (VS Code, Discord):** records, not caches; about 230 MB here.
- **Unknown folders:** Discord's `component_crx_cache`, `download_cache`, `module_data`;
  VS Code's `CachedProfilesData`, `CachedConfigurations`; Claude's `fcache`. No source
  says what they hold.
