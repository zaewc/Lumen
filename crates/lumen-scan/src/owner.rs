//! Ownership of scanned objects on Unix (roadmap 10.2b, ADR-0030).
//!
//! Scanned entries do not carry their owner, so it is read when an item is
//! assessed: the object at the path is looked at without following a final
//! link, must still be the scanned object (the enumerators' identity rule:
//! `dev:<hex device>` and inode), and is owned by the current user when its
//! owner ID is the process's effective user ID. Anything that cannot be
//! established is `None`, never `false` and never `true`.

use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::fs::MetadataExt as _;
use std::path::Path;

use lumen_application::ports::OwnershipProbe;
use lumen_domain::{FileIdentity, PathFlavor, RawPath, VolumeId};

/// Reads owners with `lstat`.
#[derive(Debug, Clone, Copy, Default)]
pub struct UnixOwnershipProbe;

impl OwnershipProbe for UnixOwnershipProbe {
    fn owned_by_current_user(&self, path: &RawPath, expected: &FileIdentity) -> Option<bool> {
        if path.flavor() != PathFlavor::Unix {
            return None;
        }
        let meta =
            std::fs::symlink_metadata(Path::new(std::ffi::OsStr::from_bytes(path.as_bytes())))
                .ok()?;
        let volume = VolumeId::new(format!("dev:{:x}", meta.dev())).ok()?;
        if volume != expected.volume || u128::from(meta.ino()) != expected.file.get() {
            return None;
        }
        Some(meta.uid() == rustix::process::geteuid().as_raw())
    }
}

#[cfg(test)]
mod tests {
    use lumen_domain::FileId;
    use lumen_testkit::{Fixture, Node};

    use super::*;

    fn observed(path: &Path) -> (RawPath, FileIdentity) {
        let meta = std::fs::symlink_metadata(path).unwrap_or_else(|e| unreachable!("{e}"));
        let raw = RawPath::from_unix_bytes(path.as_os_str().as_bytes().to_vec())
            .unwrap_or_else(|e| unreachable!("{e}"));
        let identity = FileIdentity {
            volume: VolumeId::new(format!("dev:{:x}", meta.dev()))
                .unwrap_or_else(|e| unreachable!("{e}")),
            file: FileId::new(u128::from(meta.ino())),
        };
        (raw, identity)
    }

    #[test]
    fn a_file_the_test_created_is_owned_by_the_current_user() {
        let fx = Fixture::build(&[Node::File {
            path: "mine".into(),
            contents: b"x".to_vec(),
        }])
        .unwrap_or_else(|e| unreachable!("{e}"));
        let (raw, identity) = observed(&fx.root().join("mine"));
        assert_eq!(
            UnixOwnershipProbe.owned_by_current_user(&raw, &identity),
            Some(true)
        );
    }

    #[test]
    fn a_root_owned_file_is_not() {
        if rustix::process::geteuid().is_root() {
            return;
        }
        let (raw, identity) = observed(Path::new("/etc/hosts"));
        assert_eq!(
            UnixOwnershipProbe.owned_by_current_user(&raw, &identity),
            Some(false)
        );
    }

    #[test]
    fn a_replaced_or_missing_object_is_unknown() {
        let fx = Fixture::build(&[Node::File {
            path: "mine".into(),
            contents: b"x".to_vec(),
        }])
        .unwrap_or_else(|e| unreachable!("{e}"));
        let (raw, mut identity) = observed(&fx.root().join("mine"));
        identity.file = FileId::new(identity.file.get() ^ 1);
        assert_eq!(
            UnixOwnershipProbe.owned_by_current_user(&raw, &identity),
            None
        );
        let gone = RawPath::from_unix_bytes(b"/nonexistent/lumen".to_vec())
            .unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(
            UnixOwnershipProbe.owned_by_current_user(&gone, &identity),
            None
        );
    }
}
