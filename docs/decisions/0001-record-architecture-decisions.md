# ADR-0001: Record architecture decisions

- Status: Accepted
- Date: 2026-10-06

## Context and problem statement

Lumen spans a Rust core, four platform adapters, a desktop shell, a mobile app, a web
dashboard, and an AI judgment subsystem. It will be built largely by AI coding agents
working in small, atomic pull requests over a long period. Without a durable record,
the reasons behind decisions get lost, agents re-litigate settled questions, and
platform constraints discovered during research are forgotten.

## Decision drivers

- Decisions must be discoverable by humans and by agents with limited context.
- Each decision must link to the evidence (research notes, official documentation) it rests on.
- Superseding a decision must be explicit, never silent.

## Considered options

1. MADR-style ADRs in `docs/decisions/`, one file per decision.
2. A single, continuously edited architecture document.
3. Decisions recorded only in pull request descriptions.

## Decision outcome

Chosen option: **1, MADR-style ADRs**, because one file per decision keeps reviews
small, history linear, and context loading targeted.

Rules:

- Files are named `NNNN-kebab-title.md` with a zero-padded, never-reused number.
- Each ADR has: status, date, context, decision drivers, considered options, decision
  outcome, consequences, and links to supporting research.
- Status is one of `Proposed`, `Accepted`, `Superseded by ADR-NNNN`, or `Deprecated`.
  `Proposed` marks decisions that need the project owner's confirmation; work may
  proceed on the recommended option, but the decision may still change.
- Accepted ADRs are immutable except for status changes and link fixes. A changed
  decision gets a new ADR that supersedes the old one.
- Every ADR is added in its own pull request together with its line in
  [the index](README.md).

### Consequences

- Good: agents load one small file to understand one decision.
- Good: the research → decision → implementation chain stays traceable.
- Bad: cross-cutting summaries must be maintained separately in
  `docs/architecture/system.md`.

## More information

- Template: [`0000-template.md`](0000-template.md)
- MADR: <https://adr.github.io/madr/>
