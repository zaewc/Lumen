//! Choosing what to assess and assembling the policy's input (roadmap 10.8,
//! ADR-0013, ADR-0014).
//!
//! **Selection.** A scanned file or directory is a candidate when some
//! evidence says what it is: a knowledge-base match, an application data
//! location, or a vendor's cache declaration. A candidate inside another
//! candidate is dropped, so space is never counted twice and the outer item
//! is decided as a whole.
//!
//! **Assembly.** The candidate's bundle is the relationships within
//! [`BUNDLE_DEPTH`] hops of it, plus every piece of evidence about it or about
//! a subject those relationships reach (such as the owning application's
//! installation state), plus the fresh evidence probed for it at assessment
//! time (in use, ownership). The bundle's hash is recorded with the decision,
//! so the decision can be replayed from exactly what it saw.

use std::collections::{BTreeMap, BTreeSet};

use lumen_domain::{
    EntryKind, Evidence, EvidenceId, Fact, FilesystemEntry, RawPath, SourceName, Subject,
};
use lumen_graph::{EvidenceBundle, Graph};
use lumen_policy::{Context, KnowledgeClass, PolicyInput};

use crate::gather::Gathered;
use crate::observe::{below, observed};
use crate::ports::DirOutcome;

/// How many relationship hops around a candidate its bundle covers.
pub const BUNDLE_DEPTH: usize = 2;

/// Whether `fact` says what a filesystem object is (and so makes it worth
/// deciding about).
const fn identifies(fact: &Fact) -> bool {
    matches!(
        fact,
        Fact::KnowledgeBaseMatch { .. }
            | Fact::AppDataLocation { .. }
            | Fact::VendorDeclaredCache { .. }
    )
}

/// The scanned entries some evidence identifies, outermost only, in path order.
pub fn select<'a>(gathered: &Gathered, scanned: &'a [FilesystemEntry]) -> Vec<&'a FilesystemEntry> {
    let identified: BTreeSet<&Subject> = gathered
        .evidence
        .iter()
        .filter(|record| identifies(&record.fact))
        .map(|record| &record.subject)
        .collect();
    let mut chosen: Vec<&FilesystemEntry> = scanned
        .iter()
        .filter(|entry| matches!(entry.kind, EntryKind::File | EntryKind::Directory))
        .filter(|entry| {
            identified.contains(&Subject::File {
                identity: entry.identity().clone(),
            })
        })
        .collect();
    chosen.sort_by(|a, b| a.path.cmp(&b.path));
    let mut outermost: Vec<&FilesystemEntry> = Vec::new();
    for entry in chosen {
        if !outermost
            .iter()
            .any(|outer| below(&entry.path, &outer.path))
        {
            outermost.push(entry);
        }
    }
    outermost
}

/// Everything the policy needs to decide about one candidate.
#[derive(Debug, Clone, Copy)]
pub struct Sources<'a> {
    /// The scan's graph (from the recorded relationships).
    pub graph: &'a Graph,
    /// The scan's gathered evidence.
    pub gathered: &'a Gathered,
    /// Every scanned entry.
    pub scanned: &'a [FilesystemEntry],
    /// Every directory listing outcome.
    pub listings: &'a [(RawPath, DirOutcome)],
    /// The machine.
    pub context: &'a Context,
    /// Knowledge-base classes by entry.
    pub knowledge: &'a BTreeMap<SourceName, KnowledgeClass>,
}

/// The policy input for `candidate`, with `fresh` evidence probed for it now.
pub fn assemble(
    candidate: &FilesystemEntry,
    fresh: &[Evidence],
    sources: &Sources<'_>,
) -> PolicyInput {
    let subject = Subject::File {
        identity: candidate.identity().clone(),
    };
    let around = sources.graph.bundle(&subject, BUNDLE_DEPTH, []);
    let mut reached: BTreeSet<&Subject> = BTreeSet::from([&subject]);
    for relationship in around.relationships() {
        reached.insert(relationship.from());
        reached.insert(relationship.to());
    }
    let mut records: BTreeMap<EvidenceId, &Evidence> = sources
        .gathered
        .evidence
        .iter()
        .chain(fresh)
        .filter(|record| reached.contains(&record.subject))
        .map(|record| (record.id(), record))
        .collect();
    let bundle = EvidenceBundle::new(
        subject.clone(),
        around.relationships().iter().cloned(),
        records.keys().copied().collect::<Vec<_>>(),
    );
    // Evidence the relationships cite is part of the bundle too.
    for record in &sources.gathered.evidence {
        if bundle.evidence().contains(&record.id()) {
            records.entry(record.id()).or_insert(record);
        }
    }
    PolicyInput {
        subject,
        evidence_hash: bundle.hash(),
        evidence: records.into_values().cloned().collect(),
        paths: vec![candidate.path.clone()],
        context: sources.context.clone(),
        observed: Some(observed(candidate, sources.scanned, sources.listings)),
        knowledge: sources.knowledge.clone(),
        jev: None,
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use lumen_domain::{
        AccessState, AppDataKind, AppId, AppIdScheme, Basis, ByteCount, EntryTimes, FileId,
        FileIdentity, Platform, Protection, Provenance, RelationKind, Relationship, SizeFacts,
        SizeFlags, Timestamp, UntrustedText, VolumeId,
    };

    use super::*;

    fn entry(path: &str, kind: EntryKind, file: u128) -> FilesystemEntry {
        FilesystemEntry {
            path: RawPath::from_unix_bytes(path.as_bytes().to_vec())
                .unwrap_or_else(|_| unreachable!()),
            kind,
            size: SizeFacts {
                identity: FileIdentity {
                    volume: VolumeId::new("v").unwrap_or_else(|_| unreachable!()),
                    file: FileId::new(file),
                },
                logical: ByteCount::new(4096),
                allocated: ByteCount::new(4096),
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

    fn file(n: u128) -> Subject {
        Subject::File {
            identity: FileIdentity {
                volume: VolumeId::new("v").unwrap_or_else(|_| unreachable!()),
                file: FileId::new(n),
            },
        }
    }

    fn app() -> AppId {
        AppId {
            scheme: AppIdScheme::BundleId,
            value: UntrustedText::new("com.example.gone"),
        }
    }

    fn fact(subject: Subject, fact: Fact) -> Evidence {
        Evidence {
            subject,
            fact,
            basis: Basis::Observed,
            provenance: Provenance {
                source: SourceName::new("test").unwrap_or_else(|_| unreachable!()),
                observed_at: Timestamp::UNIX_EPOCH,
            },
        }
    }

    fn context() -> Context {
        Context {
            platform: Platform::Macos,
            home: None,
            windows_dir: None,
            local_app_data: None,
            mechanisms: BTreeSet::new(),
            complete_sources: BTreeSet::new(),
        }
    }

    /// `/c` (an app's cache, owned by an absent app) with `/c/x` inside, and an
    /// unidentified `/other`.
    fn world() -> (Vec<FilesystemEntry>, Gathered, Graph) {
        let scanned = vec![
            entry("/c", EntryKind::Directory, 1),
            entry("/c/x", EntryKind::File, 2),
            entry("/other", EntryKind::File, 3),
        ];
        let owner = fact(file(1), Fact::OwningApplication(app()));
        let gathered = Gathered {
            evidence: vec![
                owner.clone(),
                fact(
                    file(1),
                    Fact::AppDataLocation {
                        kind: AppDataKind::Caches,
                    },
                ),
                fact(
                    file(2),
                    Fact::AppDataLocation {
                        kind: AppDataKind::Caches,
                    },
                ),
                fact(
                    Subject::Application { id: app() },
                    Fact::ApplicationPresent {
                        app: app(),
                        present: false,
                    },
                ),
                fact(file(3), Fact::CloudBacked(false)),
            ],
            relationships: vec![
                Relationship::new(
                    file(1),
                    RelationKind::OwnedBy,
                    Subject::Application { id: app() },
                    [owner.id()],
                )
                .unwrap_or_else(|e| unreachable!("{e:?}")),
            ],
        };
        let mut graph = Graph::new();
        for relationship in &gathered.relationships {
            graph
                .add(relationship.clone())
                .unwrap_or_else(|e| unreachable!("{e:?}"));
        }
        (scanned, gathered, graph)
    }

    #[test]
    fn only_identified_outermost_entries_are_candidates() {
        let (scanned, gathered, _graph) = world();
        let chosen: Vec<&[u8]> = select(&gathered, &scanned)
            .iter()
            .map(|e| e.path.as_bytes())
            .collect();
        assert_eq!(chosen, [b"/c".as_slice()]);
    }

    #[test]
    fn the_input_carries_the_owner_state_and_fresh_facts_bound_by_the_hash() {
        let (scanned, gathered, graph) = world();
        let knowledge = BTreeMap::new();
        let ctx = context();
        let sources = Sources {
            graph: &graph,
            gathered: &gathered,
            scanned: &scanned,
            listings: &[],
            context: &ctx,
            knowledge: &knowledge,
        };
        let idle = assemble(&scanned[0], &[fact(file(1), Fact::InUse(false))], &sources);
        let facts: Vec<&Fact> = idle.evidence.iter().map(|e| &e.fact).collect();
        assert!(facts.contains(&&Fact::ApplicationPresent {
            app: app(),
            present: false
        }));
        assert!(facts.contains(&&Fact::InUse(false)));
        assert!(
            !facts.contains(&&Fact::CloudBacked(false)),
            "nothing about /other"
        );
        assert_eq!(idle.paths, [scanned[0].path.clone()]);
        assert!(idle.observed.is_some());
        let busy = assemble(&scanned[0], &[fact(file(1), Fact::InUse(true))], &sources);
        assert_ne!(
            idle.evidence_hash, busy.evidence_hash,
            "fresh facts change the hash"
        );
        let again = assemble(&scanned[0], &[fact(file(1), Fact::InUse(false))], &sources);
        assert_eq!(idle.evidence_hash, again.evidence_hash, "deterministic");
    }
}
