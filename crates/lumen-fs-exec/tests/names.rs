//! Hostile and unusual names through the whole lifecycle (roadmap 9.9).
//!
//! Every name is carried as raw bytes from preparation to restore and
//! finalization; nothing decodes, normalizes or case-folds it. Where the
//! filesystem itself folds case or normalization (APFS does; ext4 does not),
//! twins collide, and the only acceptable outcomes are a correct restore or a
//! refused one: never an overwrite. Runs in temporary fixture directories only.

#![cfg(unix)]

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};

use lumen_application::ports::{
    Located, NotRestored, QuarantineExecutor as _, RestoreFailure, RestoredItem,
};
use lumen_domain::{ItemLocation, OperationId, RawPath};
use lumen_fs_exec::Executor;
use lumen_testkit::{Fixture, Node};

fn op(n: u8) -> OperationId {
    format!("0190f0d2-6c1e-7a3b-8c4d-5e6f708192{n:02x}")
        .parse()
        .unwrap_or_else(|_| unreachable!())
}

/// A private fixture with an empty `cache` directory and an executor whose
/// store is `Quarantine` in the same fixture.
fn setup() -> (Fixture, PathBuf, Executor) {
    let fx = Fixture::build(&[Node::Dir {
        path: "cache".into(),
    }])
    .unwrap_or_else(|e| unreachable!("{e}"));
    let mut perms = std::fs::metadata(fx.root())
        .unwrap_or_else(|e| unreachable!("{e}"))
        .permissions();
    perms.set_mode(0o755);
    #[allow(
        clippy::disallowed_methods,
        reason = "test fixture inside its own temporary directory"
    )]
    std::fs::set_permissions(fx.root(), perms).unwrap_or_else(|e| unreachable!("{e}"));
    let root = std::fs::canonicalize(fx.root()).unwrap_or_else(|e| unreachable!("{e}"));
    let executor = Executor::new(root.clone(), vec![b"Quarantine".to_vec()]);
    (fx, root, executor)
}

fn raw(path: &Path) -> RawPath {
    RawPath::from_unix_bytes(path.as_os_str().as_bytes().to_vec())
        .unwrap_or_else(|_| unreachable!())
}

fn cache(root: &Path, name: &[u8]) -> PathBuf {
    root.join("cache").join(OsStr::from_bytes(name))
}

/// Creates `cache/<name>` holding `contents`; `false` if the filesystem
/// refuses the name (APFS rejects bytes that are not UTF-8).
fn create(root: &Path, name: &[u8], contents: &[u8]) -> bool {
    std::fs::write(cache(root, name), contents).is_ok()
}

fn read(root: &Path, name: &[u8]) -> Vec<u8> {
    std::fs::read(cache(root, name)).unwrap_or_default()
}

const NAMES: &[&[u8]] = &[
    "école".as_bytes(),        // NFC
    "e\u{301}cole".as_bytes(), // NFD
    "invoice\u{202e}fdp.exe".as_bytes(),
    b"line\nbreak",
    b" leading space",
    b"trailing dot.",
    b"-starts-with-dash",
    b"\x01control",
    "日本語のファイル".as_bytes(),
    b"\xff\xfenot-utf8",
];

#[test]
fn every_name_survives_quarantine_restore_and_finalize() {
    let (_fx, root, executor) = setup();
    let mut tested = 0;
    for (n, name) in (1_u8..).zip(NAMES) {
        if !create(&root, name, name) {
            continue;
        }
        tested += 1;
        let relative = [b"cache".to_vec(), name.to_vec()];
        let item = executor
            .prepare(&raw(&root), &relative)
            .unwrap_or_else(|e| unreachable!("{name:?}: {e:?}"));
        assert_eq!(executor.quarantine(&item, op(n), b"{}"), Ok(()), "{name:?}");
        assert_eq!(executor.locate(&item, op(n)), Located::InStore, "{name:?}");
        assert!(!cache(&root, name).exists(), "{name:?} left its source");
        assert_eq!(
            executor.restore(&item, op(n)),
            Ok(RestoredItem {
                parent_replaced: false
            }),
            "{name:?}"
        );
        assert_eq!(read(&root, name), *name, "{name:?} came back intact");
        let again = executor
            .prepare(&raw(&root), &relative)
            .unwrap_or_else(|e| unreachable!("{name:?}: {e:?}"));
        assert_eq!(executor.quarantine(&again, op(n + 100), b"{}"), Ok(()));
        assert_eq!(executor.finalize(&again, op(n + 100)), Ok(()), "{name:?}");
        assert_eq!(executor.locate(&again, op(n + 100)), Located::Unknown);
    }
    assert!(tested >= NAMES.len() - 1, "only non-UTF-8 may be refused");
}

#[test]
fn twins_are_never_overwritten() {
    let twins: &[(&[u8], &[u8])] = &[
        (b"Data.bin", b"data.bin"),
        ("école".as_bytes(), "e\u{301}cole".as_bytes()),
    ];
    for (n, (original, twin)) in (1_u8..).zip(twins) {
        let (_fx, root, executor) = setup();
        assert!(create(&root, original, b"original"));
        let relative = [b"cache".to_vec(), original.to_vec()];
        let item = executor
            .prepare(&raw(&root), &relative)
            .unwrap_or_else(|e| unreachable!("{e:?}"));
        assert_eq!(executor.quarantine(&item, op(n), b"{}"), Ok(()));
        assert!(create(&root, twin, b"newcomer"));
        match executor.restore(&item, op(n)) {
            // The filesystem tells the names apart: both live side by side.
            Ok(_) => assert_eq!(read(&root, original), b"original"),
            // It folds them: the twin keeps the name, the item stays held.
            Err(RestoreFailure {
                reason: NotRestored::NameTaken,
                left: ItemLocation::Quarantine,
            }) => assert_eq!(executor.locate(&item, op(n)), Located::InStore),
            other => unreachable!("{original:?}/{twin:?}: {other:?}"),
        }
        assert_eq!(read(&root, twin), b"newcomer", "{twin:?} never overwritten");
    }
}
