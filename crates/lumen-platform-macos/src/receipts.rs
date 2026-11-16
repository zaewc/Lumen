//! Installer package receipts (#806, ADR-0060): what `pkgutil --pkgs` lists,
//! read from the receipt property lists in `/var/db/receipts` with the
//! system's own parser. Nothing is run and nothing is written.
//!
//! A receipt records that a package was installed and under which prefix. It
//! does not prove the files are still there, and forgetting it
//! (`pkgutil --forget`) removes nothing, so a receipt is never a way to remove
//! software.
//!
//! Each receipt's bill of materials (the `.bom` beside it, read by
//! [`crate::bom`]) gives the application bundles the package installed: those
//! whose `Contents/Info.plist` it records as a file (ADR-0064). A bill that is
//! missing or unreadable leaves the package's relations unknown.

use std::collections::BTreeMap;
use std::io::{ErrorKind, Read as _};
use std::path::{Path, PathBuf};

use lumen_application::CancelToken;
use lumen_application::ports::{InventoryReport, InventorySource};
use lumen_domain::{
    Package, PackageManager, PackageRelation, PackageRelationKind, RawPath, RelationMethod,
    SourceName, SourceStatus, UntrustedText,
};
use objc2_core_foundation::{CFDictionary, CFString};

use crate::cf;

/// Inventory source name.
const SOURCE: &str = "macos.package-receipts";

/// The largest bill of materials read (Xcode's is about 40 MiB).
const MAX_BOM: u64 = 256 * 1024 * 1024;

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

/// The package one receipt describes, with the absolute prefix its bill of
/// materials is relative to (an empty prefix is the volume's root); `None` if
/// it is not a property list or names no package.
fn receipt(bytes: &[u8]) -> Option<(Package, Option<String>)> {
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
    let prefix = cf::get::<CFString>(dict, &CFString::from_str("InstallPrefixPath"))
        .map(|prefix| format!("/{}", prefix.to_string().trim_start_matches('/')));
    let package = Package {
        manager: PackageManager::MacInstaller,
        id: UntrustedText::new(id),
        version: text("PackageVersion").map(UntrustedText::new),
        install_root,
        // The macOS installer has no "part of the base system" mark.
        required: false,
        // A product can install several component packages; a receipt does
        // not say which one was asked for.
        explicit: None,
        relations: None,
    };
    Some((package, prefix))
}

/// The application bundles a receipt's bill of materials records, placed
/// under the receipt's prefix; `None` if there is no prefix, or the bill is
/// missing, too large or unreadable.
fn installed(bom: &Path, prefix: Option<&str>) -> Option<Vec<PackageRelation>> {
    let prefix = prefix?;
    let mut bytes = Vec::new();
    std::fs::File::open(bom)
        .ok()?
        .take(MAX_BOM + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_BOM {
        return None;
    }
    crate::bom::bundles(&bytes)
        .ok()?
        .into_iter()
        .filter_map(|relative| {
            let mut path = prefix.as_bytes().to_vec();
            if relative.is_empty() {
                // The prefix itself, which must then be a bundle.
                let trimmed = prefix.trim_end_matches('/');
                if !trimmed.to_ascii_lowercase().ends_with(".app") {
                    return None;
                }
                path = trimmed.as_bytes().to_vec();
            } else {
                if !path.ends_with(b"/") {
                    path.push(b'/');
                }
                path.extend_from_slice(&relative);
            }
            Some(RawPath::from_unix_bytes(path))
        })
        .map(|location| {
            Some(PackageRelation {
                kind: PackageRelationKind::Installed {
                    location: location.ok()?,
                },
                method: RelationMethod::ReceiptFileList,
            })
        })
        .collect()
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
                Some((mut package, prefix)) => {
                    package.relations = installed(&path.with_extension("bom"), prefix.as_deref());
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
        // Its bill of materials is unreadable, so what it installed is unknown.
        assert_eq!(package.relations, None);

        std::fs::write(dir.path().join("broken.plist"), b"not a plist")?;
        let report = inventory.collect(&CancelToken::new());
        assert_eq!(report.status, SourceStatus::Partial);
        assert_eq!(report.packages.len(), 1);
        Ok(())
    }

    #[test]
    fn the_bill_of_materials_gives_the_bundles_installed_under_the_prefix() -> std::io::Result<()> {
        let dir = tempfile::tempdir()?;
        std::fs::write(dir.path().join("com.example.tool.plist"), RECEIPT)?;
        let bom = crate::bom::tests::build(&[
            (1, 0, ".", 2),
            (2, 1, "Tool.app", 2),
            (3, 2, "Contents", 2),
            (4, 3, "Info.plist", 1),
        ]);
        std::fs::write(dir.path().join("com.example.tool.bom"), bom)?;
        let report =
            ReceiptInventory::in_dir(dir.path().to_path_buf()).collect(&CancelToken::new());
        let relations = report.packages[0].relations.clone().unwrap_or_default();
        assert_eq!(
            relations,
            [PackageRelation {
                kind: PackageRelationKind::Installed {
                    location: RawPath::from_unix_bytes(b"/Applications/Tool.app".to_vec())
                        .unwrap_or_else(|_| unreachable!()),
                },
                method: RelationMethod::ReceiptFileList,
            }]
        );
        Ok(())
    }

    #[test]
    fn an_empty_prefix_is_the_root_and_a_bundle_prefix_is_the_bundle() -> std::io::Result<()> {
        let dir = tempfile::tempdir()?;
        let top = crate::bom::tests::build(&[
            (1, 0, ".", 2),
            (2, 1, "Contents", 2),
            (3, 2, "Info.plist", 1),
        ]);
        let nested = crate::bom::tests::build(&[
            (1, 0, ".", 2),
            (2, 1, "Applications", 2),
            (3, 2, "Tool.app", 2),
            (4, 3, "Contents", 2),
            (5, 4, "Info.plist", 1),
        ]);
        for (id, prefix, bom) in [
            ("root", "", &nested),
            ("bundle", "Applications/Tool.app", &top),
            ("folder", "Library/Tool", &top),
        ] {
            let plist = RECEIPT.replace("com.example.tool", id).replace(
                "<string>Applications</string>",
                &format!("<string>{prefix}</string>"),
            );
            std::fs::write(dir.path().join(format!("{id}.plist")), plist)?;
            std::fs::write(dir.path().join(format!("{id}.bom")), bom)?;
        }
        let report =
            ReceiptInventory::in_dir(dir.path().to_path_buf()).collect(&CancelToken::new());
        let locations: Vec<(String, Vec<String>)> = report
            .packages
            .iter()
            .map(|p| {
                let found = p.relations.iter().flatten().filter_map(|r| match &r.kind {
                    PackageRelationKind::Installed { location } => Some(location.display()),
                    _ => None,
                });
                (p.id.raw().to_owned(), found.collect())
            })
            .collect();
        assert_eq!(
            locations,
            [
                (
                    "bundle".to_owned(),
                    vec!["/Applications/Tool.app".to_owned()]
                ),
                // A prefix that is not a bundle cannot be the bundle.
                ("folder".to_owned(), vec![]),
                ("root".to_owned(), vec!["/Applications/Tool.app".to_owned()]),
            ]
        );
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
        assert!(
            report.packages.iter().any(|p| p.relations.is_some()),
            "the receipts' bills of materials are read"
        );
    }
}
