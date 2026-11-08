//! Deterministic feature vectors for the clustering benchmarks (#778): the same
//! seed always builds the same vectors, so runs on one machine compare.

/// How alike the vectors are.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Kind {
    /// Unrelated vectors: almost every pair stops early, nothing groups.
    Sparse,
    /// One original and near-identical copies: every pair is compared in full
    /// and matches, the worst case for time and for matching pairs.
    Dense,
    /// Originals with four near-copies each, as a photo library might hold.
    Mixed,
}

impl Kind {
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Sparse => "sparse",
            Self::Dense => "dense",
            Self::Mixed => "mixed",
        }
    }
}

/// The threshold every benchmark groups with: near-copies (noise of ±0.005
/// per element) are within it, unrelated vectors (uniform in [0, 1)) are not.
pub(crate) const THRESHOLD: f32 = 0.5;

/// `count` vectors of `dims` elements of `kind`.
pub(crate) fn vectors(kind: Kind, count: usize, dims: usize) -> Vec<Vec<f32>> {
    let mut state = 0x2545_F491_4F6C_DD1D_u64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        #[allow(clippy::cast_precision_loss, reason = "noise in [0, 1)")]
        let unit = (state >> 40) as f32 / (1_u64 << 24) as f32;
        unit
    };
    let originals = match kind {
        Kind::Sparse => count,
        Kind::Dense => 1,
        Kind::Mixed => count.div_ceil(5),
    };
    let base: Vec<Vec<f32>> = (0..originals)
        .map(|_| (0..dims).map(|_| next()).collect())
        .collect();
    (0..count)
        .map(|i| {
            let original = &base[i % originals];
            if i < originals {
                original.clone()
            } else {
                original.iter().map(|x| x + (next() - 0.5) * 0.01).collect()
            }
        })
        .collect()
}
