//! Parser for `FILE_ID_EXTD_DIR_INFO` buffers.
//!
//! `GetFileInformationByHandleEx(FileIdExtdDirectoryInfo)` fills a buffer with a
//! chain of records ([`FILE_ID_EXTD_DIR_INFO`], `winbase.h`):
//!
//! | Offset | Field | Type |
//! | --- | --- | --- |
//! | 0 | `NextEntryOffset` | `u32`, 0 on the last record |
//! | 4 | `FileIndex` | `u32` (ignored) |
//! | 8, 16, 24, 32 | `CreationTime`, `LastAccessTime`, `LastWriteTime`, `ChangeTime` | `i64` `FILETIME` ticks |
//! | 40 | `EndOfFile` | `i64` |
//! | 48 | `AllocationSize` | `i64` |
//! | 56 | `FileAttributes` | `u32` |
//! | 60 | `FileNameLength` | `u32`, in bytes |
//! | 64 | `EaSize` | `u32` (ignored) |
//! | 68 | `ReparsePointTag` | `u32` |
//! | 72 | `FileId` | `FILE_ID_128`, 16 bytes |
//! | 88 | `FileName` | UTF-16, not NUL-terminated |
//!
//! The offsets match the `windows-sys` 0.61 definition. The buffer is kernel
//! output, but it is still treated as untrusted input: every offset and length is
//! bounds-checked, and a record that does not fit stops parsing.
//!
//! [`FILE_ID_EXTD_DIR_INFO`]: https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_id_extd_dir_info

/// Size of the fixed part of a record, before the name.
pub const HEADER: usize = 88;

/// One directory record, exactly as returned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirRecord {
    /// `CreationTime` (`FILETIME` ticks).
    pub created: i64,
    /// `LastAccessTime`.
    pub accessed: i64,
    /// `LastWriteTime`.
    pub written: i64,
    /// `ChangeTime` (metadata change).
    pub changed: i64,
    /// `EndOfFile`: logical size.
    pub end_of_file: i64,
    /// `AllocationSize`: bytes allocated.
    pub allocation_size: i64,
    /// `FileAttributes` (`FILE_ATTRIBUTE_*`).
    pub attributes: u32,
    /// `ReparsePointTag`, meaningful when `FILE_ATTRIBUTE_REPARSE_POINT` is set.
    pub reparse_tag: u32,
    /// `FileId`: the 128-bit file ID (little-endian bytes as a number).
    pub file_id: u128,
    /// The name as UTF-16 code units; may be `.` or `..`, which callers skip.
    pub name: Vec<u16>,
}

impl DirRecord {
    /// Whether this is the `.` or `..` record that NTFS lists in every directory.
    pub fn is_dot(&self) -> bool {
        let dot = u16::from(b'.');
        self.name == [dot] || self.name == [dot, dot]
    }
}

/// Why a buffer could not be parsed. Offsets are byte positions in the buffer.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    /// A record, or its name, runs past the end of the buffer.
    #[error("record at byte {offset} runs past the buffer")]
    Truncated {
        /// Record position.
        offset: usize,
    },
    /// `NextEntryOffset` points inside the current record. Nothing after it can be
    /// located.
    #[error("record at byte {offset} has an impossible next-entry offset {next}")]
    BadNext {
        /// Record position.
        offset: usize,
        /// The offset it claimed.
        next: u32,
    },
    /// The name has an odd byte length, or is not a single path component (empty,
    /// or containing `\`, `/` or NUL).
    #[error("record at byte {offset} has an invalid name")]
    BadName {
        /// Record position.
        offset: usize,
    },
}

/// Iterates over the records in `buffer`, following `NextEntryOffset`.
///
/// The iterator ends after the record whose `NextEntryOffset` is 0, or after the
/// first error: an error stops it because the chain cannot be trusted any more.
pub fn records(buffer: &[u8]) -> Records<'_> {
    Records {
        buffer,
        offset: Some(0),
    }
}

/// Iterator returned by [`records`].
#[derive(Debug, Clone)]
pub struct Records<'a> {
    buffer: &'a [u8],
    offset: Option<usize>,
}

impl Iterator for Records<'_> {
    type Item = Result<DirRecord, ParseError>;

    fn next(&mut self) -> Option<Self::Item> {
        let offset = self.offset.take()?;
        let result = parse(self.buffer, offset);
        if let Ok((_, Some(next))) = result {
            self.offset = Some(next);
        }
        Some(result.map(|(record, _)| record))
    }
}

fn u32_at(record: &[u8], at: usize) -> u32 {
    let mut bytes = [0; 4];
    bytes.copy_from_slice(&record[at..at + 4]);
    u32::from_le_bytes(bytes)
}

fn i64_at(record: &[u8], at: usize) -> i64 {
    let mut bytes = [0; 8];
    bytes.copy_from_slice(&record[at..at + 8]);
    i64::from_le_bytes(bytes)
}

/// Parses the record at `offset`; returns it and the next record's offset.
fn parse(buffer: &[u8], offset: usize) -> Result<(DirRecord, Option<usize>), ParseError> {
    let truncated = ParseError::Truncated { offset };
    let header = buffer
        .get(offset..)
        .and_then(|rest| rest.get(..HEADER))
        .ok_or(truncated.clone())?;
    // Every index below is within the HEADER bytes just checked.
    let next = u32_at(header, 0);
    let name_bytes = usize::try_from(u32_at(header, 60)).map_err(|_| truncated.clone())?;
    let name_start = offset + HEADER;
    let name = name_start
        .checked_add(name_bytes)
        .and_then(|end| buffer.get(name_start..end))
        .ok_or(truncated)?;
    let (units, odd) = name.as_chunks::<2>();
    if !odd.is_empty() {
        return Err(ParseError::BadName { offset });
    }
    let name: Vec<u16> = units.iter().map(|u| u16::from_le_bytes(*u)).collect();
    let forbidden = [0, u16::from(b'\\'), u16::from(b'/')];
    if name.is_empty() || name.iter().any(|u| forbidden.contains(u)) {
        return Err(ParseError::BadName { offset });
    }
    let mut file_id = [0; 16];
    file_id.copy_from_slice(&header[72..88]);

    let following = match next {
        0 => None,
        // The next record must start after this one's fixed part and name, so
        // the chain always moves forward and cannot loop.
        n => {
            let step = usize::try_from(n).map_err(|_| ParseError::BadNext { offset, next })?;
            if step < HEADER + name_bytes {
                return Err(ParseError::BadNext { offset, next });
            }
            Some(offset + step)
        }
    };
    Ok((
        DirRecord {
            created: i64_at(header, 8),
            accessed: i64_at(header, 16),
            written: i64_at(header, 24),
            changed: i64_at(header, 32),
            end_of_file: i64_at(header, 40),
            allocation_size: i64_at(header, 48),
            attributes: u32_at(header, 56),
            reparse_tag: u32_at(header, 68),
            file_id: u128::from_le_bytes(file_id),
            name,
        },
        following,
    ))
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    fn record(name: &str) -> DirRecord {
        DirRecord {
            created: 133_000_000_000_000_000,
            accessed: 133_000_000_000_000_001,
            written: 133_000_000_000_000_002,
            changed: 133_000_000_000_000_003,
            end_of_file: 5,
            allocation_size: 8192,
            attributes: 0x20,
            reparse_tag: 0,
            file_id: 0x0001_0000_0000_0000_0000_0000_0000_002a,
            name: wide(name),
        }
    }

    /// Lays out records as the kernel does: each padded to 8 bytes, chained by
    /// `NextEntryOffset`, the last with 0.
    fn encode(records: &[DirRecord]) -> Vec<u8> {
        let mut buffer = Vec::new();
        for (i, r) in records.iter().enumerate() {
            let start = buffer.len();
            let name: Vec<u8> = r.name.iter().flat_map(|u| u.to_le_bytes()).collect();
            let mut len = HEADER + name.len();
            len = len.next_multiple_of(8);
            let next = if i + 1 == records.len() { 0 } else { len };
            buffer.extend_from_slice(&u32::try_from(next).unwrap_or(0).to_le_bytes());
            buffer.extend_from_slice(&7_u32.to_le_bytes());
            for v in [r.created, r.accessed, r.written, r.changed] {
                buffer.extend_from_slice(&v.to_le_bytes());
            }
            buffer.extend_from_slice(&r.end_of_file.to_le_bytes());
            buffer.extend_from_slice(&r.allocation_size.to_le_bytes());
            buffer.extend_from_slice(&r.attributes.to_le_bytes());
            buffer.extend_from_slice(&u32::try_from(name.len()).unwrap_or(0).to_le_bytes());
            buffer.extend_from_slice(&0_u32.to_le_bytes());
            buffer.extend_from_slice(&r.reparse_tag.to_le_bytes());
            buffer.extend_from_slice(&r.file_id.to_le_bytes());
            buffer.extend_from_slice(&name);
            buffer.resize(start + len, 0);
        }
        buffer
    }

    #[test]
    fn follows_the_chain_and_reads_every_field() -> Result<(), ParseError> {
        let input = [record("."), record(".."), record("a.txt"), record("한글")];
        let parsed = records(&encode(&input)).collect::<Result<Vec<_>, _>>()?;
        assert_eq!(parsed, input);
        assert!(parsed[0].is_dot() && parsed[1].is_dot());
        assert!(!parsed[2].is_dot());
        Ok(())
    }

    #[test]
    fn rejects_names_that_are_not_one_component() {
        for bad in ["", "a\\b", "a/b", "a\0b"] {
            assert_eq!(
                records(&encode(&[record(bad)])).next(),
                Some(Err(ParseError::BadName { offset: 0 })),
                "{bad:?}"
            );
        }
        let mut odd = encode(&[record("ab")]);
        odd[60..64].copy_from_slice(&3_u32.to_le_bytes());
        assert_eq!(
            records(&odd).next(),
            Some(Err(ParseError::BadName { offset: 0 }))
        );
    }

    #[test]
    fn a_backward_or_overlapping_next_offset_stops_iteration() {
        let mut buffer = encode(&[record("a"), record("b")]);
        for next in [1_u32, 40, 89] {
            buffer[0..4].copy_from_slice(&next.to_le_bytes());
            let parsed: Vec<_> = records(&buffer).collect();
            assert_eq!(
                parsed,
                [Err(ParseError::BadNext { offset: 0, next })],
                "next {next}"
            );
        }
    }

    #[test]
    fn records_and_names_past_the_buffer_are_truncated() {
        let buffer = encode(&[record("name")]);
        assert_eq!(
            records(&buffer[..HEADER - 1]).next(),
            Some(Err(ParseError::Truncated { offset: 0 }))
        );
        assert_eq!(
            records(&buffer[..HEADER + 2]).next(),
            Some(Err(ParseError::Truncated { offset: 0 }))
        );
        let mut chained = encode(&[record("a"), record("b")]);
        chained.truncate(100);
        let parsed: Vec<_> = records(&chained).collect();
        assert!(parsed[0].is_ok());
        assert_eq!(parsed[1], Err(ParseError::Truncated { offset: 96 }));
        assert_eq!(parsed.len(), 2);
        assert_eq!(
            records(&[]).count(),
            1,
            "an empty buffer is one truncated record"
        );
    }

    fn arbitrary_record() -> impl Strategy<Value = DirRecord> {
        let name = proptest::collection::vec(
            any::<u16>().prop_filter("one component", |u| ![0, 0x5C, 0x2F].contains(u)),
            1..256,
        );
        (
            any::<[i64; 6]>(),
            any::<u32>(),
            any::<u32>(),
            any::<u128>(),
            name,
        )
            .prop_map(|(t, attributes, reparse_tag, file_id, name)| DirRecord {
                created: t[0],
                accessed: t[1],
                written: t[2],
                changed: t[3],
                end_of_file: t[4],
                allocation_size: t[5],
                attributes,
                reparse_tag,
                file_id,
                name,
            })
    }

    proptest! {
        #[test]
        fn round_trips(input in proptest::collection::vec(arbitrary_record(), 1..10)) {
            let parsed = records(&encode(&input)).collect::<Result<Vec<_>, _>>();
            prop_assert_eq!(parsed, Ok(input));
        }

        #[test]
        fn never_panics_and_always_ends(buffer in proptest::collection::vec(any::<u8>(), 0..1024)) {
            // Each record moves at least HEADER bytes forward, so this bounds the count.
            prop_assert!(records(&buffer).count() <= buffer.len() / HEADER + 1);
        }

        #[test]
        fn never_panics_on_mutated_valid_buffers(
            input in proptest::collection::vec(arbitrary_record(), 1..4),
            flips in proptest::collection::vec((any::<prop::sample::Index>(), any::<u8>()), 1..8),
        ) {
            let mut buffer = encode(&input);
            for (index, value) in flips {
                let at = index.index(buffer.len());
                buffer[at] = value;
            }
            prop_assert!(records(&buffer).count() <= buffer.len() / HEADER + 1);
        }
    }
}
