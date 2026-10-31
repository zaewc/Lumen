# Reference cleaners and Lumen's recovery gap

- Date: 2026-10-31
- Question: how far behind established cleaners is the space Lumen can recover, where
  does the gap come from, and which of their practices can Lumen adopt without
  weakening its safety rules?
- Outcome: most of the gap is space Lumen never assesses, not space it refuses. Widen
  the knowledge base with sourced, regenerable locations; tell people which running
  apps hold caches back; let people quarantine review items they choose
  ([ADR-0051](../decisions/0051-quarantine-review-items-the-user-chooses.md)). AI stays
  evidence only.

## Measurement on the owner's Mac

A read-only `lumen scan ~` and `lumen assess --no-ai` at `dc55a3e` (policy 0.13.0),
with full coverage (nothing denied or failed). No plan, quarantine or tool command ran.

| | Items | GB |
| --- | --- | --- |
| Scanned under `~` | | 307.3 |
| Assessed at all | 254 | about 84 |
| Proposed for quarantine | 15 | 12.85 (12.8 of it the Lumen repository's own Cargo intermediates) |
| Review, with a tool command | 6 | 17.93 |
| Kept because a running app had it open (`protect.in-use`) | 55 | 49.6 |
| Never assessed | | about 223 |

An item is assessed only when evidence names it: a knowledge-base match, an
application's support location by exact bundle ID, or a vendor cache declaration.
Everything else is invisible in the app. The largest never-assessed groups were VM disk
images (58.9 GB), project folders (57.1 GB, of which `node_modules` 29.1 GB, Android
module builds and `.cxx` 11.1 GB, `ios/build` 5.3 GB), the Android SDK (26.8 GB),
unlinked `~/Library/Caches` children (10.3 GB: Arc's Chromium data, `*.ShipIt` update
staging, dotslash, pnpm's metadata cache, React Native) and unlinked Application
Support folders (14.1 GB, mostly user data such as game saves).

## What the references do

Read 2026-10-30/31: MacPaw's CleanMyMac knowledge base, PureMac
([momenbasel/PureMac](https://github.com/momenbasel/PureMac) at `b66e705`), BleachBit's
CleanerML ([bleachbit/bleachbit](https://github.com/bleachbit/bleachbit) at `31dd501`),
Mole ([tw93/Mole](https://github.com/tw93/Mole)), Microsoft's Disk Cleanup and Storage
Sense documentation, and Apple's storage and file-system guides.

| Category | References | Adopt in Lumen |
| --- | --- | --- |
| Chromium and Electron disk caches (`Cache`, `Code Cache`, `GPUCache`, Dawn caches) | all; BleachBit and Mole check that the app is closed | Yes, per app, from Chromium's own layout; never cookies, storage, IndexedDB or service workers |
| Project build outputs (Gradle intermediates, `.cxx`, Xcode derived data in `ios/build`, `node_modules` beside a lockfile) | PureMac, Mole | Yes, narrow and marker-gated, from tool documentation |
| Package-manager and tool caches (Yarn, dotslash, pnpm metadata, Expo, SwiftPM, rustup downloads) | Mole | Yes where the tool documents the cache; the tool's own command where it manages the cache |
| Electron update staging (`<bundle id>.ShipIt`, `*-updater/pending`) | Mole | Yes, from Squirrel and electron-updater sources |
| Large and old files, unknown large folders | CleanMyMac, DaisyDisk | Shown for review; quarantined only when the person chooses ([ADR-0051](../decisions/0051-quarantine-review-items-the-user-chooses.md)) |
| Xcode archives, iOS backups, Windows.old | all offer them | Never proposed: not regenerable; review only |
| Language files and universal-binary thinning | CleanMyMac; PureMac refuses signed bundles | No: it breaks code signatures for little space |
| Wholesale `~/Library/Caches` | BleachBit, PureMac | No: it hits offline music, iCloud and File Provider databases (PureMac #142) |
| Logs and crash reports | all | Later, user-domain only, with an age rule |
| Windows `%TEMP%` | Disk Cleanup, Storage Sense, BleachBit | Later, files older than 7 days, as Microsoft documents |

Practices worth copying: per-app "is it running" checks (Lumen already has a stronger
open-file probe), age thresholds for logs and temp files, explicit exceptions for
iCloud and File Provider data, never preselecting what cannot be rebuilt. Practices to
avoid: permanent deletion without undo (all four delete outright; Lumen quarantines),
cleaning straight from a summary without review (CleanMyMac), no in-use check
(PureMac), and user data filed under "cache" or "history" (BleachBit's Zoom recordings,
VS Code local history, Slack local storage).

## AI

Lumen's judge may only make a decision more cautious (ADR-0020), and its measured
critical false-positive bound on the 2026.10.1 dataset is far above the 0.5 % gate
([evaluation](../ai/evaluation.md)). Letting it propose removals would trade safety for
space, which AGENTS.md forbids. AI contributes where a person decides: describing what
review items are (ADR-0041), and, with ADR-0051, helping a person choose which review
items to quarantine. Knowledge-base growth itself is AI-assisted at authoring time:
entries are researched and drafted with AI help, then checked against the cited
sources before merge; nothing a model says at run time becomes a rule.

## Limits

One machine, one user, one day; sizes are allocated bytes from the scan, not measured
free-space gains (see [research 24](24-space-recovery-experiment.md)). `/Library`,
`/opt/homebrew` and other volumes were not scanned.
