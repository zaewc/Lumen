# Executor review checklist

Every change to `crates/lumen-fs-exec` (the only code allowed to move or delete user
files) is reviewed against this list. A "no" blocks the merge until it is fixed or the
exception is recorded in an ADR.

## Acting on the right object

- [ ] Every object is reached by handle from a verified root, one name at a time, never
      by a path string resolved after verification
      ([ADR-0016](../decisions/0016-single-handle-relative-executor.md)).
- [ ] No link is followed: `O_NOFOLLOW` / `O_NOFOLLOW_ANY` on Unix,
      `FILE_OPEN_REPARSE_POINT` with reparse points refused on Windows
      ([ADR-0033](../decisions/0033-windows-executor-mechanics.md)).
- [ ] No volume is crossed; a mount point is never entered.
- [ ] Before acting, the item is compared with its record: identity, kind, size, time,
      link count (a directory held in the store: identity and kind).
- [ ] Names are checked (`plain`): not empty, `.`, `..`, no separator, no NUL.

## Never losing data

- [ ] Renames never replace (`RENAME_NOREPLACE` / `RENAME_EXCL` /
      `ReplaceIfExists = FALSE`); nothing is ever copied and deleted.
- [ ] Permanent removal happens only inside a store, through `finalize`, after the
      whole tree is checked. The only other removal is `forget`: one manifest
      sidecar, inside the store, and only while nothing there has the item's name.
- [ ] Every error says where it left the item (`Left`, `Removed`), and a test checks
      that the answer matches the filesystem afterwards.
- [ ] Store directories are private (Unix: owner, mode 0700; Windows: owner and
      protected DACL compared exactly) and a planted one is refused, never repaired.

## Code hygiene

- [ ] `unsafe` appears only in `src/win_sys.rs`, each block with a `// SAFETY:` comment
      stating the invariant, why `unsafe` is needed and why the invariant holds; the
      rest of the crate is `forbid(unsafe_code)` on Unix and safe code on Windows.
- [ ] Every new fallible system call that follows a change on disk has a fault point
      and a fault test ([ADR-0032](../decisions/0032-test-only-fault-points.md)).
- [ ] No `std::fs` destructive call outside tests; tests touch only fixture directories.
- [ ] New behavior is covered on every platform it runs on (the Windows code by the
      Windows CI runner; checked locally with
      `cargo clippy -p lumen-fs-exec --all-targets --target x86_64-pc-windows-msvc`).
- [ ] System-call behavior cited in code or ADRs is quoted from official documentation
      (Apple Developer, Microsoft Learn, man pages) and dated.
