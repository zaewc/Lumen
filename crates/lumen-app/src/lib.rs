//! Lumen's composition root as a library (system.md §5.1, roadmap 11.2).
//!
//! It wires the platform adapters (enumerators, probes, stores, the executor)
//! to the use cases in `lumen-application`, so that `lumen-cli` and the
//! desktop host run exactly the same core. It returns structured results and
//! typed errors; presenting them is the caller's job.

pub mod scan;
pub mod system;

pub use scan::{RootSummary, ScanError, ScanSummary, raw_path, scan};
pub use system::{SystemClock, UuidV7Ids};
