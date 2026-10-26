# ADR-0041: AI describes what review items are

- Status: Accepted
- Date: 2026-10-26
- Amends: [ADR-0036](0036-jeb-on-the-school-ai-hub.md) and
  [ADR-0037](0037-keep-ai-review-on-once-turned-on.md) (what AI review sends, and about
  which items); carries out point 4 of
  [ADR-0039](0039-ask-for-review-only-where-a-person-can-act.md)

## Context and problem statement

After ADR-0039 and ADR-0040, "Needs review" holds only a few items on the owner's Mac:
things Lumen does not recognise but which are large enough to matter, and the leftovers
of uninstalled apps. Lumen says *why* each needs review ("Lumen does not recognise this
item") but not *what* it probably is, which is exactly what the person has to work out
before deciding. The owner chose (ADR-0039, point 4) to let optional AI review help
with that.

Until now the judge is asked only about proposed quarantines (ADR-0037), because a
judgment can only demote those, and asking about anything else could never change a
decision. A description of a review item changes no decision either: it is for the
person. The question is how to add it without letting AI output decide anything, and
without sending more than the user agreed to.

## Decision drivers

- AI output is evidence and stays out of actions (ADR-0020): it never selects actions,
  paths or commands, and by default it can only make a decision more cautious.
- Cloud AI stays opt-in and metadata-only; the disclosure must say exactly what is
  sent, about which items.
- No free text from a model is shown or parsed (AGENTS.md).
- Jeb allows one request at a time and has per-account quotas (ADR-0036).

## Considered options

1. **A closed "what kind of item" question, shown as a labelled note:** the decision
   below.
2. **Ask the existing judgment questions about review items** (keep or remove).
   Rejected: "likely removable" on an item the policy will not propose invites the
   person to delete it by hand on a model's word, and the answer cannot change the
   decision anyway.
3. **A free-text summary from the model.** Rejected: free text from attacker-influenced
   input cannot be validated, and Jeb's protocol returns no text.
4. **No AI for review items.** Rejected by the owner (ADR-0039, point 4).

## Decision outcome

Chosen option: **1**.

- **When:** only while AI review is on (ADR-0037 consent, or a run's `--jev`). With AI
  review off, nothing is asked.
- **Which items:** items the policy itself left at `REVIEW` (the judge was not
  consulted about them) that no knowledge-base entry identifies; Lumen already knows
  what knowledge-base matches are, and shows their tool's command. At most the
  **20 largest** such items are described per assessment (by the larger of allocated
  and reclaimable size, then by path). The CLI says when more were left undescribed.
- **What is sent:** the same metadata-only, redacted `jev.request/1` payload as for
  proposed items (home shown as `~`, sizes, ages, evidence facts; never file
  contents), as Decisions `state`, with two questions (prompt version
  `jev-describe/1`):
  - `kind` (`choice`): "What kind of item is this most likely?" over a fixed set:
    `app_settings`, `user_documents`, `cache`, `logs`, `downloads_or_installers`,
    `developer_tool_data`, `backup_or_archive`, `unknown`;
  - `injection` (`noul`): the same injection question as for judgments.
- **How the answer is checked:** as strictly as a judgment (`judgment_for`): only the
  provider's pinned model, exactly these two answers, known options only,
  probabilities within [0, 1], no unexpected fields. Anything else is dropped. The
  confidence is the chosen option's probability in the same buckets as judgments
  (`high` from 0.9, `medium` from 0.7, otherwise `low`). An answer that suspects
  injection is kept in the trace but never shown.
- **What it does:** it is shown as a note only, labelled as AI ("AI: likely a cache
  (low confidence)"), in the item's details and under its row. It never changes the
  verdict, never reaches the policy, and never creates a plan entry; plans are made
  from the policy's decisions alone, and `lumen plan` asks nothing. Only the
  enumerated kind and confidence are shown, never model text.
- **Limits:** requests are sent one at a time with the judge's existing 30-second
  timeout. The first unavailable answer (unreachable, timed out, rate limit or quota)
  ends describing for that assessment, so a busy server is not asked again and again.
- **Traces:** every description request is recorded in the ledger like a judgment
  (`jev.trace/1`, output schema `jev.item-note/1`, with the note and no judgment;
  policy effect `none`).
- **Disclosure:** every place that says what AI review sends (the CLI's per-run
  notice and `lumen ai on`, the app's consent dialog and settings text,
  `docs/ai/architecture.md`) now says that metadata of proposed items **and of up to 20
  items left for review** is sent.

### Consequences

- Good: a person looking at a review item gets a hint of what it is, without anything
  being decided for them.
- Good: no new path from AI output to an action: the note is not an input to the
  policy, the plan or the executor, and the tests check that a note never changes a
  verdict.
- Bad: more metadata leaves the machine while AI review is on: up to 20 more requests
  per assessment, to a server whose operator may keep them (Jeb). The disclosure says
  so before consent.
- Bad: the answer is uncalibrated; the confidence is shown as the model's own, and a
  wrong note may mislead. It is labelled as AI, and the verdict and reasons stay
  Lumen's.

## More information

- [ADR-0020](0020-jev-evidence-only-judge.md): AI is evidence only.
- [ADR-0036](0036-jeb-on-the-school-ai-hub.md), [ADR-0037](0037-keep-ai-review-on-once-turned-on.md):
  the judge and the consent.
- [ADR-0039](0039-ask-for-review-only-where-a-person-can-act.md): what is left for review.
- `crates/lumen-jev/src/decisions.rs` (`describe_body_for`, `note_for`),
  `crates/lumen-application/src/describe.rs`.
