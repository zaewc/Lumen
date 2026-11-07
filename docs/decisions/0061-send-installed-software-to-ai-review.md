# ADR-0061: While AI review is on, it also explains installed software

- Status: Accepted (owner decision, 2026-11-07)
- Date: 2026-11-07
- Supersedes in part: [ADR-0060](0060-list-software-and-remove-it-only-through-its-own-mechanism.md)
  rule 9 ("the software list stays on the device")
- Refines: [ADR-0050](0050-ask-about-ai-review-on-first-launch.md) (what AI review
  sends, and how that is disclosed), [ADR-0020](0020-jev-evidence-only-judge.md)
  (AI output is evidence)
- Issue: #808

## Context and problem statement

#808 asks Jev to explain installed software:
- what it is for;
- where it came from;
- what runs in the background because of it;
- what removing it could affect.

That needs software metadata at the model. ADR-0060 kept the list on the device.
ADR-0050's disclosure describes only the items Lumen proposes or asks about.

Two ways were considered:
- send one app's details when the person asks for an explanation;
- send the list whenever AI review is on.

The owner chose the second (2026-11-07). This ADR records that choice and the limits
it comes with.

## Decision

1. **When.** While AI review is on, Lumen sends the software list for explanation.
   - Nothing is sent while it is off.
   - Turning it off stops sending, including requests that have not started.
   - The choice and its off switch are the ones ADR-0050 defines; there is no second
     switch.
2. **What, per item.** Only these, as untrusted text in a closed request vocabulary:
   - origin (installer or package manager);
   - native identifier;
   - display name;
   - publisher;
   - version;
   - whether the platform marks it protected or hidden;
   - the kinds and labels of the background items linked to it by evidence (#807),
     with their verified or heuristic state.

   Never sent:
   - install paths, or any path;
   - user or account names;
   - usage signals (last opened, running; ADR-0059);
   - sizes;
   - anything read from files.
3. **How often.** Each item is sent at most once per origin, identifier and version.
   - The answer is cached on the device.
   - A new version is a new item.
   - Requests are batched and bounded, as AI review already is.
4. **Disclosure first.** The pull request that starts sending also updates these, in
   every locale, in the same change:
   - the first-launch question;
   - the AI review description in Settings;
   - the privacy line in Settings.

   Each must say that installed software names, publishers and versions are sent
   while AI review is on. The release notes of the version that starts sending say
   so too.

   Consent given before that release extends to the software list without a new
   question or notice (owner decision, 2026-11-07). Someone who turned AI review on
   earlier starts sending it after updating. The updated descriptions and the
   release notes are how they learn this, and the off switch stops it.
5. **What the answer may do.** The explanation is evidence and description only
   (ADR-0020).
   - It never selects software to remove, a mechanism, or an argument.
   - It never marks software safe to remove.
   - It never overrides a protected item.
   - It never turns a heuristic link into a verified one.

   Observations, sourced knowledge and the model's inference are shown apart, and
   what is unknown is said.
6. **No silent uploads elsewhere.** The list stays out of telemetry and logs. Only the
   AI service the person chose receives it, under ADR-0050's terms.

## Consequences

- Good: explanations are ready when the person opens the Apps page, with no extra
  step.
- Bad: while AI review is on, the names and versions of installed software leave the
  device. They describe a person's tools, interests and sometimes employer.
  - The chosen AI service and, for Jeb, its operator may keep them.
  - The updated descriptions, the release notes and the off switch are the
    mitigations.
  - Consent given before the change is extended without asking again. People who
    do not read the release notes or Settings start sending their software list
    without being told in the app. The owner accepted this to avoid interrupting
    them.
  - The data sent is limited to identity fields; nothing else is sent.
- Neutral: on-demand explanation remains possible later as a narrower mode if people
  prefer it.

## More information

- Owner decisions, 2026-11-07:
  - send automatically while AI review is on, not on demand;
  - small-scale Jeb evaluation runs allowed (one or two runs over a few dozen
    fictional software entries);
  - existing consent extends to the software list, with no in-app notice. An
    earlier draft required a one-time notice before anything was sent; the owner
    chose not to have one.
- ADR-0060 rule 9 otherwise stands: no usage signals in requests, no telemetry.
