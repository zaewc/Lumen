# Leftovers after removal (roadmap 16.9)

- Status: **Accepted** (owner, 2026-11-16): every recommended answer below, Q1–Q6.
  Q1–Q2 are recorded as [ADR-0068](../decisions/0068-recognise-a-removed-apps-data-only-with-corroboration.md).
- Roadmap: [Phase 16, item 16.9](../architecture/roadmap.md#phase-16-installed-software-after-0100),
  issue #810 (part of #804)
- Builds on: [ADR-0008](../decisions/0008-capability-model-and-coverage.md),
  [ADR-0039](../decisions/0039-ask-for-review-only-where-a-person-can-act.md),
  [ADR-0051](../decisions/0051-quarantine-review-items-the-user-chooses.md),
  [ADR-0059](../decisions/0059-use-application-usage-as-cautious-evidence.md),
  [ADR-0060](../decisions/0060-list-software-and-remove-it-only-through-its-own-mechanism.md),
  [ADR-0062](../decisions/0062-uninstall-macos-apps-into-quarantine.md),
  [ADR-0064](../decisions/0064-read-package-relations-from-each-managers-own-records.md),
  [apps-in-explore.md](apps-in-explore.md)

## Problem

When a macOS app is removed, its data in `~/Library` stays. ADR-0062 rule 4 promises
that after the next scan "that data appears in Explore as data of an app that is not
installed, and is decided there (ADR-0039)". Today it does not.

## What is settled already

| Question | Answer | Fixed by |
| --- | --- | --- |
| What verdict does a removed app's data get? | `REVIEW` (`classify.orphan`), never a proposal. A removed app's cache is also `REVIEW`: the most cautious proposal wins. The person can still choose items into Quarantine. | ADR-0039 point 1, ADR-0051 |
| How is "removed" known? | Every inventory that would list the app was read completely and does not list it (`ApplicationPresent = false` with `Basis::Absence` over complete sources). An app Spotlight finds installed anywhere is not removed. | ADR-0008, ADR-0039, ADR-0059, ADR-0060 rule 7 |
| Is there a new deletion path? | No. Leftovers go through the scan, the policy and the quarantine executor. | ADR-0060 rule 8 |
| May Lumen read files to decide? | No file contents during scanning. Folder names and metadata only. | AGENTS.md, ADR-0008 |

The policy rule exists and is tested (`crates/lumen-policy/src/classify.rs`, `orphan`).
The absence fact exists (`lumen_application::collect::application_evidence`).

## The gap

The orphan rule needs an `OwningApplication` fact on the item. The only producer for
`~/Library` (`lumen-platform-macos::support`) links an item to an app **only when that
app is installed**: it matches the folder name against installed bundle IDs. For a
removed app there is no installed bundle to match, so no owner is recorded, the orphan
rule never fires, and the item is decided as unrecognised (`default.keep-small` under
100 MB, `default.review` above). [Research 36](../research/36-software-cleanup.md)
assumed otherwise.

So 16.9 needs one new piece of evidence: **an owner that is not installed**. The only
place that owner's identity can come from is the item's own name. ADR-0064 says "a
matching file or folder name is never enough to establish ownership". That ADR is about
package files, but its caution applies here, and no ADR says when a name may stand for
an app that is gone. That is the decision this document asks for.

## Measured on the owner's Mac (2026-11-16)

Folder and file names only, in the eight locations `support.rs` reads (`Caches`,
`Containers`, `HTTPStorages`, `WebKit`, `Application Scripts`, `Preferences`,
`Saved Application State`, `Cookies`):

- 114 installed bundle IDs in the inventory's locations; 3,629 names in those folders.
- **215 distinct names look like a bundle ID** (three or more dot-separated labels), do
  not match an installed app exactly or as a prefix, and do not start with
  `com.apple.`.
- Spotlight found **13** of them installed outside the inventory's locations (helpers
  in `/Library/Application Support`: Microsoft AutoUpdate, Google Updater, Adobe
  services, Karabiner, a game launcher). ADR-0059's lookup already handles these.
- Most of the other 202 are **not apps at all**:
  - about 90 Apple app groups (`group.com.apple.*`);
  - team-prefixed app groups (`UBF8T346G9.com.microsoft.oneauth`);
  - SDK and framework names (Segment, SDWebImage, Kingfisher, PLCrashReporter,
    Sparkle);
  - command-line tools (`org.python.python`, `org.swift.swiftpm`, `com.vercel.cli`);
  - JetBrains settings domains.

  Calling these "the app it belongs to seems to be uninstalled" would be false.
- Names that also have a sandbox container in `~/Library/Containers`: **25**.
  Leaving out names that share the vendor part (the first two labels) with an
  installed app leaves **16**. All 16 look like real leftovers (Telegram, Tailscale,
  Parallels, IINA, Blender, The Unarchiver and others). The vendor rule also hid the
  ones that belong to installed software under another ID (Microsoft error reporting
  while Word is installed, an Adobe Finder extension).

A name alone is too weak. A name with a container is strong, because macOS creates
`~/Library/Containers/<bundle ID>` itself, only for signed, sandboxed bundles.

## Decisions (owner, 2026-11-16)

Each question was open until the owner accepted its recommended answer.

### Q1. What establishes that a removed app owned an item?

**Decided:** claim an owner that is not installed only when all of these hold:

1. **The name is shaped like a bundle ID.** It has three or more labels, uses only the
   characters Apple allows (`A–Z a–z 0–9 - .`), and sits directly in a location macOS
   names after the bundle ID. These are the same locations and suffixes `support.rs`
   uses today.
2. **The ID is corroborated by macOS itself.** A `~/Library/Containers/<id>` folder
   exists for the ID or for an ID that extends it (`<id>.<more>`). Apple requires an
   app extension's ID to start with its app's ID, so a leftover extension container
   vouches for its app. Alternatively, Lumen's own uninstall record names the ID (Q3).
3. **No installed app is related to it.** No installed app's ID equals it, extends
   it, or is extended by it, and none shares its vendor part (the first two labels).
   Spotlight finds no app whose ID equals it or extends it (ADR-0059's lookup,
   widened to `<id>.*`). Every claimed owner is looked up, with no cap. An owner that
   could not be looked up is not claimed. Absence counts only when every inventory
   source was read completely (added in review; see ADR-0068 rules 3 and 5).

The fact gets its own source (`macos.support-location.removed-app`), so it can never
be mistaken for a link to an installed app. Everything that fails these checks stays
**unknown ownership**, decided exactly as today.

*Alternative:* any bundle-ID-shaped name. Rejected because it gives 202 false orphans
on the measured Mac.

### Q2. Which names are never claimed for a removed app?

**Decided:**

- `com.apple.*`;
- app groups (`group.*`, and names starting with a 10-character Team ID);
- names that extend an installed app's ID. These are its extensions and helpers.
  Should they instead be linked to the installed app as its data (then `KEEP` under
  `classify.app-state`, which is more cautious than today)? Decided: yes, as a
  separate PR.

### Q3. Use Lumen's own uninstall records?

When Lumen moved an app into Quarantine (ADR-0062), its ledger knows the bundle ID and
name.

**Decided:** yes, as corroboration for Q1.2 and to show the app's name instead of
its ID. While the app can still be restored, its data shows with "{app} is in
Quarantine; restoring it uses this data". It stays `REVIEW`. This is a second step,
after the name-and-container step.

### Q4. Folders that are not named by bundle ID?

This covers `Application Support/<Name>`, `Logs/<Name>`, `Group Containers` and
team-prefixed groups.

**Decided:** not linked to a removed app. The display name is not reliable
(research 36: "never link by display name"), and groups are shared by several apps.
Large ones still appear as unrecognised items over 100 MB (`default.review`). They
could be revisited with Q3's names later.

### Q5. How does Explore show them?

**Decided:**

- A group of its own, "지운 앱이 남긴 데이터" (EN "Left by removed apps"), in the
  results list. Items are grouped per app, with the app's ID (or name from Q3) and a
  total. The total is "at least" when any item was not read completely, never a
  made-up zero (ADR-0008).
- Under "검토 필요" and "전체", like every review item. Rows are selectable into
  Quarantine as chosen review items (ADR-0051).
- One line says why: "앱은 이미 없지만 데이터는 남아 있어요. 필요 없는지 확인한 뒤
  옮겨 주세요."
- No new primary action.

*Alternative:* leave them inside the existing "앱 데이터" and "앱 캐시" groups. This is
simpler, but a person cannot see which app left them.

### Q6. Before/after snapshots (issue #810)?

**Decided:** no separate snapshot store for now. The scan after a removal is the
"after"; the complete inventory and Spotlight are the comparison. ADR-0062's preview
already says the data stays. Startup entries and services of removed software are the
Orphan Hunter's (system.md §12.1) and #807's, not this item's.

## Plan (one PR each, about 400 lines at most)

1. `feat(macos)`: removed-app owner evidence in `support.rs` (Q1, Q2), pure, with
   fixtures from the measurements above; new source name; docs in `evidence-graph.md`.
2. `feat(app)`: wire it into gathering, so Spotlight checks every claimed absent owner.
   Test that the orphan rule fires end to end and an installed or found app never
   fires it. No policy change is expected; if one is needed, it bumps
   `POLICY_VERSION` and cites this document's ADR.
3. `feat(api)` and `feat(web)`: the Explore group (Q5), in 7 locales.
4. Optional: Lumen's uninstall records (Q3); extensions linked to installed apps (Q2).

The evidence rule (Q1–Q2) is [ADR-0068](../decisions/0068-recognise-a-removed-apps-data-only-with-corroboration.md).
