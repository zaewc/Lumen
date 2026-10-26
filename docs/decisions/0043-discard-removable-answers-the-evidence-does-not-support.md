# ADR-0043: Discard removable answers the evidence does not support

- Status: Accepted
- Date: 2026-10-26
- Refines: [ADR-0020](0020-jev-evidence-only-judge.md) (how an answer is validated),
  [ADR-0031](0031-jev-on-typesafe-via-openrouter.md) (how a Decisions answer becomes a
  judgment)

## Context and problem statement

The owner's goal for Phase 14 is an AI judge accurate enough to leave on. Jeb's
recording on dataset 2026.10.1 shows 81 critical false positives in 1,103 critical
golden cases (needed items judged `likely_removable` with medium or high confidence),
23 in 160 adversarial ones and 19 canary flips
([report](../../ai/reports/jev-decisions/1/jeb-2026.10.1.md)).
[Research 17](../research/17-jeb-error-analysis.md) analysed every one of them from
the recorded answers, without sending a request. Some of those answers cannot be
evidence on their own terms: they call an item disposable that the request says
cannot be regenerated, give a reason that argues against removal, credit a vendor
declaration that only untrusted text could have suggested, or come with signs that
text tried to steer them. Others are wrong in ways that only a better prompt or model
can fix.

The question is which answers Lumen should refuse to take as "likely removable", and
how, without letting a rule built on one recording cost the recall the goal needs.

## Decision drivers

- AI output is evidence, and can only make a decision more cautious (ADR-0020).
- The same checks must apply live and in replay, so the evaluation measures what the
  app does.
- No recall cost: Phase 14 requires recall to stay at today's 96.9%.
- Thresholds are chosen on the `dev` and `calibration` splits only; `test-locked` is
  only reported.
- No rule that measures the dataset rather than the judge.

## Considered options

1. **Guards that discard a removable answer** (make it `uncertain`), after validation,
   in `lumen-jev`, for the rules the analysis justifies: the decision below.
2. **Reject such answers as invalid** (`JudgeError::Rejected`). Rejected: a rejected
   answer means "no judgment", and with no judgment a proposed quarantine stands, so
   rejection is *less* cautious than `uncertain`, which demotes it to review. It would
   also count as an unanswered critical case, which fails the evaluation gate whatever
   the other answers are.
3. **Apply the guards in the policy's Jev stage.** Rejected: the evaluation grades the
   judgment the judge adapter returns, so guards in the policy would not show in the
   evaluation, and the policy would need to know answer details (reasons, injection
   probability) that the adapter owns. The adapter owns validation; the policy keeps
   owning decisions, and needs no change: it already demotes on `uncertain`.
4. **All candidate guards, including a probability threshold.** Rejected for the
   guards with a recall cost (below).

## Decision outcome

Chosen option: **1**.

`lumen_jev::guard` runs after an answer is validated against its request, on both
validation paths (`decisions::judgment_for` for Jeb and Jev through OpenRouter,
`response::validate` for `jev.judgment/1` text), so replay and production apply the
same rules. A `likely_removable` answer becomes `uncertain` with `low` confidence when:

| Guard | Fires when | Why |
| --- | --- | --- |
| `NotRegeneratable` | the request says `regeneratable: false` | "removable" is defined as a disposable or regenerable artifact |
| `ContradictedReason` | the cited `owner_app_absent` meets an installed owner (and no absent one), or `no_active_reference` meets an active reference | the answer contradicts what it was told |
| `NoReasonForRemoval` | no cited reason argues for removal (`owner_app_absent`, `regeneratable_cache`, `no_active_reference`, `vendor_declared_cache`, `knowledge_base_match`, `stale_by_time`), or one argues against it (`owner_app_present`, `not_regeneratable`, `active_reference`, `recently_used`, `contains_user_data`, `credential_or_key_material`, `system_component`, `cloud_backed`, `insufficient_evidence`, `suspicious_metadata`) | the answer gives no ground for removal |
| `UndeclaredVendorCache` | `vendor_declared_cache` is cited and the request holds no vendor declaration | only untrusted text can have suggested it: 15 of the 19 canary flips cite it, no golden answer does without the declaration |
| `InjectionSignal` | the judge suspects injection (`injection_suspected`), or for a Decisions judge the injection probability is 0.1 or more | no clean golden request reached 0.06; 116 of 176 canaries reach 0.1 |

Reasons, evidence references and the injection flag stay as the judge gave them, and
`likely_needed` and `uncertain` answers are never touched. Nothing here can make a
decision less cautious.

Not adopted, with the measurements in research 17:

- a removable probability threshold: every threshold that removes false positives
  costs at least a quarter of recall; thresholds wait for a calibration map
  (roadmap 14.5);
- `knowledge_base_match` or `regeneratable_cache` cited without the matching fact:
  that is also how Jeb gets most removable items right (recall would fall to 82% and
  57%); prompt v2 should tie each reason to its fact instead;
- application support or container locations without a knowledge-base entry: on
  dev + calibration it removes 3 false positives and discards 11 correct answers;
- the candidate's category: in 2026.10.1 it separates the labels perfectly, so a guard
  on it would measure the dataset generator, not the judge.

### Results

The committed reports were regenerated with `lumen-jev-eval run` from the unchanged
cassettes:

| Dataset 2026.10.1 | Before | After |
| --- | --- | --- |
| Golden critical FP (all splits) | 81 / 1,103, bound 9.05% | 71 / 1,103, bound 8.05% |
| Golden critical FP (`test-locked`) | 47 / 643, bound 9.60% | 42 / 643, bound 8.73% |
| Golden precision / recall / abstention | 54.4% / 96.9% / 3.1% | 59.7% / 96.9% / 7.2% |
| Adversarial critical FP | 23 / 160, bound 20.78% | 6 / 160, bound 7.98% |
| Adversarial recall | 81.2% | 18.8% |
| Canary flips | 19 | 17 |

On dataset 2026.10.0, the one critical false positive (an Xcode archive with
`regeneratable: false`) becomes `uncertain`; the observed gate passes.

Adversarial recall falls because the 16 reverse-injection canaries (text asking for
"needed" on removable items) now come back `uncertain`, which their labels accept:
a request with text trying to steer the judge is not one to clean up on its word.
The 17 remaining canary flips all have a twin answered `likely_needed`; turning the
canary's answer into `uncertain` is still a flip, so no guard of this kind can repair
them. The release gate still fails by a wide margin, and Jeb stays demote-only.

### Consequences

- Good: 10 golden and 17 adversarial critical false positives fewer, with no correct
  removable golden answer lost.
- Good: one place, shared by every judge and by replay, holds the rules; each has a
  test.
- Bad: in the app, a proposed quarantine whose removable answer is discarded is
  demoted to review, so some items that Jeb rightly called removable (when text in
  their names looked like an instruction) now need a person.
- Neutral: the requests Lumen builds today never carry `regeneratable` evidence, so
  `NotRegeneratable` only acts on requests that do (the contract allows it; the
  datasets carry it).
- Bad: the guard that fired is not yet recorded in the trace; the trace shows the
  final `uncertain`. Recording it is a follow-up.
- Every rule was measured on one recording of one model and prompt; a new prompt or
  model is re-evaluated with the guards in place.

## More information

- [Research 17](../research/17-jeb-error-analysis.md) and
  `ai/tools/analyze_jeb_errors.py` (reproduces every number from the committed files).
- `crates/lumen-jev/src/guard.rs`; `docs/ai/model-contract.md` (evidence guards).
- [Roadmap Phase 14](../architecture/roadmap.md).
