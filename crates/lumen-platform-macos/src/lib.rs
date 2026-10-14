//! Lumen's macOS platform adapter (ADR-0017).
//!
//! [`attrbuf`] parses the result buffers of `getattrlistbulk(2)` and [`entry`]
//! turns the parsed attributes into domain entries. Both are safe,
//! platform-independent code so that they are tested (and fuzzed) on every CI
//! platform. On macOS, [`BulkEnumerator`] lists directories with them, and
//! [`FsEventsFeed`] reports changes through `FSEvents` after [`events`] decides
//! what each event means. The only `unsafe` code is the system calls in `sys`,
//! the stream lifecycle in `feed`, and the framework calls in `keychain` and
//! `backup` (which excludes quarantine stores from Time Machine).

#[cfg(target_os = "macos")]
mod apps;
pub mod attrbuf;
#[cfg(target_os = "macos")]
mod backup;
#[cfg(target_os = "macos")]
mod bulk;
#[cfg(target_os = "macos")]
mod cf;
pub mod entry;
pub mod events;
#[cfg(target_os = "macos")]
mod feed;
#[cfg(target_os = "macos")]
mod keychain;
#[cfg(target_os = "macos")]
mod launchd;
#[cfg(target_os = "macos")]
mod probe;
pub mod support;
#[cfg(target_os = "macos")]
mod sys;

#[cfg(target_os = "macos")]
pub use apps::AppInventory;
#[cfg(target_os = "macos")]
pub use backup::{BackupExclusionError, exclude_from_backup};
#[cfg(target_os = "macos")]
pub use bulk::BulkEnumerator;
#[cfg(target_os = "macos")]
pub use feed::FsEventsFeed;
#[cfg(target_os = "macos")]
pub use keychain::{KeychainStore, SERVICE as KEYCHAIN_SERVICE};
#[cfg(target_os = "macos")]
pub use launchd::LaunchdInventory;
#[cfg(target_os = "macos")]
pub use probe::ProcessProbe;
