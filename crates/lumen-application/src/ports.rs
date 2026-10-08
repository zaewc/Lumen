//! Ports for scanning: time, identifiers, directory enumeration and change feeds.

use std::ops::ControlFlow;

use lumen_domain::{
    AccessState, FileIdentity, FilesystemEntry, OperationId, PlanId, RawPath, ScanId, SnapshotId,
    Timestamp, VolumeId,
};

/// Source of the current time. The domain never reads the clock itself, so tests
/// can control time exactly.
pub trait Clock: Send + Sync {
    /// The current UTC time.
    fn now(&self) -> Timestamp;
}

/// Source of fresh identifiers. Production adapters use `UUIDv7` (time-ordered);
/// tests use deterministic sequences.
pub trait IdGenerator: Send + Sync {
    /// A new scan ID.
    fn scan_id(&self) -> ScanId;
    /// A new snapshot ID.
    fn snapshot_id(&self) -> SnapshotId;
    /// A new plan ID.
    fn plan_id(&self) -> PlanId;
    /// A new operation ID.
    fn operation_id(&self) -> OperationId;
}

/// How reading one directory ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirOutcome {
    /// Whether the directory's contents were observed. Anything but
    /// [`AccessState::Readable`] means the listing is unknown, not empty
    /// (ADR-0008).
    pub access: AccessState,
    /// Whether every entry was delivered. `false` if the sink stopped early
    /// (cancellation or backpressure) or the read failed part-way.
    pub complete: bool,
}

/// Errors from directory enumeration that are not observations about the
/// filesystem. Denied, vanished or unreadable directories are reported as
/// [`DirOutcome::access`], never as errors.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum EnumerateError {
    /// The object at the path is not the one expected: it was replaced (for
    /// example by a symlink or junction swap) after it was first observed.
    #[error("{path:?} is no longer the object first observed (identity changed)")]
    IdentityChanged {
        /// Path that was opened.
        path: RawPath,
        /// Identity the caller expected.
        expected: FileIdentity,
    },
    /// The path is not a directory (for `read_dir`).
    #[error("{path:?} is not a directory")]
    NotADirectory {
        /// Path that was opened.
        path: RawPath,
    },
    /// The path cannot be represented on this platform (wrong flavor, interior
    /// NUL).
    #[error("{path:?} cannot be used on this platform")]
    UnsupportedPath {
        /// Offending path.
        path: RawPath,
    },
    /// The object itself could not be observed: it does not exist (any more), or
    /// a parent directory refuses access. `access` says which; it is never
    /// [`AccessState::Readable`].
    #[error("{path:?} could not be observed ({access:?})")]
    Unobservable {
        /// Path that was looked up.
        path: RawPath,
        /// Why it could not be observed.
        access: AccessState,
    },
}

/// Reads filesystem metadata, one directory at a time (ADR-0017).
///
/// Implementations must never follow symbolic links or reparse points, never open
/// file contents, never hydrate cloud placeholders, and must report links,
/// placeholders and protections through [`FilesystemEntry`] fields.
pub trait DirEnumerator: Send + Sync {
    /// Metadata for the object at `path` itself, without following a final link.
    ///
    /// # Errors
    ///
    /// Returns [`EnumerateError::Unobservable`] if the object is missing or a
    /// parent denies access, and [`EnumerateError::UnsupportedPath`] for paths this
    /// platform cannot use.
    fn stat(&self, path: &RawPath) -> Result<FilesystemEntry, EnumerateError>;

    /// Delivers the entries of the directory at `path` to `sink`, in no particular
    /// order. If `expected` is given, the opened directory's identity must match
    /// it, which detects directories swapped since they were first observed.
    ///
    /// The sink returns [`ControlFlow::Break`] to stop early (cancellation,
    /// backpressure); the outcome then reports `complete: false`.
    ///
    /// # Errors
    ///
    /// Returns [`EnumerateError`] if the identity changed, the object is not a
    /// directory (links to directories are not directories), the directory itself
    /// cannot be observed, or the path is unusable. A directory whose *contents*
    /// cannot be read is not an error: see [`DirOutcome::access`].
    fn read_dir(
        &self,
        path: &RawPath,
        expected: Option<&FileIdentity>,
        sink: &mut dyn FnMut(FilesystemEntry) -> ControlFlow<()>,
    ) -> Result<DirOutcome, EnumerateError>;
}

/// Opaque, platform-specific position in a volume's change journal (for example
/// an `FSEvents` event ID with the device UUID, or a USN journal ID and USN).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ResumePoint {
    /// Volume the position belongs to.
    pub volume: VolumeId,
    /// Adapter-defined encoding of the position.
    pub token: Vec<u8>,
}

/// One change reported by a change feed.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Change {
    /// Something under this path changed (created, modified, removed or renamed);
    /// rescan this directory.
    Directory(RawPath),
    /// Events were coalesced or dropped below this path; rescan the subtree.
    RescanSubtree(RawPath),
    /// The journal cannot be trusted any more (volume changed, history lost);
    /// rescan everything on the volume.
    RescanVolume(VolumeId),
}

/// A batch of changes and the position to resume from after applying them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeBatch {
    /// Changes, in journal order.
    pub changes: Vec<Change>,
    /// Where to resume after this batch has been applied and persisted.
    pub resume: Option<ResumePoint>,
}

/// Errors from a change feed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ChangeFeedError {
    /// The resume point belongs to another volume or an older journal; the caller
    /// must run a full rescan and start fresh.
    #[error("resume point is no longer valid for volume {volume}")]
    StaleResumePoint {
        /// Affected volume.
        volume: VolumeId,
    },
    /// The platform feed could not be started or read.
    #[error("change feed unavailable: {reason}")]
    Unavailable {
        /// Short, non-sensitive reason.
        reason: &'static str,
    },
}

/// A persistent change journal for incremental scanning (ADR-0018). Feeds are
/// advisory: candidates are always re-verified before any action.
pub trait ChangeFeed: Send {
    /// Starts watching `roots`, from `resume` if given or from now otherwise. Start
    /// the feed *before* a full scan so no change falls between the two.
    ///
    /// # Errors
    ///
    /// Returns [`ChangeFeedError`] if the feed cannot start or the resume point is
    /// stale.
    fn start(
        &mut self,
        roots: &[RawPath],
        resume: Option<ResumePoint>,
    ) -> Result<(), ChangeFeedError>;

    /// Returns up to `max` pending changes without blocking.
    ///
    /// # Errors
    ///
    /// Returns [`ChangeFeedError`] if the feed failed.
    fn poll(&mut self, max: usize) -> Result<ChangeBatch, ChangeFeedError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    // Ports must stay usable as trait objects so composition roots can choose
    // adapters at runtime, and shareable across scan threads.
    fn assert_dyn_send_sync<T: ?Sized + Send + Sync>() {}
    fn assert_dyn_send<T: ?Sized + Send>() {}

    #[test]
    fn ports_are_object_safe_and_thread_safe() {
        assert_dyn_send_sync::<dyn Clock>();
        assert_dyn_send_sync::<dyn IdGenerator>();
        assert_dyn_send_sync::<dyn DirEnumerator>();
        assert_dyn_send::<dyn ChangeFeed>();
    }

    struct EmptyDir;

    impl DirEnumerator for EmptyDir {
        fn stat(&self, path: &RawPath) -> Result<FilesystemEntry, EnumerateError> {
            Err(EnumerateError::UnsupportedPath { path: path.clone() })
        }

        fn read_dir(
            &self,
            _path: &RawPath,
            _expected: Option<&FileIdentity>,
            _sink: &mut dyn FnMut(FilesystemEntry) -> ControlFlow<()>,
        ) -> Result<DirOutcome, EnumerateError> {
            Ok(DirOutcome {
                access: AccessState::Readable,
                complete: true,
            })
        }
    }

    #[test]
    fn a_denied_directory_is_an_outcome_not_an_empty_listing()
    -> Result<(), Box<dyn std::error::Error>> {
        struct Denied;
        impl DirEnumerator for Denied {
            fn stat(&self, path: &RawPath) -> Result<FilesystemEntry, EnumerateError> {
                Err(EnumerateError::UnsupportedPath { path: path.clone() })
            }
            fn read_dir(
                &self,
                _path: &RawPath,
                _expected: Option<&FileIdentity>,
                _sink: &mut dyn FnMut(FilesystemEntry) -> ControlFlow<()>,
            ) -> Result<DirOutcome, EnumerateError> {
                Ok(DirOutcome {
                    access: AccessState::Denied(lumen_domain::DenialReason::Tcc),
                    complete: false,
                })
            }
        }
        let path = RawPath::from_unix_bytes(b"/Users/ana/Library/Mail".to_vec())?;
        let enumerators: [&dyn DirEnumerator; 2] = [&EmptyDir, &Denied];
        let outcomes: Vec<DirOutcome> = enumerators
            .iter()
            .map(|e| e.read_dir(&path, None, &mut |_| ControlFlow::Continue(())))
            .collect::<Result<_, _>>()?;
        assert!(outcomes[0].access.is_observed() && outcomes[0].complete);
        assert!(
            !outcomes[1].access.is_observed(),
            "denied must stay distinguishable from empty"
        );
        Ok(())
    }
}
