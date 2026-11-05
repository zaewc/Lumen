# Explicit developer-tool storage inventory

Verified 2026-11-05. `lumen inspect xcode [--json]` queries only Apple's
`xcrun simctl runtime list --json`. It never opens runtime image paths, scans
their mounted filesystems, or selects images to delete. Run normal `scan` and
`assess` for DerivedData, device support, archives and simulator device data.

The installed Xcode's `xcrun simctl help runtime` documents JSON on the `list`
operation. A local read-only observation confirmed `identifier`, `version`,
`build`, `state` and optional `sizeBytes` fields. Unknown additional fields are
ignored; a missing field, changed identity or malformed format fails explicitly.
Missing size is unknown. Image size is not physically reclaimable space.

The executable follows the existing user-terminal PATH trust boundary (ADR-0049).
Arguments are constants, input is closed, output is bounded to 8 KiB, and a query
is stopped after 30 seconds. A full output buffer is rejected as potentially
truncated; service failures and permission errors never become an empty list.
Queries run only on explicit request, not during scanning or assessment, and
metadata stays local. Non-macOS platforms report unsupported.

[Apple's developer support guidance](https://developer.apple.com/forums/thread/812408)
recommends managing CoreSimulator through Xcode or `simctl`, and quitting Xcode
before changing files behind it.
[Apple's runtime management instructions](https://developer.apple.com/documentation/safari-developer-tools/adding-additional-simulators)
describe Xcode Settings > Components and Window > Devices and Simulators. The
inventory prints those workflows and their documentation link. This read-only
preview grants no deletion permission and does not change KEEP/REVIEW precedence.
