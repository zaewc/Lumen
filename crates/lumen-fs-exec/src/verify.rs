//! Re-verifying a plan item by handle before anything happens to it
//! (roadmap 9.1, ADR-0016 steps 1–2).
//!
//! Only the scan root is opened by path. From there each component is opened
//! relative to its parent's handle with `O_NOFOLLOW` (and `O_NOFOLLOW_ANY` on
//! Apple platforms), so a symlink swapped in anywhere along the way fails the
//! open instead of being followed. Every directory must stay on the root's
//! device: mount points are never descended into. The leaf is examined with
//! `AT_SYMLINK_NOFOLLOW` and compared with what the plan expects.
//!
//! Nothing here changes the filesystem. A successful verification returns the
//! parent's open handle, so the move (9.3) acts on exactly what was verified.

use std::os::fd::OwnedFd;
use std::path::PathBuf;

use rustix::fs::{AtFlags, FileType, Mode, OFlags, Stat, fstat, open, openat, statat};
use rustix::io::Errno;

/// A filesystem object's identity on Unix: device and inode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UnixIdentity {
    /// `st_dev`, as its bit pattern.
    pub dev: u64,
    /// `st_ino`.
    pub ino: u64,
}

/// What a plan item may be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeafKind {
    /// A regular file.
    File,
    /// A directory.
    Directory,
}

/// What the leaf must still be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExpectedLeaf {
    /// Its identity.
    pub identity: UnixIdentity,
    /// Its kind.
    pub kind: LeafKind,
    /// Its length in bytes (files only; ignored for directories).
    pub size: u64,
    /// Its modification time, `(seconds, nanoseconds)`.
    pub modified: (i64, i64),
    /// Its hard-link count.
    pub links: u64,
}

/// One item of a confirmed plan: where it is, relative to a scan root, and
/// what it must still be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanItem {
    /// The scan root, opened by path. It must be canonical: on Apple platforms
    /// a symlink anywhere in it fails verification.
    pub root: PathBuf,
    /// The root's identity.
    pub root_identity: UnixIdentity,
    /// Directory names from the root down to the leaf's parent.
    pub components: Vec<Vec<u8>>,
    /// The parent directory's identity.
    pub parent_identity: UnixIdentity,
    /// The leaf's name in its parent.
    pub leaf: Vec<u8>,
    /// What the leaf must be.
    pub expected: ExpectedLeaf,
}

/// Why a plan item does not match the filesystem. Each one sends the item to
/// `REVIEW`; none is retried.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Mismatch {
    /// A name is empty, `.`, `..` or contains `/` or NUL.
    #[error("invalid name in the plan")]
    InvalidName,
    /// The root, a directory on the way, or the leaf could not be opened or
    /// examined (missing, denied, or a symlink where a directory was expected).
    #[error("{what} could not be opened: {errno}")]
    Unreachable {
        /// Which step.
        what: &'static str,
        /// The error number.
        errno: Errno,
    },
    /// A directory on the way is on another device (a mount point).
    #[error("the path crosses into another filesystem")]
    CrossesDevice,
    /// The root or the parent is no longer the planned directory.
    #[error("{0} is no longer the planned directory")]
    DirectoryChanged(&'static str),
    /// The leaf is another object now.
    #[error("the item was replaced by another object")]
    IdentityChanged,
    /// The leaf is a different kind of object (e.g. a symlink now).
    #[error("the item changed kind")]
    KindChanged,
    /// The leaf's size changed.
    #[error("the item changed size")]
    SizeChanged,
    /// The leaf was modified.
    #[error("the item was modified")]
    Modified,
    /// The leaf gained or lost hard links.
    #[error("the item's link count changed")]
    LinksChanged,
}

/// A verified item: its parent's handle and the leaf's name in it.
#[derive(Debug)]
pub struct Verified {
    /// The parent directory, opened by handle along the verified walk.
    pub parent: OwnedFd,
    /// The leaf's name in `parent`.
    pub leaf: Vec<u8>,
}

/// `(dev, ino)` of a `stat`. Device numbers are signed on some platforms; only
/// the bit pattern matters, because it is compared for equality and never
/// interpreted.
#[allow(
    clippy::cast_sign_loss,
    clippy::cast_lossless,
    clippy::unnecessary_cast,
    reason = "st_dev's type differs per platform; only equality of the bits is used"
)]
pub(crate) fn identity(stat: &Stat) -> UnixIdentity {
    UnixIdentity {
        dev: stat.st_dev as u64,
        ino: stat.st_ino as u64,
    }
}

#[allow(
    clippy::cast_lossless,
    clippy::cast_possible_wrap,
    clippy::unnecessary_cast,
    reason = "time field types differ per platform (unsigned nanoseconds on Linux); values are compared, not computed"
)]
fn modified(stat: &Stat) -> (i64, i64) {
    (stat.st_mtime as i64, stat.st_mtime_nsec as i64)
}

#[allow(
    clippy::cast_lossless,
    clippy::unnecessary_cast,
    reason = "st_nlink is u16 on Apple platforms and u64 on Linux"
)]
fn links(stat: &Stat) -> u64 {
    stat.st_nlink as u64
}

/// Whether `name` is one plain directory entry name.
pub(crate) fn plain(name: &[u8]) -> bool {
    !name.is_empty() && name != b"." && name != b".." && !name.contains(&b'/') && !name.contains(&0)
}

/// Flags for opening a directory on the walk without following anything.
///
/// On Apple platforms `O_NOFOLLOW_ANY` refuses a symlink in *any* component
/// (`ELOOP`), including the root's own path, and it replaces `O_NOFOLLOW`:
/// passing both fails with `EINVAL` (measured on macOS 26). Plan roots must
/// therefore be canonical paths, which scan roots are.
pub(crate) fn directory_flags() -> OFlags {
    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC;
    #[cfg(target_vendor = "apple")]
    let flags = flags | OFlags::NOFOLLOW_ANY;
    #[cfg(not(target_vendor = "apple"))]
    let flags = flags | OFlags::NOFOLLOW;
    flags
}

/// Walks from the root to the leaf's parent by handle and checks the leaf.
///
/// # Errors
///
/// The first [`Mismatch`] found; the filesystem is never changed.
pub fn verify(item: &PlanItem) -> Result<Verified, Mismatch> {
    if !item.components.iter().all(|c| plain(c)) || !plain(&item.leaf) {
        return Err(Mismatch::InvalidName);
    }
    let unreachable = |what| move |errno| Mismatch::Unreachable { what, errno };
    let root =
        open(&item.root, directory_flags(), Mode::empty()).map_err(unreachable("the scan root"))?;
    let root_stat = fstat(&root).map_err(unreachable("the scan root"))?;
    if identity(&root_stat) != item.root_identity {
        return Err(Mismatch::DirectoryChanged("the scan root"));
    }
    let device = item.root_identity.dev;
    let mut current = root;
    for component in &item.components {
        let next = openat(
            &current,
            component.as_slice(),
            directory_flags(),
            Mode::empty(),
        )
        .map_err(unreachable("a directory on the way"))?;
        let stat = fstat(&next).map_err(unreachable("a directory on the way"))?;
        if identity(&stat).dev != device {
            return Err(Mismatch::CrossesDevice);
        }
        current = next;
    }
    let parent_stat = fstat(&current).map_err(unreachable("the parent"))?;
    if identity(&parent_stat) != item.parent_identity {
        return Err(Mismatch::DirectoryChanged("the parent"));
    }
    let leaf = statat(&current, item.leaf.as_slice(), AtFlags::SYMLINK_NOFOLLOW)
        .map_err(unreachable("the item"))?;
    check_leaf(&leaf, &item.expected)?;
    Ok(Verified {
        parent: current,
        leaf: item.leaf.clone(),
    })
}

/// Compares a leaf's `stat` with what the plan expects.
pub(crate) fn check_leaf(stat: &Stat, expected: &ExpectedLeaf) -> Result<(), Mismatch> {
    if identity(stat) != expected.identity {
        return Err(Mismatch::IdentityChanged);
    }
    let kind = match FileType::from_raw_mode(stat.st_mode) {
        FileType::RegularFile => LeafKind::File,
        FileType::Directory => LeafKind::Directory,
        _ => return Err(Mismatch::KindChanged),
    };
    if kind != expected.kind {
        return Err(Mismatch::KindChanged);
    }
    if kind == LeafKind::File && u64::try_from(stat.st_size).ok() != Some(expected.size) {
        return Err(Mismatch::SizeChanged);
    }
    if modified(stat) != expected.modified {
        return Err(Mismatch::Modified);
    }
    if links(stat) != expected.links {
        return Err(Mismatch::LinksChanged);
    }
    Ok(())
}

/// What a plan would expect of an object that looks like `stat` now.
/// Checks an item held in a quarantine store. A file must match in full; a
/// directory by identity and kind only, because an interrupted finalization
/// has already changed its contents and must be able to finish. The store is
/// private to its owner (ADR-0015), so nothing else changes it.
pub(crate) fn check_held(stat: &Stat, expected: &ExpectedLeaf) -> Result<(), Mismatch> {
    match expected.kind {
        LeafKind::File => check_leaf(stat, expected),
        LeafKind::Directory => {
            if identity(stat) != expected.identity {
                Err(Mismatch::IdentityChanged)
            } else if FileType::from_raw_mode(stat.st_mode) == FileType::Directory {
                Ok(())
            } else {
                Err(Mismatch::KindChanged)
            }
        }
    }
}

pub(crate) fn expected_from(stat: &Stat, is_dir: bool) -> ExpectedLeaf {
    ExpectedLeaf {
        identity: identity(stat),
        kind: if is_dir {
            LeafKind::Directory
        } else {
            LeafKind::File
        },
        size: u64::try_from(stat.st_size).unwrap_or(0),
        modified: modified(stat),
        links: links(stat),
    }
}

/// Test helpers shared with the other modules' tests.
#[cfg(test)]
pub(crate) mod tests_support {
    use super::{ExpectedLeaf, Stat, expected_from};

    /// What a plan would expect of an object that looks like `stat` now.
    pub(crate) fn expected(stat: &Stat, is_dir: bool) -> ExpectedLeaf {
        expected_from(stat, is_dir)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;
    use std::path::Path;

    use lumen_testkit::{Fixture, Node};

    use super::*;

    fn stat_path(path: &Path) -> Stat {
        rustix::fs::lstat(path).unwrap_or_else(|e| unreachable!("{e}"))
    }

    /// A plan for `relative` (a/b/leaf) under the fixture root, as it is now.
    fn plan(fx: &Fixture, relative: &str) -> PlanItem {
        // Temporary directories can sit behind symlinks (`/var` on macOS).
        let root = std::fs::canonicalize(fx.root()).unwrap_or_else(|e| unreachable!("{e}"));
        let mut parts: Vec<&str> = relative.split('/').collect();
        let leaf = parts.pop().unwrap_or_default();
        let parent = parts.iter().fold(root.clone(), |p, c| p.join(c));
        let leaf_stat = stat_path(&parent.join(leaf));
        let kind = if FileType::from_raw_mode(leaf_stat.st_mode) == FileType::Directory {
            LeafKind::Directory
        } else {
            LeafKind::File
        };
        PlanItem {
            root: root.clone(),
            root_identity: identity(&stat_path(&root)),
            components: parts.iter().map(|c| c.as_bytes().to_vec()).collect(),
            parent_identity: identity(&stat_path(&parent)),
            leaf: leaf.as_bytes().to_vec(),
            expected: ExpectedLeaf {
                identity: identity(&leaf_stat),
                kind,
                size: u64::try_from(leaf_stat.st_size).unwrap_or(0),
                modified: modified(&leaf_stat),
                links: links(&leaf_stat),
            },
        }
    }

    fn tree() -> Fixture {
        Fixture::build(&[
            Node::File {
                path: "cache/app/data.bin".into(),
                contents: b"0123456789".to_vec(),
            },
            Node::File {
                path: "cache/other".into(),
                contents: b"x".to_vec(),
            },
        ])
        .unwrap_or_else(|e| unreachable!("{e}"))
    }

    #[test]
    fn an_unchanged_item_verifies_and_yields_its_parent() {
        let fx = tree();
        for relative in ["cache/app/data.bin", "cache/app"] {
            let item = plan(&fx, relative);
            let verified = verify(&item).unwrap_or_else(|e| unreachable!("{relative}: {e}"));
            assert_eq!(verified.leaf, item.leaf);
            let parent = fstat(&verified.parent).unwrap_or_else(|e| unreachable!("{e}"));
            assert_eq!(identity(&parent), item.parent_identity);
        }
    }

    #[test]
    fn invalid_names_are_rejected() {
        let fx = tree();
        for (components, leaf) in [
            (vec![b"..".to_vec()], b"x".to_vec()),
            (vec![b"cache/app".to_vec()], b"x".to_vec()),
            (vec![b"cache".to_vec()], b"".to_vec()),
            (vec![b"cache".to_vec()], b"a\0b".to_vec()),
        ] {
            let mut item = plan(&fx, "cache/app/data.bin");
            item.components = components;
            item.leaf = leaf;
            assert_eq!(verify(&item).map(|_| ()), Err(Mismatch::InvalidName));
        }
    }

    #[test]
    fn changes_to_the_leaf_are_detected() {
        let fx = tree();
        let item = plan(&fx, "cache/app/data.bin");
        let path = fx.root().join("cache/app/data.bin");
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap_or_else(|e| unreachable!("{e}"));
        file.write_all(b"more")
            .unwrap_or_else(|e| unreachable!("{e}"));
        drop(file);
        assert_eq!(verify(&item).map(|_| ()), Err(Mismatch::SizeChanged));

        let mut modified_only = plan(&fx, "cache/app/data.bin");
        modified_only.expected.modified.1 = modified_only.expected.modified.1.wrapping_add(1);
        assert_eq!(verify(&modified_only).map(|_| ()), Err(Mismatch::Modified));

        let mut other = plan(&fx, "cache/app/data.bin");
        other.expected.identity = identity(&stat_path(&fx.root().join("cache/other")));
        assert_eq!(verify(&other).map(|_| ()), Err(Mismatch::IdentityChanged));

        let mut linked = plan(&fx, "cache/app/data.bin");
        linked.expected.links += 1;
        assert_eq!(verify(&linked).map(|_| ()), Err(Mismatch::LinksChanged));

        let mut kind = plan(&fx, "cache/app/data.bin");
        kind.expected.kind = LeafKind::Directory;
        assert_eq!(verify(&kind).map(|_| ()), Err(Mismatch::KindChanged));
    }

    #[test]
    fn symlinks_on_the_way_are_never_followed() {
        let fx = Fixture::build(&[
            Node::File {
                path: "real/data.bin".into(),
                contents: b"x".to_vec(),
            },
            Node::Symlink {
                path: "link".into(),
                target: "real".into(),
            },
        ])
        .unwrap_or_else(|e| unreachable!("{e}"));
        // A plan through the real directory verifies…
        let real = plan(&fx, "real/data.bin");
        assert!(verify(&real).is_ok());
        // …but the same plan walked through the symlink fails to open it.
        let mut through_link = real.clone();
        through_link.components = vec![b"link".to_vec()];
        assert!(matches!(
            verify(&through_link),
            Err(Mismatch::Unreachable { .. })
        ));
        // A symlink as the leaf is a different kind of object.
        let mut leaf_link = plan(&fx, "link");
        leaf_link.expected.kind = LeafKind::Directory;
        assert_eq!(verify(&leaf_link).map(|_| ()), Err(Mismatch::KindChanged));
    }

    #[test]
    fn roots_behind_symlinks_are_refused_on_apple_platforms() {
        let fx = tree();
        let mut item = plan(&fx, "cache/app/data.bin");
        let link_parent = Fixture::build(&[Node::Symlink {
            path: "root".into(),
            target: item.root.to_string_lossy().into_owned(),
        }])
        .unwrap_or_else(|e| unreachable!("{e}"));
        item.root = std::fs::canonicalize(link_parent.root())
            .unwrap_or_else(|e| unreachable!("{e}"))
            .join("root");
        let result = verify(&item).map(|_| ());
        if cfg!(target_vendor = "apple") {
            assert!(
                matches!(
                    result,
                    Err(Mismatch::Unreachable {
                        what: "the scan root",
                        ..
                    })
                ),
                "{result:?}"
            );
        } else {
            // O_NOFOLLOW only guards the last component, which is the link here.
            assert!(result.is_err(), "{result:?}");
        }
    }

    #[test]
    fn replaced_directories_are_detected() {
        let fx = tree();
        let mut item = plan(&fx, "cache/app/data.bin");
        item.parent_identity = identity(&stat_path(fx.root()));
        assert_eq!(
            verify(&item).map(|_| ()),
            Err(Mismatch::DirectoryChanged("the parent"))
        );
        let mut root = plan(&fx, "cache/app/data.bin");
        root.root_identity.ino = root.root_identity.ino.wrapping_add(1);
        assert_eq!(
            verify(&root).map(|_| ()),
            Err(Mismatch::DirectoryChanged("the scan root"))
        );
        let mut missing = plan(&fx, "cache/app/data.bin");
        missing.leaf = b"gone".to_vec();
        assert!(matches!(
            verify(&missing),
            Err(Mismatch::Unreachable {
                what: "the item",
                ..
            })
        ));
    }
}
