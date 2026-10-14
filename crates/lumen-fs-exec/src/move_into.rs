//! Moving a verified item into a quarantine store (roadmap 9.3, ADR-0015,
//! ADR-0016 step 4).
//!
//! The move is one rename between two verified directory handles with the
//! no-replace flag (`renameatx_np(RENAME_EXCL)` on Apple platforms,
//! `renameat2(RENAME_NOREPLACE)` on Linux): it never overwrites anything, never
//! copies, and never crosses volumes. Both directories are `fsync`ed, then the
//! result is checked: the source name must be gone and the item in the store
//! must still be exactly what the plan expected.
//!
//! A failure before the rename leaves the item untouched at its source. A
//! failure of the post-move check leaves it in the store, where it can be
//! restored; it is never deleted.

use rustix::fs::{AtFlags, RenameFlags, fsync, renameat_with, statat};
use rustix::io::Errno;

use crate::store::Store;
use crate::verify::{ExpectedLeaf, Mismatch, Verified, check_leaf};

/// Where an item is after a failed move.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Left {
    /// Untouched at its original location.
    AtSource,
    /// In the quarantine store, restorable.
    InStore,
}

/// Why a move failed, and where it left the item.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum MoveError {
    /// The store name is empty, `.`, `..` or contains `/` or NUL.
    #[error("invalid name in the store")]
    InvalidName,
    /// Something already has that name in the store; nothing was overwritten.
    #[error("the name is already taken in the store")]
    NameTaken,
    /// The store is on another volume.
    #[error("the store is on another volume")]
    OtherVolume,
    /// The volume cannot rename without replacing; quarantine is not safe here.
    #[error("the volume cannot rename without replacing")]
    NoReplaceUnsupported,
    /// The rename failed for another reason.
    #[error("the rename failed: {0}")]
    Rename(Errno),
    /// The directories could not be flushed after the rename.
    #[error("the move could not be made durable: {0}")]
    Sync(Errno),
    /// After the rename, the source name still exists.
    #[error("the source name still exists after the move")]
    SourceRemains,
    /// After the rename, the item in the store is not what was planned.
    #[error("the moved item does not match the plan: {0}")]
    Unverified(Mismatch),
}

impl MoveError {
    /// Where the failure left the item.
    pub const fn left(&self) -> Left {
        match self {
            Self::InvalidName
            | Self::NameTaken
            | Self::OtherVolume
            | Self::NoReplaceUnsupported
            | Self::Rename(_) => Left::AtSource,
            Self::Sync(_) | Self::SourceRemains | Self::Unverified(_) => Left::InStore,
        }
    }
}

fn plain(name: &[u8]) -> bool {
    !name.is_empty() && name != b"." && name != b".." && !name.contains(&b'/') && !name.contains(&0)
}

/// Renames the verified item into `store` as `name`, then checks the result
/// against `expected`.
///
/// # Errors
///
/// A [`MoveError`]; [`MoveError::left`] says where the item is.
pub fn move_into(
    verified: &Verified,
    store: &Store,
    name: &[u8],
    expected: &ExpectedLeaf,
) -> Result<(), MoveError> {
    if !plain(name) {
        return Err(MoveError::InvalidName);
    }
    renameat_with(
        &verified.parent,
        verified.leaf.as_slice(),
        store.handle(),
        name,
        RenameFlags::NOREPLACE,
    )
    .map_err(|errno| match errno {
        Errno::EXIST => MoveError::NameTaken,
        Errno::XDEV => MoveError::OtherVolume,
        Errno::NOTSUP | Errno::INVAL | Errno::NOSYS => MoveError::NoReplaceUnsupported,
        other => MoveError::Rename(other),
    })?;
    fsync(store.handle()).map_err(MoveError::Sync)?;
    fsync(&verified.parent).map_err(MoveError::Sync)?;
    match statat(
        &verified.parent,
        verified.leaf.as_slice(),
        AtFlags::SYMLINK_NOFOLLOW,
    ) {
        Err(Errno::NOENT) => {}
        Ok(_) | Err(_) => return Err(MoveError::SourceRemains),
    }
    let moved = statat(store.handle(), name, AtFlags::SYMLINK_NOFOLLOW).map_err(|errno| {
        MoveError::Unverified(Mismatch::Unreachable {
            what: "the moved item",
            errno,
        })
    })?;
    check_leaf(&moved, expected).map_err(MoveError::Unverified)
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;
    use std::os::unix::fs::PermissionsExt as _;
    use std::path::{Path, PathBuf};

    use lumen_testkit::{Fixture, Node};

    use super::*;
    use crate::store::open_store;
    use crate::verify::{PlanItem, identity, verify};

    /// A private fixture with `cache/app/data.bin`, `cache/dir/x` and a store.
    fn setup() -> (Fixture, PathBuf, Store) {
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
        let dev = identity(&rustix::fs::stat(&root).unwrap_or_else(|e| unreachable!("{e}"))).dev;
        let store = open_store(&root, &[b"Quarantine".as_slice()], dev)
            .unwrap_or_else(|e| unreachable!("{e}"));
        (fx, root, store)
    }

    fn stat(path: &Path) -> rustix::fs::Stat {
        rustix::fs::lstat(path).unwrap_or_else(|e| unreachable!("{e}"))
    }

    /// The plan for `relative` (`cache/<name>`) as it is now.
    fn plan(root: &Path, relative: &str) -> PlanItem {
        let (parent, leaf) = relative.rsplit_once('/').unwrap_or_else(|| unreachable!());
        let leaf_stat = stat(&root.join(relative));
        let is_dir = rustix::fs::FileType::from_raw_mode(leaf_stat.st_mode)
            == rustix::fs::FileType::Directory;
        PlanItem {
            root: root.to_path_buf(),
            root_identity: identity(&stat(root)),
            components: parent.split('/').map(|c| c.as_bytes().to_vec()).collect(),
            parent_identity: identity(&stat(&root.join(parent))),
            leaf: leaf.as_bytes().to_vec(),
            expected: crate::verify::tests_support::expected(&leaf_stat, is_dir),
        }
    }

    #[test]
    fn files_and_directories_move_into_the_store() {
        let (_fx, root, store) = setup();
        for (relative, name) in [("cache/app/data.bin", "item-1"), ("cache/dir", "item-2")] {
            let item = plan(&root, relative);
            let verified = verify(&item).unwrap_or_else(|e| unreachable!("{e}"));
            assert_eq!(
                move_into(&verified, &store, name.as_bytes(), &item.expected),
                Ok(())
            );
            assert!(!root.join(relative).exists(), "{relative} left its source");
            let moved = stat(&root.join("Quarantine").join(name));
            assert_eq!(identity(&moved), item.expected.identity, "same object");
        }
    }

    #[test]
    fn nothing_is_ever_overwritten() {
        let (_fx, root, store) = setup();
        let mut occupant = std::fs::File::create(root.join("Quarantine/item-1"))
            .unwrap_or_else(|e| unreachable!("{e}"));
        occupant
            .write_all(b"keep me")
            .unwrap_or_else(|e| unreachable!("{e}"));
        drop(occupant);
        let item = plan(&root, "cache/app/data.bin");
        let verified = verify(&item).unwrap_or_else(|e| unreachable!("{e}"));
        let result = move_into(&verified, &store, b"item-1", &item.expected);
        assert_eq!(result, Err(MoveError::NameTaken));
        assert_eq!(result.err().map(|e| e.left()), Some(Left::AtSource));
        assert_eq!(
            std::fs::read(root.join("Quarantine/item-1")).unwrap_or_default(),
            b"keep me"
        );
        assert!(root.join("cache/app/data.bin").exists(), "source untouched");
    }

    #[test]
    fn a_change_after_verification_is_caught_in_the_store() {
        let (_fx, root, store) = setup();
        let item = plan(&root, "cache/app/data.bin");
        let verified = verify(&item).unwrap_or_else(|e| unreachable!("{e}"));
        // The file grows between verification and the move.
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(root.join("cache/app/data.bin"))
            .unwrap_or_else(|e| unreachable!("{e}"));
        file.write_all(b"late write")
            .unwrap_or_else(|e| unreachable!("{e}"));
        drop(file);
        let result = move_into(&verified, &store, b"item-1", &item.expected);
        assert_eq!(result, Err(MoveError::Unverified(Mismatch::SizeChanged)));
        assert_eq!(result.err().map(|e| e.left()), Some(Left::InStore));
        assert!(
            root.join("Quarantine/item-1").exists(),
            "kept in the store, restorable"
        );
    }

    #[test]
    fn invalid_store_names_are_rejected_before_anything_moves() {
        let (_fx, root, store) = setup();
        let item = plan(&root, "cache/app/data.bin");
        let verified = verify(&item).unwrap_or_else(|e| unreachable!("{e}"));
        for name in [b"".as_slice(), b"..", b"a/b"] {
            assert_eq!(
                move_into(&verified, &store, name, &item.expected),
                Err(MoveError::InvalidName)
            );
        }
        assert!(root.join("cache/app/data.bin").exists());
    }
}
