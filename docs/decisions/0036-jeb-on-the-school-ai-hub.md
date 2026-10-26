# ADR-0036: Use Jeb on the school AI Hub as the default judge

- Status: Accepted
- Date: 2026-10-21
- Amends: [ADR-0031](0031-jev-on-typesafe-via-openrouter.md) (provider and retention)
- Amended by: [ADR-0041](0041-ai-describes-what-review-items-are.md) (what is sent, about which items)

## Context and problem statement

ADR-0031 connects the optional judge to TypeSafe's Jev through OpenRouter's Decisions
API, on zero-data-retention endpoints only. The owner does not use OpenRouter: their
key is for **Jeb**, a decision model their school serves on its AI Hub
(`https://jeb.https.gsmsv.site`, sign-in with a DataGSM account, keys issued per
student). OpenRouter rejects that key, so `--jev` never reached a model.

The AI Hub's own documentation (its web app, read on 2026-10-21) describes the
endpoint:

- `POST /v1/systemone`, `Authorization: Bearer <key>`, model `jeb`;
- the request has `model`, `state` (the situation to judge) and `questions`, each a
  `choice`, `noul` (yes/no) or `score` question with `instructions` and `criteria`;
- the answer returns, per question, the chosen key with its probabilities and
  confidence, or the probability of "yes"; no free text;
- one concurrent request per account (429 otherwise), about 8,000 input tokens, and
  per-account quotas.

That is the protocol Lumen already speaks to Jev (`lumen_jev::decisions`).
`GET /v1/models` with the owner's key listed `jeb` (service `decision`), and one real
request on the owner's machine came back valid in 1.1 s.

The AI Hub publishes no retention or logging policy. ADR-0031's zero-retention
routing (`provider.zdr`, `data_collection: deny`) is an OpenRouter feature with no
equivalent here.

## Decision drivers

- AI output stays evidence only, and can only make a decision more cautious
  (ADR-0020). That holds whatever the provider.
- Cloud use stays opt-in per run and metadata-only, and the user is told where the
  data goes.
- The owner's actual service must work.

## Considered options

1. **Jeb as the default provider, Jev through OpenRouter as the alternative:** the
   same questions, answer checks, calibration thresholds and traces. Only the endpoint,
   the pinned model and the routing block differ.
2. **Keep OpenRouter only**, and leave the owner without a judge.
3. **A separate Jeb adapter** with its own questions.

## Decision outcome

Chosen option: **1**.

- `lumen_jev::decisions::Provider` is `Jeb` or `OpenRouter`. Both send the redacted
  `jev.request/1` payload as `state` and the same three questions (prompt version
  `jev-decisions/1`). Both accept an answer only from their own pinned model (`jeb` or
  `typesafe/jev-1.13`, or a dated snapshot of it) and only with the expected shape;
  anything else is rejected, and the decision proceeds without the judge.
- Only OpenRouter requests carry the zero-retention routing block.
- Keys are kept in separate keychain slots (`jeb-api-key`, `openrouter-api-key`).
  `lumen jev key set` stores a Jeb key by default; `--openrouter` stores the other.
  `--jev` uses Jeb when its key is stored.
- **Retention is not guaranteed for Jeb.** Before each `--jev` run, the CLI says the
  metadata goes to the school's Jeb server and that its operator may keep requests.
  The payload is unchanged: metadata only, with home paths shown as `~`, never file
  contents.
- Requests are sent one at a time, so the single-request limit is respected. A 429,
  an exhausted quota or a busy server is reported as "unavailable", and the decision
  proceeds without the judge.

### Consequences

- Good: the owner's judge works.
- Good: one protocol, one validator and one trace format for both services.
- Bad: the school operator may log the metadata Lumen sends. This is accepted on the
  owner's behalf, with disclosure on every run; anyone who needs zero retention can use
  OpenRouter instead.
- Bad: the AI Hub's terms and model can change without notice. The pinned model name
  and the strict answer checks turn any change into "no judgment", never a different
  one.

## More information

- [ADR-0020](0020-jev-evidence-only-judge.md), [ADR-0031](0031-jev-on-typesafe-via-openrouter.md)
- `docs/ai/architecture.md` (tiers)
