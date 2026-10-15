//! Moving items into a Windows store and back (roadmap 9.10c, ADR-0015,
//! ADR-0033).
//!
//! Both directions are one `FileRenameInformation` (`NtSetInformationFile`) on
//! the verified item's own handle
//! with `ReplaceIfExists = FALSE` and the destination directory's handle: never
//! an overwrite, never a copy, never another volume. Windows refuses to rename
//! an item another program holds open (or, for a directory, anything inside
//! it); that is reported as in use. After the rename the source name must be
//! gone and the item must still be exactly what was planned.
//!
//! Directory entries are not flushed one by one on Windows (ADR-0033); the
//! durable journal and startup reconciliation cover what NTFS has not logged.

use std::io;

use windows_sys::Win32::Foundation::{
    ERROR_ALREADY_EXISTS, ERROR_FILE_EXISTS, ERROR_FILE_NOT_FOUND, ERROR_NOT_SAME_DEVICE,
    ERROR_PATH_NOT_FOUND, ERROR_SHARING_VIOLATION,
};

use crate::location::Left;
use crate::win_store::WinStore;
use crate::win_sys::{self, Handle, Want};
use crate::win_verify::{
    WinExpected, WinMismatch, WinPlanItem, WinVerified, check_leaf, identity, is_reparse, walk,
};

/// Why a move or restore failed, and where it left the item.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum WinMoveError {
    /// A name is empty, `.`, `..` or contains a separator or NUL.
    #[error("invalid name")]
    InvalidName,
    /// The item in the store is missing or no longer what was quarantined.
    #[error("the quarantined item is missing or changed: {0}")]
    NotInStore(WinMismatch),
    /// The original location cannot be reached safely.
    #[error("the original location cannot be reached: {0}")]
    ParentUnreachable(WinMismatch),
    /// Something already has the destination name; nothing was replaced.
    #[error("the name is already taken")]
    NameTaken,
    /// The destination is on another volume.
    #[error("the destination is on another volume")]
    OtherVolume,
    /// Another program has the item (or something inside it) open.
    #[error("the item is in use")]
    InUse,
    /// The rename failed for another reason.
    #[error("the rename failed: OS error {0}")]
    Rename(i32),
    /// After the rename, the old name still exists.
    #[error("the old name still exists after the rename")]
    SourceRemains,
    /// After the rename, the item is not what was planned.
    #[error("the moved item does not match: {0}")]
    Unverified(WinMismatch),
}

/// Where a failed move left the item, from its point of view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Into the store.
    Quarantine,
    /// Back to its source.
    Restore,
}

impl WinMoveError {
    /// Where the failure left the item, for a move in `direction`.
    pub const fn left(&self, direction: Direction) -> Left {
        let renamed = matches!(self, Self::SourceRemains | Self::Unverified(_));
        match (direction, renamed) {
            (Direction::Quarantine, false) | (Direction::Restore, true) => Left::AtSource,
            (Direction::Quarantine, true) | (Direction::Restore, false) => Left::InStore,
        }
    }
}

/// A completed restore.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WinRestored {
    /// The original parent was replaced by another directory at its path.
    pub parent_replaced: bool,
}

fn code(error: &io::Error) -> i32 {
    error.raw_os_error().unwrap_or(0)
}

fn rename_error(error: &io::Error) -> WinMoveError {
    match error.raw_os_error().and_then(|c| u32::try_from(c).ok()) {
        Some(ERROR_ALREADY_EXISTS | ERROR_FILE_EXISTS) => WinMoveError::NameTaken,
        Some(ERROR_NOT_SAME_DEVICE) => WinMoveError::OtherVolume,
        Some(ERROR_SHARING_VIOLATION) => WinMoveError::InUse,
        _ => WinMoveError::Rename(code(error)),
    }
}

fn gone(error: &io::Error) -> bool {
    matches!(
        error.raw_os_error().and_then(|c| u32::try_from(c).ok()),
        Some(ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND)
    )
}

/// After a rename: the old name is gone and the item, reopened by its new
/// name, is still what was planned.
fn check_after(
    from: &Handle,
    old: &[u16],
    to: &Handle,
    new: &[u16],
    expected: &WinExpected,
) -> Result<(), WinMoveError> {
    let want = Want::Movable {
        directory: expected.directory,
    };
    match win_sys::open_at(from, old, want) {
        Err(error) if gone(&error) => {}
        Ok(_) | Err(_) => return Err(WinMoveError::SourceRemains),
    }
    let moved = win_sys::open_at(to, new, want)
        .and_then(|handle| win_sys::facts(&handle))
        .map_err(|error| {
            WinMoveError::Unverified(WinMismatch::Unreachable {
                what: "the moved item",
                code: code(&error),
            })
        })?;
    check_leaf(&moved, expected).map_err(WinMoveError::Unverified)
}

/// Renames the verified item into `store` as `name`, then checks it.
///
/// # Errors
///
/// A [`WinMoveError`]; [`WinMoveError::left`] says where the item is.
pub fn move_into(
    verified: &WinVerified,
    store: &WinStore,
    name: &[u16],
    expected: &WinExpected,
) -> Result<(), WinMoveError> {
    if !win_sys::plain_name(name) {
        return Err(WinMoveError::InvalidName);
    }
    win_sys::rename_into(&verified.leaf, &store.dir, name).map_err(|e| rename_error(&e))?;
    check_after(&verified.parent, &verified.name, &store.dir, name, expected)
}

/// Moves `name` in `store` back to where `item` was.
///
/// # Errors
///
/// A [`WinMoveError`]; [`WinMoveError::left`] says where the item is.
pub fn restore(
    store: &WinStore,
    name: &[u16],
    item: &WinPlanItem,
) -> Result<WinRestored, WinMoveError> {
    if !win_sys::plain_name(name) || !win_sys::plain_name(&item.leaf) {
        return Err(WinMoveError::InvalidName);
    }
    let want = Want::Movable {
        directory: item.expected.directory,
    };
    let held = win_sys::open_at(&store.dir, name, want).map_err(|error| {
        WinMoveError::NotInStore(WinMismatch::Unreachable {
            what: "the quarantined item",
            code: code(&error),
        })
    })?;
    let facts = win_sys::facts(&held).map_err(|error| {
        WinMoveError::NotInStore(WinMismatch::Unreachable {
            what: "the quarantined item",
            code: code(&error),
        })
    })?;
    check_leaf(&facts, &item.expected).map_err(WinMoveError::NotInStore)?;
    let (_root, parent) =
        walk(&item.root, &item.components).map_err(WinMoveError::ParentUnreachable)?;
    let parent_facts = win_sys::facts(&parent).map_err(|error| {
        WinMoveError::ParentUnreachable(WinMismatch::Unreachable {
            what: "the parent",
            code: code(&error),
        })
    })?;
    if is_reparse(&parent_facts) {
        return Err(WinMoveError::ParentUnreachable(WinMismatch::ReparsePoint(
            "the parent",
        )));
    }
    if parent_facts.volume != store.identity.volume {
        return Err(WinMoveError::ParentUnreachable(WinMismatch::CrossesVolume));
    }
    let parent_replaced = identity(&parent_facts) != item.parent_identity;
    win_sys::rename_into(&held, &parent, &item.leaf).map_err(|e| rename_error(&e))?;
    check_after(&store.dir, name, &parent, &item.leaf, &item.expected)?;
    Ok(WinRestored { parent_replaced })
}

/// Writes `bytes` as the manifest sidecar of `item` in `store`, never over an
/// existing one.
///
/// # Errors
///
/// The OS error; `AlreadyExists` if a sidecar is there.
pub fn write_sidecar(store: &WinStore, item: &[u16], bytes: &[u8]) -> io::Result<()> {
    let mut name = item.to_vec();
    name.extend(".manifest.json".encode_utf16());
    win_sys::create_file_at(&store.dir, &name, bytes)
}

#[cfg(test)]
mod tests {
    use std::os::windows::ffi::OsStrExt as _;
    use std::path::PathBuf;

    use lumen_testkit::{Fixture, Node};

    use super::*;
    use crate::win_store::open_store;
    use crate::win_verify::{observe, verify};

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    fn setup() -> (Fixture, PathBuf, Vec<u16>, WinStore) {
        let fx = Fixture::build(&[
            Node::File {
                path: "cache/app/data.bin".into(),
                contents: b"0123456789".to_vec(),
            },
            Node::File {
                path: "cache/dir/x".into(),
                contents: b"x".to_vec(),
            },
        ])
        .unwrap_or_else(|e| unreachable!("{e}"));
        let root = std::fs::canonicalize(fx.root()).unwrap_or_else(|e| unreachable!("{e}"));
        let units: Vec<u16> = root.as_os_str().encode_wide().collect();
        let item = observe(&units, &[wide("cache"), wide("app"), wide("data.bin")])
            .unwrap_or_else(|e| unreachable!("{e}"));
        let store = open_store(
            &units,
            &[&wide("Quarantine")],
            item.expected.identity.volume,
        )
        .unwrap_or_else(|e| unreachable!("{e}"));
        (fx, root, units, store)
    }

    fn plan(units: &[u16], relative: &str) -> WinPlanItem {
        let parts: Vec<Vec<u16>> = relative.split('/').map(wide).collect();
        observe(units, &parts).unwrap_or_else(|e| unreachable!("{e}"))
    }

    #[test]
    fn files_and_directories_go_into_the_store_and_come_back() {
        let (_fx, root, units, store) = setup();
        for (relative, name) in [("cache/app/data.bin", "item-1"), ("cache/dir", "item-2")] {
            let item = plan(&units, relative);
            let verified = verify(&item).unwrap_or_else(|e| unreachable!("{e}"));
            assert_eq!(
                move_into(&verified, &store, &wide(name), &item.expected),
                Ok(())
            );
            drop(verified);
            assert!(!root.join(relative).exists(), "{relative} left its source");
            assert!(root.join("Quarantine").join(name).exists());
            assert_eq!(
                restore(&store, &wide(name), &item),
                Ok(WinRestored {
                    parent_replaced: false
                })
            );
            assert!(root.join(relative).exists(), "{relative} came back");
        }
    }

    #[test]
    fn nothing_is_ever_overwritten() {
        let (_fx, root, units, store) = setup();
        std::fs::write(root.join("Quarantine/item-1"), b"keep me")
            .unwrap_or_else(|e| unreachable!("{e}"));
        let item = plan(&units, "cache/app/data.bin");
        let verified = verify(&item).unwrap_or_else(|e| unreachable!("{e}"));
        let result = move_into(&verified, &store, &wide("item-1"), &item.expected);
        assert_eq!(result, Err(WinMoveError::NameTaken));
        assert_eq!(
            result.err().map(|e| e.left(Direction::Quarantine)),
            Some(Left::AtSource)
        );
        drop(verified);
        assert_eq!(
            std::fs::read(root.join("Quarantine/item-1")).unwrap_or_default(),
            b"keep me"
        );
        assert!(root.join("cache/app/data.bin").exists());
        // And back: a newcomer at the source name is never replaced either.
        let verified = verify(&item).unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(
            move_into(&verified, &store, &wide("item-2"), &item.expected),
            Ok(())
        );
        drop(verified);
        std::fs::write(root.join("cache/app/data.bin"), b"newcomer")
            .unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(
            restore(&store, &wide("item-2"), &item),
            Err(WinMoveError::NameTaken)
        );
        assert_eq!(
            std::fs::read(root.join("cache/app/data.bin")).unwrap_or_default(),
            b"newcomer"
        );
    }

    #[test]
    fn an_item_held_open_without_delete_sharing_is_never_moved() {
        use std::os::windows::fs::OpenOptionsExt as _;

        use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;

        let (_fx, root, units, _store) = setup();
        let item = plan(&units, "cache/app/data.bin");
        // Another program holds the file open and does not share deletion.
        let holder = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .open(root.join("cache/app/data.bin"))
            .unwrap_or_else(|e| unreachable!("{e}"));
        let result = verify(&item);
        drop(holder);
        assert!(
            matches!(
                result,
                Err(WinMismatch::Unreachable {
                    what: "the item",
                    code: 32
                })
            ),
            "a sharing violation: {result:?}"
        );
        assert!(root.join("cache/app/data.bin").exists());
    }

    #[test]
    fn sidecars_are_written_once() {
        let (_fx, root, _units, store) = setup();
        assert!(write_sidecar(&store, &wide("item-1"), b"{}").is_ok());
        assert_eq!(
            std::fs::read(root.join("Quarantine/item-1.manifest.json")).unwrap_or_default(),
            b"{}"
        );
        assert!(write_sidecar(&store, &wide("item-1"), b"second").is_err());
        assert_eq!(
            std::fs::read(root.join("Quarantine/item-1.manifest.json")).unwrap_or_default(),
            b"{}"
        );
    }
}
