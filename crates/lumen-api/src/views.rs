//! What the UI reads and sends (ADR-0011): view models built from
//! `lumen-app`'s results.
//!
//! Encoding rules: 64-bit counts and byte sizes are decimal strings, times are
//! RFC 3339 UTC, identifiers are strings the UI treats as opaque, and paths
//! are display strings for people to read, never handed back to act on.
//! Outcomes are tagged unions with stable `snake_case` codes, so the UI can
//! match them exhaustively. An absent value is always sent, as `null`, so
//! optional fields are required and nullable in the schema.

use lumen_app::assess::{AssessProgress, AssessStage, ToolGuidance, tool_action};
use lumen_app::held::{HeldItem, Outcome};
use lumen_app::quarantine::{ItemResult, QuarantineReport};
use lumen_app::{Planned, RootSummary, ScanSummary};
use lumen_application::assess::Assessed;
use lumen_application::confirm::Skip;
use lumen_application::finalize::FinalizeOutcome;
use lumen_application::ports::{NotRestored, Removal};
use lumen_application::restore::RestoreOutcome;
use lumen_domain::{
    ByteCount, Chooser, ItemKind, ItemLocation, ItemNote, JudgmentConfidence, QuarantineFailure,
    QuarantineState, Verdict,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

mod duplicates;
mod facts;
#[cfg(test)]
mod fixture;
mod helper;
mod reassess;
mod relations;
mod software;
mod times;
mod uninstall;

pub use duplicates::{
    CompareOutcomeView, CompareProgressView, CompareStageView, DuplicateGroupView, DuplicatesView,
};
pub use facts::{AppDataKindView, BasisView, FactKindView, FactView};
pub use helper::{HelperReachView, HelperRegistrationView, HelperView};
pub use reassess::{
    ReassessGapView, ReassessOutcomeView, ReassessRequest, ReassessView, ReassessedItemView,
    candidate_id,
};
pub use relations::{MAX_RELATIONS, NodeView, RelationKindView, RelationView, RelationsView};
pub use software::{
    LinkedServiceView, RemovalImpactView, ServiceKindView, ServiceLinkMethodView, ServiceScopeView,
    SoftwareItemView, SoftwareNoteView, SoftwareOriginView, SoftwarePurposeView,
    SoftwareSourceStateView, SoftwareSourceView, SoftwareView, software_key, software_subjects,
};
pub use times::{TimeView, TimesView};
pub use uninstall::{
    QuitView, UninstallAppView, UninstallNoteView, UninstallOutcomeView, UninstallPreviewView,
    UninstallReasonView, UninstallResultView,
};

fn bytes(count: ByteCount) -> String {
    count.get().to_string()
}

/// Current assessment work. Each stage has its own units and total.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AssessStageView {
    /// Reading platform inventories.
    Inventory,
    /// Looking for project markers in the index.
    Markers,
    /// Matching evidence to indexed entries.
    Evidence,
    /// Recording evidence and loading relationships.
    Graph,
    /// Applying the policy and optional judge to candidates.
    Deciding,
    /// Describing selected review items.
    Describing,
}

/// Advisory progress, without paths or contents. Never an action input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AssessProgressView {
    /// Work currently running; percentages are local to this stage.
    pub stage: AssessStageView,
    /// Units processed, as a decimal string.
    pub completed: String,
    /// Total units for this stage; null when unknown.
    #[schema(required = true)]
    pub total: Option<String>,
}

impl From<AssessProgress> for AssessProgressView {
    fn from(progress: AssessProgress) -> Self {
        Self {
            stage: match progress.stage {
                AssessStage::Inventory => AssessStageView::Inventory,
                AssessStage::Markers => AssessStageView::Markers,
                AssessStage::Evidence => AssessStageView::Evidence,
                AssessStage::Graph => AssessStageView::Graph,
                AssessStage::Deciding => AssessStageView::Deciding,
                AssessStage::Describing => AssessStageView::Describing,
            },
            completed: progress.completed.to_string(),
            total: progress.total.map(|total| total.to_string()),
        }
    }
}

fn reclaim(assessed: &Assessed) -> ByteCount {
    assessed
        .input
        .observed
        .as_ref()
        .map_or(ByteCount::ZERO, |o| o.reclaim.reclaimable_now)
}

/// Whether [`reclaim`] is exact; without an observation it is not known.
fn reclaim_exact(assessed: &Assessed) -> bool {
    assessed
        .input
        .observed
        .as_ref()
        .is_some_and(|o| o.reclaim.exact)
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
    /// Its opaque ID, from its file identity, never its path (ADR-0053):
    /// stable within an assessment and across scans while the platform
    /// reports the same volume and file IDs, and shared by hard links to it.
    /// For matching relations; rows stay keyed by `path`.
    pub id: String,
    /// Where it is.
    pub path: String,
    /// The policy's verdict.
    pub verdict: VerdictView,
    /// What removing it would free now, in bytes.
    pub reclaimable_bytes: String,
    /// Whether `reclaimable_bytes` is exact rather than a lower bound (a
    /// listing could not rule out shared blocks, ADR-0017).
    pub reclaimable_exact: bool,
    /// Why: one reason per policy rule that fired, in the policy's order.
    pub reasons: Vec<ReasonView>,
    /// Versioned descriptive review categories, never permission to change a
    /// verdict. Optional on input for compatibility with older hosts.
    #[serde(default)]
    pub review_reasons: Option<ReviewReasonsView>,
    /// What Lumen did not know when it decided, which "Check again" would ask
    /// (ADR-0058); empty when the verdict rests on decisions only. Optional
    /// on input for compatibility with older hosts.
    #[serde(default)]
    pub rechecks: Option<Vec<ReassessGapView>>,
    /// The cleanup command of the developer tool that owns it, if the tool
    /// has one: displayed in full and run by entry ID after desktop confirmation.
    #[schema(required = true)]
    pub tool_command: Option<ToolCommandView>,
    /// The group it is presented in.
    pub group: GroupView,
    /// What AI review says a review item most likely is (ADR-0041): a note
    /// for people, labelled as AI, that never changes the verdict. `null`
    /// unless the item is under review and AI review described it.
    #[schema(required = true)]
    pub ai_note: Option<AiNoteView>,
    /// For an item kept only because running apps use it: the policy would
    /// propose it once they are closed and it is scanned again. Display only:
    /// the verdict stays `keep` and nothing plans from it. `null` otherwise.
    #[schema(required = true)]
    pub free_when_closed: Option<FreeWhenClosedView>,
    /// Its modification and access times as the decision saw them, each
    /// saying how far it can be relied on (ADR-0053).
    pub times: TimesView,
    /// The facts the decision saw about it, and whether each application
    /// that owns it is installed (ADR-0053). For in use, owned by the
    /// current user and owner installed, a missing fact means the question
    /// could not be answered: unknown, never "no".
    pub facts: Vec<FactView>,
    /// The relationships in its decision's evidence bundle, nearest first
    /// and bounded (ADR-0053).
    pub relations: RelationsView,
}

/// The apps to close so that a kept item could be proposed on the next scan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct FreeWhenClosedView {
    /// What the probe reported using it, as escaped display names (such as
    /// `Spotify`), sorted; empty when the platform did not name them.
    pub processes: Vec<String>,
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
    #[schema(required = true)]
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
/// shown in full, with desktop execution by entry ID after confirmation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ToolCommandView {
    /// The knowledge-base entry it comes from: the name to run it by
    /// (ADR-0049); the page never sends the command itself.
    pub entry: String,
    /// The tool's display name, such as `Xcode`.
    pub tool: String,
    /// The program, such as `xcrun`.
    pub program: String,
    /// Its arguments, one per element, unquoted.
    pub args: Vec<String>,
    /// It removes data, not just caches (Docker), so the confirmation says
    /// what it removes.
    pub removes_data: bool,
}

/// Observed free space on one volume, not a causal cleanup guarantee.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SpaceChangeView {
    /// Opaque volume identity; never a path to act on.
    pub volume: String,
    /// Available bytes before the operation.
    #[schema(pattern = r"^[0-9]+$", max_length = 20)]
    pub before_available_bytes: String,
    /// Available bytes immediately afterwards; may be smaller.
    #[schema(pattern = r"^[0-9]+$", max_length = 20)]
    pub after_available_bytes: String,
}

impl From<&lumen_app::space_change::SpaceChange> for SpaceChangeView {
    fn from(change: &lumen_app::space_change::SpaceChange) -> Self {
        Self {
            volume: change.volume.as_str().to_owned(),
            before_available_bytes: change.before_available.to_string(),
            after_available_bytes: change.after_available.to_string(),
        }
    }
}

/// How running a tool's command ended (ADR-0049).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ToolRunView {
    /// It exited with status 0.
    pub success: bool,
    /// Its exit code, if it exited on its own.
    #[schema(required = true)]
    pub exit_code: Option<i32>,
    /// It ran past the time limit and was stopped.
    pub timed_out: bool,
    /// The end of what it printed.
    pub output: String,
    /// Every matched volume measured once, or unknown measurement coverage.
    #[schema(required = true)]
    pub space_changes: Option<Vec<SpaceChangeView>>,
}

impl From<lumen_app::tools::ToolRun> for ToolRunView {
    fn from(run: lumen_app::tools::ToolRun) -> Self {
        Self {
            success: run.success,
            exit_code: run.exit_code,
            timed_out: run.timed_out,
            output: run.output,
            space_changes: run
                .space_changes
                .as_ref()
                .map(|changes| changes.iter().map(SpaceChangeView::from).collect()),
        }
    }
}

impl From<ToolGuidance> for ToolCommandView {
    fn from(guidance: ToolGuidance) -> Self {
        Self {
            entry: guidance.entry,
            removes_data: guidance.removes_data,
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

/// Machine-readable uncertainty; unknown future codes remain displayable through
/// the decision's localized fired-rule explanations and supporting facts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ReviewReasonsView {
    /// `review-reasons/1` for this contract.
    pub schema: String,
    /// Stable `snake_case` categories. Strings preserve forward compatibility.
    pub codes: Vec<String>,
}

fn review_reasons(assessed: &Assessed) -> Option<ReviewReasonsView> {
    let reasons = lumen_application::review::reasons(assessed);
    if reasons.is_empty() {
        return None;
    }
    Some(ReviewReasonsView {
        schema: lumen_domain::REVIEW_REASONS_SCHEMA.to_owned(),
        codes: reasons
            .into_iter()
            .map(|reason| reason.as_str().to_owned())
            .collect(),
    })
}

#[cfg(test)]
mod review_tests {
    use super::*;
    use lumen_domain::{AccessState, EntryKind, ReclaimEstimate};

    #[test]
    fn review_categories_preserve_the_decision_and_expose_unknowns()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut assessed = fixture::assessed(EntryKind::Directory, AccessState::Readable, vec![]);
        let before = assessed.decision.clone();
        let view = CandidateView::from(&assessed);
        let reasons = view.review_reasons.ok_or("missing review reasons")?;
        assert_eq!(reasons.schema, "review-reasons/1");
        assert_eq!(
            reasons.codes,
            ["unknown_owner", "uncertain_reclaimable_size"]
        );
        assessed.input.observed = Some(lumen_policy::Observed {
            entry: assessed.candidate.clone(),
            complete: true,
            reclaim: ReclaimEstimate {
                partially_linked_files: 1,
                exact: false,
                ..ReclaimEstimate::default()
            },
        });
        assert!(
            review_reasons(&assessed)
                .ok_or("missing review reasons")?
                .codes
                .contains(&"shared_resource".to_owned())
        );
        assert_eq!(
            assessed.decision, before,
            "describing never reevaluates policy"
        );
        assessed.decision = lumen_domain::PolicyDecision::new(
            Verdict::Keep,
            before.policy_version(),
            before.evidence_hash(),
            before.fired_rules().iter().cloned(),
            before.risk().clone(),
            before.jev_effect(),
        )?;
        assert_eq!(review_reasons(&assessed), None);
        Ok(())
    }

    #[test]
    fn future_codes_round_trip_for_existing_ui_rule_fallback() -> Result<(), serde_json::Error> {
        let json = serde_json::json!({ "schema": "review-reasons/2", "codes": ["future_reason"] });
        let view: ReviewReasonsView = serde_json::from_value(json.clone())?;
        assert_eq!(serde_json::to_value(view)?, json);
        Ok(())
    }
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
            id: relations::file_id(assessed.candidate.identity()),
            path: assessed.candidate.path.display(),
            verdict: assessed.decision.verdict().into(),
            reclaimable_bytes: bytes(reclaim(assessed)),
            reclaimable_exact: reclaim_exact(assessed),
            reasons,
            review_reasons: review_reasons(assessed),
            rechecks: Some(
                lumen_application::reassess::gaps(assessed)
                    .into_iter()
                    .map(ReassessGapView::from)
                    .collect(),
            ),
            tool_command: tool_action(assessed).map(ToolCommandView::from),
            group: lumen_app::assess::group(assessed).into(),
            ai_note: assessed
                .note
                .filter(|note| note.shown() && assessed.decision.verdict() == Verdict::Review)
                .map(AiNoteView::from),
            free_when_closed: assessed
                .free_when_closed
                .as_ref()
                .filter(|_| assessed.decision.verdict() == Verdict::Keep)
                .map(|free| FreeWhenClosedView {
                    processes: free.processes.clone(),
                }),
            times: times::times(assessed),
            facts: facts::facts(assessed),
            relations: relations::relations(assessed),
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
    /// A cleanup was interrupted and the item is not where its record says;
    /// Lumen left it as it is. It needs the person: restoring or removing it
    /// here is refused.
    NeedsAttention,
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
    /// Who chose it (ADR-0051); `null` if that was not recorded. Only an item
    /// the policy proposed is removed for good when its retention ends; the
    /// others stay until the person restores or removes them.
    #[schema(required = true)]
    pub chosen_by: Option<ChooserView>,
}

/// Who put an item into a plan (ADR-0051).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChooserView {
    /// The policy proposed it.
    Policy,
    /// The person chose it from the items left for review.
    Person,
}

impl From<Chooser> for ChooserView {
    fn from(chooser: Chooser) -> Self {
        match chooser {
            Chooser::Policy => Self::Policy,
            Chooser::Person => Self::Person,
        }
    }
}

impl From<&HeldItem> for HeldItemView {
    fn from(item: &HeldItem) -> Self {
        Self {
            operation: item.operation.to_string(),
            state: match item.state {
                QuarantineState::Retained => HeldStateView::Retained,
                QuarantineState::Failed { .. } => HeldStateView::HeldAfterFailure,
                _ => HeldStateView::NeedsAttention,
            },
            original: item.original.as_ref().map(|p| p.display().to_string()),
            since: item.since,
            reclaim_bytes: item.reclaim.map(bytes),
            chosen_by: item.chosen_by.map(ChooserView::from),
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
        /// It matched its record in full after the move; false if it changed
        /// while in quarantine (a folder that gained or lost entries) or the
        /// check after the move failed.
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
    /// Observed volume change; unavailable readings are never zero.
    #[schema(required = true)]
    pub space_change: Option<SpaceChangeView>,
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
            space_change: outcome.space_change.as_ref().map(SpaceChangeView::from),
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

/// Whether AI review is on (ADR-0037), whether the user has chosen yet
/// (ADR-0050), and which key it would use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct AiSettingsView {
    /// Whether the user has chosen on or off. Until they have, AI review is
    /// off and the app asks once (ADR-0050).
    pub chosen: bool,
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
    /// The view of the user's `choice` (`None` while they have not chosen)
    /// with `key`: the service and source of the key it would use, if any.
    pub fn new(
        choice: Option<lumen_app::ai::AiReview>,
        key: Option<(lumen_app::ai::Service, lumen_app::ai::KeySource)>,
    ) -> Self {
        let review = choice.unwrap_or_default();
        Self {
            chosen: choice.is_some(),
            enabled: review.enabled,
            consented_at: review.consented_at,
            key: key.map(|(service, _)| service).into(),
            key_source: key.map(|(_, source)| source.into()),
        }
    }
}

/// A developer tool whose own storage report the desktop shows (#705, #706, #712).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ToolKindView {
    /// Xcode's simulator runtimes, through `simctl`.
    Xcode,
    /// Docker's images, containers, volumes and build cache.
    Docker,
    /// Homebrew's download cache.
    Homebrew,
    /// npm's package cache.
    Npm,
    /// pnpm's shared package store.
    Pnpm,
}

/// Whether the tool answered. Anything but `found` means unknown, never empty.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ToolStateView {
    /// The tool answered completely.
    Found,
    /// The program is not installed where the user's terminal would find it.
    NotInstalled,
    /// This platform has no such tool or adapter.
    Unsupported,
    /// The tool could not answer completely.
    Unavailable {
        /// Why, in the core's words, for display.
        reason: String,
    },
}

/// One thing a tool reports, in the tool's own words and units.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ToolStorageItemView {
    /// What it is: a runtime, a Docker category, or a location.
    pub name: String,
    /// The tool's state or counts for it, if any.
    #[schema(required = true)]
    pub detail: Option<String>,
    /// An exact size in bytes, when the tool gives one.
    #[schema(required = true)]
    pub bytes: Option<String>,
    /// A size as the tool wrote it (Docker), when it gives no byte count.
    #[schema(required = true)]
    pub reported_size: Option<String>,
    /// What the tool itself estimates it could free; not a Lumen promise.
    #[schema(required = true)]
    pub reported_reclaimable: Option<String>,
}

/// What one developer tool reports about its storage. Read only on request;
/// nothing here removes anything.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ToolStorageView {
    /// The tool.
    pub tool: ToolKindView,
    /// Whether it answered.
    pub state: ToolStateView,
    /// Its version, as it prints it.
    #[schema(required = true)]
    pub version: Option<String>,
    /// Which installation or daemon answered (Docker's context and endpoint).
    #[schema(required = true)]
    pub scope: Option<String>,
    /// What it reported.
    pub items: Vec<ToolStorageItemView>,
    /// The tool's own cleanup command, literally; it runs only through the
    /// confirmed flow (ADR-0049).
    #[schema(required = true)]
    pub command: Option<String>,
    /// The location is a store projects link into (pnpm), not a cache.
    pub shared_store: bool,
}

impl ToolStorageView {
    const fn empty(tool: ToolKindView, state: ToolStateView) -> Self {
        Self {
            tool,
            state,
            version: None,
            scope: None,
            items: Vec::new(),
            command: None,
            shared_store: false,
        }
    }

    fn failed(tool: ToolKindView, error: &lumen_app::xcode::InspectionError) -> Self {
        use lumen_app::tools::ToolError;
        use lumen_app::xcode::InspectionError;
        let state = match error {
            InspectionError::Tool(ToolError::NotFound(_)) => ToolStateView::NotInstalled,
            InspectionError::Unsupported => ToolStateView::Unsupported,
            other => ToolStateView::Unavailable {
                reason: other.to_string(),
            },
        };
        Self::empty(tool, state)
    }

    /// Every tool's report, in a fixed order.
    pub fn all(
        xcode: &Result<lumen_app::xcode::Inventory, lumen_app::xcode::InspectionError>,
        docker: &Result<lumen_app::docker::Inventory, lumen_app::xcode::InspectionError>,
        packages: &lumen_app::packages::Inventory,
    ) -> Vec<Self> {
        let mut views = vec![
            match xcode {
                Ok(inventory) => Self {
                    items: inventory
                        .runtimes
                        .iter()
                        .map(|runtime| ToolStorageItemView {
                            name: format!("{} ({})", runtime.version, runtime.build),
                            detail: Some(runtime.state.clone()),
                            bytes: runtime.reported_image_bytes.clone(),
                            reported_size: None,
                            reported_reclaimable: None,
                        })
                        .collect(),
                    ..Self::empty(ToolKindView::Xcode, ToolStateView::Found)
                },
                Err(error) => Self::failed(ToolKindView::Xcode, error),
            },
            match docker {
                Ok(inventory) => Self {
                    version: Some(inventory.client_version.clone()),
                    scope: Some(format!("{} ({})", inventory.context, inventory.endpoint)),
                    items: inventory
                        .categories
                        .iter()
                        .map(|usage| ToolStorageItemView {
                            name: usage.kind.clone(),
                            detail: Some(format!("{}/{}", usage.active, usage.total)),
                            bytes: None,
                            reported_size: Some(usage.reported_size.clone()),
                            reported_reclaimable: Some(usage.reported_reclaimable.clone()),
                        })
                        .collect(),
                    ..Self::empty(ToolKindView::Docker, ToolStateView::Found)
                },
                Err(error) => Self::failed(ToolKindView::Docker, error),
            },
        ];
        views.extend(packages.tools.iter().map(Self::package));
        views
    }

    fn package(tool: &lumen_app::packages::Tool) -> Self {
        use lumen_app::packages::{Status, StoreKind};
        let kind = match tool.name {
            "Homebrew" => ToolKindView::Homebrew,
            "npm" => ToolKindView::Npm,
            _ => ToolKindView::Pnpm,
        };
        let (state, version, items) = match &tool.status {
            Status::Found { version, location } => (
                ToolStateView::Found,
                Some(version.clone()),
                vec![ToolStorageItemView {
                    name: location.clone(),
                    detail: None,
                    bytes: None,
                    reported_size: None,
                    reported_reclaimable: None,
                }],
            ),
            Status::NotInstalled => (ToolStateView::NotInstalled, None, Vec::new()),
            Status::Unavailable { reason } => (
                ToolStateView::Unavailable {
                    reason: reason.clone(),
                },
                None,
                Vec::new(),
            ),
        };
        Self {
            version,
            items,
            command: tool.cleanup.clone(),
            shared_store: tool.kind == StoreKind::SharedStore,
            ..Self::empty(kind, state)
        }
    }
}

#[cfg(test)]
mod tool_storage_tests {
    use super::*;
    use lumen_app::packages::{Inventory as Packages, Status, StoreKind, Tool};
    use lumen_app::tools::ToolError;
    use lumen_app::xcode::InspectionError;

    #[test]
    fn missing_unsupported_and_failed_tools_are_unknown_never_empty() {
        let packages = Packages {
            schema_version: 1,
            tools: vec![Tool {
                name: "pnpm",
                kind: StoreKind::SharedStore,
                entry: "pnpm-store",
                cleanup: Some("pnpm store prune".into()),
                status: Status::Found {
                    version: "10.33.0".into(),
                    location: "/Users/a/Library/pnpm/store/v10".into(),
                },
            }],
            limitation: "",
        };
        let views = ToolStorageView::all(
            &Err(InspectionError::Unsupported),
            &Err(InspectionError::Tool(ToolError::NotFound("docker".into()))),
            &packages,
        );
        assert_eq!(views[0].state, ToolStateView::Unsupported);
        assert_eq!(views[1].state, ToolStateView::NotInstalled);
        assert!(views[0].items.is_empty() && views[1].items.is_empty());
        assert_eq!(views[2].tool, ToolKindView::Pnpm);
        assert!(views[2].shared_store);
        assert_eq!(views[2].command.as_deref(), Some("pnpm store prune"));
        let timed_out = ToolStorageView::all(
            &Err(InspectionError::Timeout),
            &Err(InspectionError::Timeout),
            &packages,
        );
        assert!(matches!(
            timed_out[0].state,
            ToolStateView::Unavailable { .. }
        ));
    }
}
