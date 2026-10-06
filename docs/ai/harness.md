# AI and evaluation harness

The harness makes every AI-related behaviour reproducible and testable without manual
checks. It is a Rust crate and CLI, `lumen-jev-eval`, so it exercises the real request
builder, validator and policy, not a reimplementation.

## Pipeline

```text
fixture definition ─▶ build fixture tree (tempdir) ─▶ scan (real lumen-scan + fakes)
     ─▶ evidence graph ─▶ EvidenceBundle ─▶ JudgeModel (live | replay | mock)
     ─▶ validator ─▶ policy (real lumen-policy) ─▶ compare with expected
     ─▶ record result + trace ─▶ metrics ─▶ report + gate verdict
```

## Modes

| Mode | Purpose | Network |
| --- | --- | --- |
| `replay` | CI default: responses come from recorded cassettes keyed by `(case_id, request_hash, model, prompt_version)` | none |
| `record` | Run live and write cassettes; cloud runs go through the provider batch API (50% cost) with `custom_id = case_id` | yes |
| `mock` | Adversarial or scripted judgments to test policy invariants independently of any model | none |
| `compare` | Paired A/B of two configurations (prompt, model, policy version) on identical cases, with a McNemar test | replay or live |
| `calibrate` | Fit calibration maps on the calibration split | replay |

## Commands (planned)

```text
lumen-jev-eval run      --dataset ai/golden --split test-locked --config configs/sonnet-5-5.toml --mode replay
lumen-jev-eval record   --dataset ai/golden --split test-locked --config configs/sonnet-5-5.toml
lumen-jev-eval compare  --a configs/prompt-3.toml --b configs/prompt-4.toml --split dev
lumen-jev-eval calibrate --split calibration --config configs/sonnet-5-5.toml
lumen-jev-eval gate     --report ai/reports/jev-judge/4/sonnet-5-5.json
```

Configurations pin the provider, model ID, prompt version, schema versions, policy
version and redaction policy. A configuration file plus a dataset version fully
determine a run.

## Reproducibility

- Current Claude models reject `temperature` changes, so live outputs are not
  deterministic. Reproducibility comes from **recorded cassettes**, not sampling
  settings.
- Every run stores: configuration, dataset version and manifest hash, harness version
  (git SHA), cassette hashes, and per-case traces.
- Replaying a cassette must yield bit-identical metrics. CI checks this.

## Integration with development

- PRs touching `lumen-jev`, `lumen-policy`, schemas, prompts or datasets run
  `replay` + `gate` in CI.
- Cassette refreshes (`record`) are their own PRs and include the report diff.
- Developers may use promptfoo through an `exec` provider that calls this CLI for
  quick prompt A/B and red-team generation. promptfoo is never the source of truth
  for gates.
- On-device models (Foundation Models, ML Kit) are evaluated by device runners that
  produce the same cassette format, keyed by the on-device model identifier and OS
  build.

## Claude Code harness

Separately from Jev, the repository is set up so AI coding agents can work safely for
long periods. See [`AGENTS.md`](../../AGENTS.md) for the operating loop (discover →
understand → plan → implement → verify → commit → PR → merge → continue), and
[ADR-0027](../decisions/0027-atomic-commit-pr-workflow.md) for the Git workflow.
