//! What the UI reads and sends (ADR-0011): view models built from
//! `lumen-app`'s results.
//!
//! Encoding rules: 64-bit counts and byte sizes are decimal strings, times are
//! RFC 3339 UTC, identifiers are strings the UI treats as opaque, and paths
//! are display strings for people to read, never handed back to act on.
//! Outcomes are tagged unions with stable `snake_case` codes, so the UI can
//! match them exhaustively. An absent value is always sent, as `null`, so
//! optional fields are required and nullable in the schema.

use lumen_app::assess::{ToolGuidance, tool_action};
use lumen_app::held::{HeldItem, Outcome};
use lumen_app::quarantine::{ItemResult, QuarantineReport};
use lumen_app::{Planned, RootSummary, ScanSummary};
use lumen_application::assess::Assessed;
use lumen_application::confirm::Skip;
use lumen_application::finalize::FinalizeOutcome;
use lumen_application::ports::{NotRestored, Removal};
use lumen_application::restore::RestoreOutcome;
use lumen_domain::{
    ByteCount, ItemKind, ItemLocation, ItemNote, JudgmentConfidence, QuarantineFailure,
    QuarantineState, Verdict,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

fn bytes(count: ByteCount) -> String {
    count.get().to_string()
}

fn reclaim(assessed: &Assessed) -> ByteCount {
    assessed
        .input
        .observed
        .as_ref()
        .map_or(ByteCount::ZERO, |o| o.reclaim.reclaimable_now)
}

/// A request to scan the given roots, read-only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ScanRequest {
    /// The folders to scan, as the user chose them. Empty means the whole
    /// startup disk (`lumen_app::roots::whole_disk`).
    pub roots: Vec<String>,
}

/// What a scan found below one root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct RootView {
    /// The root.
    pub root: String,
    /// Files below it.
    pub files: String,
    /// Their logical size in bytes.
    pub logical_bytes: String,
    /// Their allocated size in bytes.
    pub allocated_bytes: String,
    /// What removing everything below it would free now, in bytes.
    pub reclaimable_bytes: String,
    /// Whether `reclaimable_bytes` is exact rather than a lower bound.
    pub reclaimable_exact: bool,
    /// Whether everything below it could be read. If not, every size here is
    /// a lower bound.
    pub coverage_complete: bool,
    /// Entries access was denied to.
    pub denied: String,
    /// Entries that could not be read for other reasons.
    pub failed: String,
    /// Entries not scanned.
    pub not_scanned: String,
}

impl From<&RootSummary> for RootView {
    fn from(root: &RootSummary) -> Self {
        Self {
            root: root.root.display(),
            files: root.files.to_string(),
            logical_bytes: bytes(root.logical),
            allocated_bytes: bytes(root.allocated),
            reclaimable_bytes: bytes(root.reclaimable),
            reclaimable_exact: root.exact,
            coverage_complete: root.coverage.is_complete(),
            denied: root.coverage.denied.values().sum::<u64>().to_string(),
            failed: root.coverage.failed.values().sum::<u64>().to_string(),
            not_scanned: root.coverage.not_scanned.to_string(),
        }
    }
}

/// What a scan found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ScanView {
    /// The scan's ID.
    pub scan: String,
    /// Each root.
    pub roots: Vec<RootView>,
    /// Entries seen.
    pub entries: String,
    /// Whether the scan stopped early (cancelled), so results are partial.
    pub partial: bool,
    /// Whether it stopped because the disk holding the database was nearly
    /// full; then `partial` is true too.
    pub out_of_space: bool,
}

impl From<&ScanSummary> for ScanView {
    fn from(summary: &ScanSummary) -> Self {
        Self {
            scan: summary.scan.to_string(),
            roots: summary.roots.iter().map(RootView::from).collect(),
            entries: summary.entries.to_string(),
            partial: summary.stopped.is_some(),
            out_of_space: summary.out_of_space,
        }
    }
}

/// A running scan's totals so far, sent a few times a second while it runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ScanProgressView {
    /// Entries seen so far.
    pub entries: String,
    /// Folders listed so far.
    pub directories: String,
    /// Disk space the files seen so far take up, in bytes.
    pub allocated_bytes: String,
    /// Folders that could not be read so far.
    pub unreadable: String,
    /// The folder listed most recently, if any.
    #[schema(required = true)]
    pub current: Option<String>,
    /// Milliseconds since the scan started.
    pub elapsed_ms: String,
}

/// The policy's verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum VerdictView {
    /// Leave it.
    Keep,
    /// A person should look.
    Review,
    /// Proposed for reversible quarantine.
    Quarantine,
}

impl From<Verdict> for VerdictView {
    fn from(verdict: Verdict) -> Self {
        match verdict {
            Verdict::Keep => Self::Keep,
            Verdict::Review => Self::Review,
            Verdict::Quarantine => Self::Quarantine,
        }
    }
}

/// One assessed candidate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CandidateView {
    /// Where it is.
    pub path: String,
    /// The policy's verdict.
    pub verdict: VerdictView,
    /// What removing it would free now, in bytes.
    pub reclaimable_bytes: String,
    /// Why: one reason per policy rule that fired, in the policy's order.
    pub reasons: Vec<ReasonView>,
    /// The cleanup command of the developer tool that owns it, if the tool
    /// has one: guidance for the user to run themselves. Lumen never runs it.
    #[schema(required = true)]
    pub tool_command: Option<ToolCommandView>,
    /// The group it is presented in.
    pub group: GroupView,
    /// What AI review says a review item most likely is (ADR-0041): a note
    /// for people, labelled as AI, that never changes the verdict. `null`
    /// unless the item is under review and AI review described it.
    #[schema(required = true)]
    pub ai_note: Option<AiNoteView>,
}

/// What kind of item AI review says a review item most likely is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ItemKindView {
    /// An app's settings or data.
    AppSettings,
    /// The user's documents.
    UserDocuments,
    /// A cache.
    Cache,
    /// Logs or diagnostic reports.
    Logs,
    /// Downloads or installers.
    DownloadsOrInstallers,
    /// A developer tool's data.
    DeveloperToolData,
    /// A backup or archive.
    BackupOrArchive,
    /// AI review could not tell.
    Unknown,
}

/// How sure AI review was.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConfidenceView {
    /// Low.
    Low,
    /// Medium.
    Medium,
    /// High.
    High,
}

/// An AI note on a review item: only a closed kind and a confidence, never
/// model text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct AiNoteView {
    /// What the item most likely is.
    pub kind: ItemKindView,
    /// How sure AI review was.
    pub confidence: ConfidenceView,
}

impl From<ItemNote> for AiNoteView {
    fn from(note: ItemNote) -> Self {
        Self {
            kind: match note.kind {
                ItemKind::AppSettings => ItemKindView::AppSettings,
                ItemKind::UserDocuments => ItemKindView::UserDocuments,
                ItemKind::Cache => ItemKindView::Cache,
                ItemKind::Logs => ItemKindView::Logs,
                ItemKind::DownloadsOrInstallers => ItemKindView::DownloadsOrInstallers,
                ItemKind::DeveloperToolData => ItemKindView::DeveloperToolData,
                ItemKind::BackupOrArchive => ItemKindView::BackupOrArchive,
                ItemKind::Unknown => ItemKindView::Unknown,
            },
            confidence: match note.confidence {
                JudgmentConfidence::Low => ConfidenceView::Low,
                JudgmentConfidence::Medium => ConfidenceView::Medium,
                JudgmentConfidence::High => ConfidenceView::High,
            },
        }
    }
}

/// The kind of group an item is presented in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum GroupKindView {
    /// A developer tool's cache or build output.
    Developer,
    /// An application's cache.
    AppCache,
    /// An application's other data.
    AppData,
    /// Anything else.
    Other,
}

/// The group an item is presented in, and what it belongs to when known.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct GroupView {
    /// The kind of group.
    pub kind: GroupKindView,
    /// The developer tool (e.g. `Xcode`) or the owning application's
    /// identifier, as untrusted display text; `null` when unknown.
    #[schema(required = true)]
    pub source: Option<String>,
}

impl From<lumen_app::assess::Group> for GroupView {
    fn from(group: lumen_app::assess::Group) -> Self {
        use lumen_app::assess::GroupKind;
        Self {
            kind: match group.kind {
                GroupKind::Developer => GroupKindView::Developer,
                GroupKind::AppCache => GroupKindView::AppCache,
                GroupKind::AppData => GroupKindView::AppData,
                GroupKind::Other => GroupKindView::Other,
            },
            source: group.source,
        }
    }
}

/// The app's version and whether a newer one is published (ADR-0048).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct UpdateView {
    /// The running version, such as `0.4.0`.
    pub current: String,
    /// The newer version on offer, if any.
    pub available: Option<String>,
    /// Whether the app can install it itself; `false` where it cannot
    /// replace its own files (a `.deb` install), which updates through the
    /// download page instead.
    pub installable: bool,
}

/// How full the disk holding the user's home folder is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct DiskView {
    /// The volume's size, in bytes.
    pub total_bytes: String,
    /// The bytes the user may still write, in bytes.
    pub available_bytes: String,
}

impl From<lumen_app::space::Volume> for DiskView {
    fn from(volume: lumen_app::space::Volume) -> Self {
        Self {
            total_bytes: volume.total.to_string(),
            available_bytes: volume.available.to_string(),
        }
    }
}

/// A developer tool's own cleanup command, as a literal argument vector:
/// shown to the user to copy and run themselves, never run by Lumen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ToolCommandView {
    /// The tool's display name, such as `Xcode`.
    pub tool: String,
    /// The program, such as `xcrun`.
    pub program: String,
    /// Its arguments, one per element, unquoted.
    pub args: Vec<String>,
}

impl From<ToolGuidance> for ToolCommandView {
    fn from(guidance: ToolGuidance) -> Self {
        Self {
            tool: guidance.tool,
            program: guidance.program,
            args: guidance.args,
        }
    }
}

/// One policy rule that fired, and its fixed sentence (ADR-0014).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ReasonView {
    /// The rule's stable identifier, such as `classify.devkb-regenerable`, so
    /// an interface can show the reason in the user's language.
    pub rule: String,
    /// The reason in English.
    pub text: String,
}

impl From<&Assessed> for CandidateView {
    fn from(assessed: &Assessed) -> Self {
        // `explain` renders one sentence per fired rule, in the same order.
        let reasons = assessed
            .decision
            .fired_rules()
            .iter()
            .zip(&assessed.reasons)
            .map(|(fired, text)| ReasonView {
                rule: fired.rule.as_str().to_owned(),
                text: text.clone(),
            })
            .collect();
        Self {
            path: assessed.candidate.path.display(),
            verdict: assessed.decision.verdict().into(),
            reclaimable_bytes: bytes(reclaim(assessed)),
            reasons,
            tool_command: tool_action(assessed).map(ToolCommandView::from),
            group: lumen_app::assess::group(assessed).into(),
            ai_note: assessed
                .note
                .filter(|note| note.shown() && assessed.decision.verdict() == Verdict::Review)
                .map(AiNoteView::from),
        }
    }
}

/// A cleanup plan, waiting for confirmation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct PlanView {
    /// The plan's ID.
    pub plan: String,
    /// Its full hash: the confirmation the user gives to execute it.
    pub hash: String,
    /// When it was made; it expires 24 hours later.
    #[schema(value_type = String, format = DateTime)]
    pub created_at: lumen_domain::Timestamp,
    /// Its items, each with what it would free and why.
    pub items: Vec<CandidateView>,
}

impl From<&Planned> for PlanView {
    fn from(planned: &Planned) -> Self {
        Self {
            plan: planned.plan.id().to_string(),
            hash: planned.plan.hash().to_string(),
            created_at: planned.plan.created_at(),
            items: planned.items.iter().map(CandidateView::from).collect(),
        }
    }
}

/// A request to execute a plan: its ID and, as the confirmation, its full
/// hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ExecuteRequest {
    /// The plan's ID.
    pub plan: String,
    /// The plan's full hash, as shown.
    pub confirmation: String,
}

/// Where an item is after a failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LocationView {
    /// Untouched at its original place.
    Source,
    /// In quarantine, restorable.
    Quarantine,
    /// Gone: removed for good.
    Gone,
}

impl From<ItemLocation> for LocationView {
    fn from(location: ItemLocation) -> Self {
        match location {
            ItemLocation::Source => Self::Source,
            ItemLocation::Quarantine => Self::Quarantine,
            ItemLocation::Gone => Self::Gone,
        }
    }
}

/// Why moving an item failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum FailureView {
    /// It changed or was swapped since it was planned.
    IdentityMismatch,
    /// A program was using it.
    InUse,
    /// Its drive cannot hold a quarantine store (another or removable drive,
    /// a network share, or a filesystem without a safe rename; ADR-0046).
    VolumeUnsupported,
    /// The move itself failed.
    MoveFailed,
    /// After the move, it did not match the plan.
    VerificationFailed,
    /// A reason this version does not know.
    Other,
}

impl From<QuarantineFailure> for FailureView {
    fn from(failure: QuarantineFailure) -> Self {
        match failure {
            QuarantineFailure::IdentityMismatch => Self::IdentityMismatch,
            QuarantineFailure::InUse => Self::InUse,
            QuarantineFailure::VolumeUnsupported => Self::VolumeUnsupported,
            QuarantineFailure::MoveFailed => Self::MoveFailed,
            QuarantineFailure::VerificationFailed => Self::VerificationFailed,
            _ => Self::Other,
        }
    }
}

/// Why a planned item was not moved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SkipView {
    /// It changed or cannot be observed.
    Changed,
    /// It is outside the scanned roots.
    OutsideRoots,
    /// The policy now keeps it.
    NowKeep,
    /// The policy now wants a person to review it.
    NowReview,
    /// It would now be proposed for different reasons.
    ReasonsChanged,
    /// It could not be recorded for a safe move.
    NotPrepared,
    /// Its manifest could not be written.
    Manifest,
}

impl From<Skip> for SkipView {
    fn from(skip: Skip) -> Self {
        match skip {
            Skip::Changed => Self::Changed,
            Skip::OutsideRoots => Self::OutsideRoots,
            Skip::NoLongerProposed(Verdict::Keep) => Self::NowKeep,
            // `Quarantine` cannot be "no longer proposed"; treat it as review.
            Skip::NoLongerProposed(Verdict::Review | Verdict::Quarantine) => Self::NowReview,
            Skip::ReasonsChanged => Self::ReasonsChanged,
            Skip::NotPrepared => Self::NotPrepared,
            Skip::Manifest => Self::Manifest,
        }
    }
}

/// What became of one planned item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ItemResultView {
    /// Not moved; it is `REVIEW` at most now.
    Skipped {
        /// Why.
        reason: SkipView,
    },
    /// Moved into quarantine, restorable.
    Quarantined {
        /// The operation, for restoring or finalizing it.
        operation: String,
    },
    /// The move failed.
    Failed {
        /// Why.
        reason: FailureView,
        /// Where the item is.
        left: LocationView,
    },
}

/// One planned item and what became of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ItemReportView {
    /// Where it was.
    pub path: String,
    /// What it would free, as planned, in bytes.
    pub reclaimable_bytes: String,
    /// What happened.
    pub result: ItemResultView,
}

/// What executing a plan did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct QuarantineReportView {
    /// The plan.
    pub plan: String,
    /// Interrupted operations from earlier runs that were resolved first.
    pub resolved: String,
    /// Each item, in plan order.
    pub items: Vec<ItemReportView>,
    /// What the quarantined items will free once finalized, in bytes.
    pub moved_bytes: String,
    /// Problems that did not stop the execution.
    pub warnings: Vec<String>,
}

impl From<&QuarantineReport> for QuarantineReportView {
    fn from(report: &QuarantineReport) -> Self {
        Self {
            plan: report.plan.to_string(),
            resolved: report.resolved.to_string(),
            items: report
                .items
                .iter()
                .map(|item| ItemReportView {
                    path: item.path.display(),
                    reclaimable_bytes: bytes(item.reclaim),
                    result: match item.result {
                        ItemResult::Skipped(reason) => ItemResultView::Skipped {
                            reason: reason.into(),
                        },
                        ItemResult::Quarantined(operation) => ItemResultView::Quarantined {
                            operation: operation.to_string(),
                        },
                        ItemResult::Failed { reason, left } => ItemResultView::Failed {
                            reason: reason.into(),
                            left: left.into(),
                        },
                    },
                })
                .collect(),
            moved_bytes: bytes(report.moved),
            warnings: report.warnings.clone(),
        }
    }
}

/// Why an item is held.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum HeldStateView {
    /// Quarantined and kept for its retention period.
    Retained,
    /// Left in quarantine by a failed or rolled-back operation.
    HeldAfterFailure,
}

/// One item in quarantine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct HeldItemView {
    /// The operation that moved it.
    pub operation: String,
    /// Why it is held.
    pub state: HeldStateView,
    /// Where it was, if known.
    #[schema(required = true)]
    pub original: Option<String>,
    /// When it was quarantined.
    #[schema(value_type = String, format = DateTime)]
    pub since: lumen_domain::Timestamp,
    /// What removing it for good would free, in bytes, as the confirmed plan
    /// estimated; `null` for items quarantined before sizes were recorded.
    #[schema(required = true)]
    pub reclaim_bytes: Option<String>,
}

impl From<&HeldItem> for HeldItemView {
    fn from(item: &HeldItem) -> Self {
        Self {
            operation: item.operation.to_string(),
            state: if item.state == QuarantineState::Retained {
                HeldStateView::Retained
            } else {
                HeldStateView::HeldAfterFailure
            },
            original: item.original.as_ref().map(|p| p.display().to_string()),
            since: item.since,
            reclaim_bytes: item.reclaim.map(bytes),
        }
    }
}

/// Why a restore did not happen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum NotRestoredView {
    /// Something now has the item's name; the user decides.
    NameTaken,
    /// The original folder cannot be reached safely.
    OriginUnreachable,
    /// The quarantined item changed.
    ItemChanged,
    /// The move back failed.
    Failed,
}

/// What a restore did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RestoreView {
    /// Back at its original place.
    Restored {
        /// Its folder had been recreated since.
        parent_replaced: bool,
        /// It matched its record after the move.
        verified: bool,
    },
    /// Still in quarantine.
    NotRestored {
        /// Why.
        reason: NotRestoredView,
    },
    /// Not in a state that can be restored; nothing was done.
    NotRestorable,
}

/// How much a failed removal took.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RemovedView {
    /// Nothing; the item is intact.
    Nothing,
    /// Part of it; finalizing again finishes it.
    Partly,
    /// All of it; only its record remains.
    Item,
}

/// What a finalization did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FinalizeView {
    /// Removed for good.
    Finalized,
    /// Back at its original place already; nothing removed.
    FoundAtSource,
    /// Not in the store; nothing removed.
    NotFound,
    /// The removal stopped part way.
    Failed {
        /// How much is gone.
        removed: RemovedView,
    },
    /// It may be in use, or that cannot be checked; nothing removed.
    MaybeInUse,
    /// Not retained; nothing done.
    NotFinalizable,
}

/// What a restore did to one held item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct RestoreOutcomeView {
    /// The operation.
    pub operation: String,
    /// Where the item was, if known.
    #[schema(required = true)]
    pub original: Option<String>,
    /// What happened.
    pub outcome: RestoreView,
}

/// What a finalization did to one held item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct FinalizeOutcomeView {
    /// The operation.
    pub operation: String,
    /// Where the item was, if known.
    #[schema(required = true)]
    pub original: Option<String>,
    /// What happened.
    pub outcome: FinalizeView,
}

fn original<T>(outcome: &Outcome<T>) -> Option<String> {
    outcome.original.as_ref().map(|p| p.display().to_string())
}

impl From<&Outcome<RestoreOutcome>> for RestoreOutcomeView {
    fn from(outcome: &Outcome<RestoreOutcome>) -> Self {
        let view = match outcome.outcome {
            RestoreOutcome::Restored {
                parent_replaced,
                verified,
            } => RestoreView::Restored {
                parent_replaced,
                verified,
            },
            RestoreOutcome::NotRestored(reason) => RestoreView::NotRestored {
                reason: match reason {
                    NotRestored::NameTaken => NotRestoredView::NameTaken,
                    NotRestored::OriginUnreachable => NotRestoredView::OriginUnreachable,
                    NotRestored::ItemChanged => NotRestoredView::ItemChanged,
                    NotRestored::Failed => NotRestoredView::Failed,
                },
            },
            RestoreOutcome::NotRestorable(_) => RestoreView::NotRestorable,
        };
        Self {
            operation: outcome.operation.to_string(),
            original: original(outcome),
            outcome: view,
        }
    }
}

impl From<&Outcome<FinalizeOutcome>> for FinalizeOutcomeView {
    fn from(outcome: &Outcome<FinalizeOutcome>) -> Self {
        let view = match outcome.outcome {
            FinalizeOutcome::Finalized => FinalizeView::Finalized,
            FinalizeOutcome::FoundAtSource => FinalizeView::FoundAtSource,
            FinalizeOutcome::NotFound => FinalizeView::NotFound,
            FinalizeOutcome::Failed { removed } => FinalizeView::Failed {
                removed: match removed {
                    Removal::Nothing => RemovedView::Nothing,
                    Removal::Partly => RemovedView::Partly,
                    Removal::Item => RemovedView::Item,
                },
            },
            FinalizeOutcome::MaybeInUse => FinalizeView::MaybeInUse,
            FinalizeOutcome::NotFinalizable(_) => FinalizeView::NotFinalizable,
        };
        Self {
            operation: outcome.operation.to_string(),
            original: original(outcome),
            outcome: view,
        }
    }
}

/// Which service the judge's key is for; never the key itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub enum AiKeyView {
    /// A Jeb key: requests go to the school's AI Hub (ADR-0036).
    #[serde(rename = "jeb")]
    Jeb,
    /// An `OpenRouter` key: requests go to `TypeSafe`'s Jev (ADR-0031).
    #[serde(rename = "openrouter")]
    OpenRouter,
    /// No key: AI review cannot be turned on.
    #[serde(rename = "none")]
    None,
}

impl From<Option<lumen_app::ai::Service>> for AiKeyView {
    fn from(service: Option<lumen_app::ai::Service>) -> Self {
        match service {
            Some(lumen_app::ai::Service::Jeb) => Self::Jeb,
            Some(lumen_app::ai::Service::OpenRouter) => Self::OpenRouter,
            None => Self::None,
        }
    }
}

/// Where the judge's key comes from (ADR-0042); never the key itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub enum AiKeySourceView {
    /// Lumen's built-in Jeb key, in release builds, used because the user
    /// stored none.
    #[serde(rename = "builtin")]
    Builtin,
    /// The user's own key, from the system's secret store.
    #[serde(rename = "stored")]
    Stored,
}

impl From<lumen_app::ai::KeySource> for AiKeySourceView {
    fn from(source: lumen_app::ai::KeySource) -> Self {
        match source {
            lumen_app::ai::KeySource::Builtin => Self::Builtin,
            lumen_app::ai::KeySource::Stored => Self::Stored,
        }
    }
}

/// Whether AI review is on (ADR-0037), and which key it would use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct AiSettingsView {
    /// Whether assessments ask the judge about proposed quarantines.
    pub enabled: bool,
    /// When the user last turned it on; `null` if never.
    #[schema(value_type = Option<String>, format = DateTime, required = true)]
    pub consented_at: Option<lumen_domain::Timestamp>,
    /// Which service the key it would use is for; `none` without a key.
    pub key: AiKeyView,
    /// Where that key comes from; `null` without a key.
    #[schema(required = true)]
    pub key_source: Option<AiKeySourceView>,
}

impl AiSettingsView {
    /// The view of `review` with `key`: the service and source of the key it
    /// would use, if any.
    pub fn new(
        review: lumen_app::ai::AiReview,
        key: Option<(lumen_app::ai::Service, lumen_app::ai::KeySource)>,
    ) -> Self {
        Self {
            enabled: review.enabled,
            consented_at: review.consented_at,
            key: key.map(|(service, _)| service).into(),
            key_source: key.map(|(_, source)| source.into()),
        }
    }
}
