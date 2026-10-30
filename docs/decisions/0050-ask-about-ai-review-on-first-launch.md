# ADR-0050: Ask about AI review once, on first launch

- Status: Accepted (owner decision, 2026-10-31)
- Date: 2026-10-31
- Supersedes in part: [ADR-0047](0047-ai-review-on-by-default.md) (on without a
  choice); its disclosure and kept off switch stand

## Context and problem statement

Since ADR-0047, AI review is on whenever no choice is saved next to `index.db`. A
review of the project on 2026-10-30 found three problems with that:

- **Users who updated were switched on without seeing anything.** Before ADR-0047 a
  user who never touched the setting had no `ai-review.json`, which meant off. After an
  in-app update (ADR-0048) the same missing file means on. These users never saw the
  download page again, so nothing told them before their metadata was sent.
- **The download page cannot carry the choice.** Not every install passes through it
  (GitHub Releases, shared links, source builds), and a click on a web page leaves no
  record in the installed app. The owner asked whether consent could be collected
  there; it can be disclosed there, not chosen.
- **What is sent can identify a person.** Paths and file names (for example
  `~/Downloads/passport_KimJ.pdf`) go to the school's Jeb server, whose operator may
  keep requests and publishes no retention policy (ADR-0036). Whether that needs
  consent under privacy law was not settled; asking first is the safer side.

## Decision drivers

- Nothing leaves the device before the user has seen what is sent and where.
- The choice is recorded in the app, with its time, and can be changed at any time.
- Existing users are treated like new ones: no one is switched on by an update.
- The owner wants AI review to be part of Lumen, so the question is shown plainly to
  everyone, once, not buried in Settings.
- Every request can be declined with an equal, visible choice
  ([ux-writing](../design/ux-writing.md)).

## Considered options

1. **Ask once on first launch; nothing is sent until the user chooses.**
2. **Consent on the download page.** Rejected: it misses installs that do not pass
   through the page and leaves no record in the app.
3. **Keep ADR-0047.** Rejected: it switched updating users on without a word.
4. **Show a notice on first launch with AI review already on.** Rejected: closing the
   notice is not a choice, and the record would not show one.

## Decision outcome

Chosen option: **1**.

- **Not chosen yet:** with no `ai-review.json`, the user has not chosen. AI review is
  off and nothing is sent. `lumen_app::ai::choice` returns `None`, `load` reports off,
  and `AiSettingsView.chosen` is `false`. A file that exists but cannot be read still
  counts as a choice of off, so a damaged opt-out is neither sent around nor asked
  again.
- **Desktop app:** while `chosen` is `false`, the first screen is the question, in
  place of the usual first page: what AI review does (it can only make a decision more
  careful, and adds a note to items left for review), what is sent (paths with the
  home folder as `~`, file names, sizes, ages and the evidence; never file contents),
  where (the school's Jeb server, whose operator may keep requests) and with whose key.
  Two equal buttons record the choice: turn AI review on, or continue without it. It
  is a page, not a dialog: it takes no focus, and it is the explicit exception to the
  launch rule in [ux-writing](../design/ux-writing.md). Builds without a built-in key
  ask for a key with the "on" choice, as Settings does.
- **Changing it later:** Settings keeps the switch (ADR-0037). Turning it on records
  the time (`consented_at`); turning it off keeps the last consent on record.
- **Existing users:** a saved choice stands. Anyone without one, including users of
  0.8.x who were on by default, sees the question at their next launch and sends
  nothing before answering.
- **Command line:** no question; `lumen ai status` says AI review is off until the user
  runs `lumen ai on` or `lumen ai off`. `--jev` still asks the judge for one run, with
  the disclosure printed.
- **Disclosure stays:** Settings, the scan's start panel and the sidebar keep saying
  whether AI review is on (ADR-0047). The download site and README say that Lumen asks
  on first launch, and what is sent and where.

### Consequences

- Good: no metadata leaves a device before its user chose, and the choice is on record.
- Good: updating users are no longer switched on silently.
- Bad: fewer users will have AI review on than under ADR-0047.
- Bad: one question stands between a new user and their first scan.
- The threat model's disclosure control becomes "asked on first launch, recorded, with
  a kept off switch"; redaction and metadata-only are unchanged.

## More information

- [ADR-0037](0037-keep-ai-review-on-once-turned-on.md): the kept choice and its record.
- [ADR-0047](0047-ai-review-on-by-default.md): the default this replaces.
- [ADR-0042](0042-ship-a-built-in-jeb-key-in-release-builds.md): the built-in key the
  question names.
- [Privacy](../operations/privacy.md) and the
  [threat model](../security/threat-model.md).
