//! Lumen's Windows platform adapter (ADR-0017).
//!
//! [`dirinfo`] parses the `FILE_ID_EXTD_DIR_INFO` records that
//! `GetFileInformationByHandleEx` returns for a directory handle, and [`entry`]
//! turns them into domain entries. Both are safe, platform-independent code so
//! that they are tested (and fuzzed) on every CI platform. On Windows,
//! [`HandleEnumerator`] lists directories with them; its only `unsafe` code is
//! the Win32 calls in `sys`.

pub mod dirinfo;
pub mod entry;
#[cfg(windows)]
mod handle;
pub mod notify;
#[cfg(windows)]
mod sys;
pub mod winpath;

#[cfg(windows)]
pub use handle::HandleEnumerator;
