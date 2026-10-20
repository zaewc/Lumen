-- ledger.db schema v3: what each quarantined item will free (roadmap 13).
--
-- Written by the application after an item is retained, from the confirmed
-- plan's estimate, so the quarantine can show how much space it holds. The
-- executor and its journal are unchanged. One row per operation; rows are
-- never updated or deleted.

CREATE TABLE quarantine_sizes (
    operation_id  TEXT    NOT NULL PRIMARY KEY CHECK (length(operation_id) = 36),
    reclaim_bytes INTEGER NOT NULL CHECK (reclaim_bytes >= 0)
) STRICT;

CREATE TRIGGER quarantine_sizes_append_only_update BEFORE UPDATE ON quarantine_sizes
BEGIN
    SELECT RAISE(ABORT, 'quarantine_sizes is append-only');
END;

CREATE TRIGGER quarantine_sizes_append_only_delete BEFORE DELETE ON quarantine_sizes
BEGIN
    SELECT RAISE(ABORT, 'quarantine_sizes is append-only');
END;
