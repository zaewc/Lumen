//! Confirmation drawn by the host, outside the main window's webview (FS-1 in
//! `docs/security/public-beta-readiness.md`).
//!
//! A compromised page could call the commands that move or remove files
//! directly, with the plan's hash it was given. So right before anything is
//! moved, removed for good or run, the host opens a window of its own, the
//! confirmation window, and waits for the person's answer there. What it asks
//! about ([`Ask`]) is built by the host from what it stored: the plan, the
//! held item or the knowledge base's command, never from the page. The
//! window's page is Lumen's own bundled `confirm.html`; only that window may
//! read the question and answer it (its own capability), so the main window
//! can neither see nor answer it. Closing the window, Escape or Cancel is a
//! refusal that changed nothing.

use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError, mpsc};

use lumen_app::SystemClock;
use lumen_application::ports::Clock as _;
use lumen_domain::{Chooser, OperationId, PlanId, RawPath};
use lumen_store_sqlite::{IndexStore, open_index};
use serde::Serialize;
use tauri::{Manager as _, Runtime, WebviewUrl, WebviewWindowBuilder};

use crate::commands::CommandError;

/// Asks the person about `ask`; `true` only when they confirmed. Tests use a
/// fixed answer.
pub(crate) type Prompt = dyn Fn(&Ask) -> bool + Send + Sync;

/// The confirmation window's label; its capability names it.
pub(crate) const WINDOW: &str = "confirm";

/// The most review items the person may choose for one plan (ADR-0051): the
/// window lists each of them, never summarised.
pub(crate) const MAX_CHOSEN: usize = 32;

/// The most items of any other list sent to the window; it says how many
/// more there are.
const LISTED: usize = 200;

/// One item the window names: where it is (the core's escaped form, so a
/// deceptive name cannot pass as another path), split by the host at its own
/// separator (a `\\` in a Unix name stays in the name), and what it would
/// free.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct AskItem {
    pub(crate) path: String,
    pub(crate) name: String,
    pub(crate) folder: String,
    /// Bytes, as a decimal string; `None` when unknown.
    pub(crate) bytes: Option<String>,
}

/// Items, at most [`LISTED`] of them, and how many more there are.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct Listed {
    pub(crate) items: Vec<AskItem>,
    pub(crate) more: usize,
}

impl Listed {
    fn of(mut items: Vec<AskItem>) -> Self {
        let more = items.len().saturating_sub(LISTED);
        items.truncate(LISTED);
        Self { items, more }
    }
}

/// What the host asks the person to confirm.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum Ask {
    /// A clean-up: the policy's proposals and the items the person chose.
    Move {
        /// How many items, and what they free together (bytes).
        count: usize,
        total_bytes: String,
        proposed: Listed,
        /// Each one, never summarised.
        chosen: Vec<AskItem>,
    },
    /// Uninstalling an app into Quarantine (ADR-0062).
    Uninstall { items: Vec<AskItem> },
    /// Removing a held item for good.
    Finalize { item: AskItem, chosen: bool },
    /// Running a tool's own command, which cannot be undone (ADR-0049).
    Tool {
        tool: String,
        command: String,
        targets: Listed,
    },
}

/// The question the confirmation window shows now, and where its answer
/// goes. One at a time.
#[derive(Debug, Default)]
pub(crate) struct Pending {
    open: Mutex<Option<(Ask, mpsc::Sender<bool>)>>,
}

impl Pending {
    /// The open question, for the confirmation window.
    pub(crate) fn ask(&self) -> Option<Ask> {
        self.open
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map(|(ask, _)| ask.clone())
    }

    /// The person's answer; nothing happens when no question is open.
    pub(crate) fn answer(&self, yes: bool) {
        if let Some((_, sender)) = self
            .open
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        {
            // The asking command may have stopped waiting; nothing to do then.
            sender.send(yes).ok();
        }
    }
}

/// The confirmation window. Called on a blocking worker, never the main
/// thread: building the window hands it to the main thread and this waits
/// for the answer. The main window is disabled while it is open.
pub(crate) fn window<R: Runtime>(
    app: &tauri::AppHandle<R>,
    pending: &Arc<Pending>,
    ask: &Ask,
) -> bool {
    let (sender, answer) = mpsc::channel();
    {
        let mut open = pending.open.lock().unwrap_or_else(PoisonError::into_inner);
        if open.is_some() {
            return false;
        }
        *open = Some((ask.clone(), sender));
    }
    let main = app.get_webview_window("main");
    let builder = WebviewWindowBuilder::new(app, WINDOW, WebviewUrl::App("confirm.html".into()))
        .title("Lumen")
        .inner_size(480.0, 560.0)
        .resizable(false)
        .minimizable(false)
        .maximizable(false)
        .always_on_top(true)
        .focused(true)
        // Shown once its close handler is in place, so no close is missed.
        .visible(false)
        .center();
    let built = match &main {
        Some(main) => builder.parent(main).and_then(WebviewWindowBuilder::build),
        None => builder.build(),
    };
    let Ok(confirm) = built else {
        pending.answer(false);
        return false;
    };
    let closed = Arc::clone(pending);
    confirm.on_window_event(move |event| {
        if matches!(
            event,
            tauri::WindowEvent::CloseRequested { .. } | tauri::WindowEvent::Destroyed
        ) {
            closed.answer(false);
        }
    });
    if confirm.show().is_err() {
        pending.answer(false);
        confirm.destroy().ok();
        return false;
    }
    if let Some(main) = &main {
        main.set_enabled(false).ok();
    }
    let yes = answer.recv().unwrap_or(false);
    confirm.destroy().ok();
    if let Some(main) = &main {
        main.set_enabled(true).ok();
        main.set_focus().ok();
    }
    yes
}

/// Asks, and turns anything but a confirmation into an error that changed
/// nothing.
fn ask(prompt: &Prompt, question: &Ask) -> Result<(), CommandError> {
    if prompt(question) {
        Ok(())
    } else {
        Err(CommandError::new("not confirmed; nothing was changed"))
    }
}

fn item(path: &RawPath, bytes: Option<u64>) -> AskItem {
    let shown = path.display();
    let separator = match path.flavor() {
        lumen_domain::PathFlavor::Unix => '/',
        lumen_domain::PathFlavor::Windows => '\\',
    };
    let (folder, name) = match shown.rsplit_once(separator) {
        Some(("", name)) => (separator.to_string(), name.to_owned()),
        Some((folder, name)) => (folder.to_owned(), name.to_owned()),
        None => (String::new(), shown.clone()),
    };
    AskItem {
        path: shown,
        name,
        folder,
        bytes: bytes.map(|b| b.to_string()),
    }
}

/// Confirms executing stored plan `id`, after checking `hash` and its age the
/// way executing it will. An uninstall preview (`uninstall`) names the app; a
/// clean-up lists every item the person chose and the policy's proposals.
pub(crate) fn plan(
    db: &Path,
    id: PlanId,
    hash: &str,
    uninstall: bool,
    prompt: &Prompt,
) -> Result<(), CommandError> {
    lumen_app::refuse_privileged().map_err(CommandError::new)?;
    let store = IndexStore::new(open_index(db).map_err(CommandError::new)?);
    let stored = lumen_app::quarantine::checked_plan(&store, id, hash, SystemClock.now())
        .map_err(CommandError::new)?;
    if lumen_application::uninstall::any_uninstall(&stored.decisions) != uninstall {
        return Err(CommandError::new(
            "that proposal cannot run through this action; nothing was changed",
        ));
    }
    let items = stored.plan.items();
    let shown = |planned: &lumen_domain::PlannedItem| {
        item(&planned.path, Some(planned.reclaim.reclaimable_now.get()))
    };
    if uninstall {
        let items = items.iter().map(shown).collect();
        return ask(prompt, &Ask::Uninstall { items });
    }
    let (chosen, proposed): (Vec<_>, Vec<_>) = items
        .iter()
        .partition(|planned| planned.chosen_by == Chooser::Person);
    if chosen.len() > MAX_CHOSEN {
        return Err(CommandError::new(
            "too many chosen items to confirm at once; nothing was changed",
        ));
    }
    let total: u64 = items
        .iter()
        .map(|planned| planned.reclaim.reclaimable_now.get())
        .fold(0, u64::saturating_add);
    ask(
        prompt,
        &Ask::Move {
            count: items.len(),
            total_bytes: total.to_string(),
            proposed: Listed::of(proposed.into_iter().map(shown).collect()),
            chosen: chosen.into_iter().map(shown).collect(),
        },
    )
}

/// Confirms removing held item `operation` for good, naming where it was.
pub(crate) fn finalize(
    db: &Path,
    operation: OperationId,
    prompt: &Prompt,
) -> Result<(), CommandError> {
    let held = lumen_app::held::get(db, operation).map_err(CommandError::new)?;
    let original = held
        .original
        .ok_or_else(|| CommandError::new("where the item was is unknown; nothing was changed"))?;
    let path = lumen_app::raw_path(&original).map_err(CommandError::new)?;
    ask(
        prompt,
        &Ask::Finalize {
            item: item(&path, held.reclaim.map(lumen_domain::ByteCount::get)),
            chosen: held.chosen_by == Some(Chooser::Person),
        },
    )
}

/// Confirms running the knowledge base's `command` for `targets`.
pub(crate) fn tool(
    command: &lumen_app::tools::ToolCommand,
    targets: &[RawPath],
    prompt: &Prompt,
) -> Result<(), CommandError> {
    let line = std::iter::once(command.program.as_str())
        .chain(command.args.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join(" ");
    ask(
        prompt,
        &Ask::Tool {
            tool: command.tool.clone(),
            command: line,
            targets: Listed::of(targets.iter().map(|path| item(path, None)).collect()),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(text: &str) -> RawPath {
        RawPath::from_unix_bytes(text.as_bytes().to_vec()).unwrap_or_else(|e| unreachable!("{e}"))
    }

    #[test]
    fn only_a_confirmation_lets_the_command_go_on() {
        let question = Ask::Uninstall { items: Vec::new() };
        assert!(ask(&|_: &Ask| true, &question).is_ok());
        assert!(ask(&|_: &Ask| false, &question).is_err());
    }

    #[test]
    fn a_unix_name_keeps_its_backslash() {
        let shown = item(&raw("/Users/ana/Documents\\cache"), None);
        assert_eq!(
            (shown.folder.as_str(), shown.name.as_str()),
            ("/Users/ana", "Documents\\cache")
        );
        let top = item(&raw("/top"), None);
        assert_eq!((top.folder.as_str(), top.name.as_str()), ("/", "top"));
    }

    #[test]
    fn long_lists_say_how_many_more() {
        let items = (0..LISTED + 8)
            .map(|i| item(&raw(&format!("/p/{i}")), None))
            .collect();
        let listed = Listed::of(items);
        assert_eq!((listed.items.len(), listed.more), (LISTED, 8));
    }

    #[test]
    fn an_answer_reaches_the_open_question_once() {
        let pending = Pending::default();
        pending.answer(true);
        assert_eq!(pending.ask(), None, "no question, nothing happens");
        let (sender, answer) = mpsc::channel();
        let question = Ask::Uninstall { items: Vec::new() };
        *pending.open.lock().unwrap_or_else(PoisonError::into_inner) =
            Some((question.clone(), sender));
        assert_eq!(pending.ask(), Some(question));
        pending.answer(true);
        pending.answer(false);
        assert_eq!(answer.recv().ok(), Some(true));
        assert!(
            answer.try_recv().is_err(),
            "the first answer is the only one"
        );
        assert_eq!(pending.ask(), None);
    }

    #[test]
    fn a_tool_question_names_the_command_and_its_targets() -> Result<(), serde_json::Error> {
        let seen = Arc::new(Mutex::new(None));
        let into = Arc::clone(&seen);
        let record = move |question: &Ask| {
            *into.lock().unwrap_or_else(PoisonError::into_inner) = Some(question.clone());
            false
        };
        let command = lumen_app::tools::ToolCommand {
            entry: "pnpm-store".into(),
            tool: "pnpm".into(),
            program: "pnpm".into(),
            args: vec!["store".into(), "prune".into()],
            removes_data: false,
        };
        assert!(tool(&command, &[raw("/Users/a/Library/pnpm")], &record).is_err());
        let question = seen.lock().unwrap_or_else(PoisonError::into_inner).clone();
        assert_eq!(
            serde_json::to_value(question)?,
            serde_json::json!({
                "kind": "tool",
                "tool": "pnpm",
                "command": "pnpm store prune",
                "targets": {
                    "items": [{
                        "path": "/Users/a/Library/pnpm",
                        "name": "pnpm",
                        "folder": "/Users/a/Library",
                        "bytes": null
                    }],
                    "more": 0
                }
            })
        );
        Ok(())
    }
}
