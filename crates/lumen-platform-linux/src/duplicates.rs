//! Explicit duplicate-content reads for [`lumen_application::duplicates`]
//! (ADR-0019). Scanning never uses this module: it reads file contents, so it
//! runs only on a scope the user chose for duplicate analysis.
//!
//! Every open uses `openat2` with `RESOLVE_NO_SYMLINKS`, so a symlink in any
//! path component is refused, as `O_NOFOLLOW_ANY` does on macOS; a kernel
//! without `openat2` (before 5.6, or where it is filtered) reads nothing. Files
//! on FUSE filesystems, where cloud-drive clients live and a read can download,
//! are never read. The open descriptor is attested against the scanned identity
//! (`st_dev`, `st_ino`), size, modification and change times before and after
//! reading. Digests and contents stay in memory and are never logged.

use std::fs::{File, Metadata};
use std::os::unix::fs::{FileExt as _, MetadataExt as _};

use lumen_application::CancelToken;
use lumen_application::duplicates::{FingerprintReader, HashFailure, HashStage};
use lumen_domain::{FileId, FileIdentity, FilesystemEntry, PathFlavor, Timestamp, VolumeId};
use rustix::fs::{CWD, Mode, OFlags, ResolveFlags, fstatfs, openat2};

/// Bytes hashed from each end of a file in the head and tail stages.
pub(crate) const SAMPLE_BYTES: u64 = 4096;
/// The live content buffer, independent of file size.
pub(crate) const BUFFER_BYTES: usize = 64 * 1024;

/// `FUSE_SUPER_MAGIC` (`linux/magic.h`).
const FUSE_SUPER_MAGIC: u64 = 0x6573_5546;

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
        let file = open(entry)?;
        let length = entry.size.logical.get();
        let sample = length.min(SAMPLE_BYTES);
        let (mut offset, mut remaining) = match stage {
            HashStage::Head => (0, sample),
            HashStage::Tail => (length - sample, sample),
            HashStage::Full => (0, length),
        };
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
            file.read_exact_at(chunk, offset)
                .map_err(|_| HashFailure::Unreadable)?;
            hasher.update(chunk);
            offset += count as u64;
            remaining -= count as u64;
        }
        attest(&file, entry)?;
        Ok(*hasher.finalize().as_bytes())
    }

    fn verify(&self, entry: &FilesystemEntry) -> Result<(), HashFailure> {
        open(entry).map(drop)
    }
}

fn open(entry: &FilesystemEntry) -> Result<File, HashFailure> {
    if entry.path.flavor() != PathFlavor::Unix {
        return Err(HashFailure::Excluded);
    }
    let fd = openat2(
        CWD,
        entry.path.as_bytes(),
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::NOCTTY | OFlags::CLOEXEC,
        Mode::empty(),
        ResolveFlags::NO_SYMLINKS | ResolveFlags::NO_MAGICLINKS,
    )
    .map_err(|_| HashFailure::Unreadable)?;
    let on_fuse = fstatfs(&fd)
        .map_err(|_| HashFailure::Unreadable)
        .map(|stats| u64::try_from(stats.f_type).ok() == Some(FUSE_SUPER_MAGIC))?;
    if on_fuse {
        return Err(HashFailure::Excluded);
    }
    let file = File::from(fd);
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
    if !meta.is_file() {
        return Err(HashFailure::Excluded);
    }
    // The same identity `lumen_scan::StdFsEnumerator` records.
    let volume =
        VolumeId::new(format!("dev:{:x}", meta.dev())).map_err(|_| HashFailure::Changed)?;
    let identity = FileIdentity {
        volume,
        file: FileId::new(u128::from(meta.ino())),
    };
    if *entry.identity() != identity
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
    use std::os::unix::ffi::OsStrExt as _;
    use std::path::{Path, PathBuf};

    use lumen_application::duplicates::detect;
    use lumen_application::ports::DirEnumerator as _;
    use lumen_domain::{RawPath, Verdict};
    use lumen_scan::StdFsEnumerator;
    use lumen_testkit::{Fixture, Node};

    use super::*;

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

    /// `relative` under the fixture's resolved root, in case the temporary
    /// directory is itself behind a symlink, which the reader rightly refuses.
    fn real(fx: &Fixture, relative: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
        Ok(std::fs::canonicalize(fx.root())?.join(relative))
    }

    /// The scanner's view of every entry directly inside `dir`.
    fn scanned(
        fx: &Fixture,
        dir: &str,
    ) -> Result<Vec<FilesystemEntry>, Box<dyn std::error::Error>> {
        let mut found = Vec::new();
        StdFsEnumerator.read_dir(&raw(&real(fx, dir)?)?, None, &mut |entry| {
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
                file.path
                    .display()
                    .rsplit('/')
                    .next()
                    .unwrap_or_default()
                    .to_owned()
            })
            .collect()
    }

    #[test]
    fn equal_samples_with_different_middles_are_not_duplicates() -> TestResult {
        let base = vec![7; 3 * BUFFER_BYTES + 5];
        let mut middle = base.clone();
        middle[BUFFER_BYTES] = 8;
        let fx = Fixture::build(&[
            file("d/a.bin", base.clone()),
            file("d/b.bin", base),
            file("d/c.bin", middle),
        ])?;
        let entries = scanned(&fx, "d")?;
        let report = detect(
            &entries,
            &DuplicateReader,
            &CancelToken::new(),
            &mut |_, _| {},
        );
        assert_eq!(report.groups.len(), 1);
        let mut grouped = names(&report.groups[0].files);
        grouped.sort();
        assert_eq!(grouped, ["a.bin", "b.bin"]);
        assert_eq!(report.groups[0].verdict(), Verdict::Review);
        assert!(report.skipped.is_empty() && !report.cancelled);
        Ok(())
    }

    #[test]
    fn a_file_changed_after_scanning_is_not_read() -> TestResult {
        let fx = Fixture::build(&[file("d/a.bin", vec![1; 100])])?;
        let entries = scanned(&fx, "d")?;
        std::fs::write(real(&fx, "d/a.bin")?, [1; 101])?;
        let entry = named(&entries, "a.bin");
        assert_eq!(
            DuplicateReader.fingerprint(entry, HashStage::Head, &CancelToken::new()),
            Err(HashFailure::Changed)
        );
        assert_eq!(DuplicateReader.verify(entry), Err(HashFailure::Changed));
        Ok(())
    }

    #[test]
    fn links_in_any_component_and_folders_are_never_read() -> TestResult {
        let fx = Fixture::build(&[
            file("d/a.bin", vec![1; 10]),
            Node::Dir {
                path: "d/sub".into(),
            },
            Node::Symlink {
                path: "via".into(),
                target: "d".into(),
            },
        ])?;
        let entries = scanned(&fx, "d")?;
        let cancel = CancelToken::new();
        assert_eq!(
            DuplicateReader.fingerprint(named(&entries, "sub"), HashStage::Full, &cancel),
            Err(HashFailure::Excluded)
        );
        // The same file, reached through a linked folder, is refused.
        let mut linked = named(&entries, "a.bin").clone();
        linked.path = raw(&real(&fx, "via/a.bin")?)?;
        assert_eq!(
            DuplicateReader.fingerprint(&linked, HashStage::Full, &cancel),
            Err(HashFailure::Unreadable)
        );
        Ok(())
    }

    #[test]
    fn cancellation_during_a_read_publishes_nothing() -> TestResult {
        let fx = Fixture::build(&[
            file("d/a.bin", vec![3; 2 * BUFFER_BYTES]),
            file("d/b.bin", vec![3; 2 * BUFFER_BYTES]),
        ])?;
        let entries = scanned(&fx, "d")?;
        let cancel = CancelToken::new();
        cancel.cancel();
        assert_eq!(
            DuplicateReader.fingerprint(named(&entries, "a.bin"), HashStage::Full, &cancel),
            Err(HashFailure::Excluded)
        );
        let report = detect(&entries, &DuplicateReader, &cancel, &mut |_, _| {});
        assert!(report.cancelled && report.groups.is_empty());
        Ok(())
    }
}
