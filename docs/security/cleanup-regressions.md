# Cleanup safety regression suite

Issue #714 builds on [ADR-0032](../decisions/0032-test-only-fault-points.md):
faults are private to unit tests, thread-local and removed from production builds.
Every filesystem test uses `lumen_testkit::Fixture` temporary directories. There
is no test command accepting a home directory, a drive or another user path.

## Running and reproducing

CI's existing Rust matrix runs these tests on Linux, macOS and Windows:

```sh
cargo nextest run -p lumen-fs-exec -p lumen-application --locked
cargo nextest run -p lumen-fs-exec --locked -E 'test(fault::) | test(concurrent_quarantine)'
```

The fault matrix exhaustively enumerates the seven declared mutation-boundary
points and `EIO`, `ENOSPC`, `EACCES` (21 cases), rather than randomly sampling.
Failure messages identify the point and errno. Each recoverable move or sidecar
failure is followed by restore and a byte comparison. Restore failures after the
rename must report the source and retain its original bytes. Partial authorized
finalization reports `Partly`; the surviving entry must still restore. A failure
after finalization removed the item reports `Item`, never a recoverable copy.

The concurrency regression releases two quarantine attempts from a barrier,
requires exactly one winner and restores its byte-identical file. It accepts
either winner; no timing or scheduler seed affects the invariant.

## Coverage by platform

| Scenario | Linux | macOS | Windows |
| --- | --- | --- | --- |
| Seven post-mutation fault points, three errnos | `fault` unit tests | `fault` unit tests | Not injected; see limitation below |
| Concurrent quarantine of the same file | `races::concurrent_quarantine_attempts_keep_one_recoverable_copy` | Same | Not exercised by this Unix test |
| Parent/leaf link swaps, replaced identities, new hardlinks | `tests/races.rs` | Same | `tests/windows.rs`: junction, replaced leaf and hardlink cases |
| Restore name conflicts and hostile names | Unit tests and `tests/names.rs` | Same | Unit tests and `tests/windows.rs` |
| Permission changes | Read-only-parent fixture (non-root) | Same | Open file without delete sharing |
| Journal interruption and idempotent reconciliation | `lumen-application/tests/crash_points.rs` | Same | Same, platform-independent location model |
| Changed held file or changed held directory | `tests/races.rs` | Same | `win_move` unit tests |
| Protected policy precedence | `lumen-policy/tests/safety.rs` | Same | Same, platform-independent inputs |

## Limits

- `ENOSPC` is injected at the boundary after an operation has taken effect. Tests
  never fill a real disk. This checks error propagation and recorded location,
  not a filesystem's allocation or cache-flush implementation.
- Journal crashes are deterministic lost-write models, not a physical power cut.
  Recovery of a real interrupted operation is also covered by the desktop fixture
  lifecycle; hardware durability still depends on the filesystem and device.
- Windows has real junction, conflict, sharing and identity fixtures but no
  post-mutation syscall fault injector. Unix fault injection is not evidence for
  Windows syscall behavior. A second real volume, removed media and power-loss
  testing remain outside this disposable-directory suite.
- No test authorizes automatic finalization. Permanent removal is exercised only
  after explicit fixture setup and is always inside its fixture quarantine store.
