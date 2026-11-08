# ADR-0062: Uninstall a user-owned macOS app by moving it into quarantine

- Status: Accepted (owner decision, 2026-11-07); rule 1 amended 2026-11-08 (running
  check, partial in-use answers); the owner check is extended by
  [ADR-0063](0063-move-other-accounts-apps-through-a-privileged-helper.md) (apps owned
  by another account, through the privileged helper)
- Date: 2026-11-07
- Refines: [ADR-0060](0060-list-software-and-remove-it-only-through-its-own-mechanism.md)
  rule 2 (macOS: quarantine instead of the Trash),
  [ADR-0015](0015-quarantine-and-reversible-cleanup.md) (quarantine),
  [ADR-0034](0034-command-line-plan-confirmation.md) (confirm by hash),
  [ADR-0051](0051-quarantine-review-items-the-user-chooses.md) (the person chooses);
  amends [ADR-0030](0030-partial-in-use-visibility.md) condition 1 for this action
- Issue: #809

## Context and problem statement

The owner wants removed software to wait in quarantine, so it can be restored and then
finalized together with cache files (2026-11-07). For package-managed software
(Homebrew, MSI, MSIX, dpkg and the rest), that is wrong. The manager still believes it
is installed and its records would be broken, so those keep their own uninstallers
(ADR-0060).

A macOS application that a person dragged into `/Applications` or `~/Applications`
is different. It is one self-contained bundle, and Apple's own instruction is to move
it to the Trash. Moving it into Lumen's quarantine instead keeps everything Apple's
way does, and also makes it restorable until it is finalized.

## Decision

1. **What can be uninstalled this way.** A `.app` bundle that the bundle inventory
   lists directly in `/Applications` or `~/Applications`, and only if every check
   below passes. Each check runs on fresh, read-only evidence when the preview is
   made, and again right before the move:
   - it is not protected: not under `/System` or `/Library/Apple`;
   - its bundle directory is owned by the account Lumen runs as, which leaves out
     apps installed by an administrator installer (`.pkg`) and App Store apps, both
     owned by root;
   - no background item is linked to it (#807, verified or heuristic), and the bundle
     has none of `Contents/Library/LoginItems`, `Contents/Library/LaunchServices`,
     `Contents/Library/LaunchAgents`, `Contents/Library/LaunchDaemons`,
     `Contents/Library/SystemExtensions`. Those helpers are registered with the
     system, and moving only the app would leave them behind and broken. This is
     only directory metadata; no file is read;
   - it is not running: no process, of any user, has its executable inside the
     bundle. Any process's executable path can be read without privileges
     (measured 2026-11-07: 569 of 570 processes; the kernel has none), unlike its
     open files. If one path cannot be read, the answer is unknown. A path the
     kernel spells differently (letter case, Unicode form) is compared by
     identity;
   - its volume has a quarantine store, so the move is a same-volume rename
     (ADR-0015).

   Any check that fails or cannot be answered refuses the uninstall, with the reason
   shown. Unknown is never "eligible".

   **Partial in-use answers (amended 2026-11-08, owner decision).** Unprivileged,
   the executor's in-use probe right before the move cannot read other users' open
   files, so outside home it never answers `No`, and ADR-0030 and ADR-0046 only
   accept `Partial` under home or on a single-user PC's other drives. For this
   action, `Partial` also passes when all of these hold:
   - the running check above answered "not running" for every process, checked
     again right before the move;
   - the item is the bundle this rule admits (user-owned, in `/Applications` or
     `~/Applications`), and the action is the quarantine rename, never
     finalization (which keeps ADR-0030's conditions where the item is held);
   - the probe still sees no process using the bundle; a process of any user that
     runs from inside it counts as using it.

   An app bundle is code: whatever would break is a process running it, and those
   are all visible by executable path. The processes whose open files stay unseen
   are system services that read bundles briefly (Spotlight, Launch Services,
   backup), and a same-volume rename keeps their open descriptors valid. `Yes` and
   `Unknown` still refuse.
2. **How.** The bundle goes through the existing executor exactly like a cleanup item:
   - it is prepared by handle from its parent folder, then journaled, renamed
     without replacing into the store, and verified;
   - in Quarantine it is listed by name, restored with one click, or finalized
     together with other items, after the whole tree is checked;
   - `lumen-fs-exec` is unchanged.
3. **The cleanup policy is not asked.** The person chose to uninstall, and the rules
   above are this action's own deterministic gate, checked again before the move.
   AI review never takes part: a software note (ADR-0061) is display only.
4. **Preview, then confirm the exact app.**
   - The preview shows:
     - name, bundle ID, path and size;
     - that it goes to Quarantine and can be restored until finalized;
     - that its data in `~/Library` stays. After the next scan, that data appears
       in Explore as data of an app that is not installed, and is decided there
       (ADR-0039).
   - The person confirms that preview by its hash (ADR-0034). The page sends only a
     preview ID, never a path (ADR-0023).
5. **Not covered:**
   - App Store apps (remove them from Launchpad);
   - apps installed by `.pkg` or with helpers;
   - Homebrew casks (`brew uninstall --cask`, a later mechanism under ADR-0060);
   - other platforms.

   The preview says which applies, and why.

## Consequences

- Good: an uninstall is reversible until finalized, uses the reviewed executor, and
  joins the quarantine flow people already use.
- Bad: many apps are not eligible (owned by root, or with helpers). The preview says
  why instead of guessing.
- Neutral: Launch Services may still list a moved app until it next refreshes; a
  restore brings it back as it was.
- Bad (residual risk): login items kept by Background Task Management are not
  readable without privileges, and an app started from a translocated copy (a
  quarantined download run where it was) reports another path. Either can leave a
  broken item behind; restoring the app fixes it.
