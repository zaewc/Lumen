//! Choosing what to assess and assembling the policy's input (roadmap 10.8,
//! ADR-0013, ADR-0014).
//!
//! **Selection.** A scanned file or directory is a candidate when some
//! evidence says what it is: a knowledge-base match, an application data
//! location, or a vendor's cache declaration. A candidate inside another
//! candidate is dropped, so space is never counted twice and the outer item
//! is decided as a whole.
//!
//! **Decision.** Each candidate is probed (in use, ownership), its input
//! assembled, and the policy decides, consulting Jev only when one is
//! configured and a request can be built (roadmap 10.8b).
//!
//! **Assembly.** The candidate's bundle is the relationships within
//! [`BUNDLE_DEPTH`] hops of it, plus every piece of evidence about it or about
//! a subject those relationships reach (such as the owning application's
//! installation state), plus the fresh evidence probed for it at assessment
//! time (in use, ownership). The bundle's hash is recorded with the decision,
//! so the decision can be replayed from exactly what it saw.

use std::collections::{BTreeMap, BTreeSet};

use lumen_domain::{
    EntryKind, Evidence, EvidenceId, Fact, FilesystemEntry, JevRequest, JevTrace, PolicyDecision,
    RawPath, SourceName, Subject, Timestamp,
};
use lumen_graph::{EvidenceBundle, Graph};
use lumen_policy::{Context, KnowledgeClass, PolicyError, PolicyInput, evaluate, explain};

use crate::collect::{in_use_evidence, ownership_evidence};
use crate::consult::{Consulted, decide};
use crate::gather::Gathered;
use crate::observe::{below, observed};
use crate::ports::{Clock, DirOutcome, InUseProbe, JudgeModel, OwnershipProbe};

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

/// The probes run for each candidate when it is assessed, and the source
/// names their facts carry.
#[derive(Clone, Copy)]
pub struct Probes<'a> {
    /// Who has the candidate open.
    pub in_use: &'a dyn InUseProbe,
    /// The in-use probe's source name.
    pub in_use_source: &'a SourceName,
    /// Who owns the candidate.
    pub ownership: &'a dyn OwnershipProbe,
    /// The ownership probe's source name.
    pub ownership_source: &'a SourceName,
}

impl std::fmt::Debug for Probes<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Probes")
            .field("in_use_source", self.in_use_source)
            .field("ownership_source", self.ownership_source)
            .finish_non_exhaustive()
    }
}

/// The facts probed for `candidate` now (roadmap 10.8b): whether it is in use
/// and whether the current user owns it. Unknown answers state nothing.
pub fn fresh_evidence(
    candidate: &FilesystemEntry,
    probes: &Probes<'_>,
    observed_at: Timestamp,
) -> Vec<Evidence> {
    let identity = candidate.identity();
    let in_use = probes.in_use.probe(&candidate.path, identity);
    let owned = probes
        .ownership
        .owned_by_current_user(&candidate.path, identity);
    in_use_evidence(identity, &in_use, probes.in_use_source, observed_at)
        .into_iter()
        .chain(ownership_evidence(
            identity,
            owned,
            probes.ownership_source,
            observed_at,
        ))
        .collect()
}

/// One assessed candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assessed {
    /// The candidate as scanned.
    pub candidate: FilesystemEntry,
    /// The input the decision was made from.
    pub input: PolicyInput,
    /// The decision.
    pub decision: PolicyDecision,
    /// The Jev consultation, if one happened.
    pub trace: Option<JevTrace>,
    /// Why, in plain sentences.
    pub reasons: Vec<String>,
}

/// Probes `candidate`, assembles its input and decides it. Jev is consulted
/// only when `judge` is configured and `request` builds a request for the
/// input; otherwise the policy decides alone.
///
/// # Errors
///
/// Only the policy's own programming errors; callers treat them as `Keep`.
pub async fn decide_candidate(
    candidate: &FilesystemEntry,
    sources: &Sources<'_>,
    probes: &Probes<'_>,
    judge: &dyn JudgeModel,
    clock: &dyn Clock,
    request: &dyn Fn(&PolicyInput) -> Option<JevRequest>,
) -> Result<Assessed, PolicyError> {
    let fresh = fresh_evidence(candidate, probes, clock.now());
    let input = assemble(candidate, &fresh, sources);
    let consulted = match (judge.descriptor(), request(&input)) {
        (Some(_), Some(jev_request)) => decide(judge, clock, input.clone(), &jev_request).await?,
        _ => Consulted {
            decision: evaluate(&input)?,
            trace: None,
        },
    };
    Ok(Assessed {
        candidate: candidate.clone(),
        reasons: explain(&consulted.decision),
        decision: consulted.decision,
        trace: consulted.trace,
        input,
    })
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

    struct Probe(crate::ports::InUse);

    impl InUseProbe for Probe {
        fn probe(&self, _path: &RawPath, _expected: &FileIdentity) -> crate::ports::InUse {
            self.0.clone()
        }
    }

    struct Owner(Option<bool>);

    impl OwnershipProbe for Owner {
        fn owned_by_current_user(&self, _path: &RawPath, _expected: &FileIdentity) -> Option<bool> {
            self.0
        }
    }

    struct Epoch;

    impl Clock for Epoch {
        fn now(&self) -> Timestamp {
            Timestamp::UNIX_EPOCH
        }
    }

    fn ready<T>(future: impl Future<Output = T>) -> T {
        let mut future = std::pin::pin!(future);
        match future
            .as_mut()
            .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
        {
            std::task::Poll::Ready(value) => value,
            std::task::Poll::Pending => unreachable!("no judge, nothing to wait for"),
        }
    }

    #[test]
    fn candidates_are_probed_decided_and_explained_without_jev() {
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
        let source = SourceName::new("test").unwrap_or_else(|_| unreachable!());
        for (answer, owned, in_use_fact) in [
            (
                crate::ports::InUse::No,
                Some(true),
                Some(Fact::InUse(false)),
            ),
            (
                crate::ports::InUse::Yes(Vec::new()),
                None,
                Some(Fact::InUse(true)),
            ),
            (
                crate::ports::InUse::Unknown { reason: "x" },
                Some(false),
                None,
            ),
        ] {
            let probe = Probe(answer);
            let owner = Owner(owned);
            let probes = Probes {
                in_use: &probe,
                in_use_source: &source,
                ownership: &owner,
                ownership_source: &source,
            };
            let assessed = ready(decide_candidate(
                &scanned[0],
                &sources,
                &probes,
                &crate::ports::NoopJudge,
                &Epoch,
                &|_| None,
            ))
            .unwrap_or_else(|e| unreachable!("{e}"));
            let facts: Vec<&Fact> = assessed.input.evidence.iter().map(|e| &e.fact).collect();
            match &in_use_fact {
                Some(fact) => assert!(facts.contains(&fact)),
                None => assert!(!facts.iter().any(|f| matches!(f, Fact::InUse(_)))),
            }
            assert_eq!(
                facts.contains(&&Fact::OwnedByCurrentUser(true)),
                owned == Some(true)
            );
            assert_eq!(assessed.trace, None, "no judge, no consultation");
            assert!(!assessed.reasons.is_empty(), "every decision is explained");
            assert_eq!(assessed.candidate, scanned[0]);
        }
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
