//! Turns [`DirRecord`]s into domain [`FilesystemEntry`] values.
//!
//! Pure and platform-independent, so it is tested on every CI platform. A record
//! with an out-of-range value is [`Unmappable`]; the enumerator then counts the
//! listing as incomplete instead of guessing (ADR-0008).

use std::num::NonZeroU32;

use lumen_domain::{
    AccessState, ByteCount, EntryKind, EntryTimes, FileId, FileIdentity, FilesystemEntry,
    PathError, Protection, RawPath, SizeFacts, SizeFlags, Timestamp, VolumeId,
};

use crate::dirinfo::DirRecord;

// `FILE_ATTRIBUTE_*` (`winnt.h`, as in `windows-sys` 0.61).
const FILE_ATTRIBUTE_HIDDEN: u32 = 0x0000_0002;
const FILE_ATTRIBUTE_SYSTEM: u32 = 0x0000_0004;
const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x0000_0010;
const FILE_ATTRIBUTE_DEVICE: u32 = 0x0000_0040;
const FILE_ATTRIBUTE_SPARSE_FILE: u32 = 0x0000_0200;
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
const FILE_ATTRIBUTE_COMPRESSED: u32 = 0x0000_0800;
const FILE_ATTRIBUTE_OFFLINE: u32 = 0x0000_1000;
const FILE_ATTRIBUTE_RECALL_ON_OPEN: u32 = 0x0004_0000;
const FILE_ATTRIBUTE_PINNED: u32 = 0x0008_0000;
const FILE_ATTRIBUTE_UNPINNED: u32 = 0x0010_0000;
const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;

// `IO_REPARSE_TAG_*` (`winnt.h`, as in `windows-sys` 0.61).
const IO_REPARSE_TAG_MOUNT_POINT: u32 = 0xA000_0003;
const IO_REPARSE_TAG_SYMLINK: u32 = 0xA000_000C;
const IO_REPARSE_TAG_LX_SYMLINK: u32 = 0xA000_001D;
const IO_REPARSE_TAG_WOF: u32 = 0x8000_0017;
const IO_REPARSE_TAG_DEDUP: u32 = 0x8000_0013;
const IO_REPARSE_TAG_ONEDRIVE: u32 = 0x8000_0021;
const IO_REPARSE_TAG_PROJFS: u32 = 0x9000_001C;
/// Cloud Files tags are `IO_REPARSE_TAG_CLOUD` with any value in the
/// `IO_REPARSE_TAG_CLOUD_MASK` nibble.
const IO_REPARSE_TAG_CLOUD: u32 = 0x9000_001A;
const IO_REPARSE_TAG_CLOUD_MASK: u32 = 0x0000_F000;

/// `FILETIME` ticks (100 ns) between 1601-01-01 and 1970-01-01.
const EPOCH_DIFFERENCE_SECONDS: i64 = 11_644_473_600;
const TICKS_PER_SECOND: i64 = 10_000_000;

/// Why a record could not become a [`FilesystemEntry`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Unmappable {
    /// The record is `.` or `..`, which is not an entry of the directory.
    #[error("`.` and `..` are not entries")]
    Dot,
    /// A size was negative.
    #[error("attribute {0} is negative")]
    Negative(&'static str),
    /// The joined path is not a valid Windows path.
    #[error("the entry path is invalid: {0}")]
    Path(PathError),
    /// The volume serial number does not form a volume ID.
    #[error("volume serial {0:#x} does not form a volume ID")]
    Volume(u64),
}

/// The volume ID for a volume serial number (`FILE_ID_INFO::VolumeSerialNumber`).
///
/// # Errors
///
/// Returns [`Unmappable::Volume`] if the formatted ID is rejected (it is visible
/// ASCII, so this does not happen in practice).
pub fn volume_id(serial: u64) -> Result<VolumeId, Unmappable> {
    VolumeId::new(format!("vsn:{serial:016x}")).map_err(|_| Unmappable::Volume(serial))
}

/// Converts `FILETIME` ticks to a timestamp; 0 and negative ticks mean "not
/// recorded" on Windows filesystems.
pub fn filetime(ticks: i64) -> Option<Timestamp> {
    if ticks <= 0 {
        return None;
    }
    let seconds = ticks.div_euclid(TICKS_PER_SECOND) - EPOCH_DIFFERENCE_SECONDS;
    let nanos = u32::try_from(ticks.rem_euclid(TICKS_PER_SECOND) * 100).ok()?;
    Timestamp::from_unix(seconds, nanos).ok()
}

fn is_cloud_tag(tag: u32) -> bool {
    tag & !IO_REPARSE_TAG_CLOUD_MASK == IO_REPARSE_TAG_CLOUD
        || tag == IO_REPARSE_TAG_ONEDRIVE
        || tag == IO_REPARSE_TAG_PROJFS
}

fn kind_of(record: &DirRecord) -> EntryKind {
    let attributes = record.attributes;
    if attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        // Reparse points are recorded and never followed (ADR-0017), whatever
        // they point at: a junction to a directory is not a directory.
        return match record.reparse_tag {
            IO_REPARSE_TAG_SYMLINK | IO_REPARSE_TAG_LX_SYMLINK => EntryKind::Symlink,
            IO_REPARSE_TAG_MOUNT_POINT => EntryKind::Junction,
            _ => EntryKind::OtherReparsePoint,
        };
    }
    if attributes & FILE_ATTRIBUTE_DIRECTORY != 0 {
        EntryKind::Directory
    } else if attributes & FILE_ATTRIBUTE_DEVICE != 0 {
        EntryKind::Special
    } else {
        EntryKind::File
    }
}

/// Data-carrying entries: regular files, and reparse-point files such as
/// deduplicated (chunks shared in the chunk store), WOF-compressed or cloud files.
/// Links, junctions and directories carry no data of their own.
fn carries_data(kind: EntryKind, attributes: u32) -> bool {
    kind == EntryKind::File
        || (kind == EntryKind::OtherReparsePoint && attributes & FILE_ATTRIBUTE_DIRECTORY == 0)
}

fn size(value: i64, name: &'static str) -> Result<ByteCount, Unmappable> {
    u64::try_from(value)
        .map(ByteCount::new)
        .map_err(|_| Unmappable::Negative(name))
}

/// Joins a directory path and an entry name, both UTF-16.
pub fn join(parent: &[u16], name: &[u16]) -> Vec<u16> {
    let separator = u16::from(b'\\');
    let mut joined = Vec::with_capacity(parent.len() + 1 + name.len());
    joined.extend_from_slice(parent);
    if parent.last() != Some(&separator) {
        joined.push(separator);
    }
    joined.extend_from_slice(name);
    joined
}

/// Builds the entry for `record`, listed in the directory at `parent` on
/// `volume`.
///
/// Directory records carry no hard-link count and cannot show `ReFS` block
/// cloning or deduplication, so every data-carrying entry (a regular file or a
/// reparse-point file) is marked `may_share_blocks`: its reclaim estimate is a
/// lower bound until it is verified by handle.
///
/// # Errors
///
/// Returns [`Unmappable`] for `.` and `..`, negative sizes, or a path that is
/// not a valid Windows path.
pub fn to_entry(
    parent: &[u16],
    volume: &VolumeId,
    record: &DirRecord,
) -> Result<FilesystemEntry, Unmappable> {
    if record.is_dot() {
        return Err(Unmappable::Dot);
    }
    let path = RawPath::from_windows_wide(&join(parent, &record.name)).map_err(Unmappable::Path)?;
    to_entry_at(path, volume, record)
}

/// Builds the entry for the object at `path` from its attributes, as
/// [`to_entry`] does for a listed record; `record.name` is not used.
///
/// # Errors
///
/// Returns [`Unmappable::Negative`] for negative sizes.
pub fn to_entry_at(
    path: RawPath,
    volume: &VolumeId,
    record: &DirRecord,
) -> Result<FilesystemEntry, Unmappable> {
    let kind = kind_of(record);
    let attributes = record.attributes;
    let carries_data = carries_data(kind, attributes);
    let reparse_tag =
        (attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0).then_some(record.reparse_tag);

    let (logical, allocated) = if kind == EntryKind::Directory {
        // Directory sizes are not measured; totals come from their contents.
        (ByteCount::ZERO, ByteCount::ZERO)
    } else {
        (
            size(record.end_of_file, "end of file")?,
            size(record.allocation_size, "allocation size")?,
        )
    };
    let recall = FILE_ATTRIBUTE_RECALL_ON_OPEN | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS;
    // Pinned and unpinned items live in a sync root even when fully local:
    // removing them would propagate to the cloud.
    let synced = FILE_ATTRIBUTE_PINNED | FILE_ATTRIBUTE_UNPINNED;

    Ok(FilesystemEntry {
        kind,
        size: SizeFacts {
            identity: FileIdentity {
                volume: volume.clone(),
                file: FileId::new(record.file_id),
            },
            logical,
            allocated,
            private: None,
            clone_id: None,
            link_count: NonZeroU32::MIN,
            flags: SizeFlags {
                sparse: attributes & FILE_ATTRIBUTE_SPARSE_FILE != 0,
                compressed: attributes & FILE_ATTRIBUTE_COMPRESSED != 0
                    || reparse_tag == Some(IO_REPARSE_TAG_WOF),
                dataless: attributes & (recall | FILE_ATTRIBUTE_OFFLINE) != 0,
                cloud_placeholder: attributes & (recall | synced) != 0
                    || reparse_tag.is_some_and(is_cloud_tag),
                may_share_blocks: carries_data,
            },
        },
        times: EntryTimes {
            modified: filetime(record.written),
            accessed: filetime(record.accessed),
            changed: filetime(record.changed),
            created: filetime(record.created),
        },
        protection: Protection {
            system_restricted: false,
            undeletable: false,
            system_attribute: attributes & FILE_ATTRIBUTE_SYSTEM != 0,
            hidden: attributes & FILE_ATTRIBUTE_HIDDEN != 0,
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

/// The filesystem of a volume, as far as block sharing is concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filesystem {
    /// NTFS: no block sharing between files (hard links share whole files).
    Ntfs,
    /// FAT, FAT32 or exFAT: no block sharing.
    Fat,
    /// `ReFS`, including Dev Drive: block cloning shares extents between files.
    Refs,
    /// Anything else, or not reported: assume sharing is possible.
    Other,
}

impl Filesystem {
    /// Classifies a `GetVolumeInformationByHandleW` filesystem name.
    pub fn from_name(name: &str) -> Self {
        match name.to_ascii_uppercase().as_str() {
            "NTFS" => Self::Ntfs,
            "FAT" | "FAT32" | "EXFAT" => Self::Fat,
            "REFS" => Self::Refs,
            _ => Self::Other,
        }
    }
}

/// Facts read through a candidate's own handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HandleFacts {
    /// `FILE_STANDARD_INFO::NumberOfLinks`.
    pub links: u32,
    /// `FILE_COMPRESSION_INFO::CompressedFileSize`, read for compressed or sparse
    /// files; `None` if not read or not available.
    pub compressed_size: Option<i64>,
    /// The volume's filesystem.
    pub filesystem: Filesystem,
}

/// Builds the verified entry for a candidate from its handle attributes
/// (`record`, in directory-record shape) and the extra [`HandleFacts`].
///
/// Unlike a listing, the link count is exact. A data-carrying entry is exactly
/// reclaimable (`may_share_blocks: false`) only on a filesystem without block
/// sharing, when it is not deduplicated, and, if compressed or sparse, when its
/// compressed size was read; its allocation is then the smaller of the two sizes.
///
/// # Errors
///
/// Returns [`Unmappable::Negative`] for negative sizes.
pub fn to_verified_entry(
    path: RawPath,
    volume: &VolumeId,
    record: &DirRecord,
    facts: &HandleFacts,
) -> Result<FilesystemEntry, Unmappable> {
    let mut entry = to_entry_at(path, volume, record)?;
    let attributes = record.attributes;
    if !carries_data(entry.kind, attributes) {
        return Ok(entry);
    }
    entry.size.link_count = NonZeroU32::new(facts.links).unwrap_or(NonZeroU32::MIN);
    let packed = attributes & (FILE_ATTRIBUTE_COMPRESSED | FILE_ATTRIBUTE_SPARSE_FILE) != 0;
    let mut measured = true;
    if packed {
        match facts.compressed_size {
            Some(compressed) => {
                let compressed = size(compressed, "compressed size")?;
                entry.size.allocated = entry.size.allocated.min(compressed);
            }
            None => measured = false,
        }
    }
    let deduplicated = attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
        && record.reparse_tag == IO_REPARSE_TAG_DEDUP;
    let unshared_filesystem = matches!(facts.filesystem, Filesystem::Ntfs | Filesystem::Fat);
    entry.size.flags.may_share_blocks = !(unshared_filesystem && !deduplicated && measured);
    Ok(entry)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    fn volume() -> VolumeId {
        volume_id(0x1234_ABCD).unwrap_or_else(|_| unreachable!())
    }

    fn record(name: &str, attributes: u32, tag: u32) -> DirRecord {
        DirRecord {
            created: 133_000_000_000_000_000,
            accessed: 0,
            written: 116_444_736_000_000_000 + 15_000_000,
            changed: 133_000_000_000_000_000,
            end_of_file: 5,
            allocation_size: 8192,
            attributes,
            reparse_tag: tag,
            file_id: 42,
            name: wide(name),
        }
    }

    fn map(name: &str, attributes: u32, tag: u32) -> Result<FilesystemEntry, Unmappable> {
        to_entry(&wide(r"C:\data"), &volume(), &record(name, attributes, tag))
    }

    #[test]
    fn maps_a_regular_file() -> Result<(), Box<dyn std::error::Error>> {
        let entry = map("a.txt", 0x20, 0)?;
        assert_eq!(entry.kind, EntryKind::File);
        assert_eq!(entry.path.to_windows_wide(), Some(wide(r"C:\data\a.txt")));
        assert_eq!(entry.size.identity.volume.as_str(), "vsn:000000001234abcd");
        assert_eq!(entry.size.identity.file, FileId::new(42));
        assert_eq!(entry.size.logical, ByteCount::new(5));
        assert_eq!(entry.size.allocated, ByteCount::new(8192));
        assert!(entry.size.flags.may_share_blocks, "link count unknown");
        assert_eq!(entry.access, AccessState::Readable);
        assert_eq!(
            entry.times.modified.map(|t| t.to_string()),
            Some("1970-01-01T00:00:01.5Z".to_owned())
        );
        assert_eq!(entry.times.accessed, None, "0 ticks: not recorded");
        Ok(())
    }

    #[test]
    fn reparse_points_are_never_directories() -> Result<(), Unmappable> {
        let dir = FILE_ATTRIBUTE_DIRECTORY;
        let reparse = FILE_ATTRIBUTE_REPARSE_POINT;
        assert_eq!(map("d", dir, 0)?.kind, EntryKind::Directory);
        assert_eq!(map("d", dir, 0)?.size.logical, ByteCount::ZERO);
        assert_eq!(map("d", dir, 0)?.access, AccessState::NotScanned);
        let cases = [
            (
                dir | reparse,
                IO_REPARSE_TAG_MOUNT_POINT,
                EntryKind::Junction,
            ),
            (dir | reparse, IO_REPARSE_TAG_SYMLINK, EntryKind::Symlink),
            (reparse, IO_REPARSE_TAG_SYMLINK, EntryKind::Symlink),
            (reparse, IO_REPARSE_TAG_LX_SYMLINK, EntryKind::Symlink),
            (reparse, IO_REPARSE_TAG_WOF, EntryKind::OtherReparsePoint),
            (dir | reparse, 0x9000_101A, EntryKind::OtherReparsePoint),
            (FILE_ATTRIBUTE_DEVICE, 0, EntryKind::Special),
        ];
        for (attributes, tag, kind) in cases {
            let entry = map("x", attributes, tag)?;
            assert_eq!(entry.kind, kind, "{attributes:#x} {tag:#x}");
            assert!(!entry.kind.is_traversable());
        }
        Ok(())
    }

    #[test]
    fn cloud_items_are_placeholders() -> Result<(), Unmappable> {
        let reparse = FILE_ATTRIBUTE_REPARSE_POINT;
        for (attributes, tag) in [
            (FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS, 0),
            (FILE_ATTRIBUTE_RECALL_ON_OPEN, 0),
            (FILE_ATTRIBUTE_PINNED, 0),
            (FILE_ATTRIBUTE_UNPINNED, 0),
            (reparse, IO_REPARSE_TAG_CLOUD),
            (reparse, 0x9000_F01A),
            (reparse, IO_REPARSE_TAG_ONEDRIVE),
            (reparse, IO_REPARSE_TAG_PROJFS),
        ] {
            assert!(
                map("c", attributes, tag)?.size.flags.cloud_placeholder,
                "{attributes:#x} {tag:#x}"
            );
        }
        let recalled = map("c", FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS, 0)?;
        assert!(recalled.size.flags.dataless);
        let offline = map("c", FILE_ATTRIBUTE_OFFLINE, 0)?;
        assert!(offline.size.flags.dataless && !offline.size.flags.cloud_placeholder);
        // A tag without the reparse attribute is stale and ignored.
        assert!(
            !map("c", 0x20, IO_REPARSE_TAG_CLOUD)?
                .size
                .flags
                .cloud_placeholder
        );
        assert!(!map("c", 0x20, 0)?.size.flags.cloud_placeholder);
        Ok(())
    }

    #[test]
    fn attributes_map_to_storage_and_protection_flags() -> Result<(), Unmappable> {
        let entry = map(
            "s",
            FILE_ATTRIBUTE_SPARSE_FILE
                | FILE_ATTRIBUTE_COMPRESSED
                | FILE_ATTRIBUTE_SYSTEM
                | FILE_ATTRIBUTE_HIDDEN,
            0,
        )?;
        assert!(entry.size.flags.sparse && entry.size.flags.compressed);
        assert!(entry.protection.system_attribute && entry.protection.hidden);
        let wof = map("w", FILE_ATTRIBUTE_REPARSE_POINT, IO_REPARSE_TAG_WOF)?;
        assert!(wof.size.flags.compressed);
        Ok(())
    }

    #[test]
    fn reparse_point_files_may_share_blocks_but_links_do_not() -> Result<(), Unmappable> {
        let reparse = FILE_ATTRIBUTE_REPARSE_POINT;
        for tag in [
            IO_REPARSE_TAG_DEDUP,
            IO_REPARSE_TAG_WOF,
            IO_REPARSE_TAG_CLOUD,
        ] {
            assert!(
                map("f", reparse, tag)?.size.flags.may_share_blocks,
                "{tag:#x}"
            );
        }
        let dir = FILE_ATTRIBUTE_DIRECTORY | reparse;
        for (attributes, tag) in [
            (reparse, IO_REPARSE_TAG_SYMLINK),
            (dir, IO_REPARSE_TAG_MOUNT_POINT),
            (dir, IO_REPARSE_TAG_CLOUD),
            (FILE_ATTRIBUTE_DIRECTORY, 0),
        ] {
            assert!(
                !map("l", attributes, tag)?.size.flags.may_share_blocks,
                "{attributes:#x} {tag:#x}"
            );
        }
        Ok(())
    }

    #[test]
    fn dots_negative_sizes_and_bad_paths_are_unmappable() {
        assert_eq!(map(".", FILE_ATTRIBUTE_DIRECTORY, 0), Err(Unmappable::Dot));
        assert_eq!(map("..", FILE_ATTRIBUTE_DIRECTORY, 0), Err(Unmappable::Dot));
        let mut negative = record("n", 0x20, 0);
        negative.allocation_size = -1;
        assert_eq!(
            to_entry(&wide(r"C:\data"), &volume(), &negative),
            Err(Unmappable::Negative("allocation size"))
        );
        // A lone surrogate is still a valid Windows name (WTF-8 storage).
        let lone = DirRecord {
            name: vec![0xD800],
            ..record("x", 0x20, 0)
        };
        assert!(to_entry(&wide(r"C:\data"), &volume(), &lone).is_ok());
    }

    #[test]
    fn filetime_handles_the_epoch_and_sub_second_ticks() {
        assert_eq!(
            filetime(116_444_736_000_000_000).map(|t| t.to_string()),
            Some("1970-01-01T00:00:00Z".to_owned())
        );
        assert_eq!(
            filetime(116_444_736_000_000_000 - 5).map(|t| t.to_string()),
            Some("1969-12-31T23:59:59.9999995Z".to_owned())
        );
        assert_eq!(filetime(0), None);
        assert_eq!(filetime(-1), None);
        assert_eq!(
            filetime(i64::MAX),
            None,
            "year 30828: beyond Timestamp::MAX"
        );
    }

    fn facts(filesystem: Filesystem) -> HandleFacts {
        HandleFacts {
            links: 2,
            compressed_size: None,
            filesystem,
        }
    }

    fn verify(record: &DirRecord, facts: &HandleFacts) -> Result<FilesystemEntry, Unmappable> {
        to_verified_entry(
            RawPath::from_windows_wide(&wide(r"C:\f")).map_err(Unmappable::Path)?,
            &volume(),
            record,
            facts,
        )
    }

    #[test]
    fn verified_files_on_ntfs_are_exact() -> Result<(), Unmappable> {
        let entry = verify(&record("f", 0x20, 0), &facts(Filesystem::Ntfs))?;
        assert_eq!(entry.size.link_count.get(), 2, "exact link count");
        assert!(!entry.size.flags.may_share_blocks);
        assert_eq!(entry.size.allocated, ByteCount::new(8192));
        Ok(())
    }

    #[test]
    fn sharing_stays_possible_unless_proven_absent() -> Result<(), Unmappable> {
        for filesystem in [Filesystem::Refs, Filesystem::Other] {
            let entry = verify(&record("f", 0x20, 0), &facts(filesystem))?;
            assert!(entry.size.flags.may_share_blocks, "{filesystem:?}");
        }
        let dedup = record("f", FILE_ATTRIBUTE_REPARSE_POINT, IO_REPARSE_TAG_DEDUP);
        assert!(
            verify(&dedup, &facts(Filesystem::Ntfs))?
                .size
                .flags
                .may_share_blocks
        );
        // Compressed, but the compressed size could not be read.
        let compressed = record("f", FILE_ATTRIBUTE_COMPRESSED, 0);
        assert!(
            verify(&compressed, &facts(Filesystem::Ntfs))?
                .size
                .flags
                .may_share_blocks
        );
        Ok(())
    }

    #[test]
    fn compressed_and_sparse_files_use_the_smaller_size() -> Result<(), Unmappable> {
        for attributes in [FILE_ATTRIBUTE_COMPRESSED, FILE_ATTRIBUTE_SPARSE_FILE] {
            let measured = HandleFacts {
                compressed_size: Some(4096),
                ..facts(Filesystem::Ntfs)
            };
            let entry = verify(&record("f", attributes, 0), &measured)?;
            assert_eq!(entry.size.allocated, ByteCount::new(4096));
            assert!(!entry.size.flags.may_share_blocks);
            // A larger "compressed" size never raises the allocation.
            let larger = HandleFacts {
                compressed_size: Some(1 << 20),
                ..measured
            };
            assert_eq!(
                verify(&record("f", attributes, 0), &larger)?.size.allocated,
                ByteCount::new(8192)
            );
        }
        Ok(())
    }

    #[test]
    fn links_and_directories_are_not_adjusted() -> Result<(), Unmappable> {
        let link = record("l", FILE_ATTRIBUTE_REPARSE_POINT, IO_REPARSE_TAG_SYMLINK);
        let entry = verify(&link, &facts(Filesystem::Ntfs))?;
        assert_eq!(entry.kind, EntryKind::Symlink);
        assert_eq!(entry.size.link_count.get(), 1);
        assert!(!entry.size.flags.may_share_blocks);
        let dir = verify(
            &record("d", FILE_ATTRIBUTE_DIRECTORY, 0),
            &facts(Filesystem::Ntfs),
        )?;
        assert_eq!(dir.kind, EntryKind::Directory);
        Ok(())
    }

    #[test]
    fn filesystem_names_are_classified() {
        assert_eq!(Filesystem::from_name("NTFS"), Filesystem::Ntfs);
        assert_eq!(Filesystem::from_name("ReFS"), Filesystem::Refs);
        assert_eq!(Filesystem::from_name("exFAT"), Filesystem::Fat);
        assert_eq!(Filesystem::from_name("FAT32"), Filesystem::Fat);
        assert_eq!(Filesystem::from_name("UDF"), Filesystem::Other);
        assert_eq!(Filesystem::from_name(""), Filesystem::Other);
    }

    #[test]
    fn join_adds_one_separator() {
        assert_eq!(join(&wide(r"C:\"), &wide("a")), wide(r"C:\a"));
        assert_eq!(join(&wide(r"C:\d"), &wide("a")), wide(r"C:\d\a"));
    }
}
