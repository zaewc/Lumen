//! Evidence bundles and their Merkle hash (ADR-0013).
//!
//! An [`EvidenceBundle`] is everything a decision about one subject may rely on:
//! the relationships within a few hops of it (both directions) and the evidence
//! they cite, plus evidence about the subject itself. Its [`EvidenceHash`] is
//! recorded with every policy decision and Jev call, so a decision can be replayed
//! and audited from exactly the evidence it saw.
//!
//! Hash construction (version 1). Every hash is BLAKE3 with a NUL-terminated
//! domain prefix:
//!
//! 1. Leaves: `H("lumen.bundle.leaf/1\0" ‖ 0x01 ‖ evidence_id)` for each evidence
//!    ID, and `H("lumen.bundle.leaf/1\0" ‖ 0x02 ‖ canonical_json(relationship))`
//!    for each relationship; all leaves sorted by their bytes.
//! 2. Tree: each level pairs neighbours, `H("lumen.bundle.node/1\0" ‖ left ‖
//!    right)`; an odd last node is carried up unchanged. The empty bundle's root
//!    is `H("lumen.bundle.empty/1\0")`.
//! 3. Result: `H("lumen.bundle/1\0" ‖ canonical_json(subject) ‖ root)`, binding
//!    the root to the subject it describes.
//!
//! Canonical JSON is the domain's serde form: fixed field order, sorted sets,
//! no floats. A golden test pins the bytes; changing any of this is a reviewed
//! change with a version bump, never an accident.

use std::collections::BTreeSet;

use lumen_domain::{EvidenceHash, EvidenceId, Relationship, Subject};

use crate::{Direction, Graph};

const LEAF: &[u8] = b"lumen.bundle.leaf/1\0";
const NODE: &[u8] = b"lumen.bundle.node/1\0";
const EMPTY: &[u8] = b"lumen.bundle.empty/1\0";
const ROOT: &[u8] = b"lumen.bundle/1\0";
const EVIDENCE_TAG: u8 = 0x01;
const RELATIONSHIP_TAG: u8 = 0x02;

/// The evidence a decision about one subject may rely on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceBundle {
    subject: Subject,
    relationships: BTreeSet<Relationship>,
    evidence: BTreeSet<EvidenceId>,
}

/// Canonical JSON. The domain types serialize infallibly (no maps with
/// non-string keys, no failing `Serialize` impls), so an error cannot occur; an
/// empty result would still hash deterministically.
fn canonical<T: serde::Serialize>(value: &T) -> Vec<u8> {
    serde_json::to_vec(value).unwrap_or_default()
}

fn hash(parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    for part in parts {
        hasher.update(part);
    }
    *hasher.finalize().as_bytes()
}

/// The Merkle root of already sorted leaf hashes.
fn merkle_root(mut level: Vec<[u8; 32]>) -> [u8; 32] {
    if level.is_empty() {
        return hash(&[EMPTY]);
    }
    while level.len() > 1 {
        level = level
            .chunks(2)
            .filter_map(|pair| match pair {
                [left, right] => Some(hash(&[NODE, left, right])),
                [odd] => Some(*odd),
                _ => None,
            })
            .collect();
    }
    level.first().copied().unwrap_or_else(|| hash(&[EMPTY]))
}

impl EvidenceBundle {
    /// A bundle from explicit parts. The evidence cited by `relationships` is
    /// always included.
    pub fn new(
        subject: Subject,
        relationships: impl IntoIterator<Item = Relationship>,
        evidence: impl IntoIterator<Item = EvidenceId>,
    ) -> Self {
        let relationships: BTreeSet<Relationship> = relationships.into_iter().collect();
        let mut evidence: BTreeSet<EvidenceId> = evidence.into_iter().collect();
        for relationship in &relationships {
            evidence.extend(relationship.evidence().iter().copied());
        }
        Self {
            subject,
            relationships,
            evidence,
        }
    }

    /// The subject the bundle is about.
    pub fn subject(&self) -> &Subject {
        &self.subject
    }

    /// The relationships, sorted.
    pub fn relationships(&self) -> &BTreeSet<Relationship> {
        &self.relationships
    }

    /// The evidence IDs, sorted.
    pub fn evidence(&self) -> &BTreeSet<EvidenceId> {
        &self.evidence
    }

    /// The bundle's evidence hash (construction in the module docs).
    pub fn hash(&self) -> EvidenceHash {
        let mut leaves: Vec<[u8; 32]> = self
            .evidence
            .iter()
            .map(|id| hash(&[LEAF, &[EVIDENCE_TAG], id.as_bytes()]))
            .chain(
                self.relationships
                    .iter()
                    .map(|r| hash(&[LEAF, &[RELATIONSHIP_TAG], &canonical(r)])),
            )
            .collect();
        leaves.sort_unstable();
        let root = merkle_root(leaves);
        EvidenceHash::from_bytes(hash(&[ROOT, &canonical(&self.subject), &root]))
    }
}

impl Graph {
    /// The bundle for `subject`: every relationship used to reach a subject within
    /// `max_depth` hops in either direction, every relationship touching `subject`
    /// directly, the evidence they cite, and `subject_evidence` (facts about the
    /// subject itself). Independent of insertion order.
    pub fn bundle(
        &self,
        subject: &Subject,
        max_depth: usize,
        subject_evidence: impl IntoIterator<Item = EvidenceId>,
    ) -> EvidenceBundle {
        let mut relationships: BTreeSet<Relationship> = BTreeSet::new();
        for direction in [Direction::Outgoing, Direction::Incoming] {
            relationships.extend(self.edges(subject, direction, None).cloned());
            relationships.extend(
                self.traverse(subject, direction, &[], max_depth)
                    .into_iter()
                    .map(|reached| reached.via.clone()),
            );
        }
        EvidenceBundle::new(subject.clone(), relationships, subject_evidence)
    }
}

#[cfg(test)]
mod tests {
    use lumen_domain::{
        AppId, AppIdScheme, FileId, FileIdentity, RelationKind, UntrustedText, VolumeId,
    };
    use proptest::prelude::*;

    use super::*;

    fn file(n: u128) -> Subject {
        Subject::File {
            identity: FileIdentity {
                volume: VolumeId::new("v").unwrap_or_else(|_| unreachable!()),
                file: FileId::new(n),
            },
        }
    }

    fn app() -> Subject {
        Subject::Application {
            id: AppId {
                scheme: AppIdScheme::BundleId,
                value: UntrustedText::new("com.example.app"),
            },
        }
    }

    fn ev(n: u8) -> EvidenceId {
        EvidenceId::from_bytes([n; 32])
    }

    fn rel(from: Subject, kind: RelationKind, to: Subject, e: u8) -> Relationship {
        Relationship::new(from, kind, to, [ev(e)]).unwrap_or_else(|_| unreachable!())
    }

    fn sample() -> Vec<Relationship> {
        vec![
            rel(file(1), RelationKind::Contains, file(2), 1),
            rel(file(2), RelationKind::OwnedBy, app(), 2),
            rel(file(2), RelationKind::Contains, file(3), 3),
            rel(file(3), RelationKind::Contains, file(4), 4),
        ]
    }

    #[test]
    fn the_hash_is_pinned() {
        // Golden value, recomputed independently from the documented
        // construction (Python `blake3`, 2026-10-13). Changing it needs a
        // reviewed version bump.
        let bundle = EvidenceBundle::new(file(2), sample(), [ev(9)]);
        assert_eq!(
            bundle.hash().to_string(),
            "b3:84786d1b19386eac682d76604aee38a123aaa984c7b09164d8c506a7415850d2",
        );
        let empty = EvidenceBundle::new(file(2), [], []);
        assert_ne!(empty.hash(), bundle.hash());
    }

    #[test]
    fn bundles_collect_nearby_relationships_and_their_evidence() {
        let mut g = Graph::new();
        for r in sample() {
            assert!(g.add(r).is_ok());
        }
        let near = g.bundle(&file(2), 1, [ev(9)]);
        // Depth 1 from file 2: its container, its owner, its child; not 3 → 4.
        assert_eq!(near.relationships().len(), 3);
        assert_eq!(
            near.evidence().iter().copied().collect::<Vec<_>>(),
            [ev(1), ev(2), ev(3), ev(9)]
        );
        let far = g.bundle(&file(2), 2, [ev(9)]);
        assert_eq!(far.relationships().len(), 4);
        assert_ne!(near.hash(), far.hash(), "more evidence, different hash");
    }

    #[test]
    fn the_hash_binds_the_subject_and_every_part() {
        let base = EvidenceBundle::new(file(2), sample(), [ev(9)]);
        let other_subject = EvidenceBundle::new(file(3), sample(), [ev(9)]);
        let more_evidence = EvidenceBundle::new(file(2), sample(), [ev(9), ev(10)]);
        let fewer_edges = EvidenceBundle::new(file(2), sample().into_iter().skip(1), [ev(9)]);
        for different in [other_subject, more_evidence, fewer_edges] {
            assert_ne!(base.hash(), different.hash());
        }
    }

    proptest! {
        #[test]
        fn the_hash_ignores_insertion_order(
            picks in proptest::collection::vec(0usize..4, 0..8),
            extra in proptest::collection::vec(any::<u8>(), 0..6),
        ) {
            let edges: Vec<Relationship> = picks.iter().map(|&i| sample()[i].clone()).collect();
            let forward = EvidenceBundle::new(file(2), edges.clone(), extra.iter().map(|&e| ev(e)));
            let backward = EvidenceBundle::new(
                file(2),
                edges.into_iter().rev(),
                extra.iter().rev().map(|&e| ev(e)),
            );
            prop_assert_eq!(forward.hash(), backward.hash());
            prop_assert_eq!(forward, backward);
        }

        #[test]
        fn merkle_roots_differ_for_different_leaf_sets(
            a in proptest::collection::btree_set(any::<u8>(), 0..10),
            b in proptest::collection::btree_set(any::<u8>(), 0..10),
        ) {
            let leaves = |set: &BTreeSet<u8>| set.iter().map(|&n| hash(&[LEAF, &[n]])).collect::<Vec<_>>();
            prop_assert_eq!(a == b, merkle_root(leaves(&a)) == merkle_root(leaves(&b)));
        }
    }
}
