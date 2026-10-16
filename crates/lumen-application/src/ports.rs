//! Ports: time, identifiers, directory enumeration, verification, change feeds,
//! inventory, in-use probes, the optional AI judge, secret storage and the
//! quarantine executor and journal.

use std::future::Future;
use std::ops::ControlFlow;
use std::pin::Pin;

use lumen_domain::{
    AccessState, Application, FileIdentity, FilesystemEntry, ItemLocation, JevRequest,
    JudgeDescriptor, Judgment, JudgmentRejection, OperationId, Package, PlanId, Process,
    QuarantineEvent, QuarantineFailure, QuarantineState, RawPath, ScanId, Service, SnapshotId,
    SourceName, SourceStatus, Timestamp, VolumeId,
};

use crate::CancelToken;

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

/// Re-reads a cleanup candidate through its own handle just before it is proposed
/// or acted on ("phase 2", ADR-0017). Listings are fast but can be stale or
/// incomplete: a Windows directory record has no hard-link count, and the size of
/// a hard-linked file is updated only at the link it was changed through.
pub trait CandidateVerifier: Send + Sync {
    /// Fresh, exact facts for the object at `path`, which must still be the
    /// object `expected` identifies. A final link or reparse point is described,
    /// never followed, and nothing is opened for data.
    ///
    /// # Errors
    ///
    /// Returns [`EnumerateError::IdentityChanged`] if `path` now names another
    /// object, [`EnumerateError::Unobservable`] if it is gone or access is
    /// refused, and [`EnumerateError::UnsupportedPath`] for paths this platform
    /// cannot use. Callers must then treat the candidate as `REVIEW` at most.
    fn verify(
        &self,
        path: &RawPath,
        expected: &FileIdentity,
    ) -> Result<FilesystemEntry, EnumerateError>;
}

/// A directory that was discovered but not yet listed: the unit of work a scan
/// resumes from after an interruption.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingDirectory {
    /// The directory.
    pub path: RawPath,
    /// Its identity when it was discovered; listing it again must match.
    pub identity: FileIdentity,
    /// Volume of the scan root it belongs to, so resuming never crosses into
    /// another volume.
    pub root_volume: VolumeId,
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

/// Everything one inventory source reported in one collection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryReport {
    /// The source, e.g. `macos.applications` or `windows.uninstall-keys`.
    pub source: SourceName,
    /// How completely the source was read. Only [`SourceStatus::Complete`] lets
    /// "not found here" count as evidence of absence
    /// ([`CoverageReport::absence_is_provable`](lumen_domain::CoverageReport::absence_is_provable)).
    pub status: SourceStatus,
    /// Installed applications found.
    pub applications: Vec<Application>,
    /// Background, startup and scheduled components found.
    pub services: Vec<Service>,
    /// Packages from package managers found.
    pub packages: Vec<Package>,
}

/// Reads one inventory of installed software (applications, services,
/// packages) without changing anything (ADR-0013).
///
/// A source that is denied, unsupported or only partly readable reports that in
/// [`InventoryReport::status`] instead of failing, exactly like a denied
/// directory (ADR-0008): an incomplete inventory is an observation, never an
/// empty one.
pub trait InventorySource: Send + Sync {
    /// The source's stable name.
    fn name(&self) -> SourceName;

    /// Collects the inventory. Stops early, reporting
    /// [`SourceStatus::Partial`], if `cancel` is cancelled.
    fn collect(&self, cancel: &CancelToken) -> InventoryReport;
}

/// Whether a file or directory is in use by running processes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InUse {
    /// These processes have it (or something under it) open, mapped, as their
    /// executable or as their working directory.
    Yes(Vec<Process>),
    /// No process uses it, and the probe could inspect every running process.
    No,
    /// No inspected process uses it, but some processes could not be inspected
    /// (an unprivileged probe cannot see other users' processes, such as root
    /// daemons). Not proof of non-use: the policy accepts it only for the
    /// reversible quarantine of the user's own files (ADR-0030), never the probe.
    Partial {
        /// How many processes could not be inspected, when the platform can
        /// tell (macOS); `None` when it cannot (Windows Restart Manager).
        unseen: Option<u32>,
    },
    /// The question could not be answered (the object changed or vanished, or
    /// the process list could not be read). Callers must treat this like
    /// [`InUse::Yes`] for any action.
    Unknown {
        /// Short, non-sensitive reason.
        reason: &'static str,
    },
}

impl InUse {
    /// Whether acting on the item is allowed by this probe: only when it is
    /// provably not in use.
    pub const fn is_provably_unused(&self) -> bool {
        matches!(self, Self::No)
    }
}

/// Asks the operating system whether running processes use an object
/// (`libproc` on macOS, Restart Manager on Windows). The answer is a snapshot:
/// executors check again right before acting (ADR-0016).
pub trait InUseProbe: Send + Sync {
    /// Whether the object at `path`, which must still be `expected`, or anything
    /// under it, is in use.
    fn probe(&self, path: &RawPath, expected: &FileIdentity) -> InUse;
}

/// An item the executor prepared at confirmation time: its exact identity and
/// location in the executor's own terms (e.g. `(dev, ino)` and a handle path
/// from a scan root). Opaque to the application, which only stores and passes
/// it back; it is JSON so it can sit in the journal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedItem(pub Vec<u8>);

/// Why the executor did not quarantine an item, and where it left it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("quarantine failed: {reason:?}")]
pub struct ExecutionFailure {
    /// What went wrong.
    pub reason: QuarantineFailure,
    /// Where the item is now: untouched at its source, or in the store and
    /// restorable.
    pub left: ItemLocation,
}

/// Where reconciliation finds an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Located {
    /// At its source, unchanged.
    AtSource,
    /// In the store, as prepared.
    InStore,
    /// Neither where it was nor in the store (or changed): unknown, never
    /// assumed gone.
    Unknown,
}

/// Who owns a filesystem object, against the account Lumen runs as (roadmap
/// 10.2b, ADR-0030). Scanned entries do not carry their owner, so it is read
/// when an item is assessed.
pub trait OwnershipProbe: Send + Sync {
    /// Whether the object at `path`, if it is still `expected`, belongs to the
    /// current user; `None` if that cannot be established (missing, replaced or
    /// unreadable), never a guess.
    fn owned_by_current_user(&self, path: &RawPath, expected: &FileIdentity) -> Option<bool>;
}

/// Lumen's only path to moving user files (ADR-0015, ADR-0016), implemented
/// by `lumen-fs-exec`.
pub trait QuarantineExecutor: Send + Sync {
    /// Captures `relative` (components below `root`) as it is now.
    ///
    /// # Errors
    ///
    /// An [`ExecutionFailure`] (left at the source) if it cannot be observed.
    fn prepare(
        &self,
        root: &RawPath,
        relative: &[Vec<u8>],
    ) -> Result<PreparedItem, ExecutionFailure>;

    /// Re-verifies the item, moves it into the store as `operation` without
    /// replacing anything, verifies it there, and writes `manifest` next to it.
    ///
    /// # Errors
    ///
    /// An [`ExecutionFailure`] saying why and where the item is.
    fn quarantine(
        &self,
        item: &PreparedItem,
        operation: OperationId,
        manifest: &[u8],
    ) -> Result<(), ExecutionFailure>;

    /// Where the item of `operation` is now, for startup reconciliation.
    fn locate(&self, item: &PreparedItem, operation: OperationId) -> Located;

    /// Moves the item of `operation` back to where it was, without replacing
    /// anything.
    ///
    /// # Errors
    ///
    /// A [`RestoreFailure`] saying why and where the item is.
    fn restore(
        &self,
        item: &PreparedItem,
        operation: OperationId,
    ) -> Result<RestoredItem, RestoreFailure>;

    /// Permanently removes the item of `operation` from the store, then its
    /// manifest sidecar. The only permanent removal (ADR-0015).
    ///
    /// # Errors
    ///
    /// How much a failed finalization removed.
    fn finalize(&self, item: &PreparedItem, operation: OperationId) -> Result<(), Removal>;

    /// Where the item of `operation` is held in the store, and its identity
    /// in the enumerators' form, so that it can be probed for use right before
    /// finalization (ADR-0030). `None` unless it is there, unchanged. An
    /// executor that cannot say keeps the default, and then nothing is
    /// finalized.
    fn held(&self, item: &PreparedItem, operation: OperationId) -> Option<(RawPath, FileIdentity)> {
        let _ = (item, operation);
        None
    }
}

/// How much a failed finalization removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Removal {
    /// Nothing; the item is intact in the store.
    Nothing,
    /// Possibly part of the item; finalizing again finishes it.
    Partly,
    /// The whole item; only its sidecar remains.
    Item,
}

/// Why a restore did not happen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotRestored {
    /// Something now has the item's name; the user decides.
    NameTaken,
    /// The original location cannot be reached safely.
    OriginUnreachable,
    /// The quarantined item is missing or changed.
    ItemChanged,
    /// The move back or a check failed.
    Failed,
}

/// A restore that did not complete, and where it left the item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RestoreFailure {
    /// Why.
    pub reason: NotRestored,
    /// Still in the store, or back at the source but unverified.
    pub left: ItemLocation,
}

/// A completed restore.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RestoredItem {
    /// The original parent had been recreated (best effort).
    pub parent_replaced: bool,
}

/// The journal could not record an entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the quarantine journal is unavailable")]
pub struct JournalError;

/// An operation that stopped between its intent and retention.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unfinished {
    /// The operation.
    pub operation: OperationId,
    /// The state its journal reached.
    pub state: QuarantineState,
    /// The prepared item recorded with its intent.
    pub item: PreparedItem,
}

/// The durable quarantine journal (ADR-0015), implemented by the ledger store.
pub trait QuarantineJournal: Send {
    /// Appends `event` for `operation` with the executor's `detail` (JSON),
    /// durably.
    ///
    /// # Errors
    ///
    /// A [`JournalError`] if the entry is not durable; nothing may happen then.
    fn append(
        &mut self,
        operation: OperationId,
        event: QuarantineEvent,
        detail: &[u8],
    ) -> Result<(), JournalError>;

    /// Operations stopped after their intent and before retention, for startup
    /// reconciliation.
    ///
    /// # Errors
    ///
    /// A [`JournalError`] if the journal cannot be read or is corrupt.
    fn unfinished(&self) -> Result<Vec<Unfinished>, JournalError>;
}

/// Which secret a [`SecretStore`] holds. Names are fixed in code, so no input
/// can choose which credential is read or overwritten.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SecretName(&'static str);

impl SecretName {
    /// The user's `OpenRouter` API key for Jev (ADR-0031).
    pub const OPENROUTER_API_KEY: Self = Self("openrouter-api-key");

    /// The name as stored (the account of a keychain item).
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

/// A secret value. Its `Debug` output never shows it; read it with
/// [`Secret::expose`] only where it is used.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    /// Wraps a value.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The value.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(redacted)")
    }
}

/// Why a secret could not be read or written.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SecretError {
    /// The platform store refused or failed (locked keychain, no access, …).
    #[error("the secret store is unavailable: {0}")]
    Unavailable(&'static str),
    /// A stored value is not valid UTF-8 text.
    #[error("the stored secret is not valid text")]
    Corrupt,
}

/// The operating system's credential store (macOS Keychain, Windows Credential
/// Manager). Secrets never go to Lumen's databases or logs.
pub trait SecretStore: Send + Sync {
    /// The secret, or `None` if none is stored.
    ///
    /// # Errors
    ///
    /// [`SecretError`] if the store cannot be read.
    fn get(&self, name: SecretName) -> Result<Option<Secret>, SecretError>;

    /// Stores `secret`, replacing any previous value.
    ///
    /// # Errors
    ///
    /// [`SecretError`] if the store cannot be written.
    fn set(&self, name: SecretName, secret: &Secret) -> Result<(), SecretError>;

    /// Removes the secret; `false` if there was none.
    ///
    /// # Errors
    ///
    /// [`SecretError`] if the store cannot be written.
    fn delete(&self, name: SecretName) -> Result<bool, SecretError>;
}

/// Why a judge produced no usable judgment. Every variant means the same thing
/// to the caller: decide without Jev (docs/ai/architecture.md).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum JudgeError {
    /// Jev is turned off (the default).
    #[error("Jev is disabled")]
    Disabled,
    /// The judge could not be reached or did not answer in time.
    #[error("the judge is unavailable: {0}")]
    Unavailable(&'static str),
    /// The model refused, ran out of tokens, or answered from an unexpected
    /// model.
    #[error("the judge gave no usable answer: {0}")]
    NoAnswer(&'static str),
    /// The answer did not match the contract.
    #[error("the judgment was rejected: {0}")]
    Rejected(JudgmentRejection),
    /// The answer was not valid JSON for `jev.judgment/1`.
    #[error("the judgment did not match its schema")]
    Malformed,
}

/// A judgment in flight.
pub type JudgeFuture<'a> = Pin<Box<dyn Future<Output = Result<Judgment, JudgeError>> + Send + 'a>>;

/// The optional AI judge (Jev, ADR-0020). It answers one request with a
/// validated [`Judgment`], which the policy may use only to add caution.
/// Adapters own transport and validation; callers treat any error as "no Jev".
pub trait JudgeModel: Send + Sync {
    /// Which judge this is, or `None` when Jev is disabled.
    fn descriptor(&self) -> Option<JudgeDescriptor>;

    /// Judges one request.
    fn judge<'a>(&'a self, request: &'a JevRequest) -> JudgeFuture<'a>;
}

/// The default judge: Jev is disabled, and every request is answered with
/// [`JudgeError::Disabled`] without leaving the process.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopJudge;

impl JudgeModel for NoopJudge {
    fn descriptor(&self) -> Option<JudgeDescriptor> {
        None
    }

    fn judge<'a>(&'a self, _request: &'a JevRequest) -> JudgeFuture<'a> {
        Box::pin(std::future::ready(Err(JudgeError::Disabled)))
    }
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
        assert_dyn_send_sync::<dyn CandidateVerifier>();
        assert_dyn_send::<dyn ChangeFeed>();
        assert_dyn_send_sync::<dyn InventorySource>();
        assert_dyn_send_sync::<dyn InUseProbe>();
        assert_dyn_send_sync::<dyn JudgeModel>();
        assert_dyn_send_sync::<dyn SecretStore>();
        assert_dyn_send_sync::<dyn QuarantineExecutor>();
        assert_dyn_send::<dyn QuarantineJournal>();
    }

    #[test]
    fn secrets_never_print() {
        let secret = Secret::new("sk-or-very-secret");
        assert_eq!(format!("{secret:?}"), "Secret(redacted)");
        assert_eq!(secret.expose(), "sk-or-very-secret");
        assert_eq!(
            SecretName::OPENROUTER_API_KEY.as_str(),
            "openrouter-api-key"
        );
    }

    #[test]
    fn the_noop_judge_is_disabled() -> Result<(), Box<dyn std::error::Error>> {
        use std::collections::{BTreeMap, BTreeSet};
        use std::task::{Context, Poll, Waker};

        let request = JevRequest {
            schema_version: JevRequest::current_schema()?,
            item_ref: lumen_domain::ItemRef::new("c_1")?,
            candidate: lumen_domain::CandidateSummary {
                category: lumen_domain::Category::Other,
                path_display: lumen_domain::UntrustedText::new("~/x"),
                size_bytes: lumen_domain::ByteCount::ZERO,
                reclaimable_bytes: lumen_domain::ByteCount::ZERO,
            },
            evidence: Vec::new(),
            coverage: lumen_domain::CoverageSummary {
                complete: true,
                notes: BTreeSet::new(),
            },
            untrusted_strings: BTreeMap::new(),
        };
        let judge: &dyn JudgeModel = &NoopJudge;
        assert_eq!(judge.descriptor(), None);
        let mut future = judge.judge(&request);
        let mut context = Context::from_waker(Waker::noop());
        assert_eq!(
            future.as_mut().poll(&mut context),
            Poll::Ready(Err(JudgeError::Disabled))
        );
        Ok(())
    }

    #[test]
    fn only_a_complete_probe_proves_an_item_unused() {
        assert!(InUse::No.is_provably_unused());
        assert!(!InUse::Yes(Vec::new()).is_provably_unused());
        assert!(
            !InUse::Partial { unseen: Some(1) }.is_provably_unused(),
            "unseen processes may use it"
        );
        assert!(
            !InUse::Unknown {
                reason: "other users' processes are not visible"
            }
            .is_provably_unused(),
            "unknown is never treated as unused"
        );
    }

    #[test]
    fn a_partial_inventory_cannot_prove_absence() {
        struct Partial;
        impl InventorySource for Partial {
            fn name(&self) -> SourceName {
                SourceName::new("test.partial").unwrap_or_else(|_| unreachable!())
            }
            fn collect(&self, _cancel: &CancelToken) -> InventoryReport {
                InventoryReport {
                    source: self.name(),
                    status: SourceStatus::Partial,
                    applications: Vec::new(),
                    services: Vec::new(),
                    packages: Vec::new(),
                }
            }
        }
        let source: &dyn InventorySource = &Partial;
        let report = source.collect(&CancelToken::new());
        let coverage = lumen_domain::CoverageReport {
            roots: Vec::new(),
            sources: [(report.source.clone(), report.status)].into(),
        };
        assert_eq!(report.applications.len(), 0);
        assert!(
            !coverage.absence_is_provable(&[source.name()].into()),
            "an empty but partial inventory proves nothing"
        );
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
