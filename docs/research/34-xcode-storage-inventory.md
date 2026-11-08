# Xcode storage inventory

Verified 2026-11-04 for issue #705. Discovery is metadata-only; none of the
following permits removing active files or files with unknown ownership.

| Category | Discovery | Policy and supported management |
| --- | --- | --- |
| DerivedData | `xcode-derived-data`: each folder in `~/Library/Developer/Xcode/DerivedData`, directory only | Regenerable; normal ownership, in-use, coverage and protection gates apply |
| Device support | `xcode-ios-device-support`: each folder in `iOS DeviceSupport`, directory only | Regenerable, with the same gates |
| Archives | `~/Library/Developer/Xcode/Archives`, directory only | Not regenerable; review, never an automatic cleanup proposal; inspect archives in Xcode Organizer |
| Simulator devices | `xcode-simulator-devices`: the `CoreSimulator/Devices` directory | Tool-managed; user data is not a cache; the existing confirmed command only removes unavailable devices |
| Simulator runtimes | Requires an explicit tool inventory, not guesses at private disk-image paths | Manage in Xcode Settings > Components; inventoried read-only by `lumen inspect xcode` and the desktop's Developer tools section |

[Apple's location reference](https://help.apple.com/xcode/mac/current/en.lproj/deva52afe8a4.html)
documents the default archive location and allows custom locations. This rule
does not guess where relocated archives went. Apple also explains that distributed
builds' archives and debug symbols are needed to diagnose crash reports in
[Building your app to include debugging information](https://developer.apple.com/documentation/xcode/building-your-app-to-include-debugging-information).
An archive therefore is not treated as regenerable merely because source exists.

[Apple's simulator management instructions](https://developer.apple.com/documentation/safari-developer-tools/adding-additional-simulators)
direct users to Xcode's Components settings to remove runtimes. No raw simulator
runtime disk image or system asset is made a quarantine candidate by this change.

## Active Xcode and simulators

Lumen does not look for an Xcode or Simulator process by name. The in-use probe
(`lumen-platform-macos::probe`) asks the kernel which processes hold an open file,
a working directory or an executable inside a candidate folder. It compares by
identity and by canonical path prefix. A DerivedData or device-support folder that
Xcode, `xcodebuild`, SourceKit or a simulator is using is therefore kept, and if
some processes cannot be checked the result is partial, never "unused". Entries
match real directories only: links and files at those locations are not
categorized.
