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

use std::sync::Arc;

use lumen_application::ports::SecretStore;
use tauri::webview::WebviewWindowBuilder;
use tauri::{Builder, Manager as _, Runtime, Url, WebviewUrl};

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
            commands::set_ai_review,
            commands::set_ai_key,
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
            app.manage(commands::Core {
                db: data.join("index.db"),
                secrets,
                builtin,
            });
            WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
                .title("Lumen")
                // Sized for the single-column layout; narrow windows still work.
                .inner_size(900.0, 780.0)
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
    app(Builder::default(), store, lumen_app::ai::builtin_key()).run(context())
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
