//! Changes between preparation and execution (roadmap 9.9b, ADR-0016).
//!
//! Each test prepares an item, changes the tree the way a racing process
//! could, and then asks the executor to quarantine it. The executor must
//! refuse with the item untouched at its source, and must never act on what
//! replaced it. Runs in temporary fixture directories only.

#![cfg(unix)]

use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};

use lumen_application::ports::{ExecutionFailure, Located, PreparedItem, QuarantineExecutor as _};
use lumen_domain::{ItemLocation, OperationId, QuarantineFailure, RawPath};
use lumen_fs_exec::Executor;
use lumen_testkit::{Fixture, Node};

fn op(n: u8) -> OperationId {
    format!("0190f0d2-6c1e-7a3b-8c4d-5e6f708192{n:02x}")
        .parse()
        .unwrap_or_else(|_| unreachable!())
}

/// A private fixture with `cache/app/data.bin` and `cache/dir/x`, and an
/// executor whose store is `Quarantine` in the same fixture.
fn setup() -> (Fixture, PathBuf, Executor) {
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

fn prepare(executor: &Executor, root: &Path, relative: &str) -> PreparedItem {
    let raw = RawPath::from_unix_bytes(root.as_os_str().as_bytes().to_vec())
        .unwrap_or_else(|_| unreachable!());
    let relative: Vec<Vec<u8>> = relative.split('/').map(|c| c.as_bytes().to_vec()).collect();
    executor
        .prepare(&raw, &relative)
        .unwrap_or_else(|e| unreachable!("{e:?}"))
}

const REFUSED: Result<(), ExecutionFailure> = Err(ExecutionFailure {
    reason: QuarantineFailure::IdentityMismatch,
    left: ItemLocation::Source,
});

/// Nothing entered the store.
fn store_is_empty(root: &Path) -> bool {
    std::fs::read_dir(root.join("Quarantine")).map_or(true, |mut d| d.next().is_none())
}

/// Moves `from` out of the way (fixture-only).
fn rename(from: &Path, to: &Path) {
    #[allow(
        clippy::disallowed_methods,
        reason = "test fixture inside its own temporary directory"
    )]
    std::fs::rename(from, to).unwrap_or_else(|e| unreachable!("{e}"));
}

#[test]
fn a_parent_swapped_for_a_link_is_never_followed() {
    let (_fx, root, executor) = setup();
    let item = prepare(&executor, &root, "cache/app/data.bin");
    rename(&root.join("cache/app"), &root.join("cache/app.old"));
    std::os::unix::fs::symlink(root.join("elsewhere"), root.join("cache/app"))
        .unwrap_or_else(|e| unreachable!("{e}"));
    assert_eq!(executor.quarantine(&item, op(1), b"{}"), REFUSED);
    assert_eq!(
        std::fs::read(root.join("elsewhere/data.bin")).unwrap_or_default(),
        b"not yours"
    );
    assert!(store_is_empty(&root));
}

#[test]
fn a_replaced_parent_is_refused() {
    let (_fx, root, executor) = setup();
    let item = prepare(&executor, &root, "cache/app/data.bin");
    rename(&root.join("cache/app"), &root.join("cache/app.old"));
    std::fs::create_dir(root.join("cache/app")).unwrap_or_else(|e| unreachable!("{e}"));
    std::fs::write(root.join("cache/app/data.bin"), b"0123456789")
        .unwrap_or_else(|e| unreachable!("{e}"));
    assert_eq!(executor.quarantine(&item, op(2), b"{}"), REFUSED);
    assert!(root.join("cache/app/data.bin").exists());
    assert!(store_is_empty(&root));
}

#[test]
fn a_replaced_leaf_is_refused() {
    let (_fx, root, executor) = setup();
    let item = prepare(&executor, &root, "cache/app/data.bin");
    // Keep the old inode alive so the replacement cannot reuse its number.
    rename(
        &root.join("cache/app/data.bin"),
        &root.join("cache/app/old.bin"),
    );
    std::fs::write(root.join("cache/app/data.bin"), b"0123456789")
        .unwrap_or_else(|e| unreachable!("{e}"));
    assert_eq!(executor.quarantine(&item, op(3), b"{}"), REFUSED);
    assert!(root.join("cache/app/data.bin").exists());
    assert!(store_is_empty(&root));
}

#[test]
fn a_leaf_swapped_for_a_link_is_never_followed() {
    let (_fx, root, executor) = setup();
    let item = prepare(&executor, &root, "cache/app/data.bin");
    rename(
        &root.join("cache/app/data.bin"),
        &root.join("cache/app/old.bin"),
    );
    std::os::unix::fs::symlink(
        root.join("elsewhere/data.bin"),
        root.join("cache/app/data.bin"),
    )
    .unwrap_or_else(|e| unreachable!("{e}"));
    assert_eq!(executor.quarantine(&item, op(4), b"{}"), REFUSED);
    assert_eq!(
        std::fs::read(root.join("elsewhere/data.bin")).unwrap_or_default(),
        b"not yours"
    );
    assert!(store_is_empty(&root));
}

#[test]
fn a_new_hard_link_is_refused() {
    let (_fx, root, executor) = setup();
    let item = prepare(&executor, &root, "cache/app/data.bin");
    std::fs::hard_link(
        root.join("cache/app/data.bin"),
        root.join("elsewhere/linked.bin"),
    )
    .unwrap_or_else(|e| unreachable!("{e}"));
    assert_eq!(executor.quarantine(&item, op(5), b"{}"), REFUSED);
    assert!(store_is_empty(&root));
}

#[test]
fn finalizing_a_hard_linked_file_keeps_its_other_name() {
    let (_fx, root, executor) = setup();
    std::fs::hard_link(
        root.join("cache/app/data.bin"),
        root.join("elsewhere/linked.bin"),
    )
    .unwrap_or_else(|e| unreachable!("{e}"));
    let item = prepare(&executor, &root, "cache/app/data.bin");
    assert_eq!(executor.quarantine(&item, op(6), b"{}"), Ok(()));
    assert_eq!(executor.finalize(&item, op(6)), Ok(()));
    let other = std::fs::metadata(root.join("elsewhere/linked.bin"))
        .unwrap_or_else(|e| unreachable!("{e}"));
    assert_eq!(other.nlink(), 1, "only Lumen's name was removed");
    assert_eq!(
        std::fs::read(root.join("elsewhere/linked.bin")).unwrap_or_default(),
        b"0123456789"
    );
}

#[test]
fn a_directory_that_gains_an_entry_is_refused() {
    let (_fx, root, executor) = setup();
    let item = prepare(&executor, &root, "cache/dir");
    std::fs::write(root.join("cache/dir/new"), b"new").unwrap_or_else(|e| unreachable!("{e}"));
    assert_eq!(executor.quarantine(&item, op(7), b"{}"), REFUSED);
    assert!(root.join("cache/dir/new").exists());
    assert!(store_is_empty(&root));
}

#[test]
fn a_read_only_parent_leaves_the_item_at_its_source() {
    if rustix::process::geteuid().is_root() {
        return; // root ignores directory permissions
    }
    let (_fx, root, executor) = setup();
    let item = prepare(&executor, &root, "cache/app/data.bin");
    let set_mode = |mode| {
        #[allow(
            clippy::disallowed_methods,
            reason = "test fixture inside its own temporary directory"
        )]
        std::fs::set_permissions(
            root.join("cache/app"),
            std::fs::Permissions::from_mode(mode),
        )
        .unwrap_or_else(|e| unreachable!("{e}"));
    };
    set_mode(0o555);
    let result = executor.quarantine(&item, op(8), b"{}");
    set_mode(0o755);
    assert_eq!(
        result.map_err(|f| f.left),
        Err(ItemLocation::Source),
        "the move fails before anything changes"
    );
    assert!(root.join("cache/app/data.bin").exists());
    assert_eq!(executor.locate(&item, op(8)), Located::AtSource);
}
