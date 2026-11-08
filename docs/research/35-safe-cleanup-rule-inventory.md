# Safe cleanup rule inventory and remaining gaps

- Date: 2026-11-05
- Scope: issue #704; the committed developer knowledge base, not a scan of user storage.
- Baseline: 46 entries before this change; Angular CLI adds one entry.

Each linked entry records **all** platform-specific roots, exact path patterns,
expected tool/owner, official evidence URLs, verification date, safety class and
supported command when applicable. This table indexes those records; source dates
are not renewed by inventorying them. Only Angular's sources were rechecked here.

## Shared exclusion and execution conditions

Locations supply identification evidence, never cleanup permission. All entries
exclude symlinks, junctions and other reparse objects. An explicit directory kind
also excludes an unexpected regular file. Project entries require a scanned regular
marker; directory, link, reparse or unobserved markers do not qualify. Matching is
component-based, not a substring, and does not classify descendants separately.
Nested instances of the same project pattern are excluded.

The existing deterministic policy applies coverage, protected-location, app-state,
cloud, in-use, ownership and storage-capability checks. A missing match leaves an
item unrecognised; name, age, extension and AI output never grant eligibility.
Known tool identity is not a claim that an OS account owns a path: current-user
ownership and identity are freshly checked before execution. Unknown evidence is
not absence. A tool-action or non-regenerable class never becomes an automatic
quarantine proposal. Actual reclaimed bytes are measured separately.

Mechanisms below: **quarantine** means a reversible proposal after all gates;
**command** means the reviewed tool command after its own confirmation; **review**
means no regenerability proof; **self-managed** means keep and let the tool manage it.

## Current entries

| Rule / expected owner | Platforms | Expected locations | Official evidence | Supported mechanism |
| --- | --- | --- | --- | --- |
| [android-gradle-project-build-cache](../../crates/lumen-devkb/entries/android-gradle-project-build-cache.json) / Android Gradle plugin | linux, macos, windows | `project: build/intermediates` | [source](https://developer.android.com/reference/tools/gradle-api/com/android/build/api/artifact/Artifact.Category) | quarantine |
| [angular-build-cache](../../crates/lumen-devkb/entries/angular-build-cache.json) / Angular CLI | linux, macos, windows | `project: .angular/cache` | [source](https://angular.dev/cli/cache) | quarantine |
| [arc-browser-caches](../../crates/lumen-devkb/entries/arc-browser-caches.json) / Arc | macos | `home: Library/Caches/Arc/User Data/*/Cache`; `home: Library/Caches/Arc/User Data/*/Code Cache` | [source](https://chromium.googlesource.com/chromium/src/+/HEAD/docs/user_data_dir.md) | quarantine |
| [brave-browser-caches](../../crates/lumen-devkb/entries/brave-browser-caches.json) / Brave | linux, macos | `home: Library/Caches/BraveSoftware/Brave-Browser/*/Cache`; `home: Library/Caches/BraveSoftware/Brave-Browser/*/Code Cache`; 4 more in entry | [source](https://github.com/brave/brave-core/blob/master/build/config.gni) | quarantine |
| [cargo-global-cache](../../crates/lumen-devkb/entries/cargo-global-cache.json) / Cargo | linux, macos, windows | `home: .cargo/registry`; `home: .cargo/git`; 2 more in entry | [source](https://doc.rust-lang.org/cargo/guide/cargo-home.html) | self-managed |
| [cargo-project-build-cache](../../crates/lumen-devkb/entries/cargo-project-build-cache.json) / Cargo | linux, macos, windows | `project: target/debug/deps`; `project: target/debug/build`; 4 more in entry | [source](https://doc.rust-lang.org/cargo/reference/build-cache.html) | quarantine |
| [claude-code-session-data](../../crates/lumen-devkb/entries/claude-code-session-data.json) / Claude Code | linux, macos, windows | `home: .claude/projects`; `home: .claude/file-history`; 4 more in entry | [source](https://code.claude.com/docs/en/claude-directory#application-data) | review |
| [claude-desktop-chromium-caches](../../crates/lumen-devkb/entries/claude-desktop-chromium-caches.json) / Claude desktop app | macos, windows | `home: Library/Application Support/Claude/Cache`; `home: Library/Application Support/Claude/Code Cache`; 18 more in entry | [source](https://modelcontextprotocol.io/docs/develop/connect-local-servers) | quarantine |
| [cocoapods-download-cache](../../crates/lumen-devkb/entries/cocoapods-download-cache.json) / CocoaPods | macos | `home: Library/Caches/CocoaPods/Pods` | [source](https://guides.cocoapods.org/terminal/commands.html#tab_pod_cache_clean) | command |
| [discord-chromium-caches](../../crates/lumen-devkb/entries/discord-chromium-caches.json) / Discord | macos, windows | `home: Library/Application Support/discord/Cache`; `home: Library/Application Support/discord/Code Cache`; 18 more in entry | [source](https://discord.com/blog/how-discord-seamlessly-upgraded-millions-of-users-to-64-bit-architecture) | quarantine |
| [docker-desktop-disk-image](../../crates/lumen-devkb/entries/docker-desktop-disk-image.json) / Docker Desktop | linux, macos, windows | `home: Library/Containers/com.docker.docker/Data/vms/0/data/Docker.raw`; `home: .docker/desktop/vms/0/data/Docker.raw`; 2 more in entry | [source](https://docs.docker.com/desktop/settings-and-maintenance/backup-and-restore/) | review; confirmed command |
| [dotslash-cache](../../crates/lumen-devkb/entries/dotslash-cache.json) / DotSlash | linux, macos, windows | `home: Library/Caches/dotslash`; `home: .cache/dotslash`; 3 more in entry | [source](https://dotslash-cli.com/docs/flags/) | command |
| [expo-cli-download-caches](../../crates/lumen-devkb/entries/expo-cli-download-caches.json) / Expo CLI | linux, macos, windows | `home: .expo/ios-simulator-app-cache`; `home: .expo/android-apk-cache`; 1 more in entry | [source](https://docs.expo.dev/more/expo-cli/#environment-variables) | quarantine |
| [figma-chromium-caches](../../crates/lumen-devkb/entries/figma-chromium-caches.json) / Figma | macos, windows | `home: Library/Application Support/Figma/DesktopProfile/*/Cache`; `home: Library/Application Support/Figma/DesktopProfile/*/Code Cache`; 8 more in entry | [source](https://help.figma.com/hc/en-us/articles/22380853110551) | quarantine |
| [go-build-cache](../../crates/lumen-devkb/entries/go-build-cache.json) / Go | linux, macos, windows | `home: Library/Caches/go-build`; `home: .cache/go-build`; 3 more in entry | [source](https://pkg.go.dev/cmd/go#hdr-Build_and_test_caching) | self-managed |
| [go-module-cache](../../crates/lumen-devkb/entries/go-module-cache.json) / Go | linux, macos, windows | `home: go/pkg/mod`; `$GOMODCACHE: <root>` | [source](https://go.dev/ref/mod#module-cache) | command |
| [google-chrome-caches](../../crates/lumen-devkb/entries/google-chrome-caches.json) / Google Chrome | linux, macos | `home: Library/Caches/Google/Chrome/*/Cache`; `home: Library/Caches/Google/Chrome/*/Code Cache`; 4 more in entry | [source](https://chromium.googlesource.com/chromium/src/+/HEAD/docs/user_data_dir.md) | quarantine |
| [gradle-user-home-caches](../../crates/lumen-devkb/entries/gradle-user-home-caches.json) / Gradle | linux, macos, windows | `home: .gradle/caches`; `home: .gradle/wrapper/dists`; 2 more in entry | [source](https://docs.gradle.org/current/userguide/directory_layout.html) | self-managed |
| [homebrew-download-cache](../../crates/lumen-devkb/entries/homebrew-download-cache.json) / Homebrew | linux, macos | `home: Library/Caches/Homebrew`; `home: .cache/Homebrew`; 2 more in entry | [source](https://docs.brew.sh/Manpage#cleanup-options-formulacask-) | command |
| [jetbrains-system-directory](../../crates/lumen-devkb/entries/jetbrains-system-directory.json) / JetBrains IDEs | linux, macos, windows | `home: Library/Caches/JetBrains/*`; `local_app_data: JetBrains/*`; 1 more in entry | [source](https://www.jetbrains.com/help/idea/directories-used-by-the-ide-to-store-settings-caches-plugins-and-logs.html#system-directory) | review |
| [microsoft-edge-caches](../../crates/lumen-devkb/entries/microsoft-edge-caches.json) / Microsoft Edge | linux, macos | `home: Library/Caches/Microsoft Edge/*/Cache`; `home: Library/Caches/Microsoft Edge/*/Code Cache`; 4 more in entry | [source](https://learn.microsoft.com/en-us/microsoft-edge/extensions/developer-guide/native-messaging) | quarantine |
| [nextjs-build-cache](../../crates/lumen-devkb/entries/nextjs-build-cache.json) / Next.js | linux, macos, windows | `project: .next/cache` | [source](https://nextjs.org/docs/app/guides/ci-build-caching) | quarantine |
| [node-modules-with-lockfile](../../crates/lumen-devkb/entries/node-modules-with-lockfile.json) / npm, pnpm, Yarn, Bun | linux, macos, windows | `project: node_modules` | [source](https://docs.npmjs.com/cli/v11/commands/npm-ci) | quarantine |
| [notion-chromium-caches](../../crates/lumen-devkb/entries/notion-chromium-caches.json) / Notion | macos, windows | `home: Library/Application Support/Notion/Cache`; `home: Library/Application Support/Notion/Code Cache`; 18 more in entry | [source](https://www.notion.com/help/reset-notion) | quarantine |
| [notion-electron-updater-cache](../../crates/lumen-devkb/entries/notion-electron-updater-cache.json) / Notion | macos | `home: Library/Caches/notion-updater` | [source](https://www.electron.build/docs/features/auto-update/) | quarantine |
| [notion-squirrel-update-staging](../../crates/lumen-devkb/entries/notion-squirrel-update-staging.json) / Notion | macos | `home: Library/Caches/notion.id.ShipIt` | [source](https://www.electronjs.org/docs/latest/api/auto-updater) | self-managed |
| [npm-cache](../../crates/lumen-devkb/entries/npm-cache.json) / npm | linux, macos, windows | `home: .npm/_cacache`; `local_app_data: npm-cache/_cacache`; 2 more in entry | [source](https://docs.npmjs.com/cli/v12/commands/npm-cache) | command |
| [npm-npx-cache](../../crates/lumen-devkb/entries/npm-npx-cache.json) / npx | linux, macos, windows | `home: .npm/_npx`; `local_app_data: npm-cache/_npx`; 2 more in entry | [source](https://docs.npmjs.com/cli/v11/commands/npm-cache/) | command |
| [obsidian-chromium-caches](../../crates/lumen-devkb/entries/obsidian-chromium-caches.json) / Obsidian | linux, macos, windows | `home: Library/Application Support/obsidian/Cache`; `home: Library/Application Support/obsidian/Code Cache`; 28 more in entry | [source](https://obsidian.md/help/data-storage) | quarantine |
| [pip-cache](../../crates/lumen-devkb/entries/pip-cache.json) / pip | linux, macos, windows | `home: .cache/pip`; `home: Library/Caches/pip`; 3 more in entry | [source](https://pip.pypa.io/en/stable/topics/caching/) | command |
| [pnpm-metadata-cache](../../crates/lumen-devkb/entries/pnpm-metadata-cache.json) / pnpm | linux, macos, windows | `home: Library/Caches/pnpm`; `home: .cache/pnpm`; 2 more in entry | [source](https://pnpm.io/settings/other#cachedir) | quarantine |
| [pnpm-store](../../crates/lumen-devkb/entries/pnpm-store.json) / pnpm | linux, macos, windows | `home: Library/pnpm/store`; `home: .local/share/pnpm/store`; 3 more in entry | [source](https://pnpm.io/cli/store) | command |
| [poetry-repository-cache](../../crates/lumen-devkb/entries/poetry-repository-cache.json) / Poetry | linux, macos, windows | `home: Library/Caches/pypoetry/cache/repositories`; `home: .cache/pypoetry/cache/repositories`; 3 more in entry | [source](https://python-poetry.org/docs/configuration/) | command |
| [postman-chromium-caches](../../crates/lumen-devkb/entries/postman-chromium-caches.json) / Postman | macos, windows | `home: Library/Application Support/Postman/Cache`; `home: Library/Application Support/Postman/Code Cache`; 18 more in entry | [source](https://support.postman.com/hc/en-us/articles/4410785125271-How-to-remove-local-data) | quarantine |
| [react-native-prebuilt-cache](../../crates/lumen-devkb/entries/react-native-prebuilt-cache.json) / React Native | macos | `home: Library/Caches/ReactNative` | [source](https://github.com/facebook/react-native/pull/56844) | quarantine |
| [rustup-downloads](../../crates/lumen-devkb/entries/rustup-downloads.json) / rustup | linux, macos, windows | `home: .rustup/downloads`; `home: .rustup/tmp`; 2 more in entry | [source](https://rust-lang.github.io/rustup/environment-variables.html) | quarantine |
| [swiftpm-shared-cache](../../crates/lumen-devkb/entries/swiftpm-shared-cache.json) / Swift Package Manager | macos | `home: Library/Caches/org.swift.swiftpm/repositories`; `home: Library/Caches/org.swift.swiftpm/registry/downloads` | [source](https://github.com/swiftlang/swift-package-manager/blob/ce48202febce7b8ca0e15ea0081f67bb94502ea7/Sources/PackageManagerDocs/Documentation.docc/Package/PackagePurgeCache.md) | quarantine |
| [uv-cache](../../crates/lumen-devkb/entries/uv-cache.json) / uv | linux, macos, windows | `home: .cache/uv`; `$XDG_CACHE_HOME: uv`; 2 more in entry | [source](https://docs.astral.sh/uv/concepts/cache/) | command |
| [visual-studio-component-model-cache](../../crates/lumen-devkb/entries/visual-studio-component-model-cache.json) / Visual Studio | windows | `local_app_data: Microsoft/VisualStudio/*/ComponentModelCache` | [source](https://learn.microsoft.com/en-us/troubleshoot/developer/visualstudio/ide/troubleshooting-docker-errors) | quarantine |
| [vscode-cached-data](../../crates/lumen-devkb/entries/vscode-cached-data.json) / Visual Studio Code | linux, macos, windows | `home: Library/Application Support/Code/CachedData`; `roaming_app_data: Code/CachedData`; 1 more in entry | [source](https://code.visualstudio.com/docs/configure/settings#_settings-file-locations) | quarantine |
| [vscode-chromium-caches](../../crates/lumen-devkb/entries/vscode-chromium-caches.json) / Visual Studio Code | linux, macos, windows | `home: Library/Application Support/Code/Cache`; `home: Library/Application Support/Code/Code Cache`; 28 more in entry | [source](https://code.visualstudio.com/docs/configure/settings#_settings-file-locations) | quarantine |
| [vscode-extension-vsix-cache](../../crates/lumen-devkb/entries/vscode-extension-vsix-cache.json) / Visual Studio Code | linux, macos, windows | `home: Library/Application Support/Code/CachedExtensionVSIXs`; `roaming_app_data: Code/CachedExtensionVSIXs`; 1 more in entry | [source](https://code.visualstudio.com/docs/configure/settings#_settings-file-locations) | quarantine |
| [vscode-squirrel-update-staging](../../crates/lumen-devkb/entries/vscode-squirrel-update-staging.json) / Visual Studio Code | macos | `home: Library/Caches/com.microsoft.VSCode.ShipIt` | [source](https://www.electronjs.org/docs/latest/api/auto-updater) | self-managed |
| [xcode-archives](../../crates/lumen-devkb/entries/xcode-archives.json) / Xcode | macos | `home: Library/Developer/Xcode/Archives` | [source](https://help.apple.com/xcode/mac/current/en.lproj/deva52afe8a4.html) | review |
| [xcode-derived-data](../../crates/lumen-devkb/entries/xcode-derived-data.json) / Xcode | macos | `home: Library/Developer/Xcode/DerivedData/*` | [source](https://developer.apple.com/forums/thread/812408) | quarantine |
| [xcode-ios-device-support](../../crates/lumen-devkb/entries/xcode-ios-device-support.json) / Xcode | macos | `home: Library/Developer/Xcode/iOS DeviceSupport/*` | [source](https://developer.apple.com/forums/thread/812408) | quarantine |
| [xcode-simulator-devices](../../crates/lumen-devkb/entries/xcode-simulator-devices.json) / Xcode | macos | `home: Library/Developer/CoreSimulator/Devices` | [source](https://developer.apple.com/forums/thread/812408) | command |

## New, bounded discovery: Angular CLI

[Angular's cache reference](https://angular.dev/cli/cache) documents the default
persistent build-cache directory. The
[workspace reference](https://angular.dev/reference/configs/workspace-config)
locates workspace configuration in `angular.json`. Recognise only directory
`.angular/cache` beside a scanned regular `angular.json`, on macOS, Windows and
Linux. The cache is regenerable build state; the next build may take longer.
Neither configuration contents nor dependency contents are opened or executed.

Exclude `.angular` itself, source, `dist`, cache descendants, sibling names,
unmarked directories and nested occurrences. Custom cache paths remain unknown:
reading or interpreting configuration would exceed metadata-only scanning.
Reversible quarantine uses the existing gates. No Angular command is run from a
scan, and no project-scoped command is added to the home-scoped tool runner.
Positive, negative and adversarial fixtures cover every desktop platform and both
whole-scan and chunked matching. Existing policy property tests preserve protected
precedence independently of which regenerable tool supplied evidence.

## Remaining gaps and deliberately rejected shortcuts

| Gap | Why it stays unknown / guarded | Next evidence needed |
| --- | --- | --- |
| Custom build/cache roots | Configuration can point at source or user data | A separate read-only, explicit configuration contract |
| Arbitrary temporary files or old logs | Name and age do not prove regenerability | Per-owner documented lifecycle and active-use checks |
| Yarn variants | Global/project cache behavior differs by version and configuration | Supported scope-aware tool inventory, not guessed paths |
| Simulator runtimes and Docker objects | A path does not describe resource ownership or impact | Native tool analyzers (#705, #706) |
| Unknown application data | It may be documents, state, credentials or a database | Keep/review until provenance can be established |

This inventory does not promise an amount of free space or broaden automatic
removal. Runtime assessment and the handle-based executor remain the authority.
