-- index.db schema v3: the evidence graph (ADR-0013).
--
-- Evidence is immutable and content-addressed: `id` is the BLAKE3 hash of `body`
-- (canonical JSON, `Evidence::canonical_bytes`), checked again on every read.
-- Subjects are keyed by their canonical JSON. A relationship cites at least one
-- piece of evidence, and only evidence the store holds (foreign keys).

CREATE TABLE evidence (
    id   BLOB NOT NULL PRIMARY KEY CHECK (length(id) = 32),
    body BLOB NOT NULL CHECK (length(body) > 0)
) STRICT, WITHOUT ROWID;

CREATE TABLE subjects (
    key BLOB NOT NULL PRIMARY KEY CHECK (length(key) > 0)
) STRICT, WITHOUT ROWID;

CREATE TABLE relationships (
    from_key BLOB NOT NULL REFERENCES subjects (key),
    kind     TEXT NOT NULL CHECK (length(kind) BETWEEN 1 AND 64),
    to_key   BLOB NOT NULL REFERENCES subjects (key),
    PRIMARY KEY (from_key, kind, to_key)
) STRICT, WITHOUT ROWID;

-- "What points at this subject?" (owners, containers, processes).
CREATE INDEX relationships_to ON relationships (to_key, kind);

CREATE TABLE relationship_evidence (
    from_key BLOB NOT NULL,
    kind     TEXT NOT NULL,
    to_key   BLOB NOT NULL,
    evidence BLOB NOT NULL REFERENCES evidence (id),
    PRIMARY KEY (from_key, kind, to_key, evidence),
    FOREIGN KEY (from_key, kind, to_key)
        REFERENCES relationships (from_key, kind, to_key) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;
