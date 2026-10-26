//! Lumen's deterministic safety policy (ADR-0014).
//!
//! [`evaluate`] turns the facts about one subject into a [`PolicyDecision`]. Rules
//! run in fixed stages:
//!
//! 1. **Hard protection**: if any rule fires, the verdict is `Keep` and no later
//!    stage runs. Nothing overrides a protection.
//! 2. **Eligibility**: rules that fire cap the verdict (e.g. an item that frees
//!    nothing is at most `Review`).
//! 3. **Classification**: rules propose verdicts; the most cautious proposal
//!    wins. If none fires, an item of at least [`REVIEW_MIN_BYTES`] (or of
//!    unknown size) is `Review` (`default.review`) and a smaller one is `Keep`
//!    (`default.keep-small`, ADR-0039). Only a classification rule can make an
//!    item a quarantine candidate.
//!
//! 4. **Jev** (ADR-0020): an optional, validated judgment can only add caution.
//!    A `Quarantine` verdict becomes `Review` when Jev thinks the item is needed,
//!    is uncertain, or saw text trying to influence it (`jev.demote`). Jev never
//!    touches `Keep` or `Review`, and promotion is not allowed for any class.
//!
//! User confirmation of a specific plan follows outside the policy.
//!
//! The policy is IO-free and depends only on `lumen-domain`. Identical inputs
//! give identical decisions, and the decision records every rule that fired
//! with the evidence it used, so it can be replayed from
//! `(policy_version, evidence_hash)`.

#![forbid(unsafe_code)]

mod classify;
mod explain;
mod locations;
mod rules;

use std::collections::{BTreeMap, BTreeSet};

use lumen_domain::{
    Assessment, ByteCount, CleanupMechanism, DecisionError, Evidence, EvidenceHash, EvidenceId,
    FilesystemEntry, FiredRule, JevEffect, Judgment, Platform, PolicyDecision, PolicyStage,
    PolicyVersion, RawPath, ReclaimEstimate, Risk, RiskFactor, RiskLevel, RuleId, SourceName,
    Subject, Verdict,
};

pub use classify::KnowledgeClass;
pub use explain::explain;

/// The policy version recorded with every decision. A change that can alter
/// any verdict bumps at least the minor version.
pub const POLICY_VERSION: PolicyVersion = PolicyVersion::new(0, 11, 0);

/// The smallest unrecognised item worth a person's review (ADR-0039): 100 MB,
/// in decimal bytes as Lumen shows sizes. Below it, an item no rule
/// recognises is kept.
pub const REVIEW_MIN_BYTES: ByteCount = ByteCount::new(100_000_000);

/// The machine a decision is made on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Context {
    /// The running platform.
    pub platform: Platform,
    /// The user's home directory.
    pub home: Option<RawPath>,
    /// The Windows directory (`%WINDIR%`), on Windows.
    pub windows_dir: Option<RawPath>,
    /// Windows `FOLDERID_LocalAppData`, on Windows.
    pub local_app_data: Option<RawPath>,
    /// Cleanup mechanisms available at runtime (from `PlatformCapabilities`).
    pub mechanisms: BTreeSet<CleanupMechanism>,
    /// Inventory sources that were read completely in this analysis; only
    /// absence they report is trusted.
    pub complete_sources: BTreeSet<SourceName>,
}

/// What the scan saw of a filesystem subject.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observed {
    /// The subject's own entry.
    pub entry: FilesystemEntry,
    /// For a directory, whether it and every directory below it were fully read;
    /// for any other kind, whether its metadata was read.
    pub complete: bool,
    /// Space removing the subject would free.
    pub reclaim: ReclaimEstimate,
}

/// Everything the policy may look at for one subject.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyInput {
    /// The subject being decided.
    pub subject: Subject,
    /// Hash of the evidence bundle the facts come from.
    pub evidence_hash: EvidenceHash,
    /// The evidence in that bundle.
    pub evidence: Vec<Evidence>,
    /// Paths the subject was observed at. They can only add protection; actions
    /// use identities.
    pub paths: Vec<RawPath>,
    /// The machine.
    pub context: Context,
    /// What the scan saw of the subject, if it is a filesystem object that was
    /// observed.
    pub observed: Option<Observed>,
    /// Safety classes of the knowledge-base entries the evidence refers to.
    pub knowledge: BTreeMap<SourceName, KnowledgeClass>,
    /// Jev's judgment, already validated against the request (ADR-0020).
    pub jev: Option<Judgment>,
}

/// Why no decision could be made.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PolicyError {
    /// A rule has an invalid identifier (a programming error the tests catch).
    #[error("rule `{0}` has an invalid identifier")]
    RuleId(String),
    /// The decision broke the domain's contract (a programming error).
    #[error(transparent)]
    Decision(#[from] DecisionError),
}

/// What a rule found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Finding {
    /// For eligibility, the cap; for classification, the proposal; ignored for
    /// hard protections, which always mean `Keep`.
    pub(crate) verdict: Verdict,
    /// The evidence the rule relied on.
    pub(crate) evidence: BTreeSet<EvidenceId>,
    /// Why the item carries risk.
    pub(crate) risk: BTreeSet<RiskFactor>,
    /// A more specific identifier for this firing (e.g. the protected location
    /// that matched), recorded instead of the rule's own.
    pub(crate) rule: Option<String>,
}

/// One rule.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Rule {
    /// Stable identifier, recorded in decisions.
    pub(crate) id: &'static str,
    /// The stage it belongs to.
    pub(crate) stage: PolicyStage,
    /// Fires by returning a finding.
    pub(crate) check: fn(&PolicyInput) -> Option<Finding>,
}

/// Fired when no classification rule recognises the item and it is not known
/// to be small.
const DEFAULT_REVIEW: &str = "default.review";
/// Fired when no classification rule recognises the item and it is smaller
/// than [`REVIEW_MIN_BYTES`].
const DEFAULT_KEEP_SMALL: &str = "default.keep-small";
/// Fired when Jev evidence demotes a quarantine proposal.
const JEV_DEMOTE: &str = "jev.demote";

/// Whether a judgment calls for more caution than a quarantine proposal.
fn jev_cautions(judgment: &Judgment) -> bool {
    judgment.injection_suspected
        || matches!(
            judgment.assessment,
            Assessment::LikelyNeeded | Assessment::Uncertain
        )
}

/// The default for an item no classification rule recognises (ADR-0039): kept
/// when it is known to be smaller than [`REVIEW_MIN_BYTES`] by every measure
/// the scan has (allocated size and space freed now), reviewed otherwise. An
/// item without a scan observation has no known size, so it is reviewed.
fn default_for(input: &PolicyInput) -> (&'static str, Verdict) {
    let small = input.observed.as_ref().is_some_and(|observed| {
        observed
            .reclaim
            .allocated
            .max(observed.reclaim.reclaimable_now)
            < REVIEW_MIN_BYTES
    });
    if small {
        (DEFAULT_KEEP_SMALL, Verdict::Keep)
    } else {
        (DEFAULT_REVIEW, Verdict::Review)
    }
}

/// The more cautious of two verdicts (`Keep` > `Review` > `Quarantine`).
fn cautious(a: Verdict, b: Verdict) -> Verdict {
    a.min(b)
}

/// The risk level implied by the factors.
fn risk(factors: BTreeSet<RiskFactor>) -> Risk {
    use RiskFactor as F;
    let level = factors
        .iter()
        .map(|factor| match factor {
            F::OsProtected | F::SensitiveData => RiskLevel::Critical,
            F::NotRegenerable | F::InUse | F::CloudBacked => RiskLevel::High,
            _ => RiskLevel::Medium,
        })
        .max()
        .unwrap_or(RiskLevel::Low);
    Risk { level, factors }
}

fn fired(
    rule: &Rule,
    specific: Option<String>,
    evidence: BTreeSet<EvidenceId>,
) -> Result<FiredRule, PolicyError> {
    let id = specific.unwrap_or_else(|| rule.id.to_owned());
    Ok(FiredRule {
        rule: RuleId::new(id.clone()).map_err(|_| PolicyError::RuleId(id))?,
        stage: rule.stage,
        evidence,
    })
}

/// Decides `input` with the policy's rules.
///
/// # Errors
///
/// Only on a programming error: an invalid rule identifier or a decision the
/// domain rejects. Callers treat an error as `Keep`.
pub fn evaluate(input: &PolicyInput) -> Result<PolicyDecision, PolicyError> {
    evaluate_with(rules::RULES, input)
}

pub(crate) fn evaluate_with(
    rules: &[Rule],
    input: &PolicyInput,
) -> Result<PolicyDecision, PolicyError> {
    let mut trace = Vec::new();
    let mut factors = BTreeSet::new();
    let mut run = |stage: PolicyStage| -> Result<Vec<Verdict>, PolicyError> {
        let mut verdicts = Vec::new();
        for rule in rules.iter().filter(|r| r.stage == stage) {
            if let Some(finding) = (rule.check)(input) {
                trace.push(fired(rule, finding.rule, finding.evidence)?);
                factors.extend(finding.risk);
                verdicts.push(finding.verdict);
            }
        }
        Ok(verdicts)
    };

    let verdict = if run(PolicyStage::HardProtection)?.is_empty() {
        let cap = run(PolicyStage::Eligibility)?
            .into_iter()
            .fold(Verdict::Quarantine, cautious);
        let proposals = run(PolicyStage::Classification)?;
        let proposal = if let Some(proposal) = proposals.into_iter().reduce(cautious) {
            proposal
        } else {
            let (id, verdict) = default_for(input);
            trace.push(fired(
                &Rule {
                    id,
                    stage: PolicyStage::Classification,
                    check: |_| None,
                },
                None,
                BTreeSet::new(),
            )?);
            verdict
        };
        cautious(proposal, cap)
    } else {
        Verdict::Keep
    };
    let (verdict, jev_effect) = match &input.jev {
        Some(judgment) if verdict == Verdict::Quarantine && jev_cautions(judgment) => {
            trace.push(fired(
                &Rule {
                    id: JEV_DEMOTE,
                    stage: PolicyStage::Jev,
                    check: |_| None,
                },
                None,
                judgment.evidence_refs.clone(),
            )?);
            (Verdict::Review, JevEffect::DemotedToReview)
        }
        _ => (verdict, JevEffect::None),
    };
    Ok(PolicyDecision::new(
        verdict,
        POLICY_VERSION,
        input.evidence_hash,
        trace,
        risk(factors),
        jev_effect,
    )?)
}

/// Inputs for tests: an observed, fully read, unprotected file on Linux, large
/// enough to be reviewed when nothing recognises it.
#[cfg(test)]
pub(crate) mod testing {
    use std::num::NonZeroU32;

    use lumen_domain::{
        AccessState, ByteCount, EntryKind, EntryTimes, FileId, FileIdentity, Protection, SizeFacts,
        SizeFlags, VolumeId,
    };

    use super::*;

    /// A size well above [`REVIEW_MIN_BYTES`].
    pub(crate) const LARGE: u64 = 1_000_000_000;

    pub(crate) fn file(n: u128) -> Subject {
        Subject::File {
            identity: identity(n),
        }
    }

    fn identity(n: u128) -> FileIdentity {
        FileIdentity {
            volume: VolumeId::new("v").unwrap_or_else(|_| unreachable!()),
            file: FileId::new(n),
        }
    }

    pub(crate) fn observed(kind: EntryKind, reclaimable: u64) -> Observed {
        Observed {
            entry: FilesystemEntry {
                path: RawPath::from_unix_bytes(b"/home/t/x".to_vec())
                    .unwrap_or_else(|_| unreachable!()),
                kind,
                size: SizeFacts {
                    identity: identity(1),
                    logical: ByteCount::new(reclaimable),
                    allocated: ByteCount::new(reclaimable),
                    private: None,
                    clone_id: None,
                    link_count: NonZeroU32::MIN,
                    flags: SizeFlags::default(),
                },
                times: EntryTimes::default(),
                protection: Protection::default(),
                access: AccessState::Readable,
            },
            complete: true,
            reclaim: ReclaimEstimate {
                allocated: ByteCount::new(reclaimable),
                reclaimable_now: ByteCount::new(reclaimable),
                files: 1,
                exact: true,
                ..ReclaimEstimate::default()
            },
        }
    }

    pub(crate) fn input(evidence: Vec<Evidence>) -> PolicyInput {
        PolicyInput {
            subject: file(1),
            evidence_hash: EvidenceHash::from_bytes([0; 32]),
            evidence,
            paths: Vec::new(),
            context: Context {
                platform: Platform::Linux,
                home: Some(
                    RawPath::from_unix_bytes(b"/home/t".to_vec())
                        .unwrap_or_else(|_| unreachable!()),
                ),
                windows_dir: None,
                local_app_data: None,
                mechanisms: BTreeSet::from([CleanupMechanism::QuarantineRename]),
                complete_sources: BTreeSet::new(),
            },
            observed: Some(observed(EntryKind::File, LARGE)),
            knowledge: BTreeMap::new(),
            jev: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use lumen_domain::{
        Basis, Fact, FileId, FileIdentity, Provenance, SourceName, Timestamp, VolumeId,
    };
    use proptest::prelude::*;

    use super::*;

    fn subject() -> Subject {
        Subject::File {
            identity: FileIdentity {
                volume: VolumeId::new("v").unwrap_or_else(|_| unreachable!()),
                file: FileId::new(1),
            },
        }
    }

    fn evidence(fact: Fact) -> Evidence {
        Evidence {
            subject: subject(),
            fact,
            basis: Basis::Observed,
            provenance: Provenance {
                source: SourceName::new("test").unwrap_or_else(|_| unreachable!()),
                observed_at: Timestamp::UNIX_EPOCH,
            },
        }
    }

    fn input(evidence: Vec<Evidence>) -> PolicyInput {
        PolicyInput {
            evidence_hash: EvidenceHash::from_bytes([7; 32]),
            ..testing::input(evidence)
        }
    }

    /// Fires with `verdict` when any evidence says the subject is in use.
    fn when_in_use(
        verdict: Verdict,
        risk: &[RiskFactor],
    ) -> impl Fn(&PolicyInput) -> Option<Finding> {
        let risk: BTreeSet<_> = risk.iter().copied().collect();
        move |input| {
            let used: BTreeSet<_> = input
                .evidence
                .iter()
                .filter(|e| e.fact == Fact::InUse(true))
                .map(Evidence::id)
                .collect();
            (!used.is_empty()).then(|| Finding {
                verdict,
                evidence: used,
                risk: risk.clone(),
                rule: None,
            })
        }
    }

    fn protect(input: &PolicyInput) -> Option<Finding> {
        when_in_use(Verdict::Keep, &[RiskFactor::InUse])(input)
    }

    fn cap_review(input: &PolicyInput) -> Option<Finding> {
        when_in_use(Verdict::Review, &[])(input)
    }

    #[allow(clippy::unnecessary_wraps, reason = "must match the rule signature")]
    fn quarantine_always(_: &PolicyInput) -> Option<Finding> {
        Some(Finding {
            verdict: Verdict::Quarantine,
            evidence: BTreeSet::new(),
            risk: BTreeSet::new(),
            rule: None,
        })
    }

    #[allow(clippy::unnecessary_wraps, reason = "must match the rule signature")]
    fn review_always(_: &PolicyInput) -> Option<Finding> {
        Some(Finding {
            verdict: Verdict::Review,
            evidence: BTreeSet::new(),
            risk: BTreeSet::from([RiskFactor::UnknownOwner]),
            rule: None,
        })
    }

    const fn rule(
        id: &'static str,
        stage: PolicyStage,
        check: fn(&PolicyInput) -> Option<Finding>,
    ) -> Rule {
        Rule { id, stage, check }
    }

    fn ids(decision: &PolicyDecision) -> Vec<&str> {
        decision
            .fired_rules()
            .iter()
            .map(|r| r.rule.as_str())
            .collect()
    }

    #[test]
    fn nothing_recognised_is_review() -> Result<(), PolicyError> {
        let decision = evaluate_with(&[], &input(Vec::new()))?;
        assert_eq!(decision.verdict(), Verdict::Review);
        assert_eq!(ids(&decision), [DEFAULT_REVIEW]);
        assert_eq!(decision.risk().level, RiskLevel::Low);
        assert_eq!(decision.policy_version(), POLICY_VERSION);
        assert_eq!(decision.evidence_hash(), EvidenceHash::from_bytes([7; 32]));
        Ok(())
    }

    /// `input` with an observed file whose reclaim estimate is `allocated` and
    /// `now`.
    fn sized(allocated: u64, now: u64) -> PolicyInput {
        let mut sized = input(Vec::new());
        let mut observed = testing::observed(lumen_domain::EntryKind::File, now);
        observed.reclaim.allocated = ByteCount::new(allocated);
        sized.observed = Some(observed);
        sized
    }

    #[test]
    fn only_items_of_review_size_or_unknown_size_default_to_review() -> Result<(), PolicyError> {
        let min = REVIEW_MIN_BYTES.get();
        for (allocated, now, verdict, rule) in [
            (min - 1, min - 1, Verdict::Keep, DEFAULT_KEEP_SMALL),
            (0, 0, Verdict::Keep, DEFAULT_KEEP_SMALL),
            (min, min, Verdict::Review, DEFAULT_REVIEW),
            (min, 0, Verdict::Review, DEFAULT_REVIEW),
            (0, min, Verdict::Review, DEFAULT_REVIEW),
            (u64::MAX, u64::MAX, Verdict::Review, DEFAULT_REVIEW),
        ] {
            let decision = evaluate_with(&[], &sized(allocated, now))?;
            assert_eq!(decision.verdict(), verdict, "{allocated} {now}");
            assert_eq!(ids(&decision), [rule], "{allocated} {now}");
        }
        let unknown = evaluate_with(
            &[],
            &PolicyInput {
                observed: None,
                ..input(Vec::new())
            },
        )?;
        assert_eq!(unknown.verdict(), Verdict::Review, "unknown is never small");
        assert_eq!(ids(&unknown), [DEFAULT_REVIEW]);
        Ok(())
    }

    #[test]
    fn the_size_default_never_overrides_a_classification() -> Result<(), PolicyError> {
        let small = sized(1, 1);
        let rules = [rule(
            "classify.always",
            PolicyStage::Classification,
            quarantine_always,
        )];
        let classified = evaluate_with(&rules, &small)?;
        assert_eq!(classified.verdict(), Verdict::Quarantine);
        assert_eq!(ids(&classified), ["classify.always"]);
        let reviewed = [rule(
            "classify.unsure",
            PolicyStage::Classification,
            review_always,
        )];
        assert_eq!(
            evaluate_with(&reviewed, &small)?.verdict(),
            Verdict::Review,
            "a rule's review stands, however small the item"
        );
        let capped = evaluate_with(
            &[rule("eligible.cap", PolicyStage::Eligibility, cap_review)],
            &PolicyInput {
                evidence: vec![evidence(Fact::InUse(true))],
                ..small
            },
        )?;
        assert_eq!(capped.verdict(), Verdict::Keep, "a cap never raises it");
        Ok(())
    }

    #[test]
    fn a_protection_ends_evaluation_with_keep() -> Result<(), PolicyError> {
        let rules = [
            rule("protect.in-use", PolicyStage::HardProtection, protect),
            rule(
                "classify.always",
                PolicyStage::Classification,
                quarantine_always,
            ),
        ];
        let used = evidence(Fact::InUse(true));
        let decision = evaluate_with(&rules, &input(vec![used.clone()]))?;
        assert_eq!(decision.verdict(), Verdict::Keep);
        assert_eq!(ids(&decision), ["protect.in-use"], "later stages never ran");
        let first = decision
            .fired_rules()
            .first()
            .ok_or_else(|| PolicyError::RuleId("none".into()))?;
        assert_eq!(first.evidence, BTreeSet::from([used.id()]));
        assert_eq!(decision.risk().level, RiskLevel::High);

        let free = evaluate_with(&rules, &input(vec![evidence(Fact::InUse(false))]))?;
        assert_eq!(free.verdict(), Verdict::Quarantine);
        Ok(())
    }

    #[test]
    fn eligibility_caps_and_the_most_cautious_proposal_wins() -> Result<(), PolicyError> {
        let capped = [
            rule("eligible.cap", PolicyStage::Eligibility, cap_review),
            rule(
                "classify.always",
                PolicyStage::Classification,
                quarantine_always,
            ),
        ];
        let used = input(vec![evidence(Fact::InUse(true))]);
        assert_eq!(evaluate_with(&capped, &used)?.verdict(), Verdict::Review);
        assert_eq!(
            evaluate_with(&capped, &input(Vec::new()))?.verdict(),
            Verdict::Quarantine
        );

        let split = [
            rule(
                "classify.always",
                PolicyStage::Classification,
                quarantine_always,
            ),
            rule(
                "classify.unsure",
                PolicyStage::Classification,
                review_always,
            ),
        ];
        let decision = evaluate_with(&split, &input(Vec::new()))?;
        assert_eq!(decision.verdict(), Verdict::Review);
        assert_eq!(ids(&decision), ["classify.always", "classify.unsure"]);
        assert_eq!(decision.risk().level, RiskLevel::Medium);
        Ok(())
    }

    #[test]
    fn an_eligibility_cap_never_makes_an_item_a_candidate() -> Result<(), PolicyError> {
        let rules = [rule("eligible.cap", PolicyStage::Eligibility, cap_review)];
        let decision = evaluate_with(&rules, &input(Vec::new()))?;
        assert_eq!(
            decision.verdict(),
            Verdict::Review,
            "default, not quarantine"
        );
        Ok(())
    }

    fn judgment(assessment: Assessment, injection_suspected: bool) -> Judgment {
        Judgment {
            item_ref: lumen_domain::ItemRef::new("item-1").unwrap_or_else(|_| unreachable!()),
            assessment,
            confidence: lumen_domain::JudgmentConfidence::High,
            reason_codes: BTreeSet::from([lumen_domain::ReasonCode::OwnerAppPresent]),
            evidence_refs: BTreeSet::from([evidence(Fact::InUse(false)).id()]),
            injection_suspected,
            rationale: lumen_domain::UntrustedText::new("display only"),
        }
    }

    #[test]
    fn jev_can_only_demote_quarantine() -> Result<(), PolicyError> {
        let quarantine = [rule(
            "classify.always",
            PolicyStage::Classification,
            quarantine_always,
        )];
        let protect_rules = [rule("protect.in-use", PolicyStage::HardProtection, protect)];
        let with = |jev: Option<Judgment>, facts: Vec<Evidence>| PolicyInput {
            jev,
            ..input(facts)
        };
        for (assessment, injection, demoted) in [
            (Assessment::LikelyNeeded, false, true),
            (Assessment::Uncertain, false, true),
            (Assessment::LikelyRemovable, true, true),
            (Assessment::LikelyRemovable, false, false),
        ] {
            let decision = evaluate_with(
                &quarantine,
                &with(Some(judgment(assessment, injection)), Vec::new()),
            )?;
            if demoted {
                assert_eq!(decision.verdict(), Verdict::Review, "{assessment:?}");
                assert_eq!(decision.jev_effect(), JevEffect::DemotedToReview);
                assert_eq!(ids(&decision), ["classify.always", JEV_DEMOTE]);
                let cited = decision
                    .fired_rules()
                    .iter()
                    .find(|r| r.stage == PolicyStage::Jev)
                    .map(|r| r.evidence.len());
                assert_eq!(cited, Some(1), "cites the judgment's evidence");
            } else {
                assert_eq!(decision.verdict(), Verdict::Quarantine);
                assert_eq!(decision.jev_effect(), JevEffect::None);
            }
        }
        let removable = Some(judgment(Assessment::LikelyRemovable, false));
        let review = evaluate_with(&[], &with(removable.clone(), Vec::new()))?;
        assert_eq!(review.verdict(), Verdict::Review, "never promoted");
        let kept = evaluate_with(
            &protect_rules,
            &with(removable, vec![evidence(Fact::InUse(true))]),
        )?;
        assert_eq!(kept.verdict(), Verdict::Keep);
        assert_eq!(kept.jev_effect(), JevEffect::None);
        Ok(())
    }

    #[test]
    fn invalid_rule_ids_are_errors_not_silently_dropped() {
        let rules = [rule("Bad Id", PolicyStage::HardProtection, protect)];
        assert_eq!(
            evaluate_with(&rules, &input(vec![evidence(Fact::InUse(true))])),
            Err(PolicyError::RuleId("Bad Id".into()))
        );
    }

    #[test]
    fn every_rule_id_is_valid_and_unique() {
        let mut seen = BTreeSet::new();
        for rule in rules::RULES {
            assert!(RuleId::new(rule.id).is_ok(), "{}", rule.id);
            assert!(seen.insert(rule.id), "duplicate {}", rule.id);
        }
        assert!(RuleId::new(DEFAULT_REVIEW).is_ok());
        assert!(RuleId::new(DEFAULT_KEEP_SMALL).is_ok());
    }

    proptest! {
        #[test]
        fn identical_inputs_in_any_order_give_identical_decisions(
            used in proptest::collection::vec(any::<bool>(), 0..6),
            seed in any::<u64>(),
        ) {
            let rules = [
                rule("protect.in-use", PolicyStage::HardProtection, protect),
                rule("eligible.cap", PolicyStage::Eligibility, cap_review),
                rule("classify.always", PolicyStage::Classification, quarantine_always),
            ];
            let facts: Vec<Evidence> = used.iter().map(|u| evidence(Fact::InUse(*u))).collect();
            let mut shuffled = facts.clone();
            let len = shuffled.len();
            if len > 1 {
                let offset = usize::try_from(seed % len as u64).unwrap_or(0);
                shuffled.rotate_left(offset);
                shuffled.reverse();
            }
            let a = evaluate_with(&rules, &input(facts));
            let b = evaluate_with(&rules, &input(shuffled));
            prop_assert_eq!(a, b);
        }
    }
}
