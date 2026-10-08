//! Portable directory enumerator on `std::fs` (Unix).
//!
//! Guarantees (the [`DirEnumerator`] contract):
//!
//! - never follows symbolic links: entries come from `lstat`-style metadata, and
//!   `read_dir` refuses a path that is a link;
//! - never opens file contents;
//! - reports unreadable directories as denied or failed, never as empty
//!   (ADR-0008);
//! - detects a directory swapped while it was being listed (identity checked
//!   before and after).
//!
//! It cannot see APFS or btrfs clone sharing, so every regular file is marked
//! `may_share_blocks`; reclaim estimates from this adapter are therefore lower
//! bounds, never over-promises (ADR-0017).

use std::ffi::OsStr;
use std::fs::{self, Metadata};
use std::io;
use std::num::NonZeroU32;
use std::ops::ControlFlow;
use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::fs::MetadataExt as _;
use std::path::Path;
use std::time::SystemTime;

use lumen_application::ports::{DirEnumerator, DirOutcome, EnumerateError};
use lumen_domain::{
    AccessState, ByteCount, DenialReason, EntryKind, EntryTimes, FailureKind, FileId, FileIdentity,
    FilesystemEntry, PathFlavor, Protection, RawPath, SizeFacts, SizeFlags, Timestamp, VolumeId,
};

/// Enumerates directories with `std::fs`. Stateless and thread-safe.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdFsEnumerator;

impl DirEnumerator for StdFsEnumerator {
    fn stat(&self, path: &RawPath) -> Result<FilesystemEntry, EnumerateError> {
        let native = native_path(path)?;
        let meta = fs::symlink_metadata(native).map_err(|e| EnumerateError::Unobservable {
            path: path.clone(),
            access: access_from_io(&e),
        })?;
        entry_from(path.clone(), &meta)
    }

    fn read_dir(
        &self,
        path: &RawPath,
        expected: Option<&FileIdentity>,
        sink: &mut dyn FnMut(FilesystemEntry) -> ControlFlow<()>,
    ) -> Result<DirOutcome, EnumerateError> {
        let native = native_path(path)?;
        let before = verified_directory(path, native, expected)?;

        let listing = match fs::read_dir(native) {
            Ok(listing) => listing,
            Err(e) => {
                return Ok(DirOutcome {
                    access: access_from_io(&e),
                    complete: false,
                });
            }
        };

        let mut complete = true;
        for item in listing {
            // An entry that vanished or became unreadable after the directory was
            // opened makes the listing incomplete, but not the whole directory.
            let Ok(item) = item else {
                complete = false;
                continue;
            };
            let Ok(child) =
                RawPath::from_unix_bytes(join(path.as_bytes(), item.file_name().as_bytes()))
            else {
                complete = false;
                continue;
            };
            // `DirEntry::metadata` does not follow symbolic links on Unix.
            let Ok(meta) = item.metadata() else {
                complete = false;
                continue;
            };
            let entry = entry_from(child, &meta)?;
            if sink(entry).is_break() {
                return Ok(DirOutcome {
                    access: AccessState::Readable,
                    complete: false,
                });
            }
        }

        // A directory replaced while being listed would make the listing describe
        // the wrong object.
        verified_directory(path, native, Some(&before))?;
        Ok(DirOutcome {
            access: AccessState::Readable,
            complete,
        })
    }
}

/// Checks that `path` is a directory (not a link to one) and, if given, still
/// has the expected identity. Returns its identity.
fn verified_directory(
    path: &RawPath,
    native: &Path,
    expected: Option<&FileIdentity>,
) -> Result<FileIdentity, EnumerateError> {
    let meta = fs::symlink_metadata(native).map_err(|e| EnumerateError::Unobservable {
        path: path.clone(),
        access: access_from_io(&e),
    })?;
    if !meta.file_type().is_dir() {
        return Err(EnumerateError::NotADirectory { path: path.clone() });
    }
    let identity = identity_of(path, &meta)?;
    if let Some(expected) = expected
        && *expected != identity
    {
        return Err(EnumerateError::IdentityChanged {
            path: path.clone(),
            expected: expected.clone(),
        });
    }
    Ok(identity)
}

fn native_path(path: &RawPath) -> Result<&Path, EnumerateError> {
    match path.flavor() {
        PathFlavor::Unix => Ok(Path::new(OsStr::from_bytes(path.as_bytes()))),
        PathFlavor::Windows => Err(EnumerateError::UnsupportedPath { path: path.clone() }),
    }
}

fn join(parent: &[u8], name: &[u8]) -> Vec<u8> {
    let mut joined = Vec::with_capacity(parent.len() + 1 + name.len());
    joined.extend_from_slice(parent);
    if !parent.ends_with(b"/") {
        joined.push(b'/');
    }
    joined.extend_from_slice(name);
    joined
}

/// `EPERM` is how macOS reports privacy (TCC) and integrity (SIP) refusals, as
/// opposed to plain permission bits (`EACCES`). Both values are the same on Linux
/// and macOS.
const EPERM: i32 = 1;
const EACCES: i32 = 13;

fn access_from_io(error: &io::Error) -> AccessState {
    match (error.kind(), error.raw_os_error()) {
        (_, Some(EACCES)) => AccessState::Denied(DenialReason::Posix),
        (_, Some(EPERM)) | (io::ErrorKind::PermissionDenied, _) => {
            AccessState::Denied(DenialReason::Unknown)
        }
        (io::ErrorKind::NotFound, _) => AccessState::Failed(FailureKind::Vanished),
        (io::ErrorKind::TimedOut, _) => AccessState::Failed(FailureKind::TimedOut),
        _ => AccessState::Failed(FailureKind::Io),
    }
}

fn identity_of(path: &RawPath, meta: &Metadata) -> Result<FileIdentity, EnumerateError> {
    // `dev` is visible ASCII once formatted, so this cannot fail in practice.
    let volume = VolumeId::new(format!("dev:{:x}", meta.dev()))
        .map_err(|_| EnumerateError::UnsupportedPath { path: path.clone() })?;
    Ok(FileIdentity {
        volume,
        file: FileId::new(u128::from(meta.ino())),
    })
}

fn entry_from(path: RawPath, meta: &Metadata) -> Result<FilesystemEntry, EnumerateError> {
    let identity = identity_of(&path, meta)?;
    let file_type = meta.file_type();
    let kind = if file_type.is_symlink() {
        EntryKind::Symlink
    } else if file_type.is_dir() {
        EntryKind::Directory
    } else if file_type.is_file() {
        EntryKind::File
    } else {
        EntryKind::Special
    };
    let is_file = kind == EntryKind::File;
    let flags = bsd_flags(meta);
    let logical = meta.len();
    let allocated = meta.blocks().saturating_mul(512);
    let hidden_name = path
        .as_bytes()
        .rsplit(|b| *b == b'/')
        .next()
        .is_some_and(|name| name.starts_with(b"."));

    Ok(FilesystemEntry {
        kind,
        size: SizeFacts {
            identity,
            logical: ByteCount::new(logical),
            allocated: ByteCount::new(allocated),
            private: None,
            clone_id: None,
            link_count: NonZeroU32::new(u32::try_from(meta.nlink()).unwrap_or(u32::MAX))
                .unwrap_or(NonZeroU32::MIN),
            flags: SizeFlags {
                sparse: is_file && allocated < logical,
                compressed: flags & UF_COMPRESSED != 0,
                dataless: flags & SF_DATALESS != 0,
                cloud_placeholder: false,
                // Clone sharing is invisible through std; be conservative.
                may_share_blocks: is_file,
            },
        },
        times: EntryTimes {
            modified: meta.modified().ok().and_then(timestamp),
            accessed: meta.accessed().ok().and_then(timestamp),
            changed: u32::try_from(meta.ctime_nsec())
                .ok()
                .and_then(|n| Timestamp::from_unix(meta.ctime(), n).ok()),
            created: meta.created().ok().and_then(timestamp),
        },
        protection: Protection {
            system_restricted: flags & SF_RESTRICTED != 0,
            undeletable: flags & (SF_NOUNLINK | UF_IMMUTABLE | SF_IMMUTABLE) != 0,
            system_attribute: false,
            hidden: hidden_name || flags & UF_HIDDEN != 0,
        },
        // Metadata was read; a directory's *contents* have not been.
        access: if kind == EntryKind::Directory {
            AccessState::NotScanned
        } else {
            AccessState::Readable
        },
        path,
    })
}

fn timestamp(time: SystemTime) -> Option<Timestamp> {
    match time.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(after) => {
            Timestamp::from_unix(i64::try_from(after.as_secs()).ok()?, after.subsec_nanos()).ok()
        }
        Err(before) => {
            let d = before.duration();
            let secs = i64::try_from(d.as_secs()).ok()?;
            if d.subsec_nanos() == 0 {
                Timestamp::from_unix(-secs, 0).ok()
            } else {
                Timestamp::from_unix(-secs - 1, 1_000_000_000 - d.subsec_nanos()).ok()
            }
        }
    }
}

// BSD file flags (`sys/stat.h`), reported on Apple platforms only.
const UF_IMMUTABLE: u32 = 0x0000_0002;
const UF_COMPRESSED: u32 = 0x0000_0020;
const UF_HIDDEN: u32 = 0x0000_8000;
const SF_IMMUTABLE: u32 = 0x0002_0000;
const SF_RESTRICTED: u32 = 0x0008_0000;
const SF_NOUNLINK: u32 = 0x0010_0000;
const SF_DATALESS: u32 = 0x4000_0000;

#[cfg(target_os = "macos")]
fn bsd_flags(meta: &Metadata) -> u32 {
    use std::os::macos::fs::MetadataExt as _;
    meta.st_flags()
}

#[cfg(target_os = "ios")]
fn bsd_flags(meta: &Metadata) -> u32 {
    use std::os::ios::fs::MetadataExt as _;
    meta.st_flags()
}

#[cfg(not(any(target_os = "macos", target_os = "ios")))]
fn bsd_flags(_meta: &Metadata) -> u32 {
    0
}

#[cfg(test)]
mod tests {
    use lumen_testkit::{Fixture, Node};

    use super::*;

    fn file(path: &str, contents: &[u8]) -> Node {
        Node::File {
            path: path.into(),
            contents: contents.to_vec(),
        }
    }

    fn raw(fx: &Fixture, relative: &str) -> Result<RawPath, Box<dyn std::error::Error>> {
        Ok(RawPath::from_unix_bytes(
            fx.path(relative)?.as_os_str().as_bytes().to_vec(),
        )?)
    }

    fn list(path: &RawPath) -> Result<(Vec<FilesystemEntry>, DirOutcome), EnumerateError> {
        let mut entries = Vec::new();
        let outcome = StdFsEnumerator.read_dir(path, None, &mut |e| {
            entries.push(e);
            ControlFlow::Continue(())
        })?;
        entries.sort_by(|a, b| a.path.as_bytes().cmp(b.path.as_bytes()));
        Ok((entries, outcome))
    }

    fn name(entry: &FilesystemEntry) -> String {
        String::from_utf8_lossy(
            entry
                .path
                .as_bytes()
                .rsplit(|b| *b == b'/')
                .next()
                .unwrap_or_default(),
        )
        .into_owned()
    }

    fn running_as_root() -> bool {
        tempfile_uid() == Some(0)
    }

    fn tempfile_uid() -> Option<u32> {
        let fx = Fixture::build(&[file("probe", b"")]).ok()?;
        fs::metadata(fx.path("probe").ok()?).ok().map(|m| m.uid())
    }

    #[test]
    fn lists_children_without_following_links() -> Result<(), Box<dyn std::error::Error>> {
        let fx = Fixture::build(&[
            file("dir/a.txt", b"hello"),
            Node::Dir {
                path: "dir/sub".into(),
            },
            Node::Symlink {
                path: "dir/link".into(),
                target: "sub".into(),
            },
            Node::Symlink {
                path: "dir/dangling".into(),
                target: "nowhere".into(),
            },
        ])?;
        let (entries, outcome) = list(&raw(&fx, "dir")?)?;
        assert_eq!(
            outcome,
            DirOutcome {
                access: AccessState::Readable,
                complete: true
            }
        );
        let kinds: Vec<_> = entries.iter().map(|e| (name(e), e.kind)).collect();
        assert_eq!(
            kinds,
            [
                ("a.txt".into(), EntryKind::File),
                ("dangling".into(), EntryKind::Symlink),
                ("link".into(), EntryKind::Symlink),
                ("sub".into(), EntryKind::Directory),
            ]
        );
        let sub = entries.iter().find(|e| name(e) == "sub");
        assert_eq!(
            sub.map(|e| e.access),
            Some(AccessState::NotScanned),
            "contents not read yet"
        );
        let a = entries.iter().find(|e| name(e) == "a.txt");
        assert_eq!(a.map(|e| e.size.logical), Some(ByteCount::new(5)));
        assert!(
            a.is_some_and(|e| e.size.flags.may_share_blocks),
            "conservative: clones are invisible"
        );
        Ok(())
    }

    #[test]
    fn refuses_to_list_through_a_symlink() -> Result<(), Box<dyn std::error::Error>> {
        let fx = Fixture::build(&[
            Node::Dir {
                path: "real".into(),
            },
            Node::Symlink {
                path: "alias".into(),
                target: "real".into(),
            },
        ])?;
        assert!(matches!(
            list(&raw(&fx, "alias")?),
            Err(EnumerateError::NotADirectory { .. })
        ));
        Ok(())
    }

    #[test]
    fn hard_links_share_identity() -> Result<(), Box<dyn std::error::Error>> {
        let fx = Fixture::build(&[
            file("a", b"x"),
            Node::HardLink {
                path: "b".into(),
                existing: "a".into(),
            },
        ])?;
        let a = StdFsEnumerator.stat(&raw(&fx, "a")?)?;
        let b = StdFsEnumerator.stat(&raw(&fx, "b")?)?;
        assert_eq!(a.size.identity, b.size.identity);
        assert_eq!(a.size.link_count.get(), 2);
        Ok(())
    }

    #[test]
    fn unreadable_directory_is_denied_not_empty() -> Result<(), Box<dyn std::error::Error>> {
        if running_as_root() {
            return Ok(()); // Root bypasses permission bits.
        }
        let fx = Fixture::build(&[
            file("locked/secret", b"x"),
            Node::Mode {
                path: "locked".into(),
                mode: 0o000,
            },
        ])?;
        let (entries, outcome) = list(&raw(&fx, "locked")?)?;
        assert_eq!(
            entries,
            Vec::<FilesystemEntry>::new(),
            "a denied directory yields no entries"
        );
        assert_eq!(
            outcome,
            DirOutcome {
                access: AccessState::Denied(DenialReason::Posix),
                complete: false
            }
        );
        assert!(!outcome.access.is_observed());
        Ok(())
    }

    #[test]
    fn missing_paths_are_unobservable() -> Result<(), Box<dyn std::error::Error>> {
        let fx = Fixture::build(&[])?;
        let missing = raw(&fx, "nope")?;
        assert!(matches!(
            StdFsEnumerator.stat(&missing),
            Err(EnumerateError::Unobservable {
                access: AccessState::Failed(FailureKind::Vanished),
                ..
            })
        ));
        Ok(())
    }

    #[test]
    fn detects_a_directory_that_is_not_the_expected_one() -> Result<(), Box<dyn std::error::Error>>
    {
        let fx = Fixture::build(&[
            Node::Dir { path: "one".into() },
            Node::Dir { path: "two".into() },
        ])?;
        let one = StdFsEnumerator.stat(&raw(&fx, "one")?)?;
        let result =
            StdFsEnumerator.read_dir(&raw(&fx, "two")?, Some(&one.size.identity), &mut |_| {
                ControlFlow::Continue(())
            });
        assert!(matches!(
            result,
            Err(EnumerateError::IdentityChanged { .. })
        ));
        Ok(())
    }

    #[test]
    fn sink_can_stop_early() -> Result<(), Box<dyn std::error::Error>> {
        let fx = Fixture::build(&[file("d/1", b""), file("d/2", b""), file("d/3", b"")])?;
        let mut seen = 0;
        let outcome = StdFsEnumerator.read_dir(&raw(&fx, "d")?, None, &mut |_| {
            seen += 1;
            ControlFlow::Break(())
        })?;
        assert_eq!((seen, outcome.complete), (1, false));
        Ok(())
    }

    #[test]
    fn records_modification_time_and_hidden_names() -> Result<(), Box<dyn std::error::Error>> {
        let at: Timestamp = "2024-02-29T12:00:00Z".parse()?;
        let fx = Fixture::build(&[
            file(".hidden", b"x"),
            Node::Modified {
                path: ".hidden".into(),
                at,
            },
        ])?;
        let entry = StdFsEnumerator.stat(&raw(&fx, ".hidden")?)?;
        assert_eq!(entry.times.modified, Some(at));
        assert!(entry.protection.hidden);
        Ok(())
    }

    #[test]
    fn rejects_windows_paths() -> Result<(), Box<dyn std::error::Error>> {
        let windows = RawPath::from_windows_wide(&[u16::from(b'C'), u16::from(b':')])?;
        assert!(matches!(
            StdFsEnumerator.stat(&windows),
            Err(EnumerateError::UnsupportedPath { .. })
        ));
        Ok(())
    }

    #[test]
    fn pre_epoch_times_convert_exactly() {
        let t = SystemTime::UNIX_EPOCH - std::time::Duration::from_millis(1500);
        assert_eq!(
            timestamp(t).map(|t| t.to_string()),
            Some("1969-12-31T23:59:58.5Z".into())
        );
    }
}
