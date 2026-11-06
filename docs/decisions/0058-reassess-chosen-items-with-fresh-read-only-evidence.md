# ADR-0058: Reassess chosen items with fresh read-only evidence

- Status: Proposed
- Date: 2026-11-06
- Refines: [ADR-0014](0014-deterministic-safety-policy-engine.md) (the policy decides),
  [ADR-0030](0030-partial-in-use-visibility.md) (in-use answers),
  [ADR-0039](0039-ask-for-review-only-where-a-person-can-act.md) (what is worth a
  review)
- Issue: #708

## Context and problem statement

An assessment decides every candidate once, from the evidence gathered when the scan
finished and the facts probed at that moment. Some `KEEP` and `REVIEW` results come
from something Lumen did not know at that moment, not from what the item is. These
results are not settled answers:

| Fired rule or review reason | What was unknown |
| --- | --- |
| `protect.in-use`, `protect.in-use-partial`, `protect.in-use-unknown` (`active_process`) | Whether a process uses it now |
| `protect.incomplete-scan`, `protect.not-observed` | Part of it, or all of it, was not read |
| `stale_evidence` | An observation is too old |
| `default.review` (`unknown_owner`), `classify.orphan` | Who owns it, and whether that application is installed |

The person can often resolve these unknowns: closing the app that held a cache,
granting Full Disk Access, or installing the owning app. Today the only way to see the
effect is a new scan and a new assessment of the whole disk.

Other results are decisions, not unknowns. Examples are protected and sensitive
locations, cloud data, application state, items only a tool may clean, irreplaceable
items, the AI's caution, and items that free nothing. Checking them again cannot
change what they are.

How can a person ask Lumen to check an item again, safely, and see why its verdict
changed or stayed?

## Decision drivers

- Safety of user data outranks reclaimed space. Unknown is never treated as false or
  absent (ADR-0008).
- The deterministic policy decides. Reassessment adds evidence, never rules, and AI
  never moves a verdict toward `QUARANTINE` (ADR-0020).
- Protected rules always win.
- Reassessment only reads; it never changes the filesystem.
- A changed verdict says what changed it, and replaying the same evidence gives the
  same verdict.
- The work is bounded, cancellable and time-limited.

## Considered options

1. **Explicit reassessment of chosen items.** A fixed table maps each unknown to the
   read-only checks allowed to resolve it, and the unchanged policy decides again.
2. **Automatic background reassessment.** Rejected: it runs work nobody asked for and
   changes results while a person reads them.
3. **Let the AI judge resolve unknowns.** Rejected by ADR-0020: AI output is evidence
   that can only make a decision more cautious.
4. **Lower the bar for `REVIEW`** (for example, treat old items as unused). Rejected:
   inactivity and age are not proof of safety (#713).

## Decision outcome

Chosen option: **1**.

1. **Only on request, and bounded.** A person reassesses items from the latest
   assessment: one item, a selection, or "every item kept or reviewed because of an
   unknown". Each run covers at most 500 items. Every check has a 30-second limit, as
   tool inspection does, and the run can be cancelled. It never rescans the whole
   disk.
2. **A fixed table of gaps and checks.** Only these read-only checks run, and only for
   these gaps:

   | Gap | Check |
   | --- | --- |
   | In use, partly visible, or unknown | The live in-use probe, item by item (not the assessment snapshot) |
   | Not read completely, or not observed | List the item's subtree again with the platform enumerator |
   | Stale observation | Observe the item and its subtree again |
   | Unknown owner, possible orphan | Read the application inventory and ownership again (the existing producers) |

   Every other fired rule is a decision, and reassessment leaves it as it is. A new
   kind of check, such as package receipts, needs its own ADR.
3. **The same policy decides.** Fresh facts become new evidence records, with their
   own observation time and source. Old records are kept, because evidence is
   content-addressed. The item is decided by `lumen_policy::evaluate` exactly as at
   assessment, so protection precedence is unchanged.
   - A check that fails, is denied or times out adds no fact, or an explicit unknown.
     It never adds a negative such as "not in use".
   - Jev is not consulted on reassessment. A judge result from the assessment stays as
     it was, and it can only demote.
4. **Identity first.** Each item is reassessed only if its path still names the
   scanned object (same identity). Otherwise the result is "changed since the scan;
   scan it again", never a verdict.
5. **Fresh results expire.** A reassessed item carries the time of its checks.
   - In-use answers are never reused: moving still asks live, item by item (ADR-0030).
   - Listings and inventory answers stay valid until the item's identity or
     modification time changes, or for 24 hours.
   - A plan made from a reassessed item binds its new evidence hash, so confirming it
     re-checks exactly what was shown (roadmap 10.14).
6. **Every change is explained.** For each item, reassessment records:
   - the verdict and fired rules before and after;
   - the facts each check added, or why a check added none.

   The interface shows it in the item's details, for example: "Checked again: was Kept
   (a process was using it), now Proposed (no process Lumen can see uses it)".
   Unchanged items say what was checked and that nothing changed.
7. **Replayable.** Given the recorded evidence, the policy yields the recorded verdict.
   A test replays the reassessment records and compares the results.

### Consequences

- Good: a person who closes an app, grants access or installs the owning app can see
  the effect on those items in seconds, and the reason is shown.
- Good: no new data source, no new rule and no AI in the loop. The safety argument is
  the existing policy's.
- Bad: one more state an item can be in ("reassessed at …"), which the plan, the
  interface and the stored assessment must carry.
- Bad: unknown owners stay unknown unless the existing producers can say more. Deeper
  ownership signals are left to #713, under its own decision.

### Implementation plan (one PR each)

1. `lumen-application`: the gap table (fired rules and reasons to checks), a
   `reassess` use case over existing ports with cancellation and time limits, and the
   before/after record. Tests: stale evidence, permission denied, cancellation, an
   item changed between scan and reassessment, protection precedence, and
   deterministic replay.
2. `lumen-app` and storage: run the checks with the platform probes, enumerators and
   producers; store reassessment records and expiry next to the assessment.
3. CLI: `lumen reassess [--all-unknowns | <path>...] [--json]`.
4. API, desktop and web: a `reassess` command that takes item IDs, a "Check again"
   action, and the before/after explanation in the item's details.

## More information

- Issue #708, and #713 for usage evidence.
- [Evidence graph and bundles](../architecture/evidence-graph.md)
