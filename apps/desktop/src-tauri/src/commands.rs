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
    AiSettingsView, AssessProgressView, CandidateView, DiskView, ExecuteRequest,
    FinalizeOutcomeView, HeldItemView, PlanView, QuarantineReportView, RestoreOutcomeView,
    ScanProgressView, ScanRequest, ScanView, ToolRunView,
};
use lumen_app::ai::{ApiKey, Service};
use lumen_app::{ScanEvent, SystemClock, UuidV7Ids};
use lumen_application::CancelToken;
use lumen_application::assess::Assessed;
use lumen_application::ports::{Clock as _, Describer, JudgeModel, NoopJudge, SecretStore};
use lumen_domain::{ConfirmationSurface, OperationId, PlanId};
use serde::{Deserialize, Serialize};
use tauri::State;
use tauri::ipc::Channel;

/// The core's state for this app: where its databases live, and where judge
/// keys are kept.
pub(crate) struct Core {
    /// `index.db` in the app's local data folder; the ledger and the AI
    /// review setting sit next to it.
    pub(crate) db: PathBuf,
    /// The OS secret store (tests use an in-memory one); `None` where the
    /// platform has none, which leaves only the built-in key.
    pub(crate) secrets: Option<Arc<dyn SecretStore>>,
    /// Lumen's built-in Jeb key in release builds (ADR-0042), used only when
    /// no key is stored. Never shown.
    pub(crate) builtin: Option<ApiKey>,
    /// The knowledge-base entries with a tool command that the latest
    /// assessment matched: the only commands the page may run (ADR-0049).
    pub(crate) tool_entries: std::sync::Mutex<
        std::collections::BTreeMap<String, Vec<(lumen_domain::RawPath, lumen_domain::VolumeId)>>,
    >,
    /// Held while a tool's command runs: one at a time.
    pub(crate) tool_running: Arc<std::sync::Mutex<()>>,
}

impl std::fmt::Debug for Core {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Core")
            .field("db", &self.db)
            .field("secrets", &self.secrets.is_some())
            .field("builtin", &self.builtin.is_some())
            .field(
                "tool_entries",
                &self
                    .tool_entries
                    .try_lock()
                    .ok()
                    .map(|entries| entries.len()),
            )
            .field("tool_running", &self.tool_running.try_lock().is_err())
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
    pub(crate) fn new(error: impl std::fmt::Display) -> Self {
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

/// Runs `work` with the judge (also the describer of review items,
/// ADR-0041) while AI review is on and a key is available (a stored one, else
/// the built-in one, ADR-0042), and records its requests in the ledger;
/// otherwise with neither. A judge that cannot be set up or answer never
/// fails the command: the policy decides alone.
fn with_judge<T>(
    db: &Path,
    secrets: Option<&dyn SecretStore>,
    builtin: Option<&ApiKey>,
    work: impl FnOnce(&dyn JudgeModel, &dyn Describer) -> Result<T, CommandError>,
    assessed: impl Fn(&T) -> &[Assessed],
) -> Result<T, CommandError> {
    let Some(judge) = lumen_app::ai::active_judge(db, secrets, builtin) else {
        return work(&NoopJudge, &NoopJudge);
    };
    let result = work(&judge, &judge)?;
    lumen_app::ai::record_traces(db, assessed(&result)).map_err(CommandError::new)?;
    Ok(result)
}

/// Decides the latest completed scan; moves nothing.
#[tauri::command]
pub(crate) async fn assess(
    core: State<'_, Core>,
    progress: Channel<AssessProgressView>,
) -> Result<Vec<CandidateView>, CommandError> {
    let db = core.db.clone();
    let secrets = core.secrets.clone();
    let builtin = core.builtin.clone();
    let (views, entries) = blocking(move || {
        let mut listening = true;
        let mut last = None;
        let assessed = with_judge(
            &db,
            secrets.as_deref(),
            builtin.as_ref(),
            |judge, describer| {
                lumen_app::assess::assess_with_progress(
                    &db,
                    &SystemClock,
                    judge,
                    describer,
                    &mut |event| {
                        if !listening {
                            return;
                        }
                        let now = std::time::Instant::now();
                        let due = last.is_none_or(|(stage, sent): (_, std::time::Instant)| {
                            stage != event.stage
                                || now.duration_since(sent) >= std::time::Duration::from_millis(250)
                        });
                        if due || event.total == Some(event.completed) {
                            last = Some((event.stage, now));
                            // A closed panel/window stops delivery, never the work.
                            listening = progress.send(AssessProgressView::from(event)).is_ok();
                        }
                    },
                )
                .map_err(CommandError::new)
            },
            Vec::as_slice,
        )?;
        let views: Vec<_> = assessed.iter().map(CandidateView::from).collect();
        let mut entries = std::collections::BTreeMap::<_, Vec<_>>::new();
        for (view, assessed) in views.iter().zip(&assessed) {
            if let Some(command) = &view.tool_command {
                entries.entry(command.entry.clone()).or_default().push((
                    assessed.candidate.path.clone(),
                    assessed.candidate.identity().volume.clone(),
                ));
            }
        }
        Ok((views, entries))
    })
    .await?;
    *core
        .tool_entries
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = entries;
    Ok(views)
}

/// Runs the cleanup command of knowledge-base entry `entry` (ADR-0049), only
/// if the latest assessment matched an item to it. The command comes from
/// the knowledge base, never from the page; the page has confirmed it with
/// the user. What it removes cannot be restored.
#[tauri::command]
pub(crate) async fn run_tool_command(
    core: State<'_, Core>,
    entry: String,
) -> Result<ToolRunView, CommandError> {
    let targets = core
        .tool_entries
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&entry)
        .cloned()
        .ok_or_else(|| {
            CommandError::new("that command is not offered for anything in the latest scan")
        })?;
    let command = lumen_app::tools::builtin(&entry)
        .ok_or_else(|| CommandError::new("that command is not in Lumen's knowledge base"))?;
    let running = Arc::clone(&core.tool_running);
    blocking(move || {
        lumen_app::refuse_privileged().map_err(CommandError::new)?;
        let Ok(_one) = running.try_lock() else {
            return Err(CommandError::new("another command is still running"));
        };
        let home =
            std::env::home_dir().ok_or_else(|| CommandError::new("the home folder is unknown"))?;
        let run =
            lumen_app::tools::run_measured(&command, &home, lumen_app::tools::TIME_LIMIT, &targets)
                .map_err(CommandError::new)?;
        Ok(ToolRunView::from(run))
    })
    .await
}

/// Plans every candidate proposed for quarantine; moves nothing.
#[tauri::command]
pub(crate) async fn plan(core: State<'_, Core>) -> Result<PlanView, CommandError> {
    let db = core.db.clone();
    let secrets = core.secrets.clone();
    let builtin = core.builtin.clone();
    blocking(move || {
        let planned = with_judge(
            &db,
            secrets.as_deref(),
            builtin.as_ref(),
            |judge, _describer| {
                lumen_app::plan(&db, &SystemClock, &UuidV7Ids, judge).map_err(CommandError::new)
            },
            |planned| planned.items.as_slice(),
        )?;
        Ok(PlanView::from(&planned))
    })
    .await
}

/// The AI review setting next to `db`, and which key it would use and where
/// that key comes from (never the key).
fn ai_view(
    db: &Path,
    secrets: Option<&dyn SecretStore>,
    builtin: bool,
) -> Result<AiSettingsView, CommandError> {
    let key = lumen_app::ai::key_source(secrets, builtin).map_err(CommandError::new)?;
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

/// The only appearance choices accepted from the webview.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ThemePreference {
    System,
    Light,
    Dark,
}

/// Changes native window chrome to match the user's appearance preference.
#[tauri::command]
pub(crate) async fn set_theme<R: tauri::Runtime>(app: tauri::AppHandle<R>, theme: ThemePreference) {
    let appearance = match theme {
        ThemePreference::System => None,
        ThemePreference::Light => Some(tauri::Theme::Light),
        ThemePreference::Dark => Some(tauri::Theme::Dark),
    };
    app.set_theme(appearance);
}

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
    let builtin = core.builtin.is_some();
    blocking(move || ai_view(&db, secrets.as_deref(), builtin)).await
}

/// Turns AI review on (the user's consent, recorded with its time) or off.
/// Turning it on needs a key: a stored one, or the built-in one (ADR-0042).
/// Having a key never turns it on by itself.
#[tauri::command]
pub(crate) async fn set_ai_review(
    core: State<'_, Core>,
    enabled: bool,
) -> Result<AiSettingsView, CommandError> {
    let db = core.db.clone();
    let secrets = core.secrets.clone();
    let builtin = core.builtin.clone();
    blocking(move || {
        if enabled {
            if secrets.is_none() && builtin.is_none() {
                return Err(CommandError::new(
                    "AI review is not available on this platform",
                ));
            }
            if lumen_app::ai::judge_key(secrets.as_deref(), builtin.as_ref())
                .map_err(CommandError::new)?
                .is_none()
            {
                return Err(CommandError::new(
                    "AI review needs an API key: add your Jeb key first",
                ));
            }
        }
        lumen_app::ai::set(&db, enabled, SystemClock.now()).map_err(CommandError::new)?;
        ai_view(&db, secrets.as_deref(), builtin.is_some())
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
    let builtin = core.builtin.is_some();
    blocking(move || {
        let store = secrets
            .as_deref()
            .ok_or_else(|| CommandError::new("AI review is not available on this platform"))?;
        lumen_app::ai::store_key(store, Service::Jeb, &key).map_err(CommandError::new)?;
        ai_view(&db, Some(store), builtin)
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

    #[test]
    fn theme_accepts_only_the_three_appearance_choices() {
        for value in [r#""system""#, r#""light""#, r#""dark""#] {
            assert!(
                serde_json::from_str::<ThemePreference>(value).is_ok(),
                "{value}"
            );
        }
        for value in [r#""auto""#, r#""../dark""#, "true", "null", "{}"] {
            assert!(
                serde_json::from_str::<ThemePreference>(value).is_err(),
                "{value}"
            );
        }
    }

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
