# ADR-0013: Store the evidence graph relationally with immutable, hashed evidence

- Status: Accepted
- Date: 2026-10-06

## Context and problem statement

Lumen's recommendations depend on relationships, not isolated files:

- an application creates a cache directory;
- a process holds files open;
- a launch agent launches an executable signed by a team that owns an app;
- a Docker VM disk holds images and layers.

Explanations ("What created this? Is anything using it?") are graph traversals. The
evidence behind every decision must be reproducible and auditable after the fact.

## Decision drivers

- Explainability: every decision cites the evidence and edges it used.
- Reproducibility: the same evidence produces the same decision, provably.
- Embedded, portable storage (ADR-0012); no server.
- Traversals are shallow (typically ≤ 4 hops) and anchored on a candidate.

## Considered options

1. A typed property graph stored relationally in SQLite (node and edge tables,
   recursive CTEs), with an in-memory adjacency index built per analysis.
2. An embedded graph database (e.g. Kùzu, or a graph layer over RocksDB).
3. No explicit graph: attributes denormalised onto each candidate.

## Decision outcome

Chosen option: **1**. A graph database is not justified: the workload is shallow,
candidate-anchored traversal over at most millions of nodes, which SQLite indexes and
an in-memory index handle well.

Model (in `lumen-domain` / `lumen-graph`):

- **Nodes** are typed: `Volume`, `FilesystemEntry`, `Application`, `Package`,
  `Process`, `Service` (launch agent/daemon, Windows service, scheduled task, startup
  entry), `CloudSyncRoot`, `ToolCache` (developer knowledge-base match), `Device`. Each
  node has a stable typed ID; filesystem nodes are keyed by
  `(volume_id, file_identity)`, not by path.
- **Edges** are typed and directed: `contains`, `owned_by`, `created_by`,
  `referenced_by`, `opened_by`, `launched_by`, `launches`, `signed_by`,
  `installed_by`, `declared_cache_of` (vendor-declared, e.g. Windows Disk Cleanup
  handlers), `clone_of`, `hard_link_of`, `duplicate_of`, `stored_in` (e.g. VM disk
  image).
- **Evidence records** are immutable facts with provenance:
  `{evidence_id, subject, kind, value, source (adapter + API), observed_at, confidence_source, coverage_ref}`.
  Each record is serialised canonically (deterministic field order, schema version)
  and identified by its BLAKE3 hash. An `EvidenceBundle` for a candidate is the
  sorted set of evidence hashes plus the relevant edges; its Merkle root is the
  **evidence hash** recorded with every policy decision and Jev call.
- Evidence is never edited. New observations append new records; snapshots reference
  the records that were current at scan time.
- Absence is represented explicitly (`not_found_in(sources, coverage)`) and is only
  ever as strong as the coverage report allows (ADR-0008).

### Consequences

- Good: any decision can be replayed from its evidence hash.
- Good: one storage engine for everything local.
- Bad: graph algorithms beyond shallow traversal (e.g. global centrality) need custom
  code; none are currently required.
- Bad: canonical serialisation must be specified and tested so that hashes are
  stable across versions and platforms.

## More information

- [macOS research, Implication 12](../research/03-macos-platform.md) (inventory edges)
- [Windows research, §H and Disk Cleanup handlers](../research/04-windows-platform.md)
- [Scanning research, Implication 9](../research/10-filesystem-scanning-quarantine-duplicates.md)
- ADR-0014 (policy consumes bundles), ADR-0020 (Jev consumes bundles)
