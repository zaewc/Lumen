//! End-to-end tests of the `lumen` binary on Windows.

#![cfg(windows)]

use std::os::windows::ffi::OsStrExt as _;
use std::process::{Command, Output};

fn lumen(args: &[&std::ffi::OsStr]) -> std::io::Result<Output> {
    Command::new(env!("CARGO_BIN_EXE_lumen"))
        .args(args)
        .output()
}

#[test]
fn scans_a_tree_and_records_a_snapshot() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let root = dir.path().join("root");
    std::fs::create_dir_all(root.join("a").join("b"))?;
    std::fs::write(root.join("a").join("one"), vec![1; 1500])?;
    std::fs::write(root.join("a").join("b").join("two"), vec![2; 2500])?;
    let before = std::fs::read(root.join("a").join("one"))?;
    let db = dir.path().join("index.db");

    let out = lumen(&[
        "scan".as_ref(),
        root.as_os_str(),
        "--json".as_ref(),
        "--db".as_ref(),
        db.as_os_str(),
    ])?;
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&out.stdout)?;
    let r = &report["roots"][0];
    assert_eq!(r["files"], 2);
    assert_eq!(r["logical"], "4000");
    assert_eq!(r["coverage_complete"], true);
    // Directory records carry no link count, so Windows reports a lower bound.
    assert_eq!(r["reclaimable_exact"], false);
    assert_eq!(
        std::fs::read(root.join("a").join("one"))?,
        before,
        "scanned files are never modified"
    );

    let store = lumen_store_sqlite::IndexStore::new(lumen_store_sqlite::open_index(&db)?);
    assert_eq!(
        store.unfinished_scans()?.len(),
        0,
        "the scan was marked finished"
    );
    let wide: Vec<u16> = std::path::absolute(&root)?
        .as_os_str()
        .encode_wide()
        .collect();
    let history = store.snapshots(&lumen_domain::RawPath::from_windows_wide(&wide)?)?;
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].files, 2);
    Ok(())
}

#[test]
fn a_missing_path_is_an_incomplete_scan_not_an_empty_one() -> Result<(), Box<dyn std::error::Error>>
{
    let dir = tempfile::tempdir()?;
    let out = lumen(&[
        "scan".as_ref(),
        dir.path().join("does-not-exist").as_os_str(),
        "--json".as_ref(),
    ])?;
    assert!(out.status.success());
    let report: serde_json::Value = serde_json::from_slice(&out.stdout)?;
    assert_eq!(report["roots"][0]["coverage_complete"], false);
    assert_eq!(report["roots"][0]["failed"], 1);
    Ok(())
}
