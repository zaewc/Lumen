//! Installer package receipts (#806, ADR-0060): what `pkgutil --pkgs` lists,
//! read from the receipt property lists in `/var/db/receipts` with the
//! system's own parser. Nothing is run and nothing is written.
//!
//! A receipt records that a package was installed and under which prefix. It
//! does not prove the files are still there, and forgetting it
//! (`pkgutil --forget`) removes nothing, so a receipt is never a way to remove
//! software.

use std::collections::BTreeMap;
use std::io::ErrorKind;
use std::path::PathBuf;

use lumen_application::CancelToken;
use lumen_application::ports::{InventoryReport, InventorySource};
use lumen_domain::{Package, PackageManager, RawPath, SourceName, SourceStatus, UntrustedText};
use objc2_core_foundation::{CFDictionary, CFString};

use crate::cf;

/// Inventory source name.
const SOURCE: &str = "macos.package-receipts";

/// The receipts of a receipt directory, as packages.
#[derive(Debug, Clone)]
pub struct ReceiptInventory {
    dir: PathBuf,
}

impl ReceiptInventory {
    /// The system's receipt directory, `/var/db/receipts`.
    #[must_use]
    pub fn new() -> Self {
        Self::in_dir(PathBuf::from("/var/db/receipts"))
    }

    /// Another directory of receipts, for tests.
    #[must_use]
    pub const fn in_dir(dir: PathBuf) -> Self {
        Self { dir }
    }
}

impl Default for ReceiptInventory {
    fn default() -> Self {
        Self::new()
    }
}

fn source() -> SourceName {
    SourceName::new(SOURCE).unwrap_or_else(|_| unreachable!("a valid literal"))
}

/// The package one receipt describes; `None` if it is not a property list or
/// names no package.
fn receipt(bytes: &[u8]) -> Option<Package> {
    let list = cf::property_list(bytes)?;
    let dict = list.downcast_ref::<CFDictionary>()?;
    let text = |key: &str| {
        cf::get::<CFString>(dict, &CFString::from_str(key))
            .map(ToString::to_string)
            .filter(|value| !value.trim().is_empty())
    };
    let id = text("PackageIdentifier")?;
    // The prefix is relative to the volume the package went to: `/`,
    // `Applications`, `Library/…`.
    let install_root = text("InstallPrefixPath").and_then(|prefix| {
        let absolute = format!("/{}", prefix.trim_start_matches('/'));
        RawPath::from_unix_bytes(absolute.into_bytes()).ok()
    });
    Some(Package {
        manager: PackageManager::MacInstaller,
        id: UntrustedText::new(id),
        version: text("PackageVersion").map(UntrustedText::new),
        install_root,
        // The macOS installer has no "part of the base system" mark.
        required: false,
        // A product can install several component packages; a receipt does
        // not say which one was asked for.
        explicit: None,
    })
}

impl InventorySource for ReceiptInventory {
    fn name(&self) -> SourceName {
        source()
    }

    fn collect(&self, cancel: &CancelToken) -> InventoryReport {
        let report = |status, packages| InventoryReport {
            source: source(),
            status,
            applications: Vec::new(),
            services: Vec::new(),
            packages,
        };
        let entries = match std::fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == ErrorKind::NotFound => {
                return report(SourceStatus::Unavailable, Vec::new());
            }
            Err(_) => return report(SourceStatus::Partial, Vec::new()),
        };
        // By identifier, so the order does not depend on the directory's.
        let mut found = BTreeMap::new();
        let mut complete = true;
        for entry in entries {
            if cancel.is_cancelled() {
                complete = false;
                break;
            }
            let Ok(entry) = entry else {
                complete = false;
                continue;
            };
            let path = entry.path();
            // Each receipt has a `.bom` (its file list) beside its `.plist`.
            if path.extension().is_none_or(|ext| ext != "plist") {
                continue;
            }
            match std::fs::read(&path).ok().as_deref().and_then(receipt) {
                Some(package) => {
                    found.insert(package.id.raw().to_owned(), package);
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
    use super::*;

    const RECEIPT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>PackageIdentifier</key><string>com.example.tool</string>
  <key>PackageVersion</key><string>2.6.3</string>
  <key>InstallPrefixPath</key><string>Applications</string>
</dict></plist>"#;

    #[test]
    fn receipts_become_packages_and_broken_ones_make_the_list_partial() -> std::io::Result<()> {
        let dir = tempfile::tempdir()?;
        std::fs::write(dir.path().join("com.example.tool.plist"), RECEIPT)?;
        std::fs::write(dir.path().join("com.example.tool.bom"), b"not a plist")?;
        let inventory = ReceiptInventory::in_dir(dir.path().to_path_buf());
        let report = inventory.collect(&CancelToken::new());
        assert_eq!(report.status, SourceStatus::Complete);
        assert_eq!(report.packages.len(), 1);
        let package = &report.packages[0];
        assert_eq!(package.manager, PackageManager::MacInstaller);
        assert_eq!(package.id.raw(), "com.example.tool");
        assert_eq!(
            package.version.as_ref().map(UntrustedText::raw),
            Some("2.6.3")
        );
        assert_eq!(
            package.install_root.as_ref().map(RawPath::as_bytes),
            Some(&b"/Applications"[..])
        );
        assert!(!package.required && package.explicit.is_none());

        std::fs::write(dir.path().join("broken.plist"), b"not a plist")?;
        let report = inventory.collect(&CancelToken::new());
        assert_eq!(report.status, SourceStatus::Partial);
        assert_eq!(report.packages.len(), 1);
        Ok(())
    }

    #[test]
    fn a_missing_directory_is_unavailable_not_empty() {
        let inventory = ReceiptInventory::in_dir(PathBuf::from("/nonexistent/lumen-receipts"));
        let report = inventory.collect(&CancelToken::new());
        assert_eq!(report.status, SourceStatus::Unavailable);
    }

    #[test]
    fn the_systems_receipts_are_read() {
        let report = ReceiptInventory::new().collect(&CancelToken::new());
        assert_eq!(report.source.as_str(), SOURCE);
        assert_ne!(report.status, SourceStatus::Unavailable);
        assert!(
            !report.packages.is_empty(),
            "macOS records installer receipts"
        );
    }
}
