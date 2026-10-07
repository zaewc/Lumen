//! Lumen's platform-independent domain model.
//!
//! This crate holds the types every other part of Lumen agrees on: typed
//! identifiers, filesystem entries and their size facts, inventory entities,
//! immutable evidence, relationships, policy decisions, cleanup candidates, and
//! the quarantine, scan and plan state machines.
//!
//! It is deliberately pure (ADR-0002, ADR-0003):
//!
//! - no IO, no async runtime, and no platform-specific code;
//! - no `#[cfg(target_os)]`;
//! - no `unsafe`.
//!
//! See `docs/architecture/system.md` for the model this crate implements.

#![forbid(unsafe_code)]

mod id;
mod path;
mod version;

pub use id::{
    DeviceId, FileId, FileIdentity, IdError, OperationId, PlanId, ScanId, SnapshotId, VolumeId,
};
pub use path::{PathError, PathFlavor, RawPath};
pub use version::{PolicyVersion, PromptVersion, SchemaVersion, VersionError};
