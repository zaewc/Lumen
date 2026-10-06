# Jev safety

## Threat summary

Jev reads attacker-controllable text: file and directory names, plist values, bundle
display names, registry strings and Android labels. Prompt injection has no complete
defense. Adaptive attacks broke 12 recent defenses, with success above 90% against
most of them (Nasr, Carlini et al. 2025). Lumen therefore relies on **architecture**,
not on prompts, for safety.

## Controls

1. **No capabilities.** Jev has no tools, cannot read files, cannot call APIs, cannot
   send data anywhere, and cannot trigger actions. In Meta's "Agents Rule of Two"
   terms, it processes untrusted input but has neither state-changing power nor
   exfiltration ability.
2. **Closed output vocabulary.** Assessments, confidences and reason codes are enums;
   evidence references must exist in the request; the rationale is display-only,
   escaped and capped ([model contract](model-contract.md)).
3. **Monotone-safety policy contract** ([ADR-0014](../decisions/0014-deterministic-safety-policy-engine.md)):
   - Protected classes are decided before Jev runs and are never sent to it.
   - By default Jev can only demote: `QUARANTINE` candidate → `REVIEW` on
     `likely_needed`, `uncertain` or `injection_suspected`.
   - Promotion (`REVIEW` → `QUARANTINE` candidate) is off by default. It can be enabled
     only per allow-listed artifact class, when the locked test set shows a critical
     false-positive upper bound (Clopper–Pearson, 95%) at or below the target, for the
     exact model and prompt version.
   - Even a promoted item still needs explicit user confirmation of a plan.
4. **Input hygiene.** Untrusted strings are JSON-encoded inside labelled fields, with
   bidi and control characters escaped; real paths are redacted for tier 2.
5. **Validation.** Responses are rejected on refusal, truncation, schema failure,
   unknown evidence IDs or an unexpected model.
6. **Canary corpus.** Release-blocking injection tests in both directions
   ([datasets](datasets.md)).
7. **Privacy.** Cloud use is opt-in, metadata-only and redacted; on-device first.

## Invariants (tested)

- For every protected-class fixture, the final verdict is `KEEP` regardless of any
  `Judgment`, including adversarial ones injected through `MockJudge`.
- No `Judgment` can turn a `REVIEW` into an executed action without user confirmation.
- No canary flips an assessment toward `likely_removable` relative to its clean twin.
- A `JudgeError` never changes the deterministic verdict.

## Low confidence

`uncertain`, or any confidence whose calibrated precision bound is below the class
threshold, maps to `REVIEW`. Lumen never manufactures confidence: an uncalibrated
model or prompt version has no effect on decisions beyond demotion.

## Change control

Changes to the Jev policy stage, the prompt, the schema or the pinned model are
CODEOWNERS-protected and must update the [threat model](../security/threat-model.md)
when they change the attack surface.
