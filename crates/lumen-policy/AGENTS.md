# lumen-policy: local guide

The deterministic safety policy (ADR-0014): facts in, `KEEP` / `REVIEW` /
`QUARANTINE` out. It is pure: no IO, no platform code, depends only on
`lumen-domain` (`cargo xtask lint` checks this).

## Invariants

- Stages run in order: hard protection → eligibility → classification → Jev. A
  protection that fires means `Keep`, and nothing overrides it.
- The most cautious proposal wins. Unknown, unreadable or unscanned is never treated
  as empty or absent (ADR-0008).
- Only a classification rule can make an item a quarantine candidate.
- Jev (AI) can only add caution: `Quarantine` may become `Review`; it never touches
  `Keep` or `Review` and never promotes (ADR-0020).
- Identical inputs give identical decisions; every fired rule records the evidence
  it used, so `(policy_version, evidence_hash)` replays a decision.

## When you change a rule

1. Bump `POLICY_VERSION` in `src/lib.rs` (at least the minor version) if any verdict
   can change.
2. Add or update the rule's sentence in `src/explain.rs`, and its translations in
   `apps/web/src/shared/i18n/` (the web tests check every rule has a sentence in
   every language).
3. Keep `tests/safety.rs` (property tests) passing; add a property if the rule adds
   a new safety claim. Never delete a line from `tests/safety.proptest-regressions`.
4. CI runs the tests and, because decisions feed the AI evaluation, replays the
   Jev cassettes; do not repeat them locally (root `AGENTS.md`, step 5).

A rule that removes more than before needs an ADR. When unsure, make it `REVIEW`.
