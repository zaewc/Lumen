//! Choosing what to assess and assembling the policy's input (roadmap 10.8,
//! ADR-0013, ADR-0014).
//!
//! **Selection.** A scanned file or directory is a candidate when some
//! evidence says what it is: a knowledge-base match, an application data
//! location, or a vendor's cache declaration. A candidate inside another
//! candidate is dropped, so the outer item is decided as a whole, with one
//! exception: a knowledge-base match inside an application's state (its
//! support folder, container or preferences) is decided on its own, such as
//! an Electron app's `Cache` folder inside its `Application Support` folder.
//! The policy never proposes state for quarantine, so the two never overlap in
//! a plan, and the state's own decision covers the rest of it
//! ([`nested`]): space is never counted twice.
//!
//! **Decision.** Each candidate is probed (in use, ownership), its input
//! assembled, and the policy decides, consulting Jev only when one is
//! configured and a request can be built (roadmap 10.8b).
//!
//! **Closing apps.** An item kept only because a running process uses it is
//! also evaluated as if that process were gone ([`FreeWhenClosed`]), so people
//! can be told which apps to quit to free its space. That counterfactual is
//! display only: it is never the decision, never planned, never sent to Jev.
//!
//! **Assembly.** The candidate's bundle is the relationships within
//! [`BUNDLE_DEPTH`] hops of it, plus every piece of evidence about it or about
//! a subject those relationships reach (such as the owning application's
//! installation state), plus the fresh evidence probed for it at assessment
//! time (in use, ownership). The bundle's hash is recorded with the decision,
//! so the decision can be replayed from exactly what it saw.

use std::collections::{BTreeMap, BTreeSet};

use lumen_domain::{
    AppDataKind, CleanupMechanism, EntryKind, Evidence, EvidenceId, Fact, FilesystemEntry,
    ItemNote, JevRequest, JevTrace, PathFlavor, PolicyDecision, Process, RawPath, SourceName,
    Subject, Timestamp, Verdict, VolumeId,
};
use lumen_graph::{EvidenceBundle, Graph};
use lumen_policy::{Context, KnowledgeClass, PolicyError, PolicyInput, evaluate, explain};

use crate::collect::{in_use_evidence, ownership_evidence};
use crate::consult::{Consulted, decide};
use crate::gather::Gathered;
use crate::observe::{below, observed};
use crate::ports::{Clock, DirOutcome, InUse, InUseProbe, JudgeModel, OwnershipProbe};

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

/// Whether `fact` says a filesystem object is an application's own state:
/// a data location of any kind but `Caches`.
const fn is_state(fact: &Fact) -> bool {
    matches!(fact, Fact::AppDataLocation { kind } if !matches!(kind, AppDataKind::Caches))
}

/// The scanned entries some evidence identifies, in path order: the outermost
/// ones, and the knowledge-base matches inside an outer candidate that is only
/// identified as an application's state.
///
/// Such state is never proposed for quarantine (the policy reviews or keeps
/// it), so a match inside it, decided on its own, never overlaps another
/// proposed item. A match inside anything else (another match, a cache) is
/// decided with its outer item.
pub fn select<'a>(gathered: &Gathered, scanned: &'a [FilesystemEntry]) -> Vec<&'a FilesystemEntry> {
    let mut identified: BTreeMap<&Subject, Vec<&Fact>> = BTreeMap::new();
    for record in &gathered.evidence {
        if identifies(&record.fact) {
            identified
                .entry(&record.subject)
                .or_default()
                .push(&record.fact);
        }
    }
    let facts = |entry: &FilesystemEntry| {
        identified
            .get(&Subject::File {
                identity: entry.identity().clone(),
            })
            .map_or(&[][..], Vec::as_slice)
    };
    let mut chosen: Vec<&FilesystemEntry> = scanned
        .iter()
        .filter(|entry| matches!(entry.kind, EntryKind::File | EntryKind::Directory))
        .filter(|entry| !facts(entry).is_empty())
        .collect();
    chosen.sort_by(|a, b| a.path.cmp(&b.path));
    let mut selected: Vec<&FilesystemEntry> = Vec::new();
    for entry in chosen {
        let matched = facts(entry)
            .iter()
            .any(|fact| matches!(fact, Fact::KnowledgeBaseMatch { .. }));
        let mut enclosing = selected
            .iter()
            .filter(|outer| below(&entry.path, &outer.path))
            .peekable();
        let free = enclosing.peek().is_none();
        if free || matched && enclosing.all(|outer| facts(outer).iter().all(|f| is_state(f))) {
            selected.push(entry);
        }
    }
    selected
}

/// The paths of the `candidates` strictly below `candidate`. They are
/// decided on their own, so they are no part of `candidate`'s decision: its
/// observation must leave them out, or their space would be counted twice.
pub fn nested<'a>(
    candidate: &FilesystemEntry,
    candidates: &[&'a FilesystemEntry],
) -> Vec<&'a RawPath> {
    candidates
        .iter()
        .filter(|inner| below(&inner.path, &candidate.path))
        .map(|inner| &inner.path)
        .collect()
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
    /// The volumes with a quarantine store (ADR-0046), or `None` when every
    /// volume is offered to the executor (which refuses those it cannot
    /// serve). A candidate on another volume gets no quarantine rename, so it
    /// is left for review instead of failing when moved.
    pub store_volumes: Option<&'a BTreeSet<VolumeId>>,
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
    let mut context = sources.context.clone();
    if sources
        .store_volumes
        .is_some_and(|volumes| !volumes.contains(&candidate.identity().volume))
    {
        context
            .mechanisms
            .remove(&CleanupMechanism::QuarantineRename);
    }
    PolicyInput {
        subject,
        evidence_hash: bundle.hash(),
        evidence: records.into_values().cloned().collect(),
        paths: vec![candidate.path.clone()],
        context,
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
    probe(candidate, probes, observed_at).1
}

/// [`fresh_evidence`], with the in-use probe's own answer.
fn probe(
    candidate: &FilesystemEntry,
    probes: &Probes<'_>,
    observed_at: Timestamp,
) -> (InUse, Vec<Evidence>) {
    let identity = candidate.identity();
    let in_use = probes.in_use.probe(&candidate.path, identity);
    let owned = probes
        .ownership
        .owned_by_current_user(&candidate.path, identity);
    let fresh = in_use_evidence(identity, &in_use, probes.in_use_source, observed_at)
        .into_iter()
        .chain(ownership_evidence(
            identity,
            owned,
            probes.ownership_source,
            observed_at,
        ))
        .collect();
    (in_use, fresh)
}

/// The rule that keeps an item a running process uses.
const IN_USE_RULE: &str = "protect.in-use";

/// An item kept only because running processes use it, which the policy
/// would propose for quarantine once they are closed. Display only: the
/// item's verdict stays `Keep`, and nothing plans, confirms or executes from
/// this.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FreeWhenClosed {
    /// What the probe reported using it, as display-safe names
    /// ([`process_name`]), sorted and without duplicates. Empty when the
    /// platform did not name them.
    pub processes: Vec<String>,
}

/// A display-safe name for `process`: the outermost `.app` bundle on its
/// executable path without the extension (`Spotify` for a helper inside
/// `Spotify.app`), otherwise the executable's file name. Escaped like every
/// displayed path; `None` when the executable is unknown.
pub fn process_name(process: &Process) -> Option<String> {
    let path = process.executable.as_ref()?;
    // Escaping never produces a separator, so splitting the display string
    // splits the path.
    let shown = path.display();
    let separators: &[char] = match path.flavor() {
        PathFlavor::Unix => &['/'],
        PathFlavor::Windows => &['/', '\\'],
    };
    let mut parts = shown.split(separators).filter(|part| !part.is_empty());
    let bundle = parts
        .clone()
        .find_map(|part| part.strip_suffix(".app"))
        .filter(|name| !name.is_empty());
    bundle.or_else(|| parts.next_back()).map(str::to_owned)
}

/// Whether `candidate`, kept by `decision` because the processes in `in_use`
/// use it, would be proposed for quarantine without them: its input is
/// evaluated again, by the policy alone, with the fresh in-use fact replaced
/// by each answer a later probe could give when nothing visible uses it
/// (`No`, or the `Partial` an unprivileged probe gives; ADR-0030). Only
/// when both would be proposed. Nothing else is tried: an item also kept for
/// another reason stays kept here too.
fn free_when_closed(
    candidate: &FilesystemEntry,
    in_use: &InUse,
    fresh: &[Evidence],
    decision: &PolicyDecision,
    sources: &Sources<'_>,
    probes: &Probes<'_>,
    observed_at: Timestamp,
) -> Option<FreeWhenClosed> {
    let InUse::Yes(processes) = in_use else {
        return None;
    };
    let kept_in_use = decision.verdict() == Verdict::Keep
        && decision
            .fired_rules()
            .iter()
            .any(|fired| fired.rule.as_str() == IN_USE_RULE);
    if !kept_in_use {
        return None;
    }
    let proposed = [InUse::No, InUse::Partial { unseen: None }]
        .iter()
        .all(|answer| {
            let unused = in_use_evidence(
                candidate.identity(),
                answer,
                probes.in_use_source,
                observed_at,
            );
            let counterfactual: Vec<Evidence> = fresh
                .iter()
                .filter(|record| record.fact != Fact::InUse(true))
                .cloned()
                .chain(unused)
                .collect();
            evaluate(&assemble(candidate, &counterfactual, sources))
                .is_ok_and(|would| would.verdict() == Verdict::Quarantine)
        });
    let names: BTreeSet<String> = processes.iter().filter_map(process_name).collect();
    proposed.then(|| FreeWhenClosed {
        processes: names.into_iter().collect(),
    })
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
    /// What AI review says a review item most likely is, when it was asked
    /// and the answer may be shown (ADR-0041). Display only: nothing decides
    /// from it.
    pub note: Option<ItemNote>,
    /// The description request, if one was made ([`crate::describe`]).
    pub note_trace: Option<JevTrace>,
    /// Whether closing the processes that use it would let the policy propose
    /// it ([`FreeWhenClosed`]). Display only: nothing decides from it.
    pub free_when_closed: Option<FreeWhenClosed>,
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
    let observed_at = clock.now();
    let (in_use, fresh) = probe(candidate, probes, observed_at);
    let input = assemble(candidate, &fresh, sources);
    let consulted = match (judge.descriptor(), request(&input)) {
        (Some(_), Some(jev_request)) => decide(judge, clock, input.clone(), &jev_request).await?,
        _ => Consulted {
            decision: evaluate(&input)?,
            trace: None,
        },
    };
    let free_when_closed = free_when_closed(
        candidate,
        &in_use,
        &fresh,
        &consulted.decision,
        sources,
        probes,
        observed_at,
    );
    Ok(Assessed {
        free_when_closed,
        candidate: candidate.clone(),
        reasons: explain(&consulted.decision),
        decision: consulted.decision,
        trace: consulted.trace,
        input,
        note: None,
        note_trace: None,
    })
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use lumen_domain::{
        AccessState, AppDataKind, AppId, AppIdScheme, Basis, ByteCount, EntryTimes, FileId,
        FileIdentity, Platform, Protection, Provenance, RelationKind, Relationship, SizeFacts,
        SizeFlags, UntrustedText,
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
            single_user: false,
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

    struct Probe(InUse);

    impl InUseProbe for Probe {
        fn probe(&self, _path: &RawPath, _expected: &FileIdentity) -> InUse {
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
    fn only_candidates_on_a_store_volume_keep_the_quarantine_rename() {
        let (scanned, gathered, graph) = world();
        let knowledge = BTreeMap::new();
        let rename = BTreeSet::from([CleanupMechanism::QuarantineRename]);
        let ctx = Context {
            mechanisms: rename.clone(),
            ..context()
        };
        let candidate = &scanned[0];
        let mechanisms = |store_volumes: Option<&BTreeSet<VolumeId>>| {
            let sources = Sources {
                graph: &graph,
                gathered: &gathered,
                scanned: &scanned,
                listings: &[],
                context: &ctx,
                knowledge: &knowledge,
                store_volumes,
            };
            assemble(candidate, &[], &sources).context.mechanisms
        };
        assert_eq!(ctx.mechanisms, rename);
        assert_eq!(mechanisms(None), rename, "every volume offered");
        let here = BTreeSet::from([candidate.identity().volume.clone()]);
        assert_eq!(mechanisms(Some(&here)), rename);
        let elsewhere = BTreeSet::from([
            VolumeId::new("vsn:00000000000000ff").unwrap_or_else(|_| unreachable!())
        ]);
        assert!(mechanisms(Some(&elsewhere)).is_empty(), "no store there");
        assert!(mechanisms(Some(&BTreeSet::new())).is_empty());
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
            store_volumes: None,
        };
        let source = SourceName::new("test").unwrap_or_else(|_| unreachable!());
        for (answer, owned, in_use_fact) in [
            (InUse::No, Some(true), Some(Fact::InUse(false))),
            (InUse::Yes(Vec::new()), None, Some(Fact::InUse(true))),
            (InUse::Unknown { reason: "x" }, Some(false), None),
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

    fn process(executable: Option<&str>) -> Process {
        Process {
            pid: 1,
            started_at: None,
            executable: executable.map(|path| {
                RawPath::from_unix_bytes(path.as_bytes().to_vec())
                    .unwrap_or_else(|_| unreachable!())
            }),
            executable_identity: None,
            application: None,
        }
    }

    /// `world()`'s cache, owned by the user below their home, decided with
    /// the in-use probe answering `answer`; it could be quarantined only when
    /// `rename` is offered.
    fn decided(answer: InUse, rename: bool) -> Assessed {
        let (scanned, gathered, graph) = world();
        let knowledge = BTreeMap::new();
        let ctx = Context {
            home: RawPath::from_unix_bytes(b"/".to_vec()).ok(),
            mechanisms: if rename {
                BTreeSet::from([CleanupMechanism::QuarantineRename])
            } else {
                BTreeSet::new()
            },
            ..context()
        };
        let listings = [(
            scanned[0].path.clone(),
            DirOutcome {
                access: AccessState::Readable,
                complete: true,
            },
        )];
        let sources = Sources {
            graph: &graph,
            gathered: &gathered,
            scanned: &scanned,
            listings: &listings,
            context: &ctx,
            knowledge: &knowledge,
            store_volumes: None,
        };
        let source = SourceName::new("test").unwrap_or_else(|_| unreachable!());
        let probe = Probe(answer);
        let owner = Owner(Some(true));
        let probes = Probes {
            in_use: &probe,
            in_use_source: &source,
            ownership: &owner,
            ownership_source: &source,
        };
        ready(decide_candidate(
            &scanned[0],
            &sources,
            &probes,
            &crate::ports::NoopJudge,
            &Epoch,
            &|_| None,
        ))
        .unwrap_or_else(|e| unreachable!("{e}"))
    }

    fn fired(assessed: &Assessed) -> Vec<&str> {
        assessed
            .decision
            .fired_rules()
            .iter()
            .map(|fired| fired.rule.as_str())
            .collect()
    }

    #[test]
    fn an_item_kept_in_use_says_what_closing_its_apps_would_free() {
        let assessed = decided(
            InUse::Yes(vec![
                process(Some(
                    "/Applications/Spotify.app/Contents/Frameworks/Spotify Helper.app/Contents/MacOS/Spotify Helper",
                )),
                process(Some("/Applications/Spotify.app/Contents/MacOS/Spotify")),
                process(Some("/usr/bin/vi\u{202e}m")),
                process(None),
            ]),
            true,
        );
        assert_eq!(assessed.decision.verdict(), Verdict::Keep, "still kept");
        assert!(fired(&assessed).contains(&IN_USE_RULE));
        assert!(
            assessed
                .input
                .evidence
                .iter()
                .any(|e| e.fact == Fact::InUse(true)),
            "the input is what the probe said"
        );
        assert_eq!(
            evaluate(&assessed.input).ok().as_ref(),
            Some(&assessed.decision),
            "the decision is the real input's"
        );
        assert_eq!(
            assessed.free_when_closed,
            Some(FreeWhenClosed {
                processes: vec!["Spotify".to_owned(), "vi⟦U+202E⟧m".to_owned()],
            }),
            "named once each, escaped, sorted"
        );
        let unnamed = decided(InUse::Yes(vec![process(None)]), true);
        assert_eq!(
            unnamed.free_when_closed,
            Some(FreeWhenClosed {
                processes: Vec::new()
            })
        );
    }

    #[test]
    fn only_items_kept_in_use_that_would_be_proposed_get_the_signal() {
        let idle = decided(InUse::No, true);
        assert_eq!(
            idle.decision.verdict(),
            Verdict::Quarantine,
            "{:?}",
            fired(&idle)
        );
        assert_eq!(idle.free_when_closed, None, "already proposed");
        let partial = decided(InUse::Partial { unseen: Some(3) }, true);
        assert_eq!(partial.decision.verdict(), Verdict::Quarantine);
        assert_eq!(partial.free_when_closed, None);
        let unknown = decided(InUse::Unknown { reason: "x" }, true);
        assert_eq!(unknown.decision.verdict(), Verdict::Keep);
        assert!(!fired(&unknown).contains(&IN_USE_RULE));
        assert_eq!(unknown.free_when_closed, None, "unknown is not in use");
        let no_rename = decided(InUse::Yes(vec![process(Some("/bin/x"))]), false);
        assert_eq!(no_rename.decision.verdict(), Verdict::Keep);
        assert_eq!(
            no_rename.free_when_closed, None,
            "closing it would only lead to review"
        );
    }

    #[test]
    fn process_names_prefer_the_outermost_app() {
        let name = |path: &str| process_name(&process(Some(path)));
        assert_eq!(
            name("/Applications/Arc.app/Contents/MacOS/Arc"),
            Some("Arc".to_owned())
        );
        assert_eq!(name("/usr/libexec/mds"), Some("mds".to_owned()));
        assert_eq!(name("/odd/.app/bin/tool"), Some("tool".to_owned()));
        assert_eq!(process_name(&process(None)), None);
        let windows = Process {
            executable: RawPath::from_windows_wide(
                &"C:\\Program Files\\Discord\\Discord.exe"
                    .encode_utf16()
                    .collect::<Vec<_>>(),
            )
            .ok(),
            ..process(None)
        };
        assert_eq!(process_name(&windows), Some("Discord.exe".to_owned()));
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

    /// A knowledge-base match inside an application's state is its own
    /// candidate; inside anything else it is part of the outer candidate.
    #[test]
    fn matches_inside_app_state_are_candidates_of_their_own() {
        let source = SourceName::new("devkb").unwrap_or_else(|_| unreachable!());
        let kb = |n| {
            fact(
                file(n),
                Fact::KnowledgeBaseMatch {
                    entry: source.clone(),
                },
            )
        };
        let location = |n, kind| fact(file(n), Fact::AppDataLocation { kind });
        let scanned = vec![
            entry("/support", EntryKind::Directory, 1),
            entry("/support/Cache", EntryKind::Directory, 2),
            entry("/support/Cache/deeper", EntryKind::Directory, 3),
            entry("/support/Local Storage", EntryKind::Directory, 4),
            entry("/caches", EntryKind::Directory, 5),
            entry("/caches/matched", EntryKind::Directory, 6),
            entry("/tool", EntryKind::Directory, 7),
            entry("/tool/matched", EntryKind::Directory, 8),
            entry("/mixed", EntryKind::Directory, 9),
            entry("/mixed/matched", EntryKind::Directory, 10),
        ];
        let gathered = Gathered {
            evidence: vec![
                location(1, AppDataKind::ApplicationSupport),
                fact(file(1), Fact::OwningApplication(app())),
                kb(2),
                kb(3),
                location(5, AppDataKind::Caches),
                kb(6),
                kb(7),
                kb(8),
                location(9, AppDataKind::Container),
                fact(
                    file(9),
                    Fact::VendorDeclaredCache {
                        declared_by: UntrustedText::new("x"),
                    },
                ),
                kb(10),
            ],
            relationships: Vec::new(),
        };
        let chosen = select(&gathered, &scanned);
        let paths: Vec<&[u8]> = chosen.iter().map(|e| e.path.as_bytes()).collect();
        assert_eq!(
            paths,
            [
                b"/caches".as_slice(),
                b"/mixed",
                b"/support",
                b"/support/Cache",
                b"/tool",
            ]
        );
        let inner: Vec<&[u8]> = nested(&scanned[0], &chosen)
            .iter()
            .map(|p| p.as_bytes())
            .collect();
        assert_eq!(inner, [b"/support/Cache".as_slice()]);
        assert_eq!(nested(&scanned[1], &chosen).len(), 0);
        assert_eq!(nested(&scanned[6], &chosen).len(), 0);
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
            store_volumes: None,
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
