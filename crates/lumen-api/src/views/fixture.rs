//! Assessments to build views from in tests.

use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU32;

use lumen_application::assess::Assessed;
use lumen_domain::{
    AccessState, AppId, AppIdScheme, Basis, ByteCount, EntryKind, EntryTimes, Evidence,
    EvidenceHash, Fact, FileId, FileIdentity, FilesystemEntry, FiredRule, JevEffect, Platform,
    PolicyDecision, PolicyStage, Protection, Provenance, RawPath, Risk, RiskLevel, RuleId,
    SizeFacts, SizeFlags, SourceName, Subject, Timestamp, UntrustedText, Verdict, VolumeId,
};
use lumen_policy::{Context, PolicyInput};

pub(crate) fn at(secs: i64) -> Timestamp {
    Timestamp::from_unix(secs, 0).unwrap_or_else(|e| unreachable!("{e}"))
}

pub(crate) fn file(n: u128) -> Subject {
    Subject::File {
        identity: FileIdentity {
            volume: VolumeId::new("v").unwrap_or_else(|e| unreachable!("{e}")),
            file: FileId::new(n),
        },
    }
}

pub(crate) fn app_id(name: &str) -> AppId {
    AppId {
        scheme: AppIdScheme::BundleId,
        value: UntrustedText::new(name),
    }
}

pub(crate) fn record(subject: Subject, fact: Fact, observed: i64) -> Evidence {
    Evidence {
        subject,
        fact,
        basis: Basis::Observed,
        provenance: Provenance {
            source: SourceName::new("lumen.scan").unwrap_or_else(|e| unreachable!("{e}")),
            observed_at: at(observed),
        },
    }
}

/// Candidate 1, of `kind` and `access`, decided from `evidence`.
pub(crate) fn assessed(kind: EntryKind, access: AccessState, evidence: Vec<Evidence>) -> Assessed {
    let Subject::File { identity } = file(1) else {
        unreachable!()
    };
    let decision = PolicyDecision::new(
        Verdict::Review,
        lumen_policy::POLICY_VERSION,
        EvidenceHash::from_bytes([3; 32]),
        [FiredRule {
            rule: RuleId::new("default.review").unwrap_or_else(|e| unreachable!("{e}")),
            stage: PolicyStage::Classification,
            evidence: BTreeSet::new(),
        }],
        Risk {
            level: RiskLevel::Low,
            factors: BTreeSet::new(),
        },
        JevEffect::None,
    )
    .unwrap_or_else(|e| unreachable!("{e}"));
    let candidate = FilesystemEntry {
        path: RawPath::from_unix_bytes(b"/Users/ana/Library/Caches/x".to_vec())
            .unwrap_or_else(|e| unreachable!("{e}")),
        kind,
        size: SizeFacts {
            identity: identity.clone(),
            logical: ByteCount::ZERO,
            allocated: ByteCount::ZERO,
            private: None,
            clone_id: None,
            link_count: NonZeroU32::MIN,
            flags: SizeFlags::default(),
        },
        times: EntryTimes::default(),
        protection: Protection::default(),
        access,
    };
    Assessed {
        input: PolicyInput {
            subject: file(1),
            evidence_hash: decision.evidence_hash(),
            evidence,
            paths: vec![candidate.path.clone()],
            context: Context {
                platform: Platform::Macos,
                home: None,
                windows_dir: None,
                local_app_data: None,
                mechanisms: BTreeSet::new(),
                complete_sources: BTreeSet::new(),
                single_user: false,
            },
            observed: None,
            knowledge: BTreeMap::new(),
            jev: None,
        },
        relationships: Vec::new(),
        candidate,
        decision,
        trace: None,
        reasons: Vec::new(),
        note: None,
        note_trace: None,
        free_when_closed: None,
    }
}
