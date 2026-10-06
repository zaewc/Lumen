# ADR-0015: Quarantine by journaled same-volume rename into a Lumen-owned store

- Status: Accepted
- Date: 2026-10-06

## Context and problem statement

Every cleanup must be reversible whenever the platform allows it. Research showed that
the OS trash is not a reliable quarantine:

- macOS: "Put Back" can silently go missing for programmatically trashed items
  (Apple DTS bug r. 23153124), there is no restore API, and the user may auto-empty the
  Trash.
- Windows: the Recycle Bin has no documented restore API and may delete items
  permanently when it cannot take them; Storage Sense may purge it.
- The `trash` crate cannot list or restore on macOS and may trigger an Automation
  permission prompt.

## Decision drivers

- Exact, verifiable rollback.
- Crash safety at any point in an operation.
- Preserve metadata (xattrs, ACLs, alternate data streams, clones, file IDs).
- No silent copy-and-delete.

## Considered options

1. Same-volume atomic rename into a per-volume, per-user Lumen quarantine store,
   driven by a write-ahead journal, with sidecar manifests.
2. The OS trash as quarantine.
3. Copy into an application container, then delete the original.

## Decision outcome

Chosen option: **1** on desktop, with platform-native equivalents on mobile.

Lifecycle (`QuarantineItem` state machine):

`Planned → Journaled → Moved → Verified → Retained → (Restored | Finalized)`, with
`Failed(reason)` reachable from any step before `Verified` and always leaving the
original in place or restorable.

Mechanics (desktop):

- Stores: macOS `~/Library/Application Support/Lumen/Quarantine/` for the Data volume
  and `<volume>/.LumenQuarantine/<uid>/` elsewhere; Windows
  `X:\$LumenQuarantine\<UserSID>\` with an owner-only DACL. Stores are excluded from
  backups and indexing.
- The move uses a **no-replace** rename (`renamex_np(RENAME_EXCL)`,
  `renameat2(RENAME_NOREPLACE)`, `SetFileInformationByHandle(FileRenameInfoEx)`
  relative to a verified directory handle) through the executor (ADR-0016).
- Volumes that do not support no-replace rename, cross-volume moves, network, removable
  and foreign filesystems: **no quarantine; the item becomes `REVIEW`**. Never
  copy-and-delete silently.
- Journal: an intent record is durably written to `ledger.db` (`synchronous=FULL`)
  **before** each move and a completion record after. On startup, incomplete intents
  are reconciled by checking identities at the source and destination.
- Manifest per item (mirrored as a sidecar file in the store): `operation_id`,
  timestamp, `device_id`, original path (bytes and display form), quarantine location,
  file identity, size facts, timestamps, mode, owner, flags and xattr list, reason,
  `policy_decision` (with `policy_version` and `evidence_hash`), user confirmation,
  result, `rollback_possible`, and the linked Jev trace, if any.
- Verification after the move: the source is gone, the destination has the same
  identity and link count, and size facts match.
- Restore: a no-replace rename back after checking that the original parent exists and
  the name is free; on conflict, ask the user and never overwrite.
- Quarantine frees no space until finalization, and the UI says so. Finalization
  happens after a retention period (default 30 days, user-configurable) or on explicit
  user request, by handle, and is recorded. Optionally the item is "released" to the
  OS trash, which the UI must not describe as recoverable by Lumen.

Platform equivalents:

| Platform | Quarantine | Rollback |
| --- | --- | --- |
| Android media | `MediaStore.createTrashRequest(…, true)` | `createTrashRequest(…, false)` within `DATE_EXPIRES` |
| Android non-media (All files access) | rename into `Documents/Lumen/.quarantine/` on the same volume | rename back |
| iOS Photos | batched `PHAssetChangeRequest.deleteAssets` → Recently Deleted (30 days) | user-guided restore in Photos |
| iOS iCloud Drive | `evictUbiquitousItem(at:)` | re-download |
| Irreversible actions (tool commands, Android external cache clear, snapshot thinning) | none | none; labelled irreversible and never batched with reversible items |

### Consequences

- Good: rollback is exact and testable with fault injection.
- Good: a lost ledger can be rebuilt from sidecar manifests.
- Bad: quarantined items keep using space during retention.
- Bad: some volumes cannot use quarantine at all, which limits cleanup there.

## More information

- [Scanning research, Implication 7](../research/10-filesystem-scanning-quarantine-duplicates.md)
- [macOS research, Implication 10](../research/03-macos-platform.md)
- [Windows research, §D](../research/04-windows-platform.md)
- [Android research, Implication 5](../research/05-android-platform.md)
- [iOS research, Implication 2](../research/06-ios-and-mobile-framework.md)
