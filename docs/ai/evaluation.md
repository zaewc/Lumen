# Jev evaluation

Evaluation is a release gate, not a dashboard. A prompt, schema or model change cannot
affect user decisions until it passes these gates. Tooling is described in
[harness](harness.md); data in [datasets](datasets.md).

## Framing

- The positive class is **safe to remove**.
- The dominant error is a **critical false positive**: an item that is needed (or
  protected) judged `likely_removable` with `medium` or `high` confidence.
- Missing a removable item only costs reclaimed space, which matters far less.

## Metrics

Computed per stratum (artifact class, platform), per `(model, prompt version)`:

| Metric | Definition | Use |
| --- | --- | --- |
| Critical FPR | needed items judged `likely_removable` at medium/high confidence ÷ needed items | primary safety metric, reported with an exact Clopper–Pearson 95% upper bound |
| Precision / recall | on `likely_removable` | utility |
| FPR / FNR | standard | utility |
| Abstention rate | share of `uncertain` | coverage |
| Expected Calibration Error (ECE) | over confidence buckets vs empirical accuracy | calibration |
| Reliability data | per bucket | calibration maps |
| Risk–coverage curve | precision vs coverage across thresholds | threshold selection |
| Cost-weighted loss | asymmetric cost matrix (critical FP ≫ FN) | threshold selection (not F1) |
| Policy violations | final verdict ≠ expected on protected fixtures | must be zero |
| Canary flips | adversarial twin moves toward removable | must be zero |
| pass^k | all k trials correct on the critical subset (k = 5) | stability under provider non-determinism |
| Rationale quality | LLM-as-judge or human rating | informational only; never a gate on safety labels |

## Gates

| Gate | Requirement |
| --- | --- |
| Schema | 100% of responses validate, or are rejected cleanly |
| Policy safety | 0 policy violations on protected fixtures (deterministic, with adversarial `MockJudge` inputs as well); enforced by `crates/lumen-application/tests/protected_fixtures.rs` on every PR |
| Injection | 0 canary flips; `injection_suspected` recall tracked and must not regress |
| Critical FPR | per promotable class, upper bound ≤ target (initial target 0.5%); otherwise Jev stays demote-only for that class |
| Stability | pass^5 = 100% on the critical subset |
| Regression | no statistically significant regression vs the current release (paired McNemar test on the same cases) |

Rare classes that cannot reach statistical power stay "Jev cannot promote".

## Procedure

1. Run the locked test split through the candidate configuration (live, via the batch
   API for cloud models; on-device via a device runner).
2. Record responses as cassettes with full traces.
3. Grade deterministically; compute metrics and gates.
4. Fit calibration maps on the calibration split only; verify on the locked test
   split.
5. Publish the report (`ai/reports/<prompt>/<version>/<model>.json` plus a Markdown
   summary) and link it from the prompt manifest.
6. CI replays cassettes on every PR touching Jev, policy or schemas, so a code change
   is evaluated without network access.

## Results

| Judge | Dataset | Summary |
| --- | --- | --- |
| AI Hub chat model (`gsm.chat`, `flash-next`, `chat-decisions/1`) | 2026.11.0, dev split | [report](../../ai/reports/chat-decisions/1/flash-next-2026.11.0-dev.md): 55 critical false positives in 339 (upper bound 20.59%), recall 82.3%, 18 canary flips; evaluation only |
| Jeb (`gsm.jeb`, `jev-decisions/2`) | 2026.10.1 | [report](../../ai/reports/jev-decisions/2/jeb-2026.10.1.md): with the evidence guards, 14 critical false positives in 1,103 (upper bound 2.12%; v1 71, 8.05%), recall 83.3% (v1 96.9%), 20 canary flips (v1 17; 32 before the guards); release gate fails; evaluation only, production stays on v1 |
| Jeb (`gsm.jeb`, `jev-decisions/2`) | 2026.10.1, dev split | [report](../../ai/reports/jev-decisions/2/jeb-2026.10.1-dev.md): 2 critical false positives in 228 (upper bound 3.13%; v1 on the same cases 17, 11.67%), recall 80.0% (v1 96.2%), 0 canary flips; evaluation only, production stays on v1 |
| Jeb (`gsm.jeb`, `jev-decisions/1`) | 2026.10.1 | [report](../../ai/reports/jev-decisions/1/jeb-2026.10.1.md): with the evidence guards ([ADR-0043](../decisions/0043-discard-removable-answers-the-evidence-does-not-support.md)), 71 critical false positives in 1,103 critical cases (upper bound 8.05% against 0.5%; 81 and 9.05% before), 17 canary flips (19 before); release gate fails, demote-only. Error analysis: [research 17](../research/17-jeb-error-analysis.md) |
| Jeb (`gsm.jeb`, `jev-decisions/1`) | 2026.10.0 | [report](../../ai/reports/jev-decisions/1/jeb-2026.10.0.md): with the evidence guards, no critical false positive in 7 critical golden cases (1 before, an Xcode archive), clean canaries; demote-only |

## Recalibration triggers

- A new model ID or model version, including OS-bundled on-device model updates.
- A new prompt version or schema major version.
- A policy version that changes Jev eligibility.
- Dataset version bumps that add strata.
