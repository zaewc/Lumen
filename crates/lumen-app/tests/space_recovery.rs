//! Manual storage experiment, confined to fresh temporary fixtures. The real
//! scanner/assessment and executor are measured separately: this is not a
//! production cleanup plan, nor permission to execute a KEEP/REVIEW decision.

#![cfg(unix)]

use std::error::Error;
use std::fs::File;
use std::io::Write as _;
use std::os::unix::fs::MetadataExt as _;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use lumen_app::space_change::SpaceWatch;
use lumen_app::{SystemClock, UuidV7Ids, raw_path, space};
use lumen_application::CancelToken;
use lumen_application::ports::{Located, NoopJudge, QuarantineExecutor as _};
use lumen_domain::{OperationId, Verdict};
use lumen_fs_exec::Executor;
use lumen_testkit::{Fixture, Node};
use serde_json::{Value, json};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

const PRESERVED: [&str; 5] = [
    "projects/rust/src/main.rs",
    "projects/rust/target/release/app",
    "projects/rust/target/package/app.crate",
    "projects/web/.next/server/app.js",
    "projects/web/source.ts",
];

/// Deterministic non-sparse payload; no cloned source or system-wide sync.
fn payload(bytes: usize) -> Vec<u8> {
    let mut state = 0x1234_5678_9abc_def0_u64;
    (0..bytes)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state.to_le_bytes()[0]
        })
        .collect()
}

fn make_tree(case: &str, cache: &str, bytes: usize) -> Result<Fixture> {
    let file = format!("{cache}/data.bin");
    let data = if case == "sparse" {
        Node::Sparse {
            path: file.clone(),
            len: u64::try_from(bytes)?,
        }
    } else {
        Node::File {
            path: file.clone(),
            contents: payload(bytes),
        }
    };
    // A second fixture owns the entire experiment; no caller-supplied paths.
    let mut nodes = vec![
        data,
        Node::File {
            path: "projects/rust/Cargo.toml".into(),
            contents: b"[package]\nname = 'fixture'\nversion = '0.0.0'\n".to_vec(),
        },
        Node::File {
            path: "projects/web/package.json".into(),
            contents: b"{}".to_vec(),
        },
    ];
    for path in PRESERVED {
        nodes.push(Node::File {
            path: path.into(),
            contents: b"keep this output or source".to_vec(),
        });
    }
    if case == "hard_link" {
        nodes.push(Node::HardLink {
            path: "preserved-link".into(),
            existing: file.clone(),
        });
    }
    Ok(Fixture::build(&nodes)?)
}

fn readings(root: &Path, timer: Instant, available: u64) -> Result<Vec<Value>> {
    let mut readings = vec![json!({
        "elapsed_ms": timer.elapsed().as_millis(),
        "available_bytes": available,
    })];
    for elapsed in [250, 1000, 3000] {
        thread::sleep(Duration::from_millis(elapsed).saturating_sub(timer.elapsed()));
        readings.push(json!({
            "elapsed_ms": timer.elapsed().as_millis(),
            "available_bytes": space::volume(root)?.available,
        }));
    }
    Ok(readings)
}

fn assert_preserved(
    tree: &Fixture,
    metadata: &std::fs::Metadata,
    hard_link: bool,
) -> Result<usize> {
    for path in PRESERVED {
        assert_eq!(
            std::fs::read(tree.path(path)?)?,
            b"keep this output or source"
        );
    }
    if hard_link {
        let kept = std::fs::metadata(tree.path("preserved-link")?)?;
        assert_eq!((kept.dev(), kept.ino()), (metadata.dev(), metadata.ino()));
        assert_eq!(
            (kept.len(), kept.blocks()),
            (metadata.len(), metadata.blocks())
        );
    }
    Ok(PRESERVED.len() + usize::from(hard_link))
}

fn experiment(case: &str, cache: &str, bytes: usize) -> Result<Value> {
    let fx = Fixture::build(&[])?;
    let db = fx.path("index.db")?;
    // Leave the normal reserve plus room for the fixture and its small index.
    space::ensure(&db, u64::try_from(bytes)? + 32 * 1024 * 1024)?;
    let before_fixture = space::volume(fx.root())?.available;
    let tree = make_tree(case, cache, bytes)?;
    let file = format!("{cache}/data.bin");
    let root = std::fs::canonicalize(tree.root())?;
    assert_eq!(
        std::fs::metadata(fx.root())?.dev(),
        std::fs::metadata(&root)?.dev(),
        "reserve check and fixture must use the same volume"
    );
    File::open(tree.path(&file)?)?.sync_all()?;
    let metadata = std::fs::metadata(tree.path(&file)?)?;
    let after_fixture = space::volume(&root)?.available;
    let scan = lumen_app::scan(
        &[root.join("projects")],
        Some(&db),
        &SystemClock,
        &UuidV7Ids,
        &CancelToken::new(),
        &mut |_| {},
    )?;
    assert!(scan.stopped.is_none() && !scan.out_of_space);
    assert!(scan.roots.iter().all(|r| r.coverage.is_complete()));
    let assessed = lumen_app::assess(&db, &SystemClock, &NoopJudge, &NoopJudge)?;
    let target = raw_path(&root.join(cache))?;
    let item = assessed
        .iter()
        .find(|a| a.candidate.path == target)
        .ok_or("the real assessment did not discover the fixture cache")?;
    let observed = item.input.observed.as_ref().ok_or("missing size facts")?;
    if case == "hard_link" || case == "sparse" {
        assert_eq!(
            observed.reclaim.reclaimable_now.get(),
            0,
            "{case}: {observed:?}"
        );
        assert_eq!(item.decision.verdict(), Verdict::Keep);
    }
    for candidate in &assessed {
        if candidate.decision.verdict() == Verdict::Quarantine {
            assert_eq!(candidate.candidate.path, target);
        }
    }

    // Explicit executor experiment only. Both its source and its store are
    // owned by `tree`; never call the app's real home/store composition here.
    let executor = Executor::new(&root, vec![b"Quarantine".to_vec()]);
    let relative: Vec<_> = cache.split('/').map(|c| c.as_bytes().to_vec()).collect();
    let prepared = executor
        .prepare(&raw_path(&root)?, &relative)
        .map_err(|e| format!("fixture preparation: {e:?}"))?;
    let operation: OperationId = "0190f0d2-6c1e-7a3b-8c4d-5e6f70819201".parse()?;
    let watch = SpaceWatch::parent(&target, &item.candidate.identity().volume)
        .ok_or("fixture watch unavailable")?;
    assert_eq!(executor.quarantine(&prepared, operation, b"{}"), Ok(()));
    assert_eq!(executor.locate(&prepared, operation), Located::InStore);
    assert!(!root.join(cache).exists());
    let held = std::fs::metadata(
        root.join("Quarantine")
            .join(operation.to_string())
            .join("data.bin"),
    )?;
    assert_eq!((held.dev(), held.ino()), (metadata.dev(), metadata.ino()));
    assert_eq!(
        (held.len(), held.blocks()),
        (metadata.len(), metadata.blocks())
    );
    let after_quarantine = space::volume(&root)?.available;
    let finalizing = SpaceWatch::parent(&target, &item.candidate.identity().volume)
        .ok_or("fixture finalize watch unavailable")?;
    let timer = Instant::now();
    assert_eq!(executor.finalize(&prepared, operation), Ok(()));
    assert_eq!(executor.locate(&prepared, operation), Located::Unknown);
    let immediate = finalizing.finish().ok_or("fixture reading unavailable")?;
    let full = watch.finish().ok_or("fixture reading unavailable")?;
    let readings = readings(&root, timer, immediate.after_available)?;
    let preserved = assert_preserved(&tree, &metadata, case == "hard_link")?;
    Ok(json!({
            "case": case,
            "logical_bytes": metadata.len(),
            "allocated_bytes": metadata.blocks() * 512,
            "estimated_reclaim_bytes": observed.reclaim.reclaimable_now.get(),
            "estimate_exact": observed.reclaim.exact,
            "verdict": format!("{:?}", item.decision.verdict()),
            "reasons": &item.reasons,
            "before_fixture_available_bytes": before_fixture,
            "after_fixture_available_bytes": after_fixture,
            "before_quarantine_available_bytes": full.before_available,
            "after_quarantine_available_bytes": after_quarantine,
            "before_finalize_available_bytes": immediate.before_available,
            "after_finalize_available_bytes": immediate.after_available,
            "readings": readings,
            "preserved_files": preserved,
    }))
}

#[test]
#[ignore = "manual fixture experiment: writes up to 256 MiB at a time; waits for disk readings"]
fn compare_estimates_with_observed_space() -> Result<()> {
    let mib: usize = std::env::var("LUMEN_RECLAIM_BENCH_MIB")
        .unwrap_or_else(|_| "64".into())
        .parse()?;
    if !(16..=256).contains(&mib) {
        return Err("LUMEN_RECLAIM_BENCH_MIB must be between 16 and 256".into());
    }
    for (case, cache) in [
        ("cargo_private", "projects/rust/target/debug/deps"),
        ("next_private", "projects/web/.next/cache"),
        ("hard_link", "projects/rust/target/debug/deps"),
        ("sparse", "projects/web/.next/cache"),
    ] {
        let row = experiment(case, cache, mib * 1024 * 1024)?;
        let mut output = std::io::stdout().lock();
        serde_json::to_writer(&mut output, &row)?;
        writeln!(output)?;
    }
    Ok(())
}
