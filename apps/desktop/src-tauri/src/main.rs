//! Lumen's desktop app (ADR-0004): the Tauri host, with the core running in
//! this process and the UI in the webview.
//!
//! The webview is untrusted. It reaches the core only through the commands in
//! [`commands`], each its own permission granted to the one window; the
//! isolation pattern checks every IPC message against that list before the
//! host sees it; the CSP is strict; and the window refuses to navigate away
//! from the app.

// No console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;

use tauri::webview::WebviewWindowBuilder;
use tauri::{Manager as _, Url, WebviewUrl};

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

fn main() -> Result<(), tauri::Error> {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::scan,
            commands::assess,
            commands::plan,
            commands::execute,
            commands::held,
            commands::restore,
            commands::finalize,
        ])
        .setup(|app| {
            let data = app.path().app_local_data_dir()?;
            std::fs::create_dir_all(&data)?;
            app.manage(commands::Core {
                db: data.join("index.db"),
            });
            WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
                .title("Lumen")
                .inner_size(1100.0, 760.0)
                .on_navigation(allowed)
                .build()?;
            Ok(())
        })
        .run(tauri::generate_context!())
}

#[cfg(test)]
mod tests {
    use super::*;

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
