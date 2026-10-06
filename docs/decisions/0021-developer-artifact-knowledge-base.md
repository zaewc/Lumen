# ADR-0021: Drive Developer Mode from a researched knowledge base that prefers tool-native cleanup

- Status: Accepted
- Date: 2026-10-06

## Context and problem statement

Developer machines accumulate large artifacts: Xcode DerivedData and simulators,
Docker images and volumes, `node_modules`, package manager caches, Cargo and Gradle
caches, IDE caches, and AI coding tool data. Their safety varies widely:

- Some caches manage themselves: Cargo GC since Rust 1.88, Gradle's automatic cleanup,
  JetBrains' 180-day rule.
- Some must only be cleaned by their tool: uv says "it's never safe to modify the cache
  directly"; Docker warns against touching `Docker.raw`.
- Some are not regenerable: Xcode Archives with dSYMs, named Docker volumes, simulator
  app data, AI tool transcripts.
- pnpm and uv hard-link or clone files, so deleting `node_modules` may free almost
  nothing.

A name containing "cache" proves nothing.

## Decision drivers

- Every integration researched individually against official documentation.
- Prefer the tool's own cleanup command over deleting files.
- Honest reclaim estimates.

## Considered options

1. A versioned, schema-validated knowledge base (`lumen-devkb`) of artifact entries,
   each with detection rules, safety class, regeneration cost, official action, and a
   `last_verified` date with sources.
2. Generic heuristics (directory name patterns, size, age).

## Decision outcome

Chosen option: **1**.

Each knowledge-base entry contains:

- `id`, `tool`, and detection rules (paths relative to known roots, marker files,
  bundle or tool presence);
- `safety_class`:
  - `self_managed`: informational only;
  - `tool_action`: run the tool's command;
  - `regenerable`: may be quarantined;
  - `not_regenerable`: `REVIEW` only;
- `regeneration_cost` (time, network) and `official_action` (an argv template plus a
  dry-run variant, if any);
- `sources` (official URLs) and `last_verified`.

Tool actions:

- Executed as the user, never elevated, with binaries resolved from known install
  locations (no `PATH` hijack) and signatures verified where possible.
- Dry-run output and before/after free space are captured as evidence.
- They are labelled **irreversible** in the plan UI and never batched with reversible
  quarantine items.
- Timeouts apply, and output parsing tolerates version and locale differences.

"Self-managed" is a per-machine observation (e.g. Cargo GC is skipped under
`--offline`), never an assumption.

The knowledge base is data reviewed like code. Each entry lands in its own PR with its
sources. Entries whose `last_verified` is older than a threshold are flagged in CI.

### Consequences

- Good: Developer Mode recommendations are traceable to vendor documentation.
- Bad: ongoing maintenance as tools change flags and layouts.

## More information

- [Scanning research, developer artifact knowledge base and Implication 8](../research/10-filesystem-scanning-quarantine-duplicates.md)
