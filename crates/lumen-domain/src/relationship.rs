//! Typed, evidence-backed relationships between subjects.
//!
//! Explanations ("What created this? Is anything using it?") are traversals over
//! these edges (ADR-0013). An edge is only as trustworthy as the evidence behind
//! it, so every [`Relationship`] cites at least one [`EvidenceId`], and each
//! [`RelationKind`] only connects the subject types it makes sense for.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{EvidenceId, Subject};

/// Coarse type of a [`Subject`], used to constrain relationship endpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubjectType {
    /// [`Subject::File`].
    File,
    /// [`Subject::Application`].
    Application,
    /// [`Subject::Volume`].
    Volume,
    /// [`Subject::Process`].
    Process,
    /// [`Subject::Service`].
    Service,
}

impl Subject {
    /// The subject's type.
    pub const fn subject_type(&self) -> SubjectType {
        match self {
            Self::File { .. } => SubjectType::File,
            Self::Application { .. } => SubjectType::Application,
            Self::Volume { .. } => SubjectType::Volume,
            Self::Process { .. } => SubjectType::Process,
            Self::Service { .. } => SubjectType::Service,
        }
    }
}

/// Kind of relationship. Read `from <kind> to`, e.g. "cache `owned_by` app".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum RelationKind {
    /// A directory or volume contains a file or directory.
    Contains,
    /// A file belongs to an application (by location convention, receipt or
    /// knowledge base).
    OwnedBy,
    /// A file was created by an application.
    CreatedBy,
    /// A file is open in, or mapped by, a running process.
    OpenedBy,
    /// A process was started by a service.
    LaunchedBy,
    /// A service runs this executable.
    Launches,
    /// A vendor declared this file a disposable cache of an application.
    DeclaredCacheOf,
    /// Two files share blocks (APFS or `ReFS` clones).
    CloneOf,
    /// Two files have identical content (confirmed by content hash).
    DuplicateOf,
    /// A file's content lives inside another file (e.g. container layers in a
    /// virtual-machine disk image).
    StoredIn,
}

impl RelationKind {
    /// Allowed `(from, to)` subject types.
    pub fn allows(self, from: SubjectType, to: SubjectType) -> bool {
        use SubjectType as T;
        match self {
            Self::Contains => matches!((from, to), (T::File | T::Volume, T::File)),
            Self::OwnedBy | Self::CreatedBy | Self::DeclaredCacheOf => {
                (from, to) == (T::File, T::Application)
            }
            Self::OpenedBy => (from, to) == (T::File, T::Process),
            Self::LaunchedBy => (from, to) == (T::Process, T::Service),
            Self::Launches => (from, to) == (T::Service, T::File),
            Self::CloneOf | Self::DuplicateOf | Self::StoredIn => (from, to) == (T::File, T::File),
        }
    }
}

/// Error returned for an invalid relationship.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum RelationshipError {
    /// The kind does not connect these subject types.
    #[error("{kind:?} cannot connect {from:?} to {to:?}")]
    EndpointTypes {
        /// Relationship kind.
        kind: RelationKind,
        /// Source type.
        from: SubjectType,
        /// Target type.
        to: SubjectType,
    },
    /// A subject cannot be related to itself.
    #[error("a relationship cannot connect a subject to itself")]
    SelfLoop,
    /// Every relationship must cite evidence.
    #[error("a relationship must cite at least one piece of evidence")]
    NoEvidence,
}

/// A typed, directed edge justified by evidence.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct Relationship {
    from: Subject,
    kind: RelationKind,
    to: Subject,
    evidence: BTreeSet<EvidenceId>,
}

impl Relationship {
    /// Creates a relationship after validating endpoints and evidence.
    ///
    /// # Errors
    ///
    /// Returns a [`RelationshipError`] if the kind does not connect the subject
    /// types, the endpoints are equal, or `evidence` is empty.
    pub fn new(
        from: Subject,
        kind: RelationKind,
        to: Subject,
        evidence: impl IntoIterator<Item = EvidenceId>,
    ) -> Result<Self, RelationshipError> {
        let evidence: BTreeSet<_> = evidence.into_iter().collect();
        let (from_type, to_type) = (from.subject_type(), to.subject_type());
        if !kind.allows(from_type, to_type) {
            return Err(RelationshipError::EndpointTypes {
                kind,
                from: from_type,
                to: to_type,
            });
        }
        if from == to {
            return Err(RelationshipError::SelfLoop);
        }
        if evidence.is_empty() {
            return Err(RelationshipError::NoEvidence);
        }
        Ok(Self {
            from,
            kind,
            to,
            evidence,
        })
    }

    /// Source subject.
    pub fn from(&self) -> &Subject {
        &self.from
    }

    /// Relationship kind.
    pub const fn kind(&self) -> RelationKind {
        self.kind
    }

    /// Target subject.
    pub fn to(&self) -> &Subject {
        &self.to
    }

    /// Evidence that justifies the edge.
    pub fn evidence(&self) -> &BTreeSet<EvidenceId> {
        &self.evidence
    }
}

impl<'de> Deserialize<'de> for Relationship {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            from: Subject,
            kind: RelationKind,
            to: Subject,
            evidence: BTreeSet<EvidenceId>,
        }
        let w = Wire::deserialize(deserializer)?;
        Self::new(w.from, w.kind, w.to, w.evidence).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AppId, AppIdScheme, FileId, FileIdentity, ServiceKind, UntrustedText, VolumeId};

    fn file(n: u128) -> Subject {
        Subject::File {
            identity: FileIdentity {
                volume: VolumeId::new("v").unwrap_or_else(|_| unreachable!()),
                file: FileId::new(n),
            },
        }
    }

    fn app() -> Subject {
        Subject::Application {
            id: AppId {
                scheme: AppIdScheme::BundleId,
                value: UntrustedText::new("com.example.App"),
            },
        }
    }

    fn process() -> Subject {
        Subject::Process {
            pid: 77,
            started_at: None,
        }
    }

    fn service() -> Subject {
        Subject::Service {
            kind: ServiceKind::LaunchAgent,
            label: UntrustedText::new("com.example.agent"),
        }
    }

    fn ev() -> EvidenceId {
        "b3:0000000000000000000000000000000000000000000000000000000000000001"
            .parse()
            .unwrap_or_else(|_| unreachable!())
    }

    #[test]
    fn valid_chains_from_the_architecture_build() -> Result<(), RelationshipError> {
        // Application -> cache -> process -> launch agent (system.md §7).
        Relationship::new(file(1), RelationKind::OwnedBy, app(), [ev()])?;
        Relationship::new(file(1), RelationKind::OpenedBy, process(), [ev()])?;
        Relationship::new(process(), RelationKind::LaunchedBy, service(), [ev()])?;
        Relationship::new(service(), RelationKind::Launches, file(2), [ev()])?;
        Relationship::new(file(3), RelationKind::StoredIn, file(4), [ev()])?;
        Ok(())
    }

    #[test]
    fn mistyped_endpoints_are_rejected() {
        assert_eq!(
            Relationship::new(app(), RelationKind::OpenedBy, process(), [ev()]),
            Err(RelationshipError::EndpointTypes {
                kind: RelationKind::OpenedBy,
                from: SubjectType::Application,
                to: SubjectType::Process,
            })
        );
        assert!(Relationship::new(file(1), RelationKind::Launches, file(2), [ev()]).is_err());
    }

    #[test]
    fn self_loops_and_unjustified_edges_are_rejected() {
        assert_eq!(
            Relationship::new(file(1), RelationKind::DuplicateOf, file(1), [ev()]),
            Err(RelationshipError::SelfLoop)
        );
        assert_eq!(
            Relationship::new(file(1), RelationKind::DuplicateOf, file(2), []),
            Err(RelationshipError::NoEvidence)
        );
    }

    #[test]
    fn deserialization_revalidates() -> serde_json::Result<()> {
        let edge = Relationship::new(file(1), RelationKind::OwnedBy, app(), [ev()])
            .unwrap_or_else(|_| unreachable!());
        let json = serde_json::to_string(&edge)?;
        assert_eq!(serde_json::from_str::<Relationship>(&json)?, edge);
        let unjustified = json.replace(&format!("[\"{}\"]", ev()), "[]");
        assert!(serde_json::from_str::<Relationship>(&unjustified).is_err());
        Ok(())
    }
}
