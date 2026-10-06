# ADR-0020: Integrate Jev as an optional, evidence-only judge behind a port

- Status: Accepted
- Date: 2026-10-06

## Context and problem statement

Jev is a probabilistic judgment component that can add useful evidence for ambiguous
artifacts (e.g. an unknown launch agent whose application seems to be gone). It
processes attacker-controllable text (filenames, plist values, bundle IDs, registry
entries). Prompt injection is not solved: adaptive attacks bypassed 12 recent defenses
with success rates above 90% against most of them (Nasr, Carlini et al. 2025).
Providers change sampling rules, schema subsets and models frequently.

## Decision drivers

- AI must never directly control deletion.
- Lumen must be fully functional without AI.
- Local-first: no data leaves the device without explicit consent.
- Every AI contribution must be reproducible and auditable.

## Considered options

1. A `JudgeModel` port with interchangeable adapters, a closed-vocabulary output
   schema, a monotone-safety policy contract, consent tiers, and a Rust evaluation
   harness as the release gate.
2. AI as the primary classifier with rules as guardrails.
3. No AI at all.

## Decision outcome

Chosen option: **1**.

Port (in `lumen-application`):

```rust
pub trait JudgeModel: Send + Sync {
    fn descriptor(&self) -> JudgeDescriptor; // provider, model id, model version, prompt version
    async fn judge(&self, bundle: &EvidenceBundle) -> Result<Judgment, JudgeError>;
}
```

The port is async because adapters are network- or model-bound. Dynamic dispatch uses
`dynosaur`, or a boxed future, behind a small adapter registry.

Adapters (in `lumen-jev`): `NoopJudge` (the default), `MockJudge`/`ReplayJudge`
(tests and evaluations), `AnthropicJudge` (cloud, opt-in), and later on-device judges
(`FoundationModelsJudge` on Apple platforms, a llama.cpp / ONNX Runtime GenAI judge on
desktop, ML Kit on Android for foreground use only). All adapters share one canonical
request builder and one response validator in Rust.

Output contract (`jev.judgment/1`, JSON Schema, `additionalProperties: false`):
`{item_ref, assessment: likely_removable | likely_needed | uncertain, confidence: low | medium | high, reason_codes: [enum], evidence_refs: [evidence id], injection_suspected: bool, rationale: string}`.
There are no paths, commands, actions or numeric scores. The rationale is display-only,
escaped, and length-capped by Rust validation. Responses that stop on `refusal` or
`max_tokens`, fail validation, reference unknown evidence IDs, or come from a model
other than the pinned one become `JudgeError`. An error never fails the policy; the
item simply lacks Jev evidence.

Policy contract (monotone safety, ADR-0014): by default Jev can only increase caution.
Any rule that lets Jev promote an item is per-class, gated on calibrated precision
bounds from the locked test set, disabled for protected classes, and off until the
evaluation gate passes. Low or uncalibrated confidence means `REVIEW`.

Consent tiers:

- Tier 0 (default): no AI.
- Tier 1: on-device judges where available.
- Tier 2: cloud Jev, opt-in per user with clear disclosure. It sends a metadata-only,
  path-redacted evidence bundle: the home directory is tokenised, user names are
  stripped, and file contents are never sent.

Cloud default model: `claude-sonnet-5-5` with structured outputs
(`output_config.format`). The model ID is pinned in configuration, and any model change
is a release that requires recalibration. No `temperature`, forced `tool_choice` or
prefill (rejected by current models). Refusal fallbacks to other models are disabled.

Traceability: every judgment that a policy decision consumed is stored as a `JevTrace`
in `ledger.db`. It records provider, model, model version, prompt version, policy
version, input and output schema versions, evidence hash, request and response hashes,
timestamp, assessment, confidence, validation result and policy effect. The trace is
linked from the quarantine manifest. Raw prompts and responses are stored locally only
if the user enables diagnostics.

Evaluation (`lumen-jev-eval`, documented in `docs/ai/`): golden JSONL datasets,
cassette replay in CI, deterministic graders, the critical false-positive rate with an
exact (Clopper–Pearson) upper bound, calibration (ECE, reliability data), pass^k on
the critical subset, an injection canary corpus, and paired A/B comparison of prompts
and models.

### Consequences

- Good: AI can only make Lumen more careful unless proven otherwise per class.
- Good: providers are swappable; tests never need a network.
- Bad: calibration must be redone per model and prompt version, including OS-bundled
  on-device models that change with OS updates.

Rejected: option 2 makes safety depend on an unsolvable injection problem; option 3
leaves useful evidence on the table, so Jev stays strictly optional.

## More information

- [AI harness research](../research/08-ai-harness-and-evaluation.md)
- [iOS research, Implication 7](../research/06-ios-and-mobile-framework.md)
- `docs/ai/` (architecture, model contract, prompting, safety, evaluation, datasets)
