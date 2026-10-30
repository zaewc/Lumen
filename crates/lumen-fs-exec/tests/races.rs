//! Changes between preparation and execution (roadmap 9.9b, ADR-0016).
//!
//! Each test prepares an item, changes the tree the way a racing process
//! could, and then asks the executor to quarantine it. The executor must
//! refuse with the item untouched at its source, and must never act on what
//! replaced it. The last tests change an item while it is held: restoring is
//! never stricter than finalizing, so what could be removed can be put back.
//! Runs in temporary fixture directories only.

#![cfg(unix)]

use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};

use lumen_application::ports::{
    ExecutionFailure, Located, NotRestored, PreparedItem, QuarantineExecutor as _, Removal,
    RestoreFailure, RestoredItem,
};
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
    // File system timestamps advance in clock ticks (NTFS: the system timer,
    // up to ~15.6 ms; Linux: the coarse clock). A change in the same tick as the
    // directory's last modification is invisible to the time check, so the
    // test waits past one tick; the residual is recorded in the threat model.
    std::thread::sleep(std::time::Duration::from_millis(50));
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

#[test]
fn a_held_directory_that_gains_an_entry_can_be_restored_as_it_could_be_finalized() {
    let (_fx, root, executor) = setup();
    let gain = |operation: OperationId| {
        let held = root.join("Quarantine").join(operation.to_string());
        // Past one timestamp tick (see above), so the change is visible.
        std::thread::sleep(std::time::Duration::from_millis(50));
        std::fs::write(held.join(".DS_Store"), b"written while held")
            .unwrap_or_else(|e| unreachable!("{e}"));
    };
    let item = prepare(&executor, &root, "cache/dir");
    assert_eq!(executor.quarantine(&item, op(9), b"{}"), Ok(()));
    gain(op(9));
    assert_eq!(executor.locate(&item, op(9)), Located::InStore);
    assert_eq!(
        executor.restore(&item, op(9)),
        Ok(RestoredItem {
            parent_replaced: false,
            matches_record: false,
        })
    );
    assert!(root.join("cache/dir/x").exists());
    assert_eq!(
        std::fs::read(root.join("cache/dir/.DS_Store")).unwrap_or_default(),
        b"written while held"
    );
    assert!(executor.forget(&item, op(9)), "restored: sidecar forgotten");
    // The same change does not stop permanent removal either.
    let again = prepare(&executor, &root, "cache/dir");
    assert_eq!(executor.quarantine(&again, op(10), b"{}"), Ok(()));
    gain(op(10));
    assert_eq!(executor.finalize(&again, op(10)), Ok(()));
    assert!(store_is_empty(&root));
}

#[test]
fn a_held_file_that_changes_is_neither_restored_nor_finalized() {
    let (_fx, root, executor) = setup();
    let item = prepare(&executor, &root, "cache/app/data.bin");
    assert_eq!(executor.quarantine(&item, op(11), b"{}"), Ok(()));
    let held = root.join("Quarantine").join(op(11).to_string());
    std::fs::write(&held, b"changed while held").unwrap_or_else(|e| unreachable!("{e}"));
    assert_eq!(
        executor.restore(&item, op(11)),
        Err(RestoreFailure {
            reason: NotRestored::ItemChanged,
            left: ItemLocation::Quarantine,
        })
    );
    assert_eq!(executor.finalize(&item, op(11)), Err(Removal::Nothing));
    assert_eq!(
        std::fs::read(&held).unwrap_or_default(),
        b"changed while held"
    );
    assert!(!root.join("cache/app/data.bin").exists());
}
