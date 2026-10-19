//! Grouping 5,000 feature vectors of 768 elements, the size ADR-0007's
//! performance spike clusters. Run with `cargo bench -p lumen-cluster`.

// Benchmark setup aborts on failure; there is no caller to report errors to.
#![allow(clippy::expect_used, missing_docs)]

use criterion::{Criterion, criterion_group, criterion_main};

/// Deterministic pseudo-random vectors: 1,000 "originals", each with four
/// slightly perturbed near-duplicates.
fn vectors() -> Vec<Vec<f32>> {
    let mut state = 0x2545_F491_4F6C_DD1D_u64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        #[allow(clippy::cast_precision_loss, reason = "noise in [0, 1)")]
        let unit = (state >> 40) as f32 / (1_u64 << 24) as f32;
        unit
    };
    let mut out = Vec::with_capacity(5_000);
    for _ in 0..1_000 {
        let original: Vec<f32> = (0..768).map(|_| next()).collect();
        for copy in 0..5 {
            out.push(
                original
                    .iter()
                    .map(|x| {
                        if copy == 0 {
                            *x
                        } else {
                            x + (next() - 0.5) * 0.01
                        }
                    })
                    .collect(),
            );
        }
    }
    out
}

fn group_5k(c: &mut Criterion) {
    let vectors = vectors();
    let refs: Vec<&[f32]> = vectors.iter().map(Vec::as_slice).collect();
    let mut group = c.benchmark_group("cluster");
    group.sample_size(10);
    group.bench_function("group 5k x 768", |b| {
        b.iter(|| lumen_cluster::group(&refs, 0.5).expect("valid input"));
    });
    group.finish();
}

criterion_group!(benches, group_5k);
criterion_main!(benches);
