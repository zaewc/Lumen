//! Lumen's desktop app; everything is in the library (`lumen_desktop`).

// No console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() -> Result<(), tauri::Error> {
    lumen_desktop::run()
}
