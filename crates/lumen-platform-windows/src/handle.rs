//! The Windows [`DirEnumerator`] on directory handles (ADR-0017).
//!
//! Guarantees, beyond the port contract:
//!
//! - each directory is opened once, by handle, as itself
//!   (`FILE_FLAG_OPEN_REPARSE_POINT`), and refused if it is a reparse point: a
//!   junction, symlink or cloud placeholder is never listed or followed;
//! - its identity is checked on that handle, and the listing comes from the same
//!   handle (`GetFileInformationByHandleEx(FileIdExtdDirectoryInfo)`, 64 KiB
//!   batches), so it describes exactly the verified object;
//! - every call runs with cloud placeholders exposed for its thread, so they are
//!   classified from directory data and never opened (no hydration);
//! - a record that cannot be parsed or mapped makes the listing incomplete; it
//!   is never guessed at or silently dropped (ADR-0008).

use std::io;
use std::ops::ControlFlow;

use lumen_application::ports::{
    CandidateVerifier, DirEnumerator, DirOutcome, EnumerateError, OwnershipProbe,
};
use lumen_domain::{
    AccessState, DenialReason, EntryKind, FailureKind, FileId, FileIdentity, FilesystemEntry,
    PathFlavor, RawPath, VolumeId,
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_COMPRESSED, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
    FILE_ATTRIBUTE_SPARSE_FILE,
};

use crate::dirinfo::{DirRecord, records};
use crate::entry::{Filesystem, HandleFacts, to_entry, to_entry_at, to_verified_entry, volume_id};
use crate::sys::{self, Access, Handle, PlaceholdersExposed, RecordBuffer};

/// Bytes per directory batch (research note 04, §B).
const BUFFER: usize = 64 * 1024;

/// Enumerates directories by handle. Stateless and thread-safe.
#[derive(Debug, Clone, Copy, Default)]
pub struct HandleEnumerator;

impl DirEnumerator for HandleEnumerator {
    fn stat(&self, path: &RawPath) -> Result<FilesystemEntry, EnumerateError> {
        let wide = windows_path(path)?;
        let unobservable = |access| EnumerateError::Unobservable {
            path: path.clone(),
            access,
        };
        let _guard = PlaceholdersExposed::enter()
            .map_err(|_| unobservable(AccessState::Failed(FailureKind::Io)))?;
        let handle =
            sys::open(&wide, Access::Attributes).map_err(|e| unobservable(access_from_io(&e)))?;
        let (volume, record) = describe(&handle).map_err(|e| unobservable(access_from_io(&e)))?;
        to_entry_at(path.clone(), &volume, &record)
            .map_err(|_| unobservable(AccessState::Failed(FailureKind::Io)))
    }

    fn read_dir(
        &self,
        path: &RawPath,
        expected: Option<&FileIdentity>,
        sink: &mut dyn FnMut(FilesystemEntry) -> ControlFlow<()>,
    ) -> Result<DirOutcome, EnumerateError> {
        let wide = windows_path(path)?;
        // Without the guard, placeholders could look like ordinary files: do
        // nothing rather than risk opening one.
        let Ok(_guard) = PlaceholdersExposed::enter() else {
            return Ok(DirOutcome {
                access: AccessState::Failed(FailureKind::Io),
                complete: false,
            });
        };
        let dir = match sys::open(&wide, Access::List) {
            Ok(dir) => dir,
            Err(open_error) => return self.unopened(path, expected, &open_error),
        };
        let unobservable = |e: &io::Error| EnumerateError::Unobservable {
            path: path.clone(),
            access: access_from_io(e),
        };
        let tag = sys::attribute_tag(&dir).map_err(|e| unobservable(&e))?;
        if tag.FileAttributes & FILE_ATTRIBUTE_DIRECTORY == 0
            || tag.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
        {
            return Err(EnumerateError::NotADirectory { path: path.clone() });
        }
        let (volume, identity) = identity_of(&dir).map_err(|e| unobservable(&e))?;
        check_identity(path, &identity, expected)?;

        let mut buffer = RecordBuffer::new(BUFFER);
        let mut complete = true;
        let mut first = true;
        loop {
            match sys::read_batch(&dir, &mut buffer) {
                Ok(true) => {}
                Ok(false) => break,
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
            }
            first = false;
            for record in records(buffer.bytes()) {
                let entry = match record {
                    Ok(record) if record.is_dot() => continue,
                    Ok(record) => to_entry(&wide, &volume, &record).ok(),
                    Err(_) => None,
                };
                let Some(entry) = entry else {
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

impl CandidateVerifier for HandleEnumerator {
    fn verify(
        &self,
        path: &RawPath,
        expected: &FileIdentity,
    ) -> Result<FilesystemEntry, EnumerateError> {
        let wide = windows_path(path)?;
        let unobservable = |access| EnumerateError::Unobservable {
            path: path.clone(),
            access,
        };
        let failed = || unobservable(AccessState::Failed(FailureKind::Io));
        let _guard = PlaceholdersExposed::enter().map_err(|_| failed())?;
        let handle =
            sys::open(&wide, Access::Attributes).map_err(|e| unobservable(access_from_io(&e)))?;
        let (volume, record) = describe(&handle).map_err(|e| unobservable(access_from_io(&e)))?;
        let identity = FileIdentity {
            volume: volume.clone(),
            file: FileId::new(record.file_id),
        };
        check_identity(path, &identity, Some(expected))?;

        let standard = sys::standard(&handle).map_err(|e| unobservable(access_from_io(&e)))?;
        let packed =
            record.attributes & (FILE_ATTRIBUTE_COMPRESSED | FILE_ATTRIBUTE_SPARSE_FILE) != 0;
        let facts = HandleFacts {
            links: standard.NumberOfLinks,
            // Unreadable compressed size: the verified entry keeps sharing possible.
            compressed_size: packed
                .then(|| sys::compression(&handle).ok().map(|c| c.CompressedFileSize))
                .flatten(),
            filesystem: sys::filesystem_name(&handle)
                .map_or(Filesystem::Other, |name| Filesystem::from_name(&name)),
        };
        to_verified_entry(path.clone(), &volume, &record, &facts).map_err(|_| failed())
    }
}

impl OwnershipProbe for HandleEnumerator {
    /// Opens the object as itself (never a link target), checks it is still the
    /// scanned object, and compares its owner SID with the process token's
    /// user (roadmap 10.2c). Anything unestablished is `None`.
    fn owned_by_current_user(&self, path: &RawPath, expected: &FileIdentity) -> Option<bool> {
        let wide = windows_path(path).ok()?;
        let _guard = PlaceholdersExposed::enter().ok()?;
        let handle = sys::open(&wide, Access::Owner).ok()?;
        let (volume, record) = describe(&handle).ok()?;
        let identity = FileIdentity {
            volume,
            file: FileId::new(record.file_id),
        };
        check_identity(path, &identity, Some(expected)).ok()?;
        sys::owned_by_current_user(&handle).ok()
    }
}

impl HandleEnumerator {
    /// The directory could not be opened for listing. Tells a missing, swapped or
    /// non-directory object (errors) from a directory whose contents are refused
    /// (an outcome).
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
            access: access_from_io(open_error),
            complete: false,
        })
    }
}

/// Volume and identity of an open object.
fn identity_of(handle: &Handle) -> io::Result<(VolumeId, FileIdentity)> {
    let info = sys::id(handle)?;
    let volume = volume_id(info.VolumeSerialNumber).map_err(io::Error::other)?;
    let identity = FileIdentity {
        volume: volume.clone(),
        file: FileId::new(u128::from_le_bytes(info.FileId.Identifier)),
    };
    Ok((volume, identity))
}

/// The attributes of an open object, in the shape of a directory record.
fn describe(handle: &Handle) -> io::Result<(VolumeId, DirRecord)> {
    let (volume, identity) = identity_of(handle)?;
    let basic = sys::basic(handle)?;
    let standard = sys::standard(handle)?;
    let tag = sys::attribute_tag(handle)?;
    let record = DirRecord {
        created: basic.CreationTime,
        accessed: basic.LastAccessTime,
        written: basic.LastWriteTime,
        changed: basic.ChangeTime,
        end_of_file: standard.EndOfFile,
        allocation_size: standard.AllocationSize,
        attributes: basic.FileAttributes,
        reparse_tag: tag.ReparseTag,
        file_id: identity.file.get(),
        name: Vec::new(),
    };
    Ok((volume, record))
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

fn windows_path(path: &RawPath) -> Result<Vec<u16>, EnumerateError> {
    match path.flavor() {
        PathFlavor::Windows => path
            .to_windows_wide()
            .ok_or_else(|| EnumerateError::UnsupportedPath { path: path.clone() }),
        PathFlavor::Unix => Err(EnumerateError::UnsupportedPath { path: path.clone() }),
    }
}

fn access_from_io(error: &io::Error) -> AccessState {
    match error.kind() {
        io::ErrorKind::PermissionDenied => AccessState::Denied(DenialReason::Acl),
        io::ErrorKind::NotFound => AccessState::Failed(FailureKind::Vanished),
        io::ErrorKind::TimedOut => AccessState::Failed(FailureKind::TimedOut),
        _ => AccessState::Failed(FailureKind::Io),
    }
}

#[cfg(test)]
mod tests {
    use std::os::windows::ffi::OsStrExt as _;
    use std::path::Path;
    use std::process::Command;

    use lumen_domain::ByteCount;

    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn raw(path: &Path) -> Result<RawPath, Box<dyn std::error::Error>> {
        let wide: Vec<u16> = path.as_os_str().encode_wide().collect();
        Ok(RawPath::from_windows_wide(&wide)?)
    }

    fn list(path: &RawPath) -> Result<(Vec<FilesystemEntry>, DirOutcome), EnumerateError> {
        let mut found = Vec::new();
        let outcome = HandleEnumerator.read_dir(path, None, &mut |e| {
            found.push(e);
            ControlFlow::Continue(())
        })?;
        found.sort_by(|a, b| a.path.as_bytes().cmp(b.path.as_bytes()));
        Ok((found, outcome))
    }

    fn name(entry: &FilesystemEntry) -> String {
        entry
            .path
            .display()
            .rsplit('\\')
            .next()
            .unwrap_or_default()
            .to_owned()
    }

    #[test]
    fn ownership_is_known_for_observed_objects_and_never_guessed() -> TestResult {
        let dir = tempfile::tempdir()?;
        let file = dir.path().join("mine.txt");
        std::fs::write(&file, b"x")?;
        let path = raw(&file)?;
        let entry = HandleEnumerator.stat(&path)?;
        // Under an elevated token new files belong to Administrators, so only
        // that the answer is known is asserted for a file the test made.
        assert!(
            HandleEnumerator
                .owned_by_current_user(&path, entry.identity())
                .is_some()
        );
        // System files belong to TrustedInstaller, never to the user.
        let system = raw(Path::new(r"C:\Windows\System32\kernel32.dll"))?;
        let system_entry = HandleEnumerator.stat(&system)?;
        assert_eq!(
            HandleEnumerator.owned_by_current_user(&system, system_entry.identity()),
            Some(false)
        );
        // A replaced object is never attributed.
        let mut other = entry.identity().clone();
        other.file = FileId::new(other.file.get() ^ 1);
        assert_eq!(HandleEnumerator.owned_by_current_user(&path, &other), None);
        Ok(())
    }

    #[test]
    fn lists_files_and_directories_by_handle() -> TestResult {
        let dir = tempfile::tempdir()?;
        std::fs::write(dir.path().join("a.txt"), b"hello")?;
        std::fs::create_dir(dir.path().join("sub"))?;
        let (found, outcome) = list(&raw(dir.path())?)?;
        assert_eq!(
            outcome,
            DirOutcome {
                access: AccessState::Readable,
                complete: true
            }
        );
        let kinds: Vec<_> = found.iter().map(|e| (name(e), e.kind)).collect();
        assert_eq!(
            kinds,
            [
                ("a.txt".to_owned(), EntryKind::File),
                ("sub".to_owned(), EntryKind::Directory)
            ]
        );
        assert_eq!(found[0].size.logical, ByteCount::new(5));
        assert!(found[0].times.modified.is_some());
        assert_eq!(found[1].access, AccessState::NotScanned);
        Ok(())
    }

    #[test]
    fn listed_identities_match_stat_and_relisting() -> TestResult {
        let dir = tempfile::tempdir()?;
        std::fs::create_dir(dir.path().join("sub"))?;
        std::fs::write(dir.path().join("sub").join("f"), b"x")?;
        let (found, _) = list(&raw(dir.path())?)?;
        let sub = &found[0];
        let stat = HandleEnumerator.stat(&sub.path)?;
        assert_eq!(stat.size.identity, sub.size.identity);
        assert_eq!(stat.kind, EntryKind::Directory);
        // The scheduler lists children with the identity their parent reported.
        let outcome = HandleEnumerator.read_dir(&sub.path, Some(sub.identity()), &mut |_| {
            ControlFlow::Continue(())
        })?;
        assert!(outcome.complete);
        Ok(())
    }

    #[test]
    fn junctions_are_recorded_but_never_listed() -> TestResult {
        let dir = tempfile::tempdir()?;
        let target = dir.path().join("target");
        std::fs::create_dir(&target)?;
        std::fs::write(target.join("inside"), b"x")?;
        let link = dir.path().join("link");
        let made = Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(&target)
            .output()?;
        assert!(
            made.status.success(),
            "{}",
            String::from_utf8_lossy(&made.stderr)
        );

        let (found, _) = list(&raw(dir.path())?)?;
        let kinds: Vec<_> = found.iter().map(|e| (name(e), e.kind)).collect();
        assert_eq!(
            kinds,
            [
                ("link".to_owned(), EntryKind::Junction),
                ("target".to_owned(), EntryKind::Directory)
            ]
        );
        assert!(matches!(
            list(&raw(&link)?),
            Err(EnumerateError::NotADirectory { .. })
        ));
        assert_eq!(
            HandleEnumerator.stat(&raw(&link)?)?.kind,
            EntryKind::Junction
        );
        Ok(())
    }

    #[test]
    fn large_directories_span_several_batches() -> TestResult {
        let dir = tempfile::tempdir()?;
        for i in 0..1500 {
            std::fs::write(dir.path().join(format!("{i:04}-{}", "n".repeat(40))), b"")?;
        }
        let (found, outcome) = list(&raw(dir.path())?)?;
        assert!(outcome.complete);
        assert_eq!(found.len(), 1500);
        Ok(())
    }

    #[test]
    fn refuses_files_missing_paths_and_swapped_directories() -> TestResult {
        let dir = tempfile::tempdir()?;
        std::fs::write(dir.path().join("f"), b"x")?;
        std::fs::create_dir(dir.path().join("one"))?;
        std::fs::create_dir(dir.path().join("two"))?;
        assert!(matches!(
            list(&raw(&dir.path().join("f"))?),
            Err(EnumerateError::NotADirectory { .. })
        ));
        assert!(matches!(
            HandleEnumerator.stat(&raw(&dir.path().join("missing"))?),
            Err(EnumerateError::Unobservable {
                access: AccessState::Failed(FailureKind::Vanished),
                ..
            })
        ));
        let one = HandleEnumerator.stat(&raw(&dir.path().join("one"))?)?;
        let swapped = HandleEnumerator.read_dir(
            &raw(&dir.path().join("two"))?,
            Some(one.identity()),
            &mut |_| ControlFlow::Continue(()),
        );
        assert!(matches!(
            swapped,
            Err(EnumerateError::IdentityChanged { .. })
        ));
        let unix = RawPath::from_unix_bytes(b"/tmp".to_vec())?;
        assert!(matches!(
            HandleEnumerator.stat(&unix),
            Err(EnumerateError::UnsupportedPath { .. })
        ));
        Ok(())
    }

    fn listed(dir: &Path, name: &str) -> Result<FilesystemEntry, Box<dyn std::error::Error>> {
        let (found, _) = list(&raw(dir)?)?;
        found
            .into_iter()
            .find(|e| e.path.display().ends_with(&format!("\\{name}")))
            .ok_or_else(|| format!("{name} not listed").into())
    }

    #[test]
    fn verification_reads_exact_link_counts_on_ntfs() -> TestResult {
        let dir = tempfile::tempdir()?;
        std::fs::write(dir.path().join("a"), vec![1; 10_000])?;
        std::fs::hard_link(dir.path().join("a"), dir.path().join("b"))?;
        let listed = listed(dir.path(), "a")?;
        assert!(listed.size.flags.may_share_blocks, "a listing cannot know");
        let verified = HandleEnumerator.verify(&listed.path, listed.identity())?;
        assert_eq!(verified.size.link_count.get(), 2);
        assert_eq!(verified.size.identity, listed.size.identity);
        assert!(
            !verified.size.flags.may_share_blocks,
            "NTFS shares no blocks"
        );
        Ok(())
    }

    #[test]
    fn verification_measures_compressed_files() -> TestResult {
        let dir = tempfile::tempdir()?;
        let file = dir.path().join("zeros");
        std::fs::write(&file, vec![0; 1 << 20])?;
        let packed = Command::new("compact")
            .args(["/c", "/q"])
            .arg(&file)
            .output()?;
        assert!(
            packed.status.success(),
            "{}",
            String::from_utf8_lossy(&packed.stdout)
        );
        let listed = listed(dir.path(), "zeros")?;
        assert!(listed.size.flags.compressed);
        let verified = HandleEnumerator.verify(&listed.path, listed.identity())?;
        assert!(
            verified.size.allocated < ByteCount::new(1 << 20),
            "{:?}",
            verified.size.allocated
        );
        assert_eq!(verified.size.logical, ByteCount::new(1 << 20));
        assert!(!verified.size.flags.may_share_blocks);
        Ok(())
    }

    #[test]
    fn verification_refuses_swapped_and_missing_candidates() -> TestResult {
        let dir = tempfile::tempdir()?;
        std::fs::write(dir.path().join("one"), b"1")?;
        std::fs::write(dir.path().join("two"), b"2")?;
        let one = listed(dir.path(), "one")?;
        let two = listed(dir.path(), "two")?;
        assert!(matches!(
            HandleEnumerator.verify(&two.path, one.identity()),
            Err(EnumerateError::IdentityChanged { .. })
        ));
        // A candidate whose path no longer resolves (nothing is deleted here).
        assert!(matches!(
            HandleEnumerator.verify(&raw(&dir.path().join("gone"))?, one.identity()),
            Err(EnumerateError::Unobservable {
                access: AccessState::Failed(FailureKind::Vanished),
                ..
            })
        ));
        Ok(())
    }

    #[test]
    fn verifying_a_junction_describes_it_without_following() -> TestResult {
        let dir = tempfile::tempdir()?;
        let target = dir.path().join("target");
        std::fs::create_dir(&target)?;
        let link = dir.path().join("link");
        let made = Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(&target)
            .output()?;
        assert!(made.status.success());
        let listed = listed(dir.path(), "link")?;
        let verified = HandleEnumerator.verify(&listed.path, listed.identity())?;
        assert_eq!(verified.kind, EntryKind::Junction);
        assert_eq!(verified.size.identity, listed.size.identity);
        Ok(())
    }

    #[test]
    fn listing_runs_with_placeholders_exposed_and_can_stop_early() -> TestResult {
        let dir = tempfile::tempdir()?;
        for name in ["1", "2", "3"] {
            std::fs::write(dir.path().join(name), b"")?;
        }
        let mut seen = 0;
        let mut mode = None;
        let outcome = HandleEnumerator.read_dir(&raw(dir.path())?, None, &mut |_| {
            seen += 1;
            mode = Some(sys::thread_placeholder_mode());
            ControlFlow::Break(())
        })?;
        assert_eq!((seen, outcome.complete), (1, false));
        assert_eq!(mode, Some(2), "PHCM_EXPOSE_PLACEHOLDERS");
        Ok(())
    }
}
