//! Turning a confirmed plan into items the executor may act on (roadmap 10.12,
//! ADR-0034).
//!
//! Every planned item is decided again, now, before anything moves:
//!
//! 1. the object at its path must still be the planned object (fresh facts
//!    from the `CandidateVerifier`), and lie under one of the scan roots;
//! 2. it is probed and decided again ([`decide_candidate`]); the verdict must
//!    still be `QUARANTINE` and for the same reasons (the same rules fired) as
//!    the planned decision. The evidence hash cannot be compared: facts probed
//!    now carry the time they were observed, so a fresh bundle never hashes
//!    like the plan's. Any change that alters the decision (in use, ownership,
//!    the owner's state, the item itself) still skips it;
//! 3. the executor records it as it is now (`prepare`), and its
//!    `quarantine.manifest/1` is built.
//!
//! Anything that fails is skipped with the reason and reported as `REVIEW`:
//! a confirmation never authorizes an item the policy would no longer
//! propose.

use std::collections::BTreeSet;

use lumen_domain::{
    Confirmation, DeviceId, JevRequest, OperationId, PathFlavor, PlanId, PlannedItem,
    PolicyDecision, QuarantineManifest, RawPath, Subject, Verdict,
};
use lumen_policy::PolicyInput;

use crate::assess::{Probes, Sources, decide_candidate};
use crate::ports::{CandidateVerifier, Clock, IdGenerator, JudgeModel, QuarantineExecutor};
use crate::quarantine::ConfirmedItem;

/// The rules that accept a partial in-use answer (ADR-0030, ADR-0046).
const PARTIAL_IN_USE: [&str; 2] = [
    "eligible.in-use-partial",
    "eligible.in-use-partial-single-user",
];

/// Why a planned item is not executed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skip {
    /// It is no longer the planned object, or cannot be observed.
    Changed,
    /// It is not under any scan root.
    OutsideRoots,
    /// The policy no longer proposes quarantine.
    NoLongerProposed(Verdict),
    /// The policy proposes quarantine for different reasons than planned.
    ReasonsChanged,
    /// The executor could not record it.
    NotPrepared,
    /// Its manifest could not be serialized (a programming error).
    Manifest,
}

/// What confirming one planned item produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Confirmed {
    /// Ready for `quarantine_item`.
    Ready(Box<ConfirmedItem>),
    /// Not executed; the item stays where it is (`REVIEW`).
    Skipped {
        /// The planned item.
        item: PlannedItem,
        /// Why.
        reason: Skip,
    },
}

/// Splits `path` into the most specific root it lies under and the components
/// below it.
pub fn split(path: &RawPath, roots: &[RawPath]) -> Option<(RawPath, Vec<Vec<u8>>)> {
    let separator = match path.flavor() {
        PathFlavor::Unix => b'/',
        PathFlavor::Windows => b'\\',
    };
    roots
        .iter()
        .filter(|root| root.flavor() == path.flavor())
        .filter_map(|root| {
            let base = root.as_bytes();
            let base = base.strip_suffix(&[separator]).unwrap_or(base);
            let rest = path
                .as_bytes()
                .strip_prefix(base)?
                .strip_prefix(&[separator])?;
            let components: Vec<Vec<u8>> = rest
                .split(|&b| b == separator)
                .map(<[u8]>::to_vec)
                .collect();
            (!components.iter().any(Vec::is_empty)).then(|| (root.clone(), components))
        })
        .max_by_key(|(root, _)| root.as_bytes().len())
}

/// Everything confirmation needs besides the plan.
#[derive(Clone, Copy)]
pub struct Confirming<'a> {
    /// What the assessment saw (graph, evidence, entries, context).
    pub sources: Sources<'a>,
    /// The scan roots.
    pub roots: &'a [RawPath],
    /// Fresh facts about each item.
    pub verifier: &'a dyn CandidateVerifier,
    /// In-use and ownership probes.
    pub probes: Probes<'a>,
    /// The executor that will act.
    pub executor: &'a dyn QuarantineExecutor,
    /// The judge, if any.
    pub judge: &'a dyn JudgeModel,
    /// Time.
    pub clock: &'a dyn Clock,
    /// Operation IDs.
    pub ids: &'a dyn IdGenerator,
    /// This installation.
    pub device: DeviceId,
    /// Builds the Jev request for an input, when Jev is to be consulted again.
    pub request: &'a dyn Fn(&PolicyInput) -> Option<JevRequest>,
}

impl std::fmt::Debug for Confirming<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Confirming")
            .field("roots", &self.roots)
            .field("device", &self.device)
            .finish_non_exhaustive()
    }
}

/// The rules a decision fired, by ID.
fn rules(decision: &PolicyDecision) -> BTreeSet<&str> {
    decision
        .fired_rules()
        .iter()
        .map(|fired| fired.rule.as_str())
        .collect()
}

/// Confirms `item` of `plan`, planned with decision `planned`, under
/// `confirmation`.
pub async fn confirm_item(
    item: &PlannedItem,
    planned: &PolicyDecision,
    plan: PlanId,
    confirmation: &Confirmation,
    with: &Confirming<'_>,
) -> Confirmed {
    let skip = |reason| Confirmed::Skipped {
        item: item.clone(),
        reason,
    };
    let Subject::File { identity } = &item.subject else {
        return skip(Skip::Changed);
    };
    let Ok(entry) = with.verifier.verify(&item.path, identity) else {
        return skip(Skip::Changed);
    };
    let Some((root, relative)) = split(&item.path, with.roots) else {
        return skip(Skip::OutsideRoots);
    };
    let Ok(assessed) = decide_candidate(
        &entry,
        &with.sources,
        &with.probes,
        with.judge,
        with.clock,
        with.request,
    )
    .await
    else {
        return skip(Skip::NoLongerProposed(Verdict::Keep));
    };
    let decision = assessed.decision;
    if decision.verdict() != Verdict::Quarantine {
        return skip(Skip::NoLongerProposed(decision.verdict()));
    }
    if rules(&decision) != rules(planned) {
        return skip(Skip::ReasonsChanged);
    }
    let Ok(prepared) = with.executor.prepare(&root, &relative) else {
        return skip(Skip::NotPrepared);
    };
    let operation: OperationId = with.ids.operation_id();
    let partial_in_use_accepted = decision
        .fired_rules()
        .iter()
        .any(|fired| PARTIAL_IN_USE.contains(&fired.rule.as_str()));
    let Ok(schema_version) = QuarantineManifest::current_schema() else {
        return skip(Skip::Manifest);
    };
    let manifest = QuarantineManifest {
        schema_version,
        operation,
        plan,
        device: with.device,
        moved_at: with.clock.now(),
        original_path: item.path.clone(),
        entry: entry.clone(),
        unix: with.executor.unix_ownership(&prepared),
        decision,
        confirmed_at: confirmation.confirmed_at,
        jev_request: assessed.trace.map(|trace| trace.request_hash),
    };
    let Ok(manifest) = serde_json::to_vec(&manifest) else {
        return skip(Skip::Manifest);
    };
    Confirmed::Ready(Box::new(ConfirmedItem {
        operation,
        item: prepared,
        path: item.path.clone(),
        identity: identity.clone(),
        manifest,
        partial_in_use_accepted,
    }))
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::num::NonZeroU32;

    use lumen_domain::{
        AccessState, Basis, ByteCount, CleanupMechanism, ConfirmationSurface, EntryKind,
        EntryTimes, Evidence, EvidenceHash, Fact, FileId, FileIdentity, FilesystemEntry, PlanHash,
        Platform, Protection, Provenance, ScanId, SizeFacts, SizeFlags, SnapshotId, SourceName,
        Timestamp, VolumeId,
    };
    use lumen_graph::Graph;
    use lumen_policy::{Context, KnowledgeClass};

    use super::*;
    use crate::gather::Gathered;
    use crate::ports::{
        EnumerateError, ExecutionFailure, InUse, InUseProbe, Located, NoopJudge, OwnershipProbe,
        PreparedItem, Removal, RestoreFailure, RestoredItem,
    };

    fn identity() -> FileIdentity {
        FileIdentity {
            volume: VolumeId::new("v").unwrap_or_else(|_| unreachable!()),
            file: FileId::new(7),
        }
    }

    fn raw(path: &str) -> RawPath {
        RawPath::from_unix_bytes(path.as_bytes().to_vec()).unwrap_or_else(|_| unreachable!())
    }

    fn entry() -> FilesystemEntry {
        FilesystemEntry {
            path: raw("/root/cache/x.bin"),
            kind: EntryKind::File,
            size: SizeFacts {
                identity: identity(),
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

    struct Verifier(bool);

    impl CandidateVerifier for Verifier {
        fn verify(
            &self,
            path: &RawPath,
            expected: &FileIdentity,
        ) -> Result<FilesystemEntry, EnumerateError> {
            if self.0 {
                Ok(entry())
            } else {
                Err(EnumerateError::IdentityChanged {
                    path: path.clone(),
                    expected: expected.clone(),
                })
            }
        }
    }

    struct Free;

    impl InUseProbe for Free {
        fn probe(&self, _path: &RawPath, _expected: &FileIdentity) -> InUse {
            InUse::No
        }
    }

    struct Mine;

    impl OwnershipProbe for Mine {
        fn owned_by_current_user(&self, _path: &RawPath, _expected: &FileIdentity) -> Option<bool> {
            Some(true)
        }
    }

    struct Executor;

    impl QuarantineExecutor for Executor {
        fn prepare(
            &self,
            root: &RawPath,
            relative: &[Vec<u8>],
        ) -> Result<PreparedItem, ExecutionFailure> {
            assert_eq!(root.as_bytes(), b"/root");
            assert_eq!(relative, [b"cache".to_vec(), b"x.bin".to_vec()]);
            Ok(PreparedItem(b"{}".to_vec()))
        }

        fn quarantine(
            &self,
            _item: &PreparedItem,
            _operation: OperationId,
            _manifest: &[u8],
        ) -> Result<(), ExecutionFailure> {
            unreachable!("confirmation moves nothing")
        }

        fn locate(&self, _item: &PreparedItem, _operation: OperationId) -> Located {
            Located::Unknown
        }

        fn restore(
            &self,
            _item: &PreparedItem,
            _operation: OperationId,
        ) -> Result<RestoredItem, RestoreFailure> {
            unreachable!("confirmation restores nothing")
        }

        fn finalize(&self, _item: &PreparedItem, _operation: OperationId) -> Result<(), Removal> {
            unreachable!("confirmation removes nothing")
        }
    }

    struct Epoch;

    impl Clock for Epoch {
        fn now(&self) -> Timestamp {
            Timestamp::UNIX_EPOCH
        }
    }

    struct Ids;

    impl IdGenerator for Ids {
        fn scan_id(&self) -> ScanId {
            "0190f0d2-6c1e-7a3b-8c4d-5e6f70819201"
                .parse()
                .unwrap_or_else(|_| unreachable!())
        }
        fn snapshot_id(&self) -> SnapshotId {
            "0190f0d2-6c1e-7a3b-8c4d-5e6f70819202"
                .parse()
                .unwrap_or_else(|_| unreachable!())
        }
        fn plan_id(&self) -> PlanId {
            "0190f0d2-6c1e-7a3b-8c4d-5e6f70819203"
                .parse()
                .unwrap_or_else(|_| unreachable!())
        }
        fn operation_id(&self) -> OperationId {
            "0190f0d2-6c1e-7a3b-8c4d-5e6f70819204"
                .parse()
                .unwrap_or_else(|_| unreachable!())
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

    fn source() -> SourceName {
        SourceName::new("test").unwrap_or_else(|_| unreachable!())
    }

    /// A regenerable knowledge-base match on the item.
    fn gathered() -> Gathered {
        Gathered {
            evidence: vec![Evidence {
                subject: Subject::File {
                    identity: identity(),
                },
                fact: Fact::KnowledgeBaseMatch {
                    entry: SourceName::new("x-cache").unwrap_or_else(|_| unreachable!()),
                },
                basis: Basis::Observed,
                provenance: Provenance {
                    source: source(),
                    observed_at: Timestamp::UNIX_EPOCH,
                },
            }],
            relationships: Vec::new(),
        }
    }

    /// A macOS machine that can quarantine by rename.
    fn context() -> Context {
        Context {
            platform: Platform::Macos,
            home: Some(raw("/Users/ana")),
            windows_dir: None,
            local_app_data: None,
            mechanisms: BTreeSet::from([CleanupMechanism::QuarantineRename]),
            complete_sources: BTreeSet::new(),
            single_user: false,
        }
    }

    /// A regenerable knowledge-base match on `/root/cache/x.bin`: the policy
    /// proposes quarantine when it is free and observed.
    fn run(present: bool, planned_other: bool) -> Confirmed {
        let gathered = gathered();
        let scanned = [entry()];
        let graph = Graph::new();
        let context = context();
        let knowledge = BTreeMap::from([(
            SourceName::new("x-cache").unwrap_or_else(|_| unreachable!()),
            KnowledgeClass::Regenerable,
        )]);
        let source = source();
        let sources = Sources {
            graph: &graph,
            gathered: &gathered,
            scanned: &scanned,
            listings: &[],
            context: &context,
            knowledge: &knowledge,
            store_volumes: None,
        };
        let probes = Probes {
            in_use: &Free,
            in_use_source: &source,
            ownership: &Mine,
            ownership_source: &source,
        };
        let roots = [raw("/root")];
        let verifier = Verifier(present);
        let with = Confirming {
            sources,
            roots: &roots,
            verifier: &verifier,
            probes,
            executor: &Executor,
            judge: &NoopJudge,
            clock: &Epoch,
            ids: &Ids,
            device: "0190f0d2-6c1e-7a3b-8c4d-5e6f70819205"
                .parse()
                .unwrap_or_else(|_| unreachable!()),
            request: &|_| None,
        };
        let now = ready(decide_candidate(
            &entry(),
            &sources,
            &probes,
            &NoopJudge,
            &Epoch,
            &|_| None,
        ))
        .unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(
            now.decision.verdict(),
            Verdict::Quarantine,
            "{:?}",
            now.reasons
        );
        let item = PlannedItem {
            subject: Subject::File {
                identity: identity(),
            },
            path: entry().path,
            policy_version: now.decision.policy_version(),
            evidence_hash: now.decision.evidence_hash(),
            reclaim: now.input.observed.map(|o| o.reclaim).unwrap_or_default(),
        };
        let confirmation = Confirmation {
            plan_hash: PlanHash::from_bytes([0; 32]),
            confirmed_at: Timestamp::UNIX_EPOCH,
            surface: ConfirmationSurface::CommandLine,
        };
        let planned = if planned_other {
            other_decision()
        } else {
            now.decision.clone()
        };
        ready(confirm_item(
            &item,
            &planned,
            Ids.plan_id(),
            &confirmation,
            &with,
        ))
    }

    /// A `QUARANTINE` decision for another reason than the one made now.
    fn other_decision() -> PolicyDecision {
        PolicyDecision::new(
            Verdict::Quarantine,
            lumen_policy::POLICY_VERSION,
            EvidenceHash::from_bytes([3; 32]),
            [lumen_domain::FiredRule {
                rule: lumen_domain::RuleId::new("classify.app-cache")
                    .unwrap_or_else(|e| unreachable!("{e}")),
                stage: lumen_domain::PolicyStage::Classification,
                evidence: BTreeSet::new(),
            }],
            lumen_domain::Risk {
                level: lumen_domain::RiskLevel::Low,
                factors: BTreeSet::new(),
            },
            lumen_domain::JevEffect::None,
        )
        .unwrap_or_else(|e| unreachable!("{e}"))
    }

    #[test]
    fn an_unchanged_proposed_item_is_ready_with_its_manifest() {
        let Confirmed::Ready(item) = run(true, false) else {
            unreachable!("expected ready");
        };
        assert_eq!(item.operation, Ids.operation_id());
        assert!(!item.partial_in_use_accepted);
        let manifest: QuarantineManifest =
            serde_json::from_slice(&item.manifest).unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(manifest.operation, item.operation);
        assert_eq!(manifest.plan, Ids.plan_id());
        assert_eq!(manifest.original_path, entry().path);
        assert_eq!(manifest.decision.verdict(), Verdict::Quarantine);
    }

    #[test]
    fn changed_items_and_reasons_are_skipped() {
        assert!(matches!(
            run(false, false),
            Confirmed::Skipped {
                reason: Skip::Changed,
                ..
            }
        ));
        assert!(matches!(
            run(true, true),
            Confirmed::Skipped {
                reason: Skip::ReasonsChanged,
                ..
            }
        ));
    }

    #[test]
    fn paths_split_under_the_most_specific_root() {
        let roots = [raw("/root"), raw("/root/cache"), raw("/other")];
        assert_eq!(
            split(&raw("/root/cache/x.bin"), &roots),
            Some((raw("/root/cache"), vec![b"x.bin".to_vec()]))
        );
        assert_eq!(split(&raw("/rootless/x"), &roots), None);
        assert_eq!(
            split(&raw("/root"), &roots),
            None,
            "a root itself is never an item"
        );
    }
}
