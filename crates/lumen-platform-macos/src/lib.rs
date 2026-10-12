//! Lumen's macOS platform adapter (ADR-0017).
//!
//! [`attrbuf`] parses the result buffers of `getattrlistbulk(2)`. It is safe,
//! platform-independent code so that it is tested (and fuzzed) on every CI
//! platform; the system call wrapper that fills those buffers is macOS-only.

#![forbid(unsafe_code)]

pub mod attrbuf;
