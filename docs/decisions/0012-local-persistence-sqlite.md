# ADR-0012: Persist locally in SQLite with separate index and ledger databases

- Status: Accepted
- Date: 2026-10-06

## Context and problem statement

Lumen stores three kinds of state with different durability needs:

- the **scan index**: entries, sizes, aggregates, hash cache and snapshots. It is
  large, written in bursts, and rebuildable by rescanning.
- the **evidence graph**: nodes, edges and immutable evidence records (ADR-0013).
- the **quarantine ledger and audit log**: operations, journal entries, manifests and
  AI traces. It is small, but a lost row can mean an unrestorable file.

## Decision drivers

- Durability of the ledger above all.
- Embedded, zero-administration, cross-platform (including iOS and Android).
- A single writer per database, matching SQLite's model.
- Forward-only migrations on user data.

## Considered options

1. `rusqlite` with the `bundled` SQLite, two database files.
2. `sqlx`, Diesel or SeaORM.
3. An embedded key-value store (redb, sled) plus custom indexing.

## Decision outcome

Chosen option: **1**.

- `rusqlite` 0.40.x with `bundled` (SQLite 3.53.x). Bundling guarantees the WAL-reset
  corruption fix, which a system SQLite may lack.
- Two files:
  - `index.db` holds the scan index and evidence graph. It uses `synchronous=NORMAL`
    and can be rebuilt.
  - `ledger.db` holds quarantine operations, journals, manifests, policy decisions and
    AI traces. It uses `synchronous=FULL` and is append-mostly; audit rows are never
    updated in place.
- Both use WAL mode, `foreign_keys=ON`, `busy_timeout`, and `STRICT` tables.
- One writer connection per file, owned by a dedicated writer thread that batches
  transactions. No second process writes or checkpoints the same file concurrently.
- Migrations use `rusqlite_migration` with `user_version`, validated in tests. They
  are forward-only in production, and each runs in its own PR.
- Exports and backups use the backup API or `VACUUM INTO`, never a raw file copy.
- Databases live in an app-local, non-synced directory (never iCloud Drive or a
  OneDrive-redirected folder; WAL is unsafe on network filesystems).
- Every quarantine manifest is also written as a sidecar file inside the quarantine
  store, so a lost `ledger.db` can be reconstructed (ADR-0015).

### Consequences

- Good: proven, portable and inspectable storage; durability tuned per data class.
- Bad: two databases mean cross-database references are by ID and not enforced by
  foreign keys.
- Bad: bundling adds SQLite to each binary's size.

Rejected: option 2's async APIs add nothing for SQLite, compile-time checking needs a
database in CI, and ORMs add abstraction cost. Option 3 means reimplementing indexing
and migrations. `sqlx` stays a candidate for any future hosted backend on PostgreSQL.

## More information

- [Rust ecosystem research, R9](../research/02-rust-ecosystem.md)
- [Scanning research, Implication 6](../research/10-filesystem-scanning-quarantine-duplicates.md)
