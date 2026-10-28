//! In-app updates (ADR-0048): whether a newer Lumen is published, and
//! installing it when the user asks.
//!
//! The updater plugin fetches `latest.json` from Lumen's GitHub Releases over
//! HTTPS and installs only an archive whose signature checks out against the
//! public key in `tauri.conf.json`, signed for the version it announces
//! (`requireSignedVersion`). The page never reaches the plugin: these two
//! commands are its only way in, and installing is always the user's request.

use lumen_api::views::UpdateView;
use tauri::{AppHandle, Runtime};
use tauri_plugin_updater::UpdaterExt as _;

use crate::commands::CommandError;

/// Whether this copy can replace itself: everywhere but a Linux install from
/// a `.deb` package (an `AppImage` says where it runs from in `APPIMAGE`).
fn installable() -> bool {
    !cfg!(target_os = "linux") || std::env::var_os("APPIMAGE").is_some()
}

/// The running version and the newer one on offer, if any. Reads only.
#[tauri::command]
pub(crate) async fn check_update<R: Runtime>(
    app: AppHandle<R>,
) -> Result<UpdateView, CommandError> {
    let current = app.package_info().version.to_string();
    let update = app
        .updater()
        .map_err(CommandError::new)?
        .check()
        .await
        .map_err(|_| CommandError::new("Lumen could not check for updates"))?;
    Ok(UpdateView {
        current,
        available: update.map(|update| update.version),
        installable: installable(),
    })
}

/// Downloads the newer version, checks its signature, installs it and starts
/// it. Does nothing (and says so) when there is none or this copy cannot
/// replace itself.
#[tauri::command]
pub(crate) async fn install_update<R: Runtime>(app: AppHandle<R>) -> Result<(), CommandError> {
    if !installable() {
        return Err(CommandError::new(
            "this copy was installed from a package; update it from the download page",
        ));
    }
    let update = app
        .updater()
        .map_err(CommandError::new)?
        .check()
        .await
        .map_err(|_| CommandError::new("Lumen could not check for updates"))?
        .ok_or_else(|| CommandError::new("Lumen is up to date"))?;
    update
        .download_and_install(|_chunk, _total| {}, || {})
        .await
        .map_err(|_| CommandError::new("the update could not be downloaded and verified"))?;
    app.restart()
}
