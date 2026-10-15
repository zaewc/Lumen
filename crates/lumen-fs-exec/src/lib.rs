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
//! after checking all of it first (Unix). On Windows, [`observe`] and
//! [`verify`] walk by relative native opens and [`open_store`] opens or creates
//! a store private to its owner (ADR-0033); moving, restoring and finalizing
//! follow (roadmap 9.10c–9.10d).

// `unsafe` is forbidden everywhere except the Windows system-call module
// (ADR-0033).
#![cfg_attr(not(windows), forbid(unsafe_code))]
#![cfg_attr(windows, deny(unsafe_code))]

#[cfg(unix)]
mod executor;
#[cfg(unix)]
mod fault;
#[cfg(unix)]
mod finalize;
mod location;
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
#[cfg(windows)]
mod win_move;
#[cfg(windows)]
mod win_store;
#[cfg(windows)]
mod win_sys;
#[cfg(windows)]
mod win_verify;

#[cfg(unix)]
pub use executor::Executor;
#[cfg(unix)]
pub use finalize::{FinalizeError, MAX_DEPTH, Removed, finalize};
pub use location::Left;
#[cfg(unix)]
pub use move_into::{MoveError, move_into};
#[cfg(unix)]
pub use restore::{RestoreError, Restored, restore};
#[cfg(unix)]
pub use sidecar::{MAX_SIDECAR_BYTES, SidecarError, read_sidecar, write_sidecar};
#[cfg(unix)]
pub use store::{Store, StoreError, open_store};
#[cfg(unix)]
pub use verify::{ExpectedLeaf, LeafKind, Mismatch, PlanItem, UnixIdentity, Verified, verify};
#[cfg(windows)]
pub use win_move::{Direction, WinMoveError, WinRestored, move_into, restore, write_sidecar};
#[cfg(windows)]
pub use win_store::{WinStore, WinStoreError, open_store};
#[cfg(windows)]
pub use win_verify::{
    WinExpected, WinIdentity, WinMismatch, WinPlanItem, WinVerified, observe, verify,
};
