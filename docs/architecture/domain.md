# Domain model (`lumen-domain`)

> Implements [system.md §6](system.md#6-domain-model). Decisions:
> [ADR-0002](../decisions/0002-modular-monolith-hexagonal-rust-core.md),
> [ADR-0008](../decisions/0008-capability-model-and-coverage.md),
> [ADR-0011](../decisions/0011-contracts-and-schema-sharing.md),
> [ADR-0013](../decisions/0013-evidence-graph-relational.md) to
> [ADR-0015](../decisions/0015-quarantine-and-reversible-cleanup.md),
> [ADR-0020](../decisions/0020-jev-evidence-only-judge.md).

## Purpose

`lumen-domain` holds the types every other part of Lumen agrees on. It is pure: no
IO, no clock, no randomness, no async runtime, no platform code, no `unsafe`. Its
main job is to make unsafe states unrepresentable. Invariants are enforced in
constructors **and** on deserialization, so a policy bug, a tampered stored record
or a malicious peer cannot introduce a value the rules forbid.

## Module map

| Module | Types | Notes |
| --- | --- | --- |
| `id` | `DeviceId`, `ScanId`, `SnapshotId`, `PlanId`, `OperationId`, `VolumeId`, `FileId`, `FileIdentity` | Generation happens behind an `IdGenerator` port; the domain only wraps and validates |
| `version` | `PolicyVersion`, `SchemaVersion`, `PromptVersion` | Strict canonical text; `SchemaVersion::is_readable_by` = same name and major |
| `time` | `Timestamp` | UTC, years 0001–9999, canonical RFC 3339 |
| `path` | `RawPath`, `PathFlavor` | Byte-exact (Unix bytes; Windows UTF-16 as strict WTF-8); safe `display()` |
| `text` | `UntrustedText` | Vendor-controlled strings; capped at 1 KiB; escaped display only |
| `size` | `ByteCount`, `CloneId`, `SizeFlags`, `SizeFacts`, `ReclaimEstimate` | Honest reclaim math |
| `coverage` | `AccessState`, `CoverageReport`, `SourceName`, `SourceStatus` | Blind spots are unknown, never empty |
| `capability` | `Platform`, `Observation`, `CleanupMechanism`, `Reversibility`, `PlatformCapabilities` | Per-platform ceilings |
| `entry` | `Volume`, `FilesystemEntry`, `EntryKind`, `EntryTimes`, `Protection` | Scanner output |
| `inventory` | `Application`, `AppId`, `Process`, `Service`, `Package`, `CodeSignature` | Graph anchors |
| `digest` | `EvidenceHash`, `PlanHash`, `PayloadHash` | BLAKE3, text form `b3:<64 hex>` |
| `evidence` | `Evidence`, `EvidenceId`, `Subject`, `Fact`, `Basis`, `Provenance` | Immutable, content-addressed |
| `relationship` | `Relationship`, `RelationKind`, `SubjectType` | Typed, evidence-backed edges |
| `decision` | `Verdict`, `Risk`, `RuleId`, `FiredRule`, `PolicyStage`, `JevEffect`, `PolicyDecision` | Policy contract |
| `candidate` | `CleanupCandidate`, `CleanupAction`, `Category`, `Confidence` | What users review |
| `quarantine` | `QuarantineState`, `QuarantineEvent`, `ItemLocation` | Reversibility lifecycle |
| `lifecycle` | `ScanState`, `PlanState`, `Confirmation` | Scan and plan lifecycles |
| `judgment` | `Judgment`, `ReasonCode`, `JudgeDescriptor`, `JevTrace` | Jev model contract |
| `schema` | JSON Schema helpers | See [Contracts](#contracts) |

## Invariants

| Invariant | Enforced by |
| --- | --- |
| Identifiers are never nil, empty or ambiguous | `id` constructors and deserializers |
| One instant, version or number has exactly one text form | strict parsers in `time`, `version`, `id`, `size` |
| Paths and vendor strings never reach a person or a model unescaped | `RawPath::display`, `UntrustedText::display` (only display paths) |
| A file is counted once; hard links free nothing unless all links are selected; shared blocks with unknown private size promise nothing | `ReclaimEstimate::for_selection` |
| Unreadable means unknown; absence needs complete coverage of every searched source | `AccessState::is_observed`, `CoverageReport::absence_is_provable` |
| A host never claims more than its platform allows | `PlatformCapabilities::new` (ceiling check) |
| Only internal, writable volumes with verified no-replace rename can quarantine | `Volume::supports_quarantine` |
| Links and reparse points are never traversed | `EntryKind::is_traversable` |
| Evidence is immutable and addressed by its content | `Evidence::id` (domain-separated BLAKE3 of canonical JSON, pinned by a golden test) |
| Relationships connect meaningful types and cite evidence | `Relationship::new` |
| A hard protection forces `Keep`; Jev cannot touch protected items; Jev effects match verdicts | `PolicyDecision::new` |
| `Keep` offers no action; `Quarantine` proposes only reversible actions | `CleanupCandidate::new` |
| No move without a journal intent; no permanent deletion without retention; failures after the move stay restorable | `QuarantineState::apply` |
| A plan executes only after confirmation of its exact hash | `PlanState::apply` |
| Model output is a closed vocabulary citing only evidence it was sent | `Judgment` (`deny_unknown_fields`), `Judgment::validate_for` |

## Contracts

- Wire format: JSON. 64- and 128-bit integers are decimal strings; times are canonical
  RFC 3339 UTC; digests are `b3:<hex>`; paths are `{flavor, hex}`.
- JSON Schemas are generated from these types (`cargo xtask schema`) and committed
  under [`schemas/`](../../schemas/). CI fails if they drift (`cargo xtask schema --check`).
- Conformance tests serialize real values and validate them against the generated
  schemas, and check that non-canonical forms are rejected.
- Evolution follows ADR-0011: additive changes bump a schema's minor version; anything
  else bumps the major. Changing evidence serialization requires bumping the
  evidence hash domain (`lumen.evidence/1`).

## Failure modes

Every fallible constructor returns a typed error. Library code does not panic on
input (`unwrap`, `expect` and `panic!` are linted). Property tests assert that the
version and timestamp parsers never panic on arbitrary strings, and that path
handling round-trips arbitrary bytes and UTF-16. Dedicated fuzz targets arrive with
the platform parsers (roadmap Phase 5).

## Security considerations

- Attacker-controlled text (file names, labels, metadata) is typed as `RawPath` or
  `UntrustedText` and can only be shown through escaping
  ([threat model TB1](../security/threat-model.md)).
- Decisions key on `FileIdentity`, never on path text.
- Re-validation on deserialization means stored or transmitted records cannot
  smuggle in states the rules forbid. For example, a decision edited from `keep` to
  `quarantine` is rejected.

## Testing

Run `just check`. The crate has unit tests for each rule, property tests for its
invariants (state machines over random event sequences, round trips, reclaim math,
coverage gating), golden tests for evidence hashing, and JSON Schema conformance
tests.
