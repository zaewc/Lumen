//! Scan throughput of the portable enumerator through the scheduler (roadmap
//! 5.19). Run with `cargo bench -p lumen-scan`; baselines are recorded in
//! `docs/architecture/performance.md`.

// Benchmark setup aborts on failure; there is no caller to report errors to.
#![allow(clippy::expect_used, missing_docs)]

use criterion::{Criterion, criterion_group, criterion_main};

#[cfg(unix)]
fn scan_tree(c: &mut Criterion) {
    use std::ops::ControlFlow;
    use std::os::unix::ffi::OsStrExt as _;

    use criterion::Throughput;
    use lumen_application::CancelToken;
    use lumen_domain::RawPath;
    use lumen_scan::{ScanConfig, ScanItem, StdFsEnumerator, scan};
    use lumen_testkit::{Fixture, Node};

    const DIRS: usize = 100;
    const FILES: usize = 100;
    let nodes: Vec<Node> = (0..DIRS)
        .flat_map(|d| {
            (0..FILES).map(move |f| Node::File {
                path: format!("tree/d{d:03}/f{f:03}"),
                contents: vec![0; 100],
            })
        })
        .collect();
    let fx = Fixture::build(&nodes).expect("fixture");
    let root = RawPath::from_unix_bytes(
        fx.path("tree")
            .expect("root")
            .as_os_str()
            .as_bytes()
            .to_vec(),
    )
    .expect("root path");
    let entries = 1 + DIRS + DIRS * FILES;

    let mut group = c.benchmark_group("scan");
    group.throughput(Throughput::Elements(u64::try_from(entries).expect("count")));
    group.sample_size(20);
    group.bench_function("std_fs/10k_files", |b| {
        b.iter(|| {
            let mut seen = 0;
            scan(
                &StdFsEnumerator,
                std::slice::from_ref(&root),
                &ScanConfig::default(),
                &CancelToken::new(),
                |item| {
                    if matches!(item, ScanItem::Entry(_)) {
                        seen += 1;
                    }
                    ControlFlow::Continue(())
                },
            );
            assert_eq!(seen, entries);
        });
    });
    group.finish();
}

#[cfg(not(unix))]
fn scan_tree(_c: &mut Criterion) {}

criterion_group!(benches, scan_tree);
criterion_main!(benches);
