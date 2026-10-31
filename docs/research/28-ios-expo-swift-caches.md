# iOS, React Native, Expo and Swift developer caches

- Date: 2026-10-31
- Question: which large iOS, React Native, Expo and Swift locations on a developer
  Mac does the owning tool's documentation or source identify as caches it
  regenerates?
- Outcome: three entries, one per tool: Expo CLI's Expo Go downloads, React Native's
  shared prebuilt-binary cache and SwiftPM's shared repository and registry caches.
  `ios/build`, `ios/Pods`, Xcode Previews, `XCTestDevices` and
  `CoreSimulator/Caches` stay unrecognized.

Measurements are directory sizes on the owner's Mac (`du`, names only; no file was
opened, moved or removed). They show where space goes, not what is reclaimable.

## Expo CLI: `~/.expo` download caches

The [Expo CLI reference](https://docs.expo.dev/more/expo-cli/#environment-variables)
documents `EXPO_NO_CACHE`: "app config JSON schemas, Expo Go binaries for simulators
and emulators, and project templates are cached in the global `.expo` directory".
The CLI source, pinned at
[expo/expo@cc008c0](https://github.com/expo/expo/tree/cc008c0d2fad8f4f0f6aadbc4bf2e2147069d6bc/packages/@expo/cli/src),
names the folders:

- `downloadExpoGoAsync.ts` writes the extracted iOS Expo Go app to
  `~/.expo/ios-simulator-app-cache/<name>.app` and the Android APK to
  `~/.expo/android-apk-cache/<name>.apk`, downloading again when the file is missing,
  and itself deletes entries older than six months.
- The same download goes through a one-week HTTP response cache in `~/.expo/expo-go`
  (`downloadAppAsync.ts`: "Users can also nuke their `~/.expo` directory to clear the
  cache").
- `UserSettings.ts` resolves the folder from `os.homedir()` on every platform.

Match exactly those three directories (the iOS one on macOS only, because only a Mac
runs the iOS Simulator), never `~/.expo` itself. `state.json` holds the sign-in
session secret, `codesigning` holds development certificates and keys, and
`ngrok.yml` is configuration: none is a cache. The `.app` inside
`ios-simulator-app-cache` is a downloaded copy of Expo Go, not something the user
built. The small response caches (`schema-cache`, `versions-cache`, `template-cache`,
`native-modules-cache`, under 1 MB together here) are left out as not worth the
surface. Owner's Mac: `expo-go` 747 MB, `ios-simulator-app-cache` 459 MB,
`android-apk-cache` 200 MB.

Classified `regenerable` (minutes, network). Expo CLI has no command that clears only
these folders, so there is no `official_action`. The redirect variables
(`__UNSAFE_EXPO_HOME_DIRECTORY`, `EXPO_STAGING`, `EXPO_LOCAL`) are not followed; only
the documented default is covered.

## React Native: `~/Library/Caches/ReactNative`

React Native's CocoaPods scripts keep a shared cache of the prebuilt Hermes,
ReactNativeDependencies and ReactNativeCore tarballs they download from Maven
([facebook/react-native#56844](https://github.com/facebook/react-native/pull/56844),
merged 2026-06-24). At
[facebook/react-native@4d536fa](https://github.com/facebook/react-native/tree/4d536fa8cd1ce5a1bb8f158ec3c0ec780d94ab20/packages/react-native),
`scripts/cocoapods/utils.rb` and `sdks/hermes-engine/hermes-utils.rb` define
`shared_cache_dir` as `~/Library/Caches/ReactNative`. On a miss `pod install`
downloads the tarball again, verifies its SHA-1 when Maven publishes one and saves
a copy; a cached tarball
that fails verification is deleted and downloaded again. Owner's Mac: 457 MB.

Classified `regenerable` (minutes, network), macOS only, directory only. There is no
cleanup command.

## SwiftPM: shared repository and registry caches

`swift package purge-cache` is documented as "Purge the global repository cache"
([SwiftPM docs source](https://github.com/swiftlang/swift-package-manager/blob/ce48202febce7b8ca0e15ea0081f67bb94502ea7/Sources/PackageManagerDocs/Documentation.docc/Package/PackagePurgeCache.md)).
At [swiftlang/swift-package-manager@ce48202](https://github.com/swiftlang/swift-package-manager/tree/ce48202febce7b8ca0e15ea0081f67bb94502ea7),
`Basics/FileSystem/FileSystem+Extensions.swift` puts the shared cache in the user
caches folder as `org.swift.swiftpm` (`~/Library/Caches/org.swift.swiftpm` on macOS;
`~/.swiftpm/cache` is a symbolic link to it), and `CoreCommands/SwiftCommandState.swift`
purges `repositories` (Git mirrors), `registry/downloads` and the manifest cache
database. Dependencies are fetched again on the next resolve.

Match only `repositories` and `registry/downloads`, macOS, directories. Leave out the
manifest database (kilobytes), `package-collection.db` and `package-metadata`, which
purge-cache does not touch, and the configuration and security folders.

Classified `regenerable` (minutes, network) rather than `tool_action`. A scratch-folder
experiment (`swift package --cache-path <tmp> purge-cache`, Swift 6.4) shows the
command works outside a package but creates a `.build` scratch folder in its working
directory; ADR-0049 runs commands in the home folder, so it would leave `~/.build`
behind. Quarantine is reversible, and the policy's in-use and coverage checks still
apply. The Linux location (`FileManager` caches directory) is not covered: not
verified against documentation.

## Left out

- **`ios/build` in React Native or Expo projects** (5.3 GB here). Neither tool
  creates it by default: Expo CLI's `run:ios` passes no `-derivedDataPath`
  (`run/ios/XcodeBuild.ts`), and its reference says `--no-build-cache` clears "the
  derived data directory", Xcode's default under `~/Library/Developer/Xcode/DerivedData`
  (already covered). React Native CLI's `--buildFolder`
  ([buildOptions.ts](https://github.com/react-native-community/cli/blob/main/packages/cli-platform-apple/src/commands/buildCommand/buildOptions.ts))
  has no default. The folder comes from a user's own `-derivedDataPath`, e.g. a Detox
  configuration, and on this Mac one project's `ios/build` holds signed IPAs from a
  custom script. Matching it would claim deliverables.
- **`ios/Pods`.** `pod install` recreates it, but
  [CocoaPods leaves committing it to source control to the team](https://guides.cocoapods.org/using/using-cocoapods.html#should-i-check-the-pods-directory-into-source-control),
  and it is where local patches to dependencies are
  often made. Its contents cannot be shown to equal what `pod install` would produce
  without opening files.
- **Xcode `~/Library/Developer/Xcode/UserData/Previews`.** No Apple documentation. The
  [forum thread](https://developer.apple.com/forums/thread/682439) reports second-hand
  that an Apple engineer suggested `xcrun simctl --set previews delete all`, and that
  deleting the folder's contents by hand stopped previews working; `simctl help` does
  not document the `previews` set.
- **`~/Library/Developer/XCTestDevices`** and **`~/Library/Developer/CoreSimulator/Caches`.**
  No Apple documentation found. An Apple DTS engineer advises managing CoreSimulator
  through Xcode or `simctl` rather than by hand
  ([thread 812408](https://developer.apple.com/forums/thread/812408)). Both are empty
  on this Mac.
- **Xcode Archives** remain unrecognized as regenerable: they hold dSYMs.

## Verification

Matcher tests cover each entry on every platform it names, the neighbouring folders
it must not match (Expo's sign-in state and keys, the parent caches, nested files)
and links, files and reparse points at a cache path. Schema, embedding and policy
class checks apply to every entry.
