//! Moving a copy of identical files the person chose (ADR-0065, as amended by
//! ADR-0069).
//!
//! Lumen never picks a copy: the person does, and at least one copy of every
//! group stays. The cleanup policy is not asked; [`gate`] is this action's own
//! deterministic gate, run on fresh, read-only evidence ([`CopyEvidence`])
//! when the preview is made and again right before the move, after both
//! copies were read again and compared (the owner's choice, 2026-11-18: the
//! check happens in the app; the executor then checks identity, size and
//! time as for every move). Every check that fails, or cannot be answered,
//! refuses with its reason ([`Ineligible`]); unknown is never eligible.
//!
//! A preview is stored as a one-item plan chosen by the person, confirmed by
//! its hash (ADR-0034), and held in Quarantine like any item. Its decision
//! ([`decision`]) cites [`RULE`], which the policy never fires, so a copy's
//! plan, a cleanup plan and an uninstall preview refuse each other
//! ([`is_copy`], [`any_copy`]). The evidence hash binds both copies and the
//! contents the person saw ([`Binding`]). Pure and IO-free.

use std::collections::BTreeSet;

use lumen_domain::{
    EntryKind, EvidenceHash, FileIdentity, FiredRule, JevEffect, PolicyDecision, PolicyStage,
    PolicyVersion, RawPath, Risk, RiskLevel, RuleId, Verdict,
};

/// The rule a copy's decision cites: the gate passed and the person chose
/// this copy. The policy never fires it.
pub const RULE: &str = "copies.person-chosen";

/// The gate's version, recorded as the decision's policy version.
pub const GATE_VERSION: PolicyVersion = PolicyVersion::new(1, 0, 0);

const HASH_DOMAIN: &[u8] = b"lumen.copy/1\0";

/// Why a chosen copy may not move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Ineligible {
    /// The chosen copy and the copy that stays are the same file.
    SameFile,
    /// One of them is not a regular file.
    NotAFile,
    /// One of them has other names (hard links): moving it frees nothing,
    /// and the other names would still see the contents.
    HardLinked,
    /// The chosen copy is in a protected location.
    Protected,
    /// The copy that stays is inside something else that moves.
    NothingStays,
    /// Their contents differ now, or could not be read and compared.
    ContentsDiffer,
}

/// One file as read now: where, what, and how many names it has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileFacts {
    /// Where it is, at its real path.
    pub path: RawPath,
    /// Which file it is.
    pub identity: FileIdentity,
    /// What it is.
    pub kind: EntryKind,
    /// Its link count; 1 for a file with one name.
    pub links: u64,
    /// Its logical size in bytes.
    pub size: u64,
}

/// The fresh facts a copy's move is decided on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyEvidence {
    /// The copy the person chose to move.
    pub chosen: FileFacts,
    /// A copy of the same group that stays where it is.
    pub stays: FileFacts,
    /// Whether the chosen copy is in a protected location
    /// (`lumen_policy`'s protected locations).
    pub protected: bool,
    /// Neither the copy that stays nor any folder above it is in what the
    /// person confirmed (items, apps, copies).
    pub stays_outside_selection: bool,
    /// Whether both were just read in full and hold the same contents;
    /// `None` when either could not be read.
    pub same_contents: Option<bool>,
}

/// Every reason a copy may not move; never empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal(Vec<Ineligible>);

impl Refusal {
    /// The reasons, sorted and without repeats.
    pub fn reasons(&self) -> &[Ineligible] {
        &self.0
    }
}

/// The gate: `Ok` only when every check passes on `evidence`.
///
/// # Errors
///
/// A [`Refusal`] with every reason that applies.
pub fn gate(evidence: &CopyEvidence) -> Result<(), Refusal> {
    let mut reasons = BTreeSet::new();
    let (chosen, stays) = (&evidence.chosen, &evidence.stays);
    if chosen.identity == stays.identity || chosen.path == stays.path {
        reasons.insert(Ineligible::SameFile);
    }
    if chosen.kind != EntryKind::File || stays.kind != EntryKind::File {
        reasons.insert(Ineligible::NotAFile);
    }
    if chosen.links != 1 || stays.links != 1 {
        reasons.insert(Ineligible::HardLinked);
    }
    if evidence.protected {
        reasons.insert(Ineligible::Protected);
    }
    if !evidence.stays_outside_selection {
        reasons.insert(Ineligible::NothingStays);
    }
    if evidence.same_contents != Some(true) || chosen.size != stays.size {
        reasons.insert(Ineligible::ContentsDiffer);
    }
    if reasons.is_empty() {
        Ok(())
    } else {
        Err(Refusal(reasons.into_iter().collect()))
    }
}

/// What the person confirms about a copy: both files and the contents they
/// share. Any change between preview and move changes [`Binding::hash`], and
/// the move is refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding<'a> {
    /// The copy that moves.
    pub chosen: &'a FileFacts,
    /// The copy that stays.
    pub stays: &'a FileFacts,
    /// The contents' digest from the comparison; it never leaves the host.
    pub digest: &'a [u8; 32],
}

impl Binding<'_> {
    /// BLAKE3 over `lumen.copy/1\0` and the canonical JSON of the facts.
    ///
    /// # Panics
    ///
    /// Never in practice: these types serialize infallibly.
    pub fn hash(&self) -> EvidenceHash {
        let file = |facts: &FileFacts| {
            serde_json::json!({
                "path": facts.path,
                "identity": facts.identity,
                "size": facts.size,
            })
        };
        let value = serde_json::json!({
            "chosen": file(self.chosen),
            "stays": file(self.stays),
            "digest": blake3::Hash::from_bytes(*self.digest).to_hex().as_str(),
        });
        let bytes = serde_json::to_vec(&value)
            .unwrap_or_else(|e| unreachable!("binding serialization is infallible: {e}"));
        let mut hasher = blake3::Hasher::new();
        hasher.update(HASH_DOMAIN);
        hasher.update(&bytes);
        EvidenceHash::from_bytes(*hasher.finalize().as_bytes())
    }
}

/// The decision a copy's plan stores and its manifest records: `REVIEW` (the
/// person chose it, ADR-0051), rule [`RULE`], over `evidence`.
///
/// # Panics
///
/// Never: the rule ID is valid and the decision cites one non-protection
/// rule.
pub fn decision(evidence: EvidenceHash) -> PolicyDecision {
    let rule = RuleId::new(RULE).unwrap_or_else(|_| unreachable!("valid rule ID"));
    PolicyDecision::new(
        Verdict::Review,
        GATE_VERSION,
        evidence,
        [FiredRule {
            rule,
            stage: PolicyStage::Eligibility,
            evidence: BTreeSet::new(),
        }],
        Risk {
            level: RiskLevel::Medium,
            factors: BTreeSet::new(),
        },
        JevEffect::None,
    )
    .unwrap_or_else(|e| unreachable!("a valid decision: {e}"))
}

/// Whether `decisions` are a copy's plan: exactly one, citing [`RULE`].
pub fn is_copy(decisions: &[PolicyDecision]) -> bool {
    matches!(decisions, [only] if cites_rule(only))
}

/// Whether any of `decisions` cites [`RULE`]: such a plan is neither a
/// cleanup plan nor an uninstall preview, whatever else it holds.
pub fn any_copy(decisions: &[PolicyDecision]) -> bool {
    decisions.iter().any(cites_rule)
}

fn cites_rule(decision: &PolicyDecision) -> bool {
    decision
        .fired_rules()
        .iter()
        .any(|fired| fired.rule.as_str() == RULE)
}

#[cfg(test)]
mod tests {
    use lumen_domain::{FileId, VolumeId};

    use super::*;

    fn facts(path: &str, file: u128) -> FileFacts {
        FileFacts {
            path: RawPath::from_unix_bytes(path.as_bytes().to_vec())
                .unwrap_or_else(|e| unreachable!("{e}")),
            identity: FileIdentity {
                volume: VolumeId::new("disk1").unwrap_or_else(|e| unreachable!("{e}")),
                file: FileId::new(file),
            },
            kind: EntryKind::File,
            links: 1,
            size: 1800,
        }
    }

    fn evidence() -> CopyEvidence {
        CopyEvidence {
            chosen: facts("/Users/ana/Downloads/tutorial.mov", 7),
            stays: facts("/Users/ana/Movies/tutorial.mov", 8),
            protected: false,
            stays_outside_selection: true,
            same_contents: Some(true),
        }
    }

    fn refused(evidence: &CopyEvidence) -> Vec<Ineligible> {
        gate(evidence)
            .err()
            .map(|r| r.reasons().to_vec())
            .unwrap_or_default()
    }

    #[test]
    fn a_copy_with_one_that_stays_and_the_same_contents_may_move() {
        assert_eq!(gate(&evidence()), Ok(()));
    }

    #[test]
    fn unknown_or_different_contents_refuse() {
        for same in [None, Some(false)] {
            let mut e = evidence();
            e.same_contents = same;
            assert_eq!(refused(&e), [Ineligible::ContentsDiffer], "{same:?}");
        }
        let mut grown = evidence();
        grown.stays.size += 1;
        assert_eq!(refused(&grown), [Ineligible::ContentsDiffer]);
    }

    #[test]
    fn every_reason_is_given() {
        let mut e = evidence();
        e.stays = e.chosen.clone();
        e.chosen.kind = EntryKind::Directory;
        e.stays.links = 2;
        e.protected = true;
        e.stays_outside_selection = false;
        assert_eq!(
            refused(&e),
            [
                Ineligible::SameFile,
                Ineligible::NotAFile,
                Ineligible::HardLinked,
                Ineligible::Protected,
                Ineligible::NothingStays,
            ]
        );
    }

    #[test]
    fn the_binding_changes_with_either_copy_and_the_contents() {
        let e = evidence();
        let digest = [3; 32];
        let base = Binding {
            chosen: &e.chosen,
            stays: &e.stays,
            digest: &digest,
        }
        .hash();
        let mut moved = e.stays.clone();
        moved.path = RawPath::from_unix_bytes(b"/Users/ana/Desktop/tutorial.mov".to_vec())
            .unwrap_or_else(|err| unreachable!("{err}"));
        let other = [4; 32];
        for changed in [
            Binding {
                chosen: &e.chosen,
                stays: &moved,
                digest: &digest,
            },
            Binding {
                chosen: &e.chosen,
                stays: &e.stays,
                digest: &other,
            },
        ] {
            assert_ne!(changed.hash(), base);
        }
    }

    #[test]
    fn a_copys_plan_is_told_apart_by_its_rule() {
        let copy = decision(EvidenceHash::from_bytes([1; 32]));
        assert!(is_copy(std::slice::from_ref(&copy)));
        assert!(any_copy(&[copy.clone(), copy.clone()]));
        assert!(!is_copy(&[copy.clone(), copy]));
        let uninstall = crate::uninstall::decision(EvidenceHash::from_bytes([1; 32]));
        assert!(!any_copy(&[uninstall]));
        assert_eq!(
            decision(EvidenceHash::from_bytes([1; 32])).verdict(),
            Verdict::Review
        );
    }
}
