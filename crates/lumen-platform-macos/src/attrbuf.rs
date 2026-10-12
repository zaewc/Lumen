//! Parser for `getattrlistbulk(2)` result buffers.
//!
//! Layout, from getattrlist(2) and getattrlistbulk(2) and checked against buffers
//! captured on macOS 26 (APFS):
//!
//! - The buffer holds one group per directory entry, back to back. Each group
//!   starts with a `u32` holding its total length, including that field; the next
//!   group starts that many bytes later.
//! - Next comes an `attribute_set_t` (five `u32` masks: common, volume, directory,
//!   file, extended common) listing the attributes actually returned
//!   (`ATTR_CMN_RETURNED_ATTRS`).
//! - Then `ATTR_CMN_ERROR` (a `u32` errno), if returned, and every other returned
//!   attribute in mask-bit order: common, directory, file, then extended common
//!   attributes. Each field is 4-byte aligned, including 64-bit ones; timestamps
//!   are two `i64`s (seconds, nanoseconds) on 64-bit macOS.
//! - The name is an `attrreference_t`: an `i32` offset from the reference itself
//!   and a `u32` length, pointing at a NUL-terminated string inside the group.
//!
//! Lumen never sets `FSOPT_PACK_INVAL_ATTRS`, so only returned attributes occupy
//! space and the layout follows from the buffer alone. The buffer is kernel
//! output, but it is still treated as untrusted input: every length and offset is
//! bounds-checked, and an attribute the parser cannot size stops parsing instead
//! of being guessed at.

use std::num::NonZeroU32;

/// `ATTR_CMN_NAME`: the entry's name (`attrreference_t`).
pub const ATTR_CMN_NAME: u32 = 0x0000_0001;
/// `ATTR_CMN_DEVID`: device of the containing volume (`dev_t`).
pub const ATTR_CMN_DEVID: u32 = 0x0000_0002;
/// `ATTR_CMN_OBJTYPE`: object type (`fsobj_type_t`, an `enum vtype`).
pub const ATTR_CMN_OBJTYPE: u32 = 0x0000_0008;
/// `ATTR_CMN_CRTIME`: creation time (`timespec`).
pub const ATTR_CMN_CRTIME: u32 = 0x0000_0200;
/// `ATTR_CMN_MODTIME`: content modification time (`timespec`).
pub const ATTR_CMN_MODTIME: u32 = 0x0000_0400;
/// `ATTR_CMN_CHGTIME`: metadata change time (`timespec`).
pub const ATTR_CMN_CHGTIME: u32 = 0x0000_0800;
/// `ATTR_CMN_ACCTIME`: access time (`timespec`).
pub const ATTR_CMN_ACCTIME: u32 = 0x0000_1000;
/// `ATTR_CMN_FLAGS`: BSD file flags (`u32`, as `st_flags`).
pub const ATTR_CMN_FLAGS: u32 = 0x0004_0000;
/// `ATTR_CMN_FILEID`: file ID within the volume (`u64`, as `st_ino`).
pub const ATTR_CMN_FILEID: u32 = 0x0200_0000;
/// `ATTR_CMN_ERROR`: per-entry error (`u32` errno), placed right after the
/// returned-attribute set.
pub const ATTR_CMN_ERROR: u32 = 0x2000_0000;
/// `ATTR_CMN_RETURNED_ATTRS`: the returned-attribute set; required.
pub const ATTR_CMN_RETURNED_ATTRS: u32 = 0x8000_0000;
/// `ATTR_DIR_LINKCOUNT`: hard links to a directory, excluding `.` and `..`
/// (`u32`).
pub const ATTR_DIR_LINKCOUNT: u32 = 0x0000_0001;
/// `ATTR_DIR_MOUNTSTATUS`: what is mounted on a directory (`u32`). For a mount
/// point, every other attribute describes the covered directory underneath, not
/// the mounted root.
pub const ATTR_DIR_MOUNTSTATUS: u32 = 0x0000_0004;
/// `DIR_MNTSTATUS_MNTPOINT`: a filesystem is mounted on the directory.
pub const DIR_MNTSTATUS_MNTPOINT: u32 = 0x0000_0001;
/// `ATTR_FILE_LINKCOUNT`: hard links to a file (`u32`, as `st_nlink`).
pub const ATTR_FILE_LINKCOUNT: u32 = 0x0000_0001;
/// `ATTR_FILE_TOTALSIZE`: logical size of all forks (`off_t`).
pub const ATTR_FILE_TOTALSIZE: u32 = 0x0000_0002;
/// `ATTR_FILE_ALLOCSIZE`: bytes allocated to all forks (`off_t`).
pub const ATTR_FILE_ALLOCSIZE: u32 = 0x0000_0004;
/// `ATTR_CMNEXT_PRIVATESIZE`: bytes freed immediately if the file were deleted
/// (`off_t`).
pub const ATTR_CMNEXT_PRIVATESIZE: u32 = 0x0000_0008;
/// `ATTR_CMNEXT_CLONEID`: identifies the data stream shared by clones (`u64`).
pub const ATTR_CMNEXT_CLONEID: u32 = 0x0000_0100;
/// `ATTR_CMNEXT_EXT_FLAGS`: `EF_*` flags (`u64`).
pub const ATTR_CMNEXT_EXT_FLAGS: u32 = 0x0000_0200;

/// One `attribute_set_t`: a mask per attribute group.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AttributeSet {
    /// Common attributes (`ATTR_CMN_*`).
    pub common: u32,
    /// Volume attributes; never valid for `getattrlistbulk`.
    pub volume: u32,
    /// Directory attributes (`ATTR_DIR_*`).
    pub directory: u32,
    /// File attributes (`ATTR_FILE_*`).
    pub file: u32,
    /// Extended common attributes (`ATTR_CMNEXT_*`), carried in the fork field
    /// under `FSOPT_ATTR_CMN_EXTENDED`.
    pub extended: u32,
}

impl AttributeSet {
    /// Every attribute this parser can size. A request must stay within it.
    pub const SUPPORTED: Self = Self {
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
        extended: ATTR_CMNEXT_PRIVATESIZE | ATTR_CMNEXT_CLONEID | ATTR_CMNEXT_EXT_FLAGS,
    };

    /// Whether every attribute in `self` is also in `other`.
    pub const fn is_subset_of(self, other: Self) -> bool {
        self.common & !other.common == 0
            && self.volume & !other.volume == 0
            && self.directory & !other.directory == 0
            && self.file & !other.file == 0
            && self.extended & !other.extended == 0
    }
}

/// A `timespec` as stored in the buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeSpec {
    /// Seconds since the Unix epoch.
    pub seconds: i64,
    /// Nanoseconds; not validated here.
    pub nanoseconds: i64,
}

/// One directory entry's attributes, exactly as returned. Every field is
/// `None` when the attribute was not returned; interpreting the values (object
/// types, flags, negative sizes) is the caller's job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawEntry<'a> {
    /// The attributes the kernel returned for this entry.
    pub returned: AttributeSet,
    /// The per-entry error, if one was reported.
    pub error: Option<NonZeroU32>,
    /// The name, without its terminating NUL: a single path component (non-empty,
    /// not `.` or `..`, no `/` or NUL).
    pub name: Option<&'a [u8]>,
    /// `ATTR_CMN_DEVID`.
    pub device: Option<i32>,
    /// `ATTR_CMN_OBJTYPE`.
    pub object_type: Option<u32>,
    /// `ATTR_CMN_CRTIME`.
    pub created: Option<TimeSpec>,
    /// `ATTR_CMN_MODTIME`.
    pub modified: Option<TimeSpec>,
    /// `ATTR_CMN_CHGTIME`.
    pub changed: Option<TimeSpec>,
    /// `ATTR_CMN_ACCTIME`.
    pub accessed: Option<TimeSpec>,
    /// `ATTR_CMN_FLAGS`.
    pub flags: Option<u32>,
    /// `ATTR_CMN_FILEID`.
    pub file_id: Option<u64>,
    /// `ATTR_DIR_LINKCOUNT`.
    pub directory_link_count: Option<u32>,
    /// `ATTR_DIR_MOUNTSTATUS`.
    pub mount_status: Option<u32>,
    /// `ATTR_FILE_LINKCOUNT`.
    pub file_link_count: Option<u32>,
    /// `ATTR_FILE_TOTALSIZE`.
    pub total_size: Option<i64>,
    /// `ATTR_FILE_ALLOCSIZE`.
    pub allocated_size: Option<i64>,
    /// `ATTR_CMNEXT_PRIVATESIZE`.
    pub private_size: Option<i64>,
    /// `ATTR_CMNEXT_CLONEID`.
    pub clone_id: Option<u64>,
    /// `ATTR_CMNEXT_EXT_FLAGS`.
    pub ext_flags: Option<u64>,
}

/// Why a buffer, or one entry in it, could not be parsed. Offsets are the byte
/// position of the entry in the buffer.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    /// The buffer ended before the number of entries the kernel reported.
    #[error("buffer ended at byte {offset} before all reported entries")]
    Exhausted {
        /// Where the missing entry should have started.
        offset: usize,
    },
    /// An entry's length field is too small or runs past the buffer. Nothing after
    /// it can be located.
    #[error("entry at byte {offset} has an impossible length {length}")]
    BadLength {
        /// Entry position.
        offset: usize,
        /// The length it claimed.
        length: u32,
    },
    /// An entry ends inside one of its attributes.
    #[error("entry at byte {offset} ends inside an attribute")]
    Truncated {
        /// Entry position.
        offset: usize,
    },
    /// An entry returned an attribute the parser cannot size, so the position of
    /// every later attribute is unknown.
    #[error("entry at byte {offset} returned unsupported attributes {returned:?}")]
    UnsupportedAttributes {
        /// Entry position.
        offset: usize,
        /// The returned set.
        returned: AttributeSet,
    },
    /// The name reference points outside the entry or lacks its NUL terminator.
    #[error("entry at byte {offset} has a name reference outside the entry")]
    BadReference {
        /// Entry position.
        offset: usize,
    },
    /// The name is not a single path component.
    #[error("entry at byte {offset} has a name that is not a single path component")]
    BadName {
        /// Entry position.
        offset: usize,
    },
}

impl ParseError {
    /// Whether the error makes the rest of the buffer unreachable. Other errors
    /// affect only their own entry.
    pub const fn is_fatal(&self) -> bool {
        matches!(self, Self::Exhausted { .. } | Self::BadLength { .. })
    }
}

/// Smallest possible group: the length field plus the returned-attribute set.
const MIN_GROUP: usize = 4 + 5 * 4;

/// Iterates over the `count` entries the kernel reported in `buffer`.
///
/// Yields one item per entry. After a fatal error (see
/// [`ParseError::is_fatal`]) the iterator ends, since later entries cannot be
/// located; other errors affect only their own entry.
pub fn entries(buffer: &[u8], count: usize) -> Entries<'_> {
    Entries {
        buffer,
        offset: 0,
        remaining: count,
    }
}

/// Iterator returned by [`entries`].
#[derive(Debug, Clone)]
pub struct Entries<'a> {
    buffer: &'a [u8],
    offset: usize,
    remaining: usize,
}

impl<'a> Iterator for Entries<'a> {
    type Item = Result<RawEntry<'a>, ParseError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        self.remaining -= 1;
        let offset = self.offset;
        let rest = self.buffer.get(offset..).unwrap_or_default();
        let Some(length) = rest.first_chunk::<4>().map(|b| u32::from_ne_bytes(*b)) else {
            self.remaining = 0;
            return Some(Err(ParseError::Exhausted { offset }));
        };
        let group = usize::try_from(length)
            .ok()
            .filter(|len| *len >= MIN_GROUP)
            .and_then(|len| rest.get(..len));
        let Some(group) = group else {
            self.remaining = 0;
            return Some(Err(ParseError::BadLength { offset, length }));
        };
        self.offset += group.len();
        Some(parse_group(group, offset))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, Some(self.remaining))
    }
}

/// Reads fields sequentially from one group, mapping running out of bytes to
/// [`ParseError::Truncated`].
struct Cursor<'a> {
    group: &'a [u8],
    pos: usize,
    offset: usize,
}

impl Cursor<'_> {
    fn bytes<const N: usize>(&mut self) -> Result<[u8; N], ParseError> {
        let bytes = self
            .group
            .get(self.pos..)
            .and_then(<[u8]>::first_chunk::<N>)
            .ok_or(ParseError::Truncated {
                offset: self.offset,
            })?;
        self.pos += N;
        Ok(*bytes)
    }

    fn u32(&mut self) -> Result<u32, ParseError> {
        self.bytes().map(u32::from_ne_bytes)
    }

    fn i32(&mut self) -> Result<i32, ParseError> {
        self.bytes().map(i32::from_ne_bytes)
    }

    fn u64(&mut self) -> Result<u64, ParseError> {
        self.bytes().map(u64::from_ne_bytes)
    }

    fn i64(&mut self) -> Result<i64, ParseError> {
        self.bytes().map(i64::from_ne_bytes)
    }

    fn time(&mut self) -> Result<TimeSpec, ParseError> {
        Ok(TimeSpec {
            seconds: self.i64()?,
            nanoseconds: self.i64()?,
        })
    }

    /// Reads a field only if it was returned.
    fn when<T>(
        &mut self,
        returned: bool,
        read: fn(&mut Self) -> Result<T, ParseError>,
    ) -> Result<Option<T>, ParseError> {
        if returned {
            read(self).map(Some)
        } else {
            Ok(None)
        }
    }
}

impl<'a> Cursor<'a> {
    /// Reads an `attrreference_t` and returns the bytes it points at.
    fn reference(&mut self) -> Result<&'a [u8], ParseError> {
        let at = self.pos;
        let data_offset = self.i32()?;
        let length = self.u32()?;
        let bad = ParseError::BadReference {
            offset: self.offset,
        };
        let start = isize::try_from(data_offset)
            .ok()
            .and_then(|delta| at.checked_add_signed(delta))
            .ok_or_else(|| bad.clone())?;
        let end = usize::try_from(length)
            .ok()
            .and_then(|len| start.checked_add(len))
            .ok_or_else(|| bad.clone())?;
        self.group.get(start..end).ok_or(bad)
    }
}

/// Strips the terminating NUL and checks that `data` names one path component.
fn component(data: &[u8], offset: usize) -> Result<&[u8], ParseError> {
    let name = data
        .strip_suffix(b"\0")
        .ok_or(ParseError::BadReference { offset })?;
    if name.is_empty() || name == b"." || name == b".." || name.contains(&0) || name.contains(&b'/')
    {
        return Err(ParseError::BadName { offset });
    }
    Ok(name)
}

fn parse_group(group: &[u8], offset: usize) -> Result<RawEntry<'_>, ParseError> {
    let mut c = Cursor {
        group,
        pos: 4,
        offset,
    };
    let returned = AttributeSet {
        common: c.u32()?,
        volume: c.u32()?,
        directory: c.u32()?,
        file: c.u32()?,
        extended: c.u32()?,
    };
    if !returned.is_subset_of(AttributeSet::SUPPORTED) {
        return Err(ParseError::UnsupportedAttributes { offset, returned });
    }
    let cmn = |bit: u32| returned.common & bit != 0;

    let error = c
        .when(cmn(ATTR_CMN_ERROR), Cursor::u32)?
        .and_then(NonZeroU32::new);
    let name = match c.when(cmn(ATTR_CMN_NAME), Cursor::reference)? {
        Some(data) => Some(component(data, offset)?),
        None => None,
    };
    let device = c.when(cmn(ATTR_CMN_DEVID), Cursor::i32)?;
    let object_type = c.when(cmn(ATTR_CMN_OBJTYPE), Cursor::u32)?;
    let created = c.when(cmn(ATTR_CMN_CRTIME), Cursor::time)?;
    let modified = c.when(cmn(ATTR_CMN_MODTIME), Cursor::time)?;
    let changed = c.when(cmn(ATTR_CMN_CHGTIME), Cursor::time)?;
    let accessed = c.when(cmn(ATTR_CMN_ACCTIME), Cursor::time)?;
    let flags = c.when(cmn(ATTR_CMN_FLAGS), Cursor::u32)?;
    let file_id = c.when(cmn(ATTR_CMN_FILEID), Cursor::u64)?;

    let dir = |bit: u32| returned.directory & bit != 0;
    let directory_link_count = c.when(dir(ATTR_DIR_LINKCOUNT), Cursor::u32)?;
    let mount_status = c.when(dir(ATTR_DIR_MOUNTSTATUS), Cursor::u32)?;

    let file = |bit: u32| returned.file & bit != 0;
    let file_link_count = c.when(file(ATTR_FILE_LINKCOUNT), Cursor::u32)?;
    let total_size = c.when(file(ATTR_FILE_TOTALSIZE), Cursor::i64)?;
    let allocated_size = c.when(file(ATTR_FILE_ALLOCSIZE), Cursor::i64)?;

    let ext = |bit: u32| returned.extended & bit != 0;
    let private_size = c.when(ext(ATTR_CMNEXT_PRIVATESIZE), Cursor::i64)?;
    let clone_id = c.when(ext(ATTR_CMNEXT_CLONEID), Cursor::u64)?;
    let ext_flags = c.when(ext(ATTR_CMNEXT_EXT_FLAGS), Cursor::u64)?;

    Ok(RawEntry {
        returned,
        error,
        name,
        device,
        object_type,
        created,
        modified,
        changed,
        accessed,
        flags,
        file_id,
        directory_link_count,
        mount_status,
        file_link_count,
        total_size,
        allocated_size,
        private_size,
        clone_id,
        ext_flags,
    })
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    /// Decodes a hexdump of 32-bit words, each written in memory order.
    fn words(dump: &str) -> Vec<u8> {
        dump.split_whitespace()
            .flat_map(|word| {
                (0..4).map(move |i| u8::from_str_radix(&word[i * 2..i * 2 + 2], 16).unwrap_or(0xEE))
            })
            .collect()
    }

    // Captured with getattrlistbulk on macOS 26 (APFS, arm64): a 20 KiB file
    // `big` (with PRIVATESIZE), a directory `d`, and a symlink `l` captured with
    // ATTR_CMN_ERROR present (value 0).
    const BIG: &str = "
        a8000000 0b1e0482 00000000 00000000 07000000 08030000 8c000000 04000000
        10000001 01000000 c474cc6a 00000000 d7b60028 00000000 c474cc6a 00000000
        335d0228 00000000 c474cc6a 00000000 335d0228 00000000 c474cc6a 00000000
        d7b60028 00000000 00000000 effeff07 00000000 01000000 00500000 00000000
        00500000 00000000 00500000 00000000 effeff07 00000000 00000000 00000000
        00000000 62696700";
    const DIR: &str = "
        90000000 0b1e0482 00000000 01000000 00000000 00030000 74000000 02000000
        10000001 02000000 8974cc6a 00000000 1eba8b2b 00000000 8974cc6a 00000000
        1eba8b2b 00000000 8974cc6a 00000000 75e38b2b 00000000 8974cc6a 00000000
        1eba8b2b 00000000 00000000 58fdff07 00000000 01000000 58fdff07 00000000
        00000000 00000000 00000000 64000000";
    const LINK: &str = "
        a0000000 0b1e04a2 00000000 00000000 07000000 00030000 00000000 80000000
        02000000 10000001 05000000 8974cc6a 00000000 b67ca82b 00000000 8974cc6a
        00000000 b67ca82b 00000000 8974cc6a 00000000 efaaa82b 00000000 8974cc6a
        00000000 b67ca82b 00000000 00000000 5afdff07 00000000 01000000 01000000
        00000000 00000000 00000000 5afdff07 00000000 00000000 00000000 6c000000";

    // `/System/Volumes/Preboot`, a mount point, with ATTR_DIR_MOUNTSTATUS.
    const MOUNT_POINT: &str = "
        98000000 0b1e0482 00000000 05000000 00000000 00030000 78000000 08000000
        10000001 02000000 4b317d6a 00000000 00000000 00000000 4b317d6a 00000000
        00000000 00000000 4b317d6a 00000000 00000000 00000000 4b317d6a 00000000
        00000000 00000000 00000800 64210a00 ffffff0f 01000000 01000000 64210a00
        ffffff0f 00000000 00000000 00000000 50726562 6f6f7400";

    #[cfg(target_endian = "little")]
    #[test]
    fn parses_a_mount_point_captured_from_macos() -> Result<(), ParseError> {
        let buffer = words(MOUNT_POINT);
        let entry = first(&buffer).ok_or(ParseError::Exhausted { offset: 0 })??;
        assert_eq!(entry.name, Some(&b"Preboot"[..]));
        assert_eq!(entry.object_type, Some(2), "VDIR");
        assert_eq!(entry.flags, Some(0x0008_0000), "SF_RESTRICTED");
        assert_eq!(entry.file_id, Some(0x0fff_ffff_000a_2164));
        assert_eq!(entry.directory_link_count, Some(1));
        assert_eq!(entry.mount_status, Some(DIR_MNTSTATUS_MNTPOINT));
        assert_eq!(entry.clone_id, Some(0x0fff_ffff_000a_2164));
        assert_eq!(entry.ext_flags, Some(0));
        Ok(())
    }

    #[cfg(target_endian = "little")]
    #[test]
    fn parses_buffers_captured_from_macos() -> Result<(), ParseError> {
        let buffer = [words(BIG), words(DIR), words(LINK)].concat();
        let parsed = entries(&buffer, 3).collect::<Result<Vec<_>, _>>()?;
        let [big, dir, link] = parsed.as_slice() else {
            return Err(ParseError::Exhausted { offset: 0 });
        };

        assert_eq!(big.name, Some(&b"big"[..]));
        assert_eq!(big.error, None);
        assert_eq!(big.device, Some(0x0100_0010));
        assert_eq!(big.object_type, Some(1), "VREG");
        let created = TimeSpec {
            seconds: 0x6acc_74c4,
            nanoseconds: 0x2800_b6d7,
        };
        assert_eq!(big.created, Some(created));
        assert_eq!(big.accessed, Some(created));
        let modified = TimeSpec {
            seconds: 0x6acc_74c4,
            nanoseconds: 0x2802_5d33,
        };
        assert_eq!(big.modified, Some(modified));
        assert_eq!(big.changed, Some(modified));
        assert_eq!(big.flags, Some(0));
        assert_eq!(big.file_id, Some(0x07ff_feef));
        assert_eq!(big.file_link_count, Some(1));
        assert_eq!(big.directory_link_count, None);
        assert_eq!(big.total_size, Some(20_480));
        assert_eq!(big.allocated_size, Some(20_480));
        assert_eq!(big.private_size, Some(20_480));
        assert_eq!(big.clone_id, Some(0x07ff_feef));
        assert_eq!(big.ext_flags, Some(0));

        assert_eq!(dir.name, Some(&b"d"[..]));
        assert_eq!(dir.object_type, Some(2), "VDIR");
        assert_eq!(dir.directory_link_count, Some(1));
        assert_eq!(dir.mount_status, None, "not requested in this capture");
        assert_eq!(dir.file_link_count, None);
        assert_eq!(dir.total_size, None);
        assert_eq!(dir.file_id, Some(0x07ff_fd58));

        assert_eq!(link.name, Some(&b"l"[..]));
        assert!(link.returned.common & ATTR_CMN_ERROR != 0);
        assert_eq!(link.error, None, "an error code of 0 is no error");
        assert_eq!(link.object_type, Some(5), "VLNK");
        assert_eq!(link.total_size, Some(1), "the target `f`");
        assert_eq!(link.allocated_size, Some(0));
        Ok(())
    }

    /// Builds one group the way the kernel lays it out.
    #[derive(Debug, Clone, Default)]
    struct Spec {
        error: Option<u32>,
        name: Option<Vec<u8>>,
        device: Option<i32>,
        object_type: Option<u32>,
        times: [Option<(i64, i64)>; 4],
        flags: Option<u32>,
        file_id: Option<u64>,
        directory_link_count: Option<u32>,
        mount_status: Option<u32>,
        file_link_count: Option<u32>,
        total_size: Option<i64>,
        allocated_size: Option<i64>,
        private_size: Option<i64>,
        clone_id: Option<u64>,
        ext_flags: Option<u64>,
    }

    fn put<const N: usize>(fields: &mut Vec<u8>, mask: &mut u32, bit: u32, value: Option<[u8; N]>) {
        if let Some(bytes) = value {
            *mask |= bit;
            fields.extend_from_slice(&bytes);
        }
    }

    /// Appends the common attributes; returns where the name reference sits.
    fn common_fields(spec: &Spec, set: &mut AttributeSet, f: &mut Vec<u8>) -> usize {
        put(
            f,
            &mut set.common,
            ATTR_CMN_ERROR,
            spec.error.map(u32::to_ne_bytes),
        );
        let name_ref_at = f.len();
        put(
            f,
            &mut set.common,
            ATTR_CMN_NAME,
            spec.name.as_ref().map(|_| [0; 8]),
        );
        put(
            f,
            &mut set.common,
            ATTR_CMN_DEVID,
            spec.device.map(i32::to_ne_bytes),
        );
        put(
            f,
            &mut set.common,
            ATTR_CMN_OBJTYPE,
            spec.object_type.map(u32::to_ne_bytes),
        );
        let time_bits = [
            ATTR_CMN_CRTIME,
            ATTR_CMN_MODTIME,
            ATTR_CMN_CHGTIME,
            ATTR_CMN_ACCTIME,
        ];
        for (time, bit) in spec.times.iter().zip(time_bits) {
            let bytes = time.map(|(s, n)| {
                let mut b = [0; 16];
                b[..8].copy_from_slice(&s.to_ne_bytes());
                b[8..].copy_from_slice(&n.to_ne_bytes());
                b
            });
            put(f, &mut set.common, bit, bytes);
        }
        put(
            f,
            &mut set.common,
            ATTR_CMN_FLAGS,
            spec.flags.map(u32::to_ne_bytes),
        );
        put(
            f,
            &mut set.common,
            ATTR_CMN_FILEID,
            spec.file_id.map(u64::to_ne_bytes),
        );
        name_ref_at
    }

    fn other_fields(spec: &Spec, set: &mut AttributeSet, f: &mut Vec<u8>) {
        put(
            f,
            &mut set.directory,
            ATTR_DIR_LINKCOUNT,
            spec.directory_link_count.map(u32::to_ne_bytes),
        );
        put(
            f,
            &mut set.directory,
            ATTR_DIR_MOUNTSTATUS,
            spec.mount_status.map(u32::to_ne_bytes),
        );
        put(
            f,
            &mut set.file,
            ATTR_FILE_LINKCOUNT,
            spec.file_link_count.map(u32::to_ne_bytes),
        );
        put(
            f,
            &mut set.file,
            ATTR_FILE_TOTALSIZE,
            spec.total_size.map(i64::to_ne_bytes),
        );
        put(
            f,
            &mut set.file,
            ATTR_FILE_ALLOCSIZE,
            spec.allocated_size.map(i64::to_ne_bytes),
        );
        put(
            f,
            &mut set.extended,
            ATTR_CMNEXT_PRIVATESIZE,
            spec.private_size.map(i64::to_ne_bytes),
        );
        put(
            f,
            &mut set.extended,
            ATTR_CMNEXT_CLONEID,
            spec.clone_id.map(u64::to_ne_bytes),
        );
        put(
            f,
            &mut set.extended,
            ATTR_CMNEXT_EXT_FLAGS,
            spec.ext_flags.map(u64::to_ne_bytes),
        );
    }

    fn encode(spec: &Spec) -> Vec<u8> {
        let mut set = AttributeSet {
            common: ATTR_CMN_RETURNED_ATTRS,
            ..AttributeSet::default()
        };
        let mut fields = Vec::new();
        let name_ref_at = common_fields(spec, &mut set, &mut fields);
        other_fields(spec, &mut set, &mut fields);

        let header = 4 + 20;
        if let Some(name) = &spec.name {
            let at = name_ref_at;
            let data_at = header + fields.len();
            let delta = i32::try_from(data_at - (header + at)).unwrap_or(i32::MAX);
            let length = u32::try_from(name.len() + 1).unwrap_or(u32::MAX);
            fields[at..at + 4].copy_from_slice(&delta.to_ne_bytes());
            fields[at + 4..at + 8].copy_from_slice(&length.to_ne_bytes());
            fields.extend_from_slice(name);
            fields.push(0);
        }
        while (header + fields.len()) % 8 != 0 {
            fields.push(0);
        }
        let total = u32::try_from(header + fields.len()).unwrap_or(u32::MAX);
        let mut group = total.to_ne_bytes().to_vec();
        for mask in [
            set.common,
            set.volume,
            set.directory,
            set.file,
            set.extended,
        ] {
            group.extend_from_slice(&mask.to_ne_bytes());
        }
        group.extend_from_slice(&fields);
        group
    }

    fn named(name: &[u8]) -> Spec {
        Spec {
            name: Some(name.to_vec()),
            ..Spec::default()
        }
    }

    fn first(buffer: &[u8]) -> Option<Result<RawEntry<'_>, ParseError>> {
        entries(buffer, 1).next()
    }

    #[test]
    fn rejects_names_that_are_not_one_component() {
        for bad in [&b""[..], b".", b"..", b"a/b", b"a\0b"] {
            assert_eq!(
                first(&encode(&named(bad))),
                Some(Err(ParseError::BadName { offset: 0 })),
                "{bad:?}"
            );
        }
        assert!(matches!(first(&encode(&named(b"...."))), Some(Ok(_))));
    }

    #[test]
    fn rejects_name_references_outside_the_entry() {
        let mut group = encode(&named(b"x"));
        // The reference sits right after the returned-attribute set.
        group[24..28].copy_from_slice(&1000_i32.to_ne_bytes());
        assert_eq!(
            first(&group),
            Some(Err(ParseError::BadReference { offset: 0 }))
        );
        group[24..28].copy_from_slice(&(-100_i32).to_ne_bytes());
        assert_eq!(
            first(&group),
            Some(Err(ParseError::BadReference { offset: 0 }))
        );
        let mut unterminated = encode(&named(b"x"));
        // Shrink the length so the NUL falls outside the referenced data.
        unterminated[28..32].copy_from_slice(&1_u32.to_ne_bytes());
        assert_eq!(
            first(&unterminated),
            Some(Err(ParseError::BadReference { offset: 0 }))
        );
    }

    #[test]
    fn unknown_attributes_stop_the_entry_but_not_the_buffer() {
        let mut odd = encode(&named(b"odd"));
        odd[4..8].copy_from_slice(&(ATTR_CMN_RETURNED_ATTRS | 0x0000_0004).to_ne_bytes());
        let mut volume = encode(&named(b"vol"));
        volume[8..12].copy_from_slice(&1_u32.to_ne_bytes());
        let buffer = [odd, volume, encode(&named(b"fine"))].concat();
        let parsed: Vec<_> = entries(&buffer, 3).collect();
        assert!(matches!(
            parsed[0],
            Err(ParseError::UnsupportedAttributes { offset: 0, .. })
        ));
        assert!(matches!(
            parsed[1],
            Err(ParseError::UnsupportedAttributes { .. })
        ));
        assert!(matches!(&parsed[2], Ok(e) if e.name == Some(&b"fine"[..])));
    }

    #[test]
    fn a_bad_length_ends_iteration() {
        let good = encode(&named(b"a"));
        for length in [0_u32, 23, u32::MAX] {
            let mut bad = encode(&named(b"b"));
            bad[0..4].copy_from_slice(&length.to_ne_bytes());
            let buffer = [good.clone(), bad, good.clone()].concat();
            let parsed: Vec<_> = entries(&buffer, 3).collect();
            assert_eq!(parsed.len(), 2, "length {length}");
            assert!(parsed[0].is_ok());
            assert!(matches!(&parsed[1], Err(e @ ParseError::BadLength { .. }) if e.is_fatal()));
        }
    }

    #[test]
    fn fewer_entries_than_reported_is_an_error() {
        let buffer = encode(&named(b"only"));
        let parsed: Vec<_> = entries(&buffer, 2).collect();
        assert_eq!(parsed.len(), 2);
        assert_eq!(
            parsed[1],
            Err(ParseError::Exhausted {
                offset: buffer.len()
            })
        );
        assert_eq!(entries(&[], 0).count(), 0);
    }

    #[test]
    fn a_group_too_short_for_its_attributes_is_truncated() {
        // 28 bytes: the header plus 4 bytes, but FILEID needs 8.
        let short = [
            &28_u32.to_ne_bytes()[..],
            &ATTR_CMN_FILEID.to_ne_bytes(),
            &[0; 20],
        ]
        .concat();
        assert_eq!(
            first(&short),
            Some(Err(ParseError::Truncated { offset: 0 }))
        );
    }

    fn spec() -> impl Strategy<Value = Spec> {
        let name = proptest::collection::vec(
            any::<u8>().prop_filter("not / or NUL", |b| *b != 0 && *b != b'/'),
            1..300,
        )
        .prop_filter("not . or ..", |n| n != b"." && n != b"..");
        let time = proptest::option::of(any::<(i64, i64)>());
        (
            (
                proptest::option::of(1..u32::MAX),
                proptest::option::of(name),
                any::<Option<i32>>(),
                any::<Option<u32>>(),
                [time.clone(), time.clone(), time.clone(), time],
                any::<Option<u32>>(),
                any::<Option<u64>>(),
            ),
            (
                any::<Option<u32>>(),
                any::<Option<u32>>(),
                any::<Option<i64>>(),
                any::<Option<i64>>(),
                any::<Option<i64>>(),
                any::<Option<u64>>(),
                any::<Option<u64>>(),
                any::<Option<u32>>(),
            ),
        )
            .prop_map(|(c, f)| Spec {
                error: c.0,
                name: c.1,
                device: c.2,
                object_type: c.3,
                times: c.4,
                flags: c.5,
                file_id: c.6,
                directory_link_count: f.0,
                file_link_count: f.1,
                total_size: f.2,
                allocated_size: f.3,
                private_size: f.4,
                clone_id: f.5,
                ext_flags: f.6,
                mount_status: f.7,
            })
    }

    fn matches(spec: &Spec, entry: &RawEntry<'_>) -> bool {
        let time = |t: Option<(i64, i64)>| {
            t.map(|(seconds, nanoseconds)| TimeSpec {
                seconds,
                nanoseconds,
            })
        };
        entry.error == spec.error.and_then(NonZeroU32::new)
            && entry.name == spec.name.as_deref()
            && entry.device == spec.device
            && entry.object_type == spec.object_type
            && entry.created == time(spec.times[0])
            && entry.modified == time(spec.times[1])
            && entry.changed == time(spec.times[2])
            && entry.accessed == time(spec.times[3])
            && entry.flags == spec.flags
            && entry.file_id == spec.file_id
            && entry.directory_link_count == spec.directory_link_count
            && entry.mount_status == spec.mount_status
            && entry.file_link_count == spec.file_link_count
            && entry.total_size == spec.total_size
            && entry.allocated_size == spec.allocated_size
            && entry.private_size == spec.private_size
            && entry.clone_id == spec.clone_id
            && entry.ext_flags == spec.ext_flags
    }

    proptest! {
        #[test]
        fn round_trips_every_attribute_combination(specs in proptest::collection::vec(spec(), 0..8)) {
            let buffer: Vec<u8> = specs.iter().flat_map(encode).collect();
            let parsed = entries(&buffer, specs.len()).collect::<Result<Vec<_>, _>>();
            prop_assert!(parsed.is_ok(), "{parsed:?}");
            let parsed = parsed.unwrap_or_default();
            prop_assert_eq!(parsed.len(), specs.len());
            for (spec, entry) in specs.iter().zip(&parsed) {
                prop_assert!(matches(spec, entry), "{spec:?} != {entry:?}");
            }
        }

        #[test]
        fn never_panics_and_never_yields_more_than_reported(
            buffer in proptest::collection::vec(any::<u8>(), 0..512),
            count in 0..16_usize,
        ) {
            let mut seen = 0;
            let mut fatal = false;
            for item in entries(&buffer, count) {
                prop_assert!(!fatal, "an item followed a fatal error");
                seen += 1;
                fatal = item.as_ref().err().is_some_and(ParseError::is_fatal);
            }
            prop_assert!(seen <= count);
        }

        #[test]
        fn never_panics_on_mutated_valid_buffers(
            specs in proptest::collection::vec(spec(), 1..4),
            flips in proptest::collection::vec((any::<prop::sample::Index>(), any::<u8>()), 1..8),
        ) {
            let mut buffer: Vec<u8> = specs.iter().flat_map(encode).collect();
            for (index, value) in flips {
                let at = index.index(buffer.len());
                buffer[at] = value;
            }
            prop_assert!(entries(&buffer, specs.len()).count() <= specs.len());
        }
    }
}
