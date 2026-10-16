//! Hostile names and races through the Windows executor (roadmap 9.10e,
//! ADR-0016, ADR-0033).
//!
//! The Windows counterparts of `names.rs` and `races.rs`: every name is
//! carried as UTF-16 (WTF-8 at the port) through the lifecycle, NTFS folds
//! case but not Unicode normalization, and every change between preparation
//! and execution is refused with the item left where it was. Fixture
//! directories only.

#![cfg(windows)]

use std::ffi::OsString;
use std::os::windows::ffi::{OsStrExt as _, OsStringExt as _};
use std::os::windows::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use lumen_application::ports::{
    ExecutionFailure, Located, NotRestored, PreparedItem, QuarantineExecutor as _, RestoreFailure,
};
use lumen_domain::{ItemLocation, OperationId, QuarantineFailure, RawPath};
use lumen_fs_exec::Executor;
use lumen_testkit::{Fixture, Node};

fn op(n: u8) -> OperationId {
    format!("0190f0d2-6c1e-7a3b-8c4d-5e6f708192{n:02x}")
        .parse()
        .unwrap_or_else(|_| unreachable!())
}

fn setup() -> (Fixture, PathBuf, Executor, RawPath) {
    let fx = Fixture::build(&[
        Node::File {
            path: "cache/app/data.bin".into(),
            contents: b"0123456789".to_vec(),
        },
        Node::File {
            path: "cache/dir/x".into(),
            contents: b"x".to_vec(),
        },
        Node::File {
            path: "elsewhere/data.bin".into(),
            contents: b"not yours".to_vec(),
        },
    ])
    .unwrap_or_else(|e| unreachable!("{e}"));
    // `canonicalize` gives the verbatim `\\?\` form, which also allows names
    // Win32 would otherwise normalize (a trailing dot).
    let root = std::fs::canonicalize(fx.root()).unwrap_or_else(|e| unreachable!("{e}"));
    let executor = Executor::new(root.clone(), vec![b"Quarantine".to_vec()]);
    let units: Vec<u16> = root.as_os_str().encode_wide().collect();
    let raw = RawPath::from_windows_wide(&units).unwrap_or_else(|e| unreachable!("{e}"));
    (fx, root, executor, raw)
}

/// The port's form of a name: WTF-8 bytes.
fn component(name: &[u16]) -> Vec<u8> {
    RawPath::from_windows_wide(name)
        .unwrap_or_else(|e| unreachable!("{e}"))
        .as_bytes()
        .to_vec()
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

fn path_of(root: &Path, dir: &str, name: &[u16]) -> PathBuf {
    root.join(dir).join(OsString::from_wide(name))
}

fn prepare(executor: &Executor, raw: &RawPath, parts: &[Vec<u16>]) -> PreparedItem {
    let relative: Vec<Vec<u8>> = parts.iter().map(|p| component(p)).collect();
    executor
        .prepare(raw, &relative)
        .unwrap_or_else(|e| unreachable!("{e:?}"))
}

#[test]
fn every_name_survives_quarantine_restore_and_finalize() {
    let (_fx, root, executor, raw) = setup();
    let names: Vec<Vec<u16>> = vec![
        wide("école"),        // NFC
        wide("e\u{301}cole"), // NFD: a different name on NTFS
        wide("invoice\u{202e}fdp.exe"),
        wide("日本語のファイル"),
        wide("trailing dot."),
        wide(" leading space"),
        vec![u16::from(b'a'), 0xD800, u16::from(b'z')], // unpaired surrogate
    ];
    let mut tested = 0;
    for (n, name) in (1_u8..).zip(&names) {
        let path = path_of(&root, "cache", name);
        if std::fs::write(&path, b"payload").is_err() {
            continue;
        }
        tested += 1;
        let item = prepare(&executor, &raw, &[wide("cache"), name.clone()]);
        assert_eq!(executor.quarantine(&item, op(n), b"{}"), Ok(()), "{name:?}");
        assert_eq!(executor.locate(&item, op(n)), Located::InStore, "{name:?}");
        assert!(!path.exists(), "{name:?} left its source");
        assert!(!executor.forget(&item, op(n)), "held: {name:?}");
        assert!(executor.restore(&item, op(n)).is_ok(), "{name:?}");
        assert_eq!(
            std::fs::read(&path).unwrap_or_default(),
            b"payload",
            "{name:?}"
        );
        assert!(executor.forget(&item, op(n)), "restored: {name:?}");
        let again = prepare(&executor, &raw, &[wide("cache"), name.clone()]);
        assert_eq!(executor.quarantine(&again, op(n + 100), b"{}"), Ok(()));
        assert_eq!(executor.finalize(&again, op(n + 100)), Ok(()), "{name:?}");
        assert_eq!(executor.locate(&again, op(n + 100)), Located::Unknown);
        assert!(store_is_empty(&root), "nothing left behind: {name:?}");
    }
    assert!(
        tested >= names.len() - 2,
        "only {tested} of {} names",
        names.len()
    );
}

#[test]
fn twins_are_never_overwritten() {
    for (n, (original, twin)) in (1_u8..).zip([
        (wide("Data.bin"), wide("data.bin")),
        (wide("école"), wide("e\u{301}cole")),
    ]) {
        let (_fx, root, executor, raw) = setup();
        std::fs::write(path_of(&root, "cache", &original), b"original")
            .unwrap_or_else(|e| unreachable!("{e}"));
        let item = prepare(&executor, &raw, &[wide("cache"), original.clone()]);
        assert_eq!(executor.quarantine(&item, op(n), b"{}"), Ok(()));
        std::fs::write(path_of(&root, "cache", &twin), b"newcomer")
            .unwrap_or_else(|e| unreachable!("{e}"));
        match executor.restore(&item, op(n)) {
            // NTFS tells the names apart (normalization twins).
            Ok(_) => assert_eq!(
                std::fs::read(path_of(&root, "cache", &original)).unwrap_or_default(),
                b"original"
            ),
            // NTFS folds them (case twins): the item stays held.
            Err(RestoreFailure {
                reason: NotRestored::NameTaken,
                left: ItemLocation::Quarantine,
            }) => assert_eq!(executor.locate(&item, op(n)), Located::InStore),
            other => unreachable!("{other:?}"),
        }
        assert_eq!(
            std::fs::read(path_of(&root, "cache", &twin)).unwrap_or_default(),
            b"newcomer",
            "the twin is never overwritten"
        );
    }
}

const REFUSED: Result<(), ExecutionFailure> = Err(ExecutionFailure {
    reason: QuarantineFailure::IdentityMismatch,
    left: ItemLocation::Source,
});

fn store_is_empty(root: &Path) -> bool {
    std::fs::read_dir(root.join("Quarantine")).map_or(true, |mut d| d.next().is_none())
}

fn rename(from: &Path, to: &Path) {
    #[allow(
        clippy::disallowed_methods,
        reason = "test fixture inside its own temporary directory"
    )]
    std::fs::rename(from, to).unwrap_or_else(|e| unreachable!("{e}"));
}

fn data(executor: &Executor, raw: &RawPath) -> PreparedItem {
    prepare(
        executor,
        raw,
        &[wide("cache"), wide("app"), wide("data.bin")],
    )
}

#[test]
fn a_parent_swapped_for_a_junction_is_never_followed() {
    let (_fx, root, executor, raw) = setup();
    let item = data(&executor, &raw);
    rename(&root.join("cache/app"), &root.join("cache/app.old"));
    let status = Command::new("cmd")
        .arg("/C")
        .arg("mklink")
        .arg("/J")
        .arg(root.join("cache").join("app"))
        .arg(root.join("elsewhere"))
        .status()
        .unwrap_or_else(|e| unreachable!("{e}"));
    assert!(status.success());
    assert_eq!(executor.quarantine(&item, op(1), b"{}"), REFUSED);
    assert_eq!(
        std::fs::read(root.join("elsewhere/data.bin")).unwrap_or_default(),
        b"not yours"
    );
    assert!(store_is_empty(&root));
}

#[test]
fn a_replaced_leaf_or_a_new_hard_link_is_refused() {
    let (_fx, root, executor, raw) = setup();
    let item = data(&executor, &raw);
    rename(
        &root.join("cache/app/data.bin"),
        &root.join("cache/app/old.bin"),
    );
    std::fs::write(root.join("cache/app/data.bin"), b"0123456789")
        .unwrap_or_else(|e| unreachable!("{e}"));
    assert_eq!(executor.quarantine(&item, op(2), b"{}"), REFUSED);
    let item = data(&executor, &raw);
    std::fs::hard_link(
        root.join("cache/app/data.bin"),
        root.join("elsewhere/linked.bin"),
    )
    .unwrap_or_else(|e| unreachable!("{e}"));
    assert_eq!(executor.quarantine(&item, op(3), b"{}"), REFUSED);
    assert!(root.join("cache/app/data.bin").exists());
    assert!(store_is_empty(&root));
}

#[test]
fn a_directory_that_gains_an_entry_is_refused() {
    let (_fx, root, executor, raw) = setup();
    let item = prepare(&executor, &raw, &[wide("cache"), wide("dir")]);
    // File system timestamps advance in clock ticks (NTFS: the system timer,
    // up to ~15.6 ms; Linux: the coarse clock). A change in the same tick as the
    // directory's last modification is invisible to the time check, so the
    // test waits past one tick; the residual is recorded in the threat model.
    std::thread::sleep(std::time::Duration::from_millis(50));
    std::fs::write(root.join("cache/dir/new"), b"new").unwrap_or_else(|e| unreachable!("{e}"));
    assert_eq!(executor.quarantine(&item, op(4), b"{}"), REFUSED);
    assert!(root.join("cache/dir/new").exists());
    assert!(store_is_empty(&root));
}

#[test]
fn a_file_held_open_without_delete_sharing_stays() {
    use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;

    let (_fx, root, executor, raw) = setup();
    let item = data(&executor, &raw);
    let holder = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(root.join("cache/app/data.bin"))
        .unwrap_or_else(|e| unreachable!("{e}"));
    let result = executor.quarantine(&item, op(5), b"{}");
    drop(holder);
    assert_eq!(result.map_err(|f| f.left), Err(ItemLocation::Source));
    assert!(root.join("cache/app/data.bin").exists());
    assert!(store_is_empty(&root));
}
