//! Where a failed operation left an item, on every platform.

/// Where an item is after a failed move.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Left {
    /// Untouched at its original location.
    AtSource,
    /// In the quarantine store, restorable.
    InStore,
}

/// How much a failed finalization removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Removed {
    /// Nothing; the item is intact in the store.
    Nothing,
    /// Possibly part of the item; finalizing again finishes it.
    Partly,
    /// The whole item; only its sidecar remains.
    Item,
}
