# ADR-0046: Quarantine on other drives of a single-user PC

- Status: Accepted
- Date: 2026-10-27
- Amends: [ADR-0015](0015-quarantine-and-reversible-cleanup.md) (stores on other
  volumes), [ADR-0030](0030-partial-in-use-visibility.md) (condition 1);
  carries out roadmap 13.a for Windows

## Context and problem statement

Items on a second drive (`D:\` on Windows) can never be quarantined today, for two
reasons (design review, roadmap 13.a):

- **No store there.** Quarantine is a same-volume rename into a store
  ([ADR-0015](0015-quarantine-and-reversible-cleanup.md)). There is one store, under
  the user's profile on the system drive, so an item on another volume fails with
  `VolumeUnsupported` and stays where it is.
- **The in-use rule keeps them first.** An unprivileged in-use probe never answers
  `No`: it cannot see other users' or system processes, so it answers `Partial`.
  [ADR-0030](0030-partial-in-use-visibility.md) lets `Partial` pass only for
  the user's own items **under their home**, because only there are the processes it
  cannot see limited to system services; another user's applications cannot reach a
  private home. Outside home they can, so items on other drives are kept.

A second internal data drive is common on Windows PCs, and caches and build output
there (game shader caches, `node_modules`, `target`) are what Lumen is for. The owner
decided on 2026-10-27 to allow quarantine there on a single-user PC.

## Decision drivers

- Safety of user data outranks reclaimed space; quarantine must stay reversible and
  same-volume, never copy-and-delete (ADR-0015).
- The processes an unprivileged probe cannot see must be no more dangerous than the
  ones ADR-0030 already accepts under home: system services, not other people's
  applications.
- A volume Lumen cannot protect a store on (removable, network, foreign filesystem,
  no ACLs) gets no store.

## Considered options

1. **Single-user PC:** `Partial` also passes outside home on a volume with a Lumen
   store, for the user's own items, when the current user is the only user with a
   profile on the computer (the decision below).
2. **Keep ADR-0030 as it is** and only show other-drive items as review instead of a
   failure. Rejected by the owner: the drives that need cleaning most stay out of
   reach.
3. **Any PC, outside home, for the user's own items.** Rejected: another user's
   running application could have the item open, and nothing Lumen can see would say
   so.

## Decision outcome

Chosen option: **1**, on Windows first.

**Stores on other volumes (amends ADR-0015).** A volume gets a store only if all of
these hold; anything else stays as today (items there are left for review):

- a local fixed drive: `GetDriveTypeW` is `DRIVE_FIXED`, and the storage device is
  not removable and not on a USB, IEEE 1394, SD or MMC bus
  (`IOCTL_STORAGE_QUERY_PROPERTY`), because USB disks also report `DRIVE_FIXED`;
- NTFS or ReFS with persistent ACLs (`FILE_PERSISTENT_ACLS`), not read-only;
- its volume serial number is unique among the mounted volumes (a cloned disk would
  make two volumes indistinguishable to identity checks).

The system drive keeps its store under `%LOCALAPPDATA%\Lumen\Quarantine\`. Another
eligible drive `X:` gets `X:\$LumenQuarantine.<UserSID>\`, a direct child of the
drive root created with the owner-only DACL of today's stores, so no shared parent is
needed. An existing directory there that is not private to the user is refused (the
store is then unavailable, never shared). A store is only ever opened on the volume of
the item that goes into it: the executor checks the base's volume before creating
anything. There is still one ledger; restore, finalize and reconciliation find an
item's store by the volume recorded when it was quarantined. While that drive is not
connected, its held items cannot be restored or finalized and wait, unchanged, until
it is back.

**In-use condition 1 (amends ADR-0030).** `Partial` passes when conditions 2–4 hold
and either:

- **(a)** the item lies under the current user's home and is owned by that user
  (unchanged), or
- **(b)** the item is owned by the current user, lies on a volume with a Lumen store
  (above), and the computer is **single-user**: the current user is the only user
  account with a profile on it (Windows: exactly one `S-1-5-21-…` or `S-1-12-1-…`
  SID under `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProfileList`). On
  such a computer the processes the probe cannot see belong to the system and to
  services, as under home.

If the profile list cannot be read, the computer is not single-user. Finalization
applies the same conditions to the item where it is held, with a fresh probe, as
ADR-0030 already requires. Every decision that relies on (b) says so in its
explanation.

### Consequences

- Good: caches and build output on a second internal drive can be quarantined and
  restored like those under home.
- Good: removable, network and foreign volumes stay excluded; a shared PC changes
  nothing.
- Bad: a service that keeps data on the second drive (a database server, a game
  server) runs where the probe cannot see it. Its files are rarely proposed (the
  policy proposes only identified caches and artifacts, never databases or
  application state), the executor probes again just before the rename, and
  quarantine is undone by restore.
- Bad: a second account created later makes the computer shared; decisions made
  after that keep such items, and items already held are restored or finalized only
  under the same check at that time.
- Neutral: macOS is unchanged. Its other internal volumes are rare and external ones
  usually ignore ownership, so they get no store; a later ADR may extend this.

## Implementation

1. `lumen-platform-windows`: classify volumes for a store; read the profile list.
2. `lumen-policy`: `Context::single_user` and condition (b); a new policy version.
3. `lumen-fs-exec`: the Windows executor holds several stores and opens the one on the
   item's volume, never creating a store on another volume.
4. `lumen-app`: open the stores of eligible volumes, and give an item the quarantine
   mechanism only when its volume has one.
5. Messages for a drive that is not connected and for each failure reason.
