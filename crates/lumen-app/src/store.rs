//! Where quarantine stores live, and who may use them (ADR-0015, ADR-0005).

use std::path::PathBuf;

/// A base directory and the store's names below it.
pub type StoreLocation = (PathBuf, Vec<Vec<u8>>);

/// Why the store location is unknown.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum LocationError {
    /// The home directory is unknown.
    #[error("the home directory is unknown")]
    NoHome,
    /// `%LOCALAPPDATA%` is unset or not absolute.
    #[error("the local application data folder is unknown")]
    NoLocalAppData,
}

/// Where quarantine stores live (ADR-0015): `~/Library/Application Support`
/// on macOS, `$XDG_DATA_HOME` (or `~/.local/share`) elsewhere on Unix.
///
/// # Errors
///
/// [`LocationError::NoHome`] if the home directory is unknown.
#[cfg(unix)]
pub fn store_location() -> Result<StoreLocation, LocationError> {
    let home = std::env::home_dir().ok_or(LocationError::NoHome)?;
    if cfg!(target_os = "macos") {
        Ok((
            home.join("Library/Application Support"),
            vec![b"Lumen".to_vec(), b"Quarantine".to_vec()],
        ))
    } else {
        let data = std::env::var_os("XDG_DATA_HOME")
            .map_or_else(|| home.join(".local/share"), PathBuf::from);
        Ok((data, vec![b"lumen".to_vec(), b"quarantine".to_vec()]))
    }
}

/// Where quarantine stores live (ADR-0015): the user's local, non-roaming
/// application data. Items on other volumes cannot be moved there and stay
/// in place.
///
/// # Errors
///
/// [`LocationError::NoLocalAppData`] if `%LOCALAPPDATA%` is unset or not
/// absolute.
#[cfg(windows)]
pub fn store_location() -> Result<StoreLocation, LocationError> {
    let local = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .ok_or(LocationError::NoLocalAppData)?;
    Ok((local, vec![b"Lumen".to_vec(), b"Quarantine".to_vec()]))
}

/// Why moving files is refused.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PrivilegeError {
    /// The process runs as root.
    #[error("refusing to run as root; run as the user who owns the files")]
    Root,
    /// The process runs with an elevated Windows token.
    #[error("refusing to run elevated (as administrator); run as the user who owns the files")]
    Elevated,
    /// The process token could not be read.
    #[error("the process token could not be read: {0}")]
    Token(std::io::Error),
}

/// Refuses to move files as root or with an elevated Windows token
/// (ADR-0005): the store, the journal and the ownership checks are the
/// user's own.
///
/// # Errors
///
/// A [`PrivilegeError`] saying why.
pub fn refuse_privileged() -> Result<(), PrivilegeError> {
    #[cfg(unix)]
    if rustix::process::geteuid().is_root() {
        return Err(PrivilegeError::Root);
    }
    #[cfg(windows)]
    if lumen_platform_windows::process_is_elevated().map_err(PrivilegeError::Token)? {
        return Err(PrivilegeError::Elevated);
    }
    Ok(())
}
