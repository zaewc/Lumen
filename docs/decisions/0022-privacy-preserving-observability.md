# ADR-0022: Observe locally with tracing; make OpenTelemetry export opt-in and content-free

- Status: Accepted
- Date: 2026-10-06

## Context and problem statement

Lumen needs to measure scan duration, files and bytes inspected, candidates found,
policy decisions, cleanup and rollback operations, failures, and AI calls and latency.
Paths, filenames, application lists and hashes are private. The OpenTelemetry Rust
SDK's traces are still Beta, and the GenAI semantic conventions are at Development
status.

## Decision drivers

- Privacy by default: nothing leaves the device unless the user opts in.
- Useful local diagnostics for support and development.
- Low coupling to unstable telemetry crates.

## Considered options

1. `tracing` everywhere with a mandatory redaction layer and local sinks; OpenTelemetry
   behind an `otel` feature with explicit opt-in, exporting metadata only.
2. OpenTelemetry SDK as the primary instrumentation API.
3. A third-party analytics SDK.

## Decision outcome

Chosen option: **1**, implemented in `lumen-telemetry`.

- Instrument with `tracing` spans and events: `#[instrument(skip_all, fields(op_id, item_id))]`.
  Paths and names are never recorded at `info` or above without passing through the
  redaction layer.
- Local sinks: rolling JSON logs in the app data directory and an in-memory ring buffer
  that the user can export for support after reviewing it.
- Metrics are aggregate counters and histograms (files/s, bytes inspected, decisions by
  verdict, quarantine outcomes, rollback count, AI latency and validation failures).
  They never carry paths, names or content hashes.
- OpenTelemetry (`opentelemetry` 0.33 family, pinned together with
  `tracing-opentelemetry`) sits behind the `otel` feature and an explicit setting.
  Export goes to a local file or a localhost OTLP collector. Remote export requires a
  separate opt-in and still excludes content.
- AI spans use `gen_ai.*` metadata attributes plus `lumen.jev.*` (prompt ID, schema
  version, evidence hash, validation result, calibration map, policy effect) behind
  constants, so attribute renames are a one-line change. Content attributes
  (`gen_ai.input.messages` and similar) are not compiled into release builds.
- No hidden telemetry: `docs/operations/privacy.md` will list every signal, and the
  settings UI shows the same list.

### Consequences

- Good: full local observability with no privacy cost.
- Bad: no fleet-wide crash or performance insight unless users opt in.

## More information

- [Rust ecosystem research, R5](../research/02-rust-ecosystem.md)
- [AI harness research, Implication 10](../research/08-ai-harness-and-evaluation.md)
