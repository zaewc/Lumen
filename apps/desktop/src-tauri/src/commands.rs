//! The commands the UI may invoke (ADR-0004, ADR-0023): one per read or
//! intent, each a thin call into `lumen-app` that returns a `lumen-api` view.
//!
//! Reads return view models. Mutations name a plan (with its full hash as the
//! confirmation) or an operation; no command accepts a path to act on. Scans
//! take roots, but only read. Every command runs on a blocking thread so the
//! webview never waits on the filesystem.

use std::path::PathBuf;

use lumen_api::views::{
    CandidateView, ExecuteRequest, FinalizeOutcomeView, HeldItemView, PlanView,
    QuarantineReportView, RestoreOutcomeView, ScanRequest, ScanView,
};
use lumen_app::{SystemClock, UuidV7Ids};
use lumen_application::CancelToken;
use lumen_application::ports::NoopJudge;
use lumen_domain::{ConfirmationSurface, OperationId, PlanId};
use serde::Serialize;
use tauri::State;

/// The core's state for this app: where its databases live.
#[derive(Debug)]
pub(crate) struct Core {
    /// `index.db` in the app's local data folder; the ledger sits next to it.
    pub(crate) db: PathBuf,
}

/// A failed command, as the UI sees it: a message for people, never a stack
/// trace or a path to act on.
#[derive(Debug, Serialize, thiserror::Error)]
#[error("{message}")]
pub(crate) struct CommandError {
    message: String,
}

impl CommandError {
    fn new(error: impl std::fmt::Display) -> Self {
        Self {
            message: error.to_string(),
        }
    }
}

/// Runs `work` on a blocking thread.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, CommandError> + Send + 'static,
) -> Result<T, CommandError> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(CommandError::new)?
}

/// Scans the chosen roots, read-only, and records the scan.
#[tauri::command]
pub(crate) async fn scan(
    core: State<'_, Core>,
    request: ScanRequest,
) -> Result<ScanView, CommandError> {
    let db = core.db.clone();
    blocking(move || {
        let roots: Vec<PathBuf> = request.roots.iter().map(PathBuf::from).collect();
        let summary = lumen_app::scan(
            &roots,
            Some(&db),
            &SystemClock,
            &UuidV7Ids,
            &CancelToken::new(),
            &mut |_| {},
        )
        .map_err(CommandError::new)?;
        Ok(ScanView::from(&summary))
    })
    .await
}

/// Decides the latest completed scan; moves nothing.
#[tauri::command]
pub(crate) async fn assess(core: State<'_, Core>) -> Result<Vec<CandidateView>, CommandError> {
    let db = core.db.clone();
    blocking(move || {
        let assessed =
            lumen_app::assess(&db, &SystemClock, &NoopJudge).map_err(CommandError::new)?;
        Ok(assessed.iter().map(CandidateView::from).collect())
    })
    .await
}

/// Plans every candidate proposed for quarantine; moves nothing.
#[tauri::command]
pub(crate) async fn plan(core: State<'_, Core>) -> Result<PlanView, CommandError> {
    let db = core.db.clone();
    blocking(move || {
        let planned = lumen_app::plan(&db, &SystemClock, &UuidV7Ids, &NoopJudge)
            .map_err(CommandError::new)?;
        Ok(PlanView::from(&planned))
    })
    .await
}

/// Executes one plan, confirmed in this app by its full hash (ADR-0034).
#[tauri::command]
pub(crate) async fn execute(
    core: State<'_, Core>,
    request: ExecuteRequest,
) -> Result<QuarantineReportView, CommandError> {
    let db = core.db.clone();
    blocking(move || {
        let plan: PlanId = request
            .plan
            .parse()
            .map_err(|_| CommandError::new("not a plan ID"))?;
        let report = lumen_app::quarantine::quarantine(
            &db,
            plan,
            &request.confirmation,
            ConfirmationSurface::DesktopApp,
            &SystemClock,
            &UuidV7Ids,
        )
        .map_err(CommandError::new)?;
        Ok(QuarantineReportView::from(&report))
    })
    .await
}

/// What quarantine holds.
#[tauri::command]
pub(crate) async fn held(core: State<'_, Core>) -> Result<Vec<HeldItemView>, CommandError> {
    let db = core.db.clone();
    blocking(move || {
        let items = lumen_app::held::list(&db).map_err(CommandError::new)?;
        Ok(items.iter().map(HeldItemView::from).collect())
    })
    .await
}

fn operation(text: &str) -> Result<OperationId, CommandError> {
    text.parse()
        .map_err(|_| CommandError::new("not an operation ID"))
}

/// Moves one item back, never over anything.
#[tauri::command]
pub(crate) async fn restore(
    core: State<'_, Core>,
    operation_id: String,
) -> Result<RestoreOutcomeView, CommandError> {
    let db = core.db.clone();
    blocking(move || {
        let outcome = lumen_app::held::restore(&db, operation(&operation_id)?, &SystemClock)
            .map_err(CommandError::new)?;
        Ok(RestoreOutcomeView::from(&outcome))
    })
    .await
}

/// Removes one item for good, only if nothing uses it (ADR-0030).
#[tauri::command]
pub(crate) async fn finalize(
    core: State<'_, Core>,
    operation_id: String,
) -> Result<FinalizeOutcomeView, CommandError> {
    let db = core.db.clone();
    blocking(move || {
        let outcome = lumen_app::held::finalize(&db, operation(&operation_id)?, &SystemClock)
            .map_err(CommandError::new)?;
        Ok(FinalizeOutcomeView::from(&outcome))
    })
    .await
}
