# Electron update staging and further tool caches

- Date: 2026-10-31
- Question: which of the unlinked `~/Library/Caches` children and home-folder tool
  caches named in [research 26](26-reference-cleaners-and-recovery-gap.md) can become
  knowledge-base entries, with which safety class, and from which sources?
- Outcome: Electron update staging is recognized but kept (`self_managed`): Squirrel.Mac
  installs the staged update when the app quits and prunes older ones itself.
  electron-updater's download cache, dotslash, pnpm's metadata cache, Poetry's
  repository cache and rustup's download and temporary folders get entries, one PR
  each. Yarn, the Android SDK's `~/.android/cache` and Gradle's build cache stay out.

Sizes are allocated bytes from `du -sh` on the owner's Mac on 2026-10-31; no file was
opened. Folder names were listed only to confirm layouts.

## Electron update staging (Squirrel.Mac)

| Folder | Size |
| --- | --- |
| `~/Library/Caches/com.microsoft.VSCode.ShipIt` | 1.5 GB |
| `~/Library/Caches/notion.id.ShipIt` | 440 MB (of which `ShipIt_stderr.log` 120 MB) |
| `~/Library/Caches/com.anthropic.claudefordesktop.ShipIt`, `com.figma.Desktop.ShipIt` | 76 KB, 32 KB |

Electron's `autoUpdater` on macOS "is built upon Squirrel.Mac"
([Electron](https://www.electronjs.org/docs/latest/api/auto-updater)); Visual Studio
Code uses it directly (`updateService.darwin.ts`). In
[Squirrel.Mac](https://github.com/Squirrel/Squirrel.Mac) at `4ad0974`:

- `SQRLShipItLauncher.m`: the job label is the app's bundle ID plus `.ShipIt`;
  `SQRLDirectoryManager.m` puts the storage folder at `NSCachesDirectory/<label>`, with
  `ShipItState.plist` (the pending install request) and `ShipIt_stdout.log`/
  `ShipIt_stderr.log`.
- `SQRLUpdater.m`: each download goes to a fresh `update.XXXXXXX` folder (`mkdtemp`)
  holding the unpacked app; the archive is deleted after unpacking. Before every
  update check it prunes the other `update.*` folders, keeping the one
  `ShipItState.plist` names, and states: "Pruning directories while an update is
  pending or in progress will result in undefined behavior."
- ShipIt waits for the app to quit, then moves the staged bundle into place.

On the owner's Mac both folders hold one staged app (`Visual Studio Code.app` 1.5 GB,
`Notion.app` 319 MB) and both apps' `ShipIt` processes are running, waiting for the
app to quit. Removing the folder now would remove an update about to be installed,
which Squirrel's own source calls undefined behavior.

**Decision:** per-app entries `vscode-squirrel-update-staging` and
`notion-squirrel-update-staging`, `self_managed` (the policy keeps them): quitting the
app installs the update, and the next check prunes what is left. The knowledge base's
`*` matches a whole name only, so `*.ShipIt` cannot be written; per-app names also
avoid matching any other `.ShipIt`. Bundle IDs are the installed apps'. Claude and
Figma's folders are a few kilobytes and left out. The logs are not caches; logs are a
later, age-based category (research 26).

## electron-updater's download cache

`~/Library/Caches/notion-updater`, 260 MB: `update.zip` 130 MB and `pending/` 130 MB.

[electron-updater](https://github.com/electron-userland/electron-builder) at `e56fd9a`:
`AppAdapter.ts` puts the cache under `~/Library/Caches` on macOS; `AppUpdater.ts` names
it `updaterCacheDirName`, by default the app's name in lower case plus `-updater`
(`app-builder-lib/src/appInfo.ts`). `DownloadedUpdateHelper.ts` downloads into
`pending/` and empties it before each new download. `MacUpdater.ts` copies each
download to `update.zip` "for caching for future differential downloads" and, when it
is missing, logs "falling back to full download". The
[auto-update guide](https://www.electron.build/docs/features/auto-update/) says the
same of a missing old version: "that one update is downloaded in full, which fills the
cache for the next one", and that on macOS "Squirrel.Mac stages updates natively and
applies them on relaunch". Squirrel copies the update into its own folder above, so
removing this one costs a full download next time.

**Decision:** `notion-electron-updater-cache`, `regenerable`, macOS. A generic
`*-updater` cannot be written and would also match other updaters
(`org.pqrs.Karabiner-Updater`, compared without case on macOS).

## dotslash

`~/Library/Caches/dotslash`, 1.1 GB (React Native DevTools fetched by React Native's
DotSlash files; `dotslash` itself is not on the owner's `PATH`).

DotSlash's [flags page](https://github.com/facebook/dotslash/blob/main/website/docs/flags.md)
documents the default cache (`$XDG_CACHE_HOME/dotslash` or `$HOME/.cache/dotslash` on
Linux, `$HOME/Library/Caches/dotslash` on macOS, `{FOLDERID_LocalAppData}/dotslash` on
Windows) and `DOTSLASH_CACHE`; the [execution page](https://github.com/facebook/dotslash/blob/main/website/docs/execution.md)
shows a missing artifact being fetched again. The source (`dotslash_cache.rs`,
`subcommand.rs`) holds only content-addressed artifacts and `locks/`, and
`dotslash -- clean` makes the tree writable and removes the cache. Artifact folders
are read-only (`dr-xr-xr-x`), which Lumen's executor does not undo.

**Decision:** `dotslash-cache`, `tool_action` with `dotslash -- clean` (no dry run).
The command is listed under "experimental commands" in `dotslash --help`; where
`dotslash` is not found, the app says so.

## pnpm's metadata cache

`~/Library/Caches/pnpm`, 646 MB (`v12/metadata` 151 MB, `metadata-full` 387 MB,
`metadata-full-filtered` 109 MB, and `lockfile-verified.jsonl`).

pnpm's [`cacheDir`](https://pnpm.io/settings/other#cachedir) defaults to
`$XDG_CACHE_HOME/pnpm` when set, else `~/Library/Caches/pnpm`, `~/.cache/pnpm` or
`~/AppData/Local/pnpm-cache`, and holds "package metadata, dlx cache, and some install
verification results". [`pnpm cache path`](https://pnpm.io/cli/cache-path) names the
verification log: re-verifying a lockfile is the cost of losing it.
[`pnpm cache prune`](https://pnpm.io/cli/cache-prune): "Only the cache is affected, and
it refills itself." [Uninstalling pnpm](https://pnpm.io/uninstall) says to remove the
cache directory. Global packages live under `pnpmHomeDir/global` and the store
elsewhere, neither in this folder.

**Decision:** `pnpm-metadata-cache`, `regenerable`. pnpm's commands do not clear the
folder: `cache delete` covers one registry's metadata and `cache prune` only folders
the running pnpm cannot read, both experimental.

## Poetry

Not installed on the owner's Mac. Poetry's
[configuration](https://python-poetry.org/docs/configuration/#cache-dir) gives
`cache-dir` defaults (`~/Library/Caches/pypoetry`, `~/.cache/pypoetry`,
`%LOCALAPPDATA%\pypoetry\Cache`) and `POETRY_CACHE_DIR`, and puts
`virtualenvs.path` at `{cache-dir}/virtualenvs`: the projects' environments. The
whole folder is therefore not a cache. In the source (`config/config.py`), the
repository cache is `{cache-dir}/cache/repositories`; `cache clear --all`
(`console/commands/cache/clear.py`) flushes exactly that folder after a confirmation
whose default is yes. `{cache-dir}/artifacts` has no documentation and no command.
The default comes from `platformdirs` (`locations.py`, `platformdirs>=3.0.0,<5`), whose
current macOS class lets `XDG_CACHE_HOME` take precedence (`macos.py`), so the entry
also matches below `XDG_CACHE_HOME` on macOS, as pip's does.

**Decision:** `poetry-repository-cache`, `tool_action` with
`poetry cache clear --all --no-interaction`, only at `cache/repositories`. Never the
virtual environments; `artifacts` is left out.

## rustup

`~/.rustup/downloads` 120 MB, `~/.rustup/tmp` empty.

`RUSTUP_HOME` defaults to `~/.rustup` or `%USERPROFILE%/.rustup`
([environment variables](https://rust-lang.github.io/rustup/environment-variables.html)).
In [rustup](https://github.com/rust-lang/rustup)'s source, `dist/download.rs` keeps
partial downloads in `downloads/`, keyed by hash, to resume them, and removes each one
after use; `tmp/` holds temporary files (`dist/temp.rs`). `rustup update` without
arguments ends with "cleaning up downloads & tmp directories" (`cli/rustup_mode.rs`),
but it also updates every toolchain, so it is not a cleanup command.

**Decision:** `rustup-downloads`, `regenerable`, at `.rustup/downloads` and
`.rustup/tmp` below the home folder or `RUSTUP_HOME`. Toolchains and
`update-hashes` stay out.

## Left out

- **Yarn.** Research 10 row 35 still holds. Lumen runs a tool's command in the home
  folder (ADR-0049); there, on the owner's Mac, `yarn` is Corepack's shim and runs
  Yarn 1.22.22, whose `cache clean` empties Yarn 1's cache, not Berry's
  `~/.yarn/berry/cache` (1.2 GB). Removing Berry's global cache directly breaks
  Plug'n'Play projects, which load packages from it, until `yarn install` runs. Yarn
  1's cache path is undocumented and absent here.
- **`~/.android/cache`** (9.3 MB): no Android documentation describes it.
- **Gradle's `caches/build-cache-*`:** inside `gradle-user-home-caches`
  (`self_managed`); Gradle "will periodically clean up the local cache directory"
  ([build cache](https://docs.gradle.org/current/userguide/build_cache.html)). None on
  this Mac.
- Android Gradle intermediates, `node_modules`, Xcode, SwiftPM, React Native and Expo
  are covered in other PRs.
