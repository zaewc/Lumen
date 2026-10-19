-- index.db schema v5: drop `entries_by_identity`.
--
-- No query looks entries up by identity: every read filters on the primary key
-- `(scan_id, flavor, path)`, and hard links are counted in memory while a scan
-- streams (ADR-0017). The index repeated each entry's scan, volume and path,
-- about 40% of a scan's storage (76 of 195 MB for 198,495 entries).
DROP INDEX IF EXISTS entries_by_identity;
