# ADR-0062: Uninstall a user-owned macOS app by moving it into quarantine

- Status: Accepted (owner decision, 2026-11-07)
- Date: 2026-11-07
- Refines: [ADR-0060](0060-list-software-and-remove-it-only-through-its-own-mechanism.md)
  rule 2 (macOS: quarantine instead of the Trash),
  [ADR-0015](0015-quarantine-and-reversible-cleanup.md) (quarantine),
  [ADR-0034](0034-command-line-plan-confirmation.md) (confirm by hash),
  [ADR-0051](0051-quarantine-review-items-the-user-chooses.md) (the person chooses)
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
   - it is not running (the process probe, ADR-0059's `ApplicationRunning`);
   - its volume has a quarantine store, so the move is a same-volume rename
     (ADR-0015).

   Any check that fails or cannot be answered refuses the uninstall, with the reason
   shown. Unknown is never "eligible".
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
