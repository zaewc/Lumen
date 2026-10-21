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
| `record` | Ask the live judge (Jeb) about every case, one request at a time, and write a new cassette | yes |
| `mock` | Adversarial or scripted judgments to test policy invariants independently of any model | none |
| `compare` | Paired A/B of two configurations (prompt, model, policy version) on identical cases, with a McNemar test | replay or live |
| `calibrate` | Fit calibration maps on the calibration split | replay |

## Implemented

- `jev.cassette/1` (`lumen_jev_eval::Cassette`): recorded raw answers for one judge
  configuration (provider, pinned model, prompt version), keyed by the exact request
  hash.
- `ReplayJudge`: a `JudgeModel` answering from a cassette through the validator the
  recorded judge uses live: `lumen_jev::decisions::judgment_for` with the provider's
  pinned model for Jeb and Jev through OpenRouter (descriptor providers `gsm.jeb`,
  `openrouter.typesafe`), `lumen_jev::response` for any other judge. Unrecorded
  requests are unavailable.
- `record` (`lumen_jev_eval::record`): the raw answers of a live judge, fetched with
  `JevJudge::ask` (the transport `judge` itself uses, before validation).
- Mock mode: `lumen_testkit::MockJudge`.
- `jev.case/1` (`lumen_jev_eval::Case`) and `run`: grades each case (correct, acceptable,
  wrong, critical false positive, rejected, no answer) and reports the critical FPR
  with an exact Clopper–Pearson 95% upper bound against the 0.5% target, plus
  precision, recall and abstention. Policy-violation and canary gates join with the
  golden dataset (roadmap 8.7c).

## Commands

Implemented (roadmap 12.1):

```text
lumen-jev-eval run --dataset 2026.10.0 --cassette <FILE> [--ai ai] [--gate full|observed] [--json]
```

`run` replays the cassette over the dataset's golden and adversarial cases and exits 0
when the chosen gate passes, 1 when it fails and 2 for bad input. `full` is the release
gate (critical-FPR upper bound and canaries). It needs about 800 critical cases to be
able to pass a 0.5% bound, so the seed dataset (17 golden cases) cannot pass it.
`observed` (no observed critical false positive, clean canaries) is a regression check
for small datasets and never a release gate.

```text
lumen-jev-eval record --dataset 2026.10.0 --out <FILE> [--ai ai]
```

`record` asks Jeb (ADR-0036) about every golden and adversarial case of the dataset,
building exactly the requests `run` replays, and writes a new `jev.cassette/1` file:
`run --dataset <VERSION> --cassette <FILE>` then replays it with no network.

- **Key:** read only from the operating system's secret store, from the slot
  `lumen jev key set` writes (macOS keychain or Windows Credential Manager). It is
  never an argument or an environment variable, and never printed, logged or
  written; the cassette holds no headers. Without a stored key, `record` exits with 2
  and asks for `lumen jev key set`. The keychain may ask once before another program
  reads the CLI's item.
- **Disclosure:** before sending anything it prints the Jeb disclosure the CLI shows
  (the operator may keep requests; metadata only) and how many requests it will send.
  The cases are synthetic metadata from the dataset, never file contents.
- **Jeb's limits:** one request at a time, 1 s apart, each about 8,000 tokens at most,
  within the account's quota. A rate limit, server error, timeout or lost connection is
  retried up to three times (after 5, 10 and 20 s); a case still unanswered is left
  out of the cassette, never filled in. After the server rejects the key, nothing more
  is sent.
- **What is recorded:** the HTTP body exactly as received (what the validator reads),
  the model the answer names, `end_turn` as the stop reason (the Decisions protocol
  has none; only a complete answer is recorded), and the request hash `run` looks up.
  Answers the validator refuses are recorded too, so the replay refuses them the same
  way.
- **Output:** one progress line per case on standard error (recorded, refused by the
  validator, failed, or not sent) and a summary. The output must not exist yet; it is
  never overwritten. Exit 0 when every case was recorded, 1 when some were not (the
  rest are still written), and 2 for bad input.

Planned:

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
