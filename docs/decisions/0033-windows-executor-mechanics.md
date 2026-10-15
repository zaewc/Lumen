# ADR-0033: Build the Windows executor on relative NT opens, confined unsafe code

- Status: Accepted
- Date: 2026-10-15
- Refines: [ADR-0016](0016-single-handle-relative-executor.md) (Windows mechanics),
  [ADR-0015](0015-quarantine-and-reversible-cleanup.md) (Windows store)

## Context and problem statement

ADR-0016 requires the executor to walk from the scan root by handle, never follow a
link, and rename between verified handles without replacing anything. On Unix,
`rustix` provides `openat`, `fstatat` and `renameat` as safe calls, and
`lumen-fs-exec` is `forbid(unsafe_code)`.

Windows has no safe equivalent in the standard library or `windows-sys`:

- `CreateFileW` takes a whole path and resolves every component again on each call,
  so a junction swapped into an intermediate directory would be followed. Relative
  opens need the native `NtCreateFile` with `OBJECT_ATTRIBUTES.RootDirectory`.
- All `windows-sys` functions are `unsafe`.
- AGENTS.md allows code that moves or deletes user files only in `lumen-fs-exec`,
  so the system calls cannot move to a platform crate.

Which calls does the Windows executor use, and where does its `unsafe` code live?

## Decision drivers

- Same guarantees as on Unix: by handle, no reparse point followed, same volume, no
  replace, identity checked before and after.
- Only documented APIs, verified on Microsoft Learn on 2026-10-15.
- As little `unsafe` as possible, in one reviewable place.
- Testable on the Windows CI runner in fixture directories; checkable locally with
  `cargo clippy --target x86_64-pc-windows-msvc`.

## Considered options

1. `CreateFileW` with full paths and `FILE_FLAG_OPEN_REPARSE_POINT`, then
   `GetFinalPathNameByHandleW` comparisons.
2. **Native relative opens (`NtCreateFile` with `RootDirectory`), one component at a
   time, and handle-based rename and delete.**
3. The `cap-std` crate's Windows backend.

## Decision outcome

Chosen option: **2**.

### Walking and verifying

- Each component is opened with `NtCreateFile`, with `RootDirectory` set to the
  previous handle and a single-component `ObjectName`. Options:
  `FILE_OPEN_REPARSE_POINT | FILE_DIRECTORY_FILE | FILE_SYNCHRONOUS_IO_NONALERT`.
  Access: `FILE_TRAVERSE | FILE_READ_ATTRIBUTES | SYNCHRONIZE`. Microsoft documents
  that with `FILE_OPEN_REPARSE_POINT` "normal reparse processing does not occur"
  and the reparse point itself is opened. That applies to the final component of a
  name, which is why the walk never passes more than one.
- Every opened directory whose attributes (`FileAttributeTagInfo`) include
  `FILE_ATTRIBUTE_REPARSE_POINT` is refused: a junction, symbolic link, mount point
  or cloud placeholder is never traversed.
- Identity is the volume serial number and the 128-bit file ID (`FileIdInfo`). Every
  directory must be on the scan root's volume.
- The leaf is opened the same way, with `FILE_NON_DIRECTORY_FILE` or
  `FILE_DIRECTORY_FILE` as recorded, and `DELETE | FILE_READ_ATTRIBUTES |
  SYNCHRONIZE`. It is compared with the plan by identity, kind, size (`EndOfFile`),
  last write time and link count.

### Moving, restoring, finalizing

- **Move and restore:** `NtSetInformationFile(FileRenameInformation)` on the
  verified leaf handle, with `ReplaceIfExists = FALSE`, `RootDirectory` = the
  destination directory's handle, and a simple name. (The Win32 wrapper
  `SetFileInformationByHandle(FileRenameInfo)` documents `RootDirectory` as "NULL in
  the common case" and rejected a directory handle with `ERROR_INVALID_PARAMETER` on
  the Windows runner, so the native call that `FILE_RENAME_INFORMATION` documents is
  used; corrected during roadmap 9.10c.) Microsoft documents that the rename fails
  if the target exists, that a rename cannot leave the volume, and that "a file
  cannot be renamed if it has any open handles" (a directory: if any file below it
  has open handles). That last rule is an extra in-use gate on top of Restart
  Manager (ADR-0030), and a sharing violation is reported as in use.
- **Finalize:** each entry is opened relative to its directory with
  `FILE_OPEN_REPARSE_POINT` and `DELETE`, so a link is deleted as a link, and
  removed with `FileDispositionInfoEx` (`FILE_DISPOSITION_DELETE |
  FILE_DISPOSITION_POSIX_SEMANTICS`). Where the file system rejects the extended
  class, the classic `FileDispositionInfo` is used. As on Unix, the tree is checked
  in full first.
- **Durability:** `FlushFileBuffers` needs `GENERIC_WRITE`, which cannot be
  requested on a directory, so directory entries are not flushed one by one. NTFS
  logs metadata changes; Lumen's own journal is durable (`synchronous=FULL`), and
  startup reconciliation (roadmap 9.6c) resolves any operation the file system did
  not persist. This is a documented difference from Unix, not a gap in checking.

### Store

- `X:\$LumenQuarantine\<UserSID>\`, created with `NtCreateFile(FILE_CREATE)`
  relative to the volume root. Its security descriptor is set at creation: the user
  as owner, and a protected DACL granting full control only to that user and
  SYSTEM. It is marked `FILE_ATTRIBUTE_NOT_CONTENT_INDEXED` (ADR-0015: excluded from
  indexing).
- An existing store whose owner, DACL or volume does not match is refused, never
  repaired, as on Unix.

### Where the unsafe code lives

- `lumen-fs-exec` stays `forbid(unsafe_code)` on every non-Windows target. On
  Windows the crate is `deny(unsafe_code)`, and one private module, `win_sys`, is
  `allow(unsafe_code)`. It exposes safe functions (open relative, query, rename,
  delete, create directory), each `unsafe` block with a `// SAFETY:` comment.
- Everything else on Windows (verify, store, move, restore, finalize, the executor)
  is safe code over `win_sys`, mirroring the Unix modules.
- CODEOWNERS and the executor review checklist (roadmap 9.11) cover `win_sys`.

### Consequences

- Good: the Windows executor gives the same guarantees as the Unix one, and every
  `unsafe` call is in one module.
- Good: open handles block renames, which adds an in-use check for free.
- Bad: `NtCreateFile` is a native API. It is documented, stable since Windows 2000
  and used by the standard library, but its errors are `NTSTATUS` codes and need
  their own mapping.
- Bad: directory entries cannot be flushed individually; reconciliation covers it.
- Neutral: Windows tests run only on the Windows CI runner; local work is checked
  with `cargo clippy --target x86_64-pc-windows-msvc`.

Option 1 re-resolves intermediate components on every call, which is the TOCTOU
ADR-0016 exists to prevent. Option 3 would add a large dependency for a handful of
calls, and its reparse-point and no-replace behavior on Windows would still have to
be verified against the same documentation (not evaluated further).

## More information

- [NtCreateFile](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/nf-ntifs-ntcreatefile),
  [FILE_RENAME_INFORMATION](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/ns-ntifs-_file_rename_information),
  [FILE_RENAME_INFO](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_rename_info),
  [FILE_DISPOSITION_INFORMATION_EX](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntddk/ns-ntddk-_file_disposition_information_ex),
  [FlushFileBuffers](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-flushfilebuffers)
  (all verified 2026-10-15).
- Research: [Windows platform §D](../research/04-windows-platform.md).
- Roadmap 9.10a–9.10e.
