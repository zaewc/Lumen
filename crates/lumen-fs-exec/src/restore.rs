//! Restoring a quarantined item to where it was (roadmap 9.7, ADR-0015).
//!
//! The item must still be exactly what was quarantined, and its original
//! parent must be reachable from the scan root by handle, without following
//! any link or leaving the store's volume. The parent may have been recreated
//! since (a regenerated cache directory, say); that is reported, not refused,
//! because putting the item back at its path is what restoring means.
//!
//! The move back is one no-replace rename: if anything now has the item's
//! name, nothing is overwritten and the user decides. Both directories are
//! `fsync`ed and the result is checked. A failure before the rename leaves
//! the item in the store; after it, the item is back at its source.
//!
//! The manifest sidecar stays in the store as the record of the operation;
//! finalization (roadmap 9.8) clears it.

use std::os::fd::OwnedFd;

use rustix::fs::{AtFlags, Mode, RenameFlags, fstat, fsync, open, openat, renameat_with, statat};
use rustix::io::Errno;

use crate::move_into::Left;
use crate::store::Store;
use crate::verify::{Mismatch, PlanItem, check_leaf, directory_flags, identity, plain};

/// Why a restore failed, and where it left the item.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum RestoreError {
    /// A name is empty, `.`, `..` or contains `/` or NUL.
    #[error("invalid name")]
    InvalidName,
    /// The item in the store is missing or no longer what was quarantined.
    #[error("the quarantined item is missing or changed: {0}")]
    NotInStore(Mismatch),
    /// The original parent cannot be reached safely.
    #[error("the original location cannot be reached: {0}")]
    ParentUnreachable(Mismatch),
    /// Something now has the item's name; nothing was overwritten.
    #[error("the original name is taken")]
    NameTaken,
    /// The volume cannot rename without replacing.
    #[error("the volume cannot rename without replacing")]
    NoReplaceUnsupported,
    /// The rename failed for another reason.
    #[error("the rename failed: {0}")]
    Rename(Errno),
    /// The directories could not be flushed after the rename.
    #[error("the restore could not be made durable: {0}")]
    Sync(Errno),
    /// After the rename, the item's name still exists in the store.
    #[error("the item is still in the store after the restore")]
    StoreRemains,
    /// After the rename, the restored item is not what was quarantined.
    #[error("the restored item does not match: {0}")]
    Unverified(Mismatch),
}

impl RestoreError {
    /// Where the failure left the item.
    pub const fn left(&self) -> Left {
        match self {
            Self::InvalidName
            | Self::NotInStore(_)
            | Self::ParentUnreachable(_)
            | Self::NameTaken
            | Self::NoReplaceUnsupported
            | Self::Rename(_) => Left::InStore,
            Self::Sync(_) | Self::StoreRemains | Self::Unverified(_) => Left::AtSource,
        }
    }
}

/// A completed restore.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Restored {
    /// The original parent was replaced by another directory at its path.
    /// Informational and best effort: identity is `(dev, ino)`, and a parent
    /// recreated after the old one was freed may reuse its inode number.
    pub parent_replaced: bool,
}

/// Opens the original parent of `item` by handle, on `device` only.
fn parent(item: &PlanItem, device: u64) -> Result<OwnedFd, Mismatch> {
    let unreachable = |what| move |errno| Mismatch::Unreachable { what, errno };
    let mut current =
        open(&item.root, directory_flags(), Mode::empty()).map_err(unreachable("the scan root"))?;
    for component in &item.components {
        current = openat(
            &current,
            component.as_slice(),
            directory_flags(),
            Mode::empty(),
        )
        .map_err(unreachable("a directory on the way"))?;
    }
    let stat = fstat(&current).map_err(unreachable("the parent"))?;
    if identity(&stat).dev != device {
        return Err(Mismatch::CrossesDevice);
    }
    Ok(current)
}

/// Moves `name` in `store` back to where `item` was.
///
/// # Errors
///
/// A [`RestoreError`]; [`RestoreError::left`] says where the item is.
pub fn restore(store: &Store, name: &[u8], item: &PlanItem) -> Result<Restored, RestoreError> {
    if !plain(name) || !plain(&item.leaf) || !item.components.iter().all(|c| plain(c)) {
        return Err(RestoreError::InvalidName);
    }
    let held = statat(store.handle(), name, AtFlags::SYMLINK_NOFOLLOW).map_err(|errno| {
        RestoreError::NotInStore(Mismatch::Unreachable {
            what: "the quarantined item",
            errno,
        })
    })?;
    check_leaf(&held, &item.expected).map_err(RestoreError::NotInStore)?;
    let parent = parent(item, store.identity.dev).map_err(RestoreError::ParentUnreachable)?;
    let parent_replaced = !fstat(&parent).is_ok_and(|stat| identity(&stat) == item.parent_identity);
    renameat_with(
        store.handle(),
        name,
        &parent,
        item.leaf.as_slice(),
        RenameFlags::NOREPLACE,
    )
    .map_err(|errno| match errno {
        Errno::EXIST => RestoreError::NameTaken,
        Errno::NOTSUP | Errno::INVAL | Errno::NOSYS => RestoreError::NoReplaceUnsupported,
        other => RestoreError::Rename(other),
    })?;
    fsync(&parent).map_err(RestoreError::Sync)?;
    fsync(store.handle()).map_err(RestoreError::Sync)?;
    match statat(store.handle(), name, AtFlags::SYMLINK_NOFOLLOW) {
        Err(Errno::NOENT) => {}
        Ok(_) | Err(_) => return Err(RestoreError::StoreRemains),
    }
    let restored =
        statat(&parent, item.leaf.as_slice(), AtFlags::SYMLINK_NOFOLLOW).map_err(|errno| {
            RestoreError::Unverified(Mismatch::Unreachable {
                what: "the restored item",
                errno,
            })
        })?;
    check_leaf(&restored, &item.expected).map_err(RestoreError::Unverified)?;
    Ok(Restored { parent_replaced })
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;
    use std::path::Path;

    use super::*;
    use crate::move_into::move_into;
    use crate::move_into::tests::{plan, setup};
    use crate::verify::verify;

    /// Quarantines `relative` as `name` and returns its plan.
    fn quarantined(root: &Path, store: &Store, relative: &str, name: &str) -> PlanItem {
        let item = plan(root, relative);
        let verified = verify(&item).unwrap_or_else(|e| unreachable!("{e}"));
        move_into(&verified, store, name.as_bytes(), &item.expected)
            .unwrap_or_else(|e| unreachable!("{e}"));
        item
    }

    #[test]
    fn files_and_directories_come_back_where_they_were() {
        let (_fx, root, store) = setup();
        for (relative, name) in [("cache/app/data.bin", "item-1"), ("cache/dir", "item-2")] {
            let item = quarantined(&root, &store, relative, name);
            assert_eq!(
                restore(&store, name.as_bytes(), &item),
                Ok(Restored {
                    parent_replaced: false
                })
            );
            let back =
                rustix::fs::lstat(root.join(relative)).unwrap_or_else(|e| unreachable!("{e}"));
            assert_eq!(identity(&back), item.expected.identity, "{relative}");
            assert!(!root.join("Quarantine").join(name).exists());
        }
    }

    #[test]
    fn a_taken_name_is_never_overwritten() {
        let (_fx, root, store) = setup();
        let item = quarantined(&root, &store, "cache/app/data.bin", "item-1");
        std::fs::write(root.join("cache/app/data.bin"), b"new file")
            .unwrap_or_else(|e| unreachable!("{e}"));
        let result = restore(&store, b"item-1", &item);
        assert_eq!(result, Err(RestoreError::NameTaken));
        assert_eq!(result.err().map(|e| e.left()), Some(Left::InStore));
        assert_eq!(
            std::fs::read(root.join("cache/app/data.bin")).unwrap_or_default(),
            b"new file"
        );
        assert!(root.join("Quarantine/item-1").exists());
    }

    #[test]
    fn a_changed_item_stays_in_the_store() {
        let (_fx, root, store) = setup();
        let item = quarantined(&root, &store, "cache/app/data.bin", "item-1");
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(root.join("Quarantine/item-1"))
            .unwrap_or_else(|e| unreachable!("{e}"));
        file.write_all(b"x").unwrap_or_else(|e| unreachable!("{e}"));
        drop(file);
        assert_eq!(
            restore(&store, b"item-1", &item),
            Err(RestoreError::NotInStore(Mismatch::SizeChanged))
        );
        assert!(!root.join("cache/app/data.bin").exists());
    }

    #[test]
    fn a_recreated_parent_is_reported_and_a_linked_one_refused() {
        let (_fx, root, store) = setup();
        let item = quarantined(&root, &store, "cache/app/data.bin", "item-1");
        // Holding the old parent open keeps its inode number from being reused
        // by the recreated one (Linux reuses freed inodes at once).
        let _old_parent =
            std::fs::File::open(root.join("cache/app")).unwrap_or_else(|e| unreachable!("{e}"));
        #[allow(
            clippy::disallowed_methods,
            reason = "test fixture inside its own temporary directory"
        )]
        {
            std::fs::remove_dir(root.join("cache/app")).unwrap_or_else(|e| unreachable!("{e}"));
            std::fs::create_dir(root.join("elsewhere")).unwrap_or_else(|e| unreachable!("{e}"));
        }
        std::os::unix::fs::symlink(root.join("elsewhere"), root.join("cache/app"))
            .unwrap_or_else(|e| unreachable!("{e}"));
        assert!(matches!(
            restore(&store, b"item-1", &item),
            Err(RestoreError::ParentUnreachable(_))
        ));
        assert!(
            !root.join("elsewhere/data.bin").exists(),
            "never through a link"
        );
        #[allow(
            clippy::disallowed_methods,
            reason = "test fixture inside its own temporary directory"
        )]
        std::fs::remove_file(root.join("cache/app")).unwrap_or_else(|e| unreachable!("{e}"));
        std::fs::create_dir(root.join("cache/app")).unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(
            restore(&store, b"item-1", &item),
            Ok(Restored {
                parent_replaced: true
            })
        );
        assert!(root.join("cache/app/data.bin").exists());
    }

    #[test]
    fn invalid_names_are_rejected_before_anything_moves() {
        let (_fx, root, store) = setup();
        let item = quarantined(&root, &store, "cache/app/data.bin", "item-1");
        for name in [b"".as_slice(), b"..", b"a/b"] {
            assert_eq!(restore(&store, name, &item), Err(RestoreError::InvalidName));
        }
        assert!(root.join("Quarantine/item-1").exists());
    }
}
