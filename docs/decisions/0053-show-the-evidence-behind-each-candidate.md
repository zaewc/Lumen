# ADR-0053: Show the evidence behind each candidate

- Status: Accepted (owner decision to expose the core's evidence before the redesign,
  2026-11-01; field design by the lead, open to the owner's revision)
- Date: 2026-11-01
- Amends: [ADR-0011](0011-contracts-and-schema-sharing.md) (what `CandidateView`
  carries)

## Context and problem statement

The redesign in [`visual-language.md`](../design/visual-language.md) builds an
inspector that reads as evidence lines leading to a decision (the "evidence stack"),
and a view that highlights the items related to the one being inspected (the
"evidence lens"). Both need evidence the UI does not receive. `CandidateView` carries a
path, the verdict, the reclaimable size, the fired rules, the tool command, the group,
the AI note and the apps to close. The core holds more for every decision
([evidence graph](../architecture/evidence-graph.md)):

- **Facts** in the candidate's evidence bundle (`PolicyInput.evidence`): owning
  application and whether it is installed, application data location, knowledge-base
  match, vendor-declared cache, protection flags, cloud backing, browser profile,
  large unrecognised folder, and the answers probed at assessment time (in use, owned
  by the current user). Each has a basis (observed, or concluded from absence with the
  sources searched), a source and an observation time.
- **Times:** `LastModified` and `LastAccessed` facts, from the scanned entry's
  modification and access times. The scanner also stores creation and status-change
  times, but they are not facts and never reach a decision.
- **Relations:** the relationships within two hops of the candidate
  (`BUNDLE_DEPTH`), which the bundle's evidence hash covers. Today the only producer is
  `lumen-platform-macos::support`, so in practice they are `owned_by` edges: the
  candidate to its application, and (at two hops) the application's other folders.
  Windows produces no edge (a Disk Cleanup handler name does not identify an
  application).

How much of this should the UI see, and how is uncertainty carried so a person never
reads more into a value than the platform can vouch for?

## Decision drivers

- Unknown is never absent, empty or zero ([ADR-0008](0008-capability-model-and-coverage.md)).
  Access times are weak evidence: Linux defaults to `relatime` (an access updates the
  time only when it is older than the modification or change time, or more than a day
  old; `noatime` stops updates), macOS may skip access-time updates unless a volume is
  mounted `strictatime` (`mount(8)`), and NTFS can be told not to update them
  (`NtfsDisableLastAccessUpdate`) and defers writing them by up to an hour
  ([`fsutil behavior`](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/fsutil-behavior)).
  Lumen does not read those settings, so it cannot tell which applies.
- A folder's own times change when entries are added, removed or renamed in it, not
  when a file inside changes. They never say when its contents were last used.
- Facts are evidence, not confidence: the policy decides by rules
  ([ADR-0014](0014-deterministic-safety-policy-engine.md)), and a score would invite
  people to read a probability into it.
- No new IO: nothing is scanned, opened or hydrated for this; domain, policy and graph
  crates stay IO-free.
- AI output is evidence only ([ADR-0020](0020-jev-evidence-only-judge.md)), and model
  input is metadata-only and chosen by the user
  ([ADR-0050](0050-ask-about-ai-review-on-first-launch.md)).
- The UI keys rows by path today; a relation needs a stable way to name another
  candidate.

## Considered options

1. **Expose the decision's own evidence, typed, with its certainty:** times as states,
   facts with basis and source, the bundle's relations with stable node IDs, bounded.
2. **Expose the raw `Evidence` and `Relationship` records** (canonical JSON). Rejected:
   it ties the UI to internal serde forms, sends unescaped untrusted text and evidence
   hashes no person reads, and every domain change would break the contract.
3. **Expose derived summaries** ("last used 3 months ago", "related items: 4",
   a confidence score). Rejected: "last used" overclaims what access and folder times
   say, a count hides which items, and a score is a probability the policy never
   computes.
4. **Compute more evidence for the UI** (newest time below a folder, creation times as
   facts, process names for every in-use answer). Rejected for now: each is new
   derivation or a change to what decisions see and hash, so each needs its own
   decision.

## Decision outcome

Chosen option: **1**. `CandidateView` gains four fields, all built in `lumen-api` from
`Assessed` with no new IO:

- **`id`:** an opaque string made from the candidate's file identity (volume and file
  ID), never from its path. It is stable within an assessment, and across scans and
  renames while the platform reports the same volume and file IDs (a macOS device
  number can change when a volume is mounted again). Two hard links to one file share
  it, because they are one object. Rows stay keyed by path; `id` is for relations.
- **`times`:** `modified` and `accessed`, each one of:
  - `recorded` with `at` (modification time, as the platform reported it);
  - `may_lag` with `at` (access time: the platform may skip or defer updates, so the
    real last access may be later; never proof of disuse);
  - `not_recorded`: the metadata was read but the platform gave no value, or a value
    at or before the Unix epoch, which filesystems report when they keep none;
  - `not_read`: the metadata was not read, so the time is unknown.

  A `folder` flag says the times are a folder entry's own, not its contents'. Times
  come from the decision's `LastModified`/`LastAccessed` facts, so the UI shows what the
  decision saw.
- **`facts`:** each fact about the candidate in its bundle, plus whether each of its
  owning applications is installed, as a closed tagged union (owning application,
  application present, app data location, knowledge-base match, vendor-declared cache,
  protection flags, cloud-backed, browser profile, large unrecognised, in use, not in
  use by visible processes, owned by current user), each with its `basis` (`observed`,
  or `absence` with the sources searched), `source` and `observed_at`. Times are in
  `times`, not repeated here. Code signatures are not shown: no producer records them
  for files today. A fact kind the view does not know is left out, not guessed. For the questions asked of every candidate (in use, owned by the current
  user, owner installed), a missing fact means the probe or inventory could not
  answer: **unknown, never "no"**. Facts about other subjects belong to their own
  views.
- **`relations`:** the bundle's relationships, as `{from, kind, to, hops}`, where a
  node is a file (by the same `id`, so the UI can highlight another candidate), an
  application (its escaped identifier), a process (its ID), a service (its escaped
  label) or a volume. `hops` is 1 for an edge that touches the candidate and 2
  otherwise. At most 16 edges, the nearest first in a fixed order, with an `omitted`
  count for the rest (and for any edge of a kind the view does not know), so a bound
  never reads as "no more". Counts and process IDs are decimal strings, like every
  count in the API. Only edges the graph has;
  none is inferred from paths, names or groups. A file node that is not a candidate is
  shown without a path: the assessment does not hold one.

**Not exposed**, and why:

- Creation and status-change times: stored by the scanner but not facts, so no
  decision sees them; status-change time also changes on rename and means little to
  people. Adding creation time as a fact would change evidence hashes and model input,
  so it needs its own decision.
- A derived "last used", or the newest time below a folder: see option 3 and 4.
- Evidence IDs and the evidence hash: they serve replay and audit, not people.
- Paths of related files that are not candidates, and process names beyond
  `free_when_closed`: the assessment does not hold them; `opened_by` edges are not
  produced yet.
- Any numeric confidence or score.

`ai_note` is unchanged: a closed kind and a confidence, never model text
([ADR-0041](0041-ai-describes-what-review-items-are.md)).

### Consequences

- Good: the evidence stack and lens can show what the decision rested on, with
  certainty the data itself states, and no relation the graph does not have.
- Good: facts and times are typed and escaped; the contract does not move with
  internal serde forms.
- Bad: `CandidateView` grows; the assessment response for hundreds of candidates is
  larger (bounded per candidate).
- Bad: on Windows `relations` is empty today, and on macOS it holds only `owned_by`
  edges. The UI must treat an empty list as "no relation recorded", not "unrelated".
- Bad: `Assessed` carries the bundle's relationships, which the view needs and the
  assessment otherwise discarded.

### Security

- Nothing here changes what is sent to a model. The Jev request is built from
  `PolicyInput` in `lumen-jev`, as before; these fields are built from `Assessed`
  after the decision and travel only from the host to the UI (Tauri IPC, or the
  loopback dashboard). Times already reach the model as whole days, as before, and
  only when the user has chosen AI review.
- Untrusted text (application identifiers, vendor names, service labels, browser
  profile markers) is escaped with the same display rules as paths. No value is ever
  handed back to act on: `id` names a row for highlighting, never a file to open or
  move.
- Process IDs and file identities are local metadata of the user's own machine, at
  the same trust level as the paths the UI already shows.

## More information

- [Evidence graph](../architecture/evidence-graph.md): facts, relationships, bundles.
- [`visual-language.md`](../design/visual-language.md): the evidence stack, the
  evidence lens and the certainty states.
- [`mount(8)` (Linux)](https://man7.org/linux/man-pages/man8/mount.8.html): `relatime`,
  `noatime`, `strictatime`. macOS: `man 8 mount`, `noatime` and `strictatime`.
