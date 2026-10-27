//! Where quarantine stores live, and who may use them (ADR-0015, ADR-0005,
//! ADR-0046).

use std::collections::BTreeSet;
use std::path::PathBuf;

use lumen_domain::VolumeId;
use lumen_fs_exec::Executor;

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

/// Where quarantine goes on this computer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stores {
    /// The store locations, the main one ([`store_location`]) first.
    pub locations: Vec<StoreLocation>,
    /// The volumes with a store, or `None` when every volume is offered to
    /// the executor, which refuses those it cannot serve.
    pub volumes: Option<BTreeSet<VolumeId>>,
    /// The current user is the only user with a profile here (ADR-0046).
    pub single_user: bool,
}

impl Stores {
    /// An executor using every location: an item goes into the store on its
    /// own volume.
    pub fn executor(&self) -> Option<Executor> {
        let mut locations = self.locations.iter().cloned();
        let (base, names) = locations.next()?;
        let executor = Executor::new(base, names);
        #[cfg(windows)]
        let executor = locations.fold(executor, |executor, (base, names)| {
            executor.with_store(base, names)
        });
        Some(executor)
    }
}

/// The stores on Unix: the main one, for every volume (ADR-0046 leaves Unix
/// unchanged).
///
/// # Errors
///
/// As [`store_location`].
#[cfg(unix)]
pub fn stores() -> Result<Stores, LocationError> {
    Ok(Stores {
        locations: vec![store_location()?],
        volumes: None,
        single_user: false,
    })
}

/// The stores on Windows (ADR-0046): the main one under
/// `%LOCALAPPDATA%`, and `X:\$LumenQuarantine.<SID>\` on every other drive
/// that may hold one. Items on any other volume are not offered for
/// quarantine. If the main store's volume cannot be identified, every volume
/// is offered and only the main store is used, as before.
///
/// # Errors
///
/// As [`store_location`].
#[cfg(windows)]
pub fn stores() -> Result<Stores, LocationError> {
    use lumen_platform_windows::volumes;

    let main = store_location()?;
    let letter = |path: &std::path::Path| {
        path.to_string_lossy()
            .chars()
            .next()
            .map(|c| c.to_ascii_uppercase())
    };
    let main_letter = letter(&main.0);
    let id = |serial: u64| VolumeId::new(format!("vsn:{serial:016x}")).ok();
    let facts = volumes::mounted();
    let main_volume = facts
        .iter()
        .find(|f| letter(std::path::Path::new(&f.root)) == main_letter)
        .and_then(|f| f.serial)
        .and_then(id);
    let single_user = volumes::single_user_pc();
    let Some(main_volume) = main_volume else {
        return Ok(Stores {
            locations: vec![main],
            volumes: None,
            single_user,
        });
    };
    let mut ids = BTreeSet::from([main_volume]);
    let mut locations = vec![main];
    if let Ok(sid) = lumen_platform_windows::current_user_sid() {
        for volume in volumes::store_volumes(&facts) {
            let root = PathBuf::from(&volume.root);
            if letter(&root) == main_letter {
                continue;
            }
            if let Some(volume_id) = id(volume.serial) {
                ids.insert(volume_id);
                locations.push((root, vec![format!("$LumenQuarantine.{sid}").into_bytes()]));
            }
        }
    }
    Ok(Stores {
        locations,
        volumes: Some(ids),
        single_user,
    })
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
