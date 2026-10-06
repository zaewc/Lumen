# ADR-0016: Route every destructive filesystem operation through one handle-relative executor

- Status: Accepted
- Date: 2026-10-06

## Context and problem statement

"Check the path, then act on the path" is a time-of-check/time-of-use (TOCTOU) race
that cannot be fixed in place:

- CVE-2022-21658 hit Rust's own `std::fs::remove_dir_all`.
- gh CLI v2.102.0 (September 2026) fixed symlink-following writes.

A cleaner that follows a symlink or junction swapped in after its scan can delete or
move arbitrary files. A privileged cleaner turns that bug into privilege escalation.

## Decision drivers

- No destructive operation may act on a path string resolved after verification.
- One small, heavily reviewed crate instead of scattered `std::fs` calls.
- Enforcement by tooling, not discipline.

## Considered options

1. A single crate, `lumen-fs-exec`, that executes plans only through directory handles
   with identity re-verification; all other crates are forbidden from calling
   destructive `std::fs` APIs.
2. `cap-std` capability-based directories everywhere.
3. Careful use of `std::fs` and the `trash` crate on canonicalised paths.

## Decision outcome

Chosen option: **1**, using `rustix` on Unix and `windows-sys` on Windows. `cap-std`
is used for read-side containment in scanners where convenient.

The executor accepts a **plan item**, not a path:
`{parent_dir_identity, leaf_name_bytes, expected_identity, expected_size_facts, expected_times}`.

Execution per item:

1. Open the scan-root directory handle and walk each component with
   `openat(O_NOFOLLOW | O_DIRECTORY)` (macOS: `O_NOFOLLOW_ANY` where available) or
   `CreateFileW(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)`.
2. `fstatat(AT_SYMLINK_NOFOLLOW)` / handle queries on the leaf; compare the identity
   (`(dev, ino)` or `(volume serial, FileId)`), type, size facts and times with the plan.
3. Check the in-use gate (Restart Manager on Windows; open-file evidence elsewhere).
4. Rename relative to verified handles with a no-replace flag into the quarantine
   directory handle (ADR-0015); `fsync` the quarantine directory and the journal.
5. Any mismatch aborts that item to `REVIEW` with the reason recorded. Partial plans are
   allowed; partial items are not.

Invariants:

- Never follow symlinks, junctions or other reparse points; never cross devices; never
  descend into mount points.
- Never open file contents during execution.
- Permanent deletion happens only from inside the quarantine store, by handle.
- `clippy::disallowed_methods` bans `std::fs::remove_file`, `remove_dir`,
  `remove_dir_all`, `rename` and equivalents outside this crate. Semgrep/Opengrep rules
  back this up for `unsafe` FFI calls.
- The crate is CODEOWNERS-protected and has its own threat-model section.

Tests (release-blocking, on macOS, Windows and Linux CI): race tests flipping a
directory to a symlink or junction mid-execution, NFC/NFD and case twins,
right-to-left-override and newline names, invalid UTF-8, long Windows paths,
hard-linked and read-only files, files open in another process, mount points, APFS
clone pairs, and fault injection at every step.

### Consequences

- Good: one place to audit; races fail closed to `REVIEW`.
- Bad: platform-specific unsafe FFI concentrated in one crate, which must document
  every `unsafe` block.

Rejected: option 2 provides containment but no identity verification and does not yet
use `O_NOFOLLOW_ANY` on macOS; option 3 repeats CVE-2022-21658's mistake.

## More information

- [Security research, §A.1–A.4 and A.8](../research/09-security-devops-quality.md)
- [Windows research, §D](../research/04-windows-platform.md)
