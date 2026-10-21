//! The commands the UI may invoke (ADR-0004, ADR-0023): one per read or
//! intent, each a thin call into `lumen-app` that returns a `lumen-api` view.
//!
//! Reads return view models. Mutations name a plan (with its full hash as the
//! confirmation) or an operation; no command accepts a path to act on. Scans
//! take roots, but only read. Every command runs on a blocking thread so the
//! webview never waits on the filesystem.
//!
//! AI review (ADR-0037) is a setting next to the database: the UI can read
//! it, turn it on (only with a stored key) or off, and store a Jeb key. No
//! command returns a key.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use lumen_api::views::{
    AiSettingsView, CandidateView, DiskView, ExecuteRequest, FinalizeOutcomeView, HeldItemView,
    PlanView, QuarantineReportView, RestoreOutcomeView, ScanProgressView, ScanRequest, ScanView,
};
use lumen_app::ai::Service;
use lumen_app::{ScanEvent, SystemClock, UuidV7Ids};
use lumen_application::CancelToken;
use lumen_application::assess::Assessed;
use lumen_application::ports::{Clock as _, JudgeModel, NoopJudge, SecretStore};
use lumen_domain::{ConfirmationSurface, OperationId, PlanId};
use serde::Serialize;
use tauri::State;
use tauri::ipc::Channel;

/// The core's state for this app: where its databases live, and where judge
/// keys are kept.
pub(crate) struct Core {
    /// `index.db` in the app's local data folder; the ledger and the AI
    /// review setting sit next to it.
    pub(crate) db: PathBuf,
    /// The OS secret store (tests use an in-memory one); `None` where the
    /// platform has none, which leaves AI review unavailable.
    pub(crate) secrets: Option<Arc<dyn SecretStore>>,
}

impl std::fmt::Debug for Core {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Core")
            .field("db", &self.db)
            .field("secrets", &self.secrets.is_some())
            .finish()
    }
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
    /// The view to send for `event` seen at `now`, if any.
    fn view(&mut self, event: &ScanEvent, now: std::time::Instant) -> Option<ScanProgressView> {
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
                if let Some(view) = live.view(event, std::time::Instant::now())
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

/// Runs `work` with the judge while AI review is on and a key is stored, and
/// records its consultations in the ledger; otherwise with no judge. A judge
/// that cannot be set up or answer never fails the command: the policy
/// decides alone.
fn with_judge<T>(
    db: &Path,
    secrets: Option<&dyn SecretStore>,
    work: impl FnOnce(&dyn JudgeModel) -> Result<T, CommandError>,
    assessed: impl Fn(&T) -> &[Assessed],
) -> Result<T, CommandError> {
    let Some(judge) = lumen_app::ai::active_judge(db, secrets) else {
        return work(&NoopJudge);
    };
    let result = work(&judge)?;
    lumen_app::ai::record_traces(db, assessed(&result)).map_err(CommandError::new)?;
    Ok(result)
}

/// Decides the latest completed scan; moves nothing.
#[tauri::command]
pub(crate) async fn assess(core: State<'_, Core>) -> Result<Vec<CandidateView>, CommandError> {
    let db = core.db.clone();
    let secrets = core.secrets.clone();
    blocking(move || {
        let assessed = with_judge(
            &db,
            secrets.as_deref(),
            |judge| lumen_app::assess(&db, &SystemClock, judge).map_err(CommandError::new),
            Vec::as_slice,
        )?;
        Ok(assessed.iter().map(CandidateView::from).collect())
    })
    .await
}

/// Plans every candidate proposed for quarantine; moves nothing.
#[tauri::command]
pub(crate) async fn plan(core: State<'_, Core>) -> Result<PlanView, CommandError> {
    let db = core.db.clone();
    let secrets = core.secrets.clone();
    blocking(move || {
        let planned = with_judge(
            &db,
            secrets.as_deref(),
            |judge| {
                lumen_app::plan(&db, &SystemClock, &UuidV7Ids, judge).map_err(CommandError::new)
            },
            |planned| planned.items.as_slice(),
        )?;
        Ok(PlanView::from(&planned))
    })
    .await
}

/// The AI review setting next to `db` and which key is stored.
fn ai_view(db: &Path, secrets: Option<&dyn SecretStore>) -> Result<AiSettingsView, CommandError> {
    let key = match secrets {
        Some(store) => lumen_app::ai::stored_service(store).map_err(CommandError::new)?,
        None => None,
    };
    Ok(AiSettingsView::new(lumen_app::ai::load(db), key))
}

/// How full the disk holding the user's home folder is: a read of the
/// volume's size and free space, nothing else.
#[tauri::command]
pub(crate) async fn disk() -> Result<DiskView, CommandError> {
    blocking(move || {
        let home =
            std::env::home_dir().ok_or_else(|| CommandError::new("the home folder is unknown"))?;
        lumen_app::space::volume(&home)
            .map(DiskView::from)
            .map_err(CommandError::new)
    })
    .await
}

/// System Settings › Privacy & Security › Full Disk Access (verified on
/// macOS 26). Apple does not document these addresses; if one stops working,
/// System Settings opens on its main page instead.
#[cfg(target_os = "macos")]
const FULL_DISK_ACCESS: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles";

/// Opens the Full Disk Access pane of System Settings, so the user can allow
/// Lumen there. A fixed address with no input from the window; Lumen grants
/// itself nothing. Only on macOS.
#[tauri::command]
pub(crate) async fn open_full_disk_access() -> Result<(), CommandError> {
    blocking(open_settings_pane).await
}

#[cfg(target_os = "macos")]
fn open_settings_pane() -> Result<(), CommandError> {
    let status = std::process::Command::new("/usr/bin/open")
        .arg(FULL_DISK_ACCESS)
        .status()
        .map_err(CommandError::new)?;
    if status.success() {
        Ok(())
    } else {
        Err(CommandError::new(format!(
            "System Settings could not be opened ({status})"
        )))
    }
}

#[cfg(not(target_os = "macos"))]
fn open_settings_pane() -> Result<(), CommandError> {
    Err(CommandError::new("Full Disk Access exists only on macOS"))
}

/// Whether AI review is on, and which key it would use (never the key).
#[tauri::command]
pub(crate) async fn ai_settings(core: State<'_, Core>) -> Result<AiSettingsView, CommandError> {
    let db = core.db.clone();
    let secrets = core.secrets.clone();
    blocking(move || ai_view(&db, secrets.as_deref())).await
}

/// Turns AI review on (the user's consent, recorded with its time) or off.
/// Turning it on needs a stored key.
#[tauri::command]
pub(crate) async fn set_ai_review(
    core: State<'_, Core>,
    enabled: bool,
) -> Result<AiSettingsView, CommandError> {
    let db = core.db.clone();
    let secrets = core.secrets.clone();
    blocking(move || {
        if enabled {
            let store = secrets
                .as_deref()
                .ok_or_else(|| CommandError::new("AI review is not available on this platform"))?;
            if lumen_app::ai::stored_key(store)
                .map_err(CommandError::new)?
                .is_none()
            {
                return Err(CommandError::new(
                    "AI review needs an API key: add your Jeb key first",
                ));
            }
        }
        lumen_app::ai::set(&db, enabled, SystemClock.now()).map_err(CommandError::new)?;
        ai_view(&db, secrets.as_deref())
    })
    .await
}

/// Stores a Jeb API key in the system's secret store. It is checked, never
/// logged, and never returned; storing it sends nothing anywhere.
#[tauri::command]
pub(crate) async fn set_ai_key(
    core: State<'_, Core>,
    key: String,
) -> Result<AiSettingsView, CommandError> {
    let db = core.db.clone();
    let secrets = core.secrets.clone();
    blocking(move || {
        let store = secrets
            .as_deref()
            .ok_or_else(|| CommandError::new("AI review is not available on this platform"))?;
        lumen_app::ai::store_key(store, Service::Jeb, &key).map_err(CommandError::new)?;
        ai_view(&db, Some(store))
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
        let start = std::time::Instant::now();
        let at = |ms| start + std::time::Duration::from_millis(ms);
        let mut live = Progress {
            started: start,
            last: None,
            unreadable: 0,
        };
        let denied = ScanEvent::Unreadable {
            path: path("/Users/ana/Desktop"),
            access: AccessState::Denied(DenialReason::Tcc),
        };
        assert_eq!(live.view(&denied, at(0)), None);
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
            .view(&totals, at(10))
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
        assert_eq!(first.elapsed_ms, "10");
        assert_eq!(live.view(&totals, at(259)), None, "at most every 250 ms");
        assert!(live.view(&totals, at(260)).is_some(), "again after 250 ms");
    }
}
