//! One policy decision on a typical candidate: a regenerable developer cache
//! that every gate lets through to `QUARANTINE`, so every stage runs. Run with
//! `cargo bench -p lumen-policy`; baselines are in
//! `docs/architecture/performance.md`.

// Benchmark setup aborts on failure; there is no caller to report errors to.
#![allow(clippy::expect_used, missing_docs)]

use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU32;

use criterion::{Criterion, criterion_group, criterion_main};
use lumen_domain::{
    AccessState, Basis, ByteCount, CleanupMechanism, EntryKind, EntryTimes, Evidence, EvidenceHash,
    Fact, FileId, FileIdentity, FilesystemEntry, Platform, Protection, Provenance, RawPath,
    ReclaimEstimate, SizeFacts, SizeFlags, SourceName, Subject, Timestamp, Verdict, VolumeId,
};
use lumen_policy::{Context, KnowledgeClass, Observed, PolicyInput, evaluate};

fn input() -> PolicyInput {
    let identity = FileIdentity {
        volume: VolumeId::new("v").expect("volume"),
        file: FileId::new(1),
    };
    let subject = Subject::File {
        identity: identity.clone(),
    };
    let entry = SourceName::new("xcode-derived-data").expect("entry");
    let evidence = |fact: Fact| Evidence {
        subject: subject.clone(),
        fact,
        basis: Basis::Observed,
        provenance: Provenance {
            source: SourceName::new("bench").expect("source"),
            observed_at: Timestamp::UNIX_EPOCH,
        },
    };
    PolicyInput {
        subject: subject.clone(),
        evidence_hash: EvidenceHash::from_bytes([3; 32]),
        evidence: vec![
            evidence(Fact::InUse(false)),
            evidence(Fact::OwnedByCurrentUser(true)),
            evidence(Fact::KnowledgeBaseMatch {
                entry: entry.clone(),
            }),
        ],
        paths: vec![
            RawPath::from_unix_bytes(b"/home/t/Library/Developer/Xcode/DerivedData/App".to_vec())
                .expect("path"),
        ],
        context: Context {
            platform: Platform::Linux,
            home: Some(RawPath::from_unix_bytes(b"/home/t".to_vec()).expect("home")),
            windows_dir: None,
            local_app_data: None,
            mechanisms: BTreeSet::from([CleanupMechanism::QuarantineRename]),
            complete_sources: BTreeSet::new(),
        },
        observed: Some(Observed {
            entry: FilesystemEntry {
                path: RawPath::from_unix_bytes(
                    b"/home/t/Library/Developer/Xcode/DerivedData/App".to_vec(),
                )
                .expect("path"),
                kind: EntryKind::Directory,
                size: SizeFacts {
                    identity,
                    logical: ByteCount::new(1 << 30),
                    allocated: ByteCount::new(1 << 30),
                    private: None,
                    clone_id: None,
                    link_count: NonZeroU32::MIN,
                    flags: SizeFlags::default(),
                },
                times: EntryTimes::default(),
                protection: Protection::default(),
                access: AccessState::Readable,
            },
            complete: true,
            reclaim: ReclaimEstimate {
                reclaimable_now: ByteCount::new(1 << 30),
                ..ReclaimEstimate::default()
            },
        }),
        knowledge: BTreeMap::from([(entry, KnowledgeClass::Regenerable)]),
        jev: None,
    }
}

fn decide(c: &mut Criterion) {
    let input = input();
    assert_eq!(
        evaluate(&input).expect("decides").verdict(),
        Verdict::Quarantine,
        "the benchmark runs every stage"
    );
    c.bench_function("policy/decide quarantine candidate", |b| {
        b.iter(|| evaluate(std::hint::black_box(&input)).expect("decides"));
    });
}

criterion_group!(benches, decide);
criterion_main!(benches);
