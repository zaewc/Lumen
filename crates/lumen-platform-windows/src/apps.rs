//! Mapping Windows `Uninstall` registry entries to applications (roadmap 6.10).
//!
//! Pure and platform-independent; the registry reading is Windows-only.

use std::collections::BTreeSet;

use lumen_domain::{
    AppId, AppIdScheme, Application, RawPath, SourceName, UninstallRecord, UntrustedText,
};

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
    /// `UninstallString`.
    pub uninstall_string: Option<String>,
    /// `QuietUninstallString`.
    pub quiet_uninstall_string: Option<String>,
    /// `WindowsInstaller` (a `REG_DWORD`).
    pub windows_installer: Option<u32>,
    /// `SystemComponent` (a `REG_DWORD`).
    pub system_component: Option<u32>,
    /// `NoRemove` (a `REG_DWORD`).
    pub no_remove: Option<u32>,
    /// `ParentKeyName`.
    pub parent_key_name: Option<String>,
}

/// Whether `key` has the form of a Windows Installer product code:
/// `{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}` in hexadecimal.
fn is_product_code(key: &str) -> bool {
    let Some(inner) = key.strip_prefix('{').and_then(|k| k.strip_suffix('}')) else {
        return false;
    };
    let groups: Vec<&str> = inner.split('-').collect();
    groups.iter().map(|g| g.len()).eq([8, 4, 4, 4, 12])
        && groups
            .iter()
            .all(|g| g.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// What the entry records about removing its application. A flag that is
/// missing reads as unset, as Windows reads it.
fn uninstall_record(entry: &UninstallEntry) -> UninstallRecord {
    UninstallRecord {
        windows_installer: entry.windows_installer == Some(1),
        command: present(entry.uninstall_string.as_ref()),
        quiet_command: present(entry.quiet_uninstall_string.as_ref()),
        hidden: entry.system_component == Some(1),
        no_remove: entry.no_remove == Some(1),
        parent: present(entry.parent_key_name.as_ref()),
    }
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
    let uninstall = uninstall_record(entry);
    let mut ids = BTreeSet::from([AppId {
        scheme: AppIdScheme::UninstallKey,
        value: UntrustedText::new(entry.key.clone()),
    }]);
    // A Windows Installer entry is keyed by its product code (ADR-0060).
    if uninstall.windows_installer && is_product_code(&entry.key) {
        ids.insert(AppId {
            scheme: AppIdScheme::MsiProductCode,
            value: UntrustedText::new(entry.key.clone()),
        });
    }
    Some(Application {
        ids,
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
        uninstall: Some(uninstall),
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
            ..UninstallEntry::default()
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
    fn uninstall_metadata_is_kept_as_untrusted_text() {
        let code = "{11111111-2222-3333-4444-555555555555}";
        let command = format!("MsiExec.exe /X{code}");
        let mut msi = entry(code, Some("Example"), None);
        msi.windows_installer = Some(1);
        msi.uninstall_string = Some(command.clone());
        msi.system_component = Some(0);
        let app = to_application(&msi, &source()).unwrap_or_else(|| unreachable!());
        assert!(app.has_id(&AppId {
            scheme: AppIdScheme::MsiProductCode,
            value: UntrustedText::new(code),
        }));
        let record = app.uninstall.unwrap_or_else(|| unreachable!());
        assert!(record.windows_installer && !record.hidden && !record.no_remove);
        assert_eq!(record.command.map(|c| c.raw().to_owned()), Some(command));
        assert_eq!(record.quiet_command, None);

        let mut exe = entry("Example", Some("Example"), None);
        exe.uninstall_string = Some(r#""C:\Program Files\Example\unins000.exe""#.into());
        exe.quiet_uninstall_string = Some("   ".into());
        exe.system_component = Some(1);
        exe.no_remove = Some(1);
        exe.parent_key_name = Some("Parent".into());
        let app = to_application(&exe, &source()).unwrap_or_else(|| unreachable!());
        assert!(
            app.ids
                .iter()
                .all(|id| id.scheme == AppIdScheme::UninstallKey)
        );
        let record = app.uninstall.unwrap_or_else(|| unreachable!());
        assert!(!record.windows_installer && record.hidden && record.no_remove);
        assert_eq!(record.quiet_command, None);
        assert_eq!(
            record.parent.map(|p| p.raw().to_owned()).as_deref(),
            Some("Parent")
        );
    }

    #[test]
    fn only_guid_keys_of_installer_entries_are_product_codes() {
        for (key, expected) in [
            ("{11111111-2222-3333-4444-555555555555}", true),
            ("{abcdefab-cdef-abcd-efab-cdefabcdefab}", true),
            ("{1111-2222}", false),
            ("11111111-2222-3333-4444-555555555555", false),
            ("{1111111G-2222-3333-4444-555555555555}", false),
            ("Example", false),
        ] {
            assert_eq!(is_product_code(key), expected, "{key}");
        }
        // An entry not marked as Windows Installer is never keyed by product code.
        let key = "{11111111-2222-3333-4444-555555555555}";
        let app = to_application(&entry(key, Some("n"), None), &source());
        assert!(app.is_some_and(|a| a.ids.len() == 1));
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
