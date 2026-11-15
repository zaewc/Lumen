//! Identical copies in the roots of a completed scan (ADR-0019, ADR-0065):
//! paths and sizes for review. Content digests never leave the host, and
//! nothing here chooses a copy to keep.

use std::path::PathBuf;

use lumen_app::duplicates::{Duplicates, DuplicatesError, HashStage};
use lumen_domain::FilesystemEntry;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Which comparison step is running.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CompareStageView {
    /// The start of each file that has a same-size twin.
    Head,
    /// The end of each file still matching.
    Tail,
    /// Whole files still matching.
    Full,
}

/// A running comparison's step and how many files it has read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CompareProgressView {
    /// The step.
    pub stage: CompareStageView,
    /// Files read so far.
    pub files_read: String,
}

impl CompareProgressView {
    /// The view of the engine's progress report.
    #[must_use]
    pub fn new(stage: HashStage, files_read: usize) -> Self {
        Self {
            stage: match stage {
                HashStage::Head => CompareStageView::Head,
                HashStage::Tail => CompareStageView::Tail,
                HashStage::Full => CompareStageView::Full,
            },
            files_read: files_read.to_string(),
        }
    }
}

/// Files with the same contents, for a person to review.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct DuplicateGroupView {
    /// The size of each copy, in bytes.
    pub bytes_each: String,
    /// Where each copy is.
    pub paths: Vec<String>,
}

/// What a comparison found and what it could not cover.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct DuplicatesView {
    /// The roots of the completed scan.
    pub folders: Vec<String>,
    /// Identical copies, the largest extra copies first. Copies can share
    /// blocks (APFS clones), so this is not space removing one would free.
    pub groups: Vec<DuplicateGroupView>,
    /// Names of one file (hard links): not copies, and removing one frees nothing.
    pub hard_links: Vec<Vec<String>>,
    /// Files not compared: changed, unreadable, cloud-only or protected by the system.
    pub skipped: String,
    /// Files left unread in protected locations.
    pub protected: String,
    /// Some folders could not be listed completely; files there were not compared.
    pub incomplete: bool,
}

/// How a comparison ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CompareOutcomeView {
    /// This platform cannot read contents to compare yet.
    Unsupported,
    /// The person stopped the comparison; no groups are shown, since a stopped
    /// comparison proves nothing about the files it did not finish.
    Cancelled,
    /// The comparison finished.
    Compared {
        /// What it found.
        report: DuplicatesView,
    },
}

fn paths(files: &[FilesystemEntry]) -> Vec<String> {
    files.iter().map(|file| file.path.display()).collect()
}

impl CompareOutcomeView {
    /// The outcome of comparing `folders`.
    ///
    /// # Errors
    ///
    /// The engine's error when a folder could not be used.
    pub fn new(
        folders: &[PathBuf],
        found: Result<Duplicates, DuplicatesError>,
    ) -> Result<Self, DuplicatesError> {
        let found = match found {
            Ok(found) => found,
            Err(DuplicatesError::Unsupported) => return Ok(Self::Unsupported),
            Err(other) => return Err(other),
        };
        let report = &found.report;
        if report.cancelled {
            return Ok(Self::Cancelled);
        }
        let mut groups: Vec<_> = report.groups.iter().collect();
        groups.sort_by_key(|group| std::cmp::Reverse(group.logical_redundancy()));
        Ok(Self::Compared {
            report: DuplicatesView {
                folders: folders
                    .iter()
                    .map(|folder| folder.display().to_string())
                    .collect(),
                groups: groups
                    .into_iter()
                    .filter_map(|group| {
                        Some(DuplicateGroupView {
                            bytes_each: group.files.first()?.size.logical.get().to_string(),
                            paths: paths(&group.files),
                        })
                    })
                    .collect(),
                hard_links: report.hard_links.iter().map(|links| paths(links)).collect(),
                skipped: report.skipped.len().to_string(),
                protected: found.protected.to_string(),
                incomplete: found.incomplete,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use lumen_app::duplicates::{DuplicateGroup, DuplicateReport};
    use lumen_domain::{
        AccessState, ByteCount, EntryKind, EntryTimes, FileId, FileIdentity, Protection, RawPath,
        SizeFacts, SizeFlags, VolumeId,
    };

    use super::*;

    fn file(id: u8, bytes: u64) -> FilesystemEntry {
        FilesystemEntry {
            path: RawPath::from_unix_bytes(format!("/chosen/{id}").into_bytes())
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
                logical: ByteCount::new(bytes),
                allocated: ByteCount::new(bytes),
                private: None,
                clone_id: None,
                link_count: NonZeroU32::MIN,
                flags: SizeFlags::default(),
            },
        }
    }

    fn group(files: Vec<FilesystemEntry>) -> DuplicateGroup {
        DuplicateGroup {
            content_id: [0; 32],
            files,
        }
    }

    #[test]
    fn groups_come_largest_first_and_never_carry_digests() {
        let folders = [PathBuf::from("/chosen")];
        let found = Duplicates {
            report: DuplicateReport {
                groups: vec![
                    group(vec![file(1, 10), file(2, 10), file(3, 10)]),
                    group(vec![file(4, 100), file(5, 100)]),
                ],
                hard_links: vec![vec![file(6, 5), file(7, 5)]],
                skipped: Vec::new(),
                cancelled: false,
            },
            protected: 2,
            incomplete: false,
        };
        let Ok(CompareOutcomeView::Compared { report }) =
            CompareOutcomeView::new(&folders, Ok(found))
        else {
            unreachable!("expected a report");
        };
        assert_eq!(report.folders, ["/chosen"]);
        assert_eq!(report.groups[0].bytes_each, "100");
        assert_eq!(
            report.groups[1].paths,
            ["/chosen/1", "/chosen/2", "/chosen/3"]
        );
        assert_eq!(report.hard_links, [["/chosen/6", "/chosen/7"]]);
        assert_eq!(report.protected, "2");
        assert!(!report.incomplete);
        let json = serde_json::to_string(&report).unwrap_or_default();
        assert!(!json.contains("content_id"));
    }

    #[test]
    fn unsupported_and_cancelled_are_outcomes_not_empty_reports() {
        let folders = [PathBuf::from("/chosen")];
        assert_eq!(
            CompareOutcomeView::new(&folders, Err(DuplicatesError::Unsupported)).ok(),
            Some(CompareOutcomeView::Unsupported)
        );
        let cancelled = Duplicates {
            report: DuplicateReport {
                cancelled: true,
                ..DuplicateReport::default()
            },
            protected: 0,
            incomplete: false,
        };
        assert_eq!(
            CompareOutcomeView::new(&folders, Ok(cancelled)).ok(),
            Some(CompareOutcomeView::Cancelled)
        );
    }
}
