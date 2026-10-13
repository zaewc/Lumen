//! The directory-handle enumerator against adversarial trees (roadmap 5.18).

#![cfg(windows)]

use std::ops::ControlFlow;
use std::os::windows::ffi::OsStrExt as _;
use std::path::Path;
use std::process::Command;

use lumen_application::CancelToken;
use lumen_domain::{EntryKind, FilesystemEntry, RawPath};
use lumen_platform_windows::HandleEnumerator;
use lumen_scan::{CoverageBuilder, ScanConfig, ScanItem, scan};

type TestResult = Result<(), Box<dyn std::error::Error>>;

struct Scanned {
    entries: Vec<FilesystemEntry>,
    listed: usize,
    complete: bool,
}

fn raw(path: &Path) -> Result<RawPath, Box<dyn std::error::Error>> {
    let wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    Ok(RawPath::from_windows_wide(&wide)?)
}

fn run(root: &Path) -> Result<Scanned, Box<dyn std::error::Error>> {
    let roots = [raw(root)?];
    let mut coverage = CoverageBuilder::new(&roots);
    let mut entries = Vec::new();
    let mut listed = 0;
    let summary = scan(
        &HandleEnumerator,
        &roots,
        &ScanConfig::default(),
        &CancelToken::new(),
        |item| {
            coverage.observe(&item);
            match item {
                ScanItem::Entry(entry) => entries.push(entry),
                ScanItem::Listed { .. } => listed += 1,
                _ => {}
            }
            ControlFlow::Continue(())
        },
    );
    assert_eq!(summary.stop, None);
    let complete = coverage
        .finish(&summary)
        .roots
        .first()
        .is_some_and(|r| r.coverage.is_complete());
    Ok(Scanned {
        entries,
        listed,
        complete,
    })
}

fn name(entry: &FilesystemEntry) -> String {
    entry
        .path
        .display()
        .rsplit('\\')
        .next()
        .unwrap_or_default()
        .to_owned()
}

fn junction(link: &Path, target: &Path) -> TestResult {
    let made = Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .output()?;
    assert!(
        made.status.success(),
        "{}",
        String::from_utf8_lossy(&made.stderr)
    );
    Ok(())
}

#[test]
fn junction_loops_are_recorded_and_never_followed() -> TestResult {
    let dir = tempfile::tempdir()?;
    let d = dir.path().join("d");
    std::fs::create_dir_all(d.join("sub"))?;
    junction(&d.join("up"), dir.path())?;
    junction(&d.join("sub").join("back"), &d)?;
    let scanned = run(&d)?;
    assert_eq!(scanned.listed, 2, "only d and d\\sub are listed");
    let junctions = scanned
        .entries
        .iter()
        .filter(|e| e.kind == EntryKind::Junction)
        .count();
    assert_eq!(junctions, 2);
    assert!(scanned.complete);
    Ok(())
}

#[test]
fn hostile_names_survive_and_display_escaped() -> TestResult {
    let dir = tempfile::tempdir()?;
    let names = [
        "-rf",
        "invoice\u{202e}gpj.exe",
        "zero\u{200b}width",
        "$(rm -rf ~)",
        "%s%n%x",
        "．．",
        " leading",
        "⟦U+0041⟧",
        &"n".repeat(255),
    ];
    for n in names {
        std::fs::write(dir.path().join(n), b"x")?;
    }
    let scanned = run(dir.path())?;
    for expected in names {
        let wide: Vec<u16> = expected.encode_utf16().collect();
        let found = scanned
            .entries
            .iter()
            .filter(|e| e.path.to_windows_wide().is_some_and(|p| p.ends_with(&wide)))
            .count();
        assert_eq!(found, 1, "{expected:?}");
    }
    for entry in &scanned.entries {
        let shown = entry.path.display();
        assert!(
            !shown.chars().any(|c| c.is_control()
                || matches!(c, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}')),
            "{shown:?}"
        );
    }
    assert!(scanned.complete);
    Ok(())
}

#[test]
fn paths_beyond_max_path_are_listed() -> TestResult {
    let dir = tempfile::tempdir()?;
    let mut deep = dir.path().join("deep");
    for _ in 0..200 {
        deep.push("dd");
    }
    // std adds the \\?\ prefix itself for long paths.
    std::fs::create_dir_all(&deep)?;
    std::fs::write(deep.join("leaf"), b"x")?;
    assert!(
        deep.as_os_str().len() > 260,
        "the tree must exceed MAX_PATH"
    );
    let scanned = run(&dir.path().join("deep"))?;
    assert_eq!(scanned.listed, 201);
    assert!(scanned.entries.iter().any(|e| name(e) == "leaf"));
    assert!(scanned.complete);
    Ok(())
}

#[test]
fn flat_directories_are_listed_completely() -> TestResult {
    let dir = tempfile::tempdir()?;
    for i in 0..20_000 {
        std::fs::write(dir.path().join(format!("{i:06}")), b"")?;
    }
    let scanned = run(dir.path())?;
    assert_eq!(scanned.entries.len(), 20_001);
    assert!(scanned.complete);
    Ok(())
}
