//! Reassessing chosen items with fresh read-only evidence (ADR-0058).
//!
//! Some verdicts rest on what Lumen did not know when it assessed: whether a
//! process used the item, a part it could not read, an old observation, an
//! owner it could not name. [`gaps`] finds those by the fixed table of
//! ADR-0058; every other fired rule is a decision, and an item whose verdict
//! rests only on decisions is [`Outcome::Settled`]. A reassessed item is
//! decided again by the unchanged policy ([`decide_candidate`]) from fresh
//! facts, without AI review, and only if its path still names the scanned
//! object. Nothing here changes the filesystem.

use std::collections::BTreeSet;

use lumen_domain::{
    Evidence, EvidenceId, FilesystemEntry, JevEffect, ReviewReason, RuleId, Verdict,
};
use lumen_policy::{PolicyError, ids};

use crate::assess::{Assessed, Probes, Sources, decide_candidate};
use crate::cancel::CancelToken;
use crate::ports::{Clock, NoopJudge};

/// The most items one reassessment covers.
pub const MAX_ITEMS: usize = 500;

/// Something an earlier assessment did not know, which a read-only check may
/// now answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Gap {
    /// Whether a process uses it: asked live again.
    InUse,
    /// Part or all of it was not read: its subtree is listed again.
    Unread,
    /// An observation is too old: it is observed again.
    Stale,
    /// Who owns it, and whether that application is installed: the
    /// inventories are read again.
    Owner,
}

/// The gaps behind `assessed`'s verdict. Empty when the verdict rests on
/// decisions alone, and when AI review changed it: re-deciding without the
/// judge must never drop its caution.
pub fn gaps(assessed: &Assessed) -> BTreeSet<Gap> {
    if assessed.decision.jev_effect() != JevEffect::None {
        return BTreeSet::new();
    }
    let stale = crate::review::reasons(assessed).contains(&ReviewReason::StaleEvidence);
    gaps_for(
        assessed
            .decision
            .fired_rules()
            .iter()
            .map(|fired| fired.rule.as_str()),
        stale,
    )
}

/// The fixed table of ADR-0058: fired rules (and a stale observation) to gaps.
fn gaps_for<'a>(rules: impl IntoIterator<Item = &'a str>, stale: bool) -> BTreeSet<Gap> {
    let mut gaps = BTreeSet::new();
    for rule in rules {
        let gap = match rule {
            ids::PROTECT_IN_USE | ids::PROTECT_IN_USE_PARTIAL | ids::PROTECT_IN_USE_UNKNOWN => {
                Gap::InUse
            }
            ids::PROTECT_INCOMPLETE_SCAN | ids::PROTECT_NOT_OBSERVED => Gap::Unread,
            ids::DEFAULT_REVIEW | ids::CLASSIFY_ORPHAN => Gap::Owner,
            ids::JEV_DEMOTE => return BTreeSet::new(),
            _ => continue,
        };
        gaps.insert(gap);
    }
    if stale {
        gaps.insert(Gap::Stale);
    }
    gaps
}

/// A verdict and the rules behind it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdicted {
    /// The verdict.
    pub verdict: Verdict,
    /// The rules that fired, sorted.
    pub rules: Vec<RuleId>,
}

impl Verdicted {
    fn of(assessed: &Assessed) -> Self {
        Self {
            verdict: assessed.decision.verdict(),
            rules: assessed
                .decision
                .fired_rules()
                .iter()
                .map(|fired| fired.rule.clone())
                .collect(),
        }
    }
}

/// How reassessing one item ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Its verdict rests on decisions only; nothing was checked.
    Settled,
    /// Its path no longer names the scanned object: scan it again. No verdict.
    Changed,
    /// Decided again from fresh evidence.
    Decided {
        /// The new assessment.
        after: Box<Assessed>,
        /// Evidence the new decision used that the earlier one did not.
        added: Vec<Evidence>,
    },
}

/// One item reassessed: what it was, what was unknown, and how it ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reassessed {
    /// The earlier verdict and its rules.
    pub before: Verdicted,
    /// What was unknown.
    pub gaps: BTreeSet<Gap>,
    /// How it ended.
    pub outcome: Outcome,
}

impl Reassessed {
    /// The verdict now: the new one if it was decided again, else the earlier.
    pub fn verdict(&self) -> Verdict {
        match &self.outcome {
            Outcome::Decided { after, .. } => after.decision.verdict(),
            Outcome::Settled | Outcome::Changed => self.before.verdict,
        }
    }
}

/// Reassesses `before`. `current` is the item as observed again now, `None`
/// if it is gone; `sources` describe the scan with its subtree listed again
/// and the inventories read again; `probes` answer live.
///
/// # Errors
///
/// Only the policy's own programming errors.
pub async fn reassess(
    before: &Assessed,
    current: Option<&FilesystemEntry>,
    sources: &Sources<'_>,
    probes: &Probes<'_>,
    clock: &dyn Clock,
) -> Result<Reassessed, PolicyError> {
    let gaps = gaps(before);
    let summary = Verdicted::of(before);
    if gaps.is_empty() {
        return Ok(Reassessed {
            before: summary,
            gaps,
            outcome: Outcome::Settled,
        });
    }
    let Some(current) = current.filter(|now| now.identity() == before.candidate.identity()) else {
        return Ok(Reassessed {
            before: summary,
            gaps,
            outcome: Outcome::Changed,
        });
    };
    let after = decide_candidate(current, sources, probes, &NoopJudge, clock, &|_| None).await?;
    let known: BTreeSet<EvidenceId> = before.input.evidence.iter().map(Evidence::id).collect();
    let added = after
        .input
        .evidence
        .iter()
        .filter(|record| !known.contains(&record.id()))
        .cloned()
        .collect();
    Ok(Reassessed {
        before: summary,
        gaps,
        outcome: Outcome::Decided {
            after: Box::new(after),
            added,
        },
    })
}

/// One item to reassess, with what it is decided from.
#[derive(Debug, Clone, Copy)]
pub struct Job<'a> {
    /// The earlier assessment.
    pub before: &'a Assessed,
    /// The item as observed again now, `None` if it is gone.
    pub current: Option<&'a FilesystemEntry>,
    /// The scan, with this item's subtree listed again.
    pub sources: Sources<'a>,
}

/// Why a reassessment did not start.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ReassessError {
    /// More items than one reassessment covers.
    #[error("{count} items chosen; reassess at most {MAX_ITEMS} at a time")]
    TooMany {
        /// The number chosen.
        count: usize,
    },
    /// The policy failed (a programming error).
    #[error(transparent)]
    Policy(#[from] PolicyError),
}

/// Reassessed items, in order, and whether the run stopped early.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reassessment {
    /// Every item reassessed before the run ended.
    pub items: Vec<Reassessed>,
    /// The run was cancelled; items after the last one listed were not
    /// reassessed.
    pub cancelled: bool,
}

/// Reassesses `jobs` in order, checking `cancel` before each item.
///
/// # Errors
///
/// [`ReassessError::TooMany`] above [`MAX_ITEMS`], before anything is
/// checked; the policy's own programming errors.
pub async fn reassess_all(
    jobs: &[Job<'_>],
    probes: &Probes<'_>,
    clock: &dyn Clock,
    cancel: &CancelToken,
) -> Result<Reassessment, ReassessError> {
    if jobs.len() > MAX_ITEMS {
        return Err(ReassessError::TooMany { count: jobs.len() });
    }
    let mut items = Vec::with_capacity(jobs.len());
    for job in jobs {
        if cancel.is_cancelled() {
            return Ok(Reassessment {
                items,
                cancelled: true,
            });
        }
        items.push(reassess(job.before, job.current, &job.sources, probes, clock).await?);
    }
    Ok(Reassessment {
        items,
        cancelled: false,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::num::NonZeroU32;

    use lumen_domain::{
        AccessState, AppDataKind, AppId, AppIdScheme, Basis, ByteCount, CleanupMechanism,
        EntryKind, EntryTimes, Fact, FileId, FileIdentity, Platform, Protection, Provenance,
        RawPath, RelationKind, Relationship, SizeFacts, SizeFlags, SourceName, Subject, Timestamp,
        UntrustedText, VolumeId,
    };
    use lumen_graph::Graph;
    use lumen_policy::{Context, evaluate};

    use super::*;
    use crate::gather::{EvidenceIndex, Gathered};
    use crate::ports::{DirOutcome, InUse, InUseProbe, OwnershipProbe};

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
            value: UntrustedText::new("com.example.present"),
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

    /// An installed application's cache folder, with one file inside.
    struct World {
        scanned: Vec<FilesystemEntry>,
        gathered: Gathered,
        index: EvidenceIndex,
        graph: Graph,
        context: Context,
        knowledge: BTreeMap<SourceName, lumen_policy::KnowledgeClass>,
        listings: Vec<(RawPath, DirOutcome)>,
    }

    fn world() -> World {
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
                    Subject::Application { id: app() },
                    Fact::ApplicationPresent {
                        app: app(),
                        present: true,
                    },
                ),
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
        World {
            scanned: vec![
                entry(
                    "/Users/t/Library/Caches/com.example.present",
                    EntryKind::Directory,
                    1,
                ),
                entry(
                    "/Users/t/Library/Caches/com.example.present/x",
                    EntryKind::File,
                    2,
                ),
            ],
            index: EvidenceIndex::new(&gathered),
            gathered,
            graph,
            context: Context {
                platform: Platform::Macos,
                home: RawPath::from_unix_bytes(b"/Users/t".to_vec()).ok(),
                windows_dir: None,
                local_app_data: None,
                mechanisms: BTreeSet::from([CleanupMechanism::QuarantineRename]),
                complete_sources: BTreeSet::new(),
                single_user: false,
            },
            knowledge: BTreeMap::new(),
            listings: vec![(
                RawPath::from_unix_bytes(b"/Users/t/Library/Caches/com.example.present".to_vec())
                    .unwrap_or_else(|_| unreachable!()),
                DirOutcome {
                    access: AccessState::Readable,
                    complete: true,
                },
            )],
        }
    }

    impl World {
        fn sources(&self) -> Sources<'_> {
            Sources {
                graph: &self.graph,
                gathered: &self.gathered,
                index: &self.index,
                scanned: &self.scanned,
                listings: &self.listings,
                context: &self.context,
                knowledge: &self.knowledge,
                store_volumes: None,
            }
        }
    }

    struct Probe(InUse);

    impl InUseProbe for Probe {
        fn probe(&self, _: &RawPath, _: &FileIdentity) -> InUse {
            self.0.clone()
        }
    }

    struct Owner;

    impl OwnershipProbe for Owner {
        fn owned_by_current_user(&self, _: &RawPath, _: &FileIdentity) -> Option<bool> {
            Some(true)
        }
    }

    struct Epoch(i64);

    impl Clock for Epoch {
        fn now(&self) -> Timestamp {
            Timestamp::from_unix(self.0, 0).unwrap_or_else(|_| unreachable!())
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

    /// The cache folder assessed when the probe answered `answer`.
    fn assessed(world: &World, answer: InUse, at: i64) -> Assessed {
        let source = SourceName::new("test").unwrap_or_else(|_| unreachable!());
        let probe = Probe(answer);
        let probes = Probes {
            in_use: &probe,
            in_use_source: &source,
            ownership: &Owner,
            ownership_source: &source,
        };
        ready(decide_candidate(
            &world.scanned[0],
            &world.sources(),
            &probes,
            &NoopJudge,
            &Epoch(at),
            &|_| None,
        ))
        .unwrap_or_else(|e| unreachable!("{e}"))
    }

    fn again(world: &World, before: &Assessed, answer: InUse) -> Reassessed {
        let source = SourceName::new("test").unwrap_or_else(|_| unreachable!());
        let probe = Probe(answer);
        let probes = Probes {
            in_use: &probe,
            in_use_source: &source,
            ownership: &Owner,
            ownership_source: &source,
        };
        ready(reassess(
            before,
            Some(&world.scanned[0]),
            &world.sources(),
            &probes,
            &Epoch(100),
        ))
        .unwrap_or_else(|e| unreachable!("{e}"))
    }

    #[test]
    fn an_item_kept_while_in_use_is_decided_again_once_it_is_closed() {
        let world = world();
        let before = assessed(&world, InUse::Yes(Vec::new()), 1);
        assert_eq!(before.decision.verdict(), Verdict::Keep);
        let result = again(&world, &before, InUse::No);
        assert_eq!(result.gaps, BTreeSet::from([Gap::InUse]));
        assert_eq!(result.before.verdict, Verdict::Keep);
        assert_eq!(result.verdict(), Verdict::Quarantine);
        let Outcome::Decided { after, added } = &result.outcome else {
            unreachable!("{result:?}");
        };
        assert!(
            added.iter().any(|e| e.fact == Fact::InUse(false)),
            "{added:?}"
        );
        // Replaying the recorded evidence gives the recorded verdict.
        assert_eq!(evaluate(&after.input).ok().as_ref(), Some(&after.decision));
    }

    #[test]
    fn a_check_that_cannot_answer_adds_no_negative() {
        let world = world();
        let before = assessed(&world, InUse::Yes(Vec::new()), 1);
        let result = again(&world, &before, InUse::Unknown { reason: "denied" });
        assert_eq!(result.verdict(), Verdict::Keep);
        let Outcome::Decided { added, .. } = &result.outcome else {
            unreachable!("{result:?}");
        };
        assert!(
            !added.iter().any(|e| matches!(e.fact, Fact::InUse(_))),
            "unknown is never false: {added:?}"
        );
    }

    #[test]
    fn a_changed_or_missing_item_gets_no_verdict() {
        let world = world();
        let before = assessed(&world, InUse::Yes(Vec::new()), 1);
        let source = SourceName::new("test").unwrap_or_else(|_| unreachable!());
        let probe = Probe(InUse::No);
        let probes = Probes {
            in_use: &probe,
            in_use_source: &source,
            ownership: &Owner,
            ownership_source: &source,
        };
        let replaced = entry(
            "/Users/t/Library/Caches/com.example.present",
            EntryKind::Directory,
            9,
        );
        for current in [Some(&replaced), None] {
            let result = ready(reassess(
                &before,
                current,
                &world.sources(),
                &probes,
                &Epoch(100),
            ))
            .unwrap_or_else(|e| unreachable!("{e}"));
            assert_eq!(result.outcome, Outcome::Changed);
            assert_eq!(result.verdict(), Verdict::Keep);
        }
    }

    #[test]
    fn decisions_are_settled_and_protection_still_wins() {
        let open = world();
        let proposed = assessed(&open, InUse::No, 1);
        assert_eq!(
            proposed.decision.verdict(),
            Verdict::Quarantine,
            "{:?}",
            proposed.reasons
        );
        assert_eq!(again(&open, &proposed, InUse::No).outcome, Outcome::Settled);

        // As `gather` records it for an entry the system restricts.
        let mut protected = world();
        protected.gathered.evidence.push(fact(
            file(1),
            Fact::Protected(Protection {
                system_restricted: true,
                ..Protection::default()
            }),
        ));
        protected.index = EvidenceIndex::new(&protected.gathered);
        let before = assessed(&protected, InUse::Yes(Vec::new()), 1);
        assert_eq!(before.decision.verdict(), Verdict::Keep);
        let result = again(&protected, &before, InUse::No);
        assert!(result.gaps.contains(&Gap::InUse));
        assert_eq!(result.verdict(), Verdict::Keep, "the system protects it");
    }

    #[test]
    fn the_gap_table_skips_decisions_and_anything_ai_review_changed() {
        assert_eq!(
            gaps_for(
                [
                    ids::PROTECT_IN_USE_PARTIAL,
                    ids::PROTECT_INCOMPLETE_SCAN,
                    ids::CLASSIFY_ORPHAN
                ],
                true
            ),
            BTreeSet::from([Gap::InUse, Gap::Unread, Gap::Owner, Gap::Stale])
        );
        assert_eq!(
            gaps_for(["protect.location", "classify.app-state"], false),
            BTreeSet::new()
        );
        assert_eq!(
            gaps_for([ids::DEFAULT_REVIEW, ids::JEV_DEMOTE], false),
            BTreeSet::new()
        );
    }

    #[test]
    fn a_run_is_bounded_and_stops_when_cancelled() {
        let world = world();
        let before = assessed(&world, InUse::Yes(Vec::new()), 1);
        let source = SourceName::new("test").unwrap_or_else(|_| unreachable!());
        let probe = Probe(InUse::No);
        let probes = Probes {
            in_use: &probe,
            in_use_source: &source,
            ownership: &Owner,
            ownership_source: &source,
        };
        let job = Job {
            before: &before,
            current: Some(&world.scanned[0]),
            sources: world.sources(),
        };
        let too_many = vec![job; MAX_ITEMS + 1];
        assert_eq!(
            ready(reassess_all(
                &too_many,
                &probes,
                &Epoch(100),
                &CancelToken::new()
            )),
            Err(ReassessError::TooMany {
                count: MAX_ITEMS + 1
            })
        );
        let cancel = CancelToken::new();
        cancel.cancel();
        let stopped = ready(reassess_all(&[job, job], &probes, &Epoch(100), &cancel))
            .unwrap_or_else(|e| unreachable!("{e}"));
        assert!(stopped.cancelled && stopped.items.is_empty());
        let done = ready(reassess_all(
            &[job, job],
            &probes,
            &Epoch(100),
            &CancelToken::new(),
        ))
        .unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(done.items.len(), 2);
        assert!(!done.cancelled);
    }
}
