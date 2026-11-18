//! Identical copies in the roots of a completed scan (ADR-0019, ADR-0065):
//! paths and sizes for review. Content digests never leave the host, and
//! nothing here chooses a copy to keep.

use std::path::PathBuf;

use lumen_app::copies::Previewed;
use lumen_app::duplicates::{
    DuplicateGroup, DuplicateReport, Duplicates, DuplicatesError, HashStage,
};
use lumen_application::copies::Ineligible;
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
    /// This comparison, as the host keeps it: a copy is named by it, its
    /// group's place in `groups` and the copy's place in `paths` (ADR-0069).
    pub comparison: String,
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

/// The groups in the order the view lists them, largest extra copies first:
/// what a group's place in [`DuplicatesView::groups`] refers to.
#[must_use]
pub fn ordered(report: &DuplicateReport) -> Vec<&DuplicateGroup> {
    let mut groups: Vec<_> = report
        .groups
        .iter()
        .filter(|group| !group.files.is_empty())
        .collect();
    groups.sort_by_key(|group| std::cmp::Reverse(group.logical_redundancy()));
    groups
}

impl CompareOutcomeView {
    /// The same outcome, naming its comparison `id` (ADR-0069).
    #[must_use]
    pub fn with_comparison(self, id: &str) -> Self {
        match self {
            Self::Compared { mut report } => {
                id.clone_into(&mut report.comparison);
                Self::Compared { report }
            }
            other => other,
        }
    }

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
        let groups = ordered(report);
        Ok(Self::Compared {
            report: DuplicatesView {
                comparison: String::new(),
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

/// A copy to preview for moving (ADR-0069): the comparison, the group's
/// place in it, the copy that moves and the copy that stays, by their places
/// in the group's `paths`. Never a path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CopyRequest {
    /// [`DuplicatesView::comparison`].
    pub comparison: String,
    /// The group's place in [`DuplicatesView::groups`].
    pub group: u32,
    /// The copy that moves.
    pub copy: u32,
    /// The copy that stays.
    pub stays: u32,
}

/// Why a copy cannot move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CopyReasonView {
    /// The two are the same file.
    SameFile,
    /// One of them is not a regular file.
    NotAFile,
    /// One of them has other names (hard links).
    HardLinked,
    /// The copy is in a protected location.
    Protected,
    /// No copy of the group would stay.
    NothingStays,
    /// Their contents differ now, or could not be read and compared.
    ContentsDiffer,
}

impl From<Ineligible> for CopyReasonView {
    fn from(reason: Ineligible) -> Self {
        match reason {
            Ineligible::SameFile => Self::SameFile,
            Ineligible::NotAFile => Self::NotAFile,
            Ineligible::HardLinked => Self::HardLinked,
            Ineligible::Protected => Self::Protected,
            Ineligible::NothingStays => Self::NothingStays,
            Ineligible::ContentsDiffer => Self::ContentsDiffer,
        }
    }
}

/// A copy's preview, or why there is none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CopyPreviewView {
    /// Ready to confirm by its full hash.
    Ready {
        /// The preview's ID.
        preview: String,
        /// Its full hash, the confirmation.
        hash: String,
        /// The copy that moves; display-escaped.
        path: String,
        /// The copy that stays; display-escaped.
        stays: String,
        /// What it frees once removed for good, in bytes.
        bytes: String,
    },
    /// The gate refused, with every reason.
    Refused {
        /// Why.
        reasons: Vec<CopyReasonView>,
    },
    /// The comparison is not the latest, or names no such copy: compare again.
    Stale,
    /// This platform cannot read contents to compare.
    Unsupported,
}

impl From<&Previewed> for CopyPreviewView {
    fn from(previewed: &Previewed) -> Self {
        match previewed {
            Previewed::Ready(preview) => Self::Ready {
                preview: preview.plan.to_string(),
                hash: preview.hash.clone(),
                path: preview.path.display(),
                stays: preview.stays.display(),
                bytes: preview.reclaim.get().to_string(),
            },
            Previewed::Refused(reasons) => Self::Refused {
                reasons: reasons.iter().copied().map(CopyReasonView::from).collect(),
            },
        }
    }
}

/// What moving a copy did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CopyResultView {
    /// It is in Quarantine, restorable.
    Quarantined {
        /// The operation, for restoring or removing it for good.
        operation: String,
    },
    /// Checked again right before the move, it may not go now; not moved.
    Refused {
        /// Every reason, at least one.
        reasons: Vec<CopyReasonView>,
    },
    /// It, the copy that stays or their contents changed; not moved.
    Changed,
    /// The move failed.
    Failed {
        /// Why.
        reason: super::FailureView,
        /// Where the copy is.
        left: super::LocationView,
    },
}

/// A moved copy's preview, executed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CopyOutcomeView {
    /// The preview.
    pub preview: String,
    /// Where the copy was; display-escaped.
    pub path: String,
    /// What removing it for good frees, in bytes.
    pub size_bytes: String,
    /// What happened.
    pub result: CopyResultView,
    /// Problems that did not stop it.
    pub warnings: Vec<String>,
}

impl From<&lumen_app::copies::CopyReport> for CopyOutcomeView {
    fn from(report: &lumen_app::copies::CopyReport) -> Self {
        use lumen_app::copies::CopyResult;
        Self {
            preview: report.plan.to_string(),
            path: report.path.display(),
            size_bytes: report.reclaim.get().to_string(),
            result: match &report.result {
                CopyResult::Quarantined(operation) => CopyResultView::Quarantined {
                    operation: operation.to_string(),
                },
                CopyResult::Refused(reasons) => CopyResultView::Refused {
                    reasons: reasons.iter().copied().map(CopyReasonView::from).collect(),
                },
                CopyResult::Changed => CopyResultView::Changed,
                CopyResult::Failed { reason, left } => CopyResultView::Failed {
                    reason: (*reason).into(),
                    left: (*left).into(),
                },
            },
            warnings: report.warnings.clone(),
        }
    }
}

/// What happened to one copy of a selection: its outcome, or why it was not
/// tried. One copy's refusal never stops the others.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SelectedCopyView {
    /// The preview.
    pub preview: String,
    /// What happened, when the move was tried.
    #[schema(required = true)]
    pub outcome: Option<CopyOutcomeView>,
    /// Why it was not tried or failed; display text.
    #[schema(required = true)]
    pub error: Option<String>,
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
