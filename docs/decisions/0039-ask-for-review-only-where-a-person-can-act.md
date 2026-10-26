# ADR-0039: Ask for review only where a person can act

- Status: Accepted
- Date: 2026-10-26
- Amends: [ADR-0014](0014-deterministic-safety-policy-engine.md) (the classification
  default), [ADR-0035](0035-show-tool-commands-never-run-them.md) (how tool commands
  are presented), [ADR-0037](0037-keep-ai-review-on-once-turned-on.md) (what AI review
  is asked about, as a later change)

## Context and problem statement

The owner's whole-disk assessment produced 213 candidates: 0 proposed for quarantine,
48 kept and 165 "Needs review". Only proposed items can be cleaned up, so a review
item asks a person for a judgment that changes nothing Lumen does. 165 of them read as
noise:

| Reason | Items | Size |
| --- | --- | --- |
| An application's own data, settings or documents (`classify.app-state`) | 89 | 18 GB |
| Nothing recognised the item (`default.review`) | 67 | 33 MB in all, none above 16 MB |
| Only the tool's own cleanup command should remove it (`classify.devkb-tool-action`) | 6 | 29 GB |
| It cannot be regenerated (`classify.devkb-not-regenerable`) | 3 | 0.5 GB |

The app-state items are folders such as `~/Library/Application Support/Claude` and
`~/Library/Containers/com.microsoft.Word`, and single preference files. Lumen never
removes an application's state, so reviewing it leads nowhere. The unrecognised items
are `~/Library/HTTPStorages/*`, `~/Library/WebKit/*` and empty `Application Scripts`
folders: even a correct removal of all of them frees 33 MB. The six tool-action items
(the pnpm store, simulator devices, CocoaPods, the Go module cache, npm's cache,
Homebrew) are where the space is, and the person can act on them, by running the
command ADR-0035 shows; but they are mixed in with everything else.

How should Lumen decide what to put in front of a person, without making any decision
less safe?

## Decision drivers

- Safety outranks space: a change may only move a verdict towards `KEEP`, never towards
  `QUARANTINE`.
- A review item should be something the person can act on, and worth their time.
- Unknown is never treated as small or absent (ADR-0008).
- The policy stays IO-free and deterministic (ADR-0014); AI stays evidence only
  (ADR-0020).

## Considered options

1. **Keep what nobody can act on, and show tool commands as their own group:** the
   decisions below.
2. **Hide review items behind a filter in the interface only.** Rejected: the
   policy, the CLI and the counts would still ask for judgments that lead nowhere, and
   each interface would have to repeat the filter.
3. **A fourth verdict ("informational").** Rejected: `KEEP` already means "Lumen leaves
   it alone", and a new verdict would touch the domain, the plan, the executor's
   re-check and every interface for no safety gain.
4. **A size threshold relative to the disk** (for example 0.05 % of its capacity).
   Rejected for now: it needs the volume size in the policy input and makes the same
   item's verdict depend on the machine; a fixed threshold is easier to explain.

## Decision outcome

Chosen option: **1**.

1. **An application's own data is kept.** `classify.app-state` (an owned container,
   application support folder or preference file) proposes `KEEP`, not `REVIEW`:
   "Kept: it holds an application's own data, settings or documents; Lumen leaves it
   alone." **Orphaned** application data stays `REVIEW` (`classify.orphan`): when every
   inventory says the owning application is gone, the person can decide about the
   leftovers. The app-state rule therefore does not fire when the orphan rule does. As
   with every classification, the most cautious proposal wins, so an app data location
   that a knowledge-base entry also matches is kept; its tool command still shows in
   its details.
2. **An unrecognised item is reviewed only when it is large enough.** When no
   classification rule recognises an item, the default is `REVIEW` (`default.review`)
   only if its size is at least **100 MB (100,000,000 bytes)**; below that it is
   `KEEP` (`default.keep-small`): "Kept: too small to be worth reviewing."
   - The size is the larger of the scan's allocated size and the space removing the
     item would free now (both from `Observed.reclaim` in the policy input), so an item
     is small only when every measure says so. When `reclaimable_now` is a lower bound,
     the allocated size still counts.
   - An item without a scan observation has no known size and stays `REVIEW`; an item
     that was not read completely is already kept by a hard protection.
   - 100 MB is about 0.04 % of the smallest current Mac SSD (256 GB). On the owner's
     disk every unrecognised item was at most 16 MB and all 67 together 33 MB, while
     each item the person can act on (the tool-action caches) was at least 47 MB and
     together 29 GB. Sizes are decimal, as Lumen shows them.
   - The threshold applies only to the default. It never changes a verdict a rule gave,
     and `KEEP` is the most cautious verdict.
3. **Items with their tool's own command are their own group.** The policy keeps them at
   `REVIEW` (ADR-0035 is unchanged: Lumen shows the command and never runs it). The
   interface shows `REVIEW` items that carry a tool command under "Clean up with its
   own command" (명령으로 정리), with the command, apart from "Needs review"; filter
   counts and the Overview count them separately. This also covers an irreplaceable
   item whose vendor documents a command that frees space inside it (ADR-0035).
4. **Next, AI review will describe what remains (the owner's choice).** Optional AI
   review will be asked to describe what each remaining review item is, from metadata
   only, opt-in as ADR-0037 sets out, displayed as labelled AI text, and never changing
   a verdict (ADR-0020). ADR-0037 asks the judge only about proposed quarantines; this
   extension is a later change with its own review, not part of this decision.

The policy version moves to 0.11.0.

### Amendment: items that would free nothing are kept (2026-10-26)

An item whose removal would free no space now (`eligible.frees-nothing`: an empty
folder, or space shared with hard links, clones or snapshots) was capped at `REVIEW`.
After the per-app cache entries
([research 16](../research/16-app-caches-in-app-data-folders.md)), six empty Chromium
`Cache` folders on the owner's Mac showed as "Needs review": there is nothing for a
person to decide, since removing them changes nothing. Under the principle above, the
rule now caps at `KEEP`: "Kept: removing it would free no space." Like every change in
this decision, it only moves verdicts from `REVIEW` to `KEEP`, and an item that frees
nothing was never a quarantine candidate. The policy version moves to 0.12.0.

### Consequences

- Good: on the owner's disk, "Needs review" drops from 165 items to the 3 that cannot be
  regenerated, plus 6 under "Clean up with its own command" (29 GB) with the commands to
  run.
- Good: every verdict change is `REVIEW` → `KEEP`; nothing new can become a quarantine
  candidate, and the Jev stage is unchanged.
- Bad: a person no longer sees app data or small unknown items as "Needs review". They
  are still listed under "Kept" with their reasons.
- Bad: many small unrecognised items in one place add up without being reviewed. Lumen
  decides per item; a grouping rule would need its own decision.

## More information

- [ADR-0008](0008-capability-model-and-coverage.md): unknown is never empty.
- [ADR-0014](0014-deterministic-safety-policy-engine.md): the policy's stages.
- [ADR-0020](0020-jev-evidence-only-judge.md), [ADR-0037](0037-keep-ai-review-on-once-turned-on.md):
  AI review stays evidence only and opt-in.
- [ADR-0035](0035-show-tool-commands-never-run-them.md): tool commands are shown, never
  run.
