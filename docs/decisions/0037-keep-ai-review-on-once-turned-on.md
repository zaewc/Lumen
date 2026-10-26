# ADR-0037: Keep AI review on once the user turns it on

- Status: Accepted
- Date: 2026-10-21
- Amends: [ADR-0020](0020-jev-evidence-only-judge.md), [ADR-0036](0036-jeb-on-the-school-ai-hub.md) (how consent is given)
- Amended by: [ADR-0041](0041-ai-describes-what-review-items-are.md) (review items are also described)

## Context and problem statement

Cloud AI review is opt-in (AGENTS.md, ADR-0020): nothing leaves the machine unless the
user asks. Until now the only way to ask was per run: `lumen assess --jev`. The
desktop app had no way to ask at all. The owner wants AI review "on by default".

Turning it on for everyone without asking would break the opt-in rule. Asking on every
run makes it impractical.

## Decision drivers

- Opt-in stays opt-in: the user chooses, knowingly, before any metadata is sent.
- Once chosen, the choice should hold, like any other setting.
- Turning it off must be as easy as turning it on.

## Considered options

1. **Ask once, then keep it on:** an explicit "turn on AI review" action with the same
   disclosure, stored as a setting with the time of consent. Later assessments use the
   judge without asking again, until the user turns it off.
2. **On for everyone by default.** Rejected: it is not opt-in.
3. **Per run only** (the status quo).

## Decision outcome

Chosen option: **1**.

- The setting lives next to the database (`ai-review.json`: `enabled` and
  `consented_at`). It starts off. A missing or unreadable file means off.
- **Turning it on:** `lumen ai on --db <FILE>` in the CLI, or the AI review switch in
  the app. Either shows where metadata goes before saving the choice; for Jeb, that is
  the school's server, whose operator may keep requests. It needs a stored key.
- **While on:** `assess` and `plan` use the judge without `--jev`.
  `--no-ai` skips it for one run. `lumen ai off` (or the switch) turns it off.
- **Nothing else changes:**
  - metadata only, never file contents;
  - only proposed quarantines are asked about;
  - every answer is validated;
  - a judgment can only make a decision more cautious;
  - any failure means the decision proceeds without it.

### Consequences

- Good: the owner's "on by default" works, and consent is still explicit and on record.
- Bad: a user who turned it on months ago keeps sending metadata until they turn it
  off. The app shows the setting on the start page, and the CLI repeats where data goes
  on every run.

## More information

- [ADR-0020](0020-jev-evidence-only-judge.md), [ADR-0036](0036-jeb-on-the-school-ai-hub.md)
