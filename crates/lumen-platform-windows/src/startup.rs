//! Windows `Run`/`RunOnce` startup entries (roadmap 6.11, part 1).
//!
//! The command-line parsing is pure and tested everywhere; the registry reading
//! is Windows-only.

use lumen_domain::{RawPath, Service, ServiceKind, ServiceScope, UntrustedText};

/// Extensions Windows treats as directly runnable when it resolves an unquoted
/// command line with spaces.
const RUNNABLE: &[&str] = &[".exe", ".com", ".bat", ".cmd"];

fn absolute(path: &str) -> Option<RawPath> {
    let bytes = path.as_bytes();
    let drive =
        bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'\\';
    let share = path.starts_with(r"\\") && path.len() > 2;
    if !(drive || share) {
        return None;
    }
    let wide: Vec<u16> = path.encode_utf16().collect();
    RawPath::from_windows_wide(&wide).ok()
}

/// The program a startup command runs, if it can be named without guessing:
/// a quoted path, or an unquoted one up to its first runnable extension, or a
/// single token. Only absolute paths count; environment variables and bare
/// names (resolved through `PATH`) give `None`.
pub fn program_of(command: &str) -> Option<RawPath> {
    let command = command.trim();
    if let Some(rest) = command.strip_prefix('"') {
        return absolute(rest.split('"').next()?);
    }
    let lower = command.to_ascii_lowercase();
    let end = RUNNABLE
        .iter()
        .filter_map(|ext| lower.find(ext).map(|at| at + ext.len()))
        .filter(|&end| command[end..].is_empty() || command[end..].starts_with(' '))
        .min();
    match end {
        Some(end) => absolute(&command[..end]),
        None if !command.contains(' ') => absolute(command),
        None => None,
    }
}

/// A startup entry: value `name` with command line `command` in a `Run` key.
/// `enabled` is unknown: the Startup Apps on/off state lives in the
/// undocumented `StartupApproved` format.
pub fn startup_entry(name: &str, command: &str) -> Service {
    Service {
        kind: ServiceKind::StartupEntry,
        // Run entries start in each user's session, as that user.
        scope: ServiceScope::User,
        label: UntrustedText::new(name),
        definition: None,
        program: program_of(command),
        enabled: None,
        signature: None,
    }
}

#[cfg(windows)]
pub use registry_source::StartupEntries;

#[cfg(windows)]
mod registry_source {
    use lumen_application::CancelToken;
    use lumen_application::ports::{InventoryReport, InventorySource};
    use lumen_domain::{SourceName, SourceStatus};
    use windows_sys::Win32::System::Registry::{
        HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_WOW64_32KEY, KEY_WOW64_64KEY,
    };

    use super::startup_entry;
    use crate::registry::{self, Missing};

    const SOURCE: &str = "windows.run-keys";
    const KEYS: [&str; 2] = [
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run",
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\RunOnce",
    ];

    /// Lists `Run` and `RunOnce` entries, machine-wide in both registry views
    /// and for the current user.
    #[derive(Debug, Clone, Copy, Default)]
    pub struct StartupEntries;

    fn source() -> SourceName {
        SourceName::new(SOURCE).unwrap_or_else(|_| unreachable!("valid source name"))
    }

    impl InventorySource for StartupEntries {
        fn name(&self) -> SourceName {
            source()
        }

        fn collect(&self, cancel: &CancelToken) -> InventoryReport {
            let views = [
                (HKEY_LOCAL_MACHINE, KEY_WOW64_64KEY),
                (HKEY_LOCAL_MACHINE, KEY_WOW64_32KEY),
                (HKEY_CURRENT_USER, 0),
            ];
            let mut services = Vec::new();
            let mut complete = true;
            'keys: for (root, view) in views {
                for path in KEYS {
                    if cancel.is_cancelled() {
                        complete = false;
                        break 'keys;
                    }
                    let key = match registry::open(root, path, view) {
                        Ok(key) => key,
                        Err(Missing::NotFound) => continue,
                        Err(Missing::Unreadable) => {
                            complete = false;
                            continue;
                        }
                    };
                    let names = key.values().unwrap_or_else(|partial| {
                        complete = false;
                        partial
                    });
                    for name in names {
                        match key.string("", &name) {
                            Some(command) => services.push(startup_entry(&name, &command)),
                            // Not a string: not a command line Windows runs.
                            None => complete = false,
                        }
                    }
                }
            }
            InventoryReport {
                source: source(),
                status: if complete {
                    SourceStatus::Complete
                } else {
                    SourceStatus::Partial
                },
                applications: Vec::new(),
                services,
                packages: Vec::new(),
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn reads_run_keys_and_honours_cancellation() {
            let report = StartupEntries.collect(&CancelToken::new());
            assert_eq!(report.source.as_str(), SOURCE);
            assert!(
                report
                    .services
                    .iter()
                    .filter_map(|s| s.program.as_ref())
                    .all(|p| p.flavor() == lumen_domain::PathFlavor::Windows)
            );
            let cancel = CancelToken::new();
            cancel.cancel();
            assert_eq!(
                StartupEntries.collect(&cancel).status,
                SourceStatus::Partial
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn program(command: &str) -> Option<String> {
        program_of(command)
            .and_then(|p| p.to_windows_wide())
            .map(|w| String::from_utf16_lossy(&w))
    }

    #[test]
    fn quoted_programs_are_exact() {
        assert_eq!(
            program(r#""C:\Program Files\App\app.exe" --minimized"#).as_deref(),
            Some(r"C:\Program Files\App\app.exe")
        );
        assert_eq!(
            program(r#""C:\Tools\run me.cmd""#).as_deref(),
            Some(r"C:\Tools\run me.cmd")
        );
    }

    #[test]
    fn unquoted_programs_end_at_their_runnable_extension() {
        assert_eq!(
            program(r"C:\Program Files\App\app.EXE -background").as_deref(),
            Some(r"C:\Program Files\App\app.EXE")
        );
        assert_eq!(
            program(r"C:\Tools\agent.exe").as_deref(),
            Some(r"C:\Tools\agent.exe")
        );
        assert_eq!(
            program(r"\\server\share\tool.bat /q").as_deref(),
            Some(r"\\server\share\tool.bat")
        );
    }

    #[test]
    fn ambiguous_or_relative_commands_name_no_program() {
        for command in [
            "",
            "agent.exe",
            r"%ProgramFiles%\App\app.exe",
            r"rundll32.exe C:\x.dll,Entry",
            r"C:\Program Files\App\launcher --flag",
            r#""relative\app.exe""#,
        ] {
            assert_eq!(program(command), None, "{command:?}");
        }
        // An unterminated quote still names the path up to the end.
        assert_eq!(
            program(r#""C:\unterminated"#).as_deref(),
            Some(r"C:\unterminated")
        );
    }

    #[test]
    fn entries_are_user_scope_with_unknown_enabled_state() {
        let entry = startup_entry("Updater", r#""C:\App\updater.exe" /silent"#);
        assert_eq!(entry.kind, ServiceKind::StartupEntry);
        assert_eq!(entry.scope, ServiceScope::User);
        assert_eq!(entry.enabled, None);
        assert_eq!(entry.label.raw(), "Updater");
        assert!(entry.program.is_some());
    }
}
