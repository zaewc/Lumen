//! What "the whole disk" means when no folder is chosen: every place on the
//! startup disk where files can take up space, under the paths the user knows.
//!
//! - macOS: each firmlink of the data volume listed in `/usr/share/firmlinks`
//!   (`/Users`, `/Applications`, `/Library`, `/private`, `/opt`, `/usr/local`
//!   and the system's asset and cache folders), but not `/Volumes`, where
//!   other disks are mounted. The sealed system volume holds nothing that can
//!   be cleaned and is left out.
//! - Linux: `/`, plus the home folder when it is on another file system.
//! - Windows: the system drive, such as `C:\`.
//!
//! Scanning stays on one volume per root, so other disks, network shares and
//! cloud mounts are never entered.

use std::path::PathBuf;

/// The firmlinks on macOS 26 when the list cannot be read.
#[cfg(target_os = "macos")]
const MACOS_FALLBACK: &[&str] = &[
    "/Applications",
    "/Library",
    "/System/Library/Caches",
    "/System/Library/Assets",
    "/System/Library/AssetsV2",
    "/Users",
    "/cores",
    "/opt",
    "/private",
    "/usr/local",
];

/// The firmlink paths in the text of `/usr/share/firmlinks` (one
/// `<path>\t<target>` per line), without `/Volumes`.
pub fn firmlinks(text: &str) -> Vec<PathBuf> {
    text.lines()
        .filter_map(|line| line.split('\t').next())
        .map(str::trim)
        .filter(|path| path.starts_with('/') && *path != "/Volumes")
        .map(PathBuf::from)
        .collect()
}

/// Keeps the roots that exist and are directories, not links.
fn existing(roots: Vec<PathBuf>) -> Vec<PathBuf> {
    roots
        .into_iter()
        .filter(|root| std::fs::symlink_metadata(root).is_ok_and(|m| m.is_dir()))
        .collect()
}

/// The startup disk's roots on macOS.
#[cfg(target_os = "macos")]
pub fn whole_disk() -> Vec<PathBuf> {
    let listed = std::fs::read_to_string("/usr/share/firmlinks")
        .map(|text| firmlinks(&text))
        .unwrap_or_default();
    let roots = if listed.is_empty() {
        MACOS_FALLBACK.iter().map(PathBuf::from).collect()
    } else {
        listed
    };
    existing(roots)
}

/// The startup disk's roots on Linux and other Unix systems.
#[cfg(all(unix, not(target_os = "macos")))]
pub fn whole_disk() -> Vec<PathBuf> {
    use std::os::unix::fs::MetadataExt as _;

    let root = PathBuf::from("/");
    let mut roots = vec![root.clone()];
    let device = |path: &std::path::Path| std::fs::metadata(path).map(|m| m.dev()).ok();
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from)
        && device(&home).is_some()
        && device(&home) != device(&root)
    {
        roots.push(home);
    }
    existing(roots)
}

/// The system drive on Windows.
#[cfg(windows)]
pub fn whole_disk() -> Vec<PathBuf> {
    let drive = std::env::var_os("SystemDrive")
        .map_or_else(|| "C:".into(), |d| d.to_string_lossy().into_owned());
    existing(vec![PathBuf::from(format!("{drive}\\"))])
}

/// No default on other platforms.
#[cfg(not(any(unix, windows)))]
pub fn whole_disk() -> Vec<PathBuf> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn firmlinks_are_read_without_volumes() {
        let text = "/AppleInternal\tAppleInternal\n/Applications\tApplications\n/Users\tUsers\n\
                    /Volumes\tVolumes\n/usr/local\tusr/local\n\nnot a path\n";
        assert_eq!(
            firmlinks(text),
            ["/AppleInternal", "/Applications", "/Users", "/usr/local"].map(PathBuf::from)
        );
    }

    #[test]
    fn only_existing_directories_are_kept() {
        let fx = lumen_testkit::Fixture::build(&[lumen_testkit::Node::File {
            path: "file".into(),
            contents: Vec::new(),
        }])
        .unwrap_or_else(|e| unreachable!("{e}"));
        let dir = fx.root().to_path_buf();
        let file = fx.path("file").unwrap_or_else(|e| unreachable!("{e}"));
        let missing = fx.path("missing").unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(existing(vec![dir.clone(), file, missing]), [dir]);
    }

    #[test]
    fn the_whole_disk_has_at_least_one_root_here() {
        let roots = whole_disk();
        assert_ne!(roots, Vec::<PathBuf>::new());
        assert!(roots.iter().all(|root| root.is_absolute()));
        assert!(!roots.contains(&PathBuf::from("/Volumes")));
    }
}
