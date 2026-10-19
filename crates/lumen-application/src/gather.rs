//! Gathering the evidence and relationships of one scan (roadmap 10.6,
//! docs/architecture/evidence-graph.md).
//!
//! Platform producers (the developer knowledge base, application support
//! locations, declared caches) each contribute evidence and relationships. To
//! those the scan's own facts are added: what was observed of every entry a
//! producer said something about or a relationship connects
//! ([`filesystem_evidence`]), and whether each application named as an owner
//! is installed ([`application_evidence`]).
//!
//! A decision sees only its candidate and the subjects relationships reach
//! from it (roadmap 10.8), and every candidate is an entry some producer
//! identified. The facts of any other entry could never reach a decision, so
//! they are not gathered: on a whole disk that is millions of records. Records are content-addressed, so
//! the same fact from two producers is kept once. The result is what the
//! composition root records in `index.db` and loads back as a graph.
//!
//! Per-item facts that must be fresh (in use, ownership) are not gathered
//! here: they are probed when an item is assessed (roadmap 10.8).

use std::collections::{BTreeMap, BTreeSet};

use lumen_domain::{
    AppId, Evidence, EvidenceId, Fact, FilesystemEntry, Relationship, Subject, Timestamp,
};

use crate::collect::{Inventory, application_evidence, filesystem_evidence};

/// Everything one scan established.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Gathered {
    /// Evidence, unique by ID, in ID order.
    pub evidence: Vec<Evidence>,
    /// Relationships, unique, sorted.
    pub relationships: Vec<Relationship>,
}

/// Combines the producers' `evidence` and `relationships` with the scan's own
/// facts about `entries`, observed when the scan finished (`scanned_at`), and
/// the installation state of every owner, observed now (`observed_at`).
///
/// Dating the scan's facts by the scan, not by the assessment, keeps their
/// IDs stable: assessing the same scan again records nothing new.
pub fn gather(
    entries: &[FilesystemEntry],
    inventory: &Inventory,
    evidence: impl IntoIterator<Item = Evidence>,
    relationships: impl IntoIterator<Item = Relationship>,
    scanned_at: Timestamp,
    observed_at: Timestamp,
) -> Gathered {
    let evidence: Vec<Evidence> = evidence.into_iter().collect();
    let relationships: BTreeSet<Relationship> = relationships.into_iter().collect();
    let mut reachable: BTreeSet<&Subject> = evidence.iter().map(|record| &record.subject).collect();
    for relationship in &relationships {
        reachable.insert(relationship.from());
        reachable.insert(relationship.to());
    }
    let scanned: Vec<Evidence> = entries
        .iter()
        .filter(|entry| {
            reachable.contains(&Subject::File {
                identity: entry.identity().clone(),
            })
        })
        .flat_map(|entry| filesystem_evidence(entry, scanned_at))
        .collect();
    let mut by_id: BTreeMap<EvidenceId, Evidence> = BTreeMap::new();
    for record in evidence.into_iter().chain(scanned) {
        by_id.entry(record.id()).or_insert(record);
    }
    let owners: BTreeSet<AppId> = by_id
        .values()
        .filter_map(|record| match &record.fact {
            Fact::OwningApplication(app) => Some(app.clone()),
            _ => None,
        })
        .collect();
    for record in application_evidence(&owners, inventory, observed_at) {
        by_id.entry(record.id()).or_insert(record);
    }
    Gathered {
        evidence: by_id.into_values().collect(),
        relationships: relationships.into_iter().collect(),
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use lumen_domain::{
        AccessState, AppIdScheme, Basis, ByteCount, EntryKind, EntryTimes, FileId, FileIdentity,
        Protection, Provenance, RawPath, RelationKind, SizeFacts, SizeFlags, SourceName,
        SourceStatus, Subject, UntrustedText, VolumeId,
    };

    use super::*;
    use crate::collect::summarize;
    use crate::ports::InventoryReport;

    fn at(secs: i64) -> Timestamp {
        Timestamp::from_unix(secs, 0).unwrap_or_else(|_| unreachable!())
    }

    fn entry(file: u128) -> FilesystemEntry {
        FilesystemEntry {
            path: RawPath::from_unix_bytes(
                format!("/Users/ana/Library/Caches/{file}").into_bytes(),
            )
            .unwrap_or_else(|_| unreachable!()),
            kind: EntryKind::Directory,
            size: SizeFacts {
                identity: FileIdentity {
                    volume: VolumeId::new("v").unwrap_or_else(|_| unreachable!()),
                    file: FileId::new(file),
                },
                logical: ByteCount::ZERO,
                allocated: ByteCount::ZERO,
                private: None,
                clone_id: None,
                link_count: NonZeroU32::MIN,
                flags: SizeFlags::default(),
            },
            times: EntryTimes::default(),
            protection: Protection::default(),
            access: AccessState::Readable,
        }
    }

    fn app() -> AppId {
        AppId {
            scheme: AppIdScheme::BundleId,
            value: UntrustedText::new("com.example.gone"),
        }
    }

    fn owned(file: u128) -> Evidence {
        Evidence {
            subject: Subject::File {
                identity: entry(file).identity().clone(),
            },
            fact: Fact::OwningApplication(app()),
            basis: Basis::Observed,
            provenance: Provenance {
                source: SourceName::new("macos.support-location.bundle-id")
                    .unwrap_or_else(|_| unreachable!()),
                observed_at: at(1),
            },
        }
    }

    #[test]
    fn producers_scan_facts_and_owner_state_are_combined_once() {
        let inventory = summarize(&[InventoryReport {
            source: SourceName::new("macos.applications").unwrap_or_else(|_| unreachable!()),
            status: SourceStatus::Complete,
            applications: Vec::new(),
            services: Vec::new(),
            packages: Vec::new(),
        }]);
        let ownership = owned(1);
        let relationship = Relationship::new(
            Subject::File {
                identity: entry(1).identity().clone(),
            },
            RelationKind::OwnedBy,
            Subject::Application { id: app() },
            [ownership.id()],
        )
        .unwrap_or_else(|e| unreachable!("{e:?}"));
        let gathered = gather(
            &[entry(1), entry(2)],
            &inventory,
            [ownership.clone(), ownership.clone()],
            [relationship.clone(), relationship.clone()],
            at(1),
            at(2),
        );
        let ids: BTreeSet<EvidenceId> = gathered.evidence.iter().map(Evidence::id).collect();
        assert_eq!(ids.len(), gathered.evidence.len(), "unique by ID");
        assert!(ids.contains(&ownership.id()));
        assert_eq!(gathered.relationships, [relationship]);
        let facts: Vec<&Fact> = gathered.evidence.iter().map(|e| &e.fact).collect();
        assert_eq!(
            facts
                .iter()
                .filter(|f| matches!(f, Fact::Protected(_)))
                .count(),
            1,
            "only the owned entry's protection: nothing reaches entry 2"
        );
        assert!(facts.contains(&&Fact::ApplicationPresent {
            app: app(),
            present: false
        }));
        for record in &gathered.evidence {
            let expected = match record.fact {
                Fact::ApplicationPresent { .. } => at(2),
                _ if record.id() == ownership.id() => record.provenance.observed_at,
                _ => at(1),
            };
            assert_eq!(record.provenance.observed_at, expected, "{:?}", record.fact);
        }
    }

    #[test]
    fn the_same_scan_gathers_the_same_evidence_whenever_it_is_assessed() {
        let inventory = summarize(&[]);
        let ids = |now| {
            gather(
                &[entry(1), entry(2)],
                &inventory,
                [owned(1)],
                [],
                at(1),
                now,
            )
            .evidence
            .iter()
            .map(Evidence::id)
            .collect::<BTreeSet<EvidenceId>>()
        };
        assert_eq!(ids(at(2)), ids(at(3)));
    }

    #[test]
    fn entries_nothing_identifies_or_connects_add_no_evidence() {
        let inventory = summarize(&[]);
        let entries: Vec<FilesystemEntry> = (1..=100).map(entry).collect();
        let gathered = gather(&entries, &inventory, [owned(7)], [], at(1), at(2));
        let subjects: BTreeSet<&Subject> = gathered
            .evidence
            .iter()
            .map(|record| &record.subject)
            .collect();
        let seven = Subject::File {
            identity: entry(7).identity().clone(),
        };
        assert!(
            subjects
                .iter()
                .all(|s| **s == seven || matches!(s, Subject::Application { .. })),
            "{subjects:?}"
        );
        assert!(
            gathered
                .evidence
                .iter()
                .any(|record| record.subject == seven && matches!(record.fact, Fact::Protected(_))),
            "the identified entry keeps its scan facts"
        );
    }
}
