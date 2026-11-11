# ADR-0066: Keep the Windows store out of the install folder

- Status: Accepted
- Date: 2026-11-11
- Amends: [ADR-0015](0015-quarantine-and-reversible-cleanup.md) and
  [ADR-0046](0046-quarantine-on-other-drives-of-a-single-user-pc.md) (the system
  drive's store location on Windows)

## Context and problem statement

On Windows the main quarantine store was `%LOCALAPPDATA%\Lumen\Quarantine\`. The
desktop installer (NSIS, `installMode: currentUser`) puts the app in
`%LOCALAPPDATA%\Lumen\` too: `lumen-desktop.exe` and `uninstall.exe` live there.
That folder is created by the installer, so its owner and DACL are whatever the
installer gave it: on a reported machine the owner was `BUILTIN\Administrators` and
the DACL was inherited, granting several accounts access.

The store check opens every store directory by handle and refuses one whose owner
and DACL are not exactly private to the user (ADR-0015); an existing directory is
refused, never repaired, because it may have been planted. So `Lumen` failed the
check, no store could be opened on the system drive, and every item there failed
to move. The executor reported this as `VolumeUnsupported` ("this drive cannot hold
a quarantine"). Nothing was moved; the check failed closed as designed. The paths
simply collide.

macOS and Linux are not affected: the app is installed elsewhere
(`/Applications`, `/usr`), and the app's own data folder uses its identifier
(`dev.lumen.desktop`), so Lumen creates the `Lumen`/`lumen` folder itself, privately.

## Decision drivers

- Safety of user data outranks reclaimed space: the private-directory check stays
  exactly as strict.
- Items already held in the old location (by the command line, or by a build that
  ran before an installer created the folder) must stay restorable and finalizable.
- A store should not live inside a folder that an installer or uninstaller manages.

## Considered options

1. **A folder of its own: `%LOCALAPPDATA%\LumenQuarantine\`**, one private directory
   that Lumen creates, directly below `%LOCALAPPDATA%` (chosen).
2. **Relax the check** to accept an inherited DACL or an Administrators owner on the
   parent. Rejected: it weakens the store's protection against a swapped parent.
3. **Repair the installer folder's DACL** at first run. Rejected: the store check
   never repairs (a planted folder must not be adopted), and the installer may change
   it again on update.
4. **The app's data folder (`%LOCALAPPDATA%\dev.lumen.desktop\`).** Rejected: the
   desktop shell creates it with an inherited DACL, which fails the same check, and
   the command line does not use it.

## Decision outcome

Chosen option: **1**.

- The system drive's store is `%LOCALAPPDATA%\LumenQuarantine\`. Other drives keep
  `X:\$LumenQuarantine.<UserSID>\` (ADR-0046).
- The old location, `%LOCALAPPDATA%\Lumen\Quarantine\`, is a **former store**: the
  executor still looks there for an item it is asked to locate, restore, finalize or
  forget, with the same private-directory checks, but never creates it and never
  moves a new item into it. A store "has" an item when the item's name or its
  manifest sidecar is there, or when that cannot be told (any error other than
  not-found), so an unreadable former store is chosen and fails closed rather than
  being passed over.
- Scans keep excluding the former location.

### Consequences

- Good: quarantine works on the system drive of an installed Windows app.
- Good: no check is weakened; a folder that is not private is still refused.
- Good: items held in the old location keep their restore and finalize paths.
- Bad: one more location is opened when a held item is looked up. It is a handful
  of handle opens per item.
- Good: a store that fails its checks is reported as such
  (`QuarantineFailure::StoreUnavailable`, "the quarantine folder on this drive could
  not be opened safely"), not as an unsupported drive, so the next such problem
  points at its cause. `VolumeUnsupported` now means only that no store location is
  on the item's volume. A base that cannot be opened (access denied, missing) counts
  as on the item's volume when its drive's root is; only when that root cannot be
  opened either (a drive that is not connected) is its volume unknown.
- Bad: `StoreUnavailable` is a new failure reason written to the journal; a build
  older than this one reads it as a corrupt entry and fails closed (lists nothing,
  moves nothing) until it is updated.

## Implementation

1. `lumen-fs-exec`: `open_existing_store`, and `Executor::with_former_store`; lookups
   choose the store on the item's volume that has the item.
2. `lumen-app`: the new store location and `Stores::former`.
3. `lumen-domain`, `lumen-fs-exec`, `lumen-api`, web: the `StoreUnavailable` reason.
   The Windows store checks the base's volume before anything else, so every later
   error concerns a store on the item's volume.
