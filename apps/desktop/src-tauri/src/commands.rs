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
    QuarantineReportView, RestoreOutcomeView, ScanProgressView, ScanRequest, ScanView,
};
use lumen_app::{ScanEvent, SystemClock, UuidV7Ids};
use lumen_application::CancelToken;
use lumen_application::ports::NoopJudge;
use lumen_domain::{ConfirmationSurface, OperationId, PlanId};
use serde::Serialize;
use tauri::State;
use tauri::ipc::Channel;

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

/// How often a running scan reports its totals to the window.
const PROGRESS_EVERY: std::time::Duration = std::time::Duration::from_millis(250);

/// Turns scan events into progress views, at most every [`PROGRESS_EVERY`].
struct Progress {
    started: std::time::Instant,
    last: Option<std::time::Instant>,
    unreadable: u64,
}

impl Progress {
    fn view(&mut self, event: &ScanEvent) -> Option<ScanProgressView> {
        match event {
            ScanEvent::Unreadable { .. } => {
                self.unreadable += 1;
                None
            }
            ScanEvent::Progress {
                totals,
                allocated,
                current,
            } => {
                let now = std::time::Instant::now();
                if self
                    .last
                    .is_some_and(|last| now.duration_since(last) < PROGRESS_EVERY)
                {
                    return None;
                }
                self.last = Some(now);
                Some(ScanProgressView {
                    entries: totals.entries.to_string(),
                    directories: totals.directories.to_string(),
                    allocated_bytes: allocated.get().to_string(),
                    unreadable: self.unreadable.to_string(),
                    current: current.as_ref().map(lumen_domain::RawPath::display),
                    elapsed_ms: now.duration_since(self.started).as_millis().to_string(),
                })
            }
            _ => None,
        }
    }
}

/// Scans the chosen roots, or the whole startup disk when none are given,
/// read-only, and records the scan. `progress` receives the totals while it
/// runs.
#[tauri::command]
pub(crate) async fn scan(
    core: State<'_, Core>,
    request: ScanRequest,
    progress: Channel<ScanProgressView>,
) -> Result<ScanView, CommandError> {
    let db = core.db.clone();
    blocking(move || {
        let roots: Vec<PathBuf> = if request.roots.is_empty() {
            lumen_app::roots::whole_disk()
        } else {
            request.roots.iter().map(PathBuf::from).collect()
        };
        if roots.is_empty() {
            return Err(CommandError::new(
                "no folder was chosen and the whole disk could not be found",
            ));
        }
        let mut live = Progress {
            started: std::time::Instant::now(),
            last: None,
            unreadable: 0,
        };
        let mut listening = true;
        let summary = lumen_app::scan(
            &roots,
            Some(&db),
            &SystemClock,
            &UuidV7Ids,
            &CancelToken::new(),
            &mut |event| {
                // Once the window is gone, stop sending; the scan still
                // finishes and is recorded.
                if let Some(view) = live.view(event)
                    && listening
                {
                    listening = progress.send(view).is_ok();
                }
            },
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

#[cfg(test)]
mod tests {
    use lumen_domain::{AccessState, ByteCount, DenialReason, RawPath};
    use lumen_scan::ScanProgress;

    use super::*;

    fn path(text: &str) -> RawPath {
        RawPath::from_unix_bytes(text.as_bytes().to_vec()).unwrap_or_else(|e| unreachable!("{e}"))
    }

    #[test]
    fn progress_counts_unreadable_folders_and_is_throttled() {
        let mut live = Progress {
            started: std::time::Instant::now(),
            last: None,
            unreadable: 0,
        };
        let denied = ScanEvent::Unreadable {
            path: path("/Users/ana/Desktop"),
            access: AccessState::Denied(DenialReason::Tcc),
        };
        assert_eq!(live.view(&denied), None);
        let totals = ScanEvent::Progress {
            totals: ScanProgress {
                entries: 1_000,
                directories: 10,
                skipped: 0,
                file_bytes: ByteCount::new(5_000),
            },
            allocated: ByteCount::new(4_096),
            current: Some(path("/Users/ana/Library")),
        };
        let first = live
            .view(&totals)
            .unwrap_or_else(|| unreachable!("the first totals are sent"));
        assert_eq!(
            (
                first.entries.as_str(),
                first.allocated_bytes.as_str(),
                first.unreadable.as_str()
            ),
            ("1000", "4096", "1")
        );
        assert_eq!(first.current.as_deref(), Some("/Users/ana/Library"));
        assert_eq!(live.view(&totals), None, "at most every 250 ms");
    }
}
