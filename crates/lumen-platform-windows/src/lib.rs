//! Lumen's Windows platform adapter (ADR-0017).
//!
//! [`dirinfo`] parses the `FILE_ID_EXTD_DIR_INFO` records that
//! `GetFileInformationByHandleEx` returns for a directory handle, and [`entry`]
//! turns them into domain entries. Both are safe, platform-independent code so
//! that they are tested (and fuzzed) on every CI platform. On Windows,
//! [`HandleEnumerator`] lists directories with them; its only `unsafe` code is
//! the Win32 calls in `sys`.

pub mod apps;
pub mod dirinfo;
pub mod entry;
#[cfg(windows)]
mod handle;
#[cfg(windows)]
mod installed;
pub mod notify;
#[cfg(windows)]
mod registry;
pub mod startup;
#[cfg(windows)]
mod sys;
#[cfg(windows)]
mod watcher;
pub mod winpath;

#[cfg(windows)]
pub use handle::HandleEnumerator;
#[cfg(windows)]
pub use installed::InstalledApps;
#[cfg(windows)]
pub use startup::StartupEntries;
#[cfg(windows)]
pub use watcher::DirectoryChangesFeed;
