//! Lumen's executor: the only crate that may move or delete user files
//! (ADR-0015, ADR-0016).
//!
//! It acts on plan items, never on path strings resolved after verification.
//! Each item is re-verified by handle from its scan root without following
//! links or crossing devices; any mismatch fails closed and the item goes to
//! `REVIEW`. Moves are no-replace renames into a quarantine store on the same
//! volume, and permanent deletion only ever happens inside that store.
//!
//! So far: [`verify`] re-checks a plan item by handle, and [`open_store`] opens
//! or creates a private, same-volume quarantine store, and [`move_into`] renames
//! a verified item into it without replacing anything, and [`write_sidecar`]
//! stores its manifest next to it (Unix). Restores and finalization follow
//! (roadmap 9.7–9.8), then the Windows executor (9.10).

#![forbid(unsafe_code)]

#[cfg(unix)]
mod move_into;
#[cfg(unix)]
mod sidecar;
#[cfg(unix)]
mod store;
#[cfg(unix)]
mod verify;

#[cfg(unix)]
pub use move_into::{Left, MoveError, move_into};
#[cfg(unix)]
pub use sidecar::{MAX_SIDECAR_BYTES, SidecarError, read_sidecar, write_sidecar};
#[cfg(unix)]
pub use store::{Store, StoreError, open_store};
#[cfg(unix)]
pub use verify::{ExpectedLeaf, LeafKind, Mismatch, PlanItem, UnixIdentity, Verified, verify};
