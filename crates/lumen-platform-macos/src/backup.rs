//! Excluding quarantine stores from Time Machine (roadmap 9.2b, ADR-0015).
//!
//! Quarantined items are files the user chose to remove; backing them up again
//! during retention would grow every backup with them. A store is excluded
//! with `CSBackupSetItemExcluded` as a sticky exclusion (`excludeByPath`
//! false): Apple documents that an excluded folder excludes all its contents,
//! and the sticky form needs no administrator rights.
//! `NSURLIsExcludedFromBackupKey` is the Foundation spelling of the same flag.
//! Apple notes that common file operations can reset it, so callers reapply
//! it each time a store is opened; it is idempotent.
//!
//! The API takes a URL, not a handle, so the path is read back from the
//! store's own handle (`F_GETPATH`) and the directory at that path is compared
//! with the handle by identity before and after the call. A mismatch is an
//! error and nothing is retried: the store may have been swapped. A same-user
//! process able to swap the store could exclude anything itself with `tmutil`,
//! so the check detects races; it does not defend a privilege boundary.
//!
//! Spotlight has no documented programmatic exclusion for a directory (only
//! the user's Search Privacy list), so stores are not hidden from indexing;
//! see `docs/research/10-filesystem-scanning-quarantine-duplicates.md`.

#![allow(unsafe_code)]

use std::ffi::c_int;
use std::fs::{File, OpenOptions};
use std::io;
use std::os::fd::{AsRawFd as _, BorrowedFd};
use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::fs::{MetadataExt as _, OpenOptionsExt as _};
use std::path::Path;

use objc2_core_foundation::{CFIndex, CFURL};

/// `Boolean` in `MacTypes.h`.
type Boolean = u8;

#[link(name = "CoreServices", kind = "framework")]
unsafe extern "C" {
    // CarbonCore/BackupCore.h; `objc2-core-services` does not bind these.
    fn CSBackupSetItemExcluded(item: &CFURL, exclude: Boolean, exclude_by_path: Boolean) -> i32;
    fn CSBackupIsItemExcluded(item: &CFURL, exclude_by_path: *mut Boolean) -> Boolean;
}

/// `MAXPATHLEN` in `sys/param.h`: the buffer size `F_GETPATH` requires.
const MAXPATHLEN: usize = 1024;

/// Why a store could not be excluded from backups.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum BackupExclusionError {
    /// The store's path could not be read from its handle.
    #[error("the store's path could not be read (errno {0})")]
    Path(i32),
    /// The path no longer leads to the store.
    #[error("the store's path no longer leads to the store")]
    Swapped,
    /// `CSBackupSetItemExcluded` failed.
    #[error("CSBackupSetItemExcluded failed with status {0}")]
    Status(i32),
    /// The call succeeded but the store is not reported as excluded.
    #[error("the store is still included in backups")]
    NotExcluded,
}

fn errno(error: &io::Error) -> i32 {
    error.raw_os_error().unwrap_or(0)
}

/// The path the kernel holds for `dir`.
fn path_of(dir: BorrowedFd<'_>) -> Result<Vec<u8>, BackupExclusionError> {
    let mut buffer = [0_u8; MAXPATHLEN];
    // SAFETY: invariant: `F_GETPATH` writes a NUL-terminated path of at most
    // `MAXPATHLEN` bytes into its argument. Why unsafe: FFI call with a raw
    // pointer. Why it holds: `buffer` is `MAXPATHLEN` bytes, owned and live
    // for the call, and `dir` is an open descriptor.
    let result = unsafe { libc::fcntl(dir.as_raw_fd(), libc::F_GETPATH, buffer.as_mut_ptr()) };
    if result == -1 {
        return Err(BackupExclusionError::Path(errno(
            &io::Error::last_os_error(),
        )));
    }
    let length = buffer
        .iter()
        .position(|&b| b == 0)
        .ok_or(BackupExclusionError::Path(libc::ENAMETOOLONG))?;
    Ok(buffer.get(..length).unwrap_or_default().to_vec())
}

fn identity(file: &File) -> Option<(u64, u64)> {
    file.metadata().ok().map(|m| (m.dev(), m.ino()))
}

/// Whether `path` still leads, without following any link, to `expected`.
fn leads_to(path: &[u8], expected: (u64, u64)) -> bool {
    let flags: c_int = libc::O_DIRECTORY | libc::O_NOFOLLOW_ANY | libc::O_CLOEXEC;
    OpenOptions::new()
        .read(true)
        .custom_flags(flags)
        .open(Path::new(std::ffi::OsStr::from_bytes(path)))
        .ok()
        .and_then(|file| identity(&file))
        == Some(expected)
}

/// Excludes the store directory `dir` (and everything in it) from Time
/// Machine backups.
///
/// # Errors
///
/// [`BackupExclusionError`]; the store is still usable, only backed up.
pub fn exclude_from_backup(dir: BorrowedFd<'_>) -> Result<(), BackupExclusionError> {
    let handle = File::from(
        dir.try_clone_to_owned()
            .map_err(|e| BackupExclusionError::Path(errno(&e)))?,
    );
    let expected = identity(&handle).ok_or(BackupExclusionError::Swapped)?;
    let path = path_of(dir)?;
    if !leads_to(&path, expected) {
        return Err(BackupExclusionError::Swapped);
    }
    let length = CFIndex::try_from(path.len()).map_err(|_| BackupExclusionError::Swapped)?;
    // SAFETY: invariant: the buffer holds `length` readable bytes. Why unsafe:
    // the binding takes a raw pointer. Why it holds: `path` is a live `Vec` of
    // exactly `length` bytes, and the default allocator (`None`) is allowed.
    let url = unsafe { CFURL::from_file_system_representation(None, path.as_ptr(), length, true) }
        .ok_or(BackupExclusionError::Swapped)?;
    // SAFETY: invariant: `item` is a valid `CFURL`. Why unsafe: FFI call. Why
    // it holds: `url` is a retained `CFURL` alive for the call.
    let status = unsafe { CSBackupSetItemExcluded(&url, 1, 0) };
    if status != 0 {
        return Err(BackupExclusionError::Status(status));
    }
    if !leads_to(&path, expected) {
        return Err(BackupExclusionError::Swapped);
    }
    // SAFETY: invariant: `item` is a valid `CFURL` and `exclude_by_path` is
    // null or writable. Why unsafe: FFI call. Why it holds: `url` is alive and
    // the out-parameter is null, which the API allows.
    if unsafe { CSBackupIsItemExcluded(&url, std::ptr::null_mut()) } == 0 {
        return Err(BackupExclusionError::NotExcluded);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::os::fd::AsFd as _;

    use lumen_testkit::Fixture;

    use super::*;

    /// The names of `path`'s extended attributes.
    fn attribute_names(path: &Path) -> Vec<u8> {
        let path = std::ffi::CString::new(path.as_os_str().as_bytes())
            .unwrap_or_else(|e| unreachable!("{e}"));
        let mut buffer = vec![0_u8; 4096];
        // SAFETY: invariant: `listxattr` reads the NUL-terminated `path` and
        // writes at most `buffer.len()` bytes into `buffer`. Why unsafe: FFI
        // call with raw pointers. Why it holds: both are owned and live.
        let length = unsafe {
            libc::listxattr(
                path.as_ptr(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                libc::XATTR_NOFOLLOW,
            )
        };
        buffer.truncate(usize::try_from(length).unwrap_or_else(|e| unreachable!("{e}")));
        buffer
    }

    #[test]
    fn a_store_directory_is_excluded_from_backups() {
        let fx = Fixture::build(&[]).unwrap_or_else(|e| unreachable!("{e}"));
        let store = File::open(fx.root()).unwrap_or_else(|e| unreachable!("{e}"));
        let before = attribute_names(fx.root());
        assert_eq!(exclude_from_backup(store.as_fd()), Ok(()));
        // Temporary directories are excluded by location already, so
        // `CSBackupIsItemExcluded` alone proves nothing here; a sticky
        // exclusion is recorded on the item itself (`tmutil(8)`).
        assert_ne!(attribute_names(fx.root()), before);
    }

    #[test]
    fn a_handle_whose_directory_is_gone_is_refused() {
        let fx = Fixture::build(&[]).unwrap_or_else(|e| unreachable!("{e}"));
        let dir = fx.root().join("store");
        std::fs::create_dir(&dir).unwrap_or_else(|e| unreachable!("{e}"));
        let store = File::open(&dir).unwrap_or_else(|e| unreachable!("{e}"));
        #[allow(
            clippy::disallowed_methods,
            reason = "test fixture inside its own temporary directory"
        )]
        std::fs::remove_dir(&dir).unwrap_or_else(|e| unreachable!("{e}"));
        assert!(matches!(
            exclude_from_backup(store.as_fd()),
            Err(BackupExclusionError::Swapped | BackupExclusionError::Path(_))
        ));
    }
}
