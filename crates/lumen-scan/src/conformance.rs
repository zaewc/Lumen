//! Adversarial conformance checks for any [`DirEnumerator`] (roadmap 5.18).
//!
//! Each check builds a hostile tree in a temporary [`Fixture`], scans it through
//! the real scheduler, and verifies what a safe scanner must do:
//!
//! - **link loops**: links to `.`, `..`, `/` and to each other are recorded and
//!   never followed, so the scan ends and lists only real directories;
//! - **hostile names**: control characters, bidirectional overrides, shell and
//!   format metacharacters, full-width dots and 255-byte names survive byte for
//!   byte, and never reach a display string unescaped;
//! - **flat directories**: tens of thousands of entries arrive complete;
//! - **deep trees**: hundreds of nested levels are listed without recursion;
//! - **special files**: a named pipe is reported, not opened (opening one for
//!   reading would block the scanner forever).
//!
//! Enabled by the `conformance` feature, for enumerator test suites only.

use std::ops::ControlFlow;
use std::os::unix::ffi::OsStrExt as _;

use lumen_application::CancelToken;
use lumen_application::ports::DirEnumerator;
use lumen_domain::{EntryKind, FilesystemEntry, RawPath};
use lumen_testkit::{Fixture, Node};

use crate::{CoverageBuilder, ScanConfig, ScanItem, ScanSummary, scan};

/// A conformance failure, described for the test log.
pub type Failure = String;

struct Scanned {
    entries: Vec<FilesystemEntry>,
    listed: Vec<RawPath>,
    summary: ScanSummary,
    complete: bool,
}

fn root_of(fx: &Fixture, relative: &str) -> Result<RawPath, Failure> {
    let path = fx.path(relative).map_err(|e| e.to_string())?;
    RawPath::from_unix_bytes(path.as_os_str().as_bytes().to_vec()).map_err(|e| e.to_string())
}

fn run<E: DirEnumerator>(enumerator: &E, root: &RawPath) -> Scanned {
    let roots = std::slice::from_ref(root);
    let mut coverage = CoverageBuilder::new(roots);
    let mut entries = Vec::new();
    let mut listed = Vec::new();
    let summary = scan(
        enumerator,
        roots,
        &ScanConfig::default(),
        &CancelToken::new(),
        |item| {
            coverage.observe(&item);
            match item {
                ScanItem::Entry(entry) => entries.push(entry),
                ScanItem::Listed { path, .. } => listed.push(path),
                _ => {}
            }
            ControlFlow::Continue(())
        },
    );
    let complete = coverage
        .finish(&summary)
        .roots
        .first()
        .is_some_and(|r| r.coverage.is_complete());
    Scanned {
        entries,
        listed,
        summary,
        complete,
    }
}

fn ensure(condition: bool, failure: impl FnOnce() -> Failure) -> Result<(), Failure> {
    if condition { Ok(()) } else { Err(failure()) }
}

fn name(entry: &FilesystemEntry) -> &[u8] {
    let bytes = entry.path.as_bytes();
    bytes.rsplit(|b| *b == b'/').next().unwrap_or(bytes)
}

/// Links to `.`, `..`, `/`, a two-link cycle and a link back up a subtree are
/// all recorded as symlinks and none is followed.
///
/// # Errors
///
/// Describes the first violated expectation.
pub fn link_loops<E: DirEnumerator>(enumerator: &E) -> Result<(), Failure> {
    let link = |path: &str, target: &str| Node::Symlink {
        path: path.into(),
        target: target.into(),
    };
    let fx = Fixture::build(&[
        Node::Dir {
            path: "d/sub".into(),
        },
        link("d/self", "."),
        link("d/up", ".."),
        link("d/root", "/"),
        link("d/a", "b"),
        link("d/b", "a"),
        link("d/sub/back", "../.."),
    ])
    .map_err(|e| e.to_string())?;
    let root = root_of(&fx, "d")?;
    let scanned = run(enumerator, &root);
    ensure(scanned.summary.stop.is_none(), || {
        "scan stopped early".into()
    })?;
    ensure(scanned.listed.len() == 2, || {
        format!(
            "listed {} directories, expected d and d/sub",
            scanned.listed.len()
        )
    })?;
    let links = scanned
        .entries
        .iter()
        .filter(|e| e.kind == EntryKind::Symlink)
        .count();
    ensure(links == 6, || {
        format!("{links} symlinks reported, expected 6")
    })?;
    ensure(scanned.entries.len() == 8, || {
        format!("{} entries, expected 8", scanned.entries.len())
    })?;
    ensure(scanned.complete, || "coverage incomplete".into())
}

/// Names a scanner must carry byte for byte and render only escaped.
pub const HOSTILE_NAMES: &[&str] = &[
    "-rf",
    "line\nbreak",
    "tab\there",
    "bell\u{7}",
    "esc\u{1b}[31mred",
    "invoice\u{202e}gpj.exe",
    "zero\u{200b}width",
    "$(rm -rf ~)",
    "`id`",
    "%s%n%x",
    "*?[]<>|",
    "．．",
    " leading",
    "trailing ",
    "⟦U+0041⟧",
];

/// Every name in [`HOSTILE_NAMES`], plus a 255-byte name, is reported exactly
/// once with its exact bytes, and its display string contains no control or
/// bidirectional formatting character.
///
/// # Errors
///
/// Describes the first violated expectation.
pub fn hostile_names<E: DirEnumerator>(enumerator: &E) -> Result<(), Failure> {
    let longest = "n".repeat(255);
    let names: Vec<&str> = HOSTILE_NAMES
        .iter()
        .copied()
        .chain([longest.as_str()])
        .collect();
    let nodes: Vec<Node> = names
        .iter()
        .map(|n| Node::File {
            path: format!("names/{n}"),
            contents: b"x".to_vec(),
        })
        .collect();
    let fx = Fixture::build(&nodes).map_err(|e| e.to_string())?;
    let scanned = run(enumerator, &root_of(&fx, "names")?);
    for expected in &names {
        let found = scanned
            .entries
            .iter()
            .filter(|e| name(e) == expected.as_bytes())
            .count();
        ensure(found == 1, || {
            format!("{expected:?} reported {found} times")
        })?;
    }
    for entry in &scanned.entries {
        let shown = entry.path.display();
        ensure(
            !shown.chars().any(|c| {
                c.is_control() || matches!(c, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}')
            }),
            || format!("unescaped display string {shown:?}"),
        )?;
    }
    ensure(scanned.complete, || "coverage incomplete".into())
}

/// A directory with `count` entries is listed completely.
///
/// # Errors
///
/// Describes the first violated expectation.
pub fn flat_directory<E: DirEnumerator>(enumerator: &E, count: usize) -> Result<(), Failure> {
    let nodes: Vec<Node> = (0..count)
        .map(|i| Node::File {
            path: format!("flat/{i:06}"),
            contents: Vec::new(),
        })
        .collect();
    let fx = Fixture::build(&nodes).map_err(|e| e.to_string())?;
    let scanned = run(enumerator, &root_of(&fx, "flat")?);
    ensure(scanned.entries.len() == count + 1, || {
        format!("{} entries, expected {}", scanned.entries.len(), count + 1)
    })?;
    ensure(scanned.complete, || "coverage incomplete".into())
}

/// A chain of `depth` nested directories is listed to the bottom.
///
/// # Errors
///
/// Describes the first violated expectation.
pub fn deep_tree<E: DirEnumerator>(enumerator: &E, depth: usize) -> Result<(), Failure> {
    let path = std::iter::repeat_n("d", depth)
        .collect::<Vec<_>>()
        .join("/");
    let fx = Fixture::build(&[Node::File {
        path: format!("deep/{path}/leaf"),
        contents: b"x".to_vec(),
    }])
    .map_err(|e| e.to_string())?;
    let scanned = run(enumerator, &root_of(&fx, "deep")?);
    ensure(scanned.listed.len() == depth + 1, || {
        format!(
            "listed {} directories, expected {}",
            scanned.listed.len(),
            depth + 1
        )
    })?;
    ensure(scanned.entries.iter().any(|e| name(e) == b"leaf"), || {
        "the leaf file was not reached".into()
    })?;
    ensure(scanned.complete, || "coverage incomplete".into())
}

/// A named pipe is reported as a special file; the scan finishing at all proves
/// it was not opened.
///
/// # Errors
///
/// Describes the first violated expectation.
pub fn special_files<E: DirEnumerator>(enumerator: &E) -> Result<(), Failure> {
    let fx = Fixture::build(&[Node::Fifo {
        path: "special/pipe".into(),
    }])
    .map_err(|e| e.to_string())?;
    let scanned = run(enumerator, &root_of(&fx, "special")?);
    let pipe = scanned.entries.iter().find(|e| name(e) == b"pipe");
    ensure(pipe.is_some_and(|e| e.kind == EntryKind::Special), || {
        format!("pipe reported as {:?}", pipe.map(|e| e.kind))
    })?;
    ensure(scanned.complete, || "coverage incomplete".into())
}
