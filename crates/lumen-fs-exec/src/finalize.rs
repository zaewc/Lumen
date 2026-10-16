//! Finalizing a quarantined item: Lumen's only permanent removal (roadmap
//! 9.8, ADR-0015).
//!
//! Finalization happens after retention or at the user's request, only inside
//! a quarantine store and only by handle. The item must still be what was
//! quarantined: a file is compared in full (identity, kind, size, time,
//! links); a directory by identity and kind, because a finalization that was
//! interrupted has already changed its contents and must be able to finish.
//! The store is private to its owner (ADR-0015), so nothing else changes it.
//!
//! A directory is checked completely before anything is removed: every
//! directory inside it is on the store's volume (a mount point is never
//! entered), it is at most [`MAX_DEPTH`] deep, and nothing is followed (a link
//! is removed as a link, never its target). Only then is it removed bottom-up.
//! The manifest sidecar goes last, then the store is flushed.
//!
//! [`forget`] removes only the sidecar of an item that has left the store
//! (restored), and only while nothing in the store has the item's name.

use std::ffi::CString;
use std::os::fd::OwnedFd;

use rustix::fs::{AtFlags, Dir, FileType, Mode, fstat, fsync, openat, statat, unlinkat};
use rustix::io::Errno;

use crate::fault::{self, Point};
use crate::sidecar::sidecar_name;
use crate::store::Store;
use crate::verify::{
    ExpectedLeaf, LeafKind, Mismatch, check_held, directory_flags, identity, plain,
};

/// The deepest directory nesting finalization removes.
pub const MAX_DEPTH: usize = 256;

pub(crate) use crate::location::Removed;

/// Why finalization failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum FinalizeError {
    /// The item name is empty, `.`, `..` or contains `/` or NUL.
    #[error("invalid name")]
    InvalidName,
    /// The item is missing from the store or is not what was quarantined.
    #[error("the quarantined item is missing or changed: {0}")]
    NotInStore(Mismatch),
    /// A directory inside the item is on another volume (a mount point).
    #[error("the item contains another volume")]
    CrossesDevice,
    /// The item is nested deeper than [`MAX_DEPTH`].
    #[error("the item is nested too deeply")]
    TooDeep,
    /// Part of the item could not be read while checking it.
    #[error("the item could not be checked: {0}")]
    Unreadable(Errno),
    /// A removal failed part way.
    #[error("the item could not be removed: {0}")]
    Removal(Errno),
    /// The item is gone but its sidecar could not be removed.
    #[error("the sidecar could not be removed: {0}")]
    Sidecar(Errno),
}

impl FinalizeError {
    /// How much was removed before the failure.
    pub const fn removed(&self) -> Removed {
        match self {
            Self::InvalidName
            | Self::NotInStore(_)
            | Self::CrossesDevice
            | Self::TooDeep
            | Self::Unreadable(_) => Removed::Nothing,
            Self::Removal(_) => Removed::Partly,
            Self::Sidecar(_) => Removed::Item,
        }
    }
}

/// One entry of a directory: its name and whether it is a directory itself.
type Entry = (CString, bool);

/// The entries of `dir` (without `.` and `..`), read in full before anything
/// is changed. Kinds come from `fstatat`, never from `d_type`.
fn entries(dir: &OwnedFd) -> Result<Vec<Entry>, Errno> {
    let mut out = Vec::new();
    for entry in Dir::read_from(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        if matches!(name.to_bytes(), b"." | b"..") {
            continue;
        }
        let stat = statat(dir, name, AtFlags::SYMLINK_NOFOLLOW)?;
        let is_dir = FileType::from_raw_mode(stat.st_mode) == FileType::Directory;
        out.push((name.to_owned(), is_dir));
    }
    Ok(out)
}

/// Opens the directory `name` in `dir`, refusing links and other volumes.
fn open_child(dir: &OwnedFd, name: &CString, device: u64) -> Result<OwnedFd, FinalizeError> {
    let child = openat(dir, name.as_c_str(), directory_flags(), Mode::empty())
        .map_err(FinalizeError::Unreadable)?;
    let stat = fstat(&child).map_err(FinalizeError::Unreadable)?;
    if identity(&stat).dev != device {
        return Err(FinalizeError::CrossesDevice);
    }
    Ok(child)
}

/// Checks the tree below `dir` without changing anything.
fn check_tree(dir: &OwnedFd, device: u64, depth: usize) -> Result<(), FinalizeError> {
    if depth > MAX_DEPTH {
        return Err(FinalizeError::TooDeep);
    }
    for (name, is_dir) in entries(dir).map_err(FinalizeError::Unreadable)? {
        if is_dir {
            check_tree(&open_child(dir, &name, device)?, device, depth + 1)?;
        }
    }
    Ok(())
}

/// Removes everything below `dir`, bottom-up.
fn remove_tree(dir: &OwnedFd, device: u64) -> Result<(), FinalizeError> {
    let partly = FinalizeError::Removal;
    for (name, is_dir) in entries(dir).map_err(partly)? {
        if is_dir {
            let child = open_child(dir, &name, device).map_err(|error| match error {
                FinalizeError::Unreadable(errno) => partly(errno),
                other => other,
            })?;
            remove_tree(&child, device)?;
            unlinkat(dir, name.as_c_str(), AtFlags::REMOVEDIR).map_err(partly)?;
        } else {
            unlinkat(dir, name.as_c_str(), AtFlags::empty())
                .and_then(|()| fault::check(Point::FinalizeUnlink))
                .map_err(partly)?;
        }
    }
    Ok(())
}

/// Permanently removes `name` from `store`, then its sidecar.
///
/// # Errors
///
/// A [`FinalizeError`]; [`FinalizeError::removed`] says how much is gone.
pub fn finalize(store: &Store, name: &[u8], expected: &ExpectedLeaf) -> Result<(), FinalizeError> {
    if !plain(name) {
        return Err(FinalizeError::InvalidName);
    }
    let held = statat(store.handle(), name, AtFlags::SYMLINK_NOFOLLOW).map_err(|errno| {
        FinalizeError::NotInStore(Mismatch::Unreachable {
            what: "the quarantined item",
            errno,
        })
    })?;
    let device = store.identity.dev;
    check_held(&held, expected).map_err(FinalizeError::NotInStore)?;
    match expected.kind {
        LeafKind::File => {
            unlinkat(store.handle(), name, AtFlags::empty()).map_err(FinalizeError::Removal)?;
        }
        LeafKind::Directory => {
            let name = CString::new(name).map_err(|_| FinalizeError::InvalidName)?;
            let dir = open_child(store.handle(), &name, device)?;
            check_tree(&dir, device, 1)?;
            remove_tree(&dir, device)?;
            unlinkat(store.handle(), name.as_c_str(), AtFlags::REMOVEDIR)
                .map_err(FinalizeError::Removal)?;
        }
    }
    let sidecar = sidecar_name(name).map_err(|_| FinalizeError::InvalidName)?;
    match unlinkat(store.handle(), sidecar.as_slice(), AtFlags::empty())
        .and_then(|()| fault::check(Point::FinalizeSidecar))
    {
        Ok(()) | Err(Errno::NOENT) => {}
        Err(errno) => return Err(FinalizeError::Sidecar(errno)),
    }
    fsync(store.handle()).map_err(FinalizeError::Sidecar)
}

/// Removes the manifest sidecar of `name` once its item has left `store`
/// (it was restored). Nothing is removed while anything in the store has the
/// item's name.
///
/// # Errors
///
/// [`FinalizeError::InvalidName`] for a bad name;
/// [`FinalizeError::Unreadable`] if the item's name is taken (`EEXIST`) or
/// cannot be checked; [`FinalizeError::Sidecar`] if the removal fails. A
/// missing sidecar is not an error.
pub fn forget(store: &Store, name: &[u8]) -> Result<(), FinalizeError> {
    if !plain(name) {
        return Err(FinalizeError::InvalidName);
    }
    match statat(store.handle(), name, AtFlags::SYMLINK_NOFOLLOW) {
        Err(Errno::NOENT) => {}
        Ok(_) => return Err(FinalizeError::Unreadable(Errno::EXIST)),
        Err(errno) => return Err(FinalizeError::Unreadable(errno)),
    }
    let sidecar = sidecar_name(name).map_err(|_| FinalizeError::InvalidName)?;
    match unlinkat(store.handle(), sidecar.as_slice(), AtFlags::empty()) {
        Ok(()) | Err(Errno::NOENT) => {}
        Err(errno) => return Err(FinalizeError::Sidecar(errno)),
    }
    fsync(store.handle()).map_err(FinalizeError::Sidecar)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::move_into::move_into;
    use crate::move_into::tests::{plan, setup};
    use crate::sidecar::write_sidecar;
    use crate::verify::{PlanItem, verify};

    fn quarantined(root: &Path, store: &Store, relative: &str, name: &str) -> PlanItem {
        let item = plan(root, relative);
        let verified = verify(&item).unwrap_or_else(|e| unreachable!("{e}"));
        move_into(&verified, store, name.as_bytes(), &item.expected)
            .unwrap_or_else(|e| unreachable!("{e}"));
        write_sidecar(store, name.as_bytes(), b"{}").unwrap_or_else(|e| unreachable!("{e}"));
        item
    }

    #[test]
    fn files_and_trees_are_removed_with_their_sidecars() {
        let (_fx, root, store) = setup();
        std::fs::create_dir_all(root.join("cache/dir/deep/er"))
            .unwrap_or_else(|e| unreachable!("{e}"));
        std::fs::write(root.join("cache/dir/deep/er/f"), b"f")
            .unwrap_or_else(|e| unreachable!("{e}"));
        std::os::unix::fs::symlink(root.join("cache/app"), root.join("cache/dir/link"))
            .unwrap_or_else(|e| unreachable!("{e}"));
        let file = quarantined(&root, &store, "cache/app/data.bin", "item-1");
        let tree = quarantined(&root, &store, "cache/dir", "item-2");
        assert_eq!(finalize(&store, b"item-1", &file.expected), Ok(()));
        assert_eq!(finalize(&store, b"item-2", &tree.expected), Ok(()));
        let left: Vec<_> = std::fs::read_dir(root.join("Quarantine"))
            .unwrap_or_else(|e| unreachable!("{e}"))
            .collect();
        assert_eq!(left.len(), 0, "items and sidecars are gone");
        assert!(
            root.join("cache/app").exists(),
            "a link's target is never touched"
        );
    }

    #[test]
    fn a_changed_or_missing_item_is_never_removed() {
        let (_fx, root, store) = setup();
        let file = quarantined(&root, &store, "cache/app/data.bin", "item-1");
        std::fs::write(root.join("Quarantine/item-1"), b"rewritten!!")
            .unwrap_or_else(|e| unreachable!("{e}"));
        let result = finalize(&store, b"item-1", &file.expected);
        assert_eq!(
            result,
            Err(FinalizeError::NotInStore(Mismatch::SizeChanged))
        );
        assert_eq!(result.err().map(|e| e.removed()), Some(Removed::Nothing));
        assert!(root.join("Quarantine/item-1").exists());
        assert!(matches!(
            finalize(&store, b"item-9", &file.expected),
            Err(FinalizeError::NotInStore(_))
        ));
    }

    #[test]
    fn a_tree_too_deep_is_refused_before_anything_is_removed() {
        let (_fx, root, store) = setup();
        let mut deep = root.join("cache/dir");
        for _ in 0..=MAX_DEPTH {
            deep.push("d");
        }
        std::fs::create_dir_all(&deep).unwrap_or_else(|e| unreachable!("{e}"));
        let tree = quarantined(&root, &store, "cache/dir", "item-2");
        let result = finalize(&store, b"item-2", &tree.expected);
        assert_eq!(result, Err(FinalizeError::TooDeep));
        assert!(root.join("Quarantine/item-2/x").exists(), "nothing removed");
    }

    #[test]
    fn an_interrupted_tree_removal_can_finish() {
        let (_fx, root, store) = setup();
        let tree = quarantined(&root, &store, "cache/dir", "item-2");
        // A previous finalization removed part of the tree, then stopped.
        #[allow(
            clippy::disallowed_methods,
            reason = "test fixture inside its own temporary directory"
        )]
        std::fs::remove_file(root.join("Quarantine/item-2/x"))
            .unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(finalize(&store, b"item-2", &tree.expected), Ok(()));
        assert!(!root.join("Quarantine/item-2").exists());
    }

    #[test]
    fn invalid_names_are_rejected() {
        let (_fx, root, store) = setup();
        let file = quarantined(&root, &store, "cache/app/data.bin", "item-1");
        for name in [b"".as_slice(), b"..", b"a/b"] {
            assert_eq!(
                finalize(&store, name, &file.expected),
                Err(FinalizeError::InvalidName)
            );
        }
        assert!(root.join("Quarantine/item-1").exists());
    }
}
