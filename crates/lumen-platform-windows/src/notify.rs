//! `ReadDirectoryChangesW` results: parsing `FILE_NOTIFY_INFORMATION` records and
//! turning them into rescans (ADR-0018).
//!
//! Each record is `NextEntryOffset: u32`, `Action: u32`, `FileNameLength: u32`
//! (bytes), then the UTF-16 name relative to the watched root (it may contain
//! `\` for entries in subdirectories). Pure and platform-independent, so it is
//! tested on every CI platform; the watching itself is Windows-only.

use lumen_application::ports::Change;
use lumen_domain::RawPath;

use crate::entry::join;

/// Size of a record before its name.
pub const HEADER: usize = 12;

// `FILE_ACTION_*` (`winnt.h`, as in `windows-sys` 0.61).
const FILE_ACTION_ADDED: u32 = 1;
const FILE_ACTION_REMOVED: u32 = 2;
const FILE_ACTION_MODIFIED: u32 = 3;
const FILE_ACTION_RENAMED_OLD_NAME: u32 = 4;
const FILE_ACTION_RENAMED_NEW_NAME: u32 = 5;

const BACKSLASH: u16 = b'\\' as u16;

/// One notification, exactly as returned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    /// `FILE_ACTION_*`.
    pub action: u32,
    /// Path relative to the watched root, UTF-16.
    pub name: Vec<u16>,
}

/// The buffer did not hold a well-formed chain of records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("malformed change notification at byte {offset}")]
pub struct Malformed {
    /// Record position.
    pub offset: usize,
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    let chunk = bytes.get(at..)?.first_chunk::<4>()?;
    Some(u32::from_le_bytes(*chunk))
}

/// Parses the records in the first `len` bytes of a result buffer.
///
/// # Errors
///
/// Returns [`Malformed`] if a record runs past the data, its name has an odd
/// length or a bad component, or the chain does not move forward. The caller
/// must then rescan the whole root: some changes are unknown.
pub fn notices(buffer: &[u8]) -> Result<Vec<Notice>, Malformed> {
    let mut out = Vec::new();
    let mut offset = 0;
    loop {
        let bad = Malformed { offset };
        let next = u32_at(buffer, offset).ok_or(bad)?;
        let action = u32_at(buffer, offset + 4).ok_or(bad)?;
        let name_bytes =
            usize::try_from(u32_at(buffer, offset + 8).ok_or(bad)?).map_err(|_| bad)?;
        let start = offset + HEADER;
        let raw = start
            .checked_add(name_bytes)
            .and_then(|end| buffer.get(start..end))
            .ok_or(bad)?;
        let (units, odd) = raw.as_chunks::<2>();
        if !odd.is_empty() {
            return Err(bad);
        }
        let name: Vec<u16> = units.iter().map(|u| u16::from_le_bytes(*u)).collect();
        if !is_relative_path(&name) {
            return Err(bad);
        }
        out.push(Notice { action, name });
        if next == 0 {
            return Ok(out);
        }
        let step = usize::try_from(next).map_err(|_| bad)?;
        if step < HEADER + name_bytes {
            return Err(bad);
        }
        offset += step;
    }
}

/// Non-empty, no NUL or `/`, and no empty, `.` or `..` component.
fn is_relative_path(name: &[u16]) -> bool {
    let dot = u16::from(b'.');
    !name.is_empty()
        && !name.contains(&0)
        && !name.contains(&u16::from(b'/'))
        && name
            .split(|u| *u == BACKSLASH)
            .all(|part| !part.is_empty() && part != [dot] && part != [dot, dot])
}

/// The rescans a batch of notices under `root` calls for. Every notice rescans
/// the directory containing the changed name; a name renamed in may be a whole
/// directory, so its subtree is rescanned too. Unknown actions rescan the root.
pub fn changes(root: &[u16], notices: &[Notice]) -> Vec<Change> {
    let mut out = Vec::new();
    for notice in notices {
        let full = join(root, &notice.name);
        let parent_len = notice
            .name
            .iter()
            .rposition(|u| *u == BACKSLASH)
            .map_or(root.len(), |cut| full.len() - (notice.name.len() - cut));
        let path = |units: &[u16]| RawPath::from_windows_wide(units).ok();
        match notice.action {
            FILE_ACTION_ADDED
            | FILE_ACTION_REMOVED
            | FILE_ACTION_MODIFIED
            | FILE_ACTION_RENAMED_OLD_NAME => {
                out.extend(path(&full[..parent_len]).map(Change::Directory));
            }
            FILE_ACTION_RENAMED_NEW_NAME => {
                out.extend(path(&full[..parent_len]).map(Change::Directory));
                out.extend(path(&full).map(Change::RescanSubtree));
            }
            _ => out.extend(path(root).map(Change::RescanSubtree)),
        }
    }
    out
}

/// What an overflowed or failed read means: everything under `root` may have
/// changed (`ERROR_NOTIFY_ENUM_DIR`, a zero-byte result, or a malformed buffer).
pub fn rescan_root(root: &[u16]) -> Option<Change> {
    RawPath::from_windows_wide(root)
        .ok()
        .map(Change::RescanSubtree)
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    fn encode(notices: &[(u32, &str)]) -> Vec<u8> {
        let mut buffer = Vec::new();
        for (i, (action, name)) in notices.iter().enumerate() {
            let start = buffer.len();
            let name: Vec<u8> = wide(name).iter().flat_map(|u| u.to_le_bytes()).collect();
            let len = (HEADER + name.len()).next_multiple_of(4);
            let next = if i + 1 == notices.len() { 0 } else { len };
            buffer.extend_from_slice(&u32::try_from(next).unwrap_or(0).to_le_bytes());
            buffer.extend_from_slice(&action.to_le_bytes());
            buffer.extend_from_slice(&u32::try_from(name.len()).unwrap_or(0).to_le_bytes());
            buffer.extend_from_slice(&name);
            buffer.resize(start + len, 0);
        }
        buffer
    }

    fn path(s: &str) -> RawPath {
        RawPath::from_windows_wide(&wide(s)).unwrap_or_else(|_| unreachable!())
    }

    #[test]
    fn parses_a_chain_of_notices() -> Result<(), Malformed> {
        let parsed = notices(&encode(&[(1, "a.txt"), (3, r"sub\b.txt"), (5, "새 폴더")]))?;
        assert_eq!(
            parsed,
            [
                Notice {
                    action: 1,
                    name: wide("a.txt")
                },
                Notice {
                    action: 3,
                    name: wide(r"sub\b.txt")
                },
                Notice {
                    action: 5,
                    name: wide("새 폴더")
                },
            ]
        );
        Ok(())
    }

    #[test]
    fn rejects_malformed_buffers() {
        for bad in ["", r"..\x", r"a\.\b", r"a\\b", "a/b", "a\0b"] {
            assert!(notices(&encode(&[(1, bad)])).is_err(), "{bad:?}");
        }
        let buffer = encode(&[(1, "a"), (1, "b")]);
        let mut backwards = buffer.clone();
        backwards[0..4].copy_from_slice(&4_u32.to_le_bytes());
        assert_eq!(notices(&backwards), Err(Malformed { offset: 0 }));
        // The last record is 12 header bytes, a 2-byte name and 2 padding bytes:
        // dropping 3 bytes cuts into the name.
        assert!(notices(&buffer[..buffer.len() - 3]).is_err());
        assert!(notices(&[]).is_err());
    }

    #[test]
    fn notices_rescan_the_containing_directory() {
        let root = wide(r"C:\data");
        let got = changes(
            &root,
            &[
                Notice {
                    action: FILE_ACTION_ADDED,
                    name: wide("a.txt"),
                },
                Notice {
                    action: FILE_ACTION_MODIFIED,
                    name: wide(r"sub\deep\b.txt"),
                },
                Notice {
                    action: FILE_ACTION_REMOVED,
                    name: wide(r"sub\c"),
                },
                Notice {
                    action: FILE_ACTION_RENAMED_OLD_NAME,
                    name: wide("old"),
                },
            ],
        );
        assert_eq!(
            got,
            [
                Change::Directory(path(r"C:\data")),
                Change::Directory(path(r"C:\data\sub\deep")),
                Change::Directory(path(r"C:\data\sub")),
                Change::Directory(path(r"C:\data")),
            ]
        );
    }

    #[test]
    fn renamed_in_names_rescan_their_subtree_and_unknown_actions_the_root() {
        let root = wide(r"C:\data");
        let got = changes(
            &root,
            &[
                Notice {
                    action: FILE_ACTION_RENAMED_NEW_NAME,
                    name: wide(r"sub\moved"),
                },
                Notice {
                    action: 99,
                    name: wide("x"),
                },
            ],
        );
        assert_eq!(
            got,
            [
                Change::Directory(path(r"C:\data\sub")),
                Change::RescanSubtree(path(r"C:\data\sub\moved")),
                Change::RescanSubtree(path(r"C:\data")),
            ]
        );
        assert_eq!(
            rescan_root(&root),
            Some(Change::RescanSubtree(path(r"C:\data")))
        );
    }

    proptest! {
        #[test]
        fn parsing_never_panics(buffer in proptest::collection::vec(any::<u8>(), 0..512)) {
            let _parsed = notices(&buffer);
        }
    }
}
