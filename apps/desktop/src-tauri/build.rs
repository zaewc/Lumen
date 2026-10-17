//! Generates one permission per app command (`allow-<command>`), so the
//! capability grants exactly these and nothing else (ADR-0004).

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let commands = &[
        "scan", "assess", "plan", "execute", "held", "restore", "finalize",
    ];
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(commands)),
    )?;
    Ok(())
}
