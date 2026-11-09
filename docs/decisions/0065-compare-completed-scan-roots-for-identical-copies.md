# ADR-0065: Compare completed scan roots for identical copies

- Status: Accepted
- Date: 2026-11-09
- Supersedes in part: [ADR-0057](0057-host-chooses-folders-to-compare.md)
  (desktop comparison scope and consent)
- Refines: [ADR-0019](0019-staged-duplicate-detection.md) (staged duplicate
  detection), [ADR-0023](0023-ipc-security.md) (IPC security)

## Context and problem statement

The desktop duplicate comparison used a host folder dialog (ADR-0057), separate
from the scan. The Explore flow already scans the paths the person is examining,
but did not report identical copies among those paths. The requested behavior is
to show that report in Explore automatically after a scan finishes.

The scanner remains metadata-only. Duplicate comparison reads file contents and
must remain a distinct, bounded operation that starts only after the scan ends.
It must not accept roots from the webview: a compromised page must not be able to
name arbitrary paths directly to the comparison command.

## Decision drivers

- Compare the exact scope represented by the completed scan, not an older scan or
  roots supplied by the UI.
- Keep scanning metadata-only; comparison reads content only in its separate
  post-scan operation.
- Reuse the existing protected-location, quarantine exclusion, stable-file
  verification, cancellation, and file-count limit behavior.
- Keep all decisions and actions with the deterministic policy; a duplicate
  report is evidence for review only.

## Considered options

1. Automatically compare the roots stored for the latest completed scan.
2. Keep the system folder dialog and ask for a second selection.
3. Let the webview pass the scan roots to a comparison command.
4. Require a separate user setting or confirmation before every comparison.

## Decision outcome

Choose **option 1**. After a non-partial scan completes, Explore starts a separate
read-only comparison. `compare_scan` receives only the scan ID. The host checks
that it is still the latest completed scan, loads its roots from `index.db`, and
calls `lumen_app::duplicates::find`. The webview never supplies paths. A stale
scan ID, an empty root list, an unsupported platform, or a comparison error is
reported rather than treated as an empty result.

The existing duplicate engine retains its safety controls: it skips protected
locations and quarantine stores, refuses unsafe links, verifies file identity
while reading, supports cancellation, and stops above `MAX_FILES`. Partial or
cancelled scans do not automatically start a comparison. The report contains
paths, sizes and counts; digests remain in the host, and no copy is selected,
moved or removed.

The comparison begins automatically because it is the requested Explore result,
but it is never part of scanning: it begins only after enumeration finishes and
shows its own progress and cancel action. It runs on macOS, Windows and Linux
where the duplicate reader is supported. The desktop folder-dialog dependency is
no longer needed.

### Security and consequences

- Good: the comparison command cannot be given an arbitrary path or stale scan
  scope by the webview.
- Good: content reads remain separate from metadata scanning and are bounded by
  the duplicate engine's existing controls.
- Trade-off: automatic comparison reads file contents in the latest scan's
  scope. A compromised webview can request a scan and then compare its latest
  completed scope; `scan` already accepts roots from the webview, so this is a
  broader information-disclosure capability than ADR-0057's host dialog. The
  host limits the comparison to the persisted latest scan and retains all
  duplicate-engine protections, but does not ask for a second confirmation.
- Trade-off: a broad scan can exceed `MAX_FILES`, in which case the report says
  it could not compare that scope; the user can run a narrower scan.

## More information

- [Threat model, TB2](../security/threat-model.md#tb2-and-tb3-ui--core)
- [ADR-0019](0019-staged-duplicate-detection.md)
