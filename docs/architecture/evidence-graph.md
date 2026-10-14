# Evidence graph

How Lumen records what it knows about a file, and how a decision cites it.
Decisions behind this design: [ADR-0013](../decisions/0013-evidence-graph-relational.md)
(relational graph, hashed evidence) and
[ADR-0008](../decisions/0008-capability-model-and-coverage.md) (coverage and absence).

## Building blocks

| Piece | Type | Where |
| --- | --- | --- |
| Fact about one subject, with provenance | `Evidence` | `lumen-domain` |
| Typed, directed edge citing evidence | `Relationship` | `lumen-domain` |
| In-memory graph, traversal | `Graph` | `lumen-graph` |
| Everything one decision may rely on | `EvidenceBundle` → `EvidenceHash` | `lumen-graph` |
| Persistence (`index.db` schema v3) | `IndexStore` graph tables | `lumen-store-sqlite` |

### Subjects (nodes)

`Subject` is the node type:

- `File`: any filesystem object, keyed by `FileIdentity` (volume + file ID), never
  by path.
- `Volume`.
- `Application`: keyed by one `AppId` (bundle ID, Team ID, product code, package
  name and so on).
- `Process`: PID plus start time where the platform reports it, because PIDs are
  reused.
- `Service`: a launch agent or daemon, Windows service, scheduled task or startup
  entry.

Paths are attributes of observations, not identities: a renamed file is the same
subject, and a new file at an old path is a different one.

### Evidence

An `Evidence` record is `{subject, fact, basis, provenance}`:

- `fact` is one typed statement: owning application, application present, last
  modified/accessed, in use (`InUse`, or `NotInUseByVisibleProcesses` when not every
  process was visible), vendor-declared cache, knowledge-base match, code
  signature, cloud-backed, protection.
- `basis` says how it is known: `Observed`, or `Absence` with the sources that
  were searched. Absence is only as strong as those sources' coverage report allows;
  unreadable is never empty.
- `provenance` names the adapter (`SourceName`, e.g. `macos.support-location.bundle-id`)
  and the observation time.

Evidence is immutable. Its ID is the BLAKE3 hash of its canonical JSON, so equal
facts get equal IDs and any change is a new record. Untrusted strings (names,
labels, vendor declarations) are `UntrustedText` and never interpreted.

### Relationships (edges)

A `Relationship` is `(from, kind, to)` plus a non-empty set of evidence IDs.
`Relationship::new` rejects edges whose endpoint types do not fit the kind:

| Kind | From → to | Meaning |
| --- | --- | --- |
| `contains` | volume or file → file | directory or volume contains an entry |
| `owned_by` | file → application | belongs to an application |
| `created_by` | file → application | written by an application |
| `declared_cache_of` | file → application | vendor declared it a disposable cache |
| `opened_by` | file → process | open in, or mapped by, a running process |
| `launched_by` | process → service | started by a service |
| `launches` | service → file | the service runs this executable |
| `clone_of`, `duplicate_of`, `stored_in` | file → file | shared blocks, equal content, content inside a disk image |

## Graph guarantees

`Graph` (in `lumen-graph`, IO-free) holds subjects and edges for one analysis:

- every subject appears once, keyed by identity;
- adding an edge adds its endpoints, so no edge dangles;
- one edge per `(from, kind, to)`: adding it again merges the evidence sets;
- `contains` is acyclic: an edge that would make a subject its own ancestor is
  rejected (`GraphError::ContainmentCycle`);
- iteration and traversal follow insertion order, so the same input always produces
  the same explanation.

Property tests cover these invariants.

Questions are answered by bounded, candidate-anchored traversal. `Graph::edges` lists
the edges touching a subject, in one direction and optionally of one kind.
`Graph::traverse` walks breadth-first along chosen kinds up to `max_depth` hops and
reports each subject once, at its smallest depth, with the edge that reached it.
Explanations need few hops (ADR-0013: typically four at most).

## Evidence bundles and the evidence hash

A decision about a subject may rely only on its `EvidenceBundle`, built by
`Graph::bundle(subject, max_depth, subject_evidence)` independently of insertion
order. The bundle holds:

- the relationships within a few hops of the subject, in both directions;
- the evidence those relationships cite;
- the evidence about the subject itself.

`EvidenceBundle::hash` produces the `EvidenceHash`, a domain-separated BLAKE3 Merkle
root bound to the subject, which is recorded with every policy decision and Jev call.
Version 1 of the construction:

1. Leaves are sorted: one per evidence ID (`0x01` tag) and one per relationship
   (`0x02` tag, canonical JSON).
2. Neighbours are hashed in pairs, and an odd last node is carried up.
3. The root is hashed together with the subject's canonical JSON.

A golden test pins the bytes. Changing the construction is a reviewed change with a
version bump. The same evidence therefore always yields the same hash, and
`(policy_version, evidence_hash)` replays a decision.

## Persistence

`index.db` schema v3 has four tables:

| Table | Content |
| --- | --- |
| `evidence` | `id` (32-byte hash), `body` (canonical JSON) |
| `subjects` | canonical JSON key |
| `relationships` | `(from_key, kind, to_key)`, indexed by `to_key` for "what points at this?" |
| `relationship_evidence` | the evidence each edge cites (foreign keys, cascade on edge delete) |

On every read the store checks each evidence ID against the hash of its body. It
rebuilds loaded edges through `Relationship::new` and `Graph::add`, so domain
validation and acyclic containment hold for whatever the database contains.

## Producers today

| Adapter | Produces |
| --- | --- |
| `lumen-platform-macos::support` | `owned_by` edges with `OwningApplication` evidence for `~/Library` items named after an installed app. Bundle-ID matches and unique-name matches use separate sources, so the policy can weigh them differently. |
| `lumen-platform-windows::cleanup` | `VendorDeclaredCache` evidence for folders declared by data-driven Disk Cleanup handlers. No edge: a handler name does not identify an application. |
| Inventory sources (macOS apps and launchd; Windows apps, startup entries, services) | `Application` and `Service` records with signatures, for later `launches` and `owned_by` edges |
| In-use probes (`libproc`, Restart Manager) | answers for `opened_by` edges and `InUse` evidence; a partial answer is recorded as `NotInUseByVisibleProcesses`, never as `InUse(false)` (ADR-0030). |
| `lumen-devkb` | `KnowledgeBaseMatch` evidence (source `devkb`) for each scanned file or directory that is exactly an entry's location, resolved against the machine's home, known folders and environment variables. Descendants, links and projects without a scanned marker never match. |

## Explanation chains

```text
~/Library/Caches/com.example.app ─owned_by→ Application(com.example.app)
C:\Windows\Temp           (VendorDeclaredCache: Disk Cleanup handler "Temporary Files")
cache file ─opened_by→ Process(pid, start) ─launched_by→ Service(launch agent)
Service ─launches→ executable ─owned_by→ Application ─(Signature)→ Team ID
```

Each step names its evidence and source. An unknown step is shown as unknown, with
the reason, rather than as absent.
