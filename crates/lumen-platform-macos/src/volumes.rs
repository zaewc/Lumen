//! How names compare on each mounted volume (#790).
//!
//! Case sensitivity belongs to a volume, not to macOS: APFS and HFS+ volumes
//! can be formatted either way. Each locally stored volume is asked with
//! `pathconf(_PC_CASE_SENSITIVE)` on its mount point. Network volumes are not
//! asked, so a slow share cannot hang the call; they and any volume that does
//! not answer are left out, and a volume left out compares names exactly.

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::fs::MetadataExt as _;
use std::path::Path;

use lumen_domain::{CaseSensitivity, VolumeId};

use crate::sys;

/// Every locally stored mounted volume, by the ID scanned entries carry
/// (`dev:` and the device number), with how it compares names. Empty if the
/// mounted filesystems cannot be listed.
#[must_use]
pub fn case_sensitivity() -> Vec<(VolumeId, CaseSensitivity)> {
    let Ok(mounts) = sys::local_mount_points() else {
        return Vec::new();
    };
    mounts
        .iter()
        .filter_map(|mount| {
            let path = Path::new(OsStr::from_bytes(mount.to_bytes()));
            let device = std::fs::symlink_metadata(path).ok()?.dev();
            let volume = VolumeId::new(format!("dev:{device:x}")).ok()?;
            let case = if sys::case_sensitive(mount)? {
                CaseSensitivity::Sensitive
            } else {
                CaseSensitivity::Insensitive
            };
            Some((volume, case))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_volume_of_the_temporary_directory_answers() -> std::io::Result<()> {
        let device = std::fs::symlink_metadata(std::env::temp_dir())?.dev();
        let volume = format!("dev:{device:x}");
        let found = case_sensitivity();
        assert!(
            found.iter().any(|(id, _)| id.as_str() == volume),
            "{volume} not in {found:?}"
        );
        Ok(())
    }
}
