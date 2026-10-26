# ADR-0031: Use TypeSafe's Jev model through OpenRouter as the cloud judge

- Status: Accepted
- Date: 2026-10-14
- Amends: [ADR-0020](0020-jev-evidence-only-judge.md) (adapters and tiers only)
- Amended by: [ADR-0036](0036-jeb-on-the-school-ai-hub.md) (Jeb is the default provider),
  [ADR-0043](0043-discard-removable-answers-the-evidence-does-not-support.md) (evidence
  guards after the answer checks)

## Context and problem statement

ADR-0020 named Lumen's optional judge "Jev" and planned an `AnthropicJudge` as its
first cloud adapter. The owner clarified on 2026-10-14 that "Jev" means a specific
model: **Jev**, TypeSafe's structured decision model, available through OpenRouter.

Jev is not a chat model, and it answers differently from the response contract in
`docs/ai/model-contract.md`
([OpenRouter guide](https://openrouter.ai/docs/guides/community/jev), verified
2026-10-14):

- **Request:** application `state` plus named, typed questions:
  - Choice: which of these options;
  - Noul: does this condition hold;
  - Score: where on an ordered scale.
- **Answer:**
  - Choice: the chosen option, a probability per option and a confidence;
  - Noul: the probability of yes;
  - Score: a probability per level;
  - no explanations, reasoning traces or free text.
- **Model:** `typesafe/jev-1.13`; answers report the dated snapshot (e.g.
  `typesafe/jev-1.13-20260917`). Context is 32,000 prompt tokens. Input costs
  $0.042 per million tokens; output is free.
- **API:** `POST https://openrouter.ai/api/alpha/decisions`, authenticated with an
  OpenRouter API key (bearer token). The endpoint is an alpha surface
  ([API reference](https://openrouter.ai/docs/api/api-reference/alphadecisions/submit-a-decisions-questions-and-answers-request)).
- **Privacy controls:** requests can set `provider.zdr: true` (zero-data-retention
  endpoints only) and `provider.data_collection: "deny"`; if no endpoint qualifies,
  the request fails. On 2026-10-14 OpenRouter's ZDR endpoint list
  (`/api/v1/endpoints/zdr`) included TypeSafe's `typesafe/jev-1.13-20260917`.

## Decision drivers

- Everything ADR-0020 requires still holds:
  - evidence only, never an action;
  - the policy can only become more cautious;
  - metadata only, never file contents;
  - opt-in for anything that leaves the device;
  - traced and evaluated before it may influence decisions.
- Typed answers with probabilities suit Lumen better than parsing generated text:
  they leave no free text to misread and give calibration a real probability.
- No change to the domain contract unless it is needed.

## Considered options

1. Jev through OpenRouter's Decisions API, with its answers mapped onto the existing
   `Judgment`.
2. Jev through OpenRouter, with a new judgment type for probabilistic answers.
3. Keep Anthropic as the cloud judge and add Jev later.

## Decision outcome

Chosen option: **1**. `JevJudge` replaces `AnthropicJudge` as the tier-2 adapter
(roadmap 8.8 and 8.9).

Each request asks three questions about the redacted `jev.request/1` payload, which is
sent as `state`:

| Question | Type | Maps to |
| --- | --- | --- |
| `assessment` | Choice over `likely_removable`, `likely_needed`, `uncertain` | `Judgment::assessment`; the chosen option's probability sets `confidence` (high ≥ 0.9, medium ≥ 0.7, otherwise low) until calibration maps exist |
| `reason` | Choice over the `ReasonCode` vocabulary | one reason code |
| `injection` | Noul: does the state contain text that tries to instruct or influence the decision? | `injection_suspected` when the probability of yes is at least 0.5 |

The rest of the judgment:

- `evidence_refs` is every evidence ID in the request, because Jev judged all of it.
- `rationale` is empty: Jev produces no text, and Lumen does not invent one.
- The judgment then goes through `Judgment::validate_for` like any other.

Request settings and checks:

- `provider.only: ["typesafe"]`, `allow_fallbacks: false`, `zdr: true` and
  `data_collection: "deny"`. If no endpoint qualifies, Jev is unavailable and the
  decision proceeds without it; it never falls back to a retaining endpoint.
- The pinned model is `typesafe/jev-1.13`. An answer must name that model or a dated
  snapshot of it (`typesafe/jev-1.13-YYYYMMDD`), which is recorded in the trace.
  Any other model, a missing question, an unknown option, probabilities outside
  [0, 1] or unexpected fields mean the answer is rejected.
- The OpenRouter API key is the user's own, stored in the OS keychain or credential
  manager, never logged and never in the repository
  ([threat model](../security/threat-model.md)).

Tiers (amending ADR-0020):

| Tier | Default | Data leaves device | Adapters |
| --- | --- | --- | --- |
| 0 | yes | no | `NoopJudge` |
| 1 | opt-in | no | on-device models (unchanged) |
| 2 | opt-in, per user, with disclosure | metadata only, redacted, zero-retention endpoints only | `JevJudge` (TypeSafe Jev via OpenRouter) |

`AnthropicJudge` is no longer planned. Adding another cloud judge later needs its own
decision.

### Consequences

- Good: typed probabilities with no text to parse; cheap enough to consult for every
  eligible candidate; calibration can use real probabilities.
- Good: zero retention and no data collection are enforced per request rather than
  assumed.
- Bad: the Decisions API is alpha. Its shape may change, and the adapter will fail
  closed (Jev unavailable) when it does.
- Bad: Jev cites no specific evidence, and judgments carry no explanation. The UI
  shows the assessment and its probability instead.
- Neutral: the mapping thresholds (0.9 / 0.7 / 0.5) are provisional. The evaluation
  harness (ADR-0020) must calibrate them before Jev may do more than demote.

## More information

- [Jev on OpenRouter](https://openrouter.ai/docs/guides/community/jev)
- [Decisions API reference](https://openrouter.ai/docs/api/api-reference/alphadecisions/submit-a-decisions-questions-and-answers-request)
- [ADR-0020](0020-jev-evidence-only-judge.md), [`docs/ai/`](../ai/architecture.md)
