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
//! The comparison is all-pairs, bounded by [`MAX_VECTORS`], and stops summing a
//! pair as soon as it exceeds the threshold. Small inputs are compared on the
//! calling thread; larger ones on the available cores, which take rows as
//! they finish (#781).

use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};
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

/// Below this many estimated chunk comparisons, every pair is compared on the
/// calling thread: starting threads costs more (#781). It is about 150 µs of
/// mixed comparisons on an M4 Pro, where all twelve threads overtake one
/// between 200 and 300 vectors of 768 elements (with #782's lanes and #783's
/// pruning; before them a chunk cost about twice as much).
const SEQUENTIAL_CHUNKS: u64 = 24_576;

/// Elements summed at a time before checking whether a pair is too far apart.
const CHUNK: usize = 64;

/// Partial sums kept apart within a chunk. Each lane adds its own elements in
/// order, so the compiler can add lanes with vector instructions (NEON, SSE,
/// AVX) where one running sum would chain every addition after the last
/// (#782). The order is fixed, so the result is the same on every run.
const LANES: usize = 16;

/// The sum of squared differences of one chunk of `a` and `b`.
fn chunk_distance(a: &[f32], b: &[f32]) -> f32 {
    let (whole_a, rest_a) = a.as_chunks::<LANES>();
    let (whole_b, rest_b) = b.as_chunks::<LANES>();
    let mut lanes = [0.0_f32; LANES];
    for (xs, ys) in whole_a.iter().zip(whole_b) {
        for ((lane, x), y) in lanes.iter_mut().zip(xs).zip(ys) {
            let d = x - y;
            *lane += d * d;
        }
    }
    let rest: f32 = rest_a
        .iter()
        .zip(rest_b)
        .map(|(x, y)| (x - y) * (x - y))
        .sum();
    lanes.iter().sum::<f32>() + rest
}

/// Whether `a` and `b` are at most `limit` apart, where `limit` is the
/// squared threshold, and how many chunks were summed; stops as soon as the
/// partial sum exceeds it.
fn within(a: &[f32], b: &[f32], limit: f32) -> (bool, u64) {
    let mut sum = 0.0_f32;
    let mut chunks = 0;
    for (chunk_a, chunk_b) in a.chunks(CHUNK).zip(b.chunks(CHUNK)) {
        chunks += 1;
        sum += chunk_distance(chunk_a, chunk_b);
        if sum > limit {
            return (false, chunks);
        }
    }
    (true, chunks)
}

/// Joins every pair `(row, j)`, `j > row`, that is within `limit` in `sets`;
/// how many chunks that summed. A pair already in one set is not compared:
/// its edge could not change any group (#783), which makes sets of near-copies
/// cost about one comparison per member instead of one per pair.
fn join_row(vectors: &[&[f32]], row: usize, limit: f32, sets: &mut Sets) -> u64 {
    let mut chunks = 0;
    let mut root = sets.find(row);
    for j in (row + 1)..vectors.len() {
        if sets.find(j) == root {
            continue;
        }
        let (close, summed) = within(vectors[row], vectors[j], limit);
        chunks += summed;
        if close {
            sets.union(row, j);
            root = sets.find(row);
        }
    }
    chunks
}

/// Each worker's sets after joining every row's pairs. The first row is
/// compared on the calling thread; what it cost per pair estimates the rest,
/// and a small rest stays there too. Otherwise `workers` threads take the
/// remaining rows one at a time, longest first, so a row that stops early
/// never leaves a thread idle while another has more to do.
fn join_all(vectors: &[&[f32]], limit: f32, workers: usize) -> Result<Vec<Sets>, ClusterError> {
    let n = vectors.len();
    let mut first = Sets::new(n);
    let first_pairs = u64::try_from(n - 1).unwrap_or(u64::MAX);
    let first_chunks = join_row(vectors, 0, limit, &mut first);
    let all_pairs = first_pairs.saturating_mul(u64::try_from(n).unwrap_or(u64::MAX)) / 2;
    let rest = all_pairs - first_pairs;
    let estimated = rest.saturating_mul(first_chunks) / first_pairs.max(1);
    if workers == 1 || estimated <= SEQUENTIAL_CHUNKS {
        for row in 1..n {
            join_row(vectors, row, limit, &mut first);
        }
        return Ok(vec![first]);
    }
    let next = AtomicUsize::new(1);
    let mut found: Vec<Sets> = thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                scope.spawn(|| {
                    let mut sets = Sets::new(n);
                    loop {
                        let row = next.fetch_add(1, Ordering::Relaxed);
                        if row >= n {
                            break;
                        }
                        join_row(vectors, row, limit, &mut sets);
                    }
                    sets
                })
            })
            .collect();
        // A failed worker would silently drop pairs: refuse instead.
        handles
            .into_iter()
            .map(|handle| handle.join().map_err(|_| ClusterError::Worker))
            .collect::<Result<_, _>>()
    })?;
    found.push(first);
    Ok(found)
}

/// The group of each vector: the smallest index in its group, so equal
/// numbers mean the same group and a singleton's group is its own index.
///
/// # Errors
///
/// A [`ClusterError`] for too many vectors, mismatched lengths, values that
/// are not finite, or a failed comparison thread.
pub fn group(vectors: &[&[f32]], threshold: f32) -> Result<Vec<usize>, ClusterError> {
    let workers = thread::available_parallelism().unwrap_or(NonZeroUsize::MIN);
    group_with_workers(vectors, threshold, workers)
}

/// [`group`] on `workers` threads (one: on the calling thread's scope alone),
/// for measuring and testing; the groups are the same for any number.
///
/// # Errors
///
/// As [`group`].
pub fn group_with_workers(
    vectors: &[&[f32]],
    threshold: f32,
    workers: NonZeroUsize,
) -> Result<Vec<usize>, ClusterError> {
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
    let workers = workers.get();
    // Each worker joins what it finds in its own sets as it goes, so memory
    // stays at two indices per vector per worker however many pairs match
    // (#779); a list of every matching pair would grow with the square of a
    // set of near-identical photos.
    let found = join_all(vectors, limit, workers)?;
    // The groups are the connected components of every worker's pairs
    // together, which joining each vector to its root in every worker's sets
    // reproduces, in any order.
    let mut sets = Sets::new(n);
    for mut worker in found {
        for i in 0..n {
            let root = worker.find(i);
            if root != i {
                sets.union(i, root);
            }
        }
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
    fn any_number_of_workers_finds_the_same_groups() -> Result<(), ClusterError> {
        // Points along a line, every third one a step too far: chains of three.
        let vectors: Vec<Vec<f32>> = (0..300_u16)
            .map(|i| vec![f32::from(i) + f32::from(i / 3) * 10.0, 0.0])
            .collect();
        let one = group_with_workers(&refs(&vectors), 1.0, NonZeroUsize::MIN)?;
        for workers in [2, 3, 7, 64] {
            let many = NonZeroUsize::new(workers).ok_or(ClusterError::Worker)?;
            assert_eq!(group_with_workers(&refs(&vectors), 1.0, many)?, one);
        }
        assert_eq!(group(&refs(&vectors), 1.0)?, one);
        assert_eq!(one[..6], [0, 0, 0, 3, 3, 3]);
        Ok(())
    }

    /// Plain connected components over every pair, the definition `group`
    /// must meet whatever it does to save memory or time.
    fn reference(vectors: &[Vec<f32>], threshold: f32) -> Vec<usize> {
        let n = vectors.len();
        let mut sets = Sets::new(n);
        for i in 0..n {
            for j in (i + 1)..n {
                let distance: f32 = vectors[i]
                    .iter()
                    .zip(&vectors[j])
                    .map(|(x, y)| (x - y) * (x - y))
                    .sum();
                if distance <= threshold * threshold {
                    sets.union(i, j);
                }
            }
        }
        let mut smallest = vec![usize::MAX; n];
        for i in 0..n {
            let root = sets.find(i);
            smallest[root] = smallest[root].min(i);
        }
        (0..n).map(|i| smallest[sets.find(i)]).collect()
    }

    #[test]
    fn groups_match_plain_connected_components() -> Result<(), ClusterError> {
        let mut state = 0x9E37_79B9_7F4A_7C15_u64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            f32::from(u16::try_from(state >> 48).unwrap_or_default()) / 65_536.0
        };
        let vectors: Vec<Vec<f32>> = (0..240).map(|_| (0..3).map(|_| next()).collect()).collect();
        // 60 vectors stay on the calling thread whatever the worker count;
        // 240 take the threads (#781).
        for count in [2, 60, 240] {
            let vectors = &vectors[..count];
            for threshold in [0.0, 0.05, 0.12, 0.3, 2.0] {
                let expected = reference(vectors, threshold);
                for workers in [1, 2, 5, 16] {
                    let workers = NonZeroUsize::new(workers).ok_or(ClusterError::Worker)?;
                    assert_eq!(
                        group_with_workers(&refs(vectors), threshold, workers)?,
                        expected,
                        "{count} vectors, threshold {threshold}, {workers} workers"
                    );
                }
            }
        }
        Ok(())
    }

    /// Distances exactly at the threshold group, and a hair beyond do not,
    /// whatever length splits elements between lanes, a lane's tail and
    /// chunks (#782). Differences of one make every sum exact.
    #[test]
    fn the_threshold_is_exact_across_lane_and_chunk_boundaries() -> Result<(), ClusterError> {
        for (dims, threshold) in [
            (1_usize, 1.0_f32),
            (4, 2.0),
            (16, 4.0),
            (64, 8.0),
            (81, 9.0),
            (100, 10.0),
            (144, 12.0),
        ] {
            let pair = vec![vec![0.0; dims], vec![1.0; dims]];
            assert_eq!(
                group(&refs(&pair), threshold)?,
                [0, 0],
                "{dims} at {threshold}"
            );
            let below = threshold - threshold * f32::EPSILON * 4.0;
            assert_eq!(
                group(&refs(&pair), below)?,
                [0, 1],
                "{dims} below {threshold}"
            );
        }
        // Lengths that leave a tail in every lane and chunk split.
        for dims in [15, 17, 63, 65, 129] {
            let pair = vec![vec![0.0; dims], vec![0.5; dims]];
            let exact = f32::from(u16::try_from(dims).unwrap_or(u16::MAX)) * 0.25;
            assert_eq!(
                group(&refs(&pair), exact.sqrt())?,
                reference(&pair, exact.sqrt())
            );
        }
        Ok(())
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
