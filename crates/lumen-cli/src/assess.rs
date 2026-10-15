//! `lumen assess`: decides the latest completed scan, read-only (roadmap
//! 10.13b).
//!
//! The composition: the platform's inventory sources and evidence producers
//! (the developer knowledge base everywhere; application support locations on
//! macOS; declared caches on Windows) run over the stored scan; their output is
//! gathered with the scan's own facts and recorded in `index.db`; candidates
//! are selected, probed (in use, ownership) and decided by the policy, without
//! Jev (roadmap 10.17). Nothing is moved; the report lists each candidate with
//! its verdict, the space it would free and the reasons.

use std::collections::BTreeSet;
use std::error::Error;
use std::path::Path;
use std::pin::pin;
use std::sync::Arc;
use std::task::{Context as TaskContext, Poll, Wake, Waker};
use std::thread::{self, Thread};

use lumen_application::assess::{Assessed, Probes, Sources, decide_candidate, select};
use lumen_application::collect::{Inventory, summarize};
use lumen_application::gather::gather;
use lumen_application::ports::{Clock, InUseProbe, InventoryReport, NoopJudge, OwnershipProbe};
use lumen_devkb::{Environment, KnowledgeBase};
use lumen_domain::{
    ByteCount, CleanupMechanism, Evidence, Platform, PlatformCapabilities, RawPath, Relationship,
    SourceName, Verdict,
};
use lumen_policy::Context;
use lumen_store_sqlite::{IndexStore, open_index};
use serde_json::{Value, json};

/// Runs `future` to completion on this thread.
pub(crate) fn block_on<F: Future>(future: F) -> F::Output {
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
#[cfg(not(any(target_os = "macos", windows)))]
struct NoInUseProbe;

#[cfg(not(any(target_os = "macos", windows)))]
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
    std::env::home_dir().and_then(|home| crate::report::raw_path(&home).ok())
}

fn env_path(name: &str) -> Option<RawPath> {
    std::env::var_os(name).and_then(|value| crate::report::raw_path(Path::new(&value)).ok())
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
pub(crate) fn with_probes<T>(run: impl FnOnce(&Probes<'_>) -> T) -> Result<T, Box<dyn Error>> {
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
    #[cfg(not(any(target_os = "macos", windows)))]
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
pub(crate) struct Assessment {
    /// The scan's roots.
    #[expect(dead_code, reason = "read by `lumen quarantine`, roadmap 10.14b")]
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
}

impl Assessment {
    /// The sources the policy decides from.
    pub(crate) fn sources(&self) -> Sources<'_> {
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
pub(crate) fn prepare(db: &Path, clock: &dyn Clock) -> Result<Assessment, Box<dyn Error>> {
    let mut store = IndexStore::new(open_index(db)?);
    let scan = store
        .latest_completed_scan()?
        .ok_or("no completed scan in this database; run `lumen scan` first")?;
    let roots = store.scan_roots(scan)?;
    let entries = store.entries(scan)?;
    let listings = store.listings(scan)?;
    let now = clock.now();
    let platform = platform();
    let home = home();
    let contributions = platform_contributions(&entries, home.as_ref(), now);
    let inventory: Inventory = summarize(&contributions.reports);
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
    })
}

/// Assesses the latest completed scan in `db`.
pub(crate) fn assess(db: &Path, clock: &dyn Clock) -> Result<Vec<Assessed>, Box<dyn Error>> {
    let assessment = prepare(db, clock)?;
    let sources = assessment.sources();
    let candidates = select(&assessment.gathered, &assessment.entries);
    with_probes(|probes| {
        candidates
            .into_iter()
            .filter_map(|candidate| {
                block_on(decide_candidate(
                    candidate,
                    &sources,
                    probes,
                    &NoopJudge,
                    clock,
                    &|_| None,
                ))
                .ok()
            })
            .collect()
    })
}

fn verdict_text(verdict: Verdict) -> &'static str {
    match verdict {
        Verdict::Keep => "KEEP",
        Verdict::Review => "REVIEW",
        Verdict::Quarantine => "QUARANTINE",
    }
}

fn reclaim(assessed: &Assessed) -> ByteCount {
    assessed
        .input
        .observed
        .as_ref()
        .map_or(ByteCount::ZERO, |o| o.reclaim.reclaimable_now)
}

/// The report as text.
pub(crate) fn to_text(assessed: &[Assessed]) -> String {
    let mut lines = Vec::new();
    for item in assessed {
        lines.push(format!(
            "{:<10} {:>12} B  {}",
            verdict_text(item.decision.verdict()),
            reclaim(item).get(),
            item.candidate.path.display()
        ));
        lines.extend(
            item.reasons
                .iter()
                .map(|reason| format!("           - {reason}")),
        );
    }
    let proposed: u64 = assessed
        .iter()
        .filter(|a| a.decision.verdict() == Verdict::Quarantine)
        .map(|a| reclaim(a).get())
        .sum();
    lines.push(format!(
        "{} candidates; {proposed} B proposed for quarantine (nothing has been moved)",
        assessed.len()
    ));
    lines.join("\n") + "\n"
}

/// The report as JSON.
pub(crate) fn to_json(assessed: &[Assessed]) -> Value {
    json!({
        "candidates": assessed.iter().map(|item| json!({
            "path": item.candidate.path.display(),
            "verdict": verdict_text(item.decision.verdict()),
            "reclaimable_bytes": reclaim(item).get(),
            "evidence_hash": item.decision.evidence_hash().to_string(),
            "reasons": item.reasons,
        })).collect::<Vec<_>>(),
    })
}
