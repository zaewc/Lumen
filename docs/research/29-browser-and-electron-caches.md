# Chromium browsers' and large Electron apps' caches

- Date: 2026-10-31
- Question: which disk caches of Chromium-based browsers (Google Chrome, Arc, Microsoft
  Edge, Brave) and of large Electron or embedded-Chromium apps (Slack, Microsoft Teams,
  Figma, Spotify, Zoom) can Lumen's knowledge base name, with sources, without touching
  a browser profile or weakening a protected location?
- Outcome: one knowledge-base entry per app, each in its own PR: Google Chrome, Arc,
  Microsoft Edge and Brave on macOS and Linux (their user cache directory, outside the
  profile), Figma on macOS and Windows (inside its documented data folder, decided on
  its own under [ADR-0040](../decisions/0040-decide-known-artifacts-inside-app-state-on-their-own.md)).
  Browsers on Windows, Slack, Teams, Spotify and Zoom are not added (below).
  Continues [research 16](16-app-caches-in-app-data-folders.md) and the first row of
  [research 26](26-reference-cleaners-and-recovery-gap.md)'s adoption table.

## Chromium keeps a browser's caches outside the profile on macOS and Linux

Chromium's [user data directory](https://chromium.googlesource.com/chromium/src/+/HEAD/docs/user_data_dir.md)
document defines a "user cache dir" per profile:

- macOS: when `Application Support` is an ancestor of the profile directory, "the user
  cache dir is `Library/Caches` plus the relative path from `Application Support` to the
  profile dir", e.g. `~/Library/Caches/Google/Chrome/Default`. Otherwise it is the
  profile directory itself.
- Linux: when the config directory (`~/.config` or `$XDG_CONFIG_HOME`) is an ancestor,
  it is the cache directory (`~/.cache` or `$XDG_CACHE_HOME`) plus the same relative
  path, e.g. `~/.cache/google-chrome/Default`. Otherwise it is the profile directory.
- Windows and ChromeOS: "the user cache dir is the same as the profile dir".

The code agrees (`main`, 2026-10-31): `chrome::GetUserCacheDirectory` in
[chrome_paths_mac.mm](https://github.com/chromium/chromium/blob/main/chrome/common/chrome_paths_mac.mm)
and [chrome_paths_linux.cc](https://github.com/chromium/chromium/blob/main/chrome/common/chrome_paths_linux.cc)
maps the profile directory as above and falls back to the profile directory. Two
caches are put there, and only two:

| Folder | What it is | Source |
| --- | --- | --- |
| `<cache dir>/Cache` | The HTTP disk cache: `http_cache_directory = base_cache_path.Append(chrome::kCacheDirname)`, where `base_cache_path` is `GetUserCacheDirectory(profile path)`. Cookies, `Network Persistent State` and the rest of the network data stay in the profile (`data_directory = path.Append(kNetworkDataDirname)`). | [profile_network_context_service.cc](https://github.com/chromium/chromium/blob/main/chrome/browser/net/profile_network_context_service.cc), [chrome_constants.h](https://github.com/chromium/chromium/blob/main/chrome/common/chrome_constants.h), [Chromium disk cache](https://www.chromium.org/developers/design-documents/network-stack/disk-cache/) |
| `<cache dir>/Code Cache` | V8's generated code cache: `GetGeneratedCodeCacheSettings` returns `GetUserCacheDirectory(context->GetPath())`, and the storage partition appends `Code Cache` (site-isolated partitions add their relative path first). | [chrome_content_browser_client.cc](https://github.com/chromium/chromium/blob/main/chrome/browser/chrome_content_browser_client.cc), [storage_partition_impl.cc](https://github.com/chromium/chromium/blob/main/content/browser/storage_partition_impl.cc) |

Both are rebuilt as the browser browses. The GPU caches (`GPUCache`, `DawnWebGPUCache`,
`DawnGraphiteCache`, [gpu_disk_cache_type.cc](https://github.com/chromium/chromium/blob/main/gpu/ipc/common/gpu_disk_cache_type.cc))
and the top-level `ShaderCache`, `GrShaderCache` and `GraphiteDawnCache` stay in the
user data directory, so a browser entry names only `Cache` and `Code Cache` in the
user cache directory: nothing in the profile.

Profiles are named by the browser (`Default`, `Profile 1`, `System Profile`, `Guest
Profile`), so each entry uses one `*` for the profile and names the cache folder below
it. The `*` matches exactly one name: neither the profile's cache folder itself nor
anything deeper (site-isolated partitions' caches under `Storage/ext/...` are left for
later).

### Protections are unchanged

Lumen hard-protects Chrome's user data directory on every platform
(`protect.location.chrome-macos`, `chrome-windows`, `chrome-linux` in
`crates/lumen-policy/data/protected-locations.json`): `~/Library/Application
Support/Google/Chrome`, `%LOCALAPPDATA%\Google\Chrome\User Data` and
`~/.config/google-chrome`. The entries' macOS and Linux locations are outside those
folders, so the protections are untouched and still keep everything in the profile.
On Windows the caches are inside the protected profile, so no Windows location is
added: the policy would keep them anyway, and the protection is the decision.

Edge and Brave have no protected location. Their Windows caches are inside their
profiles (`%LOCALAPPDATA%\Microsoft\Edge\User Data\<Profile>`, `...\BraveSoftware\Brave-Browser\User Data\<Profile>`),
beside cookies, saved passwords and history. To treat every browser's profile alike,
no Windows location is added for them either. Whether Edge's and Brave's user data
directories should be protected like Chrome's is a separate decision.

### Where the caches sit in a scan

On macOS `~/Library/Caches/Google`, `~/Library/Caches/Arc`, `~/Library/Caches/Microsoft
Edge` and `~/Library/Caches/BraveSoftware` are not named after a bundle ID, so no
application data location claims them (`macos.support-location.bundle-id` links only
`~/Library/Caches/<bundle id>`). A match there is the outermost candidate and is decided
on its own. While the browser runs, its caches are open, and `protect.in-use` keeps them.

## Per app

| App | User data directory (source) | Cache locations added | On this Mac |
| --- | --- | --- | --- |
| Google Chrome 156.0.8078.76 | macOS `~/Library/Application Support/Google/Chrome`, Linux `~/.config/google-chrome` ([user data dir](https://chromium.googlesource.com/chromium/src/+/HEAD/docs/user_data_dir.md)) | `~/Library/Caches/Google/Chrome/*/{Cache,Code Cache}`; `~/.cache/google-chrome/*/...` and `$XDG_CACHE_HOME/google-chrome/*/...` | `Default/Cache` 23 MB, `Default/Code Cache` 20 MB, `System Profile/Code Cache` 16 KB |
| Arc 1.170.0 (ArcCore.framework, Chromium 156.0.8078.48) | `~/Library/Application Support/Arc/User Data` (observed: `~/Library/Caches/Arc/User Data/{Default,Profile 1,Profile 2}`, each with `Cache` and `Code Cache` only) | `~/Library/Caches/Arc/User Data/*/{Cache,Code Cache}` | `Default` 1.2 GB + 699 MB, `Profile 1` 101 MB + 12 MB, `Profile 2` 3.3 MB + 2.1 MB: about 2.0 GB |
| Microsoft Edge | macOS `~/Library/Application Support/Microsoft Edge`, Linux `~/.config/microsoft-edge` ([Edge native messaging](https://learn.microsoft.com/en-us/microsoft-edge/extensions/developer-guide/native-messaging), citing Chromium's user data dir); Windows `%LOCALAPPDATA%\Microsoft\Edge\User Data` ([Edge directory variables](https://learn.microsoft.com/en-us/deployedge/edge-learnmore-create-user-directory-vars)) | `~/Library/Caches/Microsoft Edge/*/{Cache,Code Cache}`; `~/.cache/microsoft-edge/*/...` and `$XDG_CACHE_HOME/microsoft-edge/*/...` | not installed |
| Brave | macOS `~/Library/Application Support/BraveSoftware/Brave-Browser` (`CrProductDirName` from `brave_product_dir_name` in [brave-core build/config.gni](https://github.com/brave/brave-core/blob/master/build/config.gni) and [tweak_info_plist.py](https://github.com/brave/brave-core/blob/master/build/mac/tweak_info_plist.py)); Linux `$XDG_CONFIG_HOME/BraveSoftware/Brave-Browser` ([chrome_paths_linux.cc override](https://github.com/brave/brave-core/blob/master/chromium_src/chrome/common/chrome_paths_linux.cc)) | `~/Library/Caches/BraveSoftware/Brave-Browser/*/{Cache,Code Cache}`; `~/.cache/BraveSoftware/Brave-Browser/*/...` and `$XDG_CACHE_HOME/...` | not installed |
| Figma 126.10.11 (Electron) | `~/Library/Application Support/Figma`, `%APPDATA%\Figma`, holding `Desktop` and `DesktopProfile` ([Clear the Figma desktop app cache](https://help.figma.com/hc/en-us/articles/22380853110551)) | `Figma/DesktopProfile/*/{Cache,Code Cache,GPUCache,DawnGraphiteCache,DawnWebGPUCache}` | `v43`: `Cache` 121 MB, `Code Cache` 371 MB, `GPUCache` 0.5 MB; Dawn caches in `v34`–`v43` about 50 MB |

Sizes are `du -sh` of exactly those folders on 2026-10-31; nothing was opened.

Notes:

- **Chrome's other channels** (`Google/Chrome Beta`, `Chrome Canary`, `Chrome Dev`)
  and Chromium (`~/Library/Caches/Chromium`) use the same rule; not added, since none is
  installed here and their user data directories are not protected like stable's.
- **Arc** documents no folder; The Browser Company's help centre has no data-location
  article, and Arc's sign-off is scheduled for 2026-12-22. The entry rests on Chromium's
  rule, the Chromium framework inside the signed bundle, and the observed layout (each
  profile's cache folder holds only `Cache` and `Code Cache`). `~/Library/Caches/Arc/EaselImagesCache`
  (empty here) and Arc's own folders in `Application Support/Arc` (`ArchiveSnapshotCache`,
  `*FaviconCache`, `StorableArchive*.json`) are Arc's, undocumented, and not added.
  Arc's `~/Library/Caches/company.thebrowser.Browser` is its bundle-ID cache folder,
  already decided by the app-cache rule.
- **Edge**: the native-messaging page documents the macOS and Linux user data
  directories (`NativeMessagingHosts` is "the `NativeMessagingHosts` subdirectory in the
  user data directory"); the cache directory follows from Chromium's rule. Microsoft's
  `DiskCacheDir` policy moves it ([DiskCacheDir](https://learn.microsoft.com/en-us/deployedge/microsoft-edge-browser-policies/diskcachedir));
  a moved cache is simply not matched.
- **Brave** sets `CrProductDirName` to `BraveSoftware/Brave-Browser` (`-Beta`, `-Dev`,
  `-Nightly` for other channels; only the release channel is added) and overrides only
  the Linux default user data directory, not `GetUserCacheDirectory`.
- **Figma**'s help article removes the whole `Figma` folder to "clear the cache", but
  that folder also holds the session (`Cookies`), `Local Storage`, `IndexedDB` and
  `Service Worker`; only Chromium's cache folders inside each `DesktopProfile/v<N>`
  (Electron's `userData`, with `Local State`) are named. Old `v<N>` folders that Figma
  left behind keep their state; only their caches match.

## Not added

- **Google Chrome, Edge, Brave and Arc on Windows**: the caches are inside the profile
  (above). Arc on Windows also lives in an MSIX package's `LocalCache`; no source.
- **Slack**: no Slack source documents its data folder or its layout (only third-party
  guides), and it is not installed here to observe. Research 16 reached the same.
- **Microsoft Teams (new)**: it is a WebView2 app; Microsoft's
  [Clear the Teams client cache](https://learn.microsoft.com/en-us/troubleshoot/microsoftteams/teams-administration/clear-teams-cache)
  deletes all of `%LOCALAPPDATA%\Packages\MSTeams_8wekyb3d8bbwe\LocalCache\Microsoft\MSTeams`,
  or `~/Library/Group Containers/UBF8T346G9.com.microsoft.teams` and
  `~/Library/Containers/com.microsoft.teams2` on macOS, and warns that this deletes the
  diagnostic logs and personal settings. That is the app's state, not a cache, and
  Microsoft does not document the WebView2 layout inside it. Not installed here.
- **Spotify**: `~/Library/Application Support/Spotify/PersistentCache` (19 GB here)
  stays out as in research 16: Spotify says the app stores "downloaded music and
  podcasts to play offline", and the folder also holds an offline index. Spotify's
  embedded Chromium (CEF) keeps a full browser profile in
  `~/Library/Caches/com.spotify.client` (1.1 GB): `Local State`, and in `Default` and
  `Browser` `Cookies`, `Login Data`, `Local Storage`, `History` beside `Cache` (36 KB)
  and `Code Cache` (16 KB). Because that folder is Spotify's bundle-ID cache folder, an
  entry inside it would be decided with it (ADR-0040), so it would change nothing.
  Follow-up: the app-cache rule (Apple: apps "must be able to re-create" what they keep
  in `Caches`) proposes this whole folder, sign-in data included, when Spotify is not
  running.
- **Zoom**: `~/Library/Application Support/zoom.us` holds `data` (local databases),
  `Plugins`, `CefPlugin`, `asr`; Zoom documents no cache folder there. Not added.
