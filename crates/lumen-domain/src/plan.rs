//! Cleanup plans and their hash (roadmap 10.10, ADR-0034, system.md §11).
//!
//! A plan is what the user is shown and confirms: its ID, when it was made, and
//! its items in path order, each with the subject, its raw path, and the policy
//! version, evidence hash and reclaim estimate of the decision that put it
//! there. The [`PlanHash`] is BLAKE3 over `lumen.plan/1\0` and the plan's
//! canonical JSON; a confirmation names it, so the plan that executes is
//! exactly the plan that was shown. A golden test pins the bytes.

use serde::{Deserialize, Serialize};

use crate::{
    EvidenceHash, PlanHash, PlanId, PolicyVersion, RawPath, ReclaimEstimate, Subject, Timestamp,
};

const HASH_DOMAIN: &[u8] = b"lumen.plan/1\0";

/// One item of a cleanup plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedItem {
    /// What is acted on.
    pub subject: Subject,
    /// Where it was observed.
    pub path: RawPath,
    /// The policy version that decided it.
    pub policy_version: PolicyVersion,
    /// The evidence the decision saw.
    pub evidence_hash: EvidenceHash,
    /// Space it frees.
    pub reclaim: ReclaimEstimate,
}

/// A cleanup plan: the exact set of items a confirmation authorizes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CleanupPlan {
    id: PlanId,
    created_at: Timestamp,
    items: Vec<PlannedItem>,
}

impl CleanupPlan {
    /// A plan of `items`, in path order (the order given does not matter). A
    /// deserialized plan keeps its stored order, which this constructor made.
    pub fn new(id: PlanId, created_at: Timestamp, mut items: Vec<PlannedItem>) -> Self {
        items.sort_by(|a, b| a.path.cmp(&b.path));
        Self {
            id,
            created_at,
            items,
        }
    }

    /// The plan's ID.
    pub const fn id(&self) -> PlanId {
        self.id
    }

    /// When it was made.
    pub const fn created_at(&self) -> Timestamp {
        self.created_at
    }

    /// Its items, in path order.
    pub fn items(&self) -> &[PlannedItem] {
        &self.items
    }

    /// Canonical bytes: compact JSON with fixed field order.
    ///
    /// # Panics
    ///
    /// Never in practice: these types serialize infallibly.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self)
            .unwrap_or_else(|e| unreachable!("plan serialization is infallible: {e}"))
    }

    /// The hash a confirmation names.
    pub fn hash(&self) -> PlanHash {
        let mut hasher = blake3::Hasher::new();
        hasher.update(HASH_DOMAIN);
        hasher.update(&self.canonical_bytes());
        PlanHash::from_bytes(*hasher.finalize().as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use super::*;
    use crate::{ByteCount, FileId, FileIdentity, SizeFacts, SizeFlags, VolumeId};

    fn item(path: &str, file: u128) -> PlannedItem {
        let identity = FileIdentity {
            volume: VolumeId::new("v").unwrap_or_else(|_| unreachable!()),
            file: FileId::new(file),
        };
        let size = SizeFacts {
            identity: identity.clone(),
            logical: ByteCount::new(4096),
            allocated: ByteCount::new(4096),
            private: None,
            clone_id: None,
            link_count: NonZeroU32::MIN,
            flags: SizeFlags::default(),
        };
        PlannedItem {
            subject: Subject::File { identity },
            path: RawPath::from_unix_bytes(path.as_bytes().to_vec())
                .unwrap_or_else(|_| unreachable!()),
            policy_version: PolicyVersion::new(0, 10, 0),
            evidence_hash: EvidenceHash::from_bytes([7; 32]),
            reclaim: ReclaimEstimate::for_selection([&size]),
        }
    }

    fn plan(items: Vec<PlannedItem>) -> CleanupPlan {
        CleanupPlan::new(
            "0190f0d2-6c1e-7a3b-8c4d-5e6f708192a3"
                .parse()
                .unwrap_or_else(|_| unreachable!()),
            Timestamp::UNIX_EPOCH,
            items,
        )
    }

    #[test]
    fn items_are_in_path_order_whatever_the_input_order() {
        let a = plan(vec![item("/b", 2), item("/a", 1)]);
        let b = plan(vec![item("/a", 1), item("/b", 2)]);
        assert_eq!(a, b);
        assert_eq!(a.hash(), b.hash());
        assert_eq!(a.items()[0].path.as_bytes(), b"/a");
    }

    #[test]
    fn any_change_changes_the_hash() {
        let base = plan(vec![item("/a", 1)]);
        let mut other = item("/a", 1);
        other.evidence_hash = EvidenceHash::from_bytes([8; 32]);
        assert_ne!(base.hash(), plan(vec![other]).hash());
        assert_ne!(base.hash(), plan(vec![item("/a", 1), item("/b", 2)]).hash());
        assert_ne!(base.hash(), plan(Vec::new()).hash());
    }

    #[test]
    fn the_hash_is_pinned() {
        // Golden value: changing the canonical form or the domain is a
        // reviewed change with a version bump (`lumen.plan/2`), never an accident.
        assert_eq!(plan(vec![item("/a", 1)]).hash().to_string(), GOLDEN);
    }

    /// Computed independently: BLAKE3 over `lumen.plan/1\0` and the canonical
    /// JSON printed by this crate, recomputed with Python's `blake3`.
    const GOLDEN: &str = "b3:8c05bf8f00722b4a58174b715d1b11cfc4f69000c54b269df53b85cfa0cff22f";
}
