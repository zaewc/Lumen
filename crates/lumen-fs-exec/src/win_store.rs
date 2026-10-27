//! Per-volume quarantine stores on Windows (roadmap 9.10b, ADR-0015,
//! ADR-0033).
//!
//! The caller passes an existing base directory (a full verbatim path) and the
//! store's names below it. Each store directory is opened by handle relative
//! to the previous one; a missing one is created with its security descriptor
//! set at creation: the current user as owner and a protected DACL granting
//! full control to that user and SYSTEM only, inherited by everything inside,
//! and marked not content-indexed. Every directory is checked:
//!
//! - it is not a reparse point;
//! - it is on the expected volume;
//! - its owner and DACL are exactly the private ones above.
//!
//! An existing directory that fails a check is refused, never repaired: it may
//! have been planted. Shared parents (such as `X:\$LumenQuarantine` for all
//! users of a volume) are the caller's base, not store names.

use std::io;

use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND};
use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

use crate::win_sys::{self, Handle, Want};
use crate::win_verify::WinIdentity;

/// Why a store cannot be used. Items then go to `REVIEW`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum WinStoreError {
    /// A store name is empty, `.`, `..` or contains a separator or NUL.
    #[error("invalid store name")]
    InvalidName,
    /// A directory could not be opened, created or inspected.
    #[error("{what} could not be opened: OS error {code}")]
    Unreachable {
        /// Which directory.
        what: &'static str,
        /// The Win32 error code.
        code: i32,
    },
    /// The store would be on another volume than the items.
    #[error("the store is on another volume")]
    OtherVolume,
    /// A directory is a reparse point, or its owner or DACL is not private.
    #[error("a store directory is not private to its owner")]
    Unsafe,
}

/// An open quarantine store.
#[derive(Debug)]
pub struct WinStore {
    /// The store directory.
    pub(crate) dir: Handle,
    /// Its identity.
    pub identity: WinIdentity,
}

fn plain(name: &[u16]) -> bool {
    !name.is_empty()
        && name != [u16::from(b'.')]
        && name != [u16::from(b'.'), u16::from(b'.')]
        && !name
            .iter()
            .any(|&u| u == 0 || u == u16::from(b'\\') || u == u16::from(b'/'))
}

fn unreachable(what: &'static str) -> impl Fn(io::Error) -> WinStoreError {
    move |error| WinStoreError::Unreachable {
        what,
        code: error.raw_os_error().unwrap_or(0),
    }
}

/// The owner and protected DACL every store directory has.
fn private_sddl(user: &str) -> String {
    format!("O:{user}D:P(A;OICI;FA;;;{user})(A;OICI;FA;;;SY)")
}

/// Whether `actual` (as the system prints it) is the private descriptor.
/// The expected one is printed by the system too, so well-known SIDs use the
/// same aliases (`LA` for the built-in Administrator, `SY`). Windows may
/// report the DACL as auto-inherited (`PAI`) as well as protected; nothing
/// else is accepted.
fn is_private(actual: &str, user: &str) -> bool {
    let Ok(expected) = win_sys::canonical_sddl(&private_sddl(user)) else {
        return false;
    };
    actual == expected || actual == expected.replacen("D:P(", "D:PAI(", 1)
}

fn missing(error: &io::Error) -> bool {
    matches!(
        error.raw_os_error().and_then(|c| u32::try_from(c).ok()),
        Some(ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND)
    )
}

/// Opens or creates `names` below `base`, all private, on `volume`.
///
/// # Errors
///
/// A [`WinStoreError`]; nothing that exists is changed.
pub fn open_store(base: &[u16], names: &[&[u16]], volume: u64) -> Result<WinStore, WinStoreError> {
    if names.is_empty() || !names.iter().all(|n| plain(n)) {
        return Err(WinStoreError::InvalidName);
    }
    let user = win_sys::current_user_sid().map_err(unreachable("the current user"))?;
    let sddl = private_sddl(&user);
    let mut path = base.to_vec();
    path.push(0);
    let mut dir = win_sys::open_root(&path).map_err(unreachable("the store base"))?;
    let base_facts = win_sys::facts(&dir).map_err(unreachable("the store base"))?;
    if base_facts.attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 || !base_facts.directory {
        return Err(WinStoreError::Unsafe);
    }
    // A store is only ever created on the volume of the items it is for
    // (ADR-0046): nothing is created below a base on another volume.
    if base_facts.volume != volume {
        return Err(WinStoreError::OtherVolume);
    }
    for name in names {
        dir = match win_sys::open_at(&dir, name, Want::Store) {
            Ok(existing) => existing,
            Err(error) if missing(&error) => win_sys::create_dir_at(&dir, name, &sddl)
                .map_err(unreachable("a store directory"))?,
            Err(error) => return Err(unreachable("a store directory")(error)),
        };
        let facts = win_sys::facts(&dir).map_err(unreachable("a store directory"))?;
        if facts.attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 || !facts.directory {
            return Err(WinStoreError::Unsafe);
        }
        if facts.volume != volume {
            return Err(WinStoreError::OtherVolume);
        }
        let actual = win_sys::owner_and_dacl(&dir).map_err(unreachable("a store directory"))?;
        if !is_private(&actual, &user) {
            return Err(WinStoreError::Unsafe);
        }
    }
    let facts = win_sys::facts(&dir).map_err(unreachable("the store"))?;
    Ok(WinStore {
        identity: WinIdentity {
            volume: facts.volume,
            file: facts.file,
        },
        dir,
    })
}

#[cfg(test)]
mod tests {
    use std::os::windows::ffi::OsStrExt as _;
    use std::path::PathBuf;
    use std::process::Command;

    use lumen_testkit::Fixture;

    use super::*;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    fn setup() -> (Fixture, PathBuf, Vec<u16>, u64) {
        let fx = Fixture::build(&[]).unwrap_or_else(|e| unreachable!("{e}"));
        let root = std::fs::canonicalize(fx.root()).unwrap_or_else(|e| unreachable!("{e}"));
        let units: Vec<u16> = root.as_os_str().encode_wide().collect();
        let mut path = units.clone();
        path.push(0);
        let volume = win_sys::open_root(&path)
            .and_then(|h| win_sys::facts(&h))
            .unwrap_or_else(|e| unreachable!("{e}"))
            .volume;
        (fx, root, units, volume)
    }

    #[test]
    fn a_new_store_is_private_and_reopens() {
        let (_fx, root, base, volume) = setup();
        let name = wide("Quarantine");
        let first = open_store(&base, &[&name], volume).unwrap_or_else(|e| unreachable!("{e}"));
        assert!(root.join("Quarantine").is_dir());
        let again = open_store(&base, &[&name], volume).unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(first.identity, again.identity);
    }

    #[test]
    fn a_created_directory_reports_the_private_descriptor() {
        let (_fx, _root, base, _volume) = setup();
        let user = win_sys::current_user_sid().unwrap_or_else(|e| unreachable!("{e}"));
        let mut path = base.clone();
        path.push(0);
        let parent = win_sys::open_root(&path).unwrap_or_else(|e| unreachable!("{e}"));
        let created = win_sys::create_dir_at(&parent, &wide("Probe"), &private_sddl(&user))
            .unwrap_or_else(|e| unreachable!("{e}"));
        let actual = win_sys::owner_and_dacl(&created).unwrap_or_else(|e| unreachable!("{e}"));
        assert!(
            is_private(&actual, &user),
            "expected {}, Windows reports {actual}",
            private_sddl(&user)
        );
    }

    #[test]
    fn a_widened_dacl_is_refused_not_repaired() {
        let (_fx, root, base, volume) = setup();
        let name = wide("Quarantine");
        open_store(&base, &[&name], volume).unwrap_or_else(|e| unreachable!("{e}"));
        let status = Command::new("icacls")
            .arg(root.join("Quarantine"))
            .arg("/grant")
            .arg("*S-1-1-0:(OI)(CI)(F)")
            .status()
            .unwrap_or_else(|e| unreachable!("{e}"));
        assert!(status.success(), "icacls failed");
        assert_eq!(
            open_store(&base, &[&name], volume).err(),
            Some(WinStoreError::Unsafe)
        );
    }

    #[test]
    fn a_planted_directory_or_junction_is_refused() {
        let (_fx, root, base, volume) = setup();
        std::fs::create_dir(root.join("Planted")).unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(
            open_store(&base, &[&wide("Planted")], volume).err(),
            Some(WinStoreError::Unsafe),
            "inherited, non-private DACL"
        );
        std::fs::create_dir(root.join("target")).unwrap_or_else(|e| unreachable!("{e}"));
        let status = Command::new("cmd")
            .arg("/C")
            .arg("mklink")
            .arg("/J")
            .arg(root.join("Linked"))
            .arg(root.join("target"))
            .status()
            .unwrap_or_else(|e| unreachable!("{e}"));
        assert!(status.success(), "mklink /J failed");
        assert_eq!(
            open_store(&base, &[&wide("Linked")], volume).err(),
            Some(WinStoreError::Unsafe)
        );
    }

    #[test]
    fn another_volume_and_bad_names_are_refused() {
        let (_fx, root, base, volume) = setup();
        assert_eq!(
            open_store(&base, &[&wide("Elsewhere")], volume ^ 1).err(),
            Some(WinStoreError::OtherVolume)
        );
        assert!(
            !root.join("Elsewhere").exists(),
            "nothing is created for another volume"
        );
        for name in [wide(""), wide(".."), wide("a\\b")] {
            assert_eq!(
                open_store(&base, &[&name], volume).err(),
                Some(WinStoreError::InvalidName)
            );
        }
    }
}
