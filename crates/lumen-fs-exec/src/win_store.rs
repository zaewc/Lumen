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
    /// The base could not be opened or inspected, and its drive's root is not
    /// on the items' volume or cannot be opened either (a drive that is not
    /// connected), so the base's volume is unknown. A base that fails to open
    /// on the items' volume is [`WinStoreError::Unreachable`] instead.
    #[error("the store base could not be opened: OS error {code}")]
    BaseUnreachable {
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

impl WinStoreError {
    /// Whether the store is known to be on the items' volume: the error came
    /// after the base was found on it. Otherwise this location is simply not
    /// for that volume.
    pub const fn on_volume(&self) -> bool {
        !matches!(self, Self::OtherVolume | Self::BaseUnreachable { .. })
    }
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

pub(crate) fn missing(error: &io::Error) -> bool {
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
    open(base, names, volume, true)
}

/// Opens `names` below `base` on `volume` with the same checks as
/// [`open_store`], creating nothing: for a store that only holds items put
/// there before.
///
/// # Errors
///
/// A [`WinStoreError`], [`WinStoreError::Unreachable`] if a directory is
/// missing; nothing is changed.
pub(crate) fn open_existing_store(
    base: &[u16],
    names: &[&[u16]],
    volume: u64,
) -> Result<WinStore, WinStoreError> {
    open(base, names, volume, false)
}

/// The volume of the drive root `base` is under (`X:\`, verbatim or not), if
/// the root opens. Only its attributes are read.
fn drive_volume(base: &[u16]) -> Option<u64> {
    let prefix: Vec<u16> = r"\\?\".encode_utf16().collect();
    let rest = base.strip_prefix(prefix.as_slice()).unwrap_or(base);
    let letter = *rest.first()?;
    let rooted = u8::try_from(letter).is_ok_and(|b| b.is_ascii_alphabetic())
        && rest.get(1) == Some(&u16::from(b':'))
        && rest.get(2) == Some(&u16::from(b'\\'));
    if !rooted {
        return None;
    }
    let mut root: Vec<u16> = prefix;
    root.extend_from_slice(&rest[..3]);
    root.push(0);
    win_sys::open_root(&root)
        .and_then(|handle| win_sys::facts(&handle))
        .ok()
        .map(|facts| facts.volume)
}

fn open(
    base: &[u16],
    names: &[&[u16]],
    volume: u64,
    create: bool,
) -> Result<WinStore, WinStoreError> {
    if names.is_empty() || !names.iter().all(|n| plain(n)) {
        return Err(WinStoreError::InvalidName);
    }
    let user = win_sys::current_user_sid().map_err(unreachable("the current user"))?;
    let sddl = private_sddl(&user);
    let mut path = base.to_vec();
    path.push(0);
    // A base that cannot be opened is still known to be on the items' volume
    // when its drive's root is: then the store there is unavailable (access
    // denied, missing), not absent from the volume.
    let base_unreachable = |error: io::Error| {
        let code = error.raw_os_error().unwrap_or(0);
        if drive_volume(base) == Some(volume) {
            WinStoreError::Unreachable {
                what: "the store base",
                code,
            }
        } else {
            WinStoreError::BaseUnreachable { code }
        }
    };
    let mut dir = win_sys::open_root(&path).map_err(base_unreachable)?;
    let base_facts = win_sys::facts(&dir).map_err(base_unreachable)?;
    // A store is only ever created on the volume of the items it is for
    // (ADR-0046): nothing is created below a base on another volume. Checked
    // first, so every later error is about a store on the items' volume.
    if base_facts.volume != volume {
        return Err(WinStoreError::OtherVolume);
    }
    if base_facts.attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 || !base_facts.directory {
        return Err(WinStoreError::Unsafe);
    }
    for name in names {
        dir = match win_sys::open_at(&dir, name, Want::Store) {
            Ok(existing) => existing,
            Err(error) if create && missing(&error) => win_sys::create_dir_at(&dir, name, &sddl)
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
    fn an_existing_store_opens_but_a_missing_one_is_not_created() {
        let (_fx, root, base, volume) = setup();
        let name = wide("Quarantine");
        assert!(matches!(
            open_existing_store(&base, &[&name], volume),
            Err(WinStoreError::Unreachable { .. })
        ));
        assert!(!root.join("Quarantine").exists(), "nothing is created");
        let made = open_store(&base, &[&name], volume).unwrap_or_else(|e| unreachable!("{e}"));
        let found =
            open_existing_store(&base, &[&name], volume).unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(made.identity, found.identity);
        std::fs::create_dir(root.join("Planted")).unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(
            open_existing_store(&base, &[&wide("Planted")], volume).err(),
            Some(WinStoreError::Unsafe)
        );
    }

    #[test]
    fn a_base_that_does_not_open_is_on_the_volume_only_if_its_drive_is() {
        let (_fx, root, _base, volume) = setup();
        let missing: Vec<u16> = root.join("missing").as_os_str().encode_wide().collect();
        let name = wide("Quarantine");
        let here = open_store(&missing, &[&name], volume).err();
        assert!(
            matches!(here, Some(WinStoreError::Unreachable { .. })),
            "{here:?}"
        );
        assert!(here.is_some_and(|e| e.on_volume()));
        let elsewhere = open_store(&missing, &[&name], volume ^ 1).err();
        assert!(
            matches!(elsewhere, Some(WinStoreError::BaseUnreachable { .. })),
            "{elsewhere:?}"
        );
        assert!(elsewhere.is_some_and(|e| !e.on_volume()));
        assert!(!root.join("missing").exists(), "nothing is created");
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
