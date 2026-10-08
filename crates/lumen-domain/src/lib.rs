//! Lumen's platform-independent domain model.
//!
//! This crate holds the types every other part of Lumen agrees on: typed
//! identifiers, filesystem entries and their size facts, inventory entities,
//! immutable evidence, relationships, policy decisions, cleanup candidates, and
//! the quarantine, scan and plan state machines.
//!
//! It is deliberately pure (ADR-0002, ADR-0003):
//!
//! - no IO, no async runtime, and no platform-specific code;
//! - no `#[cfg(target_os)]`;
//! - no `unsafe`.
//!
//! See `docs/architecture/domain.md` for the module map, the invariants each type
//! enforces, and the wire contracts; `docs/architecture/system.md` places the model
//! in the overall architecture.

#![forbid(unsafe_code)]

#[macro_use]
mod schema;
#[macro_use]
mod digest;

mod candidate;
mod capability;
mod coverage;
mod decision;
mod entry;
mod escape;
mod evidence;
mod id;
mod inventory;
mod judgment;
mod lifecycle;
mod name;
mod path;
mod quarantine;
mod relationship;
mod size;
mod text;
mod time;
mod version;

pub use candidate::{
    CandidateError, CandidateParts, CandidateTarget, Category, CleanupAction, CleanupCandidate,
    Confidence,
};
pub use capability::{
    CapabilityError, CleanupMechanism, Observation, Platform, PlatformCapabilities, Reversibility,
};
pub use coverage::{
    AccessState, CoverageReport, DenialReason, FailureKind, RootCoverage, RootEntry, SourceName,
    SourceNameError, SourceStatus,
};
pub use decision::{
    DecisionError, FiredRule, JevEffect, PolicyDecision, PolicyStage, Risk, RiskFactor, RiskLevel,
    RuleId, RuleIdError, Verdict,
};
pub use digest::{
    EvidenceHash, EvidenceHashError, PayloadHash, PayloadHashError, PlanHash, PlanHashError,
};
pub use entry::{
    CaseSensitivity, EntryKind, EntryTimes, FilesystemEntry, FilesystemKind, Protection, Support,
    Volume, VolumeLocation,
};
pub use evidence::{Basis, Evidence, EvidenceId, EvidenceIdError, Fact, Provenance, Subject};
pub use id::{
    DeviceId, FileId, FileIdentity, IdError, OperationId, PlanId, ScanId, SnapshotId, VolumeId,
};
pub use inventory::{
    AppId, AppIdScheme, Application, CodeSignature, Package, PackageManager, Process, Service,
    ServiceKind, ServiceScope, SignatureCheck,
};
pub use judgment::{
    Assessment, ItemRef, JevTrace, JudgeDescriptor, Judgment, JudgmentConfidence,
    JudgmentRejection, ModelId, OpaqueIdError, ReasonCode, TraceValidation,
};
pub use lifecycle::{
    Confirmation, ConfirmationSurface, PlanEvent, PlanState, PlanTransitionError, ScanEvent,
    ScanState, ScanTransitionError,
};
pub use path::{PathError, PathFlavor, RawPath};
pub use quarantine::{
    ItemLocation, QuarantineEvent, QuarantineFailure, QuarantineState, TransitionError,
};
pub use relationship::{RelationKind, Relationship, RelationshipError, SubjectType};
pub use size::{ByteCount, CloneId, ReclaimEstimate, SizeFacts, SizeFlags};
pub use text::{UntrustedText, UntrustedTextTooLong};
pub use time::{Timestamp, TimestampError};
pub use version::{PolicyVersion, PromptVersion, SchemaVersion, VersionError};
