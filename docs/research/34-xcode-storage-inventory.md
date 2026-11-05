# Xcode storage inventory

Verified 2026-11-04 for issue #705. Discovery is metadata-only; none of the
following permits removing active files or files with unknown ownership.

| Category | Discovery | Policy and supported management |
| --- | --- | --- |
| DerivedData | Existing `xcode-derived-data` entry | Regenerable; normal ownership, in-use, coverage and protection gates apply |
| Device support | Existing `xcode-ios-device-support` entry | Regenerable, with the same gates |
| Archives | `~/Library/Developer/Xcode/Archives`, directory only | Not regenerable; review, never an automatic cleanup proposal; inspect archives in Xcode Organizer |
| Simulator devices | Existing `xcode-simulator-devices` entry | Tool-managed; user data is not a cache; the existing confirmed command only removes unavailable devices |
| Simulator runtimes | Requires an explicit tool inventory, not guesses at private disk-image paths | Manage in Xcode Settings > Components; runtime inventory is a follow-up part of #705 |

[Apple's location reference](https://help.apple.com/xcode/mac/current/en.lproj/deva52afe8a4.html)
documents the default archive location and allows custom locations. This rule
does not guess where relocated archives went. Apple also explains that distributed
builds' archives and debug symbols are needed to diagnose crash reports in
[Building your app to include debugging information](https://developer.apple.com/documentation/xcode/building-your-app-to-include-debugging-information).
An archive therefore is not treated as regenerable merely because source exists.

[Apple's simulator management instructions](https://developer.apple.com/documentation/safari-developer-tools/adding-additional-simulators)
direct users to Xcode's Components settings to remove runtimes. No raw simulator
runtime disk image or system asset is made a quarantine candidate by this change.
