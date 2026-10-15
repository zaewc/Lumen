//! Where a failed operation left an item, on every platform.

/// Where an item is after a failed move.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Left {
    /// Untouched at its original location.
    AtSource,
    /// In the quarantine store, restorable.
    InStore,
}
