//! Lumen's local SQLite persistence (ADR-0012).
//!
//! Two database files with different durability:
//!
//! - `index.db`: scans, entries and listings. Rebuildable by rescanning, so it
//!   uses `synchronous=NORMAL`.
//! - `ledger.db`: audit records (Jev traces and the quarantine journal),
//!   append-only and `synchronous=FULL`.
//!
//! Every connection is hardened: WAL journaling (verified, not assumed), foreign
//! keys on, a busy timeout, `trusted_schema=OFF` and SQLite's defensive mode so a
//! tampered database file cannot use schema tricks against Lumen. Migrations are
//! forward-only; opening a database written by a newer Lumen fails instead of
//! misreading it. Databases must live in an app-local, non-synced directory (WAL
//! is unsafe on network filesystems).

#![forbid(unsafe_code)]

mod index;
mod ledger;
mod writer;

use std::path::Path;
use std::time::Duration;

use rusqlite::Connection;
use rusqlite::config::DbConfig;
use rusqlite_migration::{M, Migrations};

pub use index::{IndexStore, RootSnapshot, StoredPlan};
pub use ledger::{JournalEntry, LedgerJournal, LedgerStore};
pub use writer::{IndexWriter, WriteStats, WriterConfig};

/// How long a connection waits for a lock held by another connection.
const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// Migrations for `index.db`, in order. Never edit a released migration; add a
/// new one.
const INDEX_MIGRATIONS: &[M<'static>] = &[
    M::up(include_str!("migrations/index/0001_initial.sql")),
    M::up(include_str!("migrations/index/0002_snapshots.sql")),
    M::up(include_str!("migrations/index/0003_graph.sql")),
    M::up(include_str!("migrations/index/0004_plans.sql")),
    M::up(include_str!(
        "migrations/index/0005_drop_identity_index.sql"
    )),
];

/// Migrations for `ledger.db`, in order. Never edit a released migration; add a
/// new one.
const LEDGER_MIGRATIONS: &[M<'static>] = &[
    M::up(include_str!("migrations/ledger/0001_jev_traces.sql")),
    M::up(include_str!(
        "migrations/ledger/0002_quarantine_journal.sql"
    )),
];

/// Errors opening or migrating a database.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StoreError {
    /// SQLite reported an error.
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// A migration failed, or the database was written by a newer schema.
    #[error("migration error: {0}")]
    Migration(#[from] rusqlite_migration::Error),
    /// The database could not be switched to WAL journaling (e.g. a network
    /// filesystem or a read-only location).
    #[error("database does not support WAL journaling (got journal_mode={mode})")]
    WalUnavailable {
        /// Journal mode SQLite reported instead.
        mode: String,
    },
    /// A value does not fit SQLite's signed 64-bit integers.
    #[error("value for column {column} exceeds SQLite's integer range")]
    OutOfRange {
        /// Column being written.
        column: &'static str,
    },
    /// A stored value failed validation (corruption or tampering).
    #[error("stored value in column {column} is invalid")]
    Corrupt {
        /// Column being read.
        column: &'static str,
    },
    /// The writer thread stopped after an error; `finish` reports the error.
    #[error("the index writer has stopped")]
    WriterStopped,
    /// The writer thread panicked.
    #[error("the index writer panicked")]
    WriterPanicked,
    /// The scan already ended and cannot be resumed.
    #[error("scan {scan} already ended ({state:?}) and cannot be resumed")]
    NotResumable {
        /// Requested scan.
        scan: lumen_domain::ScanId,
        /// Its recorded state.
        state: lumen_domain::ScanState,
    },
    /// The scan does not exist.
    #[error("scan {scan} does not exist")]
    UnknownScan {
        /// Requested scan.
        scan: lumen_domain::ScanId,
    },
    /// The plan does not exist.
    #[error("plan {plan} does not exist")]
    UnknownPlan {
        /// Requested plan.
        plan: lumen_domain::PlanId,
    },
    /// A plan must be saved with exactly one decision per item.
    #[error("a plan's decisions do not match its items")]
    PlanMismatch,
    /// The plan's lifecycle forbids the event.
    #[error(transparent)]
    PlanTransition(#[from] lumen_domain::PlanTransitionError),
}

/// Durability setting per database class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Durability {
    /// `synchronous=NORMAL`: rebuildable data.
    Normal,
    /// `synchronous=FULL`: audit records that must survive power loss.
    Full,
}

/// Opens (creating if needed) and migrates `index.db` at `path`.
///
/// # Errors
///
/// Returns [`StoreError`] if the file cannot be opened, cannot use WAL, a
/// migration fails, or the database was written by a newer Lumen.
pub fn open_index(path: &Path) -> Result<Connection, StoreError> {
    open(
        path,
        &Migrations::from_slice(INDEX_MIGRATIONS),
        Durability::Normal,
    )
}

/// Opens (creating if needed) and migrates `ledger.db` at `path`.
///
/// # Errors
///
/// Returns [`StoreError`] if the file cannot be opened, cannot use WAL, a
/// migration fails, or the database was written by a newer Lumen.
pub fn open_ledger(path: &Path) -> Result<Connection, StoreError> {
    open(
        path,
        &Migrations::from_slice(LEDGER_MIGRATIONS),
        Durability::Full,
    )
}

fn open(
    path: &Path,
    migrations: &Migrations<'_>,
    durability: Durability,
) -> Result<Connection, StoreError> {
    let mut conn = Connection::open(path)?;
    harden(&conn, durability)?;
    migrations.to_latest(&mut conn)?;
    Ok(conn)
}

fn harden(conn: &Connection, durability: Durability) -> Result<(), StoreError> {
    conn.busy_timeout(BUSY_TIMEOUT)?;
    // Refuse schema constructs that could run unexpected code or write the
    // schema directly if the file was tampered with.
    conn.set_db_config(DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true)?;
    conn.pragma_update(None, "trusted_schema", false)?;
    conn.pragma_update(None, "foreign_keys", true)?;
    let mode: String =
        conn.pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))?;
    if !mode.eq_ignore_ascii_case("wal") {
        return Err(StoreError::WalUnavailable { mode });
    }
    let synchronous = match durability {
        Durability::Normal => "NORMAL",
        Durability::Full => "FULL",
    };
    conn.pragma_update(None, "synchronous", synchronous)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use lumen_testkit::Fixture;

    use super::*;

    fn pragma<T: rusqlite::types::FromSql>(
        conn: &Connection,
        name: &str,
    ) -> Result<T, rusqlite::Error> {
        conn.pragma_query_value(None, name, |row| row.get(0))
    }

    fn fresh() -> Result<(Fixture, Connection), Box<dyn std::error::Error>> {
        let fx = Fixture::build(&[])?;
        let conn = open_index(&fx.path("index.db")?)?;
        Ok((fx, conn))
    }

    #[test]
    fn bundled_sqlite_includes_the_wal_reset_fix() {
        // ADR-0012: the WAL-reset corruption bug is fixed in 3.51.3 and later.
        let version = rusqlite::version_number();
        assert!(
            version >= 3_051_003,
            "linked SQLite {} predates the WAL-reset fix",
            rusqlite::version()
        );
    }

    #[test]
    fn migrations_are_valid() -> Result<(), rusqlite_migration::Error> {
        Migrations::from_slice(INDEX_MIGRATIONS).validate()?;
        Migrations::from_slice(LEDGER_MIGRATIONS).validate()
    }

    #[test]
    fn the_ledger_is_durable_and_hardened() -> Result<(), Box<dyn std::error::Error>> {
        let fx = Fixture::build(&[])?;
        let conn = open_ledger(&fx.path("ledger.db")?)?;
        assert_eq!(pragma::<i64>(&conn, "user_version")?, 2);
        assert_eq!(
            pragma::<String>(&conn, "journal_mode")?.to_lowercase(),
            "wal"
        );
        assert_eq!(pragma::<i64>(&conn, "synchronous")?, 2, "FULL");
        assert_eq!(pragma::<i64>(&conn, "trusted_schema")?, 0);
        assert!(conn.db_config(DbConfig::SQLITE_DBCONFIG_DEFENSIVE)?);
        Ok(())
    }

    #[test]
    fn opening_applies_schema_and_hardening() -> Result<(), Box<dyn std::error::Error>> {
        let (_fx, conn) = fresh()?;
        assert_eq!(pragma::<i64>(&conn, "user_version")?, 5);
        assert_eq!(
            pragma::<String>(&conn, "journal_mode")?.to_lowercase(),
            "wal"
        );
        assert_eq!(pragma::<i64>(&conn, "foreign_keys")?, 1);
        assert_eq!(pragma::<i64>(&conn, "synchronous")?, 1, "NORMAL");
        assert_eq!(pragma::<i64>(&conn, "trusted_schema")?, 0);
        assert!(conn.db_config(DbConfig::SQLITE_DBCONFIG_DEFENSIVE)?);
        Ok(())
    }

    #[test]
    fn entries_are_read_by_primary_key_without_an_identity_index()
    -> Result<(), Box<dyn std::error::Error>> {
        let (_fx, conn) = fresh()?;
        let indexes: i64 = conn.query_row(
            "SELECT count(*) FROM sqlite_schema WHERE type = 'index' AND name = 'entries_by_identity'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(indexes, 0);
        let plan: Vec<String> = conn
            .prepare(
                "EXPLAIN QUERY PLAN SELECT path FROM entries WHERE scan_id = ?1 ORDER BY flavor, path",
            )?
            .query_map(["s"], |row| row.get::<_, String>(3))?
            .collect::<Result<_, _>>()?;
        let plan = plan.join("; ");
        assert!(plan.contains("PRIMARY KEY"), "{plan}");
        assert!(!plan.contains("TEMP B-TREE"), "no sort needed: {plan}");
        Ok(())
    }

    #[test]
    fn reopening_is_idempotent() -> Result<(), Box<dyn std::error::Error>> {
        let fx = Fixture::build(&[])?;
        let path = fx.path("index.db")?;
        drop(open_index(&path)?);
        let conn = open_index(&path)?;
        assert_eq!(pragma::<i64>(&conn, "user_version")?, 5);
        Ok(())
    }

    #[test]
    fn strict_tables_and_checks_reject_bad_rows() -> Result<(), Box<dyn std::error::Error>> {
        let (_fx, conn) = fresh()?;
        let id = "0190f1c2-7a3b-7c4d-8e5f-6a7b8c9d0e1f";
        // STRICT: wrong storage type.
        assert!(conn
            .execute("INSERT INTO scans (id, started_secs, started_nanos, state) VALUES (?1, 'yesterday', 0, 'running')", [id])
            .is_err());
        // CHECK: unknown state, nanos out of range.
        assert!(conn.execute("INSERT INTO scans (id, started_secs, started_nanos, state) VALUES (?1, 0, 0, 'done')", [id]).is_err());
        assert!(conn
            .execute("INSERT INTO scans (id, started_secs, started_nanos, state) VALUES (?1, 0, 1000000000, 'running')", [id])
            .is_err());
        conn.execute("INSERT INTO scans (id, started_secs, started_nanos, state) VALUES (?1, 0, 0, 'running')", [id])?;
        // CHECK: a file always has at least one link.
        let entry = "INSERT INTO entries (scan_id, flavor, path, volume, file_id, kind, logical, allocated, link_count, size_flags, protection, access)
                     VALUES (?1, 'unix', x'2f61', 'v', zeroblob(16), 'file', 1, 1, ?2, 0, 0, 'readable')";
        assert!(conn.execute(entry, rusqlite::params![id, 0]).is_err());
        conn.execute(entry, rusqlite::params![id, 1])?;
        Ok(())
    }

    #[test]
    fn foreign_keys_are_enforced() -> Result<(), Box<dyn std::error::Error>> {
        let (_fx, conn) = fresh()?;
        let orphan = conn.execute(
            "INSERT INTO scan_roots (scan_id, flavor, path) VALUES ('00000000-0000-0000-0000-000000000001', 'unix', x'2f')",
            [],
        );
        assert!(orphan.is_err(), "rows must reference an existing scan");
        Ok(())
    }

    #[test]
    fn version_1_databases_upgrade_in_place() -> Result<(), Box<dyn std::error::Error>> {
        let fx = Fixture::build(&[])?;
        let path = fx.path("index.db")?;
        {
            // Create a database exactly as the v1 release did.
            let mut conn = Connection::open(&path)?;
            harden(&conn, Durability::Normal)?;
            Migrations::from_slice(&INDEX_MIGRATIONS[..1]).to_latest(&mut conn)?;
            conn.execute(
                "INSERT INTO scans (id, started_secs, started_nanos, state) VALUES ('0190f1c2-7a3b-7c4d-8e5f-6a7b8c9d0e1f', 0, 0, 'completed')",
                [],
            )?;
            assert_eq!(pragma::<i64>(&conn, "user_version")?, 1);
        }
        let conn = open_index(&path)?;
        assert_eq!(pragma::<i64>(&conn, "user_version")?, 5);
        let scans: i64 = conn.query_row("SELECT count(*) FROM scans", [], |row| row.get(0))?;
        assert_eq!(scans, 1, "existing data survives the upgrade");
        Ok(())
    }

    #[test]
    fn databases_from_a_newer_schema_are_refused() -> Result<(), Box<dyn std::error::Error>> {
        let fx = Fixture::build(&[])?;
        let path = fx.path("index.db")?;
        let conn = open_index(&path)?;
        conn.pragma_update(None, "user_version", 99)?;
        drop(conn);
        assert!(matches!(open_index(&path), Err(StoreError::Migration(_))));
        Ok(())
    }
}
