-- index.db schema v2: per-root totals of finished scans, for history and
-- forecasting (system.md §12.4). One row per scan root.

CREATE TABLE snapshots (
    id          TEXT    NOT NULL CHECK (length(id) = 36),
    scan_id     TEXT    NOT NULL REFERENCES scans (id) ON DELETE CASCADE,
    taken_secs  INTEGER NOT NULL,
    taken_nanos INTEGER NOT NULL CHECK (taken_nanos BETWEEN 0 AND 999999999),
    flavor      TEXT    NOT NULL CHECK (flavor IN ('unix', 'windows')),
    root        BLOB    NOT NULL,
    files       INTEGER NOT NULL CHECK (files >= 0),
    logical     INTEGER NOT NULL CHECK (logical >= 0),
    allocated   INTEGER NOT NULL CHECK (allocated >= 0),
    reclaimable INTEGER NOT NULL CHECK (reclaimable >= 0),
    exact       INTEGER NOT NULL CHECK (exact IN (0, 1)),
    complete    INTEGER NOT NULL CHECK (complete IN (0, 1)),
    PRIMARY KEY (id, flavor, root)
) STRICT, WITHOUT ROWID;

-- History for one root, in time order.
CREATE INDEX snapshots_by_root ON snapshots (flavor, root, taken_secs, taken_nanos);
