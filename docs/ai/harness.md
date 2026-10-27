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
  `openrouter.typesafe`), with the prompt version the cassette records
  (`jev-decisions/1` or `/2`; a Decisions cassette naming any other is refused),
  `lumen_jev::response` for any other judge, both followed by the evidence guards
  (ADR-0043), exactly as live. Unrecorded requests are unavailable.
- `record` (`lumen_jev_eval::record`): the raw answers of a live judge, fetched with
  `JevJudge::ask` (the transport `judge` itself uses, before validation), or with
  `FmHelper` from Apple's on-device model (provider `apple.fm`, validated by
  `lumen_jev::foundation`, ADR-0044).
- Mock mode: `lumen_testkit::MockJudge`.
- `jev.case/1` (`lumen_jev_eval::Case`) and `run`: grades each case (correct, acceptable,
  wrong, critical false positive, rejected, no answer) and reports the critical FPR
  with an exact Clopper–Pearson 95% upper bound against the 0.5% target, plus
  precision, recall and abstention. Policy-violation and canary gates join with the
  golden dataset (roadmap 8.7c).

## Commands

Implemented (roadmap 12.1):

```text
lumen-jev-eval run --dataset 2026.10.0 --cassette <FILE> [--split <NAME>] [--prompt v1|v2]
                   [--ai ai] [--gate full|observed] [--json]
```

`run` replays the cassette over the dataset's golden and adversarial cases and exits 0
when the chosen gate passes, 1 when it fails and 2 for bad input. `full` is the release
gate (critical-FPR upper bound over the answered critical cases, and canaries). It needs
about 800 critical cases to be able to pass a 0.5% bound, so the seed dataset (17 golden
cases) cannot pass it. `observed` (no observed critical false positive, clean canaries)
is a regression check for small datasets and never a release gate. Both gates fail
when any critical case or canary has no valid answer (unavailable or rejected): such a
case is no evidence, and a judge must not pass by failing on exactly the hard cases.

- `--split <NAME>` keeps only that split's golden cases (`dev`, `calibration` or
  `test-locked`) and the canaries of their twins (a canary is always in its twin's
  split; loading a dataset checks it). The report then names the split (`"split"` in
  the JSON; a whole-dataset report has no such field), and every metric and gate is
  computed on those cases only. An unknown split is bad input.
- `--prompt v1|v2` refuses (exit 2) a cassette recorded with another prompt, so a
  comparison cannot mix them up. Without it, the cassette's own prompt is used.

```text
lumen-jev-eval record --dataset 2026.10.0 --out <FILE> [--split <NAME>] [--prompt v1|v2]
                      [--judge jeb|apple-fm|hub-chat] [--helper <PATH>] [--ai ai] [--resume <FILE>]
```

`record` asks Jeb (ADR-0036) about every golden and adversarial case of the dataset,
or of one `--split`, building exactly the requests `run` replays, and writes a new
`jev.cassette/1` file: `run --dataset <VERSION> [--split <NAME>] --cassette <FILE>` then
replays it with no network. `--prompt` chooses the questions (`v1`, the app's prompt,
by default; `v2` for roadmap 14.4); the cassette's descriptor records it. Iterating on
a prompt uses the `dev` split only (346 golden cases and 15 canaries of 2026.10.1, 361
requests).

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
- **Resuming:** `--resume <FILE>` completes an earlier recording of the same dataset:
  answers it holds for the same request are kept unchanged and only the missing cases
  are sent, then everything is written, in dataset order, to the new `--out` file. The
  earlier file must come from the same judge, model and prompt (checked before
  anything is sent); with `--split`, give the same split again.

### Recording the AI Hub's chat model

`record --judge hub-chat` asks the AI Hub's chat model `flash-next` beside Jeb
([ADR-0045](../decisions/0045-evaluate-the-ai-hub-chat-model-as-a-judge.md)): the same
server, key, disclosure and pacing as Jeb (one request at a time, 1 s apart, retries
after 5, 10 and 20 s), at `/v1/chat/completions`, with its own prompt
(`chat-decisions/1`; `--prompt` and `--helper` are refused). Each request spends the
owner's Hub quota, so a recording needs the owner's go-ahead first.

```text
lumen-jev-eval record --dataset 2026.11.0 --split dev --judge hub-chat \
    --out ai/cassettes/chat-decisions/1/flash-next-2026.11.0-dev.json
lumen-jev-eval run --dataset 2026.11.0 --split dev \
    --cassette ai/cassettes/chat-decisions/1/flash-next-2026.11.0-dev.json
```

The cassette keeps each response body as the server sent it; replay accepts only an
answer whose whole content is the closed JSON object (`lumen_jev::chat`).

### Recording the on-device judge

`record --judge apple-fm` asks Apple's on-device model instead of Jeb
([ADR-0044](../decisions/0044-evaluate-an-on-device-judge-with-foundation-models.md)),
through the `fm-judge` helper, with its own prompt (`fm-decisions/1`; `--prompt` is
refused). Nothing leaves the Mac and no quota is spent. It needs macOS 26 with Apple
Intelligence on and its model downloaded.

```text
just fm-judge                                   # swift build -c release
apps/fm-judge/.build/release/fm-judge --check   # exit 0: available; 3: Apple Intelligence
                                                # off; 4: model not ready; 5: Mac not eligible
lumen-jev-eval record --dataset 2026.11.0 --split dev --judge apple-fm \
    --out ai/cassettes/fm-decisions/1/apple-fm-2026.11.0-dev.json
lumen-jev-eval run --dataset 2026.11.0 --split dev \
    --cassette ai/cassettes/fm-decisions/1/apple-fm-2026.11.0-dev.json
```

- **Helper:** `--helper <PATH>` names it by absolute path; by default
  `apps/fm-judge/.build/release/fm-judge` under the current directory. It is started
  without a shell and kept running; it gets each request as one `jev.request/1` line,
  exactly what Jeb gets as `state`.
- **Before sending:** `fm-judge --check` must report the model available, with the
  model and prompt Lumen expects; otherwise `record` exits 2 and says why ("Apple
  Intelligence is not enabled", "the on-device model is not ready yet", "this Mac is
  not eligible"). Its `model_version` (the OS version and build) goes into the
  cassette's descriptor, and replay refuses answers from another version.
- **Errors:** an `fm.error/1` line is never recorded. The model becoming unavailable
  stops the recording (finish it later with `--resume`); a rate limit or a busy model
  is retried twice, after 2 and 4 s; an item that does not fit the context, a
  guardrail, a refusal or an unsupported language fails that case only. An answer
  that takes over 60 s fails its case and restarts the helper.
- **Pace:** one request at a time, with no pause; the summary says how long the
  requests took. On the owner's Mac (macOS 26.6.2), the 558 `dev` requests of
  2026.11.0 took 486 s, 0.87 s each (median 0.86 s, slowest 1.5 s). `fm-judge --render` (bytes) and `--measure` (tokens, macOS 26.4 and
  the model) size every prompt of a dataset without recording.

Comparing prompts on one split (roadmap 14.4) uses
`ai/tools/compare_cassettes.py`, which reads only committed or recorded files and
sends nothing:

```text
lumen-jev-eval run --dataset 2026.10.1 --split dev --cassette A.json --json > run-a.json
lumen-jev-eval run --dataset 2026.10.1 --split dev --cassette B.json --json > run-b.json
python3 ai/tools/compare_cassettes.py --dataset 2026.10.1 --split dev \
    --run-a run-a.json --run-b run-b.json A.json B.json
```

It reads each cassette's raw answers as the validator does and prints, for both, the
critical false positives with exact 95% Clopper–Pearson intervals, precision, recall,
abstention and canary flips, without the evidence guards (computed from the raw
answers, as `analyze_jeb_errors.py` does) and with them (checked case by case against
the `run` outputs, exiting with 1 on any difference), then the paired differences with
an exact McNemar test, and the reason, probability and injection-probability
distributions. With one cassette it prints that cassette alone: the `jev-decisions/1`
baseline on `dev` is
`compare_cassettes.py ai/cassettes/jev-decisions/1/jeb-2026.10.1.json` (golden: 15 / 228
critical false positives with the guards, 17 without; recall 96.2%; no canary flip).

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

- PRs touching `ai/`, `lumen-jev`, `lumen-jev-eval`, `lumen-policy`, `lumen-domain`,
  `lumen-application` or `Cargo.lock` replay the recorded Jeb cassettes
  (`.github/workflows/jev-eval.yml`, datasets 2026.10.0 and 2026.10.1) and must
  reproduce the committed reports (`ai/reports/jev-decisions/1/`) byte for byte; a change that
  moves a grade updates the report in the same PR.
- Cassette refreshes (`record`) are their own PRs and include the report diff.
- `ai/tools/analyze_jeb_errors.py` breaks a recording's errors down and measures
  candidate guards from the committed cassette, with no network
  ([research 17](../research/17-jeb-error-analysis.md)).
- Developers may use promptfoo through an `exec` provider that calls this CLI for
  quick prompt A/B and red-team generation. promptfoo is never the source of truth
  for gates.
- On-device models are evaluated by device runners that produce the same cassette
  format, keyed by the on-device model identifier and OS build: Foundation Models
  through `record --judge apple-fm` (above); ML Kit later. CI builds and tests the
  `fm-judge` helper (`fm-judge.yml`) but cannot record: hosted runners are not eligible
  for Apple Intelligence.

## Claude Code harness

Separately from Jev, the repository is set up so AI coding agents can work safely for
long periods. See [`AGENTS.md`](../../AGENTS.md) for the operating loop (discover →
understand → plan → implement → verify → commit → PR → merge → continue), and
[ADR-0027](../decisions/0027-atomic-commit-pr-workflow.md) for the Git workflow.
