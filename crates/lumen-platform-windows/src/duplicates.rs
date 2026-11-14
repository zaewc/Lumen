//! Explicit duplicate-content reads for [`lumen_application::duplicates`]
//! (ADR-0019). Scanning never uses this module: it reads file contents, so it
//! runs only on a scope the user chose for duplicate analysis.
//!
//! Every read runs with cloud placeholders exposed for its thread. The file is
//! first opened for its attributes only, and refused if it is a reparse point
//! (link, junction, placeholder), offline, recalled on open or access, a system
//! file or a directory, or if it is not the scanned object. Only then is it
//! opened again, as itself, with read access and all share modes, and that
//! handle is attested against the scanned identity (volume serial and 128-bit
//! file ID), size, last-write and change times before and after reading. A
//! junction in a parent folder could lead an open elsewhere, but the file ID
//! would then differ and the file is skipped as changed. Digests and contents
//! stay in memory and are never logged.

use lumen_application::CancelToken;
use lumen_application::duplicates::{FingerprintReader, HashFailure, HashStage};
use lumen_domain::{EntryKind, FilesystemEntry, PathFlavor};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_OFFLINE, FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS,
    FILE_ATTRIBUTE_RECALL_ON_OPEN, FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_SYSTEM,
};

use crate::entry::to_entry_at;
use crate::handle::describe;
use crate::sys::{self, Access, Handle, PlaceholdersExposed};

/// Bytes hashed from each end of a file in the head and tail stages.
pub(crate) const SAMPLE_BYTES: u64 = 4096;
/// The live content buffer, independent of file size.
pub(crate) const BUFFER_BYTES: usize = 64 * 1024;

/// Files that have no local contents, are not plain files, or belong to the
/// system are never read.
const NOT_READ: u32 = FILE_ATTRIBUTE_REPARSE_POINT
    | FILE_ATTRIBUTE_OFFLINE
    | FILE_ATTRIBUTE_RECALL_ON_OPEN
    | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS
    | FILE_ATTRIBUTE_SYSTEM
    | FILE_ATTRIBUTE_DIRECTORY;

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
        let _guard = PlaceholdersExposed::enter().map_err(|_| HashFailure::Excluded)?;
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
            read_exact_at(&file, offset, chunk)?;
            hasher.update(chunk);
            offset += count as u64;
            remaining -= count as u64;
        }
        attest(&file, entry)?;
        Ok(*hasher.finalize().as_bytes())
    }

    fn verify(&self, entry: &FilesystemEntry) -> Result<(), HashFailure> {
        let _guard = PlaceholdersExposed::enter().map_err(|_| HashFailure::Excluded)?;
        open(entry).map(drop)
    }
}

fn read_exact_at(file: &Handle, mut offset: u64, mut chunk: &mut [u8]) -> Result<(), HashFailure> {
    while !chunk.is_empty() {
        let read = sys::read_at(file, offset, chunk).map_err(|_| HashFailure::Unreadable)?;
        if read == 0 {
            return Err(HashFailure::Unreadable);
        }
        offset += read as u64;
        chunk = &mut chunk[read..];
    }
    Ok(())
}

/// Opens `entry` for reading once its attributes and identity allow it.
fn open(entry: &FilesystemEntry) -> Result<Handle, HashFailure> {
    if entry.path.flavor() != PathFlavor::Windows {
        return Err(HashFailure::Excluded);
    }
    let wide = entry.path.to_windows_wide().ok_or(HashFailure::Excluded)?;
    attest(
        &sys::open(&wide, Access::Attributes).map_err(|_| HashFailure::Unreadable)?,
        entry,
    )?;
    let file = sys::open(&wide, Access::Read).map_err(|_| HashFailure::Unreadable)?;
    attest(&file, entry)?;
    Ok(file)
}

fn attest(handle: &Handle, entry: &FilesystemEntry) -> Result<(), HashFailure> {
    let (volume, record) = describe(handle).map_err(|_| HashFailure::Unreadable)?;
    if record.attributes & NOT_READ != 0 {
        return Err(HashFailure::Excluded);
    }
    let seen =
        to_entry_at(entry.path.clone(), &volume, &record).map_err(|_| HashFailure::Changed)?;
    if seen.kind != EntryKind::File
        || seen.identity() != entry.identity()
        || seen.size.logical != entry.size.logical
        || entry.times.modified.is_none()
        || seen.times.modified != entry.times.modified
        || entry.times.changed.is_none()
        || seen.times.changed != entry.times.changed
    {
        return Err(HashFailure::Changed);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::ops::ControlFlow;
    use std::os::windows::ffi::OsStrExt as _;
    use std::path::Path;

    use lumen_application::duplicates::detect;
    use lumen_application::ports::DirEnumerator as _;
    use lumen_domain::{RawPath, Verdict};

    use super::*;
    use crate::HandleEnumerator;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn raw(path: &Path) -> Result<RawPath, Box<dyn std::error::Error>> {
        let wide: Vec<u16> = path.as_os_str().encode_wide().collect();
        Ok(RawPath::from_windows_wide(&wide)?)
    }

    /// The scanner's view of every entry directly inside `dir`.
    fn scanned(dir: &Path) -> Result<Vec<FilesystemEntry>, Box<dyn std::error::Error>> {
        let mut found = Vec::new();
        HandleEnumerator.read_dir(&raw(dir)?, None, &mut |entry| {
            found.push(entry);
            ControlFlow::Continue(())
        })?;
        Ok(found)
    }

    fn named<'a>(entries: &'a [FilesystemEntry], name: &str) -> &'a FilesystemEntry {
        entries
            .iter()
            .find(|entry| entry.path.display().ends_with(name))
            .unwrap_or_else(|| unreachable!("{name} was scanned"))
    }

    fn names(files: &[FilesystemEntry]) -> Vec<String> {
        files
            .iter()
            .map(|file| {
                file.path
                    .display()
                    .rsplit('\\')
                    .next()
                    .unwrap_or_default()
                    .to_owned()
            })
            .collect()
    }

    #[test]
    fn equal_samples_with_different_middles_are_not_duplicates() -> TestResult {
        let dir = tempfile::tempdir()?;
        let base = vec![7; 3 * BUFFER_BYTES + 5];
        let mut middle = base.clone();
        middle[BUFFER_BYTES] = 8;
        std::fs::write(dir.path().join("a.bin"), &base)?;
        std::fs::write(dir.path().join("b.bin"), &base)?;
        std::fs::write(dir.path().join("c.bin"), &middle)?;
        let entries = scanned(dir.path())?;
        let report = detect(
            &entries,
            &DuplicateReader,
            &CancelToken::new(),
            &mut |_, _| {},
        );
        assert_eq!(report.groups.len(), 1);
        assert_eq!(names(&report.groups[0].files), ["a.bin", "b.bin"]);
        assert_eq!(report.groups[0].verdict(), Verdict::Review);
        assert!(report.skipped.is_empty() && !report.cancelled);
        Ok(())
    }

    #[test]
    fn a_file_changed_after_scanning_is_not_read() -> TestResult {
        let dir = tempfile::tempdir()?;
        std::fs::write(dir.path().join("a.bin"), [1; 100])?;
        let entries = scanned(dir.path())?;
        std::fs::write(dir.path().join("a.bin"), [1; 101])?;
        let entry = named(&entries, "a.bin");
        let cancel = CancelToken::new();
        assert_eq!(
            DuplicateReader.fingerprint(entry, HashStage::Head, &cancel),
            Err(HashFailure::Changed)
        );
        assert_eq!(DuplicateReader.verify(entry), Err(HashFailure::Changed));
        Ok(())
    }

    #[test]
    fn folders_and_missing_files_are_never_read_as_empty() -> TestResult {
        let dir = tempfile::tempdir()?;
        std::fs::create_dir(dir.path().join("sub"))?;
        std::fs::write(dir.path().join("gone.bin"), [1; 10])?;
        let entries = scanned(dir.path())?;
        let cancel = CancelToken::new();
        assert_eq!(
            DuplicateReader.fingerprint(named(&entries, "sub"), HashStage::Full, &cancel),
            Err(HashFailure::Excluded)
        );
        drop(dir);
        assert_eq!(
            DuplicateReader.fingerprint(named(&entries, "gone.bin"), HashStage::Full, &cancel),
            Err(HashFailure::Unreadable)
        );
        Ok(())
    }

    #[test]
    fn cancellation_during_a_read_publishes_nothing() -> TestResult {
        let dir = tempfile::tempdir()?;
        std::fs::write(dir.path().join("a.bin"), vec![3; 2 * BUFFER_BYTES])?;
        std::fs::write(dir.path().join("b.bin"), vec![3; 2 * BUFFER_BYTES])?;
        let entries = scanned(dir.path())?;
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
