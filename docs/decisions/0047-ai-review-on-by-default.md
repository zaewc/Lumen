# ADR-0047: AI review on by default

- Status: Accepted
- Date: 2026-10-28
- Supersedes: the opt-in default of [ADR-0020](0020-jev-evidence-only-judge.md) (tier 2)
  and [ADR-0037](0037-keep-ai-review-on-once-turned-on.md) (AI review starts off and is
  turned on with consent)

## Context and problem statement

AI review sends the metadata of proposed items and of up to 20 large items left for
review (paths with the home folder as `~`, sizes, ages, evidence facts; never file
contents) to the school's Jeb server (ADR-0036). So far it has been off until the user
turned it on in a consent dialog (ADR-0037), as AGENTS.md required: "Cloud AI is
opt-in". Release builds carry a built-in key (ADR-0042), so the only step left between
a new user and AI review is that dialog.

The owner decided on 2026-10-28, after the conflict with the opt-in rule was put to
them, that every AI feature is on by default, without asking first, and that the app
says it can be turned off.

## Decision drivers

- The owner's product decision: AI is part of what Lumen is.
- What the user is told must stay true and visible: what is sent, where, and how to
  stop it.
- AI output stays evidence only: it can only make a decision more cautious, and review
  notes never change a verdict (ADR-0020, ADR-0041). Nothing about what the AI may do
  changes here, only whether it runs by default.

## Considered options

1. **On by default, with the disclosure always visible and a one-click off switch**
   (the decision below).
2. **Ask once at first launch, with "Turn on" as the main button.** Rejected by the
   owner.
3. **Keep it off until turned on** (ADR-0037). Rejected by the owner.

## Decision outcome

Chosen option: **1**.

- **Default:** with no saved choice (`ai-review.json` absent), AI review is on. A saved
  choice wins: a user who turned it off stays off. A choice file that exists but cannot
  be read counts as off, so a damaged opt-out never turns into sending.
- **Disclosure:** wherever AI review runs or is shown, the app says it is on, what is
  sent and where, and that it can be turned off: the AI review section of Settings, the
  scan's start panel and the sidebar status. The command line keeps printing the
  disclosure on every run. The download site and release notes say it is on by default
  and can be turned off.
- **Off switch:** one click in Settings (or `lumen ai off`); the choice is kept.
  Turning it back on needs no new consent; the dialog still explains what is sent.
- **Without a key** (development builds have none built in), AI review stays on but
  cannot run; the app says so, as before.
- **AGENTS.md** now reads: cloud AI is metadata-only, on by default with the disclosure
  shown and a visible off switch (this ADR).

### Consequences

- Good: every user gets the review notes and the extra caution without a setup step.
- Bad: metadata of every user's proposed and largest review items goes to the school's
  Jeb server unless they turn it off; the operator may keep requests and publishes no
  retention policy. This is disclosed, not prevented.
- Bad: users on a network that blocks the Hub see AI review as unavailable.
- The threat model's disclosure control changes from "per-user opt-in" to "disclosure
  and an off switch"; the redaction and metadata-only controls are unchanged.
