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

#[test]
fn quarantine_refuses_an_unknown_plan() -> Result<(), Box<dyn std::error::Error>> {
    let fx = Fixture::build(&[])?;
    let db = fx.path("index.db")?;
    let out = lumen(&[
        "quarantine".as_ref(),
        "--db".as_ref(),
        db.as_os_str(),
        "--plan".as_ref(),
        "0190f0d2-6c1e-7a3b-8c4d-5e6f708192a3".as_ref(),
        "--confirm".as_ref(),
        "b3:00".as_ref(),
    ])?;
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("does not exist"));
    Ok(())
}

/// The whole command-line path on a fixture home: a wrong hash moves nothing;
/// the right one quarantines the planned folder into a private store with its
/// manifest (or skips it, where in-use visibility keeps it in REVIEW); running
/// the plan again is refused.
#[cfg(target_os = "macos")]
#[test]
fn quarantine_executes_exactly_the_confirmed_plan() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt as _;

    let fx = Fixture::build(&[
        Node::File {
            path: "home/Library/Developer/Xcode/DerivedData/App-abc/Build/out.o".into(),
            contents: vec![7; 4096],
        },
        Node::Dir {
            path: "home/Library/Application Support".into(),
        },
    ])?;
    let home = std::fs::canonicalize(fx.path("home")?)?;
    let db = fx.path("index.db")?;
    let item = home.join("Library/Developer/Xcode/DerivedData/App-abc");
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
    let planned = run(&["plan".as_ref(), "--db".as_ref(), db.as_os_str()])?;
    if !planned.status.success() {
        return Ok(()); // REVIEW here: nothing to plan, covered by the plan tests.
    }
    let text = String::from_utf8_lossy(&planned.stdout).into_owned();
    let field = |flag: &str| {
        text.split_whitespace()
            .skip_while(|word| *word != flag)
            .nth(1)
            .map(str::to_owned)
            .ok_or("missing in plan output")
    };
    let (plan, hash) = (field("--plan")?, field("--confirm")?);
    let quarantine = |confirm: &str| {
        run(&[
            "quarantine".as_ref(),
            "--db".as_ref(),
            db.as_os_str(),
            "--plan".as_ref(),
            plan.as_ref(),
            "--confirm".as_ref(),
            confirm.as_ref(),
        ])
    };
    let wrong = quarantine("b3:0000")?;
    assert!(!wrong.status.success());
    assert!(item.exists(), "a wrong hash moves nothing");
    let done = quarantine(&hash)?;
    assert!(
        done.status.success(),
        "{}",
        String::from_utf8_lossy(&done.stderr)
    );
    let report = String::from_utf8_lossy(&done.stdout);
    let store = home.join("Library/Application Support/Lumen/Quarantine");
    if report.contains("QUARANTINED") {
        assert!(!item.exists(), "moved out of its place");
        let names: Vec<String> = std::fs::read_dir(&store)?
            .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
            .collect();
        assert_eq!(names.len(), 2, "the item and its manifest: {names:?}");
        let moved = names
            .iter()
            .find(|n| !n.ends_with(".manifest.json"))
            .ok_or("item")?;
        assert_eq!(
            std::fs::read(store.join(moved).join("Build/out.o"))?,
            vec![7; 4096],
            "contents intact"
        );
        assert_eq!(
            std::fs::metadata(&store)?.permissions().mode() & 0o777,
            0o700
        );
    } else {
        assert!(report.contains("REVIEW"), "{report}");
        assert!(item.exists());
    }
    let again = quarantine(&hash)?;
    assert!(!again.status.success(), "a plan executes once");
    Ok(())
}
