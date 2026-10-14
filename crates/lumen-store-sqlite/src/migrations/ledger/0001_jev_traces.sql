-- ledger.db schema v1: Jev traces (ADR-0012, docs/ai/model-contract.md).
--
-- The ledger holds audit records: rows are appended, never updated or deleted,
-- which the triggers enforce. `body` is the trace's canonical JSON
-- (`jev.trace/1`); the hash columns are copies kept for lookups.

CREATE TABLE jev_traces (
    seq            INTEGER PRIMARY KEY,
    requested_secs INTEGER NOT NULL,
    requested_nanos INTEGER NOT NULL CHECK (requested_nanos BETWEEN 0 AND 999999999),
    request_hash   BLOB NOT NULL CHECK (length(request_hash) = 32),
    evidence_hash  BLOB NOT NULL CHECK (length(evidence_hash) = 32),
    body           BLOB NOT NULL CHECK (length(body) > 0)
) STRICT;

CREATE INDEX jev_traces_request ON jev_traces (request_hash);
CREATE INDEX jev_traces_evidence ON jev_traces (evidence_hash);

CREATE TRIGGER jev_traces_append_only_update BEFORE UPDATE ON jev_traces
BEGIN
    SELECT RAISE(ABORT, 'jev_traces is append-only');
END;

CREATE TRIGGER jev_traces_append_only_delete BEFORE DELETE ON jev_traces
BEGIN
    SELECT RAISE(ABORT, 'jev_traces is append-only');
END;
