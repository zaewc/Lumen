# Jev model contract

> Schemas are generated from Rust types with `schemars`
> ([ADR-0011](../decisions/0011-contracts-and-schema-sharing.md)) and committed under
> `ai/schemas/`. This document explains them; the generated schemas are authoritative.

## Input: `jev.request/1`

A request describes exactly one candidate.

```json
{
  "schema_version": "jev.request/1",
  "item_ref": "c_01J9…",
  "candidate": {
    "category": "app_cache",
    "path_display": "~/Library/Caches/com.example.App",
    "size_bytes": "1288490188",
    "reclaimable_bytes": "1288490188"
  },
  "evidence": [
    { "id": "ev_b3_9f…", "kind": "owning_application", "value": "com.example.App" },
    { "id": "ev_b3_1c…", "kind": "application_installed", "value": "false" },
    { "id": "ev_b3_77…", "kind": "last_access_days", "value": "212" },
    { "id": "ev_b3_42…", "kind": "active_process_reference", "value": "false" },
    { "id": "ev_b3_0d…", "kind": "regeneratable", "value": "true" }
  ],
  "coverage": { "complete": true, "notes": [] },
  "untrusted_strings": {
    "file_name": "com.example.App",
    "bundle_display_name": "Example"
  }
}
```

Rules:

- Integers are decimal strings, matching the encoding used across Lumen.
- `path_display` is redacted: the home directory becomes `~`, user names are removed,
  and other path components are kept only if needed and allowed by the user's tier
  settings. The tier-2 redaction policy is versioned with the prompt.
- All attacker-controllable text (file names, plist values, display names, registry
  strings) appears **only** inside `untrusted_strings` or as evidence `value`s,
  JSON-encoded with bidi and control characters escaped.
- No file contents, ever.

## Output: `jev.judgment/1`

```json
{
  "schema_version": "jev.judgment/1",
  "item_ref": "c_01J9…",
  "assessment": "likely_removable",
  "confidence": "medium",
  "reason_codes": ["owner_app_absent", "regeneratable_cache", "no_active_reference"],
  "evidence_refs": ["ev_b3_1c…", "ev_b3_0d…", "ev_b3_42…"],
  "injection_suspected": false,
  "rationale": "Cache of an application that is no longer installed; regenerated on demand."
}
```

| Field | Type | Validation (in Rust, regardless of provider enforcement) |
| --- | --- | --- |
| `item_ref` | string | must equal the request's `item_ref` |
| `assessment` | enum `likely_removable`, `likely_needed`, `uncertain` | exact lowercase match |
| `confidence` | enum `low`, `medium`, `high` | exact lowercase match |
| `reason_codes` | array of enum (versioned list in `ai/schemas/reason-codes.json`) | 1–6 items, known codes only |
| `evidence_refs` | array of evidence IDs | non-empty subset of the request's evidence IDs |
| `injection_suspected` | bool | — |
| `rationale` | string | ≤ 280 characters after trimming; displayed escaped; never parsed |

`additionalProperties: false` applies everywhere. There are no paths, commands,
actions, numeric scores or free-form recommendations. Enum values never differ only
by case.

Responses are rejected (`JudgeError`) when:

- `stop_reason` is `refusal` or `max_tokens`;
- the JSON does not validate;
- `item_ref` mismatches;
- any evidence reference is unknown;
- the responding model differs from the pinned model ID.

## Trace: `jev.trace/1`

Stored in `ledger.db` for every judgment that a policy decision consumed:

| Field | Notes |
| --- | --- |
| `trace_id`, `timestamp` | |
| `provider`, `model`, `model_version` | On-device models record the OS build and model identifier when exposed |
| `prompt_version` | e.g. `jev-prompt/3` |
| `policy_version` | |
| `input_schema_version`, `output_schema_version` | |
| `evidence_hash` | Merkle root of the bundle |
| `request_hash`, `response_hash` | BLAKE3 of canonical bytes |
| `assessment`, `confidence`, `calibrated_precision_bound` | |
| `validation` | `ok` or the rejection reason |
| `policy_effect` | `none`, `demoted_to_review`, or `promoted` (only for allow-listed classes) |
| `latency_ms`, `cache_hit` | |

Raw request and response bodies are stored only when local diagnostics are enabled.

## Versioning

- Additive optional fields bump the minor version; any change to enums, required
  fields or semantics bumps the major version.
- A new major version of the input or output schema, a new prompt version, or a new
  model requires a full evaluation run and new calibration maps before it can affect
  decisions ([evaluation](evaluation.md)).
