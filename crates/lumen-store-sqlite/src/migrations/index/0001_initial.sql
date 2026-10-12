-- index.db schema v1 (ADR-0012): scans, scan roots, entries, directory listings.
-- Rebuildable by rescanning, so the connection uses synchronous=NORMAL.
-- Conventions: every table is STRICT; times are (seconds, nanoseconds) since the
-- Unix epoch; paths are raw bytes with a flavor; file IDs are 16-byte big-endian
-- blobs; 64-bit unsigned sizes must fit in a signed INTEGER (checked on write).

CREATE TABLE scans (
    id             TEXT    NOT NULL PRIMARY KEY CHECK (length(id) = 36),
    started_secs   INTEGER NOT NULL,
    started_nanos  INTEGER NOT NULL CHECK (started_nanos BETWEEN 0 AND 999999999),
    finished_secs  INTEGER,
    finished_nanos INTEGER CHECK (finished_nanos BETWEEN 0 AND 999999999),
    state          TEXT    NOT NULL CHECK (state IN ('created', 'running', 'paused', 'completed', 'cancelled', 'failed')),
    CHECK ((finished_secs IS NULL) = (finished_nanos IS NULL))
) STRICT;

CREATE TABLE scan_roots (
    scan_id TEXT NOT NULL REFERENCES scans (id) ON DELETE CASCADE,
    flavor  TEXT NOT NULL CHECK (flavor IN ('unix', 'windows')),
    path    BLOB NOT NULL,
    PRIMARY KEY (scan_id, flavor, path)
) STRICT, WITHOUT ROWID;

CREATE TABLE entries (
    scan_id        TEXT    NOT NULL REFERENCES scans (id) ON DELETE CASCADE,
    flavor         TEXT    NOT NULL CHECK (flavor IN ('unix', 'windows')),
    path           BLOB    NOT NULL,
    volume         TEXT    NOT NULL CHECK (length(volume) BETWEEN 1 AND 128),
    file_id        BLOB    NOT NULL CHECK (length(file_id) = 16),
    kind           TEXT    NOT NULL CHECK (kind IN ('file', 'directory', 'symlink', 'junction', 'other_reparse_point', 'special')),
    logical        INTEGER NOT NULL CHECK (logical >= 0),
    allocated      INTEGER NOT NULL CHECK (allocated >= 0),
    private        INTEGER CHECK (private >= 0),
    clone_id       BLOB CHECK (length(clone_id) = 8),
    link_count     INTEGER NOT NULL CHECK (link_count >= 1),
    size_flags     INTEGER NOT NULL CHECK (size_flags BETWEEN 0 AND 31),
    protection     INTEGER NOT NULL CHECK (protection BETWEEN 0 AND 15),
    modified_secs  INTEGER,
    modified_nanos INTEGER CHECK (modified_nanos BETWEEN 0 AND 999999999),
    accessed_secs  INTEGER,
    accessed_nanos INTEGER CHECK (accessed_nanos BETWEEN 0 AND 999999999),
    changed_secs   INTEGER,
    changed_nanos  INTEGER CHECK (changed_nanos BETWEEN 0 AND 999999999),
    created_secs   INTEGER,
    created_nanos  INTEGER CHECK (created_nanos BETWEEN 0 AND 999999999),
    access         TEXT    NOT NULL,
    PRIMARY KEY (scan_id, flavor, path)
) STRICT, WITHOUT ROWID;

-- Hard-link and identity lookups (ADR-0017 size accounting).
CREATE INDEX entries_by_identity ON entries (scan_id, volume, file_id);

CREATE TABLE listings (
    scan_id  TEXT    NOT NULL REFERENCES scans (id) ON DELETE CASCADE,
    flavor   TEXT    NOT NULL CHECK (flavor IN ('unix', 'windows')),
    path     BLOB    NOT NULL,
    access   TEXT    NOT NULL,
    complete INTEGER NOT NULL CHECK (complete IN (0, 1)),
    PRIMARY KEY (scan_id, flavor, path)
) STRICT, WITHOUT ROWID;
