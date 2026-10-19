//! The macOS [`DirEnumerator`] on `getattrlistbulk(2)` (ADR-0017, ADR-0028).
//!
//! Guarantees, beyond the port contract:
//!
//! - one system call per batch of entries, into a 32 KiB buffer;
//! - the directory is opened with `O_DIRECTORY | O_NOFOLLOW` and its identity is
//!   checked on the open descriptor, so the listing describes exactly the object
//!   that was verified;
//! - mount points and firmlinks are re-read with `getattrlistat(FSOPT_NOFOLLOW)`,
//!   so every identity matches what `lstat` reports;
//! - it never downloads cloud content: every call runs with dataless-file
//!   materialization turned off for its thread, and a dataless directory is
//!   reported as not scanned instead of being listed;
//! - an entry that cannot be parsed or mapped makes the listing incomplete; it is
//!   never guessed at or silently dropped (ADR-0008).

use std::ffi::{CString, OsStr};
use std::fs::{Metadata, OpenOptions};
use std::io;
use std::ops::ControlFlow;
use std::os::macos::fs::MetadataExt as _;
use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::fs::{MetadataExt as _, OpenOptionsExt as _};
use std::path::Path;

use lumen_application::ports::{CandidateVerifier, DirEnumerator, DirOutcome, EnumerateError};
use lumen_domain::{
    AccessState, DenialReason, EntryKind, FailureKind, FileId, FileIdentity, FilesystemEntry,
    PathFlavor, RawPath, VolumeId,
};

use crate::attrbuf::{ATTR_CMN_NAME, AttributeSet, entries};
use crate::entry::{needs_lookup, request, to_entry};
use crate::sys;

/// Bytes per `getattrlistbulk` batch (ADR-0017). A name takes at most 766 bytes,
/// so every entry fits.
const BUFFER: usize = 32 * 1024;

/// `SF_DATALESS` (`sys/stat.h`): the contents are not stored locally.
const SF_DATALESS: u32 = 0x4000_0000;

/// Enumerates directories with `getattrlistbulk`. Stateless and thread-safe.
#[derive(Debug, Clone, Copy)]
pub struct BulkEnumerator {
    private_size: bool,
}

impl BulkEnumerator {
    /// An enumerator that measures `PRIVATESIZE` for every file (ADR-0028).
    pub const fn new() -> Self {
        Self { private_size: true }
    }

    /// A faster enumerator for quick scans: no `PRIVATESIZE`, so no file counts
    /// as exactly reclaimable.
    pub const fn without_private_size() -> Self {
        Self {
            private_size: false,
        }
    }

    fn lookup_set(self) -> AttributeSet {
        let mut set = request(self.private_size);
        set.common &= !ATTR_CMN_NAME;
        set
    }
}

impl Default for BulkEnumerator {
    fn default() -> Self {
        Self::new()
    }
}

impl DirEnumerator for BulkEnumerator {
    fn stat(&self, path: &RawPath) -> Result<FilesystemEntry, EnumerateError> {
        let native = c_path(path)?;
        let _guard = sys::NoMaterialization::enter().map_err(|_| EnumerateError::Unobservable {
            path: path.clone(),
            access: AccessState::Failed(FailureKind::Io),
        })?;
        let mut buffer = vec![0; BUFFER];
        sys::lookup(&native, self.lookup_set(), &mut buffer).map_err(|e| {
            EnumerateError::Unobservable {
                path: path.clone(),
                access: access_from_io(&e),
            }
        })?;
        single(path.clone(), &buffer)
    }

    fn read_dir(
        &self,
        path: &RawPath,
        expected: Option<&FileIdentity>,
        sink: &mut dyn FnMut(FilesystemEntry) -> ControlFlow<()>,
    ) -> Result<DirOutcome, EnumerateError> {
        let native = native_path(path)?;
        // Without the backstop, listing could download cloud content: do nothing.
        let Ok(_guard) = sys::NoMaterialization::enter() else {
            return Ok(DirOutcome {
                access: AccessState::Failed(FailureKind::Io),
                complete: false,
            });
        };
        let dir = match OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(native)
        {
            Ok(dir) => dir,
            Err(open_error) => return self.unopened(path, expected, &open_error),
        };
        let meta = dir.metadata().map_err(|e| EnumerateError::Unobservable {
            path: path.clone(),
            access: access_from_io(&e),
        })?;
        if !meta.is_dir() {
            return Err(EnumerateError::NotADirectory { path: path.clone() });
        }
        check_identity(path, &identity_of(path, &meta)?, expected)?;
        // A dataless directory's entries live in the cloud; listing them would
        // download them (or fail with `EDEADLK` under the guard).
        if meta.st_flags() & SF_DATALESS != 0 {
            return Ok(DirOutcome {
                access: AccessState::NotScanned,
                complete: false,
            });
        }

        let set = request(self.private_size);
        let mut buffer = vec![0; BUFFER];
        let mut lookup = Vec::new();
        let mut complete = true;
        let mut first = true;
        loop {
            let count = match sys::read_batch(&dir, set, &mut buffer) {
                Ok(0) => break,
                Ok(count) => count,
                // Nothing was listed: the contents are unknown, not empty.
                Err(e) if first => {
                    return Ok(DirOutcome {
                        access: access_from_io(&e),
                        complete: false,
                    });
                }
                Err(_) => {
                    complete = false;
                    break;
                }
            };
            first = false;
            for item in entries(&buffer, count) {
                let Some(entry) = item.ok().and_then(|raw| {
                    let name = raw.name?;
                    let child = RawPath::from_unix_bytes(join(path.as_bytes(), name)).ok()?;
                    if needs_lookup(&raw) {
                        let name = CString::new(name).ok()?;
                        lookup.resize(BUFFER, 0);
                        sys::lookup_at(&dir, &name, self.lookup_set(), &mut lookup).ok()?;
                        single(child, &lookup).ok()
                    } else {
                        to_entry(child, &raw).ok()
                    }
                }) else {
                    complete = false;
                    continue;
                };
                if sink(entry).is_break() {
                    return Ok(DirOutcome {
                        access: AccessState::Readable,
                        complete: false,
                    });
                }
            }
        }
        Ok(DirOutcome {
            access: AccessState::Readable,
            complete,
        })
    }
}

impl CandidateVerifier for BulkEnumerator {
    /// A single `getattrlist(FSOPT_NOFOLLOW)` lookup is already exact: link count,
    /// `PRIVATESIZE` (always measured here, even for quick-scan enumerators) and
    /// flags. Verification adds the identity check.
    fn verify(
        &self,
        path: &RawPath,
        expected: &FileIdentity,
    ) -> Result<FilesystemEntry, EnumerateError> {
        let entry = Self::new().stat(path)?;
        check_identity(path, entry.identity(), Some(expected))?;
        Ok(entry)
    }
}

impl BulkEnumerator {
    /// The directory could not be opened. Tells a missing, swapped or non-directory
    /// object (errors) from a directory whose contents are refused (an outcome).
    fn unopened(
        self,
        path: &RawPath,
        expected: Option<&FileIdentity>,
        open_error: &io::Error,
    ) -> Result<DirOutcome, EnumerateError> {
        let entry = self.stat(path)?;
        if entry.kind != EntryKind::Directory {
            return Err(EnumerateError::NotADirectory { path: path.clone() });
        }
        check_identity(path, entry.identity(), expected)?;
        Ok(DirOutcome {
            access: refused(
                access_from_io(open_error),
                entry.protection.system_restricted,
            ),
            complete: false,
        })
    }
}

/// Names the refusal of a directory that could not be opened, now that its
/// own metadata is known. `EPERM` comes from System Integrity Protection on a
/// directory flagged `SF_RESTRICTED`, and otherwise from privacy protection
/// (TCC: Full Disk Access, or another app's container or data vault).
/// Either way its contents stay unknown, never empty (ADR-0008).
fn refused(access: AccessState, system_restricted: bool) -> AccessState {
    match access {
        AccessState::Denied(DenialReason::Unknown) if system_restricted => {
            AccessState::Denied(DenialReason::Sip)
        }
        AccessState::Denied(DenialReason::Unknown) => AccessState::Denied(DenialReason::Tcc),
        other => other,
    }
}

/// Maps a single-object lookup buffer to an entry.
fn single(path: RawPath, buffer: &[u8]) -> Result<FilesystemEntry, EnumerateError> {
    let unreadable = |path: RawPath| EnumerateError::Unobservable {
        path,
        access: AccessState::Failed(FailureKind::Io),
    };
    match entries(buffer, 1).next() {
        Some(Ok(raw)) => to_entry(path.clone(), &raw).map_err(|_| unreadable(path)),
        _ => Err(unreadable(path)),
    }
}

fn check_identity(
    path: &RawPath,
    actual: &FileIdentity,
    expected: Option<&FileIdentity>,
) -> Result<(), EnumerateError> {
    match expected {
        Some(expected) if expected != actual => Err(EnumerateError::IdentityChanged {
            path: path.clone(),
            expected: expected.clone(),
        }),
        _ => Ok(()),
    }
}

/// Identity of an open descriptor, formatted like
/// [`volume_id`](crate::entry::volume_id) (`MetadataExt::dev` sign-extends).
fn identity_of(path: &RawPath, meta: &Metadata) -> Result<FileIdentity, EnumerateError> {
    let volume = VolumeId::new(format!("dev:{:x}", meta.dev()))
        .map_err(|_| EnumerateError::UnsupportedPath { path: path.clone() })?;
    Ok(FileIdentity {
        volume,
        file: FileId::new(u128::from(meta.ino())),
    })
}

fn native_path(path: &RawPath) -> Result<&Path, EnumerateError> {
    match path.flavor() {
        PathFlavor::Unix => Ok(Path::new(OsStr::from_bytes(path.as_bytes()))),
        PathFlavor::Windows => Err(EnumerateError::UnsupportedPath { path: path.clone() }),
    }
}

fn c_path(path: &RawPath) -> Result<CString, EnumerateError> {
    let native = native_path(path)?;
    CString::new(native.as_os_str().as_bytes())
        .map_err(|_| EnumerateError::UnsupportedPath { path: path.clone() })
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
/// opposed to plain permission bits (`EACCES`).
fn access_from_io(error: &io::Error) -> AccessState {
    match (error.kind(), error.raw_os_error()) {
        (_, Some(libc::EACCES)) => AccessState::Denied(DenialReason::Posix),
        (_, Some(libc::EPERM)) | (io::ErrorKind::PermissionDenied, _) => {
            AccessState::Denied(DenialReason::Unknown)
        }
        // Materialization refused by the guard: deliberately not read.
        (_, Some(libc::EDEADLK)) => AccessState::NotScanned,
        (io::ErrorKind::NotFound, _) => AccessState::Failed(FailureKind::Vanished),
        (io::ErrorKind::TimedOut, _) => AccessState::Failed(FailureKind::TimedOut),
        _ => AccessState::Failed(FailureKind::Io),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    #[test]
    fn an_unopenable_directory_names_privacy_or_integrity_protection() {
        use super::refused;
        let unknown = AccessState::Denied(DenialReason::Unknown);
        assert_eq!(
            refused(unknown, false),
            AccessState::Denied(DenialReason::Tcc)
        );
        assert_eq!(
            refused(unknown, true),
            AccessState::Denied(DenialReason::Sip)
        );
        let posix = AccessState::Denied(DenialReason::Posix);
        assert_eq!(
            refused(posix, true),
            posix,
            "permission bits stay permission bits"
        );
        let gone = AccessState::Failed(FailureKind::Vanished);
        assert_eq!(refused(gone, false), gone);
    }

    use lumen_scan::StdFsEnumerator;
    use lumen_testkit::{Fixture, Node};

    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn raw_path(p: &Path) -> Result<RawPath, Box<dyn std::error::Error>> {
        Ok(RawPath::from_unix_bytes(p.as_os_str().as_bytes().to_vec())?)
    }

    fn raw(fx: &Fixture, relative: &str) -> Result<RawPath, Box<dyn std::error::Error>> {
        raw_path(&fx.path(relative)?)
    }

    fn list(
        enumerator: &dyn DirEnumerator,
        path: &RawPath,
    ) -> Result<(Vec<FilesystemEntry>, DirOutcome), EnumerateError> {
        let mut found = Vec::new();
        let outcome = enumerator.read_dir(path, None, &mut |e| {
            found.push(e);
            ControlFlow::Continue(())
        })?;
        found.sort_by(|a, b| a.path.as_bytes().cmp(b.path.as_bytes()));
        Ok((found, outcome))
    }

    fn file(path: &str, contents: &[u8]) -> Node {
        Node::File {
            path: path.into(),
            contents: contents.to_vec(),
        }
    }

    fn tree() -> Result<Fixture, Box<dyn std::error::Error>> {
        Ok(Fixture::build(&[
            file("d/a.txt", b"hello"),
            file("d/.hidden", &[7; 9000]),
            Node::Dir {
                path: "d/sub".into(),
            },
            Node::Symlink {
                path: "d/link".into(),
                target: "sub".into(),
            },
            Node::Symlink {
                path: "d/dangling".into(),
                target: "nowhere".into(),
            },
            Node::HardLink {
                path: "d/again".into(),
                existing: "d/a.txt".into(),
            },
            Node::Sparse {
                path: "d/sparse".into(),
                len: 1 << 20,
            },
        ])?)
    }

    #[test]
    fn agrees_with_the_portable_enumerator() -> TestResult {
        let fx = tree()?;
        let dir = raw(&fx, "d")?;
        let (bulk, bulk_outcome) = list(&BulkEnumerator::new(), &dir)?;
        let (std, std_outcome) = list(&StdFsEnumerator, &dir)?;
        assert_eq!(bulk_outcome, std_outcome);
        assert_eq!(bulk.len(), std.len());
        for (b, s) in bulk.iter().zip(&std) {
            let what = b.path.display();
            assert_eq!(b.path, s.path);
            assert_eq!(b.kind, s.kind, "{what}");
            assert_eq!(b.size.identity, s.size.identity, "{what}");
            assert_eq!(b.access, s.access, "{what}");
            assert_eq!(b.protection, s.protection, "{what}");
            assert_eq!(b.times, s.times, "{what}");
            if b.kind != EntryKind::Directory {
                assert_eq!(b.size.logical, s.size.logical, "{what}");
                assert_eq!(b.size.allocated, s.size.allocated, "{what}");
                assert_eq!(b.size.link_count, s.size.link_count, "{what}");
            }
        }
        let a = bulk.iter().find(|e| e.path.as_bytes().ends_with(b"/a.txt"));
        assert_eq!(a.map(|e| e.size.link_count.get()), Some(2));
        assert!(
            a.is_some_and(|e| e.size.private.is_some()),
            "PRIVATESIZE measured"
        );
        Ok(())
    }

    #[test]
    fn without_private_size_nothing_is_exactly_reclaimable() -> TestResult {
        let fx = tree()?;
        let (found, _) = list(&BulkEnumerator::without_private_size(), &raw(&fx, "d")?)?;
        for entry in found.iter().filter(|e| e.kind == EntryKind::File) {
            assert_eq!(entry.size.private, None);
            assert!(entry.size.flags.may_share_blocks);
        }
        Ok(())
    }

    #[test]
    fn stat_matches_lstat_and_does_not_follow_links() -> TestResult {
        let fx = tree()?;
        for name in ["d", "d/a.txt", "d/link", "d/dangling", "d/sparse"] {
            let path = raw(&fx, name)?;
            let bulk = BulkEnumerator::new().stat(&path)?;
            let std = StdFsEnumerator.stat(&path)?;
            assert_eq!(bulk.kind, std.kind, "{name}");
            assert_eq!(bulk.size.identity, std.size.identity, "{name}");
        }
        assert!(
            BulkEnumerator::new()
                .stat(&raw_path(Path::new("/"))?)
                .is_ok()
        );
        Ok(())
    }

    #[test]
    fn identities_match_lstat_across_firmlinks_and_mount_points() -> TestResult {
        // Read-only listings of system directories: `/` holds firmlinks (`/Users`)
        // and the `/dev` mount; `/System/Volumes` holds APFS volume mounts.
        for dir in ["/", "/System/Volumes"] {
            let Ok((found, _)) = list(&BulkEnumerator::new(), &raw_path(Path::new(dir))?) else {
                continue;
            };
            for entry in found {
                let native = Path::new(OsStr::from_bytes(entry.path.as_bytes()));
                let Ok(meta) = fs::symlink_metadata(native) else {
                    continue;
                };
                assert_eq!(
                    entry.size.identity,
                    identity_of(&entry.path, &meta)?,
                    "{}",
                    entry.path.display()
                );
            }
        }
        Ok(())
    }

    #[test]
    fn large_directories_span_several_batches() -> TestResult {
        let names: Vec<Node> = (0..1500)
            .map(|i| file(&format!("big/{i:04}-{}", "n".repeat(40)), b""))
            .collect();
        let fx = Fixture::build(&names)?;
        let (found, outcome) = list(&BulkEnumerator::new(), &raw(&fx, "big")?)?;
        assert!(outcome.complete);
        assert_eq!(found.len(), 1500);
        Ok(())
    }

    #[test]
    fn refuses_links_files_and_swapped_directories() -> TestResult {
        let fx = tree()?;
        let bulk = BulkEnumerator::new();
        for not_dir in ["d/link", "d/a.txt"] {
            assert!(
                matches!(
                    list(&bulk, &raw(&fx, not_dir)?),
                    Err(EnumerateError::NotADirectory { .. })
                ),
                "{not_dir}"
            );
        }
        let other = bulk.stat(&raw(&fx, "d/sub")?)?;
        let swapped = bulk.read_dir(&raw(&fx, "d")?, Some(other.identity()), &mut |_| {
            ControlFlow::Continue(())
        });
        assert!(matches!(
            swapped,
            Err(EnumerateError::IdentityChanged { .. })
        ));
        assert!(matches!(
            bulk.stat(&raw(&fx, "missing")?),
            Err(EnumerateError::Unobservable {
                access: AccessState::Failed(FailureKind::Vanished),
                ..
            })
        ));
        Ok(())
    }

    #[test]
    fn unreadable_directory_is_denied_not_empty() -> TestResult {
        let fx = Fixture::build(&[
            file("locked/secret", b"x"),
            Node::Mode {
                path: "locked".into(),
                mode: 0o000,
            },
        ])?;
        if fs::read_dir(fx.path("locked")?).is_ok() {
            return Ok(()); // Root bypasses permission bits.
        }
        let (found, outcome) = list(&BulkEnumerator::new(), &raw(&fx, "locked")?)?;
        assert_eq!(found, Vec::<FilesystemEntry>::new());
        assert_eq!(
            outcome,
            DirOutcome {
                access: AccessState::Denied(DenialReason::Posix),
                complete: false
            }
        );
        Ok(())
    }

    #[test]
    fn listing_runs_with_materialization_off() -> TestResult {
        let fx = Fixture::build(&[file("d/1", b"")])?;
        let mut during = None;
        BulkEnumerator::new().read_dir(&raw(&fx, "d")?, None, &mut |_| {
            during = sys::thread_materialization();
            ControlFlow::Continue(())
        })?;
        assert_eq!(during, Some(1), "IOPOL_MATERIALIZE_DATALESS_FILES_OFF");
        Ok(())
    }

    #[test]
    fn verification_is_exact_and_refuses_other_objects() -> TestResult {
        let fx = tree()?;
        let quick = BulkEnumerator::without_private_size();
        let (found, _) = list(&quick, &raw(&fx, "d")?)?;
        let a = found
            .iter()
            .find(|e| e.path.as_bytes().ends_with(b"/a.txt"))
            .ok_or("a.txt not listed")?;
        assert_eq!(a.size.private, None, "quick listing");
        let verified = quick.verify(&a.path, a.identity())?;
        assert_eq!(verified.size.link_count.get(), 2, "a.txt has a hard link");
        assert!(
            verified.size.private.is_some(),
            "verification always measures"
        );
        assert_eq!(verified.size.identity, a.size.identity);

        let sub = quick.stat(&raw(&fx, "d/sub")?)?;
        assert!(matches!(
            quick.verify(&a.path, sub.identity()),
            Err(EnumerateError::IdentityChanged { .. })
        ));
        assert!(matches!(
            quick.verify(&raw(&fx, "d/gone")?, a.identity()),
            Err(EnumerateError::Unobservable { .. })
        ));
        let link = quick.verify(
            &raw(&fx, "d/link")?,
            quick.stat(&raw(&fx, "d/link")?)?.identity(),
        )?;
        assert_eq!(link.kind, EntryKind::Symlink, "described, not followed");
        Ok(())
    }

    #[test]
    fn sink_can_stop_early() -> TestResult {
        let fx = Fixture::build(&[file("d/1", b""), file("d/2", b""), file("d/3", b"")])?;
        let mut seen = 0;
        let outcome = BulkEnumerator::new().read_dir(&raw(&fx, "d")?, None, &mut |_| {
            seen += 1;
            ControlFlow::Break(())
        })?;
        assert_eq!((seen, outcome.complete), (1, false));
        Ok(())
    }
}
