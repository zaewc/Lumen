# ADR-0057: The host asks which folders to compare for identical copies

- Status: Accepted
- Date: 2026-11-06
- Refines: [ADR-0019](0019-staged-duplicate-detection.md) (staged duplicate
  detection), [ADR-0023](0023-ipc-security.md) (IPC security)

## Context and problem statement

Duplicate detection (ADR-0019) reads file contents, which a scan never does. The CLI
(`lumen duplicates <PATH>...`) already makes the person name the folders, and never
covers the whole disk. The desktop app has no way to choose a folder: scans take roots
from the page, but the page always asks for the whole disk.

A compromised webview can call any of Lumen's commands (ADR-0023, FS-1). If the page
could pass the folders to compare, it could make Lumen read the contents of any folder
the user can read. Lumen would only compare files locally, and the policy skips
protected locations, but the result still shows which files are identical.

## Decision drivers

- Lumen reads file contents only in folders the person chose, never in folders the
  page names.
- The desktop app keeps its rules (ADR-0004): no Tauri plugins and no file-system
  access from the webview.
- Prefer platform dialogs and dependencies already in the tree.

## Considered options

1. The host opens the platform's folder dialog and compares what the person picks,
   in one command that takes no path.
2. The page passes folders, typed or dropped, to a comparison command.
3. `tauri-plugin-dialog`, so the page opens the dialog itself.

## Decision outcome

Chosen option: **1**. `compare_folders` takes no input. It opens the system's folder
dialog over the window, then compares the folders the person picked with
`lumen_app::duplicates::find`, sending progress through a channel. If the person
cancels the dialog, nothing is read. The page receives the chosen folders, groups of
paths with their sizes, and counts; content digests stay in the host (ADR-0019).

The dialog comes from [`rfd`](https://github.com/PolyMeilex/rfd) (MIT), which
`tauri-plugin-dialog` uses as well. Only the host links it; it has no JavaScript API
and needs no capability. On macOS it shows the panel as a sheet on the main thread,
as AppKit requires; on Windows, the shell's folder dialog owned by the window. It is
linked on macOS and Windows only, without default features, and adds no other crate:
Tauri already depends on its AppKit (`objc2`) and Win32 (`windows-sys`) bindings.

Where the host shows no dialog (Linux for now, although the core can read there),
the command says comparing is unsupported without opening one, instead of reporting
that nothing was found. The same crate covers GTK through bindings Tauri already
links.

Option 2 lets a compromised page choose what Lumen reads. Option 3 adds a plugin and
gives the webview a file-system capability, which the desktop rules forbid.

### Consequences

- Good: a compromised page can start a comparison, but only the folders the person
  picks in a host-drawn dialog are read.
- Good: the comparison never covers the whole disk.
- Bad: one more native dependency to keep current, through the same review as `tauri`.
- Bad: the page cannot offer folder choices of its own, such as recent folders.

## More information

- [Threat model, TB2](../security/threat-model.md#tb2-and-tb3-ui--core)
- [rfd documentation](https://docs.rs/rfd)
