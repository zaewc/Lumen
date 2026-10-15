//! Gathering the evidence and relationships of one scan (roadmap 10.6,
//! docs/architecture/evidence-graph.md).
//!
//! Platform producers (the developer knowledge base, application support
//! locations, declared caches) each contribute evidence and relationships. To
//! those the scan's own facts are added: what was observed of every entry
//! ([`filesystem_evidence`]) and whether each application named as an owner
//! is installed ([`application_evidence`]). Records are content-addressed, so
//! the same fact from two producers is kept once. The result is what the
//! composition root records in `index.db` and loads back as a graph.
//!
//! Per-item facts that must be fresh (in use, ownership) are not gathered
//! here: they are probed when an item is assessed (roadmap 10.8).

use std::collections::{BTreeMap, BTreeSet};

use lumen_domain::{AppId, Evidence, EvidenceId, Fact, FilesystemEntry, Relationship, Timestamp};

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
/// facts about `entries` and the installation state of every owner.
pub fn gather(
    entries: &[FilesystemEntry],
    inventory: &Inventory,
    evidence: impl IntoIterator<Item = Evidence>,
    relationships: impl IntoIterator<Item = Relationship>,
    observed_at: Timestamp,
) -> Gathered {
    let mut by_id: BTreeMap<EvidenceId, Evidence> = BTreeMap::new();
    let scanned = entries
        .iter()
        .flat_map(|entry| filesystem_evidence(entry, observed_at));
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
    let relationships: BTreeSet<Relationship> = relationships.into_iter().collect();
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
            2,
            "both entries' protections"
        );
        assert!(facts.contains(&&Fact::ApplicationPresent {
            app: app(),
            present: false
        }));
    }
}
