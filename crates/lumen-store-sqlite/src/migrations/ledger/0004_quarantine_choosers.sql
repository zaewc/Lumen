-- ledger.db schema v4: who chose each quarantined item (ADR-0051).
--
-- Written by the application after an item is retained, from the confirmed
-- plan. Only items recorded as chosen by the policy are removed for good when
-- their retention ends; an item the person chose, or one with no record, waits
-- for the person. The executor and its journal are unchanged. One row per
-- operation; rows are never updated or deleted.

CREATE TABLE quarantine_choosers (
    operation_id TEXT NOT NULL PRIMARY KEY CHECK (length(operation_id) = 36),
    chooser      TEXT NOT NULL CHECK (chooser IN ('policy', 'person'))
) STRICT;

-- Every item quarantined before this version was proposed by the policy.
INSERT INTO quarantine_choosers (operation_id, chooser)
    SELECT DISTINCT operation_id, 'policy' FROM quarantine_journal;

CREATE TRIGGER quarantine_choosers_append_only_update BEFORE UPDATE ON quarantine_choosers
BEGIN
    SELECT RAISE(ABORT, 'quarantine_choosers is append-only');
END;

CREATE TRIGGER quarantine_choosers_append_only_delete BEFORE DELETE ON quarantine_choosers
BEGIN
    SELECT RAISE(ABORT, 'quarantine_choosers is append-only');
END;
