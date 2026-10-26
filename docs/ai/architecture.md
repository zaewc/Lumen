# Jev architecture

> Decision: [ADR-0020](../decisions/0020-jev-evidence-only-judge.md). Research:
> [08-ai-harness-and-evaluation](../research/08-ai-harness-and-evaluation.md).

Jev is Lumen's optional, probabilistic judgment component. It contributes **evidence**
about ambiguous artifacts. It never decides, never acts, and Lumen works fully
without it.

## Position in the pipeline

```text
Evidence graph ─▶ EvidenceBundle ─▶ Deterministic policy (stages 1–3)
                        │                     │
                        │        candidate verdict + "Jev eligible?"
                        ▼                     │
                 Request builder ◀────────────┘   (only REVIEW/QUARANTINE candidates
                        │                          of non-protected classes)
                        ▼
                 JudgeModel adapter (Noop | Replay | Anthropic | FoundationModels | …)
                        │
                        ▼
                 Response validator ─▶ Judgment (closed vocabulary) + JevTrace
                        │
                        ▼
                 Policy stage 4 (monotone: may only increase caution by default)
                        │
                        ▼
                 PolicyDecision ─▶ explanation (Jev rationale shown separately, labelled)
```

Jev is consulted for a judgment only about items the policy proposes for quarantine: a
judgment can only demote those to `REVIEW`, so asking about anything else could never
change a decision. Items fixed by hard protections (stage 1, `KEEP`) never reach it.

Items the policy itself leaves at `REVIEW` are not judged, but while AI review is on,
up to 20 of the largest of them that no knowledge-base entry identifies are
**described**
([ADR-0041](../decisions/0041-ai-describes-what-review-items-are.md)): the same
redacted request, with one closed question, "What kind of item is this most likely?"
(`lumen_application::describe`). The answer is a note shown beside the item, labelled
as AI ("AI: likely a cache (low confidence)"); it never reaches the policy, a plan or
the executor, and it is traced like a judgment. `lumen plan` describes nothing.

## Components

| Component | Crate | Responsibility |
| --- | --- | --- |
| `JudgeModel` port | `lumen-application` | `descriptor()` and `judge(&EvidenceBundle) -> Result<Judgment, JudgeError>` |
| Consultation | `lumen-application::consult` | Ask only when the policy's first decision is `QUARANTINE` and a judge is configured; re-evaluate with the judgment; trace every consultation |
| Description | `lumen-application::describe` | Ask up to 20 of the largest unrecognised `REVIEW` items what kind of item each is, one at a time; set the note beside the decision, never in it; stop at the first unavailable answer; trace every request |
| Request builder | `lumen-jev` | Canonical, redacted, versioned request from a bundle; untrusted strings JSON-encoded in labelled fields |
| Adapters | `lumen-jev` | Provider-specific transport only; no parsing logic of their own |
| Response validator | `lumen-jev` | `stop_reason` handling, JSON Schema validation, enum and length checks, evidence-ID membership, pinned-model check |
| Calibration maps | `lumen-jev` | Map `(model, prompt version, confidence bucket)` → empirical precision bound |
| Trace recorder | `lumen-jev` + `lumen-store-sqlite` | `JevTrace` rows in `ledger.db` |
| Evaluation harness | `lumen-jev-eval` | Datasets, replay, graders, metrics, gates ([harness](harness.md)) |

## Tiers and consent

| Tier | Default | Data leaves device | Adapters |
| --- | --- | --- | --- |
| 0 | yes | no | `NoopJudge` |
| 1 | opt-in | no | on-device (Apple Foundation Models; later ONNX Runtime / llama.cpp; Android ML Kit for foreground only) |
| 2 | opt-in, per user, with disclosure; once turned on it stays on until turned off ([ADR-0037](../decisions/0037-keep-ai-review-on-once-turned-on.md)) | metadata only, redacted, of proposed items and of up to 20 items left for review; zero-retention endpoints on OpenRouter, no retention guarantee on Jeb (disclosed on every run) | `JevJudge`: Jeb on the school AI Hub by default ([ADR-0036](../decisions/0036-jeb-on-the-school-ai-hub.md)), or TypeSafe's Jev via OpenRouter ([ADR-0031](../decisions/0031-jev-on-typesafe-via-openrouter.md)) |

Apple Private Cloud Compute counts as tier 2.

Tier 2 is turned on with `lumen ai on` in the CLI, or with the AI review switch in the
desktop app's Settings page. The app's switch shows the same disclosure in a consent
dialog, takes a Jeb key there if none is stored (into the OS secret store, never
returned to the UI), and records the choice next to the app's own `index.db`
(`lumen_app::ai`). While on, the app's `assess` and `plan` commands consult the judge
and record its traces in the ledger, like the CLI.

## Failure handling

Every error (network, timeout, refusal, `max_tokens`, schema failure, unknown evidence
ID, unexpected model) is a `JudgeError`. The policy then proceeds **without** Jev
evidence; it never fails the decision and never retries with a different model.

## Related documents

- [Model contract](model-contract.md): input and output schemas.
- [Prompting](prompting.md): prompt structure and versioning.
- [Safety](safety.md): injection defenses and the monotone contract.
- [Evaluation](evaluation.md) and [datasets](datasets.md): release gates.
- [Harness](harness.md): the development and evaluation harness.
