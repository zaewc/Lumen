//! Never fill the disk: recording a scan or an assessment stops, or does not
//! start, when it would leave less than [`RESERVE`] free on the volume that
//! holds the database. A full disk breaks other programs, and a storage
//! cleaner must not be the cause.

use std::path::Path;

use lumen_domain::ByteCount;

/// Free space Lumen leaves on the database's volume: 2 GB.
pub const RESERVE: ByteCount = ByteCount::new(2_000_000_000);

/// Space an assessment writes per scanned entry, with a wide margin. Only
/// entries a producer identified or a relationship connects get evidence: 30
/// records for the 198,495 entries of `~/Library/Developer`, which no longer
/// grew `index.db` measurably (before that, 2.8 kB per entry).
pub const ASSESS_BYTES_PER_ENTRY: u64 = 100;

/// Why writing to the database would not leave the reserve free.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SpaceError {
    /// The volume holding the database is too full.
    #[error(
        "only {available} bytes are free on the disk holding the database and this needs about {needed} more; Lumen keeps {reserve} bytes free. Free some space or use --db on another disk"
    )]
    Low {
        /// Free now.
        available: u64,
        /// What the operation would write.
        needed: u64,
        /// [`RESERVE`].
        reserve: u64,
    },
    /// Free space could not be measured, so it is not assumed.
    #[error("free space on the disk holding {path} could not be measured: {source}")]
    Unknown {
        /// The directory measured.
        path: String,
        /// Why.
        source: std::io::Error,
    },
}

/// The directory whose volume will hold `db`.
fn holder(db: &Path) -> &Path {
    match db.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    }
}

#[cfg(unix)]
fn measure(dir: &Path) -> std::io::Result<u64> {
    let stats = rustix::fs::statvfs(dir)?;
    Ok(stats.f_bavail.saturating_mul(stats.f_frsize))
}

#[cfg(windows)]
fn measure(dir: &Path) -> std::io::Result<u64> {
    lumen_platform_windows::available_space(dir)
}

#[cfg(not(any(unix, windows)))]
fn measure(_dir: &Path) -> std::io::Result<u64> {
    Err(std::io::ErrorKind::Unsupported.into())
}

/// Bytes free for this user on the volume that holds (or will hold) `db`.
///
/// # Errors
///
/// [`SpaceError::Unknown`] if it cannot be measured.
pub fn available(db: &Path) -> Result<u64, SpaceError> {
    let dir = holder(db);
    measure(dir).map_err(|source| SpaceError::Unknown {
        path: dir.display().to_string(),
        source,
    })
}

/// Whether `available` bytes leave the reserve after writing `needed`.
///
/// # Errors
///
/// [`SpaceError::Low`] if they do not.
pub const fn check(available: u64, needed: u64) -> Result<(), SpaceError> {
    if available.saturating_sub(needed) < RESERVE.get() {
        return Err(SpaceError::Low {
            available,
            needed,
            reserve: RESERVE.get(),
        });
    }
    Ok(())
}

/// Whether writing about `needed` bytes next to `db` leaves the reserve free.
///
/// # Errors
///
/// A [`SpaceError`] if it would not, or if free space cannot be measured.
pub fn ensure(db: &Path, needed: u64) -> Result<(), SpaceError> {
    check(available(db)?, needed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_reserve_is_kept_after_the_write() {
        let reserve = RESERVE.get();
        assert!(check(reserve + 100, 100).is_ok());
        assert!(matches!(
            check(reserve + 100, 101),
            Err(SpaceError::Low { needed: 101, .. })
        ));
        assert!(check(0, 0).is_err());
        assert!(check(u64::MAX, u64::MAX).is_err());
    }

    #[test]
    fn free_space_is_measured_where_the_database_will_be() {
        let fx = lumen_testkit::Fixture::build(&[]).unwrap_or_else(|e| unreachable!("{e}"));
        let db = fx
            .path("missing.db")
            .unwrap_or_else(|e| unreachable!("{e}"));
        assert!(available(&db).is_ok_and(|bytes| bytes > 0));
        assert_eq!(holder(Path::new("index.db")), Path::new("."));
        assert!(matches!(
            available(Path::new("/no/such/place/index.db")),
            Err(SpaceError::Unknown { .. })
        ));
    }
}
