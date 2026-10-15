//! The system clock and identifier adapters of the composition root (roadmap
//! 10.1).

use std::time::SystemTime;

use lumen_application::ports::{Clock, IdGenerator};
use lumen_domain::{OperationId, PlanId, ScanId, SnapshotId, Timestamp};
use uuid::Uuid;

/// The operating system's clock, in UTC.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct SystemClock;

impl Clock for SystemClock {
    /// The current time. A clock set before 1970 or beyond the timestamp range
    /// reads as the Unix epoch: such a time is unusable, and the epoch is
    /// obviously wrong rather than plausibly so.
    fn now(&self) -> Timestamp {
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .ok()
            .and_then(|since| {
                Timestamp::from_unix(i64::try_from(since.as_secs()).ok()?, since.subsec_nanos())
                    .ok()
            })
            .unwrap_or(Timestamp::UNIX_EPOCH)
    }
}

/// Time-ordered identifiers (`UUIDv7`).
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct UuidV7Ids;

/// A fresh `UUIDv7` accepted by `make`. `now_v7` never returns the nil UUID
/// (its version bits are set), which is all the ID types reject, so the loop
/// ends on its first pass.
fn fresh<T, E>(make: impl Fn(Uuid) -> Result<T, E>) -> T {
    loop {
        if let Ok(id) = make(Uuid::now_v7()) {
            return id;
        }
    }
}

impl IdGenerator for UuidV7Ids {
    fn scan_id(&self) -> ScanId {
        fresh(ScanId::new)
    }

    fn snapshot_id(&self) -> SnapshotId {
        fresh(SnapshotId::new)
    }

    fn plan_id(&self) -> PlanId {
        fresh(PlanId::new)
    }

    fn operation_id(&self) -> OperationId {
        fresh(OperationId::new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clock_reads_the_present() {
        let now = SystemClock.now();
        // 2026-01-01T00:00:00Z: this code did not exist before then.
        assert!(now.unix_seconds() >= 1_767_225_600, "{now:?}");
    }

    #[test]
    fn identifiers_are_fresh_and_time_ordered() {
        let ids = UuidV7Ids;
        let first = ids.operation_id().to_string();
        let second = ids.operation_id().to_string();
        assert_ne!(first, second);
        assert!(first < second, "UUIDv7 sorts by creation time");
        assert_ne!(ids.scan_id().to_string(), ids.plan_id().to_string());
    }
}
