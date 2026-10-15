-- index.db schema v4: cleanup plans (ADR-0034).
--
-- A plan is stored as its canonical JSON (`CleanupPlan::canonical_bytes`) next
-- to its hash, checked again on every read, and its lifecycle state. Each item's
-- decision is stored by the item's position in the plan.

CREATE TABLE plans (
    id    TEXT NOT NULL PRIMARY KEY CHECK (length(id) = 36),
    hash  TEXT NOT NULL CHECK (length(hash) > 0),
    body  BLOB NOT NULL CHECK (length(body) > 0),
    state BLOB NOT NULL CHECK (length(state) > 0)
) STRICT;

CREATE TABLE plan_decisions (
    plan_id  TEXT NOT NULL REFERENCES plans (id),
    position INTEGER NOT NULL CHECK (position >= 0),
    decision BLOB NOT NULL CHECK (length(decision) > 0),
    PRIMARY KEY (plan_id, position)
) STRICT, WITHOUT ROWID;
