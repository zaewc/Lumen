//! Lumen's desktop app (ADR-0004): the Tauri host, with the core running in
//! this process and the UI in the webview.
//!
//! The webview is untrusted. It reaches the core only through the commands in
//! [`commands`], each its own permission granted to the one window; the
//! isolation pattern checks every IPC message against that list before the
//! host sees it; the CSP is strict; and the window refuses to navigate away
//! from the app.
//!
//! The binary (`main.rs`) only calls [`run`].

mod commands;
#[cfg(all(test, target_os = "macos"))]
mod e2e;
mod update;

use std::sync::Arc;

use lumen_application::ports::SecretStore;
use tauri::webview::WebviewWindowBuilder;
use tauri::{AppHandle, Builder, Manager as _, RunEvent, Runtime, Url, WebviewUrl, WebviewWindow};

/// Whether the main window may show `url`: only the app itself, never the
/// web (ADR-0004).
fn allowed(url: &Url) -> bool {
    match url.scheme() {
        "tauri" => true,
        "http" | "https" => matches!(
            (url.host_str(), url.port()),
            (Some("tauri.localhost"), None) | (Some("localhost"), Some(5173))
        ),
        _ => false,
    }
}

/// Dispatches the fixed DOM event `name` into `view`, the way native window
/// events reach the page without a new IPC capability (ADR-0048).
fn tell_page<R: Runtime>(view: Option<WebviewWindow<R>>, name: &str) {
    if let Some(view) = view
        && let Err(error) = view.eval(format!("window.dispatchEvent(new Event('{name}'))"))
    {
        tracing::warn!(%error, name, "Could not notify the interface");
    }
}

/// Whether quitting now would cut a clean-up or Quarantine action short
/// (UX-3). Then quitting waits, and the page says why; Lumen does not quit
/// later by itself.
fn hold_quit<R: Runtime>(app: &AppHandle<R>) -> bool {
    let moving = app
        .try_state::<commands::Core>()
        .is_some_and(|core| core.activity.is_moving());
    if moving {
        tell_page(app.get_webview_window("main"), "lumen:quit-held");
    }
    moving
}

/// Holds closing the window and exit requests while items move (see
/// [`hold_quit`]). Exit requests come from closing the last window and from
/// `AppHandle::exit`, which Lumen's own Quit menu item uses on macOS.
fn hold_quit_events<R: Runtime>(app: &AppHandle<R>, event: &RunEvent) {
    match event {
        RunEvent::WindowEvent {
            event: tauri::WindowEvent::CloseRequested { api, .. },
            ..
        } if hold_quit(app) => api.prevent_close(),
        RunEvent::ExitRequested { api, .. } if hold_quit(app) => api.prevent_exit(),
        _ => {}
    }
}

/// macOS's standard Quit item (⌘Q) sends `terminate:`, which Tao reports
/// only after the fact (`applicationWillTerminate`, then `RunEvent::Exit`),
/// so nothing can hold it. Lumen's own item exits through
/// [`AppHandle::exit`] instead, which raises `RunEvent::ExitRequested`.
#[cfg(target_os = "macos")]
const QUIT: &str = "lumen-quit";

/// Tauri's default macOS menu with its Quit item swapped for Lumen's own.
#[cfg(target_os = "macos")]
fn menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<tauri::menu::Menu<R>> {
    use tauri::menu::{Menu, MenuItem, MenuItemKind};

    let menu = Menu::default(app)?;
    if let Some(MenuItemKind::Submenu(first)) = menu.items()?.into_iter().next() {
        let items = first.items()?;
        if let Some(MenuItemKind::Predefined(_)) = items.last() {
            first.remove_at(items.len() - 1)?;
        }
        let name = &app.package_info().name;
        first.append(&MenuItem::with_id(
            app,
            QUIT,
            format!("Quit {name}"),
            true,
            Some("CmdOrCtrl+Q"),
        )?)?;
    }
    Ok(menu)
}

/// The app on `builder`'s runtime: its commands, its core state and its one
/// window, with judge keys in `secrets` (`None`: AI review can use only
/// `builtin`) and Lumen's built-in Jeb key in `builtin` (ADR-0042; `None`
/// outside release builds). The real app runs it on Wry with the OS secret
/// store and [`lumen_app::ai::builtin_key`]; the end-to-end test on the mock
/// runtime with an in-memory store and no built-in key.
pub fn app<R: Runtime>(
    builder: Builder<R>,
    secrets: Option<Arc<dyn SecretStore>>,
    builtin: Option<lumen_app::ai::ApiKey>,
) -> Builder<R> {
    builder
        // In-app updates (ADR-0048); reached only through Lumen's own
        // `check_update` and `install_update`, never from the page.
        .plugin(tauri_plugin_updater::Builder::new().build())
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::Focused(true)) {
                tell_page(window.get_webview_window(window.label()), "lumen:window-focus");
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::scan,
            commands::assess,
            commands::plan,
            commands::execute,
            commands::held,
            commands::restore,
            commands::finalize,
            commands::ai_settings,
            commands::disk,
            commands::open_full_disk_access,
            commands::set_theme,
            commands::set_ai_review,
            commands::set_ai_key,
            commands::run_tool_command,
            commands::tool_storage,
            update::check_update,
            update::install_update,
        ])
        .setup(move |app| {
            let data = app.path().app_local_data_dir()?;
            std::fs::create_dir_all(&data)?;
            // Local, redacted logs in the app's data folder (ADR-0022); kept
            // alive by the app's state. A failure leaves telemetry off.
            let logs = data.join("logs");
            let config = lumen_telemetry::Config {
                log_dir: Some(&logs),
                ..lumen_telemetry::Config::default()
            };
            if let Ok(telemetry) = lumen_telemetry::start(&config) {
                app.manage(telemetry);
            }
            let db = data.join("index.db");
            let settled = Arc::new(std::sync::Once::new());
            // Interrupted cleanups are settled off the main thread (UX-3).
            let startup = (db.clone(), Arc::clone(&settled));
            drop(tauri::async_runtime::spawn_blocking(move || {
                commands::settle_once(&startup.0, &startup.1);
            }));
            app.manage(commands::Core {
                db,
                secrets,
                builtin,
                tool_entries: std::sync::Mutex::default(),
                tool_running: Arc::default(),
                settled,
                activity: Arc::default(),
            });
            WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
                .title("Lumen")
                // Wide enough for the sidebar and the clean-up list's columns
                // and filters on one line; narrower windows still work, and
                // the page never scrolls sideways.
                .inner_size(1120.0, 800.0)
                .min_inner_size(420.0, 560.0)
                .on_navigation(allowed)
                .build()?;
            Ok(())
        })
}

/// Runs the desktop app.
///
/// # Errors
///
/// A [`tauri::Error`] if the app cannot start.
pub fn run() -> Result<(), tauri::Error> {
    let store = lumen_app::ai::os_store().map(Arc::from);
    let builder = app(Builder::default(), store, lumen_app::ai::builtin_key());
    #[cfg(target_os = "macos")]
    let builder = builder.menu(menu).on_menu_event(|app, event| {
        if event.id() == QUIT {
            app.exit(0);
        }
    });
    builder
        .build(context())?
        .run(|app, event| hold_quit_events(app, &event));
    Ok(())
}

/// The app's configuration, assets and capabilities, embedded at build time.
/// Generated in one place: the macro may appear only once per crate.
pub fn context<R: Runtime>() -> tauri::Context<R> {
    tauri::generate_context!()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    /// The quoted words between `start` and the next `end` in `text`.
    fn quoted(text: &str, start: &str, end: &str) -> BTreeSet<String> {
        let from = text
            .find(start)
            .unwrap_or_else(|| unreachable!("`{start}` not found"));
        let body = &text[from + start.len()..];
        let body = &body[..body.find(end).unwrap_or(body.len())];
        body.split('"')
            .skip(1)
            .step_by(2)
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn every_command_is_registered_permitted_and_let_through() {
        let built = quoted(include_str!("../build.rs"), "let commands = &[", "];");
        let isolated = quoted(include_str!("../../isolation/index.js"), "new Set([", "]);");
        let permitted: BTreeSet<String> = quoted(
            include_str!("../capabilities/main.json"),
            "\"permissions\": [",
            "]",
        )
        .iter()
        .filter_map(|p| p.strip_prefix("allow-"))
        .map(|c| c.replace('-', "_"))
        .collect();
        let handled: BTreeSet<String> = include_str!("lib.rs")
            .split("commands::")
            .flat_map(|part| part.split("update::"))
            .skip(1)
            .filter_map(|rest| rest.split(',').next())
            .map(|name| name.trim().to_owned())
            .filter(|name| name.chars().all(|c| c.is_ascii_lowercase() || c == '_'))
            .collect();
        assert!(built.len() >= 12, "{built:?}");
        // A command missing from the isolation frame's list is dropped there,
        // and the window's call never answers (0.1.0 shipped that way).
        assert_eq!(isolated, built, "apps/desktop/isolation/index.js");
        assert_eq!(permitted, built, "capabilities/main.json");
        assert_eq!(handled, built, "generate_handler! in lib.rs");
    }

    fn url(text: &str) -> Url {
        Url::parse(text).unwrap_or_else(|e| unreachable!("{e}"))
    }

    #[test]
    fn the_window_stays_on_the_app() {
        assert!(allowed(&url("tauri://localhost/")));
        assert!(allowed(&url("http://tauri.localhost/index.html")));
        assert!(allowed(&url("http://localhost:5173/")));
        assert!(!allowed(&url("https://example.com/")));
        assert!(!allowed(&url("http://localhost:8080/")));
        assert!(!allowed(&url("file:///etc/passwd")));
    }
}
