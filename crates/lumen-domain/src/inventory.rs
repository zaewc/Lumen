//! Installed software, running processes, services and packages.
//!
//! These entities anchor the evidence graph (ADR-0013): an application *creates* a
//! cache, a process *holds* a file open, a launch agent *launches* an executable.
//! Every string here comes from vendor-controlled metadata and is therefore an
//! [`UntrustedText`].

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{FileIdentity, RawPath, SourceName, Timestamp, UntrustedText};

/// How an application identifier is defined.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AppIdScheme {
    /// Apple bundle identifier (`com.example.App`).
    BundleId,
    /// Android package name.
    AndroidPackage,
    /// MSIX/AppX package family name.
    MsixFamily,
    /// Windows Installer product code.
    MsiProductCode,
    /// Windows `Uninstall` registry key name.
    UninstallKey,
}

/// One identifier of an application under a given scheme.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, schemars::JsonSchema,
)]
pub struct AppId {
    /// Identifier scheme.
    pub scheme: AppIdScheme,
    /// Identifier value as reported by the platform.
    pub value: UntrustedText,
}

/// Result of verifying a code signature with the platform's own APIs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SignatureCheck {
    /// The platform validated the signature.
    Valid,
    /// A signature exists but failed validation.
    Invalid,
    /// No signature.
    Unsigned,
    /// Not checked or not checkable.
    Unknown,
}

/// Who signed a binary, and whether the platform verified it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CodeSignature {
    /// Apple Team ID, Authenticode publisher, or Android signing certificate
    /// digest, as reported.
    pub signer: Option<UntrustedText>,
    /// Verification result.
    pub check: SignatureCheck,
}

impl CodeSignature {
    /// The signer, only if the platform verified the signature. An unverified
    /// signer name is a claim, not evidence of identity.
    pub fn verified_signer(&self) -> Option<&UntrustedText> {
        match self.check {
            SignatureCheck::Valid => self.signer.as_ref(),
            SignatureCheck::Invalid | SignatureCheck::Unsigned | SignatureCheck::Unknown => None,
        }
    }
}

/// An installed application, merged from one or more inventory sources.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Application {
    /// Identifiers under every known scheme.
    pub ids: BTreeSet<AppId>,
    /// Display name.
    pub name: Option<UntrustedText>,
    /// Version string.
    pub version: Option<UntrustedText>,
    /// Vendor or publisher string.
    pub vendor: Option<UntrustedText>,
    /// Code signature of the main executable.
    pub signature: Option<CodeSignature>,
    /// Where the application is installed (bundle path, install directory).
    pub install_locations: Vec<RawPath>,
    /// Inventory sources that reported this application.
    pub sources: BTreeSet<SourceName>,
}

impl Application {
    /// Whether the application carries the given identifier.
    pub fn has_id(&self, id: &AppId) -> bool {
        self.ids.contains(id)
    }
}

/// A running process.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Process {
    /// Process ID. Only meaningful together with `started_at`, because PIDs are
    /// reused.
    pub pid: u32,
    /// Start time, used with `pid` to tell process instances apart.
    pub started_at: Option<Timestamp>,
    /// Executable path, if readable.
    pub executable: Option<RawPath>,
    /// Executable identity, if readable.
    pub executable_identity: Option<FileIdentity>,
    /// Owning application, if attributable.
    pub application: Option<AppId>,
}

/// Kind of background or startup component.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ServiceKind {
    /// macOS launchd agent (per user).
    LaunchAgent,
    /// macOS launchd daemon (system).
    LaunchDaemon,
    /// macOS login item or background item (Background Task Management).
    LoginItem,
    /// Windows service.
    WindowsService,
    /// Windows driver.
    Driver,
    /// Windows Task Scheduler task.
    ScheduledTask,
    /// Windows `Run`/`RunOnce` value or Startup-folder entry.
    StartupEntry,
}

/// Whether a service runs for one user or for the whole system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ServiceScope {
    /// Runs as the user.
    User,
    /// Runs as root, `SYSTEM` or a service account.
    System,
}

/// A background, startup or scheduled component.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Service {
    /// Component kind.
    pub kind: ServiceKind,
    /// User or system scope.
    pub scope: ServiceScope,
    /// Label, service name or task name.
    pub label: UntrustedText,
    /// File that defines the component (plist, task XML), if any.
    pub definition: Option<RawPath>,
    /// Program the component runs, if resolvable.
    pub program: Option<RawPath>,
    /// Whether the component is enabled, if known.
    pub enabled: Option<bool>,
    /// Signature of the program, if checked.
    pub signature: Option<CodeSignature>,
}

/// Installer or package manager that recorded a package.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum PackageManager {
    /// macOS Installer receipt (`pkgutil`).
    MacInstaller,
    /// Homebrew.
    Homebrew,
    /// Windows Installer (MSI).
    Msi,
    /// MSIX/AppX.
    Msix,
    /// winget.
    Winget,
    /// Android package manager.
    Android,
}

/// A package recorded by an installer or package manager.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Package {
    /// Recording installer or manager.
    pub manager: PackageManager,
    /// Package identifier.
    pub id: UntrustedText,
    /// Version string.
    pub version: Option<UntrustedText>,
    /// Install root, if the manager reports one.
    pub install_root: Option<RawPath>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bundle(id: &str) -> AppId {
        AppId {
            scheme: AppIdScheme::BundleId,
            value: UntrustedText::new(id),
        }
    }

    #[test]
    fn only_verified_signatures_name_a_signer() {
        let signer = Some(UntrustedText::new("ABCDE12345"));
        for (check, expected) in [
            (SignatureCheck::Valid, true),
            (SignatureCheck::Invalid, false),
            (SignatureCheck::Unsigned, false),
            (SignatureCheck::Unknown, false),
        ] {
            let sig = CodeSignature {
                signer: signer.clone(),
                check,
            };
            assert_eq!(sig.verified_signer().is_some(), expected, "{check:?}");
        }
    }

    #[test]
    fn application_id_lookup_is_scheme_aware() {
        let app = Application {
            ids: BTreeSet::from([bundle("com.example.App")]),
            name: Some(UntrustedText::new("Example")),
            version: None,
            vendor: None,
            signature: None,
            install_locations: vec![],
            sources: BTreeSet::new(),
        };
        assert!(app.has_id(&bundle("com.example.App")));
        assert!(!app.has_id(&AppId {
            scheme: AppIdScheme::AndroidPackage,
            value: UntrustedText::new("com.example.App")
        }));
    }

    #[test]
    fn service_labels_display_escaped() {
        let service = Service {
            kind: ServiceKind::LaunchAgent,
            scope: ServiceScope::User,
            label: UntrustedText::new("com.update\u{202E}revres"),
            definition: None,
            program: None,
            enabled: Some(true),
            signature: None,
        };
        assert_eq!(service.label.display(), "com.update⟦U+202E⟧revres");
    }

    #[test]
    fn entities_round_trip_through_json() -> Result<(), Box<dyn std::error::Error>> {
        let process = Process {
            pid: 4242,
            started_at: Some("2026-10-07T09:00:00Z".parse()?),
            executable: Some(RawPath::from_unix_bytes(
                b"/Applications/Example.app/Contents/MacOS/Example".to_vec(),
            )?),
            executable_identity: None,
            application: Some(bundle("com.example.App")),
        };
        let json = serde_json::to_string(&process)?;
        assert_eq!(serde_json::from_str::<Process>(&json)?, process);

        let package = Package {
            manager: PackageManager::MacInstaller,
            id: UntrustedText::new("com.example.pkg"),
            version: Some(UntrustedText::new("1.2")),
            install_root: None,
        };
        let json = serde_json::to_string(&package)?;
        assert_eq!(serde_json::from_str::<Package>(&json)?, package);
        Ok(())
    }
}
