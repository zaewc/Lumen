//! Lumen's executor: the only crate that may move or delete user files
//! (ADR-0015, ADR-0016).
//!
//! It acts on plan items, never on path strings resolved after verification.
//! Each item is re-verified by handle from its scan root without following
//! links or crossing devices; any mismatch fails closed and the item goes to
//! `REVIEW`. Moves are no-replace renames into a quarantine store on the same
//! volume, and permanent deletion only ever happens inside that store.
//!
//! This first part is read-only: [`verify`] (Unix). Moves, restores and
//! finalization follow (roadmap 9.2–9.8), then the Windows executor (9.10).

#![forbid(unsafe_code)]

#[cfg(unix)]
mod verify;

#[cfg(unix)]
pub use verify::{ExpectedLeaf, LeafKind, Mismatch, PlanItem, UnixIdentity, Verified, verify};
