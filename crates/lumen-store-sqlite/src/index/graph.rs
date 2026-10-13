//! Persisting the evidence graph (ADR-0013, schema v3).
//!
//! Evidence rows are verified on every read: the stored ID must equal the hash of
//! the stored body. Loaded relationships go back through
//! [`Relationship::new`] and [`Graph::add`], so domain validation and acyclic
//! containment hold for whatever the database contains.

use std::collections::BTreeMap;

use lumen_domain::{Evidence, EvidenceId, RelationKind, Relationship, Subject};
use lumen_graph::Graph;
use rusqlite::{OptionalExtension as _, params};

use super::IndexStore;
use crate::StoreError;

fn corrupt(column: &'static str) -> StoreError {
    StoreError::Corrupt { column }
}

fn subject_key(subject: &Subject) -> Result<Vec<u8>, StoreError> {
    serde_json::to_vec(subject).map_err(|_| corrupt("subjects.key"))
}

fn kind_text(kind: RelationKind) -> Result<String, StoreError> {
    match serde_json::to_value(kind) {
        Ok(serde_json::Value::String(text)) => Ok(text),
        _ => Err(corrupt("relationships.kind")),
    }
}

fn evidence_from_row(id: &[u8], body: &[u8]) -> Result<Evidence, StoreError> {
    let evidence: Evidence = serde_json::from_slice(body).map_err(|_| corrupt("evidence.body"))?;
    // Content addressing: a row whose body does not hash to its ID was altered.
    if evidence.id().as_bytes()[..] != *id || evidence.canonical_bytes() != body {
        return Err(corrupt("evidence.id"));
    }
    Ok(evidence)
}

impl IndexStore {
    /// Stores evidence records. Records are immutable and content-addressed, so a
    /// record already present is left as is.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] if SQLite fails; nothing is stored then.
    pub fn record_evidence(&mut self, records: &[Evidence]) -> Result<(), StoreError> {
        let tx = self.conn.transaction()?;
        {
            let mut insert =
                tx.prepare_cached("INSERT OR IGNORE INTO evidence (id, body) VALUES (?1, ?2)")?;
            for record in records {
                insert.execute(params![
                    &record.id().as_bytes()[..],
                    record.canonical_bytes()
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// The evidence record with this ID, verified against its hash.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Corrupt`] if the stored record does not match its ID,
    /// or [`StoreError`] if SQLite fails.
    pub fn evidence(&self, id: &EvidenceId) -> Result<Option<Evidence>, StoreError> {
        let mut query = self
            .conn
            .prepare_cached("SELECT id, body FROM evidence WHERE id = ?1")?;
        let row = query
            .query_row(params![&id.as_bytes()[..]], |row| {
                Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?))
            })
            .optional()?;
        row.map(|(id, body)| evidence_from_row(&id, &body))
            .transpose()
    }

    /// Stores relationships, their subjects and their evidence links. The
    /// evidence they cite must already be stored ([`Self::record_evidence`]).
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] if SQLite fails, including a foreign-key failure for
    /// evidence that is not stored; nothing is stored then.
    pub fn record_relationships(
        &mut self,
        relationships: &[Relationship],
    ) -> Result<(), StoreError> {
        let tx = self.conn.transaction()?;
        {
            let mut subject =
                tx.prepare_cached("INSERT OR IGNORE INTO subjects (key) VALUES (?1)")?;
            let mut edge = tx.prepare_cached(
                "INSERT OR IGNORE INTO relationships (from_key, kind, to_key) VALUES (?1, ?2, ?3)",
            )?;
            let mut cites = tx.prepare_cached(
                "INSERT OR IGNORE INTO relationship_evidence (from_key, kind, to_key, evidence)
                 VALUES (?1, ?2, ?3, ?4)",
            )?;
            for relationship in relationships {
                let from = subject_key(relationship.from())?;
                let to = subject_key(relationship.to())?;
                let kind = kind_text(relationship.kind())?;
                subject.execute(params![from])?;
                subject.execute(params![to])?;
                edge.execute(params![from, kind, to])?;
                for evidence in relationship.evidence() {
                    cites.execute(params![from, kind, to, &evidence.as_bytes()[..]])?;
                }
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Loads every stored subject and relationship into a [`Graph`].
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Corrupt`] for a row that does not decode, a
    /// relationship the domain rejects, or containment that would form a cycle;
    /// [`StoreError`] if SQLite fails.
    pub fn load_graph(&self) -> Result<Graph, StoreError> {
        let mut graph = Graph::new();
        let mut subjects = self
            .conn
            .prepare_cached("SELECT key FROM subjects ORDER BY key")?;
        let keys = subjects.query_map([], |row| row.get::<_, Vec<u8>>(0))?;
        for key in keys {
            let subject: Subject =
                serde_json::from_slice(&key?).map_err(|_| corrupt("subjects.key"))?;
            graph.add_subject(&subject);
        }

        let mut cited = self.conn.prepare_cached(
            "SELECT from_key, kind, to_key, evidence FROM relationship_evidence
             ORDER BY from_key, kind, to_key, evidence",
        )?;
        let rows = cited.query_map([], |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Vec<u8>>(2)?,
                row.get::<_, Vec<u8>>(3)?,
            ))
        })?;
        let mut edges: BTreeMap<(Vec<u8>, String, Vec<u8>), Vec<EvidenceId>> = BTreeMap::new();
        for row in rows {
            let (from, kind, to, evidence) = row?;
            let evidence: [u8; 32] = evidence
                .try_into()
                .map_err(|_| corrupt("relationship_evidence.evidence"))?;
            edges
                .entry((from, kind, to))
                .or_default()
                .push(EvidenceId::from_bytes(evidence));
        }
        for ((from, kind, to), evidence) in edges {
            let from: Subject =
                serde_json::from_slice(&from).map_err(|_| corrupt("relationships.from_key"))?;
            let to: Subject =
                serde_json::from_slice(&to).map_err(|_| corrupt("relationships.to_key"))?;
            let kind: RelationKind = serde_json::from_value(serde_json::Value::String(kind))
                .map_err(|_| corrupt("relationships.kind"))?;
            let relationship = Relationship::new(from, kind, to, evidence)
                .map_err(|_| corrupt("relationships"))?;
            graph
                .add(relationship)
                .map_err(|_| corrupt("relationships"))?;
        }
        Ok(graph)
    }
}

#[cfg(test)]
mod tests {
    use lumen_domain::{
        AppId, AppIdScheme, Basis, Fact, FileId, FileIdentity, Provenance, SourceName, Timestamp,
        UntrustedText, VolumeId,
    };
    use lumen_graph::Direction;

    use super::*;
    use crate::open_index;

    fn store() -> Result<(IndexStore, tempfile::TempDir), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let store = IndexStore::new(open_index(&dir.path().join("index.db"))?);
        Ok((store, dir))
    }

    fn file(n: u128) -> Subject {
        Subject::File {
            identity: FileIdentity {
                volume: VolumeId::new("v").unwrap_or_else(|_| unreachable!()),
                file: FileId::new(n),
            },
        }
    }

    fn app() -> Subject {
        Subject::Application {
            id: AppId {
                scheme: AppIdScheme::BundleId,
                value: UntrustedText::new("com.example.app"),
            },
        }
    }

    fn owned(subject: Subject) -> Evidence {
        Evidence {
            subject,
            fact: Fact::OwningApplication(AppId {
                scheme: AppIdScheme::BundleId,
                value: UntrustedText::new("com.example.app"),
            }),
            basis: Basis::Observed,
            provenance: Provenance {
                source: SourceName::new("macos.test").unwrap_or_else(|_| unreachable!()),
                observed_at: Timestamp::UNIX_EPOCH,
            },
        }
    }

    #[test]
    fn evidence_round_trips_and_is_verified() -> Result<(), Box<dyn std::error::Error>> {
        let (mut store, _dir) = store()?;
        let record = owned(file(1));
        store.record_evidence(&[record.clone(), record.clone()])?;
        assert_eq!(store.evidence(&record.id())?, Some(record.clone()));
        assert_eq!(store.evidence(&EvidenceId::from_bytes([0; 32]))?, None);

        // A body altered in place no longer matches its ID.
        store.conn.execute(
            "UPDATE evidence SET body = ?1 WHERE id = ?2",
            params![
                owned(file(2)).canonical_bytes(),
                &record.id().as_bytes()[..]
            ],
        )?;
        assert!(matches!(
            store.evidence(&record.id()),
            Err(StoreError::Corrupt { .. })
        ));
        Ok(())
    }

    #[test]
    fn graphs_round_trip_with_merged_evidence() -> Result<(), Box<dyn std::error::Error>> {
        let (mut store, _dir) = store()?;
        let (a, b) = (owned(file(1)), owned(file(2)));
        store.record_evidence(&[a.clone(), b.clone()])?;
        let contains = Relationship::new(file(10), RelationKind::Contains, file(1), [a.id()])?;
        let owned_by = Relationship::new(file(1), RelationKind::OwnedBy, app(), [a.id()])?;
        let owned_again = Relationship::new(file(1), RelationKind::OwnedBy, app(), [b.id()])?;
        store.record_relationships(&[contains, owned_by])?;
        store.record_relationships(&[owned_again])?;

        let graph = store.load_graph()?;
        assert_eq!(graph.subject_count(), 3);
        assert_eq!(graph.relationship_count(), 2);
        let owners: Vec<_> = graph
            .edges(&file(1), Direction::Outgoing, Some(RelationKind::OwnedBy))
            .map(|r| r.evidence().len())
            .collect();
        assert_eq!(owners, [2], "evidence from both writes");
        Ok(())
    }

    #[test]
    fn relationships_cannot_cite_unstored_evidence() -> Result<(), Box<dyn std::error::Error>> {
        let (mut store, _dir) = store()?;
        let missing = owned(file(1));
        let edge = Relationship::new(file(1), RelationKind::OwnedBy, app(), [missing.id()])?;
        assert!(store.record_relationships(&[edge]).is_err());
        assert_eq!(store.load_graph()?.subject_count(), 0, "nothing was stored");
        Ok(())
    }

    #[test]
    fn loading_rejects_stored_containment_cycles() -> Result<(), Box<dyn std::error::Error>> {
        let (mut store, _dir) = store()?;
        let e = owned(file(1));
        store.record_evidence(std::slice::from_ref(&e))?;
        // The store keeps rows as given; the graph refuses the cycle when loading.
        store.record_relationships(&[
            Relationship::new(file(1), RelationKind::Contains, file(2), [e.id()])?,
            Relationship::new(file(2), RelationKind::Contains, file(1), [e.id()])?,
        ])?;
        assert!(matches!(
            store.load_graph(),
            Err(StoreError::Corrupt { .. })
        ));
        Ok(())
    }
}
