//! MSIX/AppX packages (#806, ADR-0060): what `PackageManager` lists for the
//! current user, which needs no administrator rights. Nothing is changed.
//!
//! A package is identified by its package family name, as `Get-AppxPackage`
//! and `Remove-AppxPackage` do. Packages signed as part of Windows
//! (`PackageSignatureKind::System`) run the system: they are `required`.
//! Framework packages (runtimes other packages depend on) are listed as not
//! installed on request.

use lumen_domain::{Package, PackageManager, RawPath, UntrustedText};

/// One package as `PackageManager` describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MsixPackage {
    /// `PackageId.FamilyName`.
    pub family: String,
    /// `PackageId.Version`: major, minor, build, revision.
    pub version: [u16; 4],
    /// `Package.IsFramework`.
    pub framework: bool,
    /// `Package.SignatureKind` is `System`.
    pub system: bool,
    /// `Package.InstalledPath`, if it could be read.
    pub installed_path: Option<String>,
}

/// The domain package for `package`; `None` without a family name.
#[must_use]
pub fn to_package(package: &MsixPackage) -> Option<Package> {
    let family = package.family.trim();
    if family.is_empty() {
        return None;
    }
    let [major, minor, build, revision] = package.version;
    let install_root = package
        .installed_path
        .as_deref()
        .filter(|p| !p.is_empty())
        .and_then(|p| RawPath::from_windows_wide(&p.encode_utf16().collect::<Vec<_>>()).ok());
    Some(Package {
        manager: PackageManager::Msix,
        id: UntrustedText::new(family),
        version: Some(UntrustedText::new(format!(
            "{major}.{minor}.{build}.{revision}"
        ))),
        install_root,
        required: package.system,
        explicit: Some(!package.framework),
        relations: None,
    })
}

#[cfg(windows)]
pub use inventory::MsixInventory;

#[cfg(windows)]
mod inventory {
    use std::collections::BTreeMap;

    use lumen_application::CancelToken;
    use lumen_application::ports::{InventoryReport, InventorySource};
    use lumen_domain::{SourceName, SourceStatus};
    use windows::ApplicationModel::{Package as WinPackage, PackageSignatureKind};
    use windows::Management::Deployment::PackageManager;
    use windows::core::HSTRING;

    use super::{MsixPackage, to_package};

    const SOURCE: &str = "windows.msix";

    /// The current user's MSIX/AppX packages, source `windows.msix`.
    #[derive(Debug, Clone, Copy, Default)]
    pub struct MsixInventory;

    fn describe(package: &WinPackage) -> Option<MsixPackage> {
        let id = package.Id().ok()?;
        let version = id.Version().ok()?;
        Some(MsixPackage {
            family: id.FamilyName().ok()?.to_string_lossy(),
            version: [
                version.Major,
                version.Minor,
                version.Build,
                version.Revision,
            ],
            framework: package.IsFramework().ok()?,
            system: package.SignatureKind().ok()? == PackageSignatureKind::System,
            installed_path: package.InstalledPath().ok().map(|p| p.to_string_lossy()),
        })
    }

    impl InventorySource for MsixInventory {
        fn name(&self) -> SourceName {
            SourceName::new(SOURCE).unwrap_or_else(|_| unreachable!("a valid literal"))
        }

        fn collect(&self, cancel: &CancelToken) -> InventoryReport {
            let report = |status, packages| InventoryReport {
                source: self.name(),
                status,
                applications: Vec::new(),
                services: Vec::new(),
                packages,
            };
            // An empty security identifier means the current user.
            let Ok(packages) = PackageManager::new()
                .and_then(|manager| manager.FindPackagesByUserSecurityId(&HSTRING::new()))
            else {
                return report(SourceStatus::Unavailable, Vec::new());
            };
            // By family: several architectures or versions of one family are
            // one package to the user, as `Remove-AppxPackage` treats them.
            let mut found = BTreeMap::new();
            let mut complete = true;
            for package in packages {
                if cancel.is_cancelled() {
                    complete = false;
                    break;
                }
                match describe(&package).as_ref().and_then(to_package) {
                    Some(package) => {
                        found.entry(package.id.raw().to_owned()).or_insert(package);
                    }
                    None => complete = false,
                }
            }
            let status = if complete {
                SourceStatus::Complete
            } else {
                SourceStatus::Partial
            };
            report(status, found.into_values().collect())
        }
    }

    #[cfg(test)]
    mod tests {
        use lumen_application::CancelToken;
        use lumen_application::ports::InventorySource as _;
        use lumen_domain::SourceStatus;

        use super::MsixInventory;

        #[test]
        fn the_current_users_packages_can_be_listed() {
            let report = MsixInventory.collect(&CancelToken::new());
            assert_eq!(report.source.as_str(), "windows.msix");
            assert_ne!(report.status, SourceStatus::Unavailable);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn package(family: &str, framework: bool, system: bool) -> MsixPackage {
        MsixPackage {
            family: family.into(),
            version: [1, 2, 3, 0],
            framework,
            system,
            installed_path: Some(
                r"C:\Program Files\WindowsApps\Example_1.2.3.0_x64__8wekyb3d8bbwe".into(),
            ),
        }
    }

    #[test]
    fn packages_keep_their_family_version_and_role() {
        let app = to_package(&package("Example.App_8wekyb3d8bbwe", false, false))
            .unwrap_or_else(|| unreachable!());
        assert_eq!(app.manager, PackageManager::Msix);
        assert_eq!(app.id.raw(), "Example.App_8wekyb3d8bbwe");
        assert_eq!(
            app.version.as_ref().map(UntrustedText::raw),
            Some("1.2.3.0")
        );
        assert_eq!(app.explicit, Some(true));
        assert!(!app.required);
        assert!(app.install_root.is_some());

        let framework = to_package(&package(
            "Microsoft.VCLibs.140.00_8wekyb3d8bbwe",
            true,
            false,
        ))
        .unwrap_or_else(|| unreachable!());
        assert_eq!(framework.explicit, Some(false));

        let system = to_package(&package(
            "Microsoft.Windows.ShellExperienceHost_cw5n1h2txyewy",
            false,
            true,
        ))
        .unwrap_or_else(|| unreachable!());
        assert!(
            system.required,
            "system-signed packages are never removable"
        );
    }

    #[test]
    fn a_package_without_a_family_is_skipped() {
        assert_eq!(to_package(&package("  ", false, false)), None);
        let mut no_path = package("Example_1", false, false);
        no_path.installed_path = None;
        assert!(to_package(&no_path).is_some_and(|p| p.install_root.is_none()));
    }
}
