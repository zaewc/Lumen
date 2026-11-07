# ADR-0060: List installed software, and remove it only through its own mechanism

- Status: Accepted (owner confirmation, 2026-11-07)
- Date: 2026-11-07
- Refines: [ADR-0005](0005-process-topology-and-privilege.md) (user scope
  first), [ADR-0020](0020-jev-evidence-only-judge.md) (AI is evidence),
  [ADR-0049](0049-run-tool-commands-on-request.md) (running a
  vendor's command)
- Issues: #804 (#805–#810)
- Research: [36. Software inspection and removal](../research/36-software-cleanup.md)

## Context and problem statement

People want to see what software is installed, why it is there, what runs in the
background because of it, and how to remove what they no longer need. Hoax Eliminator
does this for Korean banking security modules on Windows. Lumen already reads most of
the platform inventory, but only to judge files.

Removing software is unlike quarantining a file. No platform mechanism can be undone
by Lumen. Uninstall commands are text written by installers. Some removals need
administrator or root rights. And a wrong target can disable banking, security or work
software.

How should Lumen list software, and when, how and through what may it remove some?

## Decision drivers

- Safety of user data and of the computer outranks reclaimed space; when uncertain,
  keep.
- Every claim comes from observed evidence or a cited source (ADR-0013).
- Unknown is never "not installed" or "safe to remove" (ADR-0008).
- AI explains; it never chooses a target, mechanism or argument (ADR-0020).
- v1 runs as the user; privileged work waits for a reviewed helper (ADR-0005).
- No command reaches a shell; installer-written text is untrusted.
- The software list is personal data and stays on the device.

## Considered options

1. **List only.** Show software, background items and leftovers; tell people how to
   remove software themselves.
2. **List, and remove through the platform's own mechanism for one exact identity,**
   after a preview and the person's confirmation; verify by a fresh inventory.
3. **Remove the way cleaners do:** run any registered uninstall string, or delete the
   application's folders.

## Decision outcome

Chosen: **option 2**, introduced in stages, with these rules.

1. **Identity.** Software is identified by its platform-native ID and inventory source:
   - a bundle ID, an MSI product code, an MSIX package family name, an Uninstall key,
     or a package name together with its package manager;
   - never by display name;
   - two records that could be the same software are shown as separate entries and are
     not removable until the person picks one.
2. **Mechanisms Lumen may invoke**, one exact identity per run:

   | Platform | Mechanisms |
   | --- | --- |
   | Windows | `msiexec /x {ProductCode}`; MSIX removal for the current user |
   | macOS | the vendor's uninstaller when its location and signature are recorded; moving a user-owned `.app` without privileged components to the Trash |
   | Linux | the package manager that owns the package; Flatpak per user |

   Each mechanism is added in its own pull request.
3. **Commands are built, never copied.** Lumen builds the program and arguments from
   the identity, as ADR-0049 does from the knowledge base:
   - it runs them with no shell, closed stdin, its own process group, a time limit and
     a bounded output tail;
   - Windows `UninstallString`/`QuietUninstallString` are untrusted text, used only
     when they parse into a recognised form: an MSI product code, or a quoted
     executable inside the product's recorded install location;
   - anything else is "remove it yourself", with the platform's own steps.
4. **Never removable from Lumen, only shown:**
   - operating system components and anything the platform marks non-removable
     (`NoRemove`, required macOS apps, Essential or protected packages);
   - security software;
   - enterprise-managed applications;
   - drivers.

   Removing protection is a person's decision outside Lumen.
5. **Preview, then confirm the exact target.** The preview shows:
   - the identity and its source;
   - the mechanism and exact command;
   - linked background items, and the dependents or shared components the mechanism
     also removes;
   - whether it needs administrator rights;
   - what it leaves behind;
   - that Lumen cannot undo it.

   The default is the mechanism that keeps user data (`apt remove`, not `purge`; no
   cask `--zap`; Flatpak without `--delete-data`). The person confirms that preview by
   its hash, as plans are (ADR-0034); the page sends a preview ID, never a command
   (ADR-0023).
6. **Privileges.** A removal that needs administrator or root rights is shown as such
   and is not run until a privileged helper exists (ADR-0005). Lumen never requests or
   approves elevation by itself.
7. **Success is observed.** The mechanism's exit status is reported as it is. The
   software counts as removed only when a fresh inventory from the same source no
   longer lists the identity. Anything else is reported as failed or partial, with
   what remains.
8. **Leftovers go through the existing file pipeline.** Data left by removed software
   becomes orphaned application data, which the policy already sends to review
   (ADR-0039). There is no new deletion path.
9. **AI and privacy.**
   - Jev may explain what software is for and what its removal could affect, from
     redacted metadata in a closed vocabulary.
   - It never selects targets or options and never makes a decision less cautious.
   - Usage signals stay off the request (ADR-0059).
   - The software list stays on the device and out of telemetry.

### Consequences

- Good: people can see and understand everything installed. Removal uses only what
  each platform supports, with no shell and no guessing. Protected software is safe
  from Lumen by construction.
- Bad: many removals will say "remove it yourself", because they need elevation or
  their uninstall string is not in a recognised form, until a privileged helper
  exists. Removal can never be undone, unlike quarantine.
- Neutral: Hoax Eliminator's targets (banking security modules) count as security
  software under rule 4. Lumen shows them with evidence and does not remove them. This
  is a deliberate difference, confirmed by the owner.

### Implementation plan (one PR each)

1. #806: read-only inventory. Uninstall metadata as untrusted text; MSIX, receipts,
   Homebrew, Linux package managers, each capability-based.
2. #807: Service→Application relationships from evidence, never names.
3. A software page: list, details, background items, "how to remove it yourself".
4. #808: Jev explanations for software.
5. #809: the typed removal preview, then one mechanism per PR, safest first: Windows
   MSI by product code, macOS user-owned app to the Trash, Linux per-user Flatpak. Each
   updates the threat model.
6. #810: the leftovers audit through the existing pipeline.

## More information

- Owner decisions, 2026-11-07:
  - Security software stays list-only (rule 4), with no extra-warning removal.
  - No privileged helper in this epic: only removals that work with the user's own
    rights are run. A helper needs its own ADR.
- Hoax Eliminator publishes no license or terms; nothing from it is reused
  (research 36).
