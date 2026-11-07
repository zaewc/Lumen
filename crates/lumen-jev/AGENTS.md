# lumen-jev: local guide

Jev, the optional AI judge (ADR-0020). Its output is evidence only: the policy
consumes a validated judgment and can only become more cautious. See
[`docs/ai/`](../../docs/ai/architecture.md) for the contract.

## Invariants

- Requests carry metadata only (`src/request.rs`). Never add file contents, and never
  send anything before the user has chosen AI review (ADR-0050).
- Model text is never parsed as instructions. Answers go through the validator
  (`src/response.rs`, `src/decisions.rs`) and then the evidence guards
  (`src/guard.rs`, ADR-0043), which discard removable answers the evidence does not
  support.
- Text from files or the model is untrusted (`UntrustedText`); an answer that tries to
  influence the judge demotes, never promotes.
- The API key lives in the OS secret store, never in databases, logs or code
  (ADR-0042 covers the built-in release key).

## When you change a prompt, schema or validator

1. A prompt change is a new prompt version (`jev-decisions/N`); never edit the
   wording of a released one in place.
2. Re-recording cassettes calls the live judge (Jeb) and spends the school server's
   quota: ask the owner first. Without a recording, CI replays the existing cassettes
   (`.github/workflows/jev-eval.yml`) and its committed reports must not change.
3. CI runs the tests, the schema check and the cassette replay; do not repeat them
   locally (root `AGENTS.md`, step 5).
4. Update [`docs/ai/evaluation.md`](../../docs/ai/evaluation.md) with new results.
