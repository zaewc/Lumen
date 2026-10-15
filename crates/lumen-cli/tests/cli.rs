//! End-to-end tests of the `lumen` binary on fixture trees.

#![cfg(unix)]

use std::process::{Command, Output};

use lumen_testkit::{Fixture, Node};

fn lumen(args: &[&std::ffi::OsStr]) -> std::io::Result<Output> {
    Command::new(env!("CARGO_BIN_EXE_lumen"))
        .args(args)
        .output()
}

fn fixture() -> Result<Fixture, Box<dyn std::error::Error>> {
    Ok(Fixture::build(&[
        Node::File {
            path: "root/a/one".into(),
            contents: vec![1; 1500],
        },
        Node::File {
            path: "root/a/b/two".into(),
            contents: vec![2; 2500],
        },
        Node::HardLink {
            path: "root/a/b/two-again".into(),
            existing: "root/a/b/two".into(),
        },
        Node::Symlink {
            path: "root/escape".into(),
            target: "..".into(),
        },
    ])?)
}

#[test]
fn json_report_counts_files_once_and_modifies_nothing() -> Result<(), Box<dyn std::error::Error>> {
    let fx = fixture()?;
    let root = fx.path("root")?;
    let before = std::fs::read(fx.path("root/a/one")?)?;
    let out = lumen(&["scan".as_ref(), root.as_os_str(), "--json".as_ref()])?;
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&out.stdout)?;
    let r = &report["roots"][0];
    // Two regular files plus the symlink itself (2 bytes: its target ".."); the
    // hard-linked file counts once, and following the link to ".." would have
    // pulled in the whole parent directory.
    assert_eq!(r["files"], 3);
    assert_eq!(r["logical"], "4002", "1500 + 2500 + 2");
    assert_eq!(r["coverage_complete"], true);
    assert_eq!(report["partial"], false);
    // The native macOS enumerator measures private sizes, so the estimate is exact;
    // the portable one cannot see clones and reports a lower bound.
    assert_eq!(r["reclaimable_exact"], cfg!(target_os = "macos"));
    assert_eq!(
        std::fs::read(fx.path("root/a/one")?)?,
        before,
        "scanned files are never modified"
    );
    Ok(())
}

#[test]
fn database_receives_a_completed_scan_and_a_snapshot() -> Result<(), Box<dyn std::error::Error>> {
    let fx = fixture()?;
    let db = fx.path("index.db")?;
    let out = lumen(&[
        "scan".as_ref(),
        fx.path("root")?.as_os_str(),
        "--db".as_ref(),
        db.as_os_str(),
    ])?;
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout)?;
    assert!(text.contains("coverage     complete"), "{text}");
    assert!(text.contains("nothing was modified"));

    let store = lumen_store_sqlite::IndexStore::new(lumen_store_sqlite::open_index(&db)?);
    assert!(
        store.unfinished_scans()?.is_empty(),
        "the scan was marked finished"
    );
    let root = std::path::absolute(fx.path("root")?)?;
    let raw = lumen_domain::RawPath::from_unix_bytes(
        std::os::unix::ffi::OsStrExt::as_bytes(root.as_os_str()).to_vec(),
    )?;
    let history = store.snapshots(&raw)?;
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].files, 3, "two files plus the symlink itself");
    assert!(history[0].complete);
    Ok(())
}

#[test]
fn a_missing_path_is_an_incomplete_scan_not_an_empty_one() -> Result<(), Box<dyn std::error::Error>>
{
    let fx = Fixture::build(&[])?;
    let out = lumen(&[
        "scan".as_ref(),
        fx.path("does-not-exist")?.as_os_str(),
        "--json".as_ref(),
    ])?;
    assert!(out.status.success());
    let report: serde_json::Value = serde_json::from_slice(&out.stdout)?;
    assert_eq!(report["roots"][0]["coverage_complete"], false);
    assert_eq!(report["roots"][0]["failed"], 1);
    Ok(())
}

#[test]
fn usage_errors_exit_with_status_2() -> Result<(), Box<dyn std::error::Error>> {
    for args in [
        &["scan"][..],
        &["scan", "/tmp", "--frobnicate"],
        &["remove", "/tmp"],
    ] {
        let os: Vec<&std::ffi::OsStr> = args.iter().map(std::ffi::OsStr::new).collect();
        let out = lumen(&os)?;
        assert_eq!(out.status.code(), Some(2), "{args:?}");
    }
    Ok(())
}

#[test]
fn assess_needs_a_completed_scan() -> Result<(), Box<dyn std::error::Error>> {
    let fx = Fixture::build(&[])?;
    let db = fx.path("index.db")?;
    let out = lumen(&["assess".as_ref(), "--db".as_ref(), db.as_os_str()])?;
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("no completed scan"));
    Ok(())
}

/// A known regenerable artifact under a fixture home is assessed, explained and
/// left exactly where it is.
#[cfg(target_os = "macos")]
#[test]
fn assess_reports_a_known_artifact_and_moves_nothing() -> Result<(), Box<dyn std::error::Error>> {
    let fx = Fixture::build(&[Node::File {
        path: "home/Library/Developer/Xcode/DerivedData/App-abc/Build/out.o".into(),
        contents: vec![7; 4096],
    }])?;
    // The scanner keeps paths as given; the home must match them exactly.
    let home = std::fs::canonicalize(fx.path("home")?)?;
    let db = fx.path("index.db")?;
    let run = |args: &[&std::ffi::OsStr]| {
        Command::new(env!("CARGO_BIN_EXE_lumen"))
            .args(args)
            .env("HOME", &home)
            .output()
    };
    let scan = run(&[
        "scan".as_ref(),
        home.as_os_str(),
        "--db".as_ref(),
        db.as_os_str(),
    ])?;
    assert!(
        scan.status.success(),
        "{}",
        String::from_utf8_lossy(&scan.stderr)
    );
    let out = run(&[
        "assess".as_ref(),
        "--db".as_ref(),
        db.as_os_str(),
        "--json".as_ref(),
    ])?;
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&out.stdout)?;
    let candidates = report["candidates"].as_array().ok_or("no candidates")?;
    let item = candidates
        .iter()
        .find(|c| {
            c["path"]
                .as_str()
                .is_some_and(|p| p.ends_with("DerivedData/App-abc"))
        })
        .ok_or("the DerivedData folder is a candidate")?;
    assert!(
        matches!(item["verdict"].as_str(), Some("QUARANTINE" | "REVIEW")),
        "{item}"
    );
    assert_ne!(
        item["reasons"].as_array().map(Vec::len),
        Some(0),
        "explained"
    );
    assert_eq!(
        std::fs::read(home.join("Library/Developer/Xcode/DerivedData/App-abc/Build/out.o"))?,
        vec![7; 4096],
        "assessment moves nothing"
    );
    Ok(())
}

#[test]
fn plan_with_nothing_proposed_says_so() -> Result<(), Box<dyn std::error::Error>> {
    let fx = fixture()?;
    let db = fx.path("index.db")?;
    let root = fx.path("root")?;
    let scan = lumen(&[
        "scan".as_ref(),
        root.as_os_str(),
        "--db".as_ref(),
        db.as_os_str(),
    ])?;
    assert!(
        scan.status.success(),
        "{}",
        String::from_utf8_lossy(&scan.stderr)
    );
    let out = lumen(&["plan".as_ref(), "--db".as_ref(), db.as_os_str()])?;
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("nothing to plan"));
    Ok(())
}

/// A proposed artifact is planned with a hash and the exact command to run;
/// nothing moves.
#[cfg(target_os = "macos")]
#[test]
fn plan_prints_the_hash_and_moves_nothing() -> Result<(), Box<dyn std::error::Error>> {
    let fx = Fixture::build(&[Node::File {
        path: "home/Library/Developer/Xcode/DerivedData/App-abc/Build/out.o".into(),
        contents: vec![7; 4096],
    }])?;
    let home = std::fs::canonicalize(fx.path("home")?)?;
    let db = fx.path("index.db")?;
    let run = |args: &[&std::ffi::OsStr]| {
        Command::new(env!("CARGO_BIN_EXE_lumen"))
            .args(args)
            .env("HOME", &home)
            .output()
    };
    let scan = run(&[
        "scan".as_ref(),
        home.as_os_str(),
        "--db".as_ref(),
        db.as_os_str(),
    ])?;
    assert!(
        scan.status.success(),
        "{}",
        String::from_utf8_lossy(&scan.stderr)
    );
    let out = run(&["plan".as_ref(), "--db".as_ref(), db.as_os_str()])?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    if out.status.success() {
        assert!(stdout.contains("DerivedData/App-abc"), "{stdout}");
        assert!(stdout.contains("Plan hash: b3:"), "{stdout}");
        assert!(stdout.contains("--confirm b3:"), "{stdout}");
        assert!(stdout.contains("Nothing has been moved."), "{stdout}");
    } else {
        // Where in-use visibility keeps it in REVIEW, there is nothing to plan.
        assert!(String::from_utf8_lossy(&out.stderr).contains("nothing to plan"));
    }
    assert_eq!(
        std::fs::read(home.join("Library/Developer/Xcode/DerivedData/App-abc/Build/out.o"))?,
        vec![7; 4096],
        "planning moves nothing"
    );
    Ok(())
}
