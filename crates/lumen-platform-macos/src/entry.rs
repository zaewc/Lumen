//! Turns parsed `getattrlist` attributes into domain [`FilesystemEntry`] values.
//!
//! Pure and platform-independent, so it is tested on every CI platform. Missing or
//! out-of-range attributes make an entry [`Unmappable`]; the enumerator then counts
//! the listing as incomplete instead of guessing (ADR-0008).

use std::num::NonZeroU32;

use lumen_domain::{
    AccessState, ByteCount, EntryKind, EntryTimes, FileId, FileIdentity, FilesystemEntry,
    Protection, RawPath, SizeFacts, SizeFlags, Timestamp, VolumeId,
};

use crate::attrbuf::{
    ATTR_CMN_ACCTIME, ATTR_CMN_CHGTIME, ATTR_CMN_CRTIME, ATTR_CMN_DEVID, ATTR_CMN_ERROR,
    ATTR_CMN_FILEID, ATTR_CMN_FLAGS, ATTR_CMN_MODTIME, ATTR_CMN_NAME, ATTR_CMN_OBJTYPE,
    ATTR_CMN_RETURNED_ATTRS, ATTR_CMNEXT_EXT_FLAGS, ATTR_CMNEXT_PRIVATESIZE, ATTR_DIR_LINKCOUNT,
    ATTR_DIR_MOUNTSTATUS, ATTR_FILE_ALLOCSIZE, ATTR_FILE_LINKCOUNT, ATTR_FILE_TOTALSIZE,
    AttributeSet, DIR_MNTSTATUS_MNTPOINT, RawEntry, TimeSpec,
};

// `enum vtype` (`sys/vnode.h`).
const VREG: u32 = 1;
const VDIR: u32 = 2;
const VLNK: u32 = 5;

// BSD file flags (`sys/stat.h`).
const UF_IMMUTABLE: u32 = 0x0000_0002;
const UF_COMPRESSED: u32 = 0x0000_0020;
const UF_HIDDEN: u32 = 0x0000_8000;
const SF_IMMUTABLE: u32 = 0x0002_0000;
const SF_RESTRICTED: u32 = 0x0008_0000;
const SF_NOUNLINK: u32 = 0x0010_0000;
const SF_FIRMLINK: u32 = 0x0080_0000;
const SF_DATALESS: u32 = 0x4000_0000;

// `ATTR_CMNEXT_EXT_FLAGS` bits (`sys/stat.h`).
const EF_MAY_SHARE_BLOCKS: u64 = 0x01;
const EF_IS_SPARSE: u64 = 0x10;
const EF_SHARES_ALL_BLOCKS: u64 = 0x40;

/// The attributes Lumen requests for every directory entry. With `private_size`,
/// `ATTR_CMNEXT_PRIVATESIZE` is included (ADR-0028). Lookups of a single object
/// drop [`ATTR_CMN_NAME`], which only bulk listings require.
pub const fn request(private_size: bool) -> AttributeSet {
    AttributeSet {
        common: ATTR_CMN_RETURNED_ATTRS
            | ATTR_CMN_ERROR
            | ATTR_CMN_NAME
            | ATTR_CMN_DEVID
            | ATTR_CMN_OBJTYPE
            | ATTR_CMN_CRTIME
            | ATTR_CMN_MODTIME
            | ATTR_CMN_CHGTIME
            | ATTR_CMN_ACCTIME
            | ATTR_CMN_FLAGS
            | ATTR_CMN_FILEID,
        volume: 0,
        directory: ATTR_DIR_LINKCOUNT | ATTR_DIR_MOUNTSTATUS,
        file: ATTR_FILE_LINKCOUNT | ATTR_FILE_TOTALSIZE | ATTR_FILE_ALLOCSIZE,
        extended: if private_size {
            ATTR_CMNEXT_PRIVATESIZE | ATTR_CMNEXT_EXT_FLAGS
        } else {
            ATTR_CMNEXT_EXT_FLAGS
        },
    }
}

/// Whether a listed entry must be looked up again by name before use.
///
/// For mount points `getattrlistbulk` describes the covered directory, and for
/// firmlinks the link itself; `getattrlistat(FSOPT_NOFOLLOW)` returns what `lstat`
/// does, which is what scan identities are built on.
pub fn needs_lookup(raw: &RawEntry<'_>) -> bool {
    raw.mount_status
        .is_some_and(|status| status & DIR_MNTSTATUS_MNTPOINT != 0)
        || raw.flags.is_some_and(|flags| flags & SF_FIRMLINK != 0)
}

/// Why attributes could not become a [`FilesystemEntry`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Unmappable {
    /// The kernel reported an error for this entry (an errno).
    #[error("the entry reported error {0}")]
    Error(NonZeroU32),
    /// A required attribute was not returned.
    #[error("attribute {0} was not returned")]
    Missing(&'static str),
    /// A size was negative.
    #[error("attribute {0} is negative")]
    Negative(&'static str),
    /// The device number does not form a volume ID.
    #[error("device {0} does not form a volume ID")]
    Device(i32),
}

/// The volume ID for a device number. `dev_t` is signed on macOS and Rust's
/// `MetadataExt::dev` sign-extends it, so this does too: identities from `stat`
/// and from `getattrlist` must compare equal.
///
/// # Errors
///
/// Returns [`Unmappable::Device`] if the formatted ID is rejected (it is visible
/// ASCII, so this does not happen in practice).
pub fn volume_id(device: i32) -> Result<VolumeId, Unmappable> {
    VolumeId::new(format!("dev:{:x}", i64::from(device).cast_unsigned()))
        .map_err(|_| Unmappable::Device(device))
}

fn size(value: Option<i64>, name: &'static str) -> Result<ByteCount, Unmappable> {
    let value = value.ok_or(Unmappable::Missing(name))?;
    u64::try_from(value)
        .map(ByteCount::new)
        .map_err(|_| Unmappable::Negative(name))
}

fn timestamp(time: Option<TimeSpec>) -> Option<Timestamp> {
    let time = time?;
    Timestamp::from_unix(time.seconds, u32::try_from(time.nanoseconds).ok()?).ok()
}

/// Builds the entry for the object at `path` from its attributes.
///
/// Clone IDs are not used until clone tracking is gated per volume
/// (`VOL_CAP_FMT_CLONE_MAPPING`, roadmap 5.12).
///
/// # Errors
///
/// Returns [`Unmappable`] if the entry carries an error, lacks the type, device,
/// file ID or flags (or, for files and symlinks, sizes), or has a negative size.
pub fn to_entry(path: RawPath, raw: &RawEntry<'_>) -> Result<FilesystemEntry, Unmappable> {
    if let Some(error) = raw.error {
        return Err(Unmappable::Error(error));
    }
    let kind = match raw.object_type.ok_or(Unmappable::Missing("object type"))? {
        VREG => EntryKind::File,
        VDIR => EntryKind::Directory,
        VLNK => EntryKind::Symlink,
        _ => EntryKind::Special,
    };
    let device = raw.device.ok_or(Unmappable::Missing("device"))?;
    let file_id = raw.file_id.ok_or(Unmappable::Missing("file ID"))?;
    // Without flags, OS protections would be unknown, not absent.
    let flags = raw.flags.ok_or(Unmappable::Missing("flags"))?;
    let is_file = kind == EntryKind::File;

    let (logical, allocated) = match kind {
        // Directory sizes are not measured; totals come from their contents.
        EntryKind::Directory => (ByteCount::ZERO, ByteCount::ZERO),
        _ => (
            size(raw.total_size, "total size")?,
            size(raw.allocated_size, "allocated size")?,
        ),
    };
    let private = match raw.private_size {
        Some(_) if is_file => Some(size(raw.private_size, "private size")?),
        _ => None,
    };
    let link_count = raw
        .file_link_count
        .or(raw.directory_link_count)
        .and_then(NonZeroU32::new)
        .unwrap_or(NonZeroU32::MIN);
    let ext = raw.ext_flags;
    let shares = ext.is_none_or(|e| e & (EF_MAY_SHARE_BLOCKS | EF_SHARES_ALL_BLOCKS) != 0);
    let sparse = match ext {
        Some(e) => e & EF_IS_SPARSE != 0,
        None => allocated < logical && flags & UF_COMPRESSED == 0,
    };
    let hidden_name = path
        .as_bytes()
        .rsplit(|b| *b == b'/')
        .next()
        .is_some_and(|name| name.starts_with(b"."));

    Ok(FilesystemEntry {
        kind,
        size: SizeFacts {
            identity: FileIdentity {
                volume: volume_id(device)?,
                file: FileId::new(u128::from(file_id)),
            },
            logical,
            allocated,
            private,
            clone_id: None,
            link_count,
            flags: SizeFlags {
                sparse: is_file && sparse,
                compressed: flags & UF_COMPRESSED != 0,
                dataless: flags & SF_DATALESS != 0,
                cloud_placeholder: false,
                // Without a private size, an unflagged file may still free less
                // than its allocation (compressed, sparse and resource-fork files;
                // ADR-0028), so only a measured private size earns `false`.
                may_share_blocks: is_file && (private.is_none() || shares),
            },
        },
        times: EntryTimes {
            modified: timestamp(raw.modified),
            accessed: timestamp(raw.accessed),
            changed: timestamp(raw.changed),
            created: timestamp(raw.created),
        },
        protection: Protection {
            system_restricted: flags & SF_RESTRICTED != 0,
            undeletable: flags & (SF_NOUNLINK | UF_IMMUTABLE | SF_IMMUTABLE) != 0,
            system_attribute: false,
            hidden: hidden_name || flags & UF_HIDDEN != 0,
        },
        // Metadata was read; a directory's *contents* have not been.
        access: if kind == EntryKind::Directory {
            AccessState::NotScanned
        } else {
            AccessState::Readable
        },
        path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(p: &str) -> RawPath {
        RawPath::from_unix_bytes(p.as_bytes().to_vec()).unwrap_or_else(|_| unreachable!())
    }

    fn file() -> RawEntry<'static> {
        RawEntry {
            returned: request(true),
            error: None,
            name: Some(b"f"),
            device: Some(0x0100_0010),
            object_type: Some(VREG),
            created: Some(TimeSpec {
                seconds: 1_700_000_000,
                nanoseconds: 5,
            }),
            modified: Some(TimeSpec {
                seconds: 1_700_000_001,
                nanoseconds: 0,
            }),
            changed: None,
            accessed: Some(TimeSpec {
                seconds: 1,
                nanoseconds: 1_000_000_000,
            }),
            flags: Some(0),
            file_id: Some(42),
            directory_link_count: None,
            mount_status: None,
            file_link_count: Some(2),
            total_size: Some(5),
            allocated_size: Some(4096),
            private_size: Some(4096),
            clone_id: Some(42),
            ext_flags: Some(0),
        }
    }

    #[test]
    fn maps_a_regular_file() -> Result<(), Unmappable> {
        let entry = to_entry(path("/t/f"), &file())?;
        assert_eq!(entry.kind, EntryKind::File);
        assert_eq!(entry.size.identity.volume.as_str(), "dev:1000010");
        assert_eq!(entry.size.identity.file, FileId::new(42));
        assert_eq!(entry.size.logical, ByteCount::new(5));
        assert_eq!(entry.size.allocated, ByteCount::new(4096));
        assert_eq!(entry.size.private, Some(ByteCount::new(4096)));
        assert_eq!(entry.size.clone_id, None, "until clone tracking is gated");
        assert_eq!(entry.size.link_count.get(), 2);
        assert!(!entry.size.flags.may_share_blocks);
        assert!(!entry.size.flags.sparse);
        assert_eq!(entry.access, AccessState::Readable);
        assert!(entry.times.created.is_some());
        assert_eq!(entry.times.changed, None);
        assert_eq!(entry.times.accessed, None, "nanoseconds out of range");
        Ok(())
    }

    #[test]
    fn negative_devices_sign_extend_like_std() {
        // `/dev` on macOS 26: st_dev -1271763927; `MetadataExt::dev` sign-extends.
        let std_dev = i64::from(-1_271_763_927_i32).cast_unsigned();
        assert_eq!(
            volume_id(-1_271_763_927).map(|v| v.as_str().to_owned()),
            Ok(format!("dev:{std_dev:x}"))
        );
    }

    #[test]
    fn kinds_follow_the_vnode_type() -> Result<(), Unmappable> {
        let dir = RawEntry {
            object_type: Some(VDIR),
            file_link_count: None,
            directory_link_count: Some(1),
            total_size: None,
            allocated_size: None,
            private_size: None,
            ..file()
        };
        let dir = to_entry(path("/t/d"), &dir)?;
        assert_eq!(dir.kind, EntryKind::Directory);
        assert_eq!(dir.access, AccessState::NotScanned);
        assert_eq!(dir.size.logical, ByteCount::ZERO);
        assert!(!dir.size.flags.may_share_blocks);

        let link = to_entry(
            path("/t/l"),
            &RawEntry {
                object_type: Some(VLNK),
                ..file()
            },
        )?;
        assert_eq!(link.kind, EntryKind::Symlink);
        assert!(!link.size.flags.may_share_blocks, "only regular files");
        for special in [0, 3, 4, 6, 7, 8, 99] {
            let entry = to_entry(
                path("/t/s"),
                &RawEntry {
                    object_type: Some(special),
                    ..file()
                },
            )?;
            assert_eq!(entry.kind, EntryKind::Special, "vtype {special}");
        }
        Ok(())
    }

    #[test]
    fn sharing_is_assumed_unless_a_private_size_was_measured() -> Result<(), Unmappable> {
        let unmeasured = RawEntry {
            private_size: None,
            ..file()
        };
        assert!(
            to_entry(path("/f"), &unmeasured)?
                .size
                .flags
                .may_share_blocks
        );
        let clone = RawEntry {
            ext_flags: Some(EF_SHARES_ALL_BLOCKS | EF_MAY_SHARE_BLOCKS),
            ..file()
        };
        assert!(to_entry(path("/f"), &clone)?.size.flags.may_share_blocks);
        let no_ext = RawEntry {
            ext_flags: None,
            ..file()
        };
        assert!(to_entry(path("/f"), &no_ext)?.size.flags.may_share_blocks);
        Ok(())
    }

    #[test]
    fn protections_and_storage_flags_come_from_bsd_flags() -> Result<(), Unmappable> {
        let raw = RawEntry {
            flags: Some(SF_RESTRICTED | SF_NOUNLINK | UF_COMPRESSED | SF_DATALESS),
            ext_flags: Some(EF_IS_SPARSE),
            ..file()
        };
        let entry = to_entry(path("/t/f"), &raw)?;
        assert!(entry.protection.system_restricted);
        assert!(entry.protection.undeletable);
        assert!(entry.protection.os_protected());
        assert!(!entry.protection.hidden);
        assert!(entry.size.flags.compressed);
        assert!(entry.size.flags.dataless);
        assert!(entry.size.flags.sparse);

        let hidden = to_entry(path("/t/.git"), &file())?;
        assert!(hidden.protection.hidden);
        let flagged = RawEntry {
            flags: Some(UF_HIDDEN),
            ..file()
        };
        assert!(to_entry(path("/t/x"), &flagged)?.protection.hidden);
        Ok(())
    }

    #[test]
    fn missing_or_invalid_attributes_are_unmappable() {
        let cases: [(RawEntry<'_>, Unmappable); 7] = [
            (
                RawEntry {
                    error: NonZeroU32::new(13),
                    ..file()
                },
                Unmappable::Error(NonZeroU32::MIN.saturating_add(12)),
            ),
            (
                RawEntry {
                    object_type: None,
                    ..file()
                },
                Unmappable::Missing("object type"),
            ),
            (
                RawEntry {
                    device: None,
                    ..file()
                },
                Unmappable::Missing("device"),
            ),
            (
                RawEntry {
                    file_id: None,
                    ..file()
                },
                Unmappable::Missing("file ID"),
            ),
            (
                RawEntry {
                    flags: None,
                    ..file()
                },
                Unmappable::Missing("flags"),
            ),
            (
                RawEntry {
                    total_size: None,
                    ..file()
                },
                Unmappable::Missing("total size"),
            ),
            (
                RawEntry {
                    allocated_size: Some(-1),
                    ..file()
                },
                Unmappable::Negative("allocated size"),
            ),
        ];
        for (raw, expected) in cases {
            assert_eq!(to_entry(path("/f"), &raw), Err(expected));
        }
    }

    #[test]
    fn mount_points_and_firmlinks_need_a_lookup() {
        assert!(!needs_lookup(&file()));
        assert!(needs_lookup(&RawEntry {
            mount_status: Some(DIR_MNTSTATUS_MNTPOINT),
            ..file()
        }));
        assert!(!needs_lookup(&RawEntry {
            mount_status: Some(0),
            ..file()
        }));
        assert!(needs_lookup(&RawEntry {
            flags: Some(SF_FIRMLINK | SF_NOUNLINK),
            ..file()
        }));
    }

    #[test]
    fn the_request_is_parseable_and_names_entries() {
        for private in [false, true] {
            let set = request(private);
            assert!(set.is_subset_of(AttributeSet::SUPPORTED));
            assert_ne!(set.common & ATTR_CMN_NAME, 0);
            assert_ne!(set.common & ATTR_CMN_RETURNED_ATTRS, 0);
            assert_eq!(set.extended & ATTR_CMNEXT_PRIVATESIZE != 0, private);
        }
    }
}
