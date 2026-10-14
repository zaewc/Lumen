//! Per-volume quarantine stores (roadmap 9.2, ADR-0015).
//!
//! A store is a directory on the same volume as the items it holds, so moving
//! an item in is a rename, never a copy. ADR-0015 places it at
//! `~/Library/Application Support/Lumen/Quarantine/` on the macOS data volume
//! and `<volume>/.LumenQuarantine/<uid>/` elsewhere. The caller passes the
//! existing base directory and the store's names below it.
//!
//! The base is opened by handle like a scan root (ADR-0016); each store
//! directory is created with mode 0700 if missing and opened without following
//! links. Every directory is checked:
//!
//! - it belongs to the current user;
//! - it is not writable by anyone else;
//! - it is on the expected volume.
//!
//! An existing directory that fails a check is refused, never repaired: it may
//! have been planted.

use std::os::fd::OwnedFd;
use std::path::Path;

use rustix::fs::{Mode, OFlags, fstat, mkdirat, open, openat};
use rustix::io::Errno;
use rustix::process::geteuid;

use crate::verify::{UnixIdentity, identity};

/// Why a store cannot be used. Items then go to `REVIEW`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum StoreError {
    /// A store name is empty, `.`, `..` or contains `/` or NUL.
    #[error("invalid store name")]
    InvalidName,
    /// A directory could not be opened or created.
    #[error("{what} could not be opened: {errno}")]
    Unreachable {
        /// Which directory.
        what: &'static str,
        /// The error number.
        errno: Errno,
    },
    /// The store would be on another volume than the items.
    #[error("the store is on another volume")]
    OtherVolume,
    /// A directory belongs to someone else or is writable by others.
    #[error("a store directory has unsafe ownership or permissions")]
    Unsafe,
}

/// An open quarantine store.
#[derive(Debug)]
pub struct Store {
    /// The store directory.
    pub(crate) dir: OwnedFd,
    /// Its identity.
    pub identity: UnixIdentity,
}

impl Store {
    /// The store directory's handle.
    pub fn handle(&self) -> &OwnedFd {
        &self.dir
    }
}

fn plain(name: &[u8]) -> bool {
    !name.is_empty() && name != b"." && name != b".." && !name.contains(&b'/') && !name.contains(&0)
}

fn directory_flags() -> OFlags {
    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC;
    #[cfg(target_vendor = "apple")]
    let flags = flags | OFlags::NOFOLLOW_ANY;
    #[cfg(not(target_vendor = "apple"))]
    let flags = flags | OFlags::NOFOLLOW;
    flags
}

/// Checks that `dir` is the current user's, private enough, and on `volume`.
fn check(dir: &OwnedFd, volume: u64, owner_only: bool) -> Result<UnixIdentity, StoreError> {
    let stat = fstat(dir).map_err(|errno| StoreError::Unreachable {
        what: "a store directory",
        errno,
    })?;
    let id = identity(&stat);
    if id.dev != volume {
        return Err(StoreError::OtherVolume);
    }
    let mode = Mode::from_raw_mode(stat.st_mode);
    let others_write = mode.intersects(Mode::WGRP | Mode::WOTH);
    let others_any = mode.intersects(Mode::RWXG | Mode::RWXO);
    if stat.st_uid != geteuid().as_raw() || others_write || (owner_only && others_any) {
        return Err(StoreError::Unsafe);
    }
    Ok(id)
}

/// Opens the store `names` below `base` on `volume` (a device number),
/// creating missing directories with mode 0700.
///
/// # Errors
///
/// A [`StoreError`] if a name is invalid, a directory cannot be opened or
/// created, or any directory fails its checks.
pub fn open_store(base: &Path, names: &[&[u8]], volume: u64) -> Result<Store, StoreError> {
    if names.is_empty() || !names.iter().all(|n| plain(n)) {
        return Err(StoreError::InvalidName);
    }
    let mut dir =
        open(base, directory_flags(), Mode::empty()).map_err(|errno| StoreError::Unreachable {
            what: "the store base",
            errno,
        })?;
    // The base may be shared (e.g. a volume root) but must not be writable by
    // others, or someone could swap the store's parent.
    let mut id = check(&dir, volume, false)?;
    for name in names {
        match mkdirat(&dir, *name, Mode::RWXU) {
            Ok(()) | Err(Errno::EXIST) => {}
            Err(errno) => {
                return Err(StoreError::Unreachable {
                    what: "a store directory",
                    errno,
                });
            }
        }
        let next = openat(&dir, *name, directory_flags(), Mode::empty()).map_err(|errno| {
            StoreError::Unreachable {
                what: "a store directory",
                errno,
            }
        })?;
        id = check(&next, volume, true)?;
        dir = next;
    }
    Ok(Store { dir, identity: id })
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt as _;
    use std::path::PathBuf;

    use lumen_testkit::{Fixture, Node};

    use super::*;

    fn base(fx: &Fixture) -> PathBuf {
        std::fs::canonicalize(fx.root()).unwrap_or_else(|e| unreachable!("{e}"))
    }

    fn volume(path: &Path) -> u64 {
        identity(&rustix::fs::stat(path).unwrap_or_else(|e| unreachable!("{e}"))).dev
    }

    fn private_base() -> Fixture {
        let fx = Fixture::build(&[]).unwrap_or_else(|e| unreachable!("{e}"));
        let mut perms = std::fs::metadata(fx.root())
            .unwrap_or_else(|e| unreachable!("{e}"))
            .permissions();
        perms.set_mode(0o755);
        #[allow(
            clippy::disallowed_methods,
            reason = "test fixture inside its own temporary directory"
        )]
        std::fs::set_permissions(fx.root(), perms).unwrap_or_else(|e| unreachable!("{e}"));
        fx
    }

    #[test]
    fn creates_private_store_directories_and_reopens_them() {
        let fx = private_base();
        let base = base(&fx);
        let names: [&[u8]; 2] = [b"Lumen", b"Quarantine"];
        let store =
            open_store(&base, &names, volume(&base)).unwrap_or_else(|e| unreachable!("{e}"));
        let path = base.join("Lumen/Quarantine");
        let mode = std::fs::metadata(&path)
            .unwrap_or_else(|e| unreachable!("{e}"))
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o700);
        assert_eq!(
            store.identity,
            identity(&rustix::fs::stat(&path).unwrap_or_else(|e| unreachable!("{e}")))
        );
        let again =
            open_store(&base, &names, volume(&base)).unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(again.identity, store.identity);
    }

    #[test]
    fn unsafe_or_foreign_stores_are_refused() {
        let fx = private_base();
        let base = base(&fx);
        let names: [&[u8]; 1] = [b"Quarantine"];
        assert_eq!(
            open_store(&base, &names, volume(&base).wrapping_add(1)).map(|_| ()),
            Err(StoreError::OtherVolume)
        );
        assert_eq!(
            open_store(&base, &[b"..".as_slice()], volume(&base)).map(|_| ()),
            Err(StoreError::InvalidName)
        );
        // A pre-existing store directory readable by others is not adopted.
        let shared = base.join("Shared");
        std::fs::create_dir(&shared).unwrap_or_else(|e| unreachable!("{e}"));
        let mut perms = std::fs::metadata(&shared)
            .unwrap_or_else(|e| unreachable!("{e}"))
            .permissions();
        perms.set_mode(0o755);
        #[allow(
            clippy::disallowed_methods,
            reason = "test fixture inside its own temporary directory"
        )]
        std::fs::set_permissions(&shared, perms).unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(
            open_store(&base, &[b"Shared".as_slice()], volume(&base)).map(|_| ()),
            Err(StoreError::Unsafe)
        );
    }

    #[test]
    fn a_planted_symlink_is_not_followed() {
        let fx = Fixture::build(&[Node::Symlink {
            path: "Quarantine".into(),
            target: "/tmp".into(),
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
        let base = base(&fx);
        assert!(matches!(
            open_store(&base, &[b"Quarantine".as_slice()], volume(&base)),
            Err(StoreError::Unreachable { .. })
        ));
    }
}
