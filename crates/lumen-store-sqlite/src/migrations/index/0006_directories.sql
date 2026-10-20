-- index.db schema v6: store each directory path once (roadmap 13.c).
--
-- Entries and listings repeated their scan ID, flavor and full path in every
-- row (about 240 of 365 B per entry on a whole disk). Now each scan's
-- directory paths are stored once in `dirs`, and an entry or a listing is a
-- (directory, name) pair: its path is the directory path followed by the name,
-- split after the path's last separator (`/` for Unix, `\` for Windows paths),
-- so `/Users/ana` is (`/Users/`, `ana`), `/` is (`/`, ``) and a path without a
-- separator is (``, the path). Scans get a small integer key so directories do
-- not repeat the 36-character scan ID either.
--
-- Rescan after upgrade: converting stored full paths into directories and
-- names is not practical in SQL, so the old entries and listings are dropped,
-- and every scan that had or could still get them (created, running, paused,
-- completed) is marked `failed`. A failed scan is never assessed nor resumed
-- (assessing needs a completed scan, ADR-0008), so nothing reads the missing
-- rows; the user scans again. Scan records and roots, snapshots (history),
-- plans and the evidence graph are kept. The dropped pages are reused by the
-- next scan; the file itself shrinks only on VACUUM.

UPDATE scans SET state = 'failed' WHERE state IN ('created', 'running', 'paused', 'completed');

DROP TABLE entries;
DROP TABLE listings;

ALTER TABLE scans ADD COLUMN key INTEGER CHECK (key >= 1);
UPDATE scans SET key = rowid;
CREATE UNIQUE INDEX scans_by_key ON scans (key);

-- One row per directory path of a scan that holds an entry or a listing.
-- `path` ends with the flavor's separator, except the empty path of names
-- without a separator.
CREATE TABLE dirs (
    id     INTEGER NOT NULL PRIMARY KEY,
    scan   INTEGER NOT NULL REFERENCES scans (key) ON DELETE CASCADE,
    flavor TEXT    NOT NULL CHECK (flavor IN ('unix', 'windows')),
    path   BLOB    NOT NULL,
    UNIQUE (scan, flavor, path)
) STRICT;

CREATE TABLE entries (
    dir            INTEGER NOT NULL REFERENCES dirs (id) ON DELETE CASCADE,
    name           BLOB    NOT NULL,
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
    PRIMARY KEY (dir, name)
) STRICT, WITHOUT ROWID;

CREATE TABLE listings (
    dir      INTEGER NOT NULL REFERENCES dirs (id) ON DELETE CASCADE,
    name     BLOB    NOT NULL,
    access   TEXT    NOT NULL,
    complete INTEGER NOT NULL CHECK (complete IN (0, 1)),
    PRIMARY KEY (dir, name)
) STRICT, WITHOUT ROWID;
