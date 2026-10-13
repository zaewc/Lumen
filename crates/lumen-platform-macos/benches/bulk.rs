//! Throughput of the `getattrlistbulk` enumerator and its parser (roadmap 5.19).
//! Run with `cargo bench -p lumen-platform-macos`; baselines are recorded in
//! `docs/architecture/performance.md`.

// Benchmark setup aborts on failure; there is no caller to report errors to.
#![allow(clippy::expect_used, missing_docs)]

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use lumen_platform_macos::attrbuf::entries;

/// One real 168-byte entry captured on macOS 26 (see `attrbuf` tests): a file
/// named `big` with every attribute Lumen requests, including `PRIVATESIZE`.
const BIG: [u32; 42] = [
    0x0000_00a8,
    0x8204_1e0b,
    0,
    0,
    7,
    0x0000_0308,
    0x0000_008c,
    4,
    0x0100_0010,
    1,
    0x6acc_74c4,
    0,
    0x2800_b6d7,
    0,
    0x6acc_74c4,
    0,
    0x2802_5d33,
    0,
    0x6acc_74c4,
    0,
    0x2802_5d33,
    0,
    0x6acc_74c4,
    0,
    0x2800_b6d7,
    0,
    0,
    0x07ff_feef,
    0,
    1,
    0x5000,
    0,
    0x5000,
    0,
    0x5000,
    0,
    0x07ff_feef,
    0,
    0,
    0,
    0,
    0x0067_6962,
];

fn parse_buffer(c: &mut Criterion) {
    let entry: Vec<u8> = BIG.iter().flat_map(|w| w.to_le_bytes()).collect();
    // As many entries as fit the enumerator's 32 KiB buffer.
    let count = 32 * 1024 / entry.len();
    let buffer = entry.repeat(count);
    let mut group = c.benchmark_group("attrbuf");
    group.throughput(Throughput::Elements(u64::try_from(count).expect("count")));
    group.bench_function("parse_32k_buffer", |b| {
        b.iter(|| {
            let parsed = entries(std::hint::black_box(&buffer), count)
                .flatten()
                .count();
            assert_eq!(parsed, count);
        });
    });
    group.finish();
}

#[cfg(target_os = "macos")]
fn scan_tree(c: &mut Criterion) {
    use std::os::unix::ffi::OsStrExt as _;

    use lumen_domain::RawPath;
    use lumen_platform_macos::BulkEnumerator;
    use lumen_scan::StdFsEnumerator;
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
    let expected = 1 + DIRS + DIRS * FILES;

    let mut group = c.benchmark_group("scan");
    group.throughput(Throughput::Elements(
        u64::try_from(expected).expect("count"),
    ));
    group.sample_size(20);
    bench(
        &mut group,
        "bulk/10k_files",
        &BulkEnumerator::new(),
        &root,
        expected,
    );
    bench(
        &mut group,
        "bulk_quick/10k_files",
        &BulkEnumerator::without_private_size(),
        &root,
        expected,
    );
    bench(
        &mut group,
        "std_fs/10k_files",
        &StdFsEnumerator,
        &root,
        expected,
    );
    group.finish();
}

#[cfg(target_os = "macos")]
fn bench<E: lumen_application::ports::DirEnumerator>(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
    name: &str,
    enumerator: &E,
    root: &lumen_domain::RawPath,
    expected: usize,
) {
    use std::ops::ControlFlow;

    use lumen_application::CancelToken;
    use lumen_scan::{ScanConfig, ScanItem, scan};

    group.bench_function(name, |b| {
        b.iter(|| {
            let mut seen = 0;
            scan(
                enumerator,
                std::slice::from_ref(root),
                &ScanConfig::default(),
                &CancelToken::new(),
                |item| {
                    if matches!(item, ScanItem::Entry(_)) {
                        seen += 1;
                    }
                    ControlFlow::Continue(())
                },
            );
            assert_eq!(seen, expected);
        });
    });
}

#[cfg(not(target_os = "macos"))]
fn scan_tree(_c: &mut Criterion) {}

criterion_group!(benches, parse_buffer, scan_tree);
criterion_main!(benches);
