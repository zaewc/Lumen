# ADR-0070: Show old user logs and temporary files for review, and keep recent ones

- Status: Proposed (lead, 2026-11-21; awaits the owner's confirmation, in particular of
  option 2 below)
- Date: 2026-11-21
- Refines: [ADR-0014](0014-deterministic-safety-policy-engine.md) (a classification
  rule may read file times, only to add caution),
  [ADR-0051](0051-quarantine-review-items-the-user-chooses.md) (which items a person may
  choose)
- Stays within: [ADR-0059](0059-use-application-usage-as-cautious-evidence.md) ("age
  never moves an item towards `QUARANTINE`")
- Roadmap: 15.11. Research: [26](../research/26-reference-cleaners-and-recovery-gap.md)

## Context and problem statement

Every reference cleaner removes logs and Windows temporary files, and research 26
deferred both to "later, user-domain only, with an age rule". Today Lumen does not
assess them at all: the folders that hold them (`~/Library/Logs`, `%LOCALAPPDATA%\Temp`)
are neither knowledge-base entries nor application data locations, and a single item
there is shown only when it passes the 100 MB review threshold as an unrecognised
folder.

The policy has no rule shape that reads a file's age. Its decisions replay from
`(policy_version, evidence_hash)`, so a rule cannot read the clock. ADR-0059 also
records the owner's rule that inactivity or age never makes an item more removable.
What may Lumen say about old logs and temporary files, and how does age enter a
decision?

## Decision drivers

- Safety outranks space. Logs are records, not caches: no platform documents them as
  re-creatable, and a log can be the only record of a problem that shows up later.
- Age only adds caution (ADR-0059). Unknown age is never "old" (ADR-0008).
- Files in use, cloud and sync-root content and crash reports a person may need are
  kept. Logs of security software are never proposed for quarantine; at most a person
  chooses one after reading what it is (rule 5).
- User domain only. Machine-wide logs and `C:\Windows\Temp` belong to the helper
  (ADR-0063, roadmap 16.10).
- Every location and age comes from an official source or is named as Lumen's own
  choice.

## Considered options

1. **Review by location; age only keeps.** Each item directly inside the user's log
   or temporary folder is a review item a person may choose (ADR-0051). Items changed
   recently, or whose age is unknown, are kept.
2. **Propose Windows temporary files older than 7 days for quarantine.** Microsoft's
   Disk Cleanup says "you can safely delete temporary files that haven't been modified
   in over a week" ([Automating Disk Cleanup tool](https://learn.microsoft.com/en-us/troubleshoot/windows-server/backup-and-storage/automating-disk-cleanup-tool)),
   and its `Temporary Files` handler's `LastAccess` defaults to 7 days
   ([%TEMP% folder deleted](https://learn.microsoft.com/en-us/troubleshoot/windows-server/shell-experience/temp-folder-with-logon-session-id-deleted)).
   Not chosen now: it lets age move an item towards `QUARANTINE`, which ADR-0059 rules
   out. It is the owner's call; if accepted, it amends ADR-0059 for this one location.
3. **Assess the whole folder as one item.** Rejected: the folder always holds recent
   and open files, and a candidate is the whole subject (the reason
   `classify.vendor-declared-cache` only reviews `C:\Windows\Temp`).
4. **Leave both unassessed.** Rejected: the space stays invisible, which research 26
   identifies as most of Lumen's gap.

## Decision outcome

Chosen option: **1**.

1. **Locations, user domain only.**

   | Platform | Folder | Source |
   | --- | --- | --- |
   | macOS | `~/Library/Logs` | Apple: `Logs` "contains log files for the console and specific system services" ([macOS Library directory details](https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/FileSystemProgrammingGuide/MacOSXDirectories/MacOSXDirectories.html)) |
   | Windows | the user's temporary folder: `FOLDERID_LocalAppData\Temp`, or a numbered session folder inside it | [`GetTempPath2`](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-gettemppath2w) reads `TMP`, then `TEMP`, then falls back to `USERPROFILE` and the Windows directory; Lumen accepts the result only when it is that folder, never the profile or the Windows directory |

   Not `/Library/Logs`, `/var/log`, `C:\Windows\Temp` or `C:\Windows\SystemTemp`.
   Linux is left out for now: the
   [XDG Base Directory specification](https://specifications.freedesktop.org/basedir/latest/)
   keeps logs in `$XDG_STATE_HOME` together with other state (history, recently used
   files), so no folder there holds only logs.
2. **Subjects.** Each file or folder directly inside one of these folders is one
   item. The folder itself is never a candidate. Links and reparse points are not
   followed (scanner contract).
3. **New evidence** (names may change in implementation):
   - `ScratchLocation { kind: logs | temporary }`: the subject lies directly in one of
     the folders above.
   - `NewestChange { at, as_of }`: the newest modification, creation or
     metadata-change time of the subject and of every entry below it, and when the
     scan read them. Access times are not used (ADR-0053). It is recorded only when the
     subtree was read completely and every entry had a modification time; otherwise it
     is absent, and the age is unknown.
   - `DiagnosticReport { marker }`: the subject is, or holds at any depth, a macOS
     crash, spin or diagnostic report, recognised by entry names only (as
     `BrowserProfile` is): a `DiagnosticReports` folder, or a name ending in `.ips`,
     `.crash`, `.spin`, `.diag` or `.dpsub`. The marker is the first such entry, relative to the
     subject. No file is opened.

   Age is `as_of - at`, from the fact itself, never the clock at decision time, so a
   decision replays exactly.
4. **Policy.**
   - `classify.scratch` proposes `REVIEW` for an item with `ScratchLocation`.
   - `classify.scratch-recent` proposes `KEEP` when the age is unknown, `at` is after
     `as_of`, or the age is below the threshold: **7 days** for temporary files (the
     Disk Cleanup default above) and **30 days** for logs (Lumen's choice: no Apple
     source gives an age, and a log is evidence of a problem that may surface later).
     The most cautious proposal wins, so recent and unknown items are kept.
   - `classify.scratch-report` proposes `KEEP` for an item with `DiagnosticReport`:
     a report itself, or a folder that holds one anywhere below it, so a report is
     never moved together with the log folder around it. The extensions are those the
     [Console guide](https://support.apple.com/guide/console/cnsl664be99a/mac) gives
     for crash, spin and diagnostic reports, and `.crash`, which Apple's
     [Acquiring crash reports and diagnostic logs](https://developer.apple.com/documentation/xcode/acquiring-crash-reports-and-diagnostic-logs)
     names with `.ips` as the common crash-report extensions. A folder whose entries were not all read
     is already kept as incompletely scanned, so an unseen report is never moved
     either.
   - No rule proposes `QUARANTINE` for these items. An old item is moved only when a
     person chooses it, as a review item without a tool command (ADR-0051), and after
     the fresh re-check of ADR-0058, which reads `NewestChange` again. The plan
     records the `at` the person saw; if the fresh `at` is later (anything below was
     written since), or the fresh age is below the threshold, the item is kept and not
     moved.
   - The existing protections apply first and keep the item: in use (`protect.in-use`,
     including partial and unknown answers), cloud-backed, below or holding a sync
     root or other protected location, operating-system protected, not observed, or
     incompletely scanned.
5. **Security software and other owners.** A log folder named after a security
   product is still only a review item; the review sheet says what it is, and nothing
   is preselected (ADR-0051). When an owning application is known, the item is shown
   under it (ADR-0069).
6. **AI.** Jev sees the new facts as metadata, as it sees `LastModified`, and may only
   make a decision more cautious (ADR-0020). It cannot change a `REVIEW` or `KEEP`.
7. **Order of work, one PR each:** the three facts in `lumen-domain` (with the schema);
   the three rules, their explanations and translations, with property tests that no
   such item is ever `QUARANTINE` and that recent, unknown, report-holding, in-use
   and cloud items are `KEEP`; gathering on macOS; gathering on Windows; showing the items
   grouped by location on the Storage page.

### Consequences

- Good: old logs and temporary files become visible and choosable, with their age
  and what they are.
- Good: nothing becomes removable without a person's choice; age only keeps.
- Bad: much less is recovered than the references recover. Windows already empties
  `%TEMP%` itself (SilentCleanup, Storage Sense), so this mostly helps macOS logs and
  machines where Storage Sense is off.
- Bad: temporary folders hold many small items, each chosen one by one (ADR-0051 has
  no "select all").
- Neutral: an app that stores data it needs in `%TEMP%` against Microsoft's guidance
  loses nothing: Lumen only shows it, and a chosen item stays in quarantine until
  retention ends.

## More information

- [Research 26](../research/26-reference-cleaners-and-recovery-gap.md): the
  references, and why logs and `%TEMP%` were deferred.
- [ADR-0059](0059-use-application-usage-as-cautious-evidence.md): usage and age only
  add caution.
- [ADR-0063](0063-use-administrator-rights-through-an-opt-in-helper.md) and roadmap
  16.10: machine-wide logs and temporary folders.
- Sources checked on 2026-11-21: the Apple, Microsoft and Console pages linked above.
