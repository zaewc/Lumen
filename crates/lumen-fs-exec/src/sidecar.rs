//! Manifest sidecars next to quarantined items (roadmap 9.5b, ADR-0015).
//!
//! Each item's manifest is written as `<item>.manifest.json` in the same store,
//! so the stores alone are enough to rebuild a lost ledger and restore every
//! item. A sidecar is created exclusively (`O_CREAT | O_EXCL`, mode 0600,
//! never through a link) and flushed with its directory; it is never
//! overwritten. The bytes are opaque here: the caller serializes the
//! `quarantine.manifest/1` record.

use std::os::fd::OwnedFd;

use rustix::fs::{Mode, OFlags, fsync, openat};
use rustix::io::{Errno, read, write};

use crate::store::Store;

/// Largest sidecar read back; real manifests are a few kilobytes.
pub const MAX_SIDECAR_BYTES: usize = 1024 * 1024;

/// Why a sidecar could not be written or read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SidecarError {
    /// The item name is empty, `.`, `..` or contains `/` or NUL.
    #[error("invalid item name")]
    InvalidName,
    /// A sidecar already exists; it is never overwritten.
    #[error("the sidecar already exists")]
    Exists,
    /// The sidecar is larger than [`MAX_SIDECAR_BYTES`].
    #[error("the sidecar is too large")]
    TooLarge,
    /// A system call failed.
    #[error("sidecar I/O failed: {0}")]
    Io(Errno),
}

fn plain(name: &[u8]) -> bool {
    !name.is_empty() && name != b"." && name != b".." && !name.contains(&b'/') && !name.contains(&0)
}

fn sidecar_name(item: &[u8]) -> Result<Vec<u8>, SidecarError> {
    if !plain(item) {
        return Err(SidecarError::InvalidName);
    }
    let mut name = item.to_vec();
    name.extend_from_slice(b".manifest.json");
    Ok(name)
}

fn no_follow() -> OFlags {
    #[cfg(target_vendor = "apple")]
    let flag = OFlags::NOFOLLOW_ANY;
    #[cfg(not(target_vendor = "apple"))]
    let flag = OFlags::NOFOLLOW;
    flag
}

/// Writes `bytes` as the sidecar of `item` in `store`.
///
/// # Errors
///
/// [`SidecarError::Exists`] if a sidecar is already there, or another
/// [`SidecarError`].
pub fn write_sidecar(store: &Store, item: &[u8], bytes: &[u8]) -> Result<(), SidecarError> {
    let name = sidecar_name(item)?;
    let file: OwnedFd = openat(
        store.handle(),
        name.as_slice(),
        OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::CLOEXEC | no_follow(),
        Mode::RUSR | Mode::WUSR,
    )
    .map_err(|errno| match errno {
        Errno::EXIST => SidecarError::Exists,
        other => SidecarError::Io(other),
    })?;
    let mut rest = bytes;
    while !rest.is_empty() {
        match write(&file, rest) {
            Ok(0) => return Err(SidecarError::Io(Errno::IO)),
            Ok(n) => rest = rest.get(n..).unwrap_or_default(),
            Err(Errno::INTR) => {}
            Err(errno) => return Err(SidecarError::Io(errno)),
        }
    }
    fsync(&file).map_err(SidecarError::Io)?;
    fsync(store.handle()).map_err(SidecarError::Io)
}

/// Reads the sidecar of `item` in `store`.
///
/// # Errors
///
/// [`SidecarError::TooLarge`] past [`MAX_SIDECAR_BYTES`], or another
/// [`SidecarError`] (a missing sidecar is `Io(NOENT)`).
pub fn read_sidecar(store: &Store, item: &[u8]) -> Result<Vec<u8>, SidecarError> {
    let name = sidecar_name(item)?;
    let file = openat(
        store.handle(),
        name.as_slice(),
        OFlags::RDONLY | OFlags::CLOEXEC | no_follow(),
        Mode::empty(),
    )
    .map_err(SidecarError::Io)?;
    let mut out = Vec::new();
    let mut buffer = [0_u8; 8192];
    loop {
        match read(&file, &mut buffer) {
            Ok(0) => return Ok(out),
            Ok(n) => {
                out.extend_from_slice(buffer.get(..n).unwrap_or_default());
                if out.len() > MAX_SIDECAR_BYTES {
                    return Err(SidecarError::TooLarge);
                }
            }
            Err(Errno::INTR) => {}
            Err(errno) => return Err(SidecarError::Io(errno)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt as _;

    use lumen_testkit::Fixture;

    use super::*;
    use crate::store::open_store;
    use crate::verify::identity;

    fn store() -> (Fixture, std::path::PathBuf, Store) {
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
        let root = std::fs::canonicalize(fx.root()).unwrap_or_else(|e| unreachable!("{e}"));
        let dev = identity(&rustix::fs::stat(&root).unwrap_or_else(|e| unreachable!("{e}"))).dev;
        let store = open_store(&root, &[b"Quarantine".as_slice()], dev)
            .unwrap_or_else(|e| unreachable!("{e}"));
        (fx, root, store)
    }

    #[test]
    fn sidecars_are_written_privately_and_read_back() {
        let (_fx, root, store) = store();
        let body = br#"{"schema_version":"quarantine.manifest/1.0"}"#;
        assert_eq!(write_sidecar(&store, b"op-1", body), Ok(()));
        assert_eq!(read_sidecar(&store, b"op-1"), Ok(body.to_vec()));
        let mode = std::fs::metadata(root.join("Quarantine/op-1.manifest.json"))
            .unwrap_or_else(|e| unreachable!("{e}"))
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn sidecars_are_never_overwritten() {
        let (_fx, _root, store) = store();
        assert_eq!(write_sidecar(&store, b"op-1", b"first"), Ok(()));
        assert_eq!(
            write_sidecar(&store, b"op-1", b"second"),
            Err(SidecarError::Exists)
        );
        assert_eq!(read_sidecar(&store, b"op-1"), Ok(b"first".to_vec()));
    }

    #[test]
    fn bad_names_missing_and_oversized_sidecars_are_errors() {
        let (_fx, root, store) = store();
        for name in [b"".as_slice(), b"..", b"a/b"] {
            assert_eq!(
                write_sidecar(&store, name, b"x"),
                Err(SidecarError::InvalidName)
            );
        }
        assert_eq!(
            read_sidecar(&store, b"missing"),
            Err(SidecarError::Io(Errno::NOENT))
        );
        std::fs::write(
            root.join("Quarantine/big.manifest.json"),
            vec![b'x'; MAX_SIDECAR_BYTES + 1],
        )
        .unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(read_sidecar(&store, b"big"), Err(SidecarError::TooLarge));
    }
}
