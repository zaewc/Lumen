# ADR-0051: Quarantine review items the user chooses, and show large unrecognised folders

- Status: Accepted (owner request to recover as much space as the reference cleaners,
  2026-10-31; design by the lead, open to the owner's revision)
- Date: 2026-10-31
- Amends: [ADR-0014](0014-deterministic-safety-policy-engine.md) (only proposed items
  enter a plan), [ADR-0015](0015-quarantine-and-reversible-cleanup.md) (automatic
  removal after retention), [ADR-0039](0039-ask-for-review-only-where-a-person-can-act.md)
  (what a person can do with a review item)

## Context and problem statement

On the owner's Mac about 223 of 307 GB under the home folder is never assessed, and the
items that are left for review cannot be acted on: a plan holds only items the policy
proposes (`lumen-app` `plan`), so "Needs review" asks for a judgment that changes
nothing ([research 26](../research/26-reference-cleaners-and-recovery-gap.md)).
CleanMyMac, DaisyDisk and PureMac let a person pick large or unknown items and remove
them, permanently. How can Lumen let a person act on what it cannot decide, without
the policy or a model ever becoming less cautious?

## Decision drivers

- Safety of user data outranks space; when uncertain, Lumen keeps or asks (AGENTS.md).
- Nothing is removed on Lumen's own initiative: a person's explicit choice is not an
  automatic removal, but it must stay reversible.
- AI is evidence only; it never selects what is moved (ADR-0020).
- Unknown is never treated as small or absent (ADR-0008): showing large unrecognised
  folders makes the unknown visible instead of hiding it.

## Considered options

1. **A person may add review items to a plan; such items are quarantined, never
   removed automatically; large unrecognised folders become review items.**
2. **Lower the policy's bar so more items are proposed.** Rejected: it trades safety
   for space, the reverse of the policy's purpose.
3. **Let the AI judge promote review items it rates removable.** Rejected: ADR-0020,
   and the judge fails its gates by a wide margin ([evaluation](../ai/evaluation.md)).
4. **A free-form file browser with delete.** Rejected: it bypasses evidence, re-checks
   and quarantine.

## Decision outcome

Chosen option: **1**.

- **Which items a person may choose:** `REVIEW` items without a tool command and
  without `classify.devkb-not-regenerable` (for those, the tool's own command or
  nothing). Never `KEEP`, so every hard protection, in-use answer, cloud placeholder,
  incomplete scan and protected location stays out of reach. There is no "select all"
  for review items, and nothing is preselected, by the policy or by a model.
- **The plan records who chose each item:** `PlannedItem` carries whether the policy
  proposed it or the person chose it, and the plan's hash covers that. The CLI takes
  chosen items explicitly (`lumen plan --choose <path>`, repeatable); the desktop app
  sends the chosen items' paths with the plan request. The host plans only paths that
  the latest assessment left for review and that are eligible as above.
- **Confirmation re-decides, as before:** just before the move, a chosen item must
  still be `REVIEW` (or `QUARANTINE`) with the same rules; `KEEP`, changed evidence or a
  changed identity skips it. The executor's checks are unchanged: identity by handle,
  in-use, no-replace rename on the same volume (ADR-0016).
- **No automatic removal:** an item the person chose is never removed when retention
  ends. It stays in quarantine until the person restores it or removes it for good,
  and the quarantine list says so. Policy-proposed items keep ADR-0015's retention.
- **Large unrecognised folders:** after a scan, directories of at least 1 GiB that no
  other evidence names, at the level where their size is spread out (no single child
  holds 80 % or more), become candidates, at most 30 per scan, none inside another
  candidate. They carry no classification, so the policy's own rules decide them: in
  practice `default.review`, or `KEEP` for a protected location or anything in use.
  Contents are never opened.
- **AI's part:** AI review describes the largest review items (ADR-0041), so the person
  sees "likely a cache" or "likely your documents" next to the choice. The note never
  preselects, sorts into a plan or changes a verdict.
- **Interface:** review items have a per-item checkbox, their size, the AI note and the
  reasons. The confirmation lists chosen items apart from proposed ones and says that
  Lumen did not propose them and keeps them until the person removes them.

### Consequences

- Good: everything large becomes visible, and a person can recover space Lumen cannot
  vouch for, reversibly.
- Good: no verdict, rule or model output becomes less cautious.
- Bad: a person can quarantine an application's unknown data; the app may then misbehave
  until it is restored. The confirmation says so, and restore stays one click away.
- Bad: chosen items occupy space in quarantine until the person removes them; space is
  freed only then.
- Bad: plans, the plan hash, confirmation, the ledger and the interface all change.

## More information

- [Research 26](../research/26-reference-cleaners-and-recovery-gap.md): measurements and
  the reference survey.
- Roadmap Phase 15 lists the pull requests.
