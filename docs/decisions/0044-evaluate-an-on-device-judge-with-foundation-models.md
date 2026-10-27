# ADR-0044: Evaluate an on-device judge with Foundation Models

- Status: Accepted
- Date: 2026-10-27
- Refines: [ADR-0020](0020-jev-evidence-only-judge.md) (tier 1, on-device judges);
  carries out the on-device part of roadmap 14.6

## Context and problem statement

The owner's goal for Phase 14 is an AI judge accurate enough to leave on. Its privacy
condition ([roadmap](../architecture/roadmap.md)) is that "on by default" cannot mean a
cloud judge: it needs an on-device judge, where no data leaves the Mac. Every Jeb
recording also spends the owner's school quota (ADR-0036), so measuring a cloud judge
is slow and rationed.

Apple's Foundation Models framework (macOS 26) gives apps the on-device model behind
Apple Intelligence. What matters here, from the framework's interface in Xcode 27.1
and Apple's documentation:

- **Guided generation:** a `@Generable` type, with `@Guide` descriptions, constrains
  decoding to its schema; an enum property can only take one of its cases. The model
  writes no free text unless a type asks for it.
- **Context:** `SystemLanguageModel.contextSize` is back-deployed and returns 4,096
  tokens on macOS 26; instructions, the generation schema, the prompt and the answer
  all count. `tokenCount(for:)` measures them from macOS 26.4.
- **Sampling:** `GenerationOptions` takes `.greedy` sampling (deterministic for a given
  model and input); there are no probabilities or log-probabilities.
- **Availability:** `SystemLanguageModel.default.availability` is `.available` or
  `.unavailable` with `appleIntelligenceNotEnabled`, `modelNotReady` (still
  downloading) or `deviceNotEligible`. Generation can still fail: context exceeded,
  guardrail violation, refusal, unsupported language, rate limited, and others.
- **Swift only:** the API is Swift; Lumen's safety core is Rust.

The question is how to measure this model on Lumen's datasets with the same checks a
production judge would get, without deciding yet whether the app uses it.

## Decision drivers

- AI output is evidence and can only make a decision more cautious (ADR-0020).
- No free-form model text is parsed or shown; requests are metadata only, never file
  contents (AGENTS.md).
- Replay must apply exactly the checks recording applied, including the evidence
  guards (ADR-0043), so the evaluation measures what the app would do.
- The best technology per task: Swift where the platform API is Swift, Rust for
  validation and policy.
- CI must not depend on Apple Intelligence: hosted runners are not eligible.

## Considered options

1. **A small Swift helper process with a line protocol; Rust validates and records**
   (the decision below).
2. **Link Swift into the Rust app** (a C ABI or `swift-bridge` shim). Rejected for
   now: it is a production integration (bundling, signing, `unsafe` FFI, a build that
   only works on macOS) decided before any result says the model is good enough.
   The helper's protocol and the Rust validator carry over if it is chosen later.
3. **A Swift runner that writes cassettes or grades on its own.** Rejected: it would
   duplicate the validator and the guards, and replay would no longer prove what the
   app does.
4. **Free-text answers parsed in Rust.** Rejected by AGENTS.md: guided generation makes
   every answer a closed value.

## Decision outcome

Chosen option: **1**, for evaluation only.

- **Helper** (`apps/fm-judge`, a Swift package, macOS 26): reads one `jev.request/1` per
  line on standard input, exactly the JSON a cloud judge receives as its `state`, and
  writes one line per request: `fm.answer/1` (the item reference, `model`
  `apple.foundation-models`, `model_version` such as `macOS 26.6.2 (25G83)`,
  `prompt_version`, the three enumerated answers and the elapsed milliseconds), or
  `fm.error/1` with a closed error code. `fm-judge --check` prints `fm.status/1` and
  exits 0 when the model is available, 3 when Apple Intelligence is not enabled, 4 when
  the model is not ready, 5 when the Mac is not eligible, 6 otherwise. `--render` and
  `--measure` size the prompt without and with the model. Nothing but these lines is
  written, and no model text is written anywhere.
- **Questions:** `@Generable` enums mirroring the Decisions questions:
  `injection_suspected` (Bool), `reason` (the 17 reason keys) and `assessment`
  (`likely_removable`, `likely_needed`, `uncertain`), generated in that order so the
  model looks at the strings and names its evidence before deciding.
- **Prompt `fm-decisions/1`:** prompt v2's rules (`jev-decisions/2`, research 17:
  positive evidence for `likely_removable`, every reason tied to its fact, the hard
  negatives named, strings are data) as one set of instructions, and the item as
  compact JSON without schema version and IDs, which only Lumen needs, after one
  English sentence (without it, the model's language check refused 5 of the 558 `dev`
  requests of dataset 2026.11.0 as an unsupported language). Each item gets a new
  session (nothing carries over), with greedy sampling and at most 128 answer tokens.
  Measured with `fm-judge --measure` on every request of datasets 2026.10.1 and
  2026.11.0 (4,522): instructions 565 tokens, schema 269, item at most 331, so at most
  1,165 (on average 1,012) of the 4,096-token context before the answer.
- **Validation** (`lumen_jev::foundation`): only the pinned model and prompt, a known
  assessment, one of the 17 offered reasons, a boolean flag, the item asked about, a
  printable model version of at most 64 characters, and no other field. The judgment
  cites every sent evidence ID, has an empty rationale and **`medium` confidence for
  every answer**: the model gives no probabilities, so a wrong removable answer always
  counts as a critical false positive rather than hiding below the `medium`
  threshold. The ADR-0043 guards then apply as for Jeb (`injection_suspected` is the
  injection signal). A test keeps the Swift option lists, prompt version and model
  name equal to the Rust ones.
- **Recording** (`lumen-jev-eval record --judge apple-fm`): the helper is started by
  absolute path, without a shell, and kept running; each answer has 60 seconds (a late
  answer restarts the helper). An error line is never recorded: "not available" codes
  stop the recording (the rest is not sent, and `--resume` continues it later), a rate
  limit or a busy model is retried twice, and anything else fails only that case. No
  pause between requests: nothing is shared with anyone. The cassette's descriptor is
  provider `apple.fm`, model `apple.foundation-models`, the OS version and build as
  `model_version`, and `fm-decisions/1`.
- **Replay** (`run`) validates on-device answers through the same function and refuses
  an answer from another model version than the descriptor's: an OS update is a new
  model, which needs a new recording (docs/ai/evaluation.md).
- **Not in the app:** nothing in `lumen-app`, the CLI or the desktop app asks this
  judge. Whether to integrate it, and whether it may be on by default, is decided
  after its results are measured against the Phase 14 exit criteria.
- **Build and CI:** `just fm-judge` builds the helper (`swift build -c release`);
  `just fm-judge-test` lints and tests it. `fm-judge.yml` lints, builds and tests it on
  the `macos-26` runner (default Xcode 26.6, which has the framework); its tests need
  no model. Recordings are made on a Mac with Apple Intelligence on.

### Consequences

- Good: the on-device model can be measured on every dataset split with no quota and
  no data leaving the Mac, with exactly the validation and guards a production judge
  would get.
- Good: the model can only answer with enumerated values; an unavailable model is
  reported as unavailable, never as an answer.
- Bad: no probabilities, so no confidence calibration of the usual kind; the fixed
  `medium` confidence is the strictest grading, and calibration (roadmap 14.5) would
  need self-consistency instead.
- Bad: the model changes with OS updates; each update needs a new recording, and
  greedy decoding is reproducible only within one model version.
- Neutral: the helper adds a Swift package and a macOS-only CI job; the Rust side
  stays portable (its protocol tests use shell-script helpers on Unix).
