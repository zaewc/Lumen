# ADR-0069: Show storage in one place, grouped by what owns it

- Status: Accepted (owner decision on direction and name, 2026-11-18; details by the
  lead, open to the owner's revision)
- Date: 2026-11-18
- Amends: [ADR-0051](0051-quarantine-review-items-the-user-chooses.md) (a person may
  also leave proposed items out of a plan),
  [ADR-0052](0052-restrained-navigation-without-glass.md) (the navigation's places),
  [ADR-0062](0062-uninstall-macos-apps-into-quarantine.md) rule 4 (an app's preview
  may be confirmed together with a plan)
- Design: [storage-in-one-place.md](../design/storage-in-one-place.md)

## Context and problem statement

Storage, Explore and Apps each answer part of "what is using my disk, and what can I
safely get back?", organised by Lumen's mechanisms: the policy's plan, a chosen review
item, an app move. One app's space is split across pages (the Xcode app on Apps, its
DerivedData and simulators in Explore). Option A of
[apps-in-explore.md](../design/apps-in-explore.md) (0.13.0) put the largest apps in
Explore's results but kept both pages. The owner asked for one consistent experience,
even if Explore is rebuilt.

## Decision drivers

- One place to find space and act on it, in terms a person recognises.
- Safety stays per item: every verdict, gate and confirmation is unchanged or stricter.
- One primary action per page ([ui-principles.md](../design/ui-principles.md)), whose
  label says what it moves.
- No feature lost: search, facts, AI notes, tool storage, identical copies.

## Considered options

1. **One page, owners with parts, one selection.** Every item, app and package is shown
   under what owns it, with a checkbox where a person may move it; one button moves the
   selection after one host confirmation.
2. **A segmented "items | apps" view inside Explore.** Rejected: apps hide behind a
   control, and the primary button either moves things not on screen or changes with
   the view.
3. **Keep Option A and fold the Apps list into its section.** Rejected: still two
   models (kinds and apps), Overview stays a third place, and an app's data stays apart
   from the app.

## Decision outcome

Chosen option: **1**.

1. **Places.** The navigation is 저장 공간 · 격리함 · 설정. Overview, Explore and Apps
   become 저장 공간; their routes lead there.
2. **Owners.** Items are grouped by the owner the host reports (`group.source`, the
   software list, the package manager), else by kind. An owner shows its total and what
   can be cleaned up; it never carries one verdict, and nothing is decided per owner.
3. **Selection.**
   - Proposed items start checked and may be unchecked. A plan of "the proposals minus
     those left out" is never larger than today's plan.
   - Choosable review items (ADR-0051) start unchecked.
   - A user-owned macOS app (ADR-0062) starts unchecked; checking it makes a fresh
     preview, and a refusal unchecks it with its reason.
   - Tool-command items (ADR-0049), kept items and protected software have no
     checkbox. Nothing is selected in bulk.
4. **Confirmation.** One host confirmation (FS-1) lists the proposals, the chosen items
   and the chosen apps apart, and is bound to the plan's hash and each app's preview
   hash. Each item and app is checked again by its own gate just before it moves; a
   refused app does not stop the others, and the report says why. AI never selects,
   and the cleanup policy is still never asked about an app (ADR-0062 rule 3).
5. **Before a scan,** installed apps are listed with sizes from the latest completed
   scan, labelled as such, or without sizes; unknown is never zero (ADR-0008).

### Consequences

- Good: one page answers where the space is, and an app's data sits under the app.
- Good: the button says exactly what moves; leaving proposals out is new control that
  can only make a plan smaller.
- Good: no verdict, rule or gate becomes less cautious.
- Bad: the host's plan and confirmation change (leaving items out; a plan with app
  previews), and the confirmation window shows more kinds of items.
- Bad: the web UI is largely rebuilt over about nine pull requests, and the glossary
  retires 탐색.

## More information

- Owner requests: 2026-11-08 (one flow), 2026-11-18 (direction and the name 저장 공간).
- Prior patterns: macOS Storage settings (by category), DaisyDisk's collector (gather,
  then delete once), app uninstallers that show an app with its leftovers.
