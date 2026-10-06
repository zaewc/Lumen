# ADR-0014: Decide every action with a deterministic, versioned safety policy

- Status: Accepted
- Date: 2026-10-06

## Context and problem statement

The final verdict for any artifact (`KEEP`, `REVIEW` or `QUARANTINE`) decides whether
a user's data moves. It must be predictable, explainable, testable exhaustively, and
immune to AI errors and prompt injection.

## Decision drivers

- Safety over reclaimed space: when uncertain, `KEEP` or `REVIEW`.
- Determinism: identical evidence and policy version give an identical decision.
- Explainability: every decision lists the rules that fired and the evidence they used.
- AI output is evidence only and may never weaken a protection.

## Considered options

1. An IO-free Rust crate evaluating ordered, typed rules in fixed stages, with versioned
   rule data and a decision trace.
2. A general rules engine or embedded scripting (Rego, CEL, Lua).
3. A learned or weighted scoring model combining rules and AI confidence.

## Decision outcome

Chosen option: **1**, implemented in `lumen-policy`, which depends only on
`lumen-domain`.

Evaluation stages, in order; the first stage that produces a final verdict wins:

1. **Hard protections → `KEEP` (non-overridable).** Operating-system and
   integrity-protected locations (SIP and the sealed system volume, `SF_RESTRICTED` and
   `SF_NOUNLINK`, `%WINDIR%\WinSxS`, `System32`, `\Windows\Installer`, pagefile,
   hiberfil and swapfile), credentials and key material (SSH, GPG, keychains, browser
   profiles, password managers, wallet data), databases and application state,
   security software, cloud placeholders and sync roots, items in use by a running
   process, items with unknown or denied coverage, and items whose identity changed
   since the scan.
2. **Eligibility.** The platform must support the action (ADR-0008); the volume must
   support atomic no-replace rename (ADR-0015); hard-linked, cloned or snapshot-held
   items report "frees nothing" and are not proposed for space reasons.
3. **Classification rules → candidate verdict.** Known-regeneratable categories (e.g.
   a vendor-declared cache of a non-running application, Xcode DerivedData, aged logs)
   may become `QUARANTINE` candidates. Developer artifacts with an official cleanup
   command become **tool actions** (ADR-0021). Everything unknown is `REVIEW` at most.
4. **Jev evidence (optional, monotone).** Jev may only **increase caution**
   (`QUARANTINE` → `REVIEW`), e.g. on `likely_needed` or `injection_suspected`. A rule
   that lets Jev promote `REVIEW` → `QUARANTINE` must be allow-listed per artifact
   class, gated on the calibrated precision bound for that class, model and prompt
   version, disabled for protected classes, and off until the locked evaluation set
   meets its target (ADR-0020).
5. **User confirmation.** `QUARANTINE` is a proposal. Nothing executes without explicit
   confirmation of a specific plan; `REVIEW` items are never executed in bulk.

Rule data:

- Rules are Rust code plus declarative, schema-validated data files (path patterns,
  bundle ID lists, knowledge-base entries) embedded at build time. No runtime-downloaded
  executable policy.
- The policy has a semantic `policy_version`. Every `PolicyDecision` records:
  `policy_version`, `evidence_hash`, the verdict, the fired rules with the evidence IDs
  each used, the Jev effect if any, and a human-readable explanation built from
  templates.
- Path matching works on identities and on normalised, case-folded copies keyed by
  per-volume case and normalisation behaviour; never on raw string equality alone.

Testing (release-blocking):

- property tests: protected items are never `QUARANTINE`; identical inputs give
  identical outputs; `REVIEW` never becomes executable without user action; Jev cannot
  lower caution on protected classes;
- snapshot tests of explanations;
- mutation testing (`cargo-mutants`) on `lumen-policy` in nightly CI.

### Consequences

- Good: the safety core is small, pure and exhaustively testable.
- Good: every decision is reproducible from `(policy_version, evidence_hash)`.
- Bad: new artifact categories require code or data changes plus review, not
  configuration at runtime. This is intended.

Rejected: option 2 adds an interpreter and makes review harder; option 3 lets a
confident-but-wrong model outvote rules and is not auditable.

## More information

- [AI harness research, Implications 3](../research/08-ai-harness-and-evaluation.md)
- [macOS research, Implication 11](../research/03-macos-platform.md)
- [Windows research, §D deny-list](../research/04-windows-platform.md)
- [Security research, §A.4](../research/09-security-devops-quality.md)
