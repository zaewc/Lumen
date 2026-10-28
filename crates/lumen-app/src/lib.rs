//! Lumen's composition root as a library (system.md §5.1, roadmap 11.2).
//!
//! It wires the platform adapters (enumerators, probes, stores, the executor)
//! to the use cases in `lumen-application`, so that `lumen-cli` and the
//! desktop host run exactly the same core. It returns structured results and
//! typed errors; presenting them is the caller's job.

pub mod ai;
pub mod assess;
#[cfg(any(unix, windows))]
pub mod held;
pub mod plan;
#[cfg(any(unix, windows))]
pub mod quarantine;
pub mod roots;
pub mod scan;
pub mod space;
#[cfg(any(unix, windows))]
pub mod store;
pub mod system;
pub mod tools;

pub use assess::{AssessError, Assessment, assess, block_on, prepare, with_probes};
pub use plan::{PlanError, Planned, plan};
pub use scan::{RootSummary, ScanError, ScanEvent, ScanSummary, raw_path, scan};
#[cfg(any(unix, windows))]
pub use store::{
    LocationError, PrivilegeError, StoreLocation, Stores, refuse_privileged, store_location, stores,
};
pub use system::{SystemClock, UuidV7Ids};
