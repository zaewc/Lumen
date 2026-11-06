//! Clustering time (#778): sizes from 100 to 20,000 vectors, sparse, dense and
//! mixed similarity, several embedding sizes, and one worker against all of
//! them. Run with `cargo bench -p lumen-cluster --bench cluster`; filter with
//! a name, e.g. `-- size`. Peak memory is the `memory` benchmark.
//! Baselines: docs/architecture/performance.md.

// Benchmark setup aborts on failure; there is no caller to report errors to.
#![allow(clippy::expect_used, missing_docs)]

mod data;

use std::num::NonZeroUsize;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use data::{Kind, THRESHOLD, vectors};

fn all_workers() -> NonZeroUsize {
    std::thread::available_parallelism().unwrap_or(NonZeroUsize::MIN)
}

fn run(vectors: &[Vec<f32>], workers: NonZeroUsize) -> Vec<usize> {
    let refs: Vec<&[f32]> = vectors.iter().map(Vec::as_slice).collect();
    lumen_cluster::group_with_workers(&refs, THRESHOLD, workers).expect("valid input")
}

/// Mixed similarity at 768 elements, from 100 to 20,000 vectors.
fn size(c: &mut Criterion) {
    let mut group = c.benchmark_group("size");
    group.sample_size(10);
    for count in [100, 1_000, 5_000, 10_000, 20_000] {
        let data = vectors(Kind::Mixed, count, 768);
        group.bench_with_input(BenchmarkId::new("mixed x 768", count), &data, |b, data| {
            b.iter(|| run(data, all_workers()));
        });
    }
    group.finish();
}

/// 5,000 vectors of 768 elements, from nothing alike to all alike.
fn similarity(c: &mut Criterion) {
    let mut group = c.benchmark_group("similarity");
    group.sample_size(10);
    for kind in [Kind::Sparse, Kind::Mixed, Kind::Dense] {
        let data = vectors(kind, 5_000, 768);
        group.bench_with_input(BenchmarkId::new(kind.name(), 5_000), &data, |b, data| {
            b.iter(|| run(data, all_workers()));
        });
    }
    group.finish();
}

/// 5,000 mixed vectors of 128, 768 and 2,048 elements.
fn dimensions(c: &mut Criterion) {
    let mut group = c.benchmark_group("dimensions");
    group.sample_size(10);
    for dims in [128, 768, 2_048] {
        let data = vectors(Kind::Mixed, 5_000, dims);
        group.bench_with_input(BenchmarkId::new("mixed 5k", dims), &data, |b, data| {
            b.iter(|| run(data, all_workers()));
        });
    }
    group.finish();
}

/// One worker against every core, from 100 to 5,000 mixed vectors.
fn workers(c: &mut Criterion) {
    let mut group = c.benchmark_group("workers");
    group.sample_size(10);
    for count in [100, 1_000, 5_000] {
        let data = vectors(Kind::Mixed, count, 768);
        for (name, workers) in [("one", NonZeroUsize::MIN), ("all", all_workers())] {
            group.bench_with_input(BenchmarkId::new(name, count), &data, |b, data| {
                b.iter(|| run(data, workers));
            });
        }
    }
    group.finish();
}

criterion_group!(benches, size, similarity, dimensions, workers);
criterion_main!(benches);
