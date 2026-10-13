//! Lumen's evidence graph (ADR-0013).
//!
//! An in-memory, typed property graph built per analysis: [`Subject`]s are the
//! nodes and evidence-backed [`Relationship`]s the directed edges. Explanations
//! ("What created this? Is anything using it?") are shallow, candidate-anchored
//! [`Graph::traverse`] calls. The graph is persisted relationally by the store;
//! this crate is IO-free and platform-free.
//!
//! Guarantees by construction:
//!
//! - every subject appears once (keyed by its identity, never by path);
//! - every edge's endpoints are in the graph (adding an edge adds them);
//! - one edge per `(from, kind, to)`: adding it again merges its evidence;
//! - iteration and traversal follow insertion order, so the same inputs always
//!   produce the same explanation.

#![forbid(unsafe_code)]

use std::collections::{HashMap, HashSet, VecDeque};

use lumen_domain::{RelationKind, Relationship, Subject};

/// Which way to follow edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// From a subject along edges it starts (`subject → other`).
    Outgoing,
    /// Back along edges that end at a subject (`other → subject`).
    Incoming,
}

/// A subject reached by [`Graph::traverse`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reached<'g> {
    /// The subject.
    pub subject: &'g Subject,
    /// Number of edges from the start (at least 1).
    pub depth: usize,
    /// The edge it was first reached through.
    pub via: &'g Relationship,
}

/// The evidence graph.
#[derive(Debug, Clone, Default)]
pub struct Graph {
    index: HashMap<Subject, usize>,
    subjects: Vec<Subject>,
    edges: Vec<Relationship>,
    edge_index: HashMap<(usize, RelationKind, usize), usize>,
    outgoing: Vec<Vec<usize>>,
    incoming: Vec<Vec<usize>>,
}

impl Graph {
    /// An empty graph.
    pub fn new() -> Self {
        Self::default()
    }

    fn node(&mut self, subject: &Subject) -> usize {
        if let Some(&index) = self.index.get(subject) {
            return index;
        }
        let index = self.subjects.len();
        self.subjects.push(subject.clone());
        self.index.insert(subject.clone(), index);
        self.outgoing.push(Vec::new());
        self.incoming.push(Vec::new());
        index
    }

    /// Adds a subject with no edges yet. Returns `false` if it was already present.
    pub fn add_subject(&mut self, subject: &Subject) -> bool {
        let before = self.subjects.len();
        self.node(subject);
        self.subjects.len() > before
    }

    /// Adds a relationship and its endpoints. If an edge with the same `from`,
    /// `kind` and `to` exists, its evidence becomes the union of both. Returns
    /// `true` if a new edge was created.
    pub fn add(&mut self, relationship: Relationship) -> bool {
        let from = self.node(relationship.from());
        let to = self.node(relationship.to());
        let key = (from, relationship.kind(), to);
        if let Some(&existing) = self.edge_index.get(&key) {
            let current = &self.edges[existing];
            if !relationship.evidence().is_subset(current.evidence()) {
                let evidence = current
                    .evidence()
                    .iter()
                    .chain(relationship.evidence())
                    .copied();
                // Both inputs were valid with the same endpoints and kind, so the
                // union is valid too; on the impossible error the edge is kept.
                if let Ok(merged) = Relationship::new(
                    current.from().clone(),
                    current.kind(),
                    current.to().clone(),
                    evidence,
                ) {
                    self.edges[existing] = merged;
                }
            }
            return false;
        }
        let index = self.edges.len();
        self.edges.push(relationship);
        self.edge_index.insert(key, index);
        self.outgoing[from].push(index);
        self.incoming[to].push(index);
        true
    }

    /// Whether `subject` is in the graph.
    pub fn contains(&self, subject: &Subject) -> bool {
        self.index.contains_key(subject)
    }

    /// Number of subjects.
    pub fn subject_count(&self) -> usize {
        self.subjects.len()
    }

    /// Number of relationships.
    pub fn relationship_count(&self) -> usize {
        self.edges.len()
    }

    /// All subjects, in insertion order.
    pub fn subjects(&self) -> impl Iterator<Item = &Subject> {
        self.subjects.iter()
    }

    /// All relationships, in insertion order.
    pub fn relationships(&self) -> impl Iterator<Item = &Relationship> {
        self.edges.iter()
    }

    /// The relationships touching `subject` in `direction`, optionally of one
    /// kind, in insertion order. Empty for an unknown subject.
    pub fn edges<'g>(
        &'g self,
        subject: &Subject,
        direction: Direction,
        kind: Option<RelationKind>,
    ) -> impl Iterator<Item = &'g Relationship> + 'g {
        let list = self
            .index
            .get(subject)
            .map_or(&[][..], |&node| match direction {
                Direction::Outgoing => &self.outgoing[node][..],
                Direction::Incoming => &self.incoming[node][..],
            });
        list.iter()
            .map(|&edge| &self.edges[edge])
            .filter(move |edge| kind.is_none_or(|k| edge.kind() == k))
    }

    /// Breadth-first traversal from `start` along edges of the given kinds (all
    /// kinds if `kinds` is empty), up to `max_depth` edges away. Each subject is
    /// reported once, at its smallest depth, with the edge it was first reached
    /// through; `start` itself is not reported. Unknown starts reach nothing.
    pub fn traverse(
        &self,
        start: &Subject,
        direction: Direction,
        kinds: &[RelationKind],
        max_depth: usize,
    ) -> Vec<Reached<'_>> {
        let Some(&origin) = self.index.get(start) else {
            return Vec::new();
        };
        let mut seen = HashSet::from([origin]);
        let mut queue = VecDeque::from([(origin, 0)]);
        let mut reached = Vec::new();
        while let Some((node, depth)) = queue.pop_front() {
            if depth == max_depth {
                continue;
            }
            let list = match direction {
                Direction::Outgoing => &self.outgoing[node],
                Direction::Incoming => &self.incoming[node],
            };
            for &edge in list {
                let relationship = &self.edges[edge];
                if !kinds.is_empty() && !kinds.contains(&relationship.kind()) {
                    continue;
                }
                let other = match direction {
                    Direction::Outgoing => relationship.to(),
                    Direction::Incoming => relationship.from(),
                };
                let Some(&next) = self.index.get(other) else {
                    continue;
                };
                if seen.insert(next) {
                    reached.push(Reached {
                        subject: &self.subjects[next],
                        depth: depth + 1,
                        via: relationship,
                    });
                    queue.push_back((next, depth + 1));
                }
            }
        }
        reached
    }
}

#[cfg(test)]
mod tests {
    use lumen_domain::{
        AppId, AppIdScheme, EvidenceId, FileId, FileIdentity, UntrustedText, VolumeId,
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

    fn app(name: &str) -> Subject {
        Subject::Application {
            id: AppId {
                scheme: AppIdScheme::BundleId,
                value: UntrustedText::new(name),
            },
        }
    }

    fn evidence(n: u8) -> EvidenceId {
        EvidenceId::from_bytes([n; 32])
    }

    fn rel(from: Subject, kind: RelationKind, to: Subject, ev: u8) -> Relationship {
        Relationship::new(from, kind, to, [evidence(ev)]).unwrap_or_else(|_| unreachable!())
    }

    #[test]
    fn adding_an_edge_adds_its_endpoints_once() {
        let mut g = Graph::new();
        assert!(g.add(rel(file(1), RelationKind::Contains, file(2), 1)));
        assert!(g.add(rel(file(1), RelationKind::Contains, file(3), 1)));
        assert_eq!(g.subject_count(), 3);
        assert_eq!(g.relationship_count(), 2);
        assert!(!g.add_subject(&file(2)), "already present");
        assert!(g.add_subject(&file(9)));
        assert!(g.contains(&file(9)));
    }

    #[test]
    fn duplicate_edges_merge_their_evidence() {
        let mut g = Graph::new();
        assert!(g.add(rel(file(1), RelationKind::OwnedBy, app("com.example.a"), 1)));
        assert!(!g.add(rel(file(1), RelationKind::OwnedBy, app("com.example.a"), 2)));
        assert!(!g.add(rel(file(1), RelationKind::OwnedBy, app("com.example.a"), 1)));
        assert_eq!(g.relationship_count(), 1);
        let only = g.relationships().next().map(|r| r.evidence().len());
        assert_eq!(only, Some(2));
        // A different kind between the same subjects is a separate edge.
        assert!(g.add(rel(
            file(1),
            RelationKind::CreatedBy,
            app("com.example.a"),
            3
        )));
    }

    #[test]
    fn edges_filter_by_direction_and_kind() {
        let mut g = Graph::new();
        g.add(rel(file(1), RelationKind::Contains, file(2), 1));
        g.add(rel(file(2), RelationKind::OwnedBy, app("com.example.a"), 1));
        g.add(rel(file(3), RelationKind::OwnedBy, app("com.example.a"), 1));
        let owners: Vec<_> = g
            .edges(
                &app("com.example.a"),
                Direction::Incoming,
                Some(RelationKind::OwnedBy),
            )
            .map(|r| r.from().clone())
            .collect();
        assert_eq!(owners, [file(2), file(3)]);
        assert_eq!(g.edges(&file(2), Direction::Outgoing, None).count(), 1);
        assert_eq!(
            g.edges(&file(2), Direction::Outgoing, Some(RelationKind::Contains))
                .count(),
            0
        );
        assert_eq!(g.edges(&file(42), Direction::Outgoing, None).count(), 0);
    }

    #[test]
    fn traversal_is_bounded_breadth_first_and_reports_each_subject_once() {
        // 1 contains 2 and 3; 2 contains 4; 3 contains 4 (a diamond); 4 contains 5.
        let mut g = Graph::new();
        for (a, b) in [(1, 2), (1, 3), (2, 4), (3, 4), (4, 5)] {
            g.add(rel(file(a), RelationKind::Contains, file(b), 1));
        }
        let found: Vec<_> = g
            .traverse(&file(1), Direction::Outgoing, &[RelationKind::Contains], 2)
            .iter()
            .map(|r| (r.subject.clone(), r.depth))
            .collect();
        assert_eq!(found, [(file(2), 1), (file(3), 1), (file(4), 2)]);
        let up: Vec<_> = g
            .traverse(&file(5), Direction::Incoming, &[], 10)
            .iter()
            .map(|r| r.subject.clone())
            .collect();
        assert_eq!(up, [file(4), file(2), file(3), file(1)]);
        assert_eq!(g.traverse(&file(1), Direction::Outgoing, &[], 0).len(), 0);
        assert_eq!(g.traverse(&file(99), Direction::Outgoing, &[], 4).len(), 0);
        assert_eq!(
            g.traverse(&file(1), Direction::Outgoing, &[RelationKind::OwnedBy], 4)
                .len(),
            0
        );
    }

    #[test]
    fn traversal_reports_the_edge_used() {
        let mut g = Graph::new();
        g.add(rel(file(1), RelationKind::OwnedBy, app("com.example.a"), 7));
        let reached = g.traverse(&file(1), Direction::Outgoing, &[], 1);
        assert_eq!(reached.len(), 1);
        assert_eq!(reached[0].via.kind(), RelationKind::OwnedBy);
        assert!(reached[0].via.evidence().contains(&evidence(7)));
    }

    proptest! {
        #[test]
        fn traversal_never_repeats_or_exceeds_its_depth(
            edges in proptest::collection::vec((0u128..12, 0u128..12), 0..60),
            start in 0u128..12,
            max_depth in 0usize..6,
        ) {
            let mut g = Graph::new();
            for (a, b) in edges {
                if a != b {
                    g.add(rel(file(a), RelationKind::Contains, file(b), 1));
                }
            }
            let reached = g.traverse(&file(start), Direction::Outgoing, &[], max_depth);
            let mut unique = HashSet::new();
            for r in &reached {
                prop_assert!(unique.insert(r.subject.clone()), "repeated subject");
                prop_assert!(r.depth >= 1 && r.depth <= max_depth);
                prop_assert!(r.subject != &file(start));
                prop_assert_eq!(r.via.to(), r.subject);
            }
            // Edges always have both endpoints in the graph.
            for e in g.relationships() {
                prop_assert!(g.contains(e.from()) && g.contains(e.to()));
            }
        }
    }
}
