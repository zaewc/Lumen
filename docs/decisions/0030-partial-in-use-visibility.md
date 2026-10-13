# ADR-0030: Accept partial in-use visibility for reversible quarantine of the user's own files

- Status: Accepted
- Date: 2026-10-13

## Context and problem statement

[ADR-0014](0014-deterministic-safety-policy-engine.md) makes "items in use by a
running process" a hard protection. The in-use probe (`InUseProbe`, #143) answers
`Yes`, `No` (every process was inspected and none uses the item), `Partial { unseen }`
(no inspected process uses it, but some processes could not be inspected), or
`Unknown`.

Lumen runs without privileges in v1 ([ADR-0005](0005-process-topology-and-privilege.md)).
Measured on macOS 26 (2026-10-13): 872 of 1,107 processes could be inspected, all of
them the user's own; the other 235 (root and other users) were denied. An
unprivileged probe therefore never returns `No`. If `Partial` counted as "in use",
Lumen could act on nothing until the privileged helper exists.

## Decision drivers

- Safety of user data outranks reclaimed space (AGENTS.md).
- Quarantine is a same-volume rename that rollback undoes
  ([ADR-0015](0015-quarantine-and-reversible-cleanup.md)). Renaming does not break
  processes that already have the file open: their descriptors keep working.
- The processes an unprivileged probe cannot see are mostly system daemons (indexing,
  backup, sync). They open user files transiently and tolerate renames. The user's own
  applications, the ones that would break, are all visible.

## Considered options

1. Strict: `Partial` counts as in use everywhere.
2. Accept `Partial` only for the reversible quarantine of the user's own files, under
   the conditions below.
3. Accept `Partial` everywhere.

## Decision outcome

Chosen option: **2** (owner decision, 2026-10-13).

The in-use gate passes when the probe answers `No`. It also passes on `Partial` only
if **every** condition holds:

1. the item lies under the current user's home directory and is owned by that user;
2. the action is the reversible quarantine rename, never permanent deletion, tool
   cleanup or finalization;
3. no other hard protection applies (ADR-0014 stage 1 is unchanged);
4. the executor probes again immediately before the rename, by handle and identity
   ([ADR-0016](0016-single-handle-relative-executor.md)), and the answer is still
   `No` or `Partial`.

`Yes` and `Unknown` always block. Finalization (permanent removal after retention)
requires `No`, or the same conditions with a fresh probe at that time. Every decision
records which answer it relied on, so `Partial` is visible in explanations
("not in use by your applications; system processes could not be checked").

### Consequences

- Good: unprivileged Lumen can quarantine the user's own caches and leftovers while
  still blocking anything a visible process uses.
- Good: the residual risk (an invisible system process using a file) is limited to a
  reversible move of the user's own files.
- Bad: a root process could hold a quarantined file open. The rename keeps its
  descriptor valid; rollback restores the path.
- The privileged helper (later) makes `No` reachable and shrinks the cases relying on
  this ADR.

## More information

- Measurement and probe: #143 (`lumen-platform-macos::ProcessProbe`).
- Port: `lumen_application::ports::InUse`.
