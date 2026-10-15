# ADR-0034: Confirm cleanup plans from the command line by plan hash

- Status: Accepted
- Date: 2026-10-15
- Refines: [ADR-0015](0015-quarantine-and-reversible-cleanup.md) (confirmation),
  [system.md §11](../architecture/system.md)

## Context and problem statement

Phase 10 runs the whole pipeline (scan → assess → plan → confirm → quarantine) from
the `lumen` command line before any graphical surface exists (Phase 11). Until now
the CLI has been read-only, and a test asserts it has no destructive command.

The domain already defines what a confirmation is: a `Confirmation` names the
`PlanHash` of the exact plan the user saw, so the plan that executes is the plan that
was shown (system.md §11). It has no command-line surface, no plan type and no
plan-hash computation yet.

How does a command-line user confirm a plan so that only that plan, unchanged and
still safe, is executed?

## Decision drivers

- Nothing moves without an explicit, plan-specific confirmation; there is no
  "yes to everything" switch.
- What executes is exactly what was shown; any change invalidates the confirmation.
- The policy is re-run on fresh evidence at execution; the confirmation never
  overrides a newer, more cautious decision.
- Scriptable for developers, without pretending the CLI can prove a person was
  present.

## Considered options

1. An interactive `[y/N]` prompt after `lumen plan`.
2. **Two commands bound by the plan hash: `lumen plan` shows the plan and its hash;
   `lumen quarantine --plan <ID> --confirm <HASH>` executes it only if the hash
   matches.**
3. A `--yes` flag on a single `lumen clean` command.

## Decision outcome

Chosen option: **2**.

### The plan

- `lumen plan` builds a `CleanupPlan` from the latest assessment: by default every
  candidate whose verdict is `QUARANTINE`, optionally narrowed to listed item
  numbers. `REVIEW` and `KEEP` items are never in a plan.
- The plan's canonical form is its ID, creation time and its items in path order,
  each with the subject identity, raw path bytes, policy version, evidence hash and
  reclaim estimate. `PlanHash` is BLAKE3 over `"lumen.plan/1\0"` and the canonical
  JSON. A golden test pins the bytes.
- The plan is stored with its items and decisions (roadmap 10.11) and printed: each
  item with its reasons and size, the total, and the full plan hash.

### The confirmation

- `lumen quarantine --plan <PLAN_ID> --confirm <PLAN_HASH>` requires the full hash;
  a prefix, a different hash or a missing argument refuses with nothing moved.
- The confirmation is recorded with `ConfirmationSurface::CommandLine` (new) and the
  time it was given; the plan becomes `Confirmed`, then `Executing`.
- A plan older than 24 hours is refused: the user plans again and sees fresh results.
- The CLI refuses to quarantine when running as root (macOS, Linux) or elevated
  (Windows): ownership and in-use visibility differ, and a user's cleanup must not
  run with more privilege than the user (ADR-0030).

### At execution

For each item, in plan order:

1. The item is assessed again on fresh evidence (roadmap 10.8b). If the verdict is
   no longer `QUARANTINE`, or it is `QUARANTINE` for other reasons (a different set
   of fired rules) than the stored plan decision, the item is skipped and reported
   as `REVIEW`; nothing about it moves. (Amended 2026-10-15: the first version
   compared evidence hashes, which can never match, because facts probed at
   confirmation carry their observation time and the partial in-use fact counts
   processes that come and go. The plan keeps each decision's evidence hash for
   the audit trail.)
2. Otherwise it is prepared by the executor, its manifest built, and it is
   quarantined through `quarantine_item` (in-use re-probe, journaled intent,
   no-replace move, verification).
3. Each outcome is printed; the plan ends `Completed` or `PartiallyCompleted`.

### Consequences

- Good: a confirmation authorizes one exact plan; a changed plan, a changed item or a
  newer cautious decision never executes under an old confirmation.
- Good: no switch skips confirmation; a typo in the hash refuses.
- Bad: the hash proves the plan is unchanged, not that a person read it. A script can
  copy the hash from `lumen plan` into `lumen quarantine`. That is accepted for a
  developer tool: everything is reversible (quarantine, not deletion), every item is
  re-decided at execution, and consumer surfaces (Phase 11) show the plan and take
  the confirmation themselves.
- Neutral: the CLI test that forbids destructive commands is replaced, in the PR that
  adds `lumen quarantine`, by tests that it refuses without a matching hash, for a
  stale plan, and as root.

Option 1 hides the plan identity behind a keystroke and cannot bind the answer to
what was shown after the terminal scrolled. Option 3 is exactly the "yes to
everything" switch the drivers exclude.

## More information

- Roadmap 10.9–10.14; ADR-0013 (evidence hashes), ADR-0015 (quarantine lifecycle),
  ADR-0020 (Jev never selects actions), ADR-0030 (partial in-use visibility).
