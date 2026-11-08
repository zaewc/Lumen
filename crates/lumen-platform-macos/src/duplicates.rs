//! Explicit duplicate-content reads for [`lumen_application::duplicates`]
//! (ADR-0019). Scanning never uses this module: it reads file contents, so it
//! runs only on a scope the user chose for duplicate analysis.
//!
//! Every open refuses a symlink in any path component (`O_NOFOLLOW_ANY`), runs
//! with dataless-file materialization off so a cloud placeholder is never
//! downloaded, and attests the open handle against the scanned identity, size,
//! modification and change times before and after reading. Digests and contents
//! stay in memory and are never logged.

use std::ffi::OsStr;
use std::fs::{File, Metadata, OpenOptions};
use std::io::{Read as _, Seek as _, SeekFrom};
use std::os::macos::fs::MetadataExt as _;
use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::fs::{MetadataExt as _, OpenOptionsExt as _};

use lumen_application::CancelToken;
use lumen_application::duplicates::{FingerprintReader, HashFailure, HashStage};
use lumen_domain::{FilesystemEntry, PathFlavor, Timestamp};

use crate::entry::{SF_DATALESS, SF_IMMUTABLE, SF_NOUNLINK, SF_RESTRICTED, UF_IMMUTABLE};
use crate::sys::NoMaterialization;

/// Bytes hashed from each end of a file in the head and tail stages.
pub const SAMPLE_BYTES: u64 = 4096;
/// The live content buffer, independent of file size.
pub const BUFFER_BYTES: usize = 64 * 1024;

/// Files the policy protects or that have no local contents are never read.
const NOT_READ: u32 = UF_IMMUTABLE | SF_IMMUTABLE | SF_RESTRICTED | SF_NOUNLINK | SF_DATALESS;

/// Reads files for duplicate analysis. The caller must already have excluded
/// policy-protected locations from the scope.
#[derive(Debug, Clone, Copy, Default)]
pub struct DuplicateReader;

impl FingerprintReader for DuplicateReader {
    fn fingerprint(
        &self,
        entry: &FilesystemEntry,
        stage: HashStage,
        cancel: &CancelToken,
    ) -> Result<[u8; 32], HashFailure> {
        let _guard = NoMaterialization::enter().map_err(|_| HashFailure::Excluded)?;
        let mut file = open(entry)?;
        let length = entry.size.logical.get();
        let sample = length.min(SAMPLE_BYTES);
        let (offset, mut remaining) = match stage {
            HashStage::Head => (0, sample),
            HashStage::Tail => (length - sample, sample),
            HashStage::Full => (0, length),
        };
        file.seek(SeekFrom::Start(offset))
            .map_err(|_| HashFailure::Unreadable)?;
        let mut buffer = vec![0; BUFFER_BYTES];
        let mut hasher = blake3::Hasher::new();
        while remaining > 0 {
            // The engine discards every group once it sees the cancellation.
            if cancel.is_cancelled() {
                return Err(HashFailure::Excluded);
            }
            let count = usize::try_from(remaining)
                .map_or(BUFFER_BYTES, |remaining| remaining.min(BUFFER_BYTES));
            let chunk = &mut buffer[..count];
            // A file that shrank fails here; one that grew fails the check below.
            file.read_exact(chunk)
                .map_err(|_| HashFailure::Unreadable)?;
            hasher.update(chunk);
            remaining -= count as u64;
        }
        attest(&file, entry)?;
        Ok(*hasher.finalize().as_bytes())
    }

    fn verify(&self, entry: &FilesystemEntry) -> Result<(), HashFailure> {
        let _guard = NoMaterialization::enter().map_err(|_| HashFailure::Excluded)?;
        open(entry).map(drop)
    }
}

fn open(entry: &FilesystemEntry) -> Result<File, HashFailure> {
    if entry.path.flavor() != PathFlavor::Unix {
        return Err(HashFailure::Excluded);
    }
    let file = OpenOptions::new()
        .read(true)
        // `O_NOFOLLOW_ANY` covers the final component too; combined with
        // `O_NOFOLLOW` the kernel rejects the open with `EINVAL`.
        .custom_flags(libc::O_NOFOLLOW_ANY | libc::O_NONBLOCK)
        .open(OsStr::from_bytes(entry.path.as_bytes()))
        .map_err(|_| HashFailure::Unreadable)?;
    attest(&file, entry)?;
    Ok(file)
}

fn attest(file: &File, entry: &FilesystemEntry) -> Result<(), HashFailure> {
    matches(
        &file.metadata().map_err(|_| HashFailure::Unreadable)?,
        entry,
    )
}

fn matches(meta: &Metadata, entry: &FilesystemEntry) -> Result<(), HashFailure> {
    if !meta.is_file() || meta.st_flags() & NOT_READ != 0 {
        return Err(HashFailure::Excluded);
    }
    // `MetadataExt::dev` sign-extends the signed `dev_t`; undo that exactly.
    let device = i32::try_from(meta.dev().cast_signed()).map_err(|_| HashFailure::Changed)?;
    let volume = crate::entry::volume_id(device).map_err(|_| HashFailure::Changed)?;
    // The scanner's size includes a resource fork and `stat`'s does not, so a
    // file with one never matches: it is skipped, never hashed incompletely.
    if entry.identity().volume != volume
        || entry.identity().file.get() != u128::from(meta.ino())
        || entry.size.logical.get() != meta.len()
        || entry.times.modified.is_none()
        || entry.times.modified != time(meta.mtime(), meta.mtime_nsec())
        || entry.times.changed.is_none()
        || entry.times.changed != time(meta.ctime(), meta.ctime_nsec())
    {
        return Err(HashFailure::Changed);
    }
    Ok(())
}

fn time(seconds: i64, nanoseconds: i64) -> Option<Timestamp> {
    Timestamp::from_unix(seconds, u32::try_from(nanoseconds).ok()?).ok()
}

#[cfg(test)]
mod tests {
    use std::ops::ControlFlow;
    use std::path::{Path, PathBuf};

    use lumen_application::duplicates::detect;
    use lumen_application::ports::DirEnumerator as _;
    use lumen_domain::{RawPath, Verdict};
    use lumen_testkit::{Fixture, Node};

    use super::*;
    use crate::BulkEnumerator;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn file(path: &str, contents: Vec<u8>) -> Node {
        Node::File {
            path: path.into(),
            contents,
        }
    }

    fn raw(path: &Path) -> Result<RawPath, Box<dyn std::error::Error>> {
        Ok(RawPath::from_unix_bytes(
            path.as_os_str().as_bytes().to_vec(),
        )?)
    }

    /// `relative` under the fixture's resolved root: the temporary directory is
    /// itself behind the `/var` symlink, which `O_NOFOLLOW_ANY` rightly refuses.
    fn real(fx: &Fixture, relative: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
        Ok(std::fs::canonicalize(fx.root())?.join(relative))
    }

    /// The scanner's view of every entry directly inside `dir`.
    fn scanned(
        fx: &Fixture,
        dir: &str,
    ) -> Result<Vec<FilesystemEntry>, Box<dyn std::error::Error>> {
        let mut found = Vec::new();
        BulkEnumerator::new().read_dir(&raw(&real(fx, dir)?)?, None, &mut |entry| {
            found.push(entry);
            ControlFlow::Continue(())
        })?;
        Ok(found)
    }

    fn named<'a>(entries: &'a [FilesystemEntry], name: &str) -> &'a FilesystemEntry {
        entries
            .iter()
            .find(|entry| entry.path.as_bytes().ends_with(name.as_bytes()))
            .unwrap_or_else(|| unreachable!("{name} was scanned"))
    }

    fn names(files: &[FilesystemEntry]) -> Vec<String> {
        files
            .iter()
            .map(|file| {
                Path::new(OsStr::from_bytes(file.path.as_bytes()))
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default()
            })
            .collect()
    }

    fn analyze(
        entries: &[FilesystemEntry],
    ) -> Result<lumen_application::duplicates::DuplicateReport, Box<dyn std::error::Error>> {
        Ok(detect(
            entries,
            &DuplicateReader,
            &CancelToken::new(),
            &mut |_, _| {},
        )?)
    }

    #[test]
    fn equal_samples_with_different_middles_are_not_duplicates() -> TestResult {
        let big = vec![7; 3 * BUFFER_BYTES + 5];
        let mut middle = big.clone();
        middle[BUFFER_BYTES] = 8;
        let fx = Fixture::build(&[
            file("d/a", big.clone()),
            file("d/b", big),
            file("d/c", middle),
            Node::HardLink {
                path: "d/a-link".into(),
                existing: "d/a".into(),
            },
            Node::Symlink {
                path: "d/a-symlink".into(),
                target: "a".into(),
            },
        ])?;
        let report = analyze(&scanned(&fx, "d")?)?;
        assert_eq!(report.groups.len(), 1);
        let names = names(&report.groups[0].files);
        assert!(names == ["a", "b"] || names == ["a-link", "b"], "{names:?}");
        assert_eq!(report.groups[0].verdict(), Verdict::Review);
        assert_eq!(report.hard_links.len(), 1, "a and a-link are one file");
        assert!(!report.cancelled);
        Ok(())
    }

    #[test]
    fn sparse_files_hash_in_a_bounded_buffer() -> TestResult {
        let fx = Fixture::build(&[
            Node::Sparse {
                path: "d/one".into(),
                len: 16 << 20,
            },
            Node::Sparse {
                path: "d/two".into(),
                len: 16 << 20,
            },
        ])?;
        let report = analyze(&scanned(&fx, "d")?)?;
        assert_eq!(report.groups.len(), 1);
        assert_eq!(report.groups[0].files.len(), 2);
        Ok(())
    }

    #[test]
    fn a_file_changed_after_scanning_is_not_read() -> TestResult {
        let fx = Fixture::build(&[file("d/grown", vec![1; 100])])?;
        let entries = scanned(&fx, "d")?;
        std::fs::write(fx.path("d/grown")?, vec![1; 101])?;
        let entry = named(&entries, "grown");
        assert_eq!(DuplicateReader.verify(entry), Err(HashFailure::Changed));
        assert_eq!(
            DuplicateReader.fingerprint(entry, HashStage::Full, &CancelToken::new()),
            Err(HashFailure::Changed)
        );
        Ok(())
    }

    #[test]
    fn links_to_the_scanned_file_are_refused_in_every_component() -> TestResult {
        let fx = Fixture::build(&[
            file("real/f", vec![2; 10]),
            Node::Symlink {
                path: "real/f-link".into(),
                target: "f".into(),
            },
            Node::Symlink {
                path: "alias".into(),
                target: "real".into(),
            },
        ])?;
        let scanned = named(&scanned(&fx, "real")?, "/f").clone();
        assert_eq!(DuplicateReader.verify(&scanned), Ok(()));
        for through in ["real/f-link", "alias/f"] {
            let mut entry = scanned.clone();
            entry.path = raw(&real(&fx, through)?)?;
            assert_eq!(
                DuplicateReader.verify(&entry),
                Err(HashFailure::Unreadable),
                "{through}"
            );
        }
        Ok(())
    }

    #[test]
    fn an_unreadable_file_is_skipped_not_treated_as_empty() -> TestResult {
        let fx = Fixture::build(&[
            file("d/a", vec![3; 50]),
            file("d/b", vec![3; 50]),
            Node::Mode {
                path: "d/b".into(),
                mode: 0o000,
            },
        ])?;
        let report = analyze(&scanned(&fx, "d")?)?;
        // Root can read mode 000; then the files simply match.
        if report.groups.is_empty() {
            assert!(
                report
                    .skipped
                    .iter()
                    .any(|(_, why)| *why == HashFailure::Unreadable)
            );
        }
        Ok(())
    }

    #[test]
    fn cancellation_during_a_read_publishes_nothing() -> TestResult {
        let fx = Fixture::build(&[file("d/a", vec![4; 10]), file("d/b", vec![4; 10])])?;
        let entries = scanned(&fx, "d")?;
        let cancel = CancelToken::new();
        cancel.cancel();
        assert_eq!(
            DuplicateReader.fingerprint(&entries[0], HashStage::Full, &cancel),
            Err(HashFailure::Excluded)
        );
        let report = detect(&entries, &DuplicateReader, &cancel, &mut |_, _| {})?;
        assert!(report.cancelled && report.groups.is_empty());
        Ok(())
    }
}
