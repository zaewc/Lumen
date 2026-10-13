//! Lumen's Windows platform adapter (ADR-0017).
//!
//! [`dirinfo`] parses the `FILE_ID_EXTD_DIR_INFO` records that
//! `GetFileInformationByHandleEx` returns for a directory handle, and [`entry`]
//! turns them into domain entries. Both are safe, platform-independent code so
//! that they are tested (and fuzzed) on every CI platform; the system calls that
//! fill those buffers are Windows-only.

#![forbid(unsafe_code)]

pub mod dirinfo;
pub mod entry;
