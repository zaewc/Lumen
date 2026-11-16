-- ledger.db schema v6: which app each uninstall moved (ADR-0062, ADR-0068).
--
-- Written by the application after an uninstall retained the bundle, from the
-- app it observed right before the move. Lumen's own record that an app with
-- this bundle identifier existed and was removed, so its data in ~/Library can
-- be recognised as left by a removed app. Bundle identifier and name are
-- untrusted text from the bundle. One row per operation; rows are never
-- updated or deleted.

CREATE TABLE uninstalled_apps (
    operation_id TEXT NOT NULL PRIMARY KEY CHECK (length(operation_id) = 36),
    bundle_id    TEXT NOT NULL CHECK (length(bundle_id) BETWEEN 1 AND 255),
    name         TEXT
) STRICT;

CREATE TRIGGER uninstalled_apps_append_only_update BEFORE UPDATE ON uninstalled_apps
BEGIN
    SELECT RAISE(ABORT, 'uninstalled_apps is append-only');
END;

CREATE TRIGGER uninstalled_apps_append_only_delete BEFORE DELETE ON uninstalled_apps
BEGIN
    SELECT RAISE(ABORT, 'uninstalled_apps is append-only');
END;
