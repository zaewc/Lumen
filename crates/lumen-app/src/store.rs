//! Where quarantine stores live, and who may use them (ADR-0015, ADR-0005,
//! ADR-0046).

use std::collections::BTreeSet;
use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};

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
    /// The current user's Windows SID cannot be read.
    #[error("the current user's SID is unknown")]
    NoUserSid,
}

/// All current-user store paths to exclude, even on Windows drives no longer
/// eligible for new quarantine operations. No stores are opened or created.
///
/// # Errors
/// A [`LocationError`] if the user's store locations cannot be determined.
pub fn scan_exclusion_paths() -> Result<Vec<PathBuf>, LocationError> {
    let paths = stores()?.paths();
    #[cfg(windows)]
    let paths = {
        let mut paths = paths;
        let sid =
            lumen_platform_windows::current_user_sid().map_err(|_| LocationError::NoUserSid)?;
        paths.extend(
            lumen_platform_windows::volumes::mounted()
                .into_iter()
                .map(|volume| PathBuf::from(volume.root).join(format!("$LumenQuarantine.{sid}"))),
        );
        paths
    };
    Ok(paths)
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

/// Where quarantine stores live (ADR-0015, ADR-0066): the user's local,
/// non-roaming application data, in a folder of its own, because the
/// installer puts the app in `%LOCALAPPDATA%\Lumen`. Items on other volumes
/// cannot be moved there and stay in place.
///
/// # Errors
///
/// [`LocationError::NoLocalAppData`] if `%LOCALAPPDATA%` is unset or not
/// absolute.
#[cfg(windows)]
pub fn store_location() -> Result<StoreLocation, LocationError> {
    Ok((local_app_data()?, vec![b"LumenQuarantine".to_vec()]))
}

#[cfg(windows)]
fn local_app_data() -> Result<PathBuf, LocationError> {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .ok_or(LocationError::NoLocalAppData)
}

/// Where the main store used to be (ADR-0066): items held there before are
/// still found, restored and finalized, but nothing new goes there.
#[cfg(windows)]
fn former_locations() -> Result<Vec<StoreLocation>, LocationError> {
    Ok(vec![(
        local_app_data()?,
        vec![b"Lumen".to_vec(), b"Quarantine".to_vec()],
    )])
}

/// Where quarantine goes on this computer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stores {
    /// The store locations, the main one ([`store_location`]) first.
    pub locations: Vec<StoreLocation>,
    /// Former store locations (ADR-0066): searched for held items, never
    /// created, never quarantined into.
    pub former: Vec<StoreLocation>,
    /// The volumes with a store, or `None` when every volume is offered to
    /// the executor, which refuses those it cannot serve.
    pub volumes: Option<BTreeSet<VolumeId>>,
    /// The current user is the only user with a profile here (ADR-0046).
    pub single_user: bool,
}

impl Stores {
    /// Every store path, including stores on other Windows volumes and
    /// former locations.
    pub fn paths(&self) -> Vec<PathBuf> {
        self.locations
            .iter()
            .chain(&self.former)
            .map(|(base, names)| {
                let mut path = base.clone();
                for name in names {
                    #[cfg(unix)]
                    {
                        use std::os::unix::ffi::OsStringExt as _;
                        path.push(std::ffi::OsString::from_vec(name.clone()));
                    }
                    // Store components are fixed ASCII names or a Windows SID.
                    #[cfg(windows)]
                    path.push(String::from_utf8_lossy(name).as_ref());
                }
                path
            })
            .collect()
    }

    /// An executor using every location: an item goes into the store on its
    /// own volume, and is found there or in a former location.
    pub fn executor(&self) -> Option<Executor> {
        let mut locations = self.locations.iter().cloned();
        let (base, names) = locations.next()?;
        let executor = Executor::new(base, names);
        #[cfg(windows)]
        let executor = locations.fold(executor, |executor, (base, names)| {
            executor.with_store(base, names)
        });
        #[cfg(windows)]
        let executor = self
            .former
            .iter()
            .cloned()
            .fold(executor, |executor, (base, names)| {
                executor.with_former_store(base, names)
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
        former: Vec::new(),
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
    let former = former_locations()?;
    let letter = |path: &Path| {
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
        .find(|f| letter(Path::new(&f.root)) == main_letter)
        .and_then(|f| f.serial)
        .and_then(id);
    let single_user = volumes::single_user_pc();
    let Some(main_volume) = main_volume else {
        return Ok(Stores {
            locations: vec![main],
            former,
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
        former,
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

/// Takes the lock on the ledger next to `db` (`ledger.lock`), exclusively and
/// across processes, until the file is dropped. `None`: someone else holds
/// it. With `wait`, waits for it instead.
///
/// Every holder settles interrupted operations first (a quarantine, the
/// held-items listing, the desktop's startup), so a caller that finds the
/// lock taken skips reconciliation: the holder did it or is doing it, and an
/// operation that looks unfinished may be in flight. Without the lock, two
/// reconciliations could journal one outcome twice, or one could journal
/// over a move still under way.
///
/// # Errors
///
/// The [`std::io::Error`] if the lock file cannot be opened or locked.
pub fn lock_ledger(db: &Path, wait: bool) -> std::io::Result<Option<File>> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(db.with_file_name("ledger.lock"))?;
    if wait {
        file.lock()?;
        return Ok(Some(file));
    }
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(error)) => Err(error),
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn the_windows_store_is_not_in_the_install_folder_and_the_former_one_is_still_known() {
        let (base, names) = store_location().unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(names, vec![b"LumenQuarantine".to_vec()]);
        let stores = stores().unwrap_or_else(|e| unreachable!("{e}"));
        let former = base.join("Lumen").join("Quarantine");
        assert_eq!(
            stores.former,
            vec![(
                base.clone(),
                vec![b"Lumen".to_vec(), b"Quarantine".to_vec()]
            )]
        );
        let paths = stores.paths();
        assert_eq!(paths.first(), Some(&base.join("LumenQuarantine")));
        assert!(paths.contains(&former), "a former store is never scanned");
    }
}
