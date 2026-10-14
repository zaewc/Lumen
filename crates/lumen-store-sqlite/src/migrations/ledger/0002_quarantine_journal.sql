-- ledger.db schema v2: the quarantine journal (ADR-0015).
--
-- Every quarantine step is appended here, the intent before the move and the
-- outcome after it, so a crash at any point can be reconciled on the next
-- start. `body` is the entry's JSON; `operation_id` and `event` are copies for
-- lookups. Rows are never updated or deleted.

CREATE TABLE quarantine_journal (
    seq             INTEGER PRIMARY KEY,
    operation_id    TEXT NOT NULL CHECK (length(operation_id) = 36),
    event           TEXT NOT NULL CHECK (length(event) BETWEEN 1 AND 32),
    recorded_secs   INTEGER NOT NULL,
    recorded_nanos  INTEGER NOT NULL CHECK (recorded_nanos BETWEEN 0 AND 999999999),
    body            BLOB NOT NULL CHECK (length(body) > 0)
) STRICT;

CREATE INDEX quarantine_journal_operation ON quarantine_journal (operation_id, seq);

CREATE TRIGGER quarantine_journal_append_only_update BEFORE UPDATE ON quarantine_journal
BEGIN
    SELECT RAISE(ABORT, 'quarantine_journal is append-only');
END;

CREATE TRIGGER quarantine_journal_append_only_delete BEFORE DELETE ON quarantine_journal
BEGIN
    SELECT RAISE(ABORT, 'quarantine_journal is append-only');
END;
