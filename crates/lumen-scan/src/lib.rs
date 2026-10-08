//! Lumen's scan engine (ADR-0017).
//!
//! [`scan`] runs a parallel breadth-first traversal over any
//! [`DirEnumerator`](lumen_application::ports::DirEnumerator) with bounded
//! backpressure. [`StdFsEnumerator`] is a portable
//! [`DirEnumerator`](lumen_application::ports::DirEnumerator) built on `std::fs`.
//! It is the reference implementation used by tests and a fallback on Unix-like
//! systems; the native batch enumerators (`getattrlistbulk`, directory handles)
//! arrive with the platform adapters.
//!
//! The portable enumerator is Unix-only: stable `std` exposes no file identity on
//! Windows, and Lumen keys every decision on identity.

#![forbid(unsafe_code)]

mod coverage;
mod scheduler;
#[cfg(unix)]
mod std_fs;

pub use coverage::CoverageBuilder;
pub use scheduler::{
    ScanConfig, ScanItem, ScanProgress, ScanSummary, SkipReason, StopReason, scan,
};

#[cfg(unix)]
pub use std_fs::StdFsEnumerator;
