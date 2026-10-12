//! Lumen's macOS platform adapter (ADR-0017).
//!
//! [`attrbuf`] parses the result buffers of `getattrlistbulk(2)` and [`entry`]
//! turns the parsed attributes into domain entries. Both are safe,
//! platform-independent code so that they are tested (and fuzzed) on every CI
//! platform; the system call wrapper that fills those buffers is macOS-only.

#![forbid(unsafe_code)]

pub mod attrbuf;
pub mod entry;
