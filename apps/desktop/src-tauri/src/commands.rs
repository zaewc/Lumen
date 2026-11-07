//! The commands the UI may invoke (ADR-0004, ADR-0023): one per read or
//! intent, each a thin call into `lumen-app` that returns a `lumen-api` view.
//!
//! Reads return view models. Mutations name a plan (with its full hash as the
//! confirmation) or an operation; no command accepts a path to act on. Scans
//! take roots, but only read. Comparing copies reads contents, so it takes no
//! roots: the host asks the person in a dialog (ADR-0057). Every command runs on a blocking thread so the
//! webview never waits on the filesystem.
//!
//! AI review (ADR-0037) is a setting next to the database: the UI can read
//! it, turn it on (only with a stored key) or off, and store a Jeb key. No
//! command returns a key.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use lumen_api::views::{
    AiSettingsView, AssessProgressView, CandidateView, CompareOutcomeView, CompareProgressView,
    DiskView, ExecuteRequest, FinalizeOutcomeView, HeldItemView, PlanView, QuarantineReportView,
    ReassessRequest, ReassessView, RestoreOutcomeView, ScanProgressView, ScanRequest, ScanView,
    SoftwareView, ToolRunView, ToolStorageView, UninstallOutcomeView, UninstallPreviewView,
    candidate_id,
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
    /// The latest assessment, as shown: what `reassess` checks again and
    /// updates (ADR-0058).
    pub(crate) assessed: std::sync::Mutex<Vec<Assessed>>,
    /// The running comparison of copies, if any: one at a time, and what
    /// `cancel_comparison` stops.
    pub(crate) comparison: Arc<std::sync::Mutex<Option<CancelToken>>>,
    /// Startup's reconciliation (UX-3): run once, when the app starts, and
    /// waited for before the held items are first listed.
    pub(crate) settled: Arc<std::sync::Once>,
    /// Clean-ups and Quarantine actions running, and an update installing:
    /// quitting and updating wait for the first, item actions for the second.
    pub(crate) activity: Arc<Activity>,
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
            .field(
                "assessed",
                &self.assessed.try_lock().ok().map(|assessed| assessed.len()),
            )
            .field(
                "comparison",
                &self
                    .comparison
                    .try_lock()
                    .map_or(true, |running| running.is_some()),
            )
            .field("settled", &self.settled.is_completed())
            .field("activity", &self.activity)
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

/// What quitting and updating must not cut short (UX-3): how many item
/// actions (a clean-up, a restore, a removal for good) run, and whether an
/// update installs. An update restarts Lumen, so the two never overlap.
#[derive(Debug, Default)]
pub(crate) struct Activity(std::sync::Mutex<Busy>);

#[derive(Debug, Default)]
struct Busy {
    moving: usize,
    updating: bool,
}

impl Activity {
    fn busy(&self) -> std::sync::MutexGuard<'_, Busy> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Marks an item action as running until the guard drops; refused while
    /// an update installs.
    pub(crate) fn moving(self: &Arc<Self>) -> Result<Moving, CommandError> {
        let mut busy = self.busy();
        if busy.updating {
            return Err(CommandError::new(
                "Lumen is installing an update; try again once it restarts",
            ));
        }
        busy.moving += 1;
        Ok(Moving(Arc::clone(self)))
    }

    /// Marks an update as installing until the guard drops (or Lumen
    /// restarts); refused while an item action runs.
    pub(crate) fn updating(self: &Arc<Self>) -> Result<Updating, CommandError> {
        let mut busy = self.busy();
        if busy.moving > 0 {
            return Err(CommandError::new(
                "Lumen is moving items; update once that finishes",
            ));
        }
        busy.updating = true;
        Ok(Updating(Arc::clone(self)))
    }

    /// Whether an item action runs, so quitting now could interrupt it.
    pub(crate) fn is_moving(&self) -> bool {
        self.busy().moving > 0
    }
}

/// An item action in progress; see [`Activity::moving`].
#[derive(Debug)]
pub(crate) struct Moving(Arc<Activity>);

impl Drop for Moving {
    fn drop(&mut self) {
        let mut busy = self.0.busy();
        busy.moving = busy.moving.saturating_sub(1);
    }
}

/// An update installing; see [`Activity::updating`].
#[derive(Debug)]
pub(crate) struct Updating(Arc<Activity>);

impl Drop for Updating {
    fn drop(&mut self) {
        self.0.busy().updating = false;
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
        Ok((views, assessed))
    })
    .await?;
    *core
        .tool_entries
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = tool_entries(&entries);
    *core
        .assessed
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = entries;
    Ok(views)
}

/// The knowledge-base entries with a tool command among `assessed`, with
/// their items: the only commands the page may run (ADR-0049).
fn tool_entries(
    assessed: &[Assessed],
) -> std::collections::BTreeMap<String, Vec<(lumen_domain::RawPath, lumen_domain::VolumeId)>> {
    let mut entries = std::collections::BTreeMap::<_, Vec<_>>::new();
    for item in assessed {
        if let Some(guidance) = lumen_app::assess::tool_action(item) {
            entries.entry(guidance.entry).or_default().push((
                item.candidate.path.clone(),
                item.candidate.identity().volume.clone(),
            ));
        }
    }
    entries
}

/// Checks items of the latest assessment again with fresh read-only evidence
/// (ADR-0058), by ID; none: every item kept or reviewed because of something
/// Lumen did not know. Moves nothing. Items decided again replace their
/// earlier assessment, so later checks and tool commands see them; planning
/// assesses again from the scan itself.
#[tauri::command]
pub(crate) async fn reassess(
    core: State<'_, Core>,
    request: ReassessRequest,
) -> Result<ReassessView, CommandError> {
    let chosen: Vec<Assessed> = {
        let latest = core
            .assessed
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if request.ids.is_empty() {
            latest
                .iter()
                .filter(|item| !lumen_application::reassess::gaps(item).is_empty())
                .cloned()
                .collect()
        } else {
            let mut chosen = Vec::new();
            for id in &request.ids {
                let before = chosen.len();
                chosen.extend(
                    latest
                        .iter()
                        .filter(|item| candidate_id(item) == *id)
                        .cloned(),
                );
                if chosen.len() == before {
                    return Err(CommandError::new(
                        "that item is not in the latest assessment; assess again",
                    ));
                }
            }
            chosen
        }
    };
    let db = core.db.clone();
    let (view, chosen, result) = blocking(move || {
        let result = lumen_app::reassess::reassess(&db, &chosen, &SystemClock, &CancelToken::new())
            .map_err(CommandError::new)?;
        Ok((ReassessView::new(&chosen, &result), chosen, result))
    })
    .await?;
    let mut latest = core
        .assessed
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for (earlier, item) in chosen.iter().zip(&result.items) {
        if let lumen_app::reassess::Outcome::Decided { after, .. } = &item.outcome
            && let Some(slot) = latest
                .iter_mut()
                .find(|slot| slot.candidate.path == earlier.candidate.path)
        {
            *slot = (**after).clone();
        }
    }
    *core
        .tool_entries
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = tool_entries(&latest);
    Ok(view)
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
                lumen_app::plan(&db, &SystemClock, &UuidV7Ids, judge, &[])
                    .map_err(CommandError::new)
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
    Ok(AiSettingsView::new(lumen_app::ai::choice(db), key))
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

/// What Xcode, Docker and the package managers report about their own
/// storage: read-only queries with fixed arguments, run at the same time so a
/// slow tool (each is bounded to 30 seconds) does not hold up the others.
#[tauri::command]
pub(crate) async fn tool_storage() -> Result<Vec<ToolStorageView>, CommandError> {
    blocking(move || {
        let (xcode, docker, packages) = std::thread::scope(|scope| {
            let xcode = scope.spawn(lumen_app::xcode::inspect);
            let docker = scope.spawn(lumen_app::docker::inspect);
            let packages = lumen_app::packages::inspect();
            (xcode.join(), docker.join(), packages)
        });
        let (Ok(xcode), Ok(docker)) = (xcode, docker) else {
            return Err(CommandError::new("a tool inspection stopped unexpectedly"));
        };
        Ok(ToolStorageView::all(&xcode, &docker, &packages))
    })
    .await
}

/// The installed software list (ADR-0060): every inventory the platform keeps
/// of installed applications and packages, read on request, with the AI notes
/// already cached on the device (ADR-0061). Read only: nothing is removed,
/// and nothing is sent.
#[tauri::command]
pub(crate) async fn software(core: State<'_, Core>) -> Result<SoftwareView, CommandError> {
    let db = core.db.clone();
    blocking(move || {
        let reports = lumen_app::software::inventory(&CancelToken::new());
        let notes = lumen_app::software_notes::NoteCache::load(&db);
        Ok(SoftwareView::from_reports(&reports).with_notes(&notes))
    })
    .await
}

/// The installed software list, after asking AI review about software it has
/// not explained yet (ADR-0061): only while AI review is on and a key is
/// available, at most 40 per call, each recorded in the ledger. Otherwise the
/// list with the notes already cached, and nothing is sent.
#[tauri::command]
pub(crate) async fn explain_software(core: State<'_, Core>) -> Result<SoftwareView, CommandError> {
    let db = core.db.clone();
    let secrets = core.secrets.clone();
    let builtin = core.builtin.clone();
    blocking(move || {
        let reports = lumen_app::software::inventory(&CancelToken::new());
        let notes = match lumen_app::ai::active_judge(&db, secrets.as_deref(), builtin.as_ref()) {
            Some(judge) => {
                let subjects = lumen_api::views::software_subjects(&reports);
                lumen_app::software_notes::explain(&db, &judge, &subjects)
                    .map_err(CommandError::new)?
            }
            None => lumen_app::software_notes::NoteCache::load(&db),
        };
        Ok(SoftwareView::from_reports(&reports).with_notes(&notes))
    })
    .await
}

/// Whether this build can show the folder dialog (ADR-0057). Without it,
/// comparing is unsupported here even where the core can read contents.
const DIALOG: bool = cfg!(any(target_os = "macos", windows));

/// The comparison slot while a comparison runs; emptied when it ends,
/// however it ends.
struct Comparing(Arc<std::sync::Mutex<Option<CancelToken>>>);

impl Comparing {
    /// Takes the slot with `cancel`, unless another comparison holds it.
    fn start(
        slot: &Arc<std::sync::Mutex<Option<CancelToken>>>,
        cancel: &CancelToken,
    ) -> Result<Self, CommandError> {
        let mut running = slot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if running.is_some() {
            return Err(CommandError::new("another comparison is still running"));
        }
        *running = Some(cancel.clone());
        Ok(Self(Arc::clone(slot)))
    }
}

impl Drop for Comparing {
    fn drop(&mut self) {
        *self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
    }
}

/// Asks which folders to compare in the system's folder dialog, over the
/// window, then finds identical copies in them (ADR-0057). Takes no path: only
/// what the person picks is read, and nothing is chosen, moved or removed.
/// Where contents cannot be read yet, says so without asking. One comparison
/// at a time; `cancel_comparison` stops it.
#[tauri::command]
pub(crate) async fn compare_folders<R: tauri::Runtime>(
    core: State<'_, Core>,
    window: tauri::WebviewWindow<R>,
    progress: Channel<CompareProgressView>,
) -> Result<CompareOutcomeView, CommandError> {
    if !DIALOG || !lumen_app::duplicates::available() {
        return Ok(CompareOutcomeView::Unsupported);
    }
    let cancel = CancelToken::new();
    let _running = Comparing::start(&core.comparison, &cancel)?;
    let Some(folders) = choose_folders(&window).await else {
        return Ok(CompareOutcomeView::NothingChosen);
    };
    blocking(move || {
        let mut listening = true;
        let mut last: Option<(lumen_app::duplicates::HashStage, std::time::Instant)> = None;
        let found = lumen_app::duplicates::find(&folders, &cancel, &mut |stage, done| {
            let now = std::time::Instant::now();
            let due = last
                .is_none_or(|(seen, at)| seen != stage || now.duration_since(at) >= PROGRESS_EVERY);
            if listening && due {
                last = Some((stage, now));
                listening = progress.send(CompareProgressView::new(stage, done)).is_ok();
            }
        });
        CompareOutcomeView::new(&folders, found).map_err(CommandError::new)
    })
    .await
}

/// Stops the running comparison, if any; it then ends as cancelled and shows
/// no groups. Reads and changes nothing else.
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri hands a command its state by value"
)]
pub(crate) fn cancel_comparison(core: State<'_, Core>) {
    if let Some(cancel) = core
        .comparison
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .as_ref()
    {
        cancel.cancel();
    }
}

/// The folders the person picks, in a dialog owned by `window` (a sheet on
/// macOS); `None` if they pick none.
#[cfg(any(target_os = "macos", windows))]
async fn choose_folders<R: tauri::Runtime>(
    window: &tauri::WebviewWindow<R>,
) -> Option<Vec<PathBuf>> {
    let chosen = rfd::AsyncFileDialog::new()
        .set_parent(window)
        .set_can_create_directories(false)
        .pick_folders()
        .await?;
    let folders: Vec<PathBuf> = chosen
        .iter()
        .map(|folder| folder.path().to_path_buf())
        .collect();
    (!folders.is_empty()).then_some(folders)
}

/// No dialog where contents cannot be compared yet.
#[cfg(not(any(target_os = "macos", windows)))]
fn choose_folders<R: tauri::Runtime>(
    _: &tauri::WebviewWindow<R>,
) -> std::future::Ready<Option<Vec<PathBuf>>> {
    std::future::ready(None)
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
    let moving = core.activity.moving()?;
    blocking(move || {
        let _moving = moving;
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

/// The longest software-list key accepted; the list's keys are an origin and
/// a display-escaped identifier.
const MAX_SOFTWARE_KEY: usize = 4096;

/// Previews uninstalling the macOS application the software list shows under
/// `key` (ADR-0062): the app, and either a preview to confirm or why it
/// cannot be uninstalled into quarantine. Takes the list's key, never a
/// path; reads only, and stores the preview. Elsewhere it says it is
/// unsupported.
#[tauri::command]
pub(crate) async fn uninstall_preview(
    core: State<'_, Core>,
    key: String,
) -> Result<UninstallPreviewView, CommandError> {
    if key.len() > MAX_SOFTWARE_KEY {
        return Err(CommandError::new("not a software key"));
    }
    let db = core.db.clone();
    blocking(move || {
        UninstallPreviewView::from_result(lumen_app::uninstall::preview(
            &db,
            &key,
            &SystemClock,
            &UuidV7Ids,
        ))
        .map_err(CommandError::new)
    })
    .await
}

/// Uninstalls one app into quarantine: executes the preview `preview`,
/// confirmed in this app by its full hash (ADR-0062, ADR-0034). The gate is
/// checked again right before the move.
#[tauri::command]
pub(crate) async fn uninstall(
    core: State<'_, Core>,
    preview: String,
    confirmation: String,
) -> Result<UninstallOutcomeView, CommandError> {
    let db = core.db.clone();
    let moving = core.activity.moving()?;
    blocking(move || {
        let _moving = moving;
        let plan: PlanId = preview
            .parse()
            .map_err(|_| CommandError::new("not a preview ID"))?;
        let report = lumen_app::uninstall::uninstall(
            &db,
            plan,
            &confirmation,
            ConfirmationSurface::DesktopApp,
            &SystemClock,
            &UuidV7Ids,
        )
        .map_err(CommandError::new)?;
        Ok(UninstallOutcomeView::from(&report))
    })
    .await
}

/// Settles cleanups a crash or quit interrupted, once per launch, so what
/// they moved is listed without another cleanup (UX-3). A failure is logged
/// and leaves everything where it is; the app works on regardless.
pub(crate) fn settle_once(db: &Path, settled: &std::sync::Once) {
    settled.call_once(|| match lumen_app::held::reconcile(db, &SystemClock) {
        Ok(Some(_)) => {}
        Ok(None) => {
            tracing::info!("another Lumen holds the ledger; it settles what was interrupted");
        }
        Err(error) => tracing::warn!(%error, "interrupted cleanups were not reconciled"),
    });
}

/// What quarantine holds.
#[tauri::command]
pub(crate) async fn held(core: State<'_, Core>) -> Result<Vec<HeldItemView>, CommandError> {
    let db = core.db.clone();
    let settled = Arc::clone(&core.settled);
    blocking(move || {
        settle_once(&db, &settled);
        let items = lumen_app::held::list(&db, &SystemClock).map_err(CommandError::new)?;
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
    let moving = core.activity.moving()?;
    blocking(move || {
        let _moving = moving;
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
    let moving = core.activity.moving()?;
    blocking(move || {
        let _moving = moving;
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
    fn one_comparison_runs_at_a_time_and_its_slot_empties_when_it_ends() {
        let slot = Arc::default();
        let cancel = CancelToken::new();
        let running = Comparing::start(&slot, &cancel).unwrap_or_else(|e| unreachable!("{e}"));
        assert!(Comparing::start(&slot, &CancelToken::new()).is_err());
        if let Some(held) = slot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
        {
            held.cancel();
        }
        assert!(cancel.is_cancelled(), "the slot holds the running token");
        drop(running);
        assert!(
            slot.lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .is_none()
        );
        assert!(Comparing::start(&slot, &CancelToken::new()).is_ok());
    }

    #[test]
    fn updates_and_item_actions_wait_for_each_other() {
        let activity = Arc::new(Activity::default());
        let first = activity.moving().unwrap_or_else(|e| unreachable!("{e}"));
        let second = activity.moving().unwrap_or_else(|e| unreachable!("{e}"));
        assert!(activity.is_moving());
        assert!(activity.updating().is_err(), "not while items move");
        drop(first);
        assert!(activity.is_moving(), "one is still running");
        assert!(activity.updating().is_err());
        drop(second);
        assert!(!activity.is_moving());
        let update = activity.updating().unwrap_or_else(|e| unreachable!("{e}"));
        assert!(activity.moving().is_err(), "not while an update installs");
        assert!(!activity.is_moving(), "a refused action is not counted");
        drop(update);
        assert!(activity.moving().is_ok(), "again once the update stops");
        assert!(!activity.is_moving(), "and the guard released it");
    }

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
