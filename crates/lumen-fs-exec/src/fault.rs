//! Test-only fault points (ADR-0032).
//!
//! `check(point)` follows a fallible system call and, in this crate's unit
//! tests only, fails as if that call had failed after taking effect. Outside
//! `cfg(test)` it is a `const fn` returning `Ok(())`: no build of Lumen can arm
//! a fault.

use rustix::io::Errno;

/// A fallible step that a test can make fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Point {
    /// Flushing the directories after moving into the store.
    MoveSync,
    /// Checking that the source name is gone after the move.
    MoveCheck,
    /// Writing the manifest sidecar.
    SidecarWrite,
    /// Flushing the directories after restoring.
    RestoreSync,
    /// Checking that the store name is gone after restoring.
    RestoreCheck,
    /// Removing an entry inside a directory being finalized.
    FinalizeUnlink,
    /// Removing the sidecar after the item.
    FinalizeSidecar,
}

#[cfg(test)]
thread_local! {
    static ARMED: std::cell::Cell<Option<Point>> = const { std::cell::Cell::new(None) };
}

/// Makes `point` fail on this thread until disarmed.
#[cfg(test)]
pub(crate) fn arm(point: Option<Point>) {
    ARMED.with(|armed| armed.set(point));
}

/// Fails if a test armed `point` on this thread.
#[cfg(test)]
pub(crate) fn check(point: Point) -> Result<(), Errno> {
    if ARMED.with(std::cell::Cell::get) == Some(point) {
        Err(Errno::IO)
    } else {
        Ok(())
    }
}

/// Never fails outside tests.
#[cfg(not(test))]
#[allow(
    clippy::inline_always,
    clippy::unnecessary_wraps,
    reason = "the same signature as the test build, compiled away entirely"
)]
#[inline(always)]
pub(crate) const fn check(_point: Point) -> Result<(), Errno> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::finalize::{FinalizeError, Removed, finalize};
    use crate::move_into::tests::{plan, setup};
    use crate::move_into::{Left, MoveError, move_into};
    use crate::restore::{RestoreError, restore};
    use crate::sidecar::write_sidecar;
    use crate::store::Store;
    use crate::verify::{PlanItem, verify};

    /// Disarms on drop, so a failing assertion cannot leak a fault.
    struct Armed;

    impl Armed {
        fn at(point: Point) -> Self {
            arm(Some(point));
            Self
        }
    }

    impl Drop for Armed {
        fn drop(&mut self) {
            arm(None);
        }
    }

    fn quarantined(root: &Path, store: &Store, relative: &str, name: &str) -> PlanItem {
        let item = plan(root, relative);
        let verified = verify(&item).unwrap_or_else(|e| unreachable!("{e}"));
        move_into(&verified, store, name.as_bytes(), &item.expected)
            .unwrap_or_else(|e| unreachable!("{e}"));
        item
    }

    #[test]
    fn faults_after_the_move_report_the_item_in_the_store() {
        for point in [Point::MoveSync, Point::MoveCheck] {
            let (_fx, root, store) = setup();
            let item = plan(&root, "cache/app/data.bin");
            let verified = verify(&item).unwrap_or_else(|e| unreachable!("{e}"));
            let result = {
                let _armed = Armed::at(point);
                move_into(&verified, &store, b"item-1", &item.expected)
            };
            assert!(matches!(
                result,
                Err(MoveError::Sync(_) | MoveError::SourceRemains)
            ));
            assert_eq!(
                result.err().map(|e| e.left()),
                Some(Left::InStore),
                "{point:?}"
            );
            assert!(root.join("Quarantine/item-1").exists(), "{point:?}");
            assert!(!root.join("cache/app/data.bin").exists(), "{point:?}");
        }
    }

    #[test]
    fn a_failed_sidecar_leaves_the_item_in_the_store_and_nothing_overwritten() {
        let (_fx, root, store) = setup();
        quarantined(&root, &store, "cache/app/data.bin", "item-1");
        let result = {
            let _armed = Armed::at(Point::SidecarWrite);
            write_sidecar(&store, b"item-1", b"{}")
        };
        assert!(result.is_err());
        assert!(root.join("Quarantine/item-1").exists());
    }

    #[test]
    fn faults_after_the_restore_report_the_item_at_its_source() {
        for point in [Point::RestoreSync, Point::RestoreCheck] {
            let (_fx, root, store) = setup();
            let item = quarantined(&root, &store, "cache/app/data.bin", "item-1");
            let result = {
                let _armed = Armed::at(point);
                restore(&store, b"item-1", &item)
            };
            assert!(matches!(
                result,
                Err(RestoreError::Sync(_) | RestoreError::StoreRemains)
            ));
            assert_eq!(
                result.err().map(|e| e.left()),
                Some(Left::AtSource),
                "{point:?}"
            );
            assert!(root.join("cache/app/data.bin").exists(), "{point:?}");
            assert!(!root.join("Quarantine/item-1").exists(), "{point:?}");
        }
    }

    #[test]
    fn a_fault_part_way_through_a_tree_is_partial_and_finishes_later() {
        let (_fx, root, store) = setup();
        std::fs::write(root.join("cache/dir/y"), b"y").unwrap_or_else(|e| unreachable!("{e}"));
        let item = quarantined(&root, &store, "cache/dir", "item-2");
        let result = {
            let _armed = Armed::at(Point::FinalizeUnlink);
            finalize(&store, b"item-2", &item.expected)
        };
        assert!(matches!(result, Err(FinalizeError::Removal(_))));
        assert_eq!(result.err().map(|e| e.removed()), Some(Removed::Partly));
        let left = std::fs::read_dir(root.join("Quarantine/item-2"))
            .unwrap_or_else(|e| unreachable!("{e}"))
            .count();
        assert_eq!(left, 1, "one of two entries removed before the fault");
        assert_eq!(finalize(&store, b"item-2", &item.expected), Ok(()));
        assert!(!root.join("Quarantine/item-2").exists());
    }

    #[test]
    fn a_fault_removing_the_sidecar_reports_the_item_gone() {
        let (_fx, root, store) = setup();
        let item = quarantined(&root, &store, "cache/app/data.bin", "item-1");
        write_sidecar(&store, b"item-1", b"{}").unwrap_or_else(|e| unreachable!("{e}"));
        let result = {
            let _armed = Armed::at(Point::FinalizeSidecar);
            finalize(&store, b"item-1", &item.expected)
        };
        assert_eq!(result.err().map(|e| e.removed()), Some(Removed::Item));
        assert!(!root.join("Quarantine/item-1").exists(), "the item is gone");
    }
}
