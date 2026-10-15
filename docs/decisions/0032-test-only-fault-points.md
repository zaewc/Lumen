# ADR-0032: Inject executor faults through test-only fault points

- Status: Accepted
- Date: 2026-10-15

## Context and problem statement

Every error from `lumen-fs-exec` reports where it left the item: untouched at its
source, or in the store and restorable (`MoveError::left`, `RestoreError::left`,
`FinalizeError::removed`). The application journals that report, and reconciliation
trusts it (ADR-0015). A wrong report is the one executor bug that could make Lumen
record an item as somewhere it is not.

The race suite (roadmap 9.9b) and the crash-point suite (9.9c) cover changes to the
tree and crashes between journal entries. Neither can make a single system call fail
after the previous one succeeded: a `fsync` after a successful rename, the
post-move `fstatat`, a sidecar `write`, an `unlinkat` half way through a tree.
Those failures are rare on real disks, and that is why they need tests.

How do tests make one chosen system call in the executor fail?

## Decision drivers

- Production builds must be byte-for-byte what they would be without the mechanism:
  no extra branch, symbol, configuration or environment variable that could arm a
  fault in a user's process.
- `lumen-fs-exec` stays small, `forbid(unsafe_code)`, and readable by a reviewer in
  one sitting (ADR-0016).
- No new dependency in the executor without a strong reason (AGENTS.md).
- Tests must be able to target each fallible call individually and run in parallel.

## Considered options

1. **A `Sys` trait** for every system call, with the executor generic over it and a
   failing implementation in tests.
2. **The `fail` crate** (TiKV failpoints), enabled by a Cargo feature.
3. **Test-only fault points**: a private `fault` module whose `check(Point)` returns
   an error when a test armed that point on the current thread, and which compiles
   to nothing outside `cfg(test)`.
4. **Real conditions only**: permissions, a full disk image, removed media.

## Decision outcome

Chosen option: **3, test-only fault points**.

- `fault::check(Point::X)?` is placed immediately after each fallible call it
  stands for, so an armed point behaves like that call failing after its effect
  (the harder case: the rename happened, then the error).
- Under `cfg(test)` the armed point is a thread-local, so tests stay parallel. Under
  `cfg(not(test))`, `check` is a `const fn` that returns `Ok(())` and is inlined
  away; there is no feature flag, so no build of Lumen can enable it.
- Faults are reachable only from the crate's own unit tests. Integration tests keep
  using real conditions (option 4) where they can.
- Each fault test asserts that the reported location (`left`, `removed`) matches the
  filesystem afterwards, checked with `lstat`.

Option 1 changes every production call site and puts a generic parameter on the
executor's public types for a test concern. Option 2 adds a dependency with a global
registry, and the risk that a feature is enabled in a release build. Option 4 cannot
reach most points (a `fsync` failing after a successful rename) and depends on the
platform.

### Consequences

- Good: every fallible step of quarantine, restore and finalize gets a test that its
  reported location is true.
- Good: zero production impact, enforced by `cfg(test)`, not by a flag.
- Bad: a fault point marks a call site by convention; a new system call without one
  is untested until someone adds it. The roadmap item lists the points, and the
  review checklist for `lumen-fs-exec` (9.11) asks for them.
- Neutral: a fault point simulates failure after the call; a failure before its
  effect is already covered by real conditions (permissions, a taken name).

## More information

- Roadmap 9.9d; ADR-0015 (quarantine lifecycle), ADR-0016 (handle-relative
  executor).
