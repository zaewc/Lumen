//! Explicit, local-only duplicate analysis (ADR-0019), separate from scanning.
//! The reader attests each hash against unchanged metadata. No group chooses a
//! keeper or authorizes cleanup; hard links are reported separately and physical
//! savings remain unknown until a user selects copies and storage is measured.

use std::collections::BTreeMap;
use std::ops::ControlFlow;

use lumen_domain::{ByteCount, EntryKind, FileIdentity, FilesystemEntry, RawPath, Verdict};

use crate::CancelToken;

/// Successively more expensive filters; only `Full` establishes content equality.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashStage {
    /// First sample of the file.
    Head,
    /// Last sample of the file.
    Tail,
    /// Every byte, using BLAKE3.
    Full,
}

/// Why a selected file was not included; none means absent or empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashFailure {
    /// Identity, length, modification time or change time changed.
    Changed,
    /// Access was refused or the file could not be read completely.
    Unreadable,
    /// A protected, cloud-backed, special or otherwise unsupported file.
    Excluded,
}

/// An adapter must open without following links or hydrating placeholders, hash
/// with a bounded buffer and cooperative cancellation, and verify the same open
/// handle's identity, size, modification and change times before/after reading.
/// Hashes and file contents must never be logged or sent to any model.
pub trait FingerprintReader {
    /// Read the requested stage of an unchanged file.
    ///
    /// # Errors
    /// Returns the reason a complete, stable fingerprint could not be obtained.
    fn fingerprint(
        &self,
        file: &FilesystemEntry,
        stage: HashStage,
        cancel: &CancelToken,
    ) -> Result<[u8; 32], HashFailure>;
    /// Recheck each member immediately before publishing a group.
    ///
    /// # Errors
    /// Returns the reason the original metadata can no longer be attested.
    fn verify(&self, file: &FilesystemEntry) -> Result<(), HashFailure>;
}

/// Identical content in distinct file identities. Files may still share blocks:
/// clone IDs and storage flags remain attached to every member's metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateGroup {
    /// Local-only full BLAKE3 digest, never a preliminary sample hash.
    pub content_id: [u8; 32],
    /// Members, ordered by byte-exact path.
    pub files: Vec<FilesystemEntry>,
}

impl DuplicateGroup {
    /// Duplicates are evidence for review only, including identical backups.
    pub const fn verdict(&self) -> Verdict {
        Verdict::Review
    }
    /// Extra logical copies; deliberately not a physical space estimate.
    pub fn logical_redundancy(&self) -> ByteCount {
        self.files
            .iter()
            .skip(1)
            .fold(ByteCount::ZERO, |sum, file| {
                sum.saturating_add(file.size.logical)
            })
    }
}

/// Outcome, with diagnostics kept separate from verified duplicate groups.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DuplicateReport {
    /// Groups that still have at least two distinct, freshly verified identities.
    pub groups: Vec<DuplicateGroup>,
    /// Multiple paths to the same file, not redundant copies.
    pub hard_links: Vec<Vec<FilesystemEntry>>,
    /// Files deliberately excluded, changed or unreadable.
    pub skipped: Vec<(RawPath, HashFailure)>,
    /// Cancellation returns no duplicate groups; work is explicitly incomplete.
    pub cancelled: bool,
}

fn verify_files(
    files: Vec<&FilesystemEntry>,
    reader: &dyn FingerprintReader,
    report: &mut DuplicateReport,
) -> Vec<FilesystemEntry> {
    files
        .into_iter()
        .filter_map(|file| match reader.verify(file) {
            Ok(()) => Some(file.clone()),
            Err(failure) => {
                report.skipped.push((file.path.clone(), failure));
                None
            }
        })
        .collect()
}

fn group_sizes<'a>(
    entries: &'a [FilesystemEntry],
    report: &mut DuplicateReport,
) -> Vec<Vec<&'a FilesystemEntry>> {
    let mut identities: BTreeMap<FileIdentity, Vec<&FilesystemEntry>> = BTreeMap::new();
    for entry in entries {
        if entry.kind != EntryKind::File
            || !entry.access.is_observed()
            || entry.protection.os_protected()
            || entry.protection.system_attribute
            || entry.size.flags.cloud_placeholder
            || entry.size.flags.dataless
        {
            report
                .skipped
                .push((entry.path.clone(), HashFailure::Excluded));
        } else if entry.size.logical != ByteCount::ZERO {
            identities
                .entry(entry.identity().clone())
                .or_default()
                .push(entry);
        }
    }
    let mut sizes: BTreeMap<ByteCount, Vec<&FilesystemEntry>> = BTreeMap::new();
    for mut links in identities.into_values() {
        links.sort_by(|a, b| a.path.cmp(&b.path));
        links.dedup_by(|a, b| a.path == b.path);
        if links.len() > 1 {
            report
                .hard_links
                .push(links.iter().map(|file| (*file).clone()).collect());
        }
        if let Some(file) = links.first() {
            sizes.entry(file.size.logical).or_default().push(file);
        }
    }
    sizes
        .into_values()
        .filter(|files| files.len() > 1)
        .collect()
}

/// The staged comparison, fed a batch of files at a time, sequentially, with at
/// most one reader active. Every file of a given size must arrive in the same
/// batch (files of one identity share a size, so hard links do too); then only
/// one batch is held at a time, however many files the scope has. Reports
/// progress after every attempted fingerprint.
pub struct Detector<'a> {
    reader: &'a dyn FingerprintReader,
    cancel: &'a CancelToken,
    report: DuplicateReport,
    completed: usize,
}

impl std::fmt::Debug for Detector<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Detector")
            .field("report", &self.report)
            .field("completed", &self.completed)
            .finish_non_exhaustive()
    }
}

impl<'a> Detector<'a> {
    /// An empty comparison reading with `reader` until `cancel`.
    #[must_use]
    pub fn new(reader: &'a dyn FingerprintReader, cancel: &'a CancelToken) -> Self {
        Self {
            reader,
            cancel,
            report: DuplicateReport::default(),
            completed: 0,
        }
    }

    /// Compares `entries`, holding every file of each size among them.
    /// Breaks once cancelled.
    pub fn add(
        &mut self,
        entries: &[FilesystemEntry],
        progress: &mut dyn FnMut(HashStage, usize),
    ) -> ControlFlow<()> {
        let mut groups = group_sizes(entries, &mut self.report);
        for stage in [HashStage::Head, HashStage::Tail, HashStage::Full] {
            let mut next = Vec::new();
            for files in groups {
                let mut hashes: BTreeMap<[u8; 32], Vec<&FilesystemEntry>> = BTreeMap::new();
                for file in files {
                    if self.cancel.is_cancelled() {
                        self.report.cancelled = true;
                        return ControlFlow::Break(());
                    }
                    match self.reader.fingerprint(file, stage, self.cancel) {
                        Ok(digest) => hashes.entry(digest).or_default().push(file),
                        Err(failure) => self.report.skipped.push((file.path.clone(), failure)),
                    }
                    self.completed += 1;
                    progress(stage, self.completed);
                }
                for (digest, files) in hashes.into_iter().filter(|(_, files)| files.len() > 1) {
                    if stage == HashStage::Full {
                        let mut verified = verify_files(files, self.reader, &mut self.report);
                        if verified.len() > 1 {
                            verified.sort_by(|a, b| a.path.cmp(&b.path));
                            self.report.groups.push(DuplicateGroup {
                                content_id: digest,
                                files: verified,
                            });
                        }
                    } else {
                        next.push(files);
                    }
                }
            }
            groups = next;
        }
        if self.cancel.is_cancelled() {
            self.report.cancelled = true;
            return ControlFlow::Break(());
        }
        ControlFlow::Continue(())
    }

    /// The report: hard links checked again, everything sorted; no groups at
    /// all once cancelled.
    #[must_use]
    pub fn finish(self) -> DuplicateReport {
        let mut report = self.report;
        if report.cancelled || self.cancel.is_cancelled() {
            report.groups.clear();
            report.hard_links.clear();
            report.cancelled = true;
            return report;
        }
        let hard_links = std::mem::take(&mut report.hard_links);
        for links in hard_links {
            let verified = verify_files(links.iter().collect(), self.reader, &mut report);
            if verified.len() > 1 {
                report.hard_links.push(verified);
            }
        }
        if self.cancel.is_cancelled() {
            report.groups.clear();
            report.hard_links.clear();
            report.cancelled = true;
        }
        report
            .groups
            .sort_by(|a, b| a.files[0].path.cmp(&b.files[0].path));
        report.hard_links.sort_by(|a, b| a[0].path.cmp(&b[0].path));
        report.skipped.sort_by(|a, b| a.0.cmp(&b.0));
        report
    }
}

/// [`Detector`] over `entries` in one batch: an explicitly selected scope,
/// held in memory.
pub fn detect(
    entries: &[FilesystemEntry],
    reader: &dyn FingerprintReader,
    cancel: &CancelToken,
    progress: &mut dyn FnMut(HashStage, usize),
) -> DuplicateReport {
    let mut detector = Detector::new(reader, cancel);
    // A break only means cancelled, which `finish` reports either way.
    if detector.add(entries, progress).is_break() {
        return detector.finish();
    }
    detector.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumen_domain::{
        AccessState, EntryTimes, FileId, Protection, SizeFacts, SizeFlags, VolumeId,
    };
    use std::cell::Cell;
    use std::num::NonZeroU32;

    fn file(id: u8) -> FilesystemEntry {
        FilesystemEntry {
            path: RawPath::from_unix_bytes(format!("/selected/{id}").into_bytes())
                .unwrap_or_else(|_| unreachable!()),
            kind: EntryKind::File,
            access: AccessState::Readable,
            protection: Protection::default(),
            times: EntryTimes::default(),
            size: SizeFacts {
                identity: FileIdentity {
                    volume: VolumeId::new("v").unwrap_or_else(|_| unreachable!()),
                    file: FileId::new(u128::from(id)),
                },
                logical: ByteCount::new(16),
                allocated: ByteCount::new(8),
                private: None,
                clone_id: None,
                link_count: NonZeroU32::MIN,
                flags: SizeFlags::default(),
            },
        }
    }
    struct Reader {
        reads: Cell<usize>,
        changed: bool,
    }
    impl FingerprintReader for Reader {
        fn fingerprint(
            &self,
            file: &FilesystemEntry,
            stage: HashStage,
            _: &CancelToken,
        ) -> Result<[u8; 32], HashFailure> {
            self.reads.set(self.reads.get() + 1);
            // Deliberate sample collisions; only 1 and 3 have identical full content.
            Ok(
                [if stage == HashStage::Full && file.identity().file != FileId::new(2) {
                    1
                } else if stage == HashStage::Full {
                    2
                } else {
                    0
                }; 32],
            )
        }
        fn verify(&self, file: &FilesystemEntry) -> Result<(), HashFailure> {
            if self.changed && file.identity().file == FileId::new(3) {
                Err(HashFailure::Changed)
            } else {
                Ok(())
            }
        }
    }
    #[test]
    fn sample_collisions_never_prove_content_equality_and_changes_remove_members() {
        let entries = [file(1), file(2), file(3)];
        let reader = Reader {
            reads: Cell::new(0),
            changed: false,
        };
        let report = detect(&entries, &reader, &CancelToken::new(), &mut |_, _| {});
        assert_eq!(report.groups.len(), 1);
        assert_eq!(report.groups[0].files, [file(1), file(3)]);
        assert_eq!(report.groups[0].verdict(), Verdict::Review);
        assert_eq!(report.groups[0].logical_redundancy(), ByteCount::new(16));
        let changed = Reader {
            changed: true,
            ..reader
        };
        assert_eq!(
            detect(&entries, &changed, &CancelToken::new(), &mut |_, _| {}).groups,
            Vec::<DuplicateGroup>::new()
        );
    }
    #[test]
    fn hardlinks_and_placeholders_are_not_hashed_as_independent_copies() {
        let mut alias = file(1);
        alias.path = file(4).path;
        let mut cloud = file(2);
        cloud.size.flags.cloud_placeholder = true;
        let reader = Reader {
            reads: Cell::new(0),
            changed: false,
        };
        let report = detect(
            &[file(1), alias, cloud],
            &reader,
            &CancelToken::new(),
            &mut |_, _| {},
        );
        assert_eq!(report.groups, Vec::<DuplicateGroup>::new());
        assert_eq!(report.hard_links.len(), 1);
        assert_eq!(reader.reads.get(), 0);
        assert_eq!(report.skipped.len(), 1);
    }
    #[test]
    fn cancellation_prevents_publishing_partial_groups() {
        let reader = Reader {
            reads: Cell::new(0),
            changed: false,
        };
        let cancel = CancelToken::new();
        let report = detect(&[file(1), file(3)], &reader, &cancel, &mut |_, _| {
            cancel.cancel();
        });
        assert!(report.cancelled && report.groups.is_empty());
    }

    #[test]
    fn batches_of_one_size_give_the_report_of_one_batch() {
        let reader = Reader {
            reads: Cell::new(0),
            changed: false,
        };
        let entries = [file(1), file(2), file(3)];
        let whole = detect(&entries, &reader, &CancelToken::new(), &mut |_, _| {});
        let cancel = CancelToken::new();
        let mut detector = Detector::new(&reader, &cancel);
        let mut sizes: BTreeMap<ByteCount, Vec<FilesystemEntry>> = BTreeMap::new();
        for entry in entries {
            sizes.entry(entry.size.logical).or_default().push(entry);
        }
        for batch in sizes.values() {
            assert!(detector.add(batch, &mut |_, _| {}).is_continue());
        }
        assert_eq!(detector.finish(), whole);
    }
}
