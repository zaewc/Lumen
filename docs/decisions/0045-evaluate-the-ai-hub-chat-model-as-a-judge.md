# ADR-0045: Evaluate the AI Hub's chat model as a judge

- Status: Accepted
- Date: 2026-10-27
- Refines: [ADR-0044](0044-evaluate-an-on-device-judge-with-foundation-models.md)
  (option 4, free-text answers) for one cloud model; carries out part of roadmap 14.6

## Context and problem statement

Roadmap 14.6 compares judges. The on-device model failed the `dev` split of dataset
2026.11.0 by a wide margin (critical-FPR upper bound 22.19%, 42 canary flips;
[report](../../ai/reports/fm-decisions/1/apple-fm-2026.11.0-dev.md)), and Jeb with the
best prompt so far still has an upper bound of 2.12% on 2026.10.1. The school's AI Hub,
which serves Jeb (ADR-0036), also serves a general chat model, `flash-next`, through an
OpenAI-compatible `POST /v1/chat/completions` with the same account key. From the Hub's
own documentation (its web app, read 2026-10-27):

- **Engine:** a llama.cpp-compatible server. It does **not** accept output constraints:
  `response_format` (`json_object`, `json_schema`), `grammar` or `json_schema` are
  refused with HTTP 400 (not charged). The documentation says to ask for JSON in the
  prompt and validate what comes back.
- **Context and concurrency:** 200,000 tokens per request, 8 slots for the service;
  text only.
- **Thinking:** `chat_template_kwargs.enable_thinking` turns the model's thinking
  phase on or off.
- **Answers:** text in `choices[0].message.content` with a `finish_reason`; no
  probabilities (`logprobs` is not supported).

ADR-0044 rejected free-text answers for the on-device model, because guided generation
made every answer a closed value there. This model has no such option. The question
is whether, and how, to measure it.

## Decision drivers

- AI output is evidence and can only make a decision more cautious (ADR-0020).
- "Never parse free-form model text as instructions" (AGENTS.md): no model text may
  steer anything, be shown, or be stored outside an evaluation cassette.
- Replay must apply exactly the checks recording applied, including the evidence
  guards (ADR-0043).
- Requests are metadata only; dataset requests are synthetic.
- The owner's quota: every bulk recording needs the owner's go-ahead.

## Considered options

1. **Ask for one JSON object and accept only a whole answer that is exactly that
   object with closed values** (the decision below).
2. **Extract JSON from anywhere in the text** (the first `{...}`, or keys found by
   pattern). Rejected: it would accept answers wrapped in prose the model chose to
   write, which is parsing free-form text.
3. **Do not evaluate it.** Rejected: the comparison is cheap to build on ADR-0044's
   pieces, and a model that does well would change what Phase 14 should try next.

## Decision outcome

Chosen option: **1**, for evaluation only.

- **Prompt `chat-decisions/1`:** the system message is `fm-decisions/1`'s instructions
  (prompt v2's rules) and one more paragraph that asks for exactly one JSON object
  with three keys, `injection_suspected` (a boolean), `reason` (one of the 17 keys) and
  `assessment` (one of the 3), in that order, and nothing else. The user message is the
  item exactly as the on-device helper renders it (the request without schema version
  and IDs, as compact JSON after one English sentence). Thinking is off; `temperature`
  0; at most 128 answer tokens. A test keeps the instructions equal to the Swift
  helper's.
- **Validation** (`lumen_jev::chat`): the response must be a chat completion from the
  pinned model `flash-next` with exactly one choice, `finish_reason` `stop`, and
  content that, after trimming surrounding whitespace, is exactly one JSON object with
  exactly those three keys and closed values. One Markdown code fence around the
  object (```` ```json ```` … ```` ``` ````), with nothing else outside it, is accepted,
  because the model may add one by habit; any other text, a missing or extra key, an
  unknown value or a second object makes the answer malformed. Other response fields
  (`usage`, `id`, and `reasoning_content` if the model thinks despite the switch) are
  not read. Content is never logged or shown; only the closed values leave the parser. The judgment cites every sent
  evidence ID, has an empty rationale and `medium` confidence for every answer (no
  probabilities, as in ADR-0044), and the ADR-0043 guards apply.
- **Recording** (`lumen-jev-eval record --judge hub-chat`): the account key from the
  keychain slot Jeb uses, never printed; one request at a time, 1 s apart, with Jeb's
  retries (ADR-0036). The cassette records the raw response body, as for Jeb, and its
  descriptor is provider `gsm.chat`, model `flash-next`, prompt `chat-decisions/1`.
- **Replay** (`run`) validates recorded answers through the same function.
- **Not in the app:** nothing in `lumen-app`, the CLI or the desktop app asks this
  judge. Using it in production would need its own decision, after its results.

### Consequences

- Good: a third judge on the same datasets, with the same strict grading, for the
  cost of one adapter.
- Good: an answer that is anything but the closed object counts as malformed, so
  prose, refusals and injected text can never become a judgment.
- Bad: an unconstrained model can fail the format; those cases are unanswered, and an
  unanswered critical case fails the gate, so format errors count against it.
- Bad: no probabilities, so the fixed `medium` confidence is the strictest grading and
  calibration would need self-consistency (roadmap 14.5).
- Neutral: the Hub may change the model behind the name; the response's `model` field
  is pinned, and a change needs a new recording.
