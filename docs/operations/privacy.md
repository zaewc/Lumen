# What Lumen records about itself

> [ADR-0022](../decisions/0022-privacy-preserving-observability.md): no hidden
> telemetry. This page lists every signal; the settings screen will show the same list.

Lumen records how its own work went, so problems can be diagnosed. These records stay
on your machine. Nothing is sent to Lumen or anyone else. You can read the log files,
delete them, or export the recent events to attach to a support request after reviewing
them.

## Where

| Surface | Location |
| --- | --- |
| Command line (`lumen`) | `lumen-logs/lumen.log.<date>` next to the database given with `--db`; nothing without `--db` |
| Desktop app | `logs/lumen.log.<date>` in the app's local data folder (macOS: `~/Library/Application Support/dev.lumen.desktop`) |
| Both | the most recent 1,000 events in memory, for review and export |

One JSON object per line: time (`ts_ms`), level, the code location (`target`), the
operation (`spans`) and the fields below. The log rotates daily (at midnight UTC; the
date in the file name is the UTC date) and only the latest 14 daily files are kept:
older `lumen.log.*` files in the `logs` folder are removed when Lumen starts and at
each rotation. Other files in that folder are never touched.

## Signals

Only `info` and above are recorded by default. Every field is a count, a duration, a
flag or an outcome code. None of them can hold a path, a file or app name, an
identifier or a content hash. A safety net in the log writer also replaces any field
named like a path, name or hash with `[redacted]`.

| Event (`message`) | Operation | Fields |
| --- | --- | --- |
| `scan finished` | `scan` (with `roots`: how many) | `entries`, `directories`, `skipped`, `files`, `complete_roots`, `elapsed_ms`, `stopped_early`, `recorded` |
| `earlier scans pruned` | `scan` | `scans` (earlier scans whose rows were removed), `abandoned` (scans left running for 6 hours, now failed) |
| `earlier scans were not pruned` | `scan` | none |
| `index database compacted` | `scan` | `freed_bytes` |
| `index database not compacted` | `scan` | `reason`: `unmeasured`, `no_room`, `failed` |
| `assessment finished` | `assess` | `candidates`, `keep`, `review`, `quarantine`, `jev_consulted`, `jev_described` (review items AI review was asked to describe, ADR-0041), `elapsed_ms` |
| `plan stored` | `plan` | `items` |
| `plan executed` | `quarantine` | `items`, `quarantined`, `skipped`, `failed`, `resolved`, `moved_bytes`, `warnings` |
| `restore finished` | `restore` | `outcome`: `restored`, `not_restored`, `not_restorable` |
| `finalize finished` | `finalize` | `outcome`: `finalized`, `found_at_source`, `not_found`, `failed`, `maybe_in_use`, `not_finalizable` |

## Not recorded

- File contents, file or folder names, paths, app names, operation or plan IDs, and
  content hashes.
- Anything about what Jev said: Jev's own traces live in the ledger (ADR-0020), not in
  these logs, and are never sent anywhere except in the opt-in request itself.
- Usage analytics, crash reports or identifiers of you or your device.

## Changes

Adding or changing a signal changes this page in the same pull request.
