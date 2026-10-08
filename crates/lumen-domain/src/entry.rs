//! Volumes and filesystem entries as observed by scanners.

use serde::{Deserialize, Serialize};

use crate::{AccessState, FileIdentity, RawPath, SizeFacts, Timestamp, VolumeId};

/// Filesystem format of a volume.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum FilesystemKind {
    /// Apple File System.
    Apfs,
    /// HFS+.
    Hfs,
    /// NTFS.
    Ntfs,
    /// `ReFS` (including Windows Dev Drive).
    Refs,
    /// FAT12/16/32.
    Fat,
    /// exFAT.
    ExFat,
    /// ext4.
    Ext4,
    /// Btrfs.
    Btrfs,
    /// XFS.
    Xfs,
    /// F2FS (common on Android).
    F2fs,
    /// SMB/CIFS network share.
    Smb,
    /// NFS network share.
    Nfs,
    /// Anything else, including FUSE and provider-backed filesystems.
    Other,
}

/// Whether a volume supports a feature, as far as the platform reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Support {
    /// Supported.
    Yes,
    /// Not supported.
    No,
    /// Not reported; treat as unsupported for anything safety-relevant.
    Unknown,
}

/// How names on a volume are compared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CaseSensitivity {
    /// `a` and `A` name different files.
    Sensitive,
    /// `a` and `A` name the same file (macOS default, Windows).
    Insensitive,
    /// Not reported.
    Unknown,
}

/// Where a volume lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VolumeLocation {
    /// Built-in storage.
    Internal,
    /// Removable or external storage.
    External,
    /// Network share.
    Network,
    /// Not reported.
    Unknown,
}

/// A mounted volume.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Volume {
    /// Platform volume identifier.
    pub id: VolumeId,
    /// Filesystem format.
    pub filesystem: FilesystemKind,
    /// Where the volume is mounted (a volume can be mounted more than once).
    pub mount_points: Vec<RawPath>,
    /// Where the storage lives.
    pub location: VolumeLocation,
    /// Mounted read-only.
    pub read_only: bool,
    /// Name comparison behaviour.
    pub case_sensitivity: CaseSensitivity,
    /// Lookups ignore Unicode normalisation differences (APFS).
    pub normalization_insensitive: Support,
    /// Rename can refuse to replace an existing target atomically
    /// (`RENAME_EXCL`, `RENAME_NOREPLACE`, `FileRenameInfoEx` without replace).
    pub no_replace_rename: Support,
    /// Copy-on-write clones (APFS `clonefile`, `ReFS` block cloning).
    pub clones: Support,
}

impl Volume {
    /// Whether Lumen may quarantine items on this volume by rename (ADR-0015):
    /// writable, internal, and with a verified no-replace rename. Anything else
    /// gets `REVIEW` rather than a copy-and-delete.
    pub fn supports_quarantine(&self) -> bool {
        !self.read_only
            && self.location == VolumeLocation::Internal
            && self.no_replace_rename == Support::Yes
    }
}

/// What kind of object a directory entry is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    /// Regular file.
    File,
    /// Directory.
    Directory,
    /// Symbolic link.
    Symlink,
    /// Windows junction or mount-point reparse point.
    Junction,
    /// Any other reparse point (cloud placeholder, `AppExecLink`, dedup, …).
    OtherReparsePoint,
    /// Device, FIFO, socket or other special file.
    Special,
}

impl EntryKind {
    /// Whether a scanner may descend into an entry of this kind. Links and reparse
    /// points are recorded as graph edges and never followed (ADR-0017).
    pub const fn is_traversable(self) -> bool {
        matches!(self, Self::Directory)
    }
}

/// Timestamps reported for an entry. Any may be unavailable. Access times are
/// weak evidence: many systems update them lazily or never (`noatime`,
/// `relatime`, `NtfsDisableLastAccessUpdate`).
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema,
)]
pub struct EntryTimes {
    /// Content last modified.
    pub modified: Option<Timestamp>,
    /// Last accessed (unreliable).
    pub accessed: Option<Timestamp>,
    /// Metadata last changed (`ctime`).
    pub changed: Option<Timestamp>,
    /// Created (birth time).
    pub created: Option<Timestamp>,
}

/// Operating-system protections that make an entry untouchable (ADR-0014 stage 1).
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema,
)]
#[allow(clippy::struct_excessive_bools)] // Independent OS flags, not a state machine.
pub struct Protection {
    /// macOS `SF_RESTRICTED` (System Integrity Protection) or an equivalent
    /// OS-integrity protection.
    pub system_restricted: bool,
    /// Cannot be unlinked or is immutable (`SF_NOUNLINK`, `UF_IMMUTABLE`,
    /// `SF_IMMUTABLE`, Linux immutable attribute).
    pub undeletable: bool,
    /// Windows `FILE_ATTRIBUTE_SYSTEM`.
    pub system_attribute: bool,
    /// Hidden from normal listings (dot-file or `FILE_ATTRIBUTE_HIDDEN`).
    pub hidden: bool,
}

impl Protection {
    /// Whether the operating system itself forbids removing the entry.
    pub const fn os_protected(self) -> bool {
        self.system_restricted || self.undeletable
    }
}

/// One directory entry as observed by a scanner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FilesystemEntry {
    /// Path as reported by the platform.
    pub path: RawPath,
    /// Object kind.
    pub kind: EntryKind,
    /// Size facts, including the entry's identity.
    pub size: SizeFacts,
    /// Timestamps.
    pub times: EntryTimes,
    /// OS protections.
    pub protection: Protection,
    /// For directories, whether their contents were read; for other kinds,
    /// whether their metadata was read.
    pub access: AccessState,
}

impl FilesystemEntry {
    /// The entry's identity (volume and file ID).
    pub fn identity(&self) -> &FileIdentity {
        &self.size.identity
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use super::*;
    use crate::{ByteCount, DenialReason, FileId, SizeFlags};

    fn volume() -> Volume {
        Volume {
            id: VolumeId::new("A1B2").unwrap_or_else(|_| unreachable!()),
            filesystem: FilesystemKind::Apfs,
            mount_points: vec![
                RawPath::from_unix_bytes(b"/".to_vec()).unwrap_or_else(|_| unreachable!()),
            ],
            location: VolumeLocation::Internal,
            read_only: false,
            case_sensitivity: CaseSensitivity::Insensitive,
            normalization_insensitive: Support::Yes,
            no_replace_rename: Support::Yes,
            clones: Support::Yes,
        }
    }

    #[test]
    fn quarantine_requires_internal_writable_no_replace_volume() {
        assert!(volume().supports_quarantine());
        assert!(
            !Volume {
                read_only: true,
                ..volume()
            }
            .supports_quarantine()
        );
        assert!(
            !Volume {
                location: VolumeLocation::External,
                ..volume()
            }
            .supports_quarantine()
        );
        assert!(
            !Volume {
                location: VolumeLocation::Network,
                ..volume()
            }
            .supports_quarantine()
        );
        assert!(
            !Volume {
                location: VolumeLocation::Unknown,
                ..volume()
            }
            .supports_quarantine()
        );
        assert!(
            !Volume {
                no_replace_rename: Support::Unknown,
                ..volume()
            }
            .supports_quarantine()
        );
        assert!(
            !Volume {
                no_replace_rename: Support::No,
                ..volume()
            }
            .supports_quarantine()
        );
    }

    #[test]
    fn only_directories_are_traversed() {
        let kinds = [
            EntryKind::File,
            EntryKind::Directory,
            EntryKind::Symlink,
            EntryKind::Junction,
            EntryKind::OtherReparsePoint,
            EntryKind::Special,
        ];
        let traversable: Vec<_> = kinds.into_iter().filter(|k| k.is_traversable()).collect();
        assert_eq!(traversable, [EntryKind::Directory]);
    }

    #[test]
    fn os_protection_ignores_cosmetic_flags() {
        assert!(
            Protection {
                system_restricted: true,
                ..Protection::default()
            }
            .os_protected()
        );
        assert!(
            Protection {
                undeletable: true,
                ..Protection::default()
            }
            .os_protected()
        );
        assert!(
            !Protection {
                hidden: true,
                system_attribute: true,
                ..Protection::default()
            }
            .os_protected()
        );
    }

    #[test]
    fn entry_round_trips_through_json() -> Result<(), Box<dyn std::error::Error>> {
        let entry = FilesystemEntry {
            path: RawPath::from_unix_bytes(b"/Users/ana/Library/Caches/com.example".to_vec())?,
            kind: EntryKind::Directory,
            size: SizeFacts {
                identity: FileIdentity {
                    volume: VolumeId::new("A1B2")?,
                    file: FileId::new(42),
                },
                logical: ByteCount::new(4096),
                allocated: ByteCount::new(4096),
                private: None,
                clone_id: None,
                link_count: NonZeroU32::MIN,
                flags: SizeFlags::default(),
            },
            times: EntryTimes {
                modified: Some("2026-10-01T08:00:00Z".parse()?),
                ..EntryTimes::default()
            },
            protection: Protection::default(),
            access: AccessState::Denied(DenialReason::Tcc),
        };
        let json = serde_json::to_string(&entry)?;
        assert_eq!(serde_json::from_str::<FilesystemEntry>(&json)?, entry);
        assert_eq!(entry.identity().file, FileId::new(42));
        let volume_json = serde_json::to_string(&volume())?;
        assert_eq!(serde_json::from_str::<Volume>(&volume_json)?, volume());
        Ok(())
    }
}
