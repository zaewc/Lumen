//! The Windows installed-applications inventory from `Uninstall` keys
//! (roadmap 6.10): machine-wide in both registry views, and per user.

use lumen_application::CancelToken;
use lumen_application::ports::{InventoryReport, InventorySource};
use lumen_domain::{SourceName, SourceStatus};
use windows_sys::Win32::System::Registry::{
    HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_WOW64_32KEY, KEY_WOW64_64KEY,
};

use crate::apps::{UninstallEntry, to_application};
use crate::registry::{self, Missing};

const SOURCE: &str = "windows.uninstall-keys";
const UNINSTALL: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";

/// Lists installed applications from the registry.
#[derive(Debug, Clone, Copy, Default)]
pub struct InstalledApps;

fn source() -> SourceName {
    SourceName::new(SOURCE).unwrap_or_else(|_| unreachable!("valid source name"))
}

impl InventorySource for InstalledApps {
    fn name(&self) -> SourceName {
        source()
    }

    fn collect(&self, cancel: &CancelToken) -> InventoryReport {
        let views = [
            (HKEY_LOCAL_MACHINE, KEY_WOW64_64KEY),
            (HKEY_LOCAL_MACHINE, KEY_WOW64_32KEY),
            (HKEY_CURRENT_USER, 0),
        ];
        let mut applications = Vec::new();
        let mut complete = true;
        'views: for (root, view) in views {
            let key = match registry::open(root, UNINSTALL, view) {
                Ok(key) => key,
                Err(Missing::NotFound) => continue,
                Err(Missing::Unreadable) => {
                    complete = false;
                    continue;
                }
            };
            let names = key.subkeys().unwrap_or_else(|partial| {
                complete = false;
                partial
            });
            for name in names {
                if cancel.is_cancelled() {
                    complete = false;
                    break 'views;
                }
                let entry = UninstallEntry {
                    display_name: key.string(&name, "DisplayName"),
                    version: key.string(&name, "DisplayVersion"),
                    publisher: key.string(&name, "Publisher"),
                    install_location: key.string(&name, "InstallLocation"),
                    uninstall_string: key.string(&name, "UninstallString"),
                    quiet_uninstall_string: key.string(&name, "QuietUninstallString"),
                    windows_installer: key.dword(&name, "WindowsInstaller"),
                    system_component: key.dword(&name, "SystemComponent"),
                    no_remove: key.dword(&name, "NoRemove"),
                    parent_key_name: key.string(&name, "ParentKeyName"),
                    key: name,
                };
                applications.extend(to_application(&entry, &source()));
            }
        }
        InventoryReport {
            source: source(),
            status: if complete {
                SourceStatus::Complete
            } else {
                SourceStatus::Partial
            },
            applications,
            services: Vec::new(),
            packages: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_installed_applications_from_the_registry() {
        let report = InstalledApps.collect(&CancelToken::new());
        assert_eq!(report.source.as_str(), SOURCE);
        assert!(
            !report.applications.is_empty(),
            "a Windows runner has installed programs"
        );
        assert!(report.applications.iter().all(|a| a.name.is_some()));
        // Runners carry Windows Installer packages (the Visual C++ runtimes);
        // reading their `WindowsInstaller` flag exercises the DWORD reader.
        assert!(
            report
                .applications
                .iter()
                .any(|a| a.uninstall.as_ref().is_some_and(|u| u.windows_installer)),
            "no Windows Installer entry was read"
        );
        let cancel = CancelToken::new();
        cancel.cancel();
        assert_eq!(InstalledApps.collect(&cancel).status, SourceStatus::Partial);
    }
}
