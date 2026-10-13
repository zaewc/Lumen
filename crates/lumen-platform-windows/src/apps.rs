//! Mapping Windows `Uninstall` registry entries to applications (roadmap 6.10).
//!
//! Pure and platform-independent; the registry reading is Windows-only.

use std::collections::BTreeSet;

use lumen_domain::{AppId, AppIdScheme, Application, RawPath, SourceName, UntrustedText};

/// One `...\CurrentVersion\Uninstall\<key>` entry, as read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UninstallEntry {
    /// The subkey name (an MSI product code for Windows Installer packages).
    pub key: String,
    /// `DisplayName`.
    pub display_name: Option<String>,
    /// `DisplayVersion`.
    pub version: Option<String>,
    /// `Publisher`.
    pub publisher: Option<String>,
    /// `InstallLocation`.
    pub install_location: Option<String>,
}

fn present(value: Option<&String>) -> Option<UntrustedText> {
    value
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .map(UntrustedText::new)
}

/// An absolute install directory (`X:\...` or `\\server\share\...`), with the
/// quotes and trailing separators installers often add removed.
fn location(raw: &str) -> Option<RawPath> {
    let trimmed = raw.trim().trim_matches('"').trim_end_matches(['\\', '/']);
    let bytes = trimmed.as_bytes();
    let drive = bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
    let share = trimmed.starts_with(r"\\") && trimmed.len() > 2;
    if !(drive || share) {
        return None;
    }
    // `C:` alone means "the current directory on C:", not the drive root.
    let path = if trimmed.len() == 2 {
        format!("{trimmed}\\")
    } else {
        trimmed.to_owned()
    };
    let wide: Vec<u16> = path.encode_utf16().collect();
    RawPath::from_windows_wide(&wide).ok()
}

/// The application an entry describes. `None` without a display name: such
/// entries are not shown as installed programs (updates, partial installs).
pub fn to_application(entry: &UninstallEntry, source: &SourceName) -> Option<Application> {
    let name = present(entry.display_name.as_ref())?;
    if entry.key.is_empty() {
        return None;
    }
    Some(Application {
        ids: BTreeSet::from([AppId {
            scheme: AppIdScheme::UninstallKey,
            value: UntrustedText::new(entry.key.clone()),
        }]),
        name: Some(name),
        version: present(entry.version.as_ref()),
        vendor: present(entry.publisher.as_ref()),
        signature: None,
        install_locations: entry
            .install_location
            .as_deref()
            .and_then(location)
            .into_iter()
            .collect(),
        sources: BTreeSet::from([source.clone()]),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> SourceName {
        SourceName::new("windows.uninstall-keys").unwrap_or_else(|_| unreachable!())
    }

    fn entry(key: &str, name: Option<&str>, location: Option<&str>) -> UninstallEntry {
        UninstallEntry {
            key: key.into(),
            display_name: name.map(Into::into),
            version: Some(" 1.2.3 ".into()),
            publisher: Some("Example Corp".into()),
            install_location: location.map(Into::into),
        }
    }

    #[test]
    fn entries_with_names_become_applications() {
        let app = to_application(
            &entry(
                "{11111111-2222-3333-4444-555555555555}",
                Some("Example"),
                Some("\"C:\\Program Files\\Example\\\""),
            ),
            &source(),
        );
        let app = app.unwrap_or_else(|| unreachable!());
        assert_eq!(
            app.name.map(|n| n.raw().to_owned()).as_deref(),
            Some("Example")
        );
        assert_eq!(
            app.version.map(|v| v.raw().to_owned()).as_deref(),
            Some("1.2.3")
        );
        assert_eq!(
            app.vendor.map(|v| v.raw().to_owned()).as_deref(),
            Some("Example Corp")
        );
        let expected: Vec<u16> = r"C:\Program Files\Example".encode_utf16().collect();
        assert_eq!(app.install_locations[0].to_windows_wide(), Some(expected));
        assert!(
            app.ids
                .iter()
                .all(|id| id.scheme == AppIdScheme::UninstallKey)
        );
    }

    #[test]
    fn nameless_and_keyless_entries_are_skipped() {
        assert!(to_application(&entry("KB123", None, None), &source()).is_none());
        assert!(to_application(&entry("KB123", Some("   "), None), &source()).is_none());
        assert!(to_application(&entry("", Some("Example"), None), &source()).is_none());
    }

    #[test]
    fn only_absolute_install_locations_are_kept() {
        for (raw, kept) in [
            (r"D:\Apps\Tool", Some(r"D:\Apps\Tool")),
            (r"\\server\share\app\", Some(r"\\server\share\app")),
            ("C:", Some(r"C:\")),
            (r"relative\dir", None),
            ("", None),
            ("%ProgramFiles%\\X", None),
        ] {
            let app = to_application(&entry("k", Some("n"), Some(raw)), &source());
            let found = app.and_then(|a| {
                a.install_locations
                    .first()
                    .and_then(RawPath::to_windows_wide)
            });
            let expected = kept.map(|k| k.encode_utf16().collect::<Vec<u16>>());
            assert_eq!(found, expected, "{raw:?}");
        }
    }
}
