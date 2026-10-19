//! Near-duplicate grouping of image feature vectors (ADR-0019, ADR-0007).
//!
//! The mobile apps compute a feature vector per photo natively (Vision feature
//! prints on iOS); this crate groups them so the same logic can serve desktop
//! photo folders. Two images are near-duplicates when the Euclidean distance
//! between their vectors is at most a threshold, and groups are the connected
//! components of that relation (single linkage), so a group never depends on
//! the order of its members.
//!
//! Groups are evidence for a `REVIEW` proposal only; they never quarantine
//! anything on their own (ADR-0019), and thresholds need a labelled
//! evaluation set before they ship.
//!
//! The comparison is all-pairs, bounded by [`MAX_VECTORS`], split across the
//! available cores with scoped threads, and stops summing a pair as soon as it
//! exceeds the threshold.

use std::num::NonZeroUsize;
use std::thread;

/// The most vectors one call groups: all-pairs work grows with the square.
pub const MAX_VECTORS: usize = 20_000;

/// Why vectors could not be grouped.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum ClusterError {
    /// More than [`MAX_VECTORS`] vectors.
    #[error("{0} vectors are more than the {MAX_VECTORS} one call can group")]
    TooMany(usize),
    /// Vectors of different lengths, or empty ones.
    #[error("vector {index} has {found} elements; expected {expected}")]
    Dimension {
        /// Which vector.
        index: usize,
        /// Its length.
        found: usize,
        /// The first vector's length.
        expected: usize,
    },
    /// A threshold or element that is not a finite, non-negative number.
    #[error("the threshold and every element must be finite (threshold >= 0)")]
    NotFinite,
    /// A comparison thread failed, so the groups would be incomplete.
    #[error("a comparison thread failed")]
    Worker,
}

/// Disjoint sets over indices, with path halving and union by size.
struct Sets {
    parent: Vec<usize>,
    size: Vec<usize>,
}

impl Sets {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
            size: vec![1; n],
        }
    }

    fn find(&mut self, mut i: usize) -> usize {
        while self.parent[i] != i {
            self.parent[i] = self.parent[self.parent[i]];
            i = self.parent[i];
        }
        i
    }

    fn union(&mut self, a: usize, b: usize) {
        let (mut a, mut b) = (self.find(a), self.find(b));
        if a == b {
            return;
        }
        if self.size[a] < self.size[b] {
            std::mem::swap(&mut a, &mut b);
        }
        self.parent[b] = a;
        self.size[a] += self.size[b];
    }
}

/// Whether `a` and `b` are at most `limit` apart, where `limit` is the
/// squared threshold; stops as soon as the partial sum exceeds it.
fn within(a: &[f32], b: &[f32], limit: f32) -> bool {
    let mut sum = 0.0_f32;
    for (chunk_a, chunk_b) in a.chunks(64).zip(b.chunks(64)) {
        sum += chunk_a
            .iter()
            .zip(chunk_b)
            .map(|(x, y)| (x - y) * (x - y))
            .sum::<f32>();
        if sum > limit {
            return false;
        }
    }
    true
}

/// Every pair `(i, j)`, `i < j`, with `i` in `rows`, that is within `limit`.
fn close_pairs(
    vectors: &[&[f32]],
    rows: std::ops::Range<usize>,
    limit: f32,
) -> Vec<(usize, usize)> {
    let mut pairs = Vec::new();
    for i in rows {
        for j in (i + 1)..vectors.len() {
            if within(vectors[i], vectors[j], limit) {
                pairs.push((i, j));
            }
        }
    }
    pairs
}

/// The group of each vector: the smallest index in its group, so equal
/// numbers mean the same group and a singleton's group is its own index.
///
/// # Errors
///
/// A [`ClusterError`] for too many vectors, mismatched lengths, values that
/// are not finite, or a failed comparison thread.
pub fn group(vectors: &[&[f32]], threshold: f32) -> Result<Vec<usize>, ClusterError> {
    if vectors.len() > MAX_VECTORS {
        return Err(ClusterError::TooMany(vectors.len()));
    }
    if !threshold.is_finite() || threshold < 0.0 {
        return Err(ClusterError::NotFinite);
    }
    let Some(first) = vectors.first() else {
        return Ok(Vec::new());
    };
    let expected = first.len();
    for (index, vector) in vectors.iter().enumerate() {
        if vector.len() != expected || expected == 0 {
            return Err(ClusterError::Dimension {
                index,
                found: vector.len(),
                expected,
            });
        }
        if !vector.iter().all(|x| x.is_finite()) {
            return Err(ClusterError::NotFinite);
        }
    }
    let limit = threshold * threshold;
    let n = vectors.len();
    let workers = thread::available_parallelism().map_or(1, NonZeroUsize::get);
    // Row i compares with n - i - 1 others; interleaving rows across workers
    // evens out the triangle.
    let pairs: Vec<Vec<(usize, usize)>> = thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|w| {
                scope.spawn(move || {
                    let mut found = Vec::new();
                    let mut row = w;
                    while row < n {
                        found.extend(close_pairs(vectors, row..row + 1, limit));
                        row += workers;
                    }
                    found
                })
            })
            .collect();
        // A failed worker would silently drop pairs: refuse instead.
        handles
            .into_iter()
            .map(|handle| handle.join().map_err(|_| ClusterError::Worker))
            .collect::<Result<_, _>>()
    })?;
    let mut sets = Sets::new(n);
    for (i, j) in pairs.into_iter().flatten() {
        sets.union(i, j);
    }
    // The smallest member of each set names the group.
    let mut smallest = vec![usize::MAX; n];
    for i in 0..n {
        let root = sets.find(i);
        smallest[root] = smallest[root].min(i);
    }
    Ok((0..n).map(|i| smallest[sets.find(i)]).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refs(vectors: &[Vec<f32>]) -> Vec<&[f32]> {
        vectors.iter().map(Vec::as_slice).collect()
    }

    #[test]
    fn close_vectors_group_transitively_and_far_ones_stay_alone() {
        let vectors = vec![
            vec![0.0, 0.0],
            vec![0.5, 0.0],
            vec![10.0, 10.0],
            vec![1.0, 0.0],
            vec![10.2, 10.0],
        ];
        assert_eq!(group(&refs(&vectors), 0.6), Ok(vec![0, 0, 2, 0, 2]));
        assert_eq!(group(&refs(&vectors), 0.1), Ok(vec![0, 1, 2, 3, 4]));
    }

    #[test]
    fn the_threshold_is_inclusive_and_order_does_not_matter() {
        let a = vec![vec![0.0, 0.0], vec![3.0, 4.0]];
        assert_eq!(group(&refs(&a), 5.0), Ok(vec![0, 0]));
        let reversed: Vec<Vec<f32>> = a.iter().rev().cloned().collect();
        assert_eq!(group(&refs(&reversed), 5.0), Ok(vec![0, 0]));
    }

    #[test]
    fn bad_input_is_refused() {
        let mismatched = vec![vec![0.0, 0.0], vec![0.0]];
        assert!(matches!(
            group(&refs(&mismatched), 1.0),
            Err(ClusterError::Dimension { index: 1, .. })
        ));
        let nan = vec![vec![f32::NAN]];
        assert_eq!(group(&refs(&nan), 1.0), Err(ClusterError::NotFinite));
        assert_eq!(
            group(&refs(&[vec![1.0]]), -1.0),
            Err(ClusterError::NotFinite)
        );
        assert_eq!(group(&[], 1.0), Ok(Vec::new()));
        let empty = vec![Vec::<f32>::new()];
        assert!(matches!(
            group(&refs(&empty), 1.0),
            Err(ClusterError::Dimension { .. })
        ));
    }
}
