//! Disk Cleanup handler declarations (roadmap 6.13, research note 04).
//!
//! Data-driven Disk Cleanup handlers (`VolumeCaches\<name>` with a `Folder`
//! value) are each vendor's own statement that the files in those folders may be
//! deleted. This module parses them and turns matching scan entries into
//! `VendorDeclaredCache` evidence. Handlers that are opaque COM code (no
//! `Folder`) declare nothing Lumen can check and are skipped. No ownership
//! relationship is created: a handler name does not identify an application.
//!
//! Parsing and matching are pure and tested everywhere; the registry reading is
//! Windows-only.

use lumen_domain::{
    Basis, Evidence, Fact, FilesystemEntry, Provenance, RawPath, SourceName, Subject, Timestamp,
    UntrustedText,
};

/// The evidence source name.
pub const SOURCE: &str = "windows.disk-cleanup-handlers";

/// One data-driven handler's declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheDeclaration {
    /// The handler's key name (e.g. `Temporary Files`).
    pub handler: String,
    /// The folders it declares, with environment variables expanded.
    pub folders: Vec<RawPath>,
    /// `FileList` patterns (e.g. `*.tmp`), as declared.
    pub file_patterns: Vec<String>,
}

/// Expands `%NAME%` references with `lookup`. `None` if a reference is
/// unterminated or unknown: an unexpanded path must never be guessed at.
pub fn expand(raw: &str, lookup: &dyn Fn(&str) -> Option<String>) -> Option<String> {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let end = after.find('%')?;
        let name = &after[..end];
        if name.is_empty() {
            return None;
        }
        out.push_str(&lookup(name)?);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Some(out)
}

/// An absolute Windows directory path, with trailing separators removed.
fn absolute_dir(path: &str) -> Option<RawPath> {
    let trimmed = path.trim().trim_end_matches(['\\', '/']);
    let bytes = trimmed.as_bytes();
    let drive =
        bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'\\';
    let share = trimmed.starts_with(r"\\") && trimmed.len() > 2;
    if !(drive || share) {
        return None;
    }
    let wide: Vec<u16> = trimmed.encode_utf16().collect();
    RawPath::from_windows_wide(&wide).ok()
}

/// Parses one handler's `Folder` value (folders separated by `|`) and `FileList`
/// value (patterns separated by `|`). `None` if no folder survives expansion.
pub fn declaration(
    handler: &str,
    folder: &str,
    file_list: Option<&str>,
    lookup: &dyn Fn(&str) -> Option<String>,
) -> Option<CacheDeclaration> {
    let folders: Vec<RawPath> = folder
        .split('|')
        .filter_map(|f| expand(f, lookup))
        .filter_map(|f| absolute_dir(&f))
        .collect();
    if folders.is_empty() {
        return None;
    }
    let file_patterns = file_list
        .unwrap_or_default()
        .split('|')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .collect();
    Some(CacheDeclaration {
        handler: handler.to_owned(),
        folders,
        file_patterns,
    })
}

/// Windows path equality: case-insensitive, as NTFS lookups are by default.
fn same_path(a: &RawPath, b: &RawPath) -> bool {
    match (a.to_windows_wide(), b.to_windows_wide()) {
        (Some(a), Some(b)) => {
            String::from_utf16_lossy(&a).to_lowercase()
                == String::from_utf16_lossy(&b).to_lowercase()
        }
        _ => false,
    }
}

/// `VendorDeclaredCache` evidence for every entry that is a declared folder.
pub fn declared_caches(
    declarations: &[CacheDeclaration],
    entries: &[FilesystemEntry],
    observed_at: Timestamp,
) -> Vec<Evidence> {
    let Ok(source) = SourceName::new(SOURCE) else {
        return Vec::new();
    };
    let mut evidence = Vec::new();
    for entry in entries {
        for declaration in declarations {
            if declaration
                .folders
                .iter()
                .any(|f| same_path(f, &entry.path))
            {
                evidence.push(Evidence {
                    subject: Subject::File {
                        identity: entry.identity().clone(),
                    },
                    fact: Fact::VendorDeclaredCache {
                        declared_by: UntrustedText::new(format!(
                            "Disk Cleanup handler: {}",
                            declaration.handler
                        )),
                    },
                    basis: Basis::Observed,
                    provenance: Provenance {
                        source: source.clone(),
                        observed_at,
                    },
                });
            }
        }
    }
    evidence
}

#[cfg(windows)]
pub use registry_source::read_declarations;

#[cfg(windows)]
mod registry_source {
    use lumen_domain::SourceStatus;
    use windows_sys::Win32::System::Registry::{HKEY_LOCAL_MACHINE, KEY_WOW64_64KEY};

    use super::{CacheDeclaration, declaration};
    use crate::registry::{self, Missing};

    const VOLUME_CACHES: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\VolumeCaches";

    /// Reads every data-driven handler, expanding variables from this process's
    /// environment, and reports how completely the key was read.
    pub fn read_declarations() -> (Vec<CacheDeclaration>, SourceStatus) {
        let key = match registry::open(HKEY_LOCAL_MACHINE, VOLUME_CACHES, KEY_WOW64_64KEY) {
            Ok(key) => key,
            Err(Missing::NotFound) => return (Vec::new(), SourceStatus::Complete),
            Err(Missing::Unreadable) => return (Vec::new(), SourceStatus::Unavailable),
        };
        let (handlers, mut complete) = match key.subkeys() {
            Ok(names) => (names, true),
            Err(partial) => (partial, false),
        };
        let lookup = |name: &str| std::env::var(name).ok();
        let mut declarations = Vec::new();
        for handler in handlers {
            // Handlers without a `Folder` value are opaque COM code.
            let Some(folder) = key.string(&handler, "Folder") else {
                continue;
            };
            match declaration(
                &handler,
                &folder,
                key.string(&handler, "FileList").as_deref(),
                &lookup,
            ) {
                Some(found) => declarations.push(found),
                // Declared but not resolvable: unknown, not absent.
                None => complete = false,
            }
        }
        let status = if complete {
            SourceStatus::Complete
        } else {
            SourceStatus::Partial
        };
        (declarations, status)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn reads_the_systems_data_driven_handlers() {
            let (declarations, _status) = read_declarations();
            // Every Windows ships data-driven handlers (e.g. temporary setup and
            // log files) with absolute folders.
            assert_ne!(declarations.len(), 0);
            assert!(declarations.iter().all(|d| !d.folders.is_empty()));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use lumen_domain::{
        AccessState, ByteCount, EntryKind, EntryTimes, FileId, FileIdentity, Protection, SizeFacts,
        SizeFlags, VolumeId,
    };

    use super::*;

    fn env(name: &str) -> Option<String> {
        match name.to_ascii_uppercase().as_str() {
            "SYSTEMROOT" | "WINDIR" => Some(r"C:\Windows".into()),
            "LOCALAPPDATA" => Some(r"C:\Users\ana\AppData\Local".into()),
            _ => None,
        }
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    #[test]
    fn expansion_never_guesses() {
        assert_eq!(
            expand(r"%SystemRoot%\Temp", &env).as_deref(),
            Some(r"C:\Windows\Temp")
        );
        assert_eq!(expand(r"C:\Plain", &env).as_deref(), Some(r"C:\Plain"));
        assert_eq!(expand(r"%Unknown%\x", &env), None);
        assert_eq!(expand(r"%SystemRoot\x", &env), None, "unterminated");
        assert_eq!(expand(r"100%%x", &env), None, "empty name");
    }

    #[test]
    fn declarations_keep_only_absolute_expanded_folders() {
        let parsed = declaration(
            "Temporary Setup Files",
            r"%windir%\msdownld.tmp\|%LOCALAPPDATA%\Temp|%Unknown%\x|relative\dir",
            Some("*.tmp|*.log| "),
            &env,
        );
        let parsed = parsed.unwrap_or_else(|| unreachable!());
        let folders: Vec<_> = parsed
            .folders
            .iter()
            .filter_map(RawPath::to_windows_wide)
            .collect();
        assert_eq!(
            folders,
            [
                wide(r"C:\Windows\msdownld.tmp"),
                wide(r"C:\Users\ana\AppData\Local\Temp")
            ]
        );
        assert_eq!(parsed.file_patterns, ["*.tmp", "*.log"]);
        assert_eq!(declaration("X", r"%Unknown%", None, &env), None);
    }

    fn entry(path: &str, ino: u128) -> FilesystemEntry {
        FilesystemEntry {
            path: RawPath::from_windows_wide(&wide(path)).unwrap_or_else(|_| unreachable!()),
            kind: EntryKind::Directory,
            size: SizeFacts {
                identity: FileIdentity {
                    volume: VolumeId::new("vsn:1").unwrap_or_else(|_| unreachable!()),
                    file: FileId::new(ino),
                },
                logical: ByteCount::ZERO,
                allocated: ByteCount::ZERO,
                private: None,
                clone_id: None,
                link_count: NonZeroU32::MIN,
                flags: SizeFlags::default(),
            },
            times: EntryTimes::default(),
            protection: Protection::default(),
            access: AccessState::NotScanned,
        }
    }

    #[test]
    fn matching_folders_become_vendor_declared_cache_evidence() {
        let declarations = [
            declaration("Temporary Files", r"%LOCALAPPDATA%\Temp", None, &env)
                .unwrap_or_else(|| unreachable!()),
        ];
        let found = declared_caches(
            &declarations,
            &[
                entry(r"c:\users\ana\appdata\local\TEMP", 1),
                entry(r"C:\Users\ana\AppData\Local\Temp\sub", 2),
                entry(r"C:\Users\ana\AppData\Local", 3),
            ],
            Timestamp::UNIX_EPOCH,
        );
        assert_eq!(
            found.len(),
            1,
            "only the declared folder itself, case-insensitively"
        );
        assert!(matches!(
            &found[0].fact,
            Fact::VendorDeclaredCache { declared_by } if declared_by.raw().contains("Temporary Files")
        ));
        assert_eq!(found[0].provenance.source.as_str(), SOURCE);
    }
}
