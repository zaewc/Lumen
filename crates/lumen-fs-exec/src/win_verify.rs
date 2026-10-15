//! Re-verifying a plan item by handle on Windows (roadmap 9.10a, ADR-0016,
//! ADR-0033).
//!
//! The walk opens the scan root, then each component relative to the previous
//! handle, one name at a time and never through a reparse point (a junction,
//! symbolic link, mount point or placeholder is refused, not followed). Every
//! directory must be on the scan root's volume. The leaf is opened the same
//! way, ready to be renamed, and compared with the plan by identity (volume
//! serial and 128-bit file ID), kind, size, last write time and link count.

use std::io;

use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

use crate::win_sys::{self, Facts, Handle, Want};

/// An object's identity on Windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WinIdentity {
    /// Volume serial number.
    pub volume: u64,
    /// 128-bit file ID.
    pub file: u128,
}

/// What the plan expects the item to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WinExpected {
    /// Its identity.
    pub identity: WinIdentity,
    /// A directory (otherwise a regular file).
    pub directory: bool,
    /// Logical size of a file (ignored for directories).
    pub size: i64,
    /// Last write time.
    pub modified: i64,
    /// Hard link count.
    pub links: u32,
}

/// One item of a confirmed plan, as observed when it was planned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WinPlanItem {
    /// The scan root: a full, verbatim (`\\?\`) path without a NUL.
    pub root: Vec<u16>,
    /// The scan root's identity.
    pub root_identity: WinIdentity,
    /// Directory names from the root to the item's parent.
    pub components: Vec<Vec<u16>>,
    /// The parent's identity.
    pub parent_identity: WinIdentity,
    /// The item's own name.
    pub leaf: Vec<u16>,
    /// What the item must still be.
    pub expected: WinExpected,
}

/// Why a plan item no longer matches. The item stays where it is.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum WinMismatch {
    /// A name is empty or contains a separator or NUL.
    #[error("invalid name in the plan")]
    InvalidName,
    /// Something could not be opened or queried.
    #[error("{what} could not be opened: OS error {code}")]
    Unreachable {
        /// Which object.
        what: &'static str,
        /// The Win32 error code.
        code: i32,
    },
    /// A directory on the way is on another volume.
    #[error("the path crosses into another volume")]
    CrossesVolume,
    /// Something on the way, or the item, is a reparse point.
    #[error("{0} is a reparse point")]
    ReparsePoint(&'static str),
    /// A directory is no longer the one planned.
    #[error("{0} was replaced")]
    DirectoryChanged(&'static str),
    /// The item is another object now.
    #[error("the item was replaced")]
    IdentityChanged,
    /// The item changed between file and directory.
    #[error("the item changed kind")]
    KindChanged,
    /// The file's size changed.
    #[error("the item's size changed")]
    SizeChanged,
    /// The item was written to.
    #[error("the item was modified")]
    Modified,
    /// The item gained or lost hard links.
    #[error("the item's link count changed")]
    LinksChanged,
}

/// A verified item: its parent's handle and its own, ready to be renamed.
#[derive(Debug)]
pub struct WinVerified {
    /// The parent directory.
    pub(crate) parent: Handle,
    /// The item itself, opened for `DELETE`.
    pub(crate) leaf: Handle,
    /// The item's name in its parent.
    pub(crate) name: Vec<u16>,
}

pub(crate) const fn identity(facts: &Facts) -> WinIdentity {
    WinIdentity {
        volume: facts.volume,
        file: facts.file,
    }
}

pub(crate) const fn is_reparse(facts: &Facts) -> bool {
    facts.attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

fn plain(name: &[u16]) -> bool {
    !name.is_empty()
        && name != [u16::from(b'.')]
        && name != [u16::from(b'.'), u16::from(b'.')]
        && !name
            .iter()
            .any(|&u| u == 0 || u == u16::from(b'\\') || u == u16::from(b'/'))
}

fn code(error: &io::Error) -> i32 {
    error.raw_os_error().unwrap_or(0)
}

fn unreachable(what: &'static str) -> impl Fn(io::Error) -> WinMismatch {
    move |error| WinMismatch::Unreachable {
        what,
        code: code(&error),
    }
}

/// Opens the root and walks to the parent, checking every directory.
pub(crate) fn walk(root: &[u16], components: &[Vec<u16>]) -> Result<(Facts, Handle), WinMismatch> {
    let mut path = root.to_vec();
    path.push(0);
    let mut dir = win_sys::open_root(&path).map_err(unreachable("the scan root"))?;
    let root_facts = win_sys::facts(&dir).map_err(unreachable("the scan root"))?;
    if is_reparse(&root_facts) || !root_facts.directory {
        return Err(WinMismatch::ReparsePoint("the scan root"));
    }
    for component in components {
        dir = win_sys::open_at(&dir, component, Want::Directory)
            .map_err(unreachable("a directory on the way"))?;
        let facts = win_sys::facts(&dir).map_err(unreachable("a directory on the way"))?;
        if is_reparse(&facts) {
            return Err(WinMismatch::ReparsePoint("a directory on the way"));
        }
        if facts.volume != root_facts.volume {
            return Err(WinMismatch::CrossesVolume);
        }
    }
    Ok((root_facts, dir))
}

/// Compares the item's facts with the plan.
pub(crate) fn check_leaf(facts: &Facts, expected: &WinExpected) -> Result<(), WinMismatch> {
    if is_reparse(facts) {
        return Err(WinMismatch::ReparsePoint("the item"));
    }
    if identity(facts) != expected.identity {
        return Err(WinMismatch::IdentityChanged);
    }
    if facts.directory != expected.directory {
        return Err(WinMismatch::KindChanged);
    }
    if !facts.directory && facts.size != expected.size {
        return Err(WinMismatch::SizeChanged);
    }
    if facts.modified != expected.modified {
        return Err(WinMismatch::Modified);
    }
    if facts.links != expected.links {
        return Err(WinMismatch::LinksChanged);
    }
    Ok(())
}

/// Checks an item held in a store: a file in full, a directory by identity
/// and kind (an interrupted finalization may have changed its contents; the
/// store is private to its owner, ADR-0015).
pub(crate) fn check_held(facts: &Facts, expected: &WinExpected) -> Result<(), WinMismatch> {
    if !expected.directory {
        return check_leaf(facts, expected);
    }
    if is_reparse(facts) {
        return Err(WinMismatch::ReparsePoint("the item"));
    }
    if identity(facts) != expected.identity {
        return Err(WinMismatch::IdentityChanged);
    }
    if facts.directory {
        Ok(())
    } else {
        Err(WinMismatch::KindChanged)
    }
}

/// Records `relative` below `root` as it is now.
///
/// # Errors
///
/// A [`WinMismatch`] if it cannot be reached safely.
pub fn observe(root: &[u16], relative: &[Vec<u16>]) -> Result<WinPlanItem, WinMismatch> {
    let (leaf, components) = relative.split_last().ok_or(WinMismatch::InvalidName)?;
    if !components.iter().all(|c| plain(c)) || !plain(leaf) {
        return Err(WinMismatch::InvalidName);
    }
    let (root_facts, parent) = walk(root, components)?;
    let parent_facts = win_sys::facts(&parent).map_err(unreachable("the parent"))?;
    // The kind is not known yet: try a file, then a directory.
    let item = win_sys::open_at(&parent, leaf, Want::Movable { directory: false })
        .or_else(|_| win_sys::open_at(&parent, leaf, Want::Movable { directory: true }))
        .map_err(unreachable("the item"))?;
    let facts = win_sys::facts(&item).map_err(unreachable("the item"))?;
    if is_reparse(&facts) {
        return Err(WinMismatch::ReparsePoint("the item"));
    }
    Ok(WinPlanItem {
        root: root.to_vec(),
        root_identity: identity(&root_facts),
        components: components.to_vec(),
        parent_identity: identity(&parent_facts),
        leaf: leaf.clone(),
        expected: WinExpected {
            identity: identity(&facts),
            directory: facts.directory,
            size: facts.size,
            modified: facts.modified,
            links: facts.links,
        },
    })
}

/// Re-verifies `item` by handle from its scan root.
///
/// # Errors
///
/// The first [`WinMismatch`] found; nothing is changed.
pub fn verify(item: &WinPlanItem) -> Result<WinVerified, WinMismatch> {
    if !item.components.iter().all(|c| plain(c)) || !plain(&item.leaf) {
        return Err(WinMismatch::InvalidName);
    }
    let (root_facts, parent) = walk(&item.root, &item.components)?;
    if identity(&root_facts) != item.root_identity {
        return Err(WinMismatch::DirectoryChanged("the scan root"));
    }
    let parent_facts = win_sys::facts(&parent).map_err(unreachable("the parent"))?;
    if identity(&parent_facts) != item.parent_identity {
        return Err(WinMismatch::DirectoryChanged("the parent"));
    }
    let want = Want::Movable {
        directory: item.expected.directory,
    };
    let leaf = win_sys::open_at(&parent, &item.leaf, want).map_err(unreachable("the item"))?;
    let facts = win_sys::facts(&leaf).map_err(unreachable("the item"))?;
    check_leaf(&facts, &item.expected)?;
    Ok(WinVerified {
        parent,
        leaf,
        name: item.leaf.clone(),
    })
}

#[cfg(test)]
mod tests {
    use std::os::windows::ffi::OsStrExt as _;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    use lumen_testkit::{Fixture, Node};

    use super::*;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    /// A fixture with `cache/app/data.bin` and `elsewhere/data.bin`, and its
    /// verbatim root.
    fn setup() -> (Fixture, PathBuf, Vec<u16>) {
        let fx = Fixture::build(&[
            Node::File {
                path: "cache/app/data.bin".into(),
                contents: b"0123456789".to_vec(),
            },
            Node::File {
                path: "elsewhere/data.bin".into(),
                contents: b"not yours".to_vec(),
            },
        ])
        .unwrap_or_else(|e| unreachable!("{e}"));
        // `canonicalize` returns the verbatim `\\?\` form.
        let root = std::fs::canonicalize(fx.root()).unwrap_or_else(|e| unreachable!("{e}"));
        let units = root.as_os_str().encode_wide().collect();
        (fx, root, units)
    }

    fn relative() -> Vec<Vec<u16>> {
        vec![wide("cache"), wide("app"), wide("data.bin")]
    }

    fn rename(from: &Path, to: &Path) {
        #[allow(
            clippy::disallowed_methods,
            reason = "test fixture inside its own temporary directory"
        )]
        std::fs::rename(from, to).unwrap_or_else(|e| unreachable!("{e}"));
    }

    #[test]
    fn an_unchanged_item_verifies() {
        let (_fx, _root, units) = setup();
        let item = observe(&units, &relative()).unwrap_or_else(|e| unreachable!("{e}"));
        assert!(!item.expected.directory);
        assert_eq!(item.expected.size, 10);
        assert!(verify(&item).is_ok());
    }

    #[test]
    fn changes_to_the_item_are_detected() {
        let (_fx, root, units) = setup();
        let item = observe(&units, &relative()).unwrap_or_else(|e| unreachable!("{e}"));
        std::fs::write(root.join("cache/app/data.bin"), b"01234567890")
            .unwrap_or_else(|e| unreachable!("{e}"));
        assert!(matches!(
            verify(&item).err(),
            Some(WinMismatch::SizeChanged | WinMismatch::Modified)
        ));
        rename(
            &root.join("cache/app/data.bin"),
            &root.join("cache/app/old.bin"),
        );
        std::fs::write(root.join("cache/app/data.bin"), b"0123456789")
            .unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(verify(&item).err(), Some(WinMismatch::IdentityChanged));
    }

    #[test]
    fn a_junction_on_the_way_is_never_followed() {
        let (_fx, root, units) = setup();
        let item = observe(&units, &relative()).unwrap_or_else(|e| unreachable!("{e}"));
        rename(&root.join("cache/app"), &root.join("cache/app.old"));
        let status = Command::new("cmd")
            .arg("/C")
            .arg("mklink")
            .arg("/J")
            .arg(root.join("cache").join("app"))
            .arg(root.join("elsewhere"))
            .status()
            .unwrap_or_else(|e| unreachable!("{e}"));
        assert!(status.success(), "mklink /J failed");
        assert_eq!(
            verify(&item).err(),
            Some(WinMismatch::ReparsePoint("a directory on the way"))
        );
        assert!(
            matches!(
                observe(&units, &relative()),
                Err(WinMismatch::ReparsePoint(_))
            ),
            "a junction is never planned through either"
        );
    }

    #[test]
    fn invalid_names_are_refused() {
        let (_fx, _root, units) = setup();
        for name in [wide(""), wide(".."), wide("a\\b"), wide("a/b")] {
            assert_eq!(
                observe(&units, &[wide("cache"), name]).err(),
                Some(WinMismatch::InvalidName)
            );
        }
    }
}
