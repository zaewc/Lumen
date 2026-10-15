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
//! stores its manifest next to it, and [`restore`] moves it back without
//! replacing anything, and [`finalize`] permanently removes it from the store
//! after checking all of it first (Unix). The Windows executor follows
//! (roadmap 9.10).

#![forbid(unsafe_code)]

#[cfg(unix)]
mod executor;
#[cfg(unix)]
mod fault;
#[cfg(unix)]
mod finalize;
#[cfg(unix)]
mod move_into;
#[cfg(unix)]
mod restore;
#[cfg(unix)]
mod sidecar;
#[cfg(unix)]
mod store;
#[cfg(unix)]
mod verify;

#[cfg(unix)]
pub use executor::Executor;
#[cfg(unix)]
pub use finalize::{FinalizeError, MAX_DEPTH, Removed, finalize};
#[cfg(unix)]
pub use move_into::{Left, MoveError, move_into};
#[cfg(unix)]
pub use restore::{RestoreError, Restored, restore};
#[cfg(unix)]
pub use sidecar::{MAX_SIDECAR_BYTES, SidecarError, read_sidecar, write_sidecar};
#[cfg(unix)]
pub use store::{Store, StoreError, open_store};
#[cfg(unix)]
pub use verify::{ExpectedLeaf, LeafKind, Mismatch, PlanItem, UnixIdentity, Verified, verify};
