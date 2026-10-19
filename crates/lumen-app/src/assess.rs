//! Assessing the latest completed scan, read-only (roadmap 10.13b, 11.3).
//!
//! The composition: the platform's inventory sources and evidence producers
//! (the developer knowledge base everywhere; application support locations on
//! macOS; declared caches on Windows) run over the stored scan; their output is
//! gathered with the scan's own facts and recorded in `index.db`; candidates
//! are selected, probed (in use, ownership) and decided by the policy, without
//! Jev (roadmap 10.17). Nothing is moved; each candidate comes back with its
//! decision and the reasons, for the caller to present.

use std::collections::BTreeSet;
use std::path::Path;
use std::pin::pin;
use std::sync::Arc;
use std::task::{Context as TaskContext, Poll, Wake, Waker};
use std::thread::{self, Thread};

use lumen_application::assess::{Assessed, Probes, Sources, decide_candidate, select};
use lumen_application::collect::{Inventory, summarize};
use lumen_application::gather::gather;
use lumen_application::ports::{Clock, InUseProbe, InventoryReport, OwnershipProbe};
use lumen_devkb::{Environment, KnowledgeBase};
use lumen_domain::{
    CleanupMechanism, Evidence, Platform, PlatformCapabilities, RawPath, Relationship, SourceName,
    SourceNameError,
};
use lumen_policy::Context;
use lumen_store_sqlite::{IndexStore, StoreError, open_index};

use crate::space::{self, ASSESS_BYTES_PER_ENTRY, SpaceError};

/// Why an assessment could not run.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AssessError {
    /// The database has no completed scan to assess.
    #[error("no completed scan in this database; run a scan first")]
    NoCompletedScan,
    /// The index database failed.
    #[error("the index database failed: {0}")]
    Store(#[from] StoreError),
    /// The built-in knowledge base did not load.
    #[error("the knowledge base did not load: {0}")]
    KnowledgeBase(#[from] lumen_devkb::LoadError),
    /// A probe's source name is invalid.
    #[error("invalid source name: {0}")]
    SourceName(#[from] SourceNameError),
    /// Recording the evidence would leave too little free space, or it
    /// cannot be measured.
    #[error(transparent)]
    Space(#[from] SpaceError),
}

/// Runs `future` to completion on this thread.
pub fn block_on<F: Future>(future: F) -> F::Output {
    struct Unpark(Thread);
    impl Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(Unpark(thread::current())));
    let mut future = pin!(future);
    loop {
        if let Poll::Ready(value) = future.as_mut().poll(&mut TaskContext::from_waker(&waker)) {
            return value;
        }
        thread::park();
    }
}

/// A probe for platforms without one: every answer is unknown, so nothing is
/// ever eligible there.
#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
struct NoInUseProbe;

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
impl InUseProbe for NoInUseProbe {
    fn probe(
        &self,
        _path: &RawPath,
        _expected: &lumen_domain::FileIdentity,
    ) -> lumen_application::ports::InUse {
        lumen_application::ports::InUse::Unknown {
            reason: "no in-use probe on this platform",
        }
    }
}

const fn platform() -> Platform {
    if cfg!(target_os = "macos") {
        Platform::Macos
    } else if cfg!(windows) {
        Platform::Windows
    } else {
        Platform::Linux
    }
}

fn home() -> Option<RawPath> {
    std::env::home_dir().and_then(|home| crate::raw_path(&home).ok())
}

fn env_path(name: &str) -> Option<RawPath> {
    std::env::var_os(name).and_then(|value| crate::raw_path(Path::new(&value)).ok())
}

/// What the platform contributes: inventory reports, evidence, relationships.
struct Contributions {
    reports: Vec<InventoryReport>,
    evidence: Vec<Evidence>,
    relationships: Vec<Relationship>,
}

#[cfg(target_os = "macos")]
fn platform_contributions(
    entries: &[lumen_domain::FilesystemEntry],
    home: Option<&RawPath>,
    now: lumen_domain::Timestamp,
) -> Contributions {
    use lumen_application::CancelToken;
    use lumen_application::ports::InventorySource as _;
    use lumen_platform_macos::{AppInventory, LaunchdInventory, support::link_support_locations};
    let cancel = CancelToken::new();
    let reports = vec![
        AppInventory::new().collect(&cancel),
        LaunchdInventory::new().collect(&cancel),
    ];
    let apps: Vec<_> = reports
        .iter()
        .flat_map(|r| r.applications.iter().cloned())
        .collect();
    let mut evidence = Vec::new();
    let mut relationships = Vec::new();
    if let Some(home) = home {
        for link in link_support_locations(home, &apps, entries, now) {
            evidence.push(link.evidence);
            evidence.push(link.location);
            relationships.push(link.relationship);
        }
    }
    Contributions {
        reports,
        evidence,
        relationships,
    }
}

#[cfg(windows)]
fn platform_contributions(
    entries: &[lumen_domain::FilesystemEntry],
    _home: Option<&RawPath>,
    now: lumen_domain::Timestamp,
) -> Contributions {
    use lumen_application::CancelToken;
    use lumen_application::ports::InventorySource as _;
    use lumen_platform_windows::cleanup::{declared_caches, read_declarations};
    use lumen_platform_windows::{InstalledApps, Services, StartupEntries};
    let cancel = CancelToken::new();
    let reports = vec![
        InstalledApps.collect(&cancel),
        StartupEntries.collect(&cancel),
        Services.collect(&cancel),
    ];
    let (declarations, _status) = read_declarations();
    Contributions {
        reports,
        evidence: declared_caches(&declarations, entries, now),
        relationships: Vec::new(),
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
fn platform_contributions(
    _entries: &[lumen_domain::FilesystemEntry],
    _home: Option<&RawPath>,
    _now: lumen_domain::Timestamp,
) -> Contributions {
    Contributions {
        reports: Vec::new(),
        evidence: Vec::new(),
        relationships: Vec::new(),
    }
}

/// The platform's probes and their source names.
///
/// # Errors
///
/// [`AssessError::SourceName`] if a probe's name is invalid (never in
/// practice: the names are constants).
pub fn with_probes<T>(run: impl FnOnce(&Probes<'_>) -> T) -> Result<T, AssessError> {
    #[cfg(target_os = "macos")]
    let (in_use, in_use_name): (Box<dyn InUseProbe>, &str) = (
        Box::new(lumen_platform_macos::ProcessProbe),
        "macos.process-probe",
    );
    #[cfg(windows)]
    let (in_use, in_use_name): (Box<dyn InUseProbe>, &str) = (
        Box::new(lumen_platform_windows::RestartManagerProbe),
        "windows.restart-manager",
    );
    #[cfg(target_os = "linux")]
    let (in_use, in_use_name): (Box<dyn InUseProbe>, &str) =
        (Box::new(lumen_scan::ProcProbe), "linux.proc");
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    let (in_use, in_use_name): (Box<dyn InUseProbe>, &str) = (Box::new(NoInUseProbe), "none");
    #[cfg(unix)]
    let (ownership, ownership_name): (Box<dyn OwnershipProbe>, &str) =
        (Box::new(lumen_scan::UnixOwnershipProbe), "unix.owner");
    #[cfg(windows)]
    let (ownership, ownership_name): (Box<dyn OwnershipProbe>, &str) = (
        Box::new(lumen_platform_windows::HandleEnumerator),
        "windows.owner-sid",
    );
    let in_use_source = SourceName::new(in_use_name)?;
    let ownership_source = SourceName::new(ownership_name)?;
    Ok(run(&Probes {
        in_use: in_use.as_ref(),
        in_use_source: &in_use_source,
        ownership: ownership.as_ref(),
        ownership_source: &ownership_source,
    }))
}

/// Everything an assessment of the latest completed scan decides from; reused
/// to re-decide planned items at confirmation (roadmap 10.14).
#[derive(Debug)]
pub struct Assessment {
    /// The scan's roots.
    pub roots: Vec<RawPath>,
    /// Its entries.
    pub entries: Vec<lumen_domain::FilesystemEntry>,
    /// Its directory listings.
    pub listings: Vec<(RawPath, lumen_application::ports::DirOutcome)>,
    /// Gathered evidence and relationships.
    pub gathered: lumen_application::gather::Gathered,
    /// The graph loaded back from `index.db`.
    pub graph: lumen_graph::Graph,
    /// The machine.
    pub context: Context,
    /// Knowledge-base classes.
    pub knowledge: std::collections::BTreeMap<SourceName, lumen_policy::KnowledgeClass>,
    /// Whether every inventory source was read completely (for Jev's notes).
    pub inventories_complete: bool,
}

impl Assessment {
    /// The sources the policy decides from.
    pub fn sources(&self) -> Sources<'_> {
        Sources {
            graph: &self.graph,
            gathered: &self.gathered,
            scanned: &self.entries,
            listings: &self.listings,
            context: &self.context,
            knowledge: &self.knowledge,
        }
    }
}

/// Collects, gathers and records everything about the latest completed scan
/// in `db`.
///
/// # Errors
///
/// An [`AssessError`] if there is no completed scan, the database or the
/// knowledge base fails, or recording would leave less than
/// [`space::RESERVE`] free on the database's disk.
pub fn prepare(db: &Path, clock: &dyn Clock) -> Result<Assessment, AssessError> {
    let mut store = IndexStore::new(open_index(db)?);
    let scan = store
        .latest_completed_scan()?
        .ok_or(AssessError::NoCompletedScan)?;
    let roots = store.scan_roots(scan)?;
    let entries = store.entries(scan)?;
    let needed = u64::try_from(entries.len())
        .unwrap_or(u64::MAX)
        .saturating_mul(ASSESS_BYTES_PER_ENTRY);
    space::ensure(db, needed)?;
    let listings = store.listings(scan)?;
    let now = clock.now();
    let platform = platform();
    let home = home();
    let contributions = platform_contributions(&entries, home.as_ref(), now);
    let inventory: Inventory = summarize(&contributions.reports);
    let inventories_complete = !contributions.reports.is_empty()
        && contributions
            .reports
            .iter()
            .all(|r| r.status == lumen_domain::SourceStatus::Complete);
    let kb = KnowledgeBase::builtin()?;
    let env = Environment {
        platform,
        home: home.clone(),
        local_app_data: env_path("LOCALAPPDATA"),
        roaming_app_data: env_path("APPDATA"),
        vars: Vec::new(),
    };
    let mut evidence = contributions.evidence;
    evidence.extend(kb.matches(&env, &entries, now));
    let gathered = gather(
        &entries,
        &inventory,
        evidence,
        contributions.relationships,
        now,
    );
    store.record_evidence(&gathered.evidence)?;
    store.record_relationships(&gathered.relationships)?;
    let graph = store.load_graph()?;
    let mechanisms: BTreeSet<CleanupMechanism> = PlatformCapabilities::ceiling(platform)
        .mechanisms()
        .filter(|m| *m == CleanupMechanism::QuarantineRename)
        .collect();
    let context = Context {
        platform,
        home,
        windows_dir: env_path("WINDIR"),
        local_app_data: env.local_app_data.clone(),
        mechanisms,
        complete_sources: inventory.complete_sources.clone(),
    };
    Ok(Assessment {
        roots,
        entries,
        listings,
        gathered,
        graph,
        context,
        knowledge: kb.knowledge(),
        inventories_complete,
    })
}

/// What kind of item the evidence says `input`'s subject is, for Jev.
fn category(input: &lumen_policy::PolicyInput) -> lumen_domain::Category {
    use lumen_domain::{AppDataKind, Category, Fact};
    let about = || {
        input
            .evidence
            .iter()
            .filter(|e| e.subject == input.subject)
            .map(|e| &e.fact)
    };
    if about().any(|f| matches!(f, Fact::KnowledgeBaseMatch { .. })) {
        Category::DeveloperCache
    } else if about().any(|f| {
        matches!(
            f,
            Fact::VendorDeclaredCache { .. }
                | Fact::AppDataLocation {
                    kind: AppDataKind::Caches
                }
        )
    }) {
        Category::AppCache
    } else {
        Category::Other
    }
}

/// The Jev request for `input`: metadata only (ADR-0020), built by
/// `lumen-jev`. `None` if it cannot be built; the policy then decides alone.
fn jev_request(
    input: &lumen_policy::PolicyInput,
    item: u32,
    inventories_complete: bool,
    now: lumen_domain::Timestamp,
) -> Option<lumen_domain::JevRequest> {
    let observed = input.observed.as_ref()?;
    let candidate = lumen_jev::request::Candidate {
        item_ref: lumen_domain::ItemRef::new(format!("item-{item}")).ok()?,
        subject: &input.subject,
        category: category(input),
        path: input.paths.first(),
        size: &observed.reclaim,
        observed_completely: observed.complete,
        inventories_complete,
        evidence: &input.evidence,
    };
    let env = lumen_jev::request::Environment {
        home: input.context.home.as_ref(),
        now,
    };
    lumen_jev::request::build(&candidate, env).ok()
}

/// Assesses the latest completed scan in `db`, consulting `judge` when it is
/// configured (a `NoopJudge` never is).
///
/// # Errors
///
/// An [`AssessError`], as for [`prepare`].
#[tracing::instrument(name = "assess", skip_all)]
pub fn assess(
    db: &Path,
    clock: &dyn Clock,
    judge: &dyn lumen_application::ports::JudgeModel,
) -> Result<Vec<Assessed>, AssessError> {
    let timer = std::time::Instant::now();
    let assessment = prepare(db, clock)?;
    let sources = assessment.sources();
    let candidates = select(&assessment.gathered, &assessment.entries);
    let counter = std::cell::Cell::new(0_u32);
    let now = clock.now();
    let request = |input: &lumen_policy::PolicyInput| {
        counter.set(counter.get().saturating_add(1));
        jev_request(input, counter.get(), assessment.inventories_complete, now)
    };
    let assessed: Vec<Assessed> = with_probes(|probes| {
        candidates
            .into_iter()
            .filter_map(|candidate| {
                block_on(decide_candidate(
                    candidate, &sources, probes, judge, clock, &request,
                ))
                .ok()
            })
            .collect()
    })?;
    let count = |verdict: lumen_domain::Verdict| {
        assessed
            .iter()
            .filter(|a| a.decision.verdict() == verdict)
            .count()
    };
    tracing::info!(
        candidates = assessed.len(),
        keep = count(lumen_domain::Verdict::Keep),
        review = count(lumen_domain::Verdict::Review),
        quarantine = count(lumen_domain::Verdict::Quarantine),
        jev_consulted = assessed.iter().filter(|a| a.trace.is_some()).count(),
        elapsed_ms = u64::try_from(timer.elapsed().as_millis()).unwrap_or(u64::MAX),
        "assessment finished"
    );
    Ok(assessed)
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use lumen_domain::{
        AccessState, AppDataKind, Basis, ByteCount, Category, EntryKind, EntryTimes, EvidenceHash,
        Fact, FileId, FileIdentity, FilesystemEntry, Protection, Provenance, ReclaimEstimate,
        SizeFacts, SizeFlags, Subject, Timestamp, VolumeId,
    };
    use lumen_policy::{Observed, PolicyInput};

    use super::*;

    fn input(facts: &[Fact]) -> PolicyInput {
        let identity = FileIdentity {
            volume: VolumeId::new("v").unwrap_or_else(|e| unreachable!("{e}")),
            file: FileId::new(1),
        };
        let subject = Subject::File {
            identity: identity.clone(),
        };
        let size = SizeFacts {
            identity,
            logical: ByteCount::new(4096),
            allocated: ByteCount::new(4096),
            private: None,
            clone_id: None,
            link_count: NonZeroU32::MIN,
            flags: SizeFlags::default(),
        };
        let path = RawPath::from_unix_bytes(b"/Users/ana/Library/Caches/x".to_vec())
            .unwrap_or_else(|e| unreachable!("{e}"));
        PolicyInput {
            subject: subject.clone(),
            evidence_hash: EvidenceHash::from_bytes([1; 32]),
            evidence: facts
                .iter()
                .map(|fact| Evidence {
                    subject: subject.clone(),
                    fact: fact.clone(),
                    basis: Basis::Observed,
                    provenance: Provenance {
                        source: SourceName::new("test").unwrap_or_else(|e| unreachable!("{e}")),
                        observed_at: Timestamp::UNIX_EPOCH,
                    },
                })
                .collect(),
            paths: vec![path.clone()],
            context: Context {
                platform: Platform::Macos,
                home: RawPath::from_unix_bytes(b"/Users/ana".to_vec()).ok(),
                windows_dir: None,
                local_app_data: None,
                mechanisms: BTreeSet::new(),
                complete_sources: BTreeSet::new(),
            },
            observed: Some(Observed {
                entry: FilesystemEntry {
                    path,
                    kind: EntryKind::Directory,
                    size: size.clone(),
                    times: EntryTimes::default(),
                    protection: Protection::default(),
                    access: AccessState::Readable,
                },
                complete: true,
                reclaim: ReclaimEstimate::for_selection([&size]),
            }),
            knowledge: std::collections::BTreeMap::new(),
            jev: None,
        }
    }

    #[test]
    fn the_category_follows_the_evidence() {
        let kb = Fact::KnowledgeBaseMatch {
            entry: SourceName::new("npm-cache").unwrap_or_else(|e| unreachable!("{e}")),
        };
        let caches = Fact::AppDataLocation {
            kind: AppDataKind::Caches,
        };
        assert_eq!(category(&input(&[kb])), Category::DeveloperCache);
        assert_eq!(category(&input(&[caches])), Category::AppCache);
        assert_eq!(
            category(&input(&[Fact::CloudBacked(false)])),
            Category::Other
        );
    }

    #[test]
    fn a_request_is_built_from_metadata_only_when_observed() {
        let with = input(&[Fact::CloudBacked(false)]);
        let request = jev_request(&with, 1, true, Timestamp::UNIX_EPOCH);
        assert!(request.is_some(), "an observed candidate gets a request");
        let mut unobserved = with;
        unobserved.observed = None;
        assert!(jev_request(&unobserved, 2, true, Timestamp::UNIX_EPOCH).is_none());
    }
}
