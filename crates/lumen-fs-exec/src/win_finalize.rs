//! Finalizing a quarantined item on Windows: the only permanent removal
//! (roadmap 9.10c2, ADR-0015, ADR-0033).
//!
//! As on Unix: only inside a store, only by handle; a file must still match
//! its record in full, a directory by identity and kind (so an interrupted
//! finalization can finish). A directory is checked completely before
//! anything is removed: every directory inside is on the store's volume, at
//! most [`MAX_DEPTH`] deep, and readable. A reparse point (junction, symbolic
//! link) is never entered: it is removed as itself. Then everything is removed
//! bottom-up, each entry opened relative to its directory with
//! `FILE_OPEN_REPARSE_POINT` and deleted through its own handle. The sidecar
//! goes last.

use std::io;

use windows_sys::Win32::Foundation::{
    ERROR_ALREADY_EXISTS, ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND,
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
};

use crate::location::Removed;
use crate::win_store::WinStore;
use crate::win_sys::{self, Handle, Want};
use crate::win_verify::{WinExpected, WinMismatch, check_leaf, identity, is_reparse};

/// The deepest directory nesting finalization removes.
pub const MAX_DEPTH: usize = 256;

/// Why finalization failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum WinFinalizeError {
    /// The item name is empty, `.`, `..` or contains a separator or NUL.
    #[error("invalid name")]
    InvalidName,
    /// The item is missing from the store or is not what was quarantined.
    #[error("the quarantined item is missing or changed: {0}")]
    NotInStore(WinMismatch),
    /// A directory inside the item is on another volume.
    #[error("the item contains another volume")]
    CrossesVolume,
    /// The item is nested deeper than [`MAX_DEPTH`].
    #[error("the item is nested too deeply")]
    TooDeep,
    /// Part of the item could not be read while checking it.
    #[error("the item could not be checked: OS error {0}")]
    Unreadable(i32),
    /// A removal failed part way.
    #[error("the item could not be removed: OS error {0}")]
    Removal(i32),
    /// The item is gone but its sidecar could not be removed.
    #[error("the sidecar could not be removed: OS error {0}")]
    Sidecar(i32),
}

impl WinFinalizeError {
    /// How much was removed before the failure.
    pub const fn removed(&self) -> Removed {
        match self {
            Self::InvalidName
            | Self::NotInStore(_)
            | Self::CrossesVolume
            | Self::TooDeep
            | Self::Unreadable(_) => Removed::Nothing,
            Self::Removal(_) => Removed::Partly,
            Self::Sidecar(_) => Removed::Item,
        }
    }
}

fn code(error: &io::Error) -> i32 {
    error.raw_os_error().unwrap_or(0)
}

fn gone(error: &io::Error) -> bool {
    matches!(
        error.raw_os_error().and_then(|c| u32::try_from(c).ok()),
        Some(ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND)
    )
}

/// A directory entry to descend into: a real directory, never a reparse point.
const fn is_real_directory(attributes: u32) -> bool {
    attributes & FILE_ATTRIBUTE_DIRECTORY != 0 && attributes & FILE_ATTRIBUTE_REPARSE_POINT == 0
}

/// Opens the directory `name` in `dir` for removal, refusing other volumes
/// and anything that turned out not to be a plain directory.
fn open_dir(dir: &Handle, name: &[u16], volume: u64) -> Result<Handle, WinFinalizeError> {
    let child = win_sys::open_at(dir, name, Want::Remove { directory: true })
        .map_err(|e| WinFinalizeError::Unreadable(code(&e)))?;
    let facts = win_sys::facts(&child).map_err(|e| WinFinalizeError::Unreadable(code(&e)))?;
    if is_reparse(&facts) || !facts.directory {
        return Err(WinFinalizeError::Unreadable(0));
    }
    if facts.volume != volume {
        return Err(WinFinalizeError::CrossesVolume);
    }
    Ok(child)
}

/// Checks the tree below `dir` without changing anything.
fn check_tree(dir: &Handle, volume: u64, depth: usize) -> Result<(), WinFinalizeError> {
    if depth > MAX_DEPTH {
        return Err(WinFinalizeError::TooDeep);
    }
    for entry in win_sys::entries(dir).map_err(|e| WinFinalizeError::Unreadable(code(&e)))? {
        if is_real_directory(entry.attributes) {
            check_tree(&open_dir(dir, &entry.name, volume)?, volume, depth + 1)?;
        }
    }
    Ok(())
}

/// Removes everything below `dir`, bottom-up.
fn remove_tree(dir: &Handle, volume: u64) -> Result<(), WinFinalizeError> {
    let partly = |e: io::Error| WinFinalizeError::Removal(code(&e));
    for entry in win_sys::entries(dir).map_err(partly)? {
        if is_real_directory(entry.attributes) {
            let child = open_dir(dir, &entry.name, volume).map_err(|error| match error {
                WinFinalizeError::Unreadable(c) => WinFinalizeError::Removal(c),
                other => other,
            })?;
            remove_tree(&child, volume)?;
            win_sys::delete(&child).map_err(partly)?;
        } else {
            // A file, or a reparse point removed as itself.
            let directory = entry.attributes & FILE_ATTRIBUTE_DIRECTORY != 0;
            let item =
                win_sys::open_at(dir, &entry.name, Want::Remove { directory }).map_err(partly)?;
            win_sys::delete(&item).map_err(partly)?;
        }
    }
    Ok(())
}

/// Permanently removes `name` from `store`, then its sidecar.
///
/// # Errors
///
/// A [`WinFinalizeError`]; [`WinFinalizeError::removed`] says how much is gone.
pub fn finalize(
    store: &WinStore,
    name: &[u16],
    expected: &WinExpected,
) -> Result<(), WinFinalizeError> {
    let want = Want::Remove {
        directory: expected.directory,
    };
    let held = win_sys::open_at(&store.dir, name, want).map_err(|error| {
        if error.kind() == io::ErrorKind::InvalidInput {
            WinFinalizeError::InvalidName
        } else {
            WinFinalizeError::NotInStore(WinMismatch::Unreachable {
                what: "the quarantined item",
                code: code(&error),
            })
        }
    })?;
    let facts = win_sys::facts(&held).map_err(|e| WinFinalizeError::Unreadable(code(&e)))?;
    if expected.directory {
        if is_reparse(&facts) {
            return Err(WinFinalizeError::NotInStore(WinMismatch::ReparsePoint(
                "the item",
            )));
        }
        if identity(&facts) != expected.identity {
            return Err(WinFinalizeError::NotInStore(WinMismatch::IdentityChanged));
        }
        if !facts.directory {
            return Err(WinFinalizeError::NotInStore(WinMismatch::KindChanged));
        }
        let volume = store.identity.volume;
        check_tree(&held, volume, 1)?;
        remove_tree(&held, volume)?;
    } else {
        check_leaf(&facts, expected).map_err(WinFinalizeError::NotInStore)?;
    }
    win_sys::delete(&held).map_err(|e| WinFinalizeError::Removal(code(&e)))?;
    drop(held);
    let mut sidecar = name.to_vec();
    sidecar.extend(".manifest.json".encode_utf16());
    match win_sys::open_at(&store.dir, &sidecar, Want::Remove { directory: false }) {
        Ok(handle) => win_sys::delete(&handle).map_err(|e| WinFinalizeError::Sidecar(code(&e))),
        Err(error) if gone(&error) => Ok(()),
        Err(error) => Err(WinFinalizeError::Sidecar(code(&error))),
    }
}

/// Removes the manifest sidecar of `name` once its item has left `store`
/// (it was restored). Nothing is removed while anything in the store has the
/// item's name, or while that cannot be told.
///
/// # Errors
///
/// [`WinFinalizeError::InvalidName`] for a bad name;
/// [`WinFinalizeError::Unreadable`] if the item's name is taken or cannot be
/// checked; [`WinFinalizeError::Sidecar`] if the removal fails. A missing
/// sidecar is not an error.
pub fn forget(store: &WinStore, name: &[u16]) -> Result<(), WinFinalizeError> {
    match win_sys::open_at(&store.dir, name, Want::Remove { directory: false }) {
        Err(error) if gone(&error) => {}
        Err(error) if error.kind() == io::ErrorKind::InvalidInput => {
            return Err(WinFinalizeError::InvalidName);
        }
        Ok(_) => {
            let taken = i32::try_from(ERROR_ALREADY_EXISTS).unwrap_or(i32::MAX);
            return Err(WinFinalizeError::Unreadable(taken));
        }
        Err(error) => return Err(WinFinalizeError::Unreadable(code(&error))),
    }
    let mut sidecar = name.to_vec();
    sidecar.extend(".manifest.json".encode_utf16());
    match win_sys::open_at(&store.dir, &sidecar, Want::Remove { directory: false }) {
        Ok(handle) => win_sys::delete(&handle).map_err(|e| WinFinalizeError::Sidecar(code(&e))),
        Err(error) if gone(&error) => Ok(()),
        Err(error) => Err(WinFinalizeError::Sidecar(code(&error))),
    }
}

#[cfg(test)]
mod tests {
    use std::os::windows::ffi::OsStrExt as _;
    use std::path::PathBuf;
    use std::process::Command;

    use lumen_testkit::{Fixture, Node};

    use super::*;
    use crate::win_move::{move_into, write_sidecar};
    use crate::win_store::open_store;
    use crate::win_verify::{WinPlanItem, observe, verify};

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
                path: "cache/dir/deep/f".into(),
                contents: b"f".to_vec(),
            },
            Node::File {
                path: "elsewhere/keep.txt".into(),
                contents: b"keep".to_vec(),
            },
        ])
        .unwrap_or_else(|e| unreachable!("{e}"));
        let root = std::fs::canonicalize(fx.root()).unwrap_or_else(|e| unreachable!("{e}"));
        let units: Vec<u16> = root.as_os_str().encode_wide().collect();
        let probe = observe(&units, &[wide("cache"), wide("app"), wide("data.bin")])
            .unwrap_or_else(|e| unreachable!("{e}"));
        let store = open_store(
            &units,
            &[&wide("Quarantine")],
            probe.expected.identity.volume,
        )
        .unwrap_or_else(|e| unreachable!("{e}"));
        (fx, root, units, store)
    }

    fn quarantined(units: &[u16], store: &WinStore, relative: &str, name: &str) -> WinPlanItem {
        let parts: Vec<Vec<u16>> = relative.split('/').map(wide).collect();
        let item = observe(units, &parts).unwrap_or_else(|e| unreachable!("{e}"));
        let verified = verify(&item).unwrap_or_else(|e| unreachable!("{e}"));
        move_into(&verified, store, &wide(name), &item.expected)
            .unwrap_or_else(|e| unreachable!("{e}"));
        write_sidecar(store, &wide(name), b"{}").unwrap_or_else(|e| unreachable!("{e}"));
        item
    }

    #[test]
    fn files_and_trees_are_removed_and_a_junction_target_is_untouched() {
        let (_fx, root, units, store) = setup();
        let status = Command::new("cmd")
            .arg("/C")
            .arg("mklink")
            .arg("/J")
            .arg(root.join("cache").join("dir").join("link"))
            .arg(root.join("elsewhere"))
            .status()
            .unwrap_or_else(|e| unreachable!("{e}"));
        assert!(status.success(), "mklink /J failed");
        let file = quarantined(&units, &store, "cache/app/data.bin", "item-1");
        let tree = quarantined(&units, &store, "cache/dir", "item-2");
        assert_eq!(finalize(&store, &wide("item-1"), &file.expected), Ok(()));
        assert_eq!(finalize(&store, &wide("item-2"), &tree.expected), Ok(()));
        let left = std::fs::read_dir(root.join("Quarantine"))
            .unwrap_or_else(|e| unreachable!("{e}"))
            .count();
        assert_eq!(left, 0, "items and sidecars are gone");
        assert_eq!(
            std::fs::read(root.join("elsewhere/keep.txt")).unwrap_or_default(),
            b"keep",
            "a junction's target is never entered"
        );
    }

    #[test]
    fn a_changed_or_missing_item_is_never_removed() {
        let (_fx, root, units, store) = setup();
        let file = quarantined(&units, &store, "cache/app/data.bin", "item-1");
        std::fs::write(root.join("Quarantine/item-1"), b"rewritten!!")
            .unwrap_or_else(|e| unreachable!("{e}"));
        let result = finalize(&store, &wide("item-1"), &file.expected);
        assert!(
            matches!(result, Err(WinFinalizeError::NotInStore(_))),
            "{result:?}"
        );
        assert_eq!(result.err().map(|e| e.removed()), Some(Removed::Nothing));
        assert!(root.join("Quarantine/item-1").exists());
        assert!(matches!(
            finalize(&store, &wide("item-9"), &file.expected),
            Err(WinFinalizeError::NotInStore(_))
        ));
    }

    #[test]
    fn an_interrupted_tree_removal_can_finish() {
        let (_fx, root, units, store) = setup();
        let tree = quarantined(&units, &store, "cache/dir", "item-2");
        #[allow(
            clippy::disallowed_methods,
            reason = "test fixture inside its own temporary directory"
        )]
        std::fs::remove_file(root.join("Quarantine/item-2/deep/f"))
            .unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(finalize(&store, &wide("item-2"), &tree.expected), Ok(()));
        assert!(!root.join("Quarantine/item-2").exists());
    }
}
