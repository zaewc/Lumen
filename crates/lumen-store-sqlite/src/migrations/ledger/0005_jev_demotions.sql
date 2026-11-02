-- ledger.db schema v5: items AI review moved to review (audit finding AI-4).
--
-- Written by the application with the traces, for each consultation whose
-- judgment demoted a proposed quarantine. Making a plan assesses the scan again
-- and asks the judge again; a demotion recorded since the latest scan finished
-- still applies to the same item, so a failed or different second answer can
-- never return it to the plan. `identity` is the item's `FileIdentity` and
-- `judgment` the accepted `jev.judgment/1`, both as JSON. Rows are appended,
-- never updated or deleted.

CREATE TABLE jev_demotions (
    seq             INTEGER PRIMARY KEY,
    requested_secs  INTEGER NOT NULL,
    requested_nanos INTEGER NOT NULL CHECK (requested_nanos BETWEEN 0 AND 999999999),
    identity        BLOB NOT NULL CHECK (length(identity) > 0),
    judgment        BLOB NOT NULL CHECK (length(judgment) > 0)
) STRICT;

CREATE INDEX jev_demotions_requested ON jev_demotions (requested_secs, requested_nanos);

CREATE TRIGGER jev_demotions_append_only_update BEFORE UPDATE ON jev_demotions
BEGIN
    SELECT RAISE(ABORT, 'jev_demotions is append-only');
END;

CREATE TRIGGER jev_demotions_append_only_delete BEFORE DELETE ON jev_demotions
BEGIN
    SELECT RAISE(ABORT, 'jev_demotions is append-only');
END;
