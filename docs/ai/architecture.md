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

Jev is consulted only about items the policy proposes for quarantine: a judgment can
only demote those to `REVIEW`, so asking about anything else could never change a
decision. Items fixed by hard protections (stage 1, `KEEP` regardless) and items
already at `REVIEW` never reach it, which keeps their metadata out of prompts and
saves the provider's quota.

## Components

| Component | Crate | Responsibility |
| --- | --- | --- |
| `JudgeModel` port | `lumen-application` | `descriptor()` and `judge(&EvidenceBundle) -> Result<Judgment, JudgeError>` |
| Consultation | `lumen-application::consult` | Ask only when the policy's first decision is `QUARANTINE` and a judge is configured; re-evaluate with the judgment; trace every consultation |
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
| 2 | opt-in, per user, with disclosure; once turned on it stays on until turned off ([ADR-0037](../decisions/0037-keep-ai-review-on-once-turned-on.md)) | metadata only, redacted; zero-retention endpoints on OpenRouter, no retention guarantee on Jeb (disclosed on every run) | `JevJudge`: Jeb on the school AI Hub by default ([ADR-0036](../decisions/0036-jeb-on-the-school-ai-hub.md)), or TypeSafe's Jev via OpenRouter ([ADR-0031](../decisions/0031-jev-on-typesafe-via-openrouter.md)) |

Apple Private Cloud Compute counts as tier 2.

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
