//! Lumen's macOS platform adapter (ADR-0017).
//!
//! [`attrbuf`] parses the result buffers of `getattrlistbulk(2)` and [`entry`]
//! turns the parsed attributes into domain entries. Both are safe,
//! platform-independent code so that they are tested (and fuzzed) on every CI
//! platform. On macOS, [`BulkEnumerator`] lists directories with them; its only
//! `unsafe` code is the three system calls in `sys`.

pub mod attrbuf;
#[cfg(target_os = "macos")]
mod bulk;
pub mod entry;
#[cfg(target_os = "macos")]
mod sys;

#[cfg(target_os = "macos")]
pub use bulk::BulkEnumerator;
