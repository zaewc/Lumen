# ADR-0068: Recognise a removed app's data only when macOS or Lumen corroborates the ID

- Status: Accepted (owner decision, 2026-11-16)
- Date: 2026-11-16
- Refines: [ADR-0039](0039-ask-for-review-only-where-a-person-can-act.md) (which data
  counts as orphaned), [ADR-0062](0062-uninstall-macos-apps-into-quarantine.md) rule 4
  (a removed app's data is decided in Explore)
- Issue: #810 (roadmap 16.9). Design and measurements:
  [leftovers-after-removal.md](../design/leftovers-after-removal.md)

## Context and problem statement

A removed macOS app leaves its data in `~/Library`. ADR-0039 sends orphaned
application data to review (`classify.orphan`), and ADR-0062 promises that this data
is decided in Explore after the next scan. The orphan rule needs to know which app
owned an item. Today Lumen only links an item to an app that is **installed**, by
matching the item's name against installed bundle IDs. For a removed app, the item's
own name is the only clue to who owned it.

A name alone is weak evidence ([ADR-0064](0064-read-package-relations-from-each-managers-own-records.md):
"a matching file or folder name is never enough to establish ownership"). The
owner's Mac shows why (2026-11-16):

- 215 bundle-ID-shaped names in `~/Library` match no installed app and are not
  `com.apple.*`;
- most of them are not apps at all: Apple app groups, team-prefixed groups, SDK names
  (Segment, Kingfisher, Sparkle), command-line tools and settings domains;
- Spotlight found 13 installed outside the inventory's folders.

When may a name stand for an app that is gone?

## Decision drivers

- Never call something "left by a removed app" without evidence that an app with that
  ID existed and is gone (ADR-0008: unknown is not absent).
- The verdict stays `REVIEW`; this decision only chooses which items the orphan rule
  sees. Fewer, true orphans are better than many false ones.
- Names and metadata only: no file contents, not even a container's metadata
  property list.

## Considered options

1. **Any bundle-ID-shaped name** whose app is absent. Rejected: 202 false orphans on
   the measured Mac.
2. **Read the container's metadata property list** for its owner. Rejected: it reads
   a file's contents during scanning.
3. **A bundle-ID-shaped name corroborated by macOS itself or by Lumen's own records,
   with nothing installed related to it** (below). On the measured Mac: 16 IDs, all of
   them real leftovers.

## Decision outcome

Chosen option: **3**.

1. **An owner that is not installed is claimed only when all of these hold:**
   1. **The name is shaped like a bundle ID:** three or more dot-separated labels of
      `A–Z a–z 0–9 -`. Apple allows only these characters, and compares bundle IDs
      without regard to case. The name sits directly in a `~/Library` location that
      macOS names after the bundle ID, with that location's suffix. These are the
      locations and suffixes `lumen-platform-macos::support` already uses for
      installed apps.
   2. **The ID is corroborated, in one of two ways:**
      - macOS made a sandbox container `~/Library/Containers/<id>` for it, or for an
        ID that extends it (`<id>.<more>`). macOS creates the container itself, for a
        signed, sandboxed bundle, on its first launch. The App Store requires an
        embedded bundle's ID to start with its app's ID, so an extension's container
        vouches for its app.
      - Lumen's own uninstall record (ADR-0062) names the ID. This is a later step.
   3. **Nothing installed is related to it:**
      - no installed app's bundle ID equals it, extends it, or is extended by it;
      - no installed app shares its vendor part (its first two labels);
      - Spotlight finds no bundle of the same vendor: none whose bundle ID is the
        vendor part, its first label alone, or starts with the vendor part
        (`<vendor>`, `<first label>`, `<vendor>.*`, compared without case;
        ADR-0059's lookup, widened). This covers, outside the inventory's folders,
        the same relations as above: the ID itself, its extensions, its parents
        (a one-label one too) and its siblings, apps and helpers alike. Every claimed owner is
        looked up, with no cap, and an owner that could not be looked up is not
        claimed.

      Spotlight can only veto a claim. An empty result is not evidence of absence
      (it does not see unindexed or unmounted volumes); absence comes from the
      complete inventories alone (rule 3). An app kept only on such a volume can
      therefore be claimed, and its data is then reviewed, never proposed.
2. **Never claimed:**
   - `com.apple.*`;
   - app groups (`group.*`, and names starting with a ten-character Team ID, Apple's
     macOS group convention `<team ID>.<group name>`);
   - `Application Support/<Name>`, `Logs` and `Group Containers`, which are not named
     after a bundle ID or are shared by several apps.
3. **Its own source.** The fact is `OwningApplication` with the source
   `macos.support-location.removed-app`, recorded with an `AppDataLocation` as for an
   installed app. It can never be mistaken for a link to an installed app. Absence is
   still established separately (ADR-0008, ADR-0039): only when **every** inventory
   source was read completely. One complete source is not enough while another,
   which could list the app, is partial. The item stays `REVIEW` (`classify.orphan`);
   a removed app's cache is `REVIEW` too, because the most cautious proposal wins.
5. **A bundle without an identifier does not make the bundle inventory partial.** If
   a bundle's information property list was read and has no `CFBundleIdentifier`,
   the bundle has no ID, so it cannot be the owner of anything named by ID (on the
   owner's Mac: a game shortcut). A bundle whose property list could not be read
   still makes the inventory partial.
4. **Everything else is unknown ownership**, decided exactly as before (ADR-0039's
   default rules).

The policy is unchanged: `classify.orphan` already decides these items.

### Consequences

- Good: Explore can show a removed app's data under its own heading. The claim that
  "the app is gone" is backed by macOS's own container and a complete inventory, not
  by a name alone.
- Good: SDK, tool and system names never become false orphans.
- Bad: some real leftovers are missed. These include an app that was never sandboxed
  and has no extension, and an app whose vendor still has another app installed (for
  example Google Drive's leftovers while Chrome is installed). They stay unknown
  ownership. Large ones still show as unrecognised items (`default.review`, 100 MB or
  more).
- Neutral: Lumen's uninstall records can add apps without containers later, under
  rule 1.2.

## More information

- Apple, [CFBundleIdentifier](https://developer.apple.com/documentation/bundleresources/information-property-list/cfbundleidentifier):
  "must contain only alphanumeric characters (A–Z, a–z, and 0–9), hyphens (-), and
  periods (.) … Bundle IDs are case-insensitive."
- Apple, [Accessing files from the macOS App Sandbox](https://developer.apple.com/documentation/security/accessing-files-from-the-macos-app-sandbox):
  "When your sandboxed app launches for the first time, macOS creates a sandbox
  container on the file system (in `~/Library/Containers`) and associates it with your
  app"; from macOS 14 the association uses the app's code signature.
- Apple, [What is a bundle ID?](https://help.apple.com/xcode/mac/current/en.lproj/deve70ea917b.html):
  the App Store requires every embedded executable bundle's ID to be prefixed with its
  parent's bundle ID.
- Apple, [Configuring app groups](https://developer.apple.com/documentation/xcode/configuring-app-groups):
  macOS app groups may be named `<Developer team ID>.<group name>`.
