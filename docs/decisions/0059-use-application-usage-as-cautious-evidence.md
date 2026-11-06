# ADR-0059: Use application usage only as cautious, explained evidence

- Status: Proposed
- Date: 2026-11-06
- Refines: [ADR-0014](0014-deterministic-safety-policy-engine.md) (the policy decides),
  [ADR-0039](0039-ask-for-review-only-where-a-person-can-act.md) (orphaned app data),
  [ADR-0053](0053-show-the-evidence-behind-each-candidate.md) (times are not "last
  used")
- Issue: #713

## Context and problem statement

Lumen already knows some things about the application behind an item:
- which application owns it (`OwningApplication`);
- whether the platform's inventory lists that application (`ApplicationPresent`);
- whether a process holds the item open (`InUse`).

It does not know whether the owning application is **running** without touching the
item. It also does not know when the application was **last opened**, whether an
"uninstalled" application is in fact installed on **another volume**, or whether a
build output belongs to a project someone **worked on today**.

These signals are easy to misuse. Access and folder times do not say when something
was last used (ADR-0053). An app that was not opened for a year may hold a person's
only copy of something, and inactivity is never proof of safety. The signals also
describe a person's habits, so they are sensitive.

Which usage signals should Lumen collect, and what may they change?

## Decision drivers

- Safety outranks space: usage may make a verdict **more cautious**, never less.
  Inactivity or age never moves an item towards `QUARANTINE` (#713).
- Unknown is never "not used" (ADR-0008). A signal Lumen could not read is absent,
  not false.
- Every influence is explained in the item's details, with what the signal cannot see.
- Privacy: only for applications that own a candidate, kept on the device, never sent
  to AI review, never in telemetry.
- The policy stays deterministic and IO-free (ADR-0014).

## Considered options

1. **Cautious and display-only signals** (below): the owning app running keeps its
   cache; an app found installed elsewhere is not an orphan; last-opened and project
   activity are shown, never decided on.
2. **Inactivity scoring** ("not opened for 6 months → propose its data"). Rejected:
   it promotes on inactivity, which the issue rules out. Last-opened dates also miss
   most uses: LaunchServices records Finder and Dock launches, not command-line or
   programmatic ones.
3. **Do nothing new.** Rejected: the running-app and installed-elsewhere cases are
   real false proposals and false orphans today.

## Decision outcome

Chosen option: **1**.

1. **New evidence, each with its source, observation time and limits.**

   | Fact | Source | What it cannot see |
   | --- | --- | --- |
   | `ApplicationRunning { app, running }` | The process list, matched by the application's executable path. `true` when a visible process runs it. `false` only when every process was visible (as for in-use, ADR-0030), otherwise absent | Other users' processes; helpers outside the bundle |
   | `ApplicationPresent` found elsewhere | macOS Spotlight lookup by bundle ID across indexed volumes. Only **adds** presence; not finding the app adds nothing | Unindexed and unmounted volumes |
   | `ApplicationLastOpened { app, at }` | macOS `kMDItemLastUsedDate` of the application bundle | Launches outside LaunchServices; Apple reports it sometimes fails to update |
   | `ProjectActivity { at }` | The newest modification time of a project's own marker files (manifest, lockfile), which identify a project root today | Work in files other than the markers |

   Windows and Linux add none of these yet; their items show the signals as unknown.
2. **What they change, only towards caution.**
   - An application cache whose owning application is running is kept, as an item in
     use is: "Kept: Slack is running. Quit it, then scan again to clean this up."
     This uses the same "free when closed" guidance as items held open. The rule is
     `protect.app-running`.
   - Application data whose owner is found installed on another volume is not
     orphaned: `classify.orphan` does not fire, and `classify.app-state` keeps it.
   - `ApplicationLastOpened` and `ProjectActivity` are **shown, never decided on**. No
     rule reads them, in either direction.
3. **Freshness.** `ApplicationRunning` is probed per item at assessment time, like
   in-use, and asked live again by reassessment (ADR-0058, the in-use gap). The other
   facts are read when the scan's evidence is gathered and carry that time.
   Reassessment's owner gap reads them again.
4. **Privacy.** Lumen reads these signals only for the applications that own a
   candidate, never as an inventory of everything the person uses. They stay in
   `index.db` on the device. They are not part of the AI review request (ADR-0020,
   ADR-0050) and not part of telemetry.
5. **Explained.** The item's details show each signal with its limit, for example:
   - "Slack was last opened on Oct 3 (macOS records launches from Finder and the Dock
     only)";
   - "The project changed 2 days ago";
   - "Slack is running".

### Consequences

- Good: fewer false proposals (caches of running apps) and fewer false orphans.
  People see when an app was last opened without Lumen pretending that means unused.
- Bad: caches of apps that are always running, such as a chat app, are kept until the
  person quits the app. The details say so, as they already do for open files.
- Bad: two of the four signals are macOS-only for now.

### Implementation plan (one PR each)

1. Domain and policy: the new facts, `protect.app-running`, and the orphan exception
   for presence found elsewhere. Tests:
   - stale or missing data;
   - partial process visibility (no `false`);
   - an app found elsewhere is not an orphan;
   - inactivity never promotes;
   - explanations for each influence.
2. macOS adapters: the running-application probe, the Spotlight presence lookup, and
   last-opened dates for owning apps only.
3. Gathering and assessment: the new facts flow in, and project activity is read from
   markers.
4. API and web: the signals in the item's details, with their limits.

## More information

- [macOS research, Spotlight](../research/03-macos-platform.md)
- [ADR-0053: why times are not "last used"](0053-show-the-evidence-behind-each-candidate.md)
- Issue #713
