//! Assessing the latest completed scan, read-only (roadmap 10.13b, 11.3).
//!
//! The composition: the platform's inventory sources and evidence producers
//! (the developer knowledge base everywhere; application support locations on
//! macOS; declared caches on Windows) run over the stored scan; their output is
//! gathered with the scan's own facts and recorded in `index.db`; candidates
//! are selected, probed (in use, ownership) and decided by the policy, without
//! Jev (roadmap 10.17). Nothing is moved; each candidate comes back with its
//! decision and the reasons, for the caller to present.

use std::collections::{BTreeSet, HashSet};
use std::path::Path;
use std::pin::pin;
use std::sync::{Arc, OnceLock};
use std::task::{Context as TaskContext, Poll, Wake, Waker};
use std::thread::{self, Thread};

use lumen_application::assess::{Assessed, Probes, Sources, decide_candidate, nested, select};
use lumen_application::collect::{Inventory, ScanDomains, summarize};
use lumen_application::gather::gather;
use lumen_application::observe::below;
use lumen_application::ports::{Clock, InUseProbe, InventoryReport, OwnershipProbe};
use lumen_devkb::{Environment, KnowledgeBase, Markers, Root};
use lumen_domain::{
    CleanupMechanism, Evidence, Fact, Platform, PlatformCapabilities, RawPath, Relationship,
    ScanId, SourceName, SourceNameError, Subject,
};
use lumen_policy::Context;
use lumen_store_sqlite::{IndexStore, StoreError, open_index};

use crate::space::{self, RECORD_BYTES, SpaceError};

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
    std::env::var_os(name).and_then(|value| cache_path(Path::new(&value)))
}

fn cache_path(path: &Path) -> Option<RawPath> {
    path.is_absolute()
        .then(|| crate::raw_path(path).ok())
        .flatten()
}

/// Read only directory variables declared by applicable knowledge-base locations.
/// Do not enumerate the process environment: it can contain credentials.
fn cache_vars(
    kb: &KnowledgeBase,
    platform: Platform,
    mut read: impl FnMut(&str) -> Option<RawPath>,
) -> Vec<(String, RawPath)> {
    let names: BTreeSet<_> = kb
        .entries()
        .iter()
        .flat_map(|entry| &entry.locations)
        .filter(|location| location.platforms.contains(&platform))
        .filter_map(|location| match &location.root {
            Root::EnvVar { name } => Some(name),
            _ => None,
        })
        .collect();
    names
        .into_iter()
        .filter_map(|name| read(name).map(|path| (name.clone(), path)))
        .collect()
}

/// Where the knowledge base's locations are, from this process's
/// environment: the one a tool's own command inherits when it runs.
fn environment(kb: &KnowledgeBase, platform: Platform) -> Environment {
    Environment {
        platform,
        home: home(),
        local_app_data: env_path("LOCALAPPDATA"),
        roaming_app_data: env_path("APPDATA"),
        vars: cache_vars(kb, platform, env_path),
    }
}

/// The platform's inventory, read once per assessment, and what its
/// producers need to link scanned entries to it.
struct PlatformInventory {
    reports: Vec<InventoryReport>,
    #[cfg(target_os = "macos")]
    apps: Vec<lumen_domain::Application>,
    #[cfg(windows)]
    declarations: Vec<lumen_platform_windows::cleanup::CacheDeclaration>,
}

/// What the platform's producers note in a first pass over the whole scan,
/// because it qualifies entries in other chunks.
#[derive(Default)]
struct PlatformMarkers {
    /// Apps' `Caches` folders that hold a browser profile.
    #[cfg(target_os = "macos")]
    web_profiles: lumen_platform_macos::web_profile::WebProfiles,
}

#[cfg(target_os = "macos")]
fn add_platform_markers(
    markers: &mut PlatformMarkers,
    entries: &[lumen_domain::FilesystemEntry],
    home: Option<&RawPath>,
) {
    if let Some(home) = home {
        markers.web_profiles.add(home, entries);
    }
}

#[cfg(not(target_os = "macos"))]
fn add_platform_markers(
    _markers: &mut PlatformMarkers,
    _entries: &[lumen_domain::FilesystemEntry],
    _home: Option<&RawPath>,
) {
}

/// What the platform's producers say about one chunk of scanned entries.
#[derive(Default)]
struct Contributions {
    evidence: Vec<Evidence>,
    relationships: Vec<Relationship>,
}

#[cfg(target_os = "macos")]
fn platform_inventory() -> PlatformInventory {
    use lumen_application::CancelToken;
    use lumen_application::ports::InventorySource as _;
    use lumen_platform_macos::{AppInventory, LaunchdInventory};
    let cancel = CancelToken::new();
    let reports = vec![
        AppInventory::new().collect(&cancel),
        LaunchdInventory::new().collect(&cancel),
    ];
    let apps = reports
        .iter()
        .flat_map(|r| r.applications.iter().cloned())
        .collect();
    PlatformInventory { reports, apps }
}

#[cfg(target_os = "macos")]
fn platform_contributions(
    inventory: &PlatformInventory,
    markers: &PlatformMarkers,
    entries: &[lumen_domain::FilesystemEntry],
    home: Option<&RawPath>,
    times: (lumen_domain::Timestamp, lumen_domain::Timestamp),
) -> Contributions {
    use lumen_platform_macos::support::link_support_locations;
    let (scanned_at, now) = times;
    let mut contributions = Contributions::default();
    if let Some(home) = home {
        for link in link_support_locations(home, &inventory.apps, entries, now) {
            contributions.evidence.push(link.evidence);
            contributions.evidence.push(link.location);
            contributions.relationships.push(link.relationship);
        }
        // Derived from the scan alone, so dated by it.
        contributions
            .evidence
            .extend(markers.web_profiles.evidence(home, entries, scanned_at));
    }
    contributions
}

#[cfg(windows)]
fn platform_inventory() -> PlatformInventory {
    use lumen_application::CancelToken;
    use lumen_application::ports::InventorySource as _;
    use lumen_platform_windows::cleanup::read_declarations;
    use lumen_platform_windows::{InstalledApps, Services, StartupEntries};
    let cancel = CancelToken::new();
    let reports = vec![
        InstalledApps.collect(&cancel),
        StartupEntries.collect(&cancel),
        Services.collect(&cancel),
    ];
    let (declarations, _status) = read_declarations();
    PlatformInventory {
        reports,
        declarations,
    }
}

#[cfg(windows)]
fn platform_contributions(
    inventory: &PlatformInventory,
    _markers: &PlatformMarkers,
    entries: &[lumen_domain::FilesystemEntry],
    _home: Option<&RawPath>,
    (_, now): (lumen_domain::Timestamp, lumen_domain::Timestamp),
) -> Contributions {
    use lumen_platform_windows::cleanup::declared_caches;
    Contributions {
        evidence: declared_caches(&inventory.declarations, entries, now),
        relationships: Vec::new(),
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
fn platform_inventory() -> PlatformInventory {
    PlatformInventory {
        reports: Vec::new(),
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
fn platform_contributions(
    _inventory: &PlatformInventory,
    _markers: &PlatformMarkers,
    _entries: &[lumen_domain::FilesystemEntry],
    _home: Option<&RawPath>,
    _times: (lumen_domain::Timestamp, lumen_domain::Timestamp),
) -> Contributions {
    Contributions::default()
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

/// Scanned entries are read this many at a time, so an assessment's memory
/// follows the page and the candidates, not the size of the scan.
const PAGE: usize = 50_000;

/// Work currently running; percentages apply within one stage, not the scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssessStage {
    /// Reading installed applications and services; no known total.
    Inventory,
    /// Finding project markers in the recorded entries.
    Markers,
    /// Matching recorded entries to evidence producers.
    Evidence,
    /// Recording evidence and loading relationships; no known total.
    Graph,
    /// Deciding candidates, including any optional judge request.
    Deciding,
    /// Describing selected review items without changing decisions.
    Describing,
}

/// Advisory counts only: no paths, contents or action authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssessProgress {
    /// Current work.
    pub stage: AssessStage,
    /// Units actually processed, including unsuccessful attempts.
    pub completed: u64,
    /// Units in this stage, or `None` when they cannot be counted.
    pub total: Option<u64>,
}

fn report(
    progress: &mut dyn FnMut(AssessProgress),
    stage: AssessStage,
    completed: u64,
    total: Option<u64>,
) {
    progress(AssessProgress {
        stage,
        completed,
        total,
    });
}

/// One candidate's part of the scan: the candidate, everything below it, and
/// their listings, read from `index.db` when the candidate is decided.
#[derive(Debug, Default)]
pub struct Subtree {
    entries: Vec<lumen_domain::FilesystemEntry>,
    listings: Vec<(RawPath, lumen_application::ports::DirOutcome)>,
}

impl Subtree {
    /// Leaves out the `nested` paths and everything below them.
    fn carve(&mut self, nested: &[&RawPath]) {
        if nested.is_empty() {
            return;
        }
        let outside = |p: &RawPath| !nested.iter().any(|n| p == *n || below(p, n));
        self.entries.retain(|entry| outside(&entry.path));
        self.listings.retain(|(listed, _)| outside(listed));
    }
}

/// Everything an assessment of the latest completed scan decides from; reused
/// to re-decide planned items at confirmation (roadmap 10.14).
#[derive(Debug)]
pub struct Assessment {
    /// The scan's roots.
    pub roots: Vec<RawPath>,
    /// The entries a producer said something about or a relationship
    /// connects: every candidate is one of them. The rest of the scan stays
    /// in `index.db` and is read per candidate ([`Assessment::subtree`]).
    pub entries: Vec<lumen_domain::FilesystemEntry>,
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
    /// The volumes with a quarantine store, or `None` for every volume
    /// ([`crate::Stores::volumes`], ADR-0046).
    pub store_volumes: Option<BTreeSet<lumen_domain::VolumeId>>,
    store: IndexStore,
    scan: ScanId,
}

impl Assessment {
    /// The part of the scan at and below `path`, with its listings.
    ///
    /// # Errors
    ///
    /// [`AssessError::Store`] if `index.db` cannot be read.
    pub fn subtree(&self, path: &RawPath) -> Result<Subtree, AssessError> {
        // The store returns whole segments (`/a/b`, not `/a/bc`); the filter
        // keeps that a property of this function, not of the store's layout.
        let within = |p: &RawPath| p == path || below(p, path);
        Ok(Subtree {
            entries: self
                .store
                .entries_within(self.scan, path)?
                .into_iter()
                .filter(|entry| within(&entry.path))
                .collect(),
            listings: self
                .store
                .listings_within(self.scan, path)?
                .into_iter()
                .filter(|(listed, _)| within(listed))
                .collect(),
        })
    }

    /// The part of the scan at and below `path` without the `nested`
    /// candidates and everything below them: those are decided on their own,
    /// and the candidate at `path` is decided for the rest.
    ///
    /// # Errors
    ///
    /// [`AssessError::Store`] if `index.db` cannot be read.
    pub fn subtree_without(
        &self,
        path: &RawPath,
        nested: &[&RawPath],
    ) -> Result<Subtree, AssessError> {
        let mut subtree = self.subtree(path)?;
        subtree.carve(nested);
        Ok(subtree)
    }

    /// The sources the policy decides one candidate from: the assessment's
    /// evidence and graph, and the candidate's `subtree`.
    pub fn sources<'a>(&'a self, subtree: &'a Subtree) -> Sources<'a> {
        Sources {
            graph: &self.graph,
            gathered: &self.gathered,
            scanned: &subtree.entries,
            listings: &subtree.listings,
            context: &self.context,
            knowledge: &self.knowledge,
            store_volumes: self.store_volumes.as_ref(),
        }
    }
}

/// Calls `each` with the scan's entries, a page at a time, in the store's order.
fn for_each_page(
    store: &IndexStore,
    scan: ScanId,
    mut each: impl FnMut(&[lumen_domain::FilesystemEntry]),
) -> Result<(), StoreError> {
    let mut after: Option<RawPath> = None;
    loop {
        let page = store.entries_page(scan, after.as_ref(), PAGE)?;
        let Some(last) = page.last() else {
            return Ok(());
        };
        after = Some(last.path.clone());
        each(&page);
    }
}

/// What the producers found in the whole scan, the entries it is about, and
/// the scan's cloud domains and browser profiles.
struct Found {
    evidence: Vec<Evidence>,
    relationships: Vec<Relationship>,
    entries: Vec<lumen_domain::FilesystemEntry>,
    domains: ScanDomains,
}

/// Runs the platform's producers and the knowledge base over the scan, a page
/// at a time: first the knowledge base's and the platform's markers, the
/// cloud domains and the browser profiles (a marker or a sync root can
/// qualify an entry in another page), then the producers. Keeps only the entries their
/// output names; no other entry's facts could reach a decision (roadmap 10.8).
fn find(
    store: &IndexStore,
    scan: ScanId,
    inventory: &PlatformInventory,
    matcher: &lumen_devkb::Matcher<'_>,
    home: Option<&RawPath>,
    times: (lumen_domain::Timestamp, lumen_domain::Timestamp),
    progress: &mut dyn FnMut(AssessProgress),
) -> Result<Found, StoreError> {
    let (scanned_at, _) = times;
    let total = Some(store.entry_count(scan)?);
    let mut completed = 0_u64;
    report(progress, AssessStage::Markers, completed, total);
    let mut markers = Markers::new();
    let mut platform_markers = PlatformMarkers::default();
    let mut domains = ScanDomains::new();
    for_each_page(store, scan, |page| {
        matcher.add_markers(page, &mut markers);
        domains.add(page);
        add_platform_markers(&mut platform_markers, page, home);
        completed = completed.saturating_add(u64::try_from(page.len()).unwrap_or(u64::MAX));
        report(progress, AssessStage::Markers, completed, total);
    })?;
    let mut found = Found {
        evidence: Vec::new(),
        relationships: Vec::new(),
        entries: Vec::new(),
        domains,
    };
    completed = 0;
    report(progress, AssessStage::Evidence, completed, total);
    for_each_page(store, scan, |page| {
        let produced = platform_contributions(inventory, &platform_markers, page, home, times);
        let known = matcher.matches(page, &markers, scanned_at);
        let named: HashSet<&Subject> = produced
            .evidence
            .iter()
            .chain(&known)
            .map(|record| &record.subject)
            .chain(
                produced
                    .relationships
                    .iter()
                    .flat_map(|r| [r.from(), r.to()]),
            )
            .collect();
        found.entries.extend(
            page.iter()
                .filter(|entry| {
                    named.contains(&Subject::File {
                        identity: entry.identity().clone(),
                    })
                })
                .cloned(),
        );
        found.evidence.extend(produced.evidence);
        found.evidence.extend(known);
        found.relationships.extend(produced.relationships);
        completed = completed.saturating_add(u64::try_from(page.len()).unwrap_or(u64::MAX));
        report(progress, AssessStage::Evidence, completed, total);
    })?;
    Ok(found)
}

/// Collects, gathers and records everything about the latest completed scan
/// in `db`, reading the scan a page at a time.
///
/// # Errors
///
/// An [`AssessError`] if there is no completed scan, the database or the
/// knowledge base fails, or recording would leave less than
/// [`space::RESERVE`] free on the database's disk.
pub fn prepare(db: &Path, clock: &dyn Clock) -> Result<Assessment, AssessError> {
    prepare_with_progress(db, clock, &mut |_| {})
}

fn prepare_with_progress(
    db: &Path,
    clock: &dyn Clock,
    progress: &mut dyn FnMut(AssessProgress),
) -> Result<Assessment, AssessError> {
    report(progress, AssessStage::Inventory, 0, None);
    let mut store = IndexStore::new(open_index(db)?);
    let scan = store
        .latest_completed_scan()?
        .ok_or(AssessError::NoCompletedScan)?;
    let roots = store.scan_roots(scan)?;
    let now = clock.now();
    let platform = platform();
    let home = home();
    let platform_inventory = platform_inventory();
    let inventory: Inventory = summarize(&platform_inventory.reports);
    let inventories_complete = !platform_inventory.reports.is_empty()
        && platform_inventory
            .reports
            .iter()
            .all(|r| r.status == lumen_domain::SourceStatus::Complete);
    let kb = KnowledgeBase::builtin()?;
    let env = environment(&kb, platform);
    // The scan's facts and what is derived from them alone are dated by the
    // scan, so assessing it again records nothing new.
    let scanned_at = store.scan_finished_at(scan)?.unwrap_or(now);
    let found = find(
        &store,
        scan,
        &platform_inventory,
        &kb.matcher(&env),
        home.as_ref(),
        (scanned_at, now),
        progress,
    )?;
    report(progress, AssessStage::Graph, 0, None);
    let gathered = gather(
        &found.entries,
        &found.domains,
        &inventory,
        found.evidence,
        found.relationships,
        scanned_at,
        now,
    );
    let records = gathered
        .evidence
        .len()
        .saturating_add(gathered.relationships.len());
    space::ensure(
        db,
        u64::try_from(records)
            .unwrap_or(u64::MAX)
            .saturating_mul(RECORD_BYTES),
    )?;
    store.record_evidence(&gathered.evidence)?;
    store.record_relationships(&gathered.relationships)?;
    let graph = store.load_graph()?;
    // Without a store location nothing can be quarantined anyway; the
    // decisions then fall back to every volume and a shared computer.
    let stores = crate::store::stores().ok();
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
        single_user: stores.as_ref().is_some_and(|s| s.single_user),
    };
    Ok(Assessment {
        roots,
        entries: found.entries,
        gathered,
        graph,
        context,
        knowledge: kb.knowledge(),
        inventories_complete,
        store_volumes: stores.and_then(|s| s.volumes),
        store,
        scan,
    })
}

/// A developer tool's own cleanup command: a literal argument vector from the
/// knowledge base, never a shell string. Shown with its item; run only when
/// the user asks, through [`crate::tools`] (ADR-0049).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolGuidance {
    /// The knowledge-base entry it comes from, which names it to run it.
    pub entry: String,
    /// The tool's display name, e.g. `Xcode`.
    pub tool: String,
    /// The program, e.g. `xcrun`.
    pub program: String,
    /// Its arguments, e.g. `["simctl", "delete", "unavailable"]`.
    pub args: Vec<String>,
    /// The item cannot be regenerated, so the command removes data, not
    /// just caches (Docker's disk image).
    pub removes_data: bool,
}

/// The built-in knowledge base, loaded once; `None` if it does not load.
pub(crate) fn builtin_knowledge_base() -> Option<&'static KnowledgeBase> {
    static KNOWLEDGE_BASE: OnceLock<Option<KnowledgeBase>> = OnceLock::new();
    KNOWLEDGE_BASE
        .get_or_init(|| KnowledgeBase::builtin().ok())
        .as_ref()
}

/// The cleanup command of the tool whose knowledge-base entry `assessed`
/// matched, if that entry has one and it acts on `assessed` here: the first
/// such entry in evidence order. `None` if no entry with a command matched,
/// the command would clear another folder the tool is configured to use, or
/// the knowledge base does not load.
pub fn tool_action(assessed: &Assessed) -> Option<ToolGuidance> {
    let kb = builtin_knowledge_base()?;
    let subject = Subject::File {
        identity: assessed.candidate.identity().clone(),
    };
    let env = std::cell::OnceCell::new();
    tool_guidance(kb, &subject, &assessed.input.evidence, |entry| {
        let env = env.get_or_init(|| environment(kb, platform()));
        kb.command_acts_on(env, &entry.id, &assessed.candidate.path)
    })
}

/// The first knowledge-base entry matched by `subject` itself in `evidence`
/// that has an official cleanup command which `acts` on it, as guidance.
fn tool_guidance(
    kb: &KnowledgeBase,
    subject: &Subject,
    evidence: &[Evidence],
    acts: impl Fn(&lumen_devkb::Entry) -> bool,
) -> Option<ToolGuidance> {
    evidence
        .iter()
        .filter(|record| &record.subject == subject)
        .filter_map(|record| match &record.fact {
            Fact::KnowledgeBaseMatch { entry } => kb.get(entry.as_str()),
            _ => None,
        })
        .filter(|entry| entry.official_action.is_some() && acts(entry))
        .find_map(|entry| {
            entry.official_action.as_ref().map(|action| ToolGuidance {
                entry: entry.id.clone(),
                tool: entry.tool.clone(),
                program: action.program.clone(),
                args: action.args.clone(),
                removes_data: entry.safety_class == lumen_devkb::SafetyClass::NotRegenerable,
            })
        })
}

/// How an item is grouped when results are presented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupKind {
    /// A developer tool's cache or build output (a knowledge-base match).
    Developer,
    /// An application's cache (its `Caches` folder, or a vendor declaration).
    AppCache,
    /// An application's other data: container, support files, preferences.
    AppData,
    /// Anything else.
    Other,
}

/// The group an item is presented in, and what it belongs to when known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    /// The kind of group.
    pub kind: GroupKind,
    /// The developer tool (e.g. `Xcode`) or the owning application's
    /// identifier (e.g. `com.docker.docker`), if the evidence names one. An
    /// identifier is untrusted text from the filesystem: shown, never acted on.
    pub source: Option<String>,
}

/// The group `assessed` is presented in.
pub fn group(assessed: &Assessed) -> Group {
    let subject = Subject::File {
        identity: assessed.candidate.identity().clone(),
    };
    group_of(builtin_knowledge_base(), &subject, &assessed.input.evidence)
}

/// The group of `subject`, from the evidence about it alone: a knowledge-base
/// match first (named after its tool), then an application data location
/// (named after the owning application, when known).
fn group_of(kb: Option<&KnowledgeBase>, subject: &Subject, evidence: &[Evidence]) -> Group {
    use lumen_domain::AppDataKind;
    let about = || {
        evidence
            .iter()
            .filter(|record| &record.subject == subject)
            .map(|record| &record.fact)
    };
    let tool = about().find_map(|fact| match fact {
        Fact::KnowledgeBaseMatch { entry } => Some(
            kb.and_then(|kb| kb.get(entry.as_str()))
                .map_or_else(|| entry.as_str().to_owned(), |e| e.tool.clone()),
        ),
        _ => None,
    });
    if let Some(tool) = tool {
        return Group {
            kind: GroupKind::Developer,
            source: Some(tool),
        };
    }
    let owner = about().find_map(|fact| match fact {
        Fact::OwningApplication(app) => Some(app.value.raw().to_owned()),
        _ => None,
    });
    let cache = about().any(|fact| {
        matches!(
            fact,
            Fact::VendorDeclaredCache { .. }
                | Fact::AppDataLocation {
                    kind: AppDataKind::Caches
                }
        )
    });
    // A cache that holds a browser profile is kept as the app's data.
    let cache = cache && !about().any(|fact| matches!(fact, Fact::BrowserProfile { .. }));
    let data = about().any(|fact| matches!(fact, Fact::AppDataLocation { .. }));
    let kind = if cache {
        GroupKind::AppCache
    } else if data {
        GroupKind::AppData
    } else {
        GroupKind::Other
    };
    Group {
        kind,
        source: owner,
    }
}

/// What kind of item the evidence says `input`'s subject is, for Jev.
fn category(input: &lumen_policy::PolicyInput) -> lumen_domain::Category {
    use lumen_domain::{AppDataKind, Category};
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
/// configured (a `NoopJudge` never is), then asking `describer` what kind of
/// item each of up to 20 of the largest review items is (ADR-0041). A
/// description is a note beside the decision and never changes it.
///
/// # Errors
///
/// An [`AssessError`], as for [`prepare`].
#[tracing::instrument(name = "assess", skip_all)]
pub fn assess(
    db: &Path,
    clock: &dyn Clock,
    judge: &dyn lumen_application::ports::JudgeModel,
    describer: &dyn lumen_application::ports::Describer,
) -> Result<Vec<Assessed>, AssessError> {
    assess_with_progress(db, clock, judge, describer, &mut |_| {})
}

/// Like [`assess`], reporting actual work at stage boundaries and after each
/// page or candidate. The observer cannot change decisions. It may stop
/// listening without stopping the assessment.
///
/// # Errors
///
/// An [`AssessError`], as for [`prepare`].
#[tracing::instrument(name = "assess_with_progress", skip_all)]
pub fn assess_with_progress(
    db: &Path,
    clock: &dyn Clock,
    judge: &dyn lumen_application::ports::JudgeModel,
    describer: &dyn lumen_application::ports::Describer,
    progress: &mut dyn FnMut(AssessProgress),
) -> Result<Vec<Assessed>, AssessError> {
    let timer = std::time::Instant::now();
    let assessment = prepare_with_progress(db, clock, progress)?;
    let candidates = select(&assessment.gathered, &assessment.entries);
    let total = Some(u64::try_from(candidates.len()).unwrap_or(u64::MAX));
    report(progress, AssessStage::Deciding, 0, total);
    let counter = std::cell::Cell::new(0_u32);
    let now = clock.now();
    let request = |input: &lumen_policy::PolicyInput| {
        counter.set(counter.get().saturating_add(1));
        jev_request(input, counter.get(), assessment.inventories_complete, now)
    };
    let assessed = with_probes(|probes| -> Result<Vec<Assessed>, AssessError> {
        let mut assessed = Vec::new();
        for (index, candidate) in candidates.iter().enumerate() {
            // Only this candidate's part of the scan is read, and dropped
            // after its decision; candidates inside it are decided on their
            // own, so its decision covers the rest.
            let inner = nested(candidate, &candidates);
            let subtree = assessment.subtree_without(&candidate.path, &inner)?;
            let sources = assessment.sources(&subtree);
            if let Ok(decided) = block_on(decide_candidate(
                candidate, &sources, probes, judge, clock, &request,
            )) {
                assessed.push(decided);
            }
            report(
                progress,
                AssessStage::Deciding,
                u64::try_from(index + 1).unwrap_or(u64::MAX),
                total,
            );
        }
        Ok(assessed)
    })??;
    let mut assessed = assessed;
    block_on(lumen_application::describe::describe_with_progress(
        describer,
        clock,
        &mut assessed,
        &request,
        &mut |completed, total| {
            report(
                progress,
                AssessStage::Describing,
                u64::try_from(completed).unwrap_or(u64::MAX),
                Some(u64::try_from(total).unwrap_or(u64::MAX)),
            );
        },
    ));
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
        jev_described = assessed.iter().filter(|a| a.note_trace.is_some()).count(),
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
        SizeFacts, SizeFlags, Subject, Timestamp, Verdict, VolumeId,
    };
    use lumen_policy::{Observed, PolicyInput};

    use super::*;

    fn scanned(path: &str, file: u128, bytes: u64) -> FilesystemEntry {
        let size = SizeFacts {
            identity: FileIdentity {
                volume: VolumeId::new("v").unwrap_or_else(|e| unreachable!("{e}")),
                file: FileId::new(file),
            },
            logical: ByteCount::new(bytes),
            allocated: ByteCount::new(bytes),
            private: None,
            clone_id: None,
            link_count: NonZeroU32::MIN,
            flags: SizeFlags::default(),
        };
        FilesystemEntry {
            path: RawPath::from_unix_bytes(path.as_bytes().to_vec())
                .unwrap_or_else(|e| unreachable!("{e}")),
            kind: if bytes == 0 {
                EntryKind::Directory
            } else {
                EntryKind::File
            },
            size,
            times: EntryTimes::default(),
            protection: Protection::default(),
            access: AccessState::Readable,
        }
    }

    #[test]
    fn custom_cache_roots_reach_the_real_matcher() -> Result<(), Box<dyn std::error::Error>> {
        let kb = KnowledgeBase::builtin()?;
        let mut read_names = Vec::new();
        let vars = cache_vars(&kb, Platform::Linux, |name| {
            read_names.push(name.to_owned());
            let value = match name {
                "UV_CACHE_DIR" => "/custom/uv",
                "npm_config_cache" => "/custom/npm-lower",
                "NPM_CONFIG_CACHE" => "/custom/npm-upper",
                _ => return None,
            };
            RawPath::from_unix_bytes(value.as_bytes().to_vec()).ok()
        });
        assert_eq!(
            read_names.len(),
            read_names.iter().collect::<BTreeSet<_>>().len()
        );
        assert!(
            !read_names
                .iter()
                .any(|name| name == "PATH" || name == "JEB_API_KEY")
        );
        assert_eq!(vars.len(), 3);
        let env = Environment {
            platform: Platform::Linux,
            home: None,
            local_app_data: None,
            roaming_app_data: None,
            vars,
        };
        let found = kb.matches(
            &env,
            &[
                scanned("/custom/uv", 1, 0),
                scanned("/custom/npm-lower/_cacache", 2, 0),
                scanned("/custom/npm-upper/_cacache", 3, 0),
                scanned("/custom/uv/child", 4, 0),
                scanned("/unrelated", 5, 0),
            ],
            Timestamp::UNIX_EPOCH,
        );
        let identities: BTreeSet<_> = found
            .iter()
            .filter_map(|e| match &e.subject {
                Subject::File { identity } => Some(identity.file.get()),
                _ => None,
            })
            .collect();
        assert_eq!(identities, BTreeSet::from([1, 2, 3]));
        Ok(())
    }

    #[test]
    fn unset_relative_and_other_platform_roots_do_not_match()
    -> Result<(), Box<dyn std::error::Error>> {
        let kb = KnowledgeBase::builtin()?;
        assert!(cache_path(Path::new("relative")).is_none());
        assert!(cache_path(Path::new("")).is_none());
        assert!(cache_path(&std::env::temp_dir()).is_some());
        let vars = cache_vars(&kb, Platform::Linux, |_| None);
        let env = Environment {
            platform: Platform::Linux,
            home: None,
            local_app_data: None,
            roaming_app_data: None,
            vars,
        };
        assert_eq!(
            kb.matches(&env, &[scanned("/relative", 1, 0)], Timestamp::UNIX_EPOCH),
            []
        );
        assert_eq!(
            cache_vars(&kb, Platform::Windows, |name| {
                assert_ne!(name, "HOMEBREW_CACHE");
                None
            }),
            []
        );
        Ok(())
    }

    /// An old default cache beside a configured one: both are uv's cache, but
    /// `uv cache prune` clears only the folder uv is configured to use, so
    /// only that one is shown with it (and only it makes the command runnable).
    #[test]
    fn a_tool_command_goes_only_with_the_folder_it_clears() -> Result<(), Box<dyn std::error::Error>>
    {
        let kb = KnowledgeBase::builtin()?;
        let items = [
            scanned("/home/ana/.cache/uv", 1, 0),
            scanned("/data/uv", 2, 0),
        ];
        // Which items are uv's cache, and which are shown with its command.
        let assess = |set: &[(&str, &str)]| {
            let env = Environment {
                platform: Platform::Linux,
                home: RawPath::from_unix_bytes("/home/ana").ok(),
                local_app_data: None,
                roaming_app_data: None,
                vars: cache_vars(&kb, Platform::Linux, |name| {
                    let (_, value) = set.iter().find(|(var, _)| *var == name)?;
                    RawPath::from_unix_bytes(*value).ok()
                }),
            };
            let evidence = kb.matches(&env, &items, Timestamp::UNIX_EPOCH);
            let mut identified = Vec::new();
            let mut commanded = Vec::new();
            for item in &items {
                let subject = Subject::File {
                    identity: item.identity().clone(),
                };
                let id = item.identity().file.get();
                if evidence
                    .iter()
                    .any(|e| e.subject == subject && e.fact == matched("uv-cache"))
                {
                    identified.push(id);
                }
                if tool_guidance(&kb, &subject, &evidence, |entry| {
                    kb.command_acts_on(&env, &entry.id, &item.path)
                })
                .is_some_and(|guidance| guidance.args == ["cache", "prune"])
                {
                    commanded.push(id);
                }
            }
            (identified, commanded)
        };
        assert_eq!(assess(&[]), (vec![1], vec![1]), "uv's default cache");
        assert_eq!(
            assess(&[("UV_CACHE_DIR", "/data/uv")]),
            (vec![1, 2], vec![2]),
            "the old default is still uv's cache, but not what the command clears"
        );
        assert_eq!(
            assess(&[("XDG_CACHE_HOME", "/data")]),
            (vec![1, 2], vec![2])
        );
        assert_eq!(
            assess(&[("XDG_CACHE_HOME", "/home/ana/.cache")]),
            (vec![1], vec![1]),
            "a variable naming the default folder keeps the command"
        );
        assert_eq!(
            assess(&[
                ("UV_CACHE_DIR", "/data/uv"),
                ("XDG_CACHE_HOME", "/elsewhere")
            ]),
            (vec![1, 2], vec![]),
            "two variables naming different folders: which one uv uses is unknown"
        );
        Ok(())
    }

    #[test]
    fn evidence_progress_counts_only_entries_in_the_selected_scan()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixture = lumen_testkit::Fixture::build(&[])?;
        let mut store = IndexStore::new(open_index(&fixture.root().join("index.db"))?);
        let scan: ScanId = "0190f0d2-6c1e-7a3b-8c4d-5e6f708192a3".parse()?;
        let other: ScanId = "0190f0d2-6c1e-7a3b-8c4d-5e6f708192a4".parse()?;
        store.begin_scan(scan, Timestamp::UNIX_EPOCH, &[])?;
        store.begin_scan(other, Timestamp::UNIX_EPOCH, &[])?;
        store.insert_entries(other, &[scanned("/other", 9, 0)])?;
        let inventory = PlatformInventory {
            reports: Vec::new(),
            #[cfg(target_os = "macos")]
            apps: Vec::new(),
            #[cfg(windows)]
            declarations: Vec::new(),
        };
        let kb = KnowledgeBase::builtin()?;
        let env = Environment {
            platform: Platform::Linux,
            home: None,
            local_app_data: None,
            roaming_app_data: None,
            vars: Vec::new(),
        };
        for count in [0_u64, 3] {
            let entries: Vec<_> = (0..count)
                .map(|n| scanned(&format!("/test/{n}"), u128::from(n), 0))
                .collect();
            store.insert_entries(scan, &entries)?;
            let mut events = Vec::new();
            find(
                &store,
                scan,
                &inventory,
                &kb.matcher(&env),
                None,
                (Timestamp::UNIX_EPOCH, Timestamp::UNIX_EPOCH),
                &mut |event| events.push(event),
            )?;
            let mut expected = Vec::new();
            for stage in [AssessStage::Markers, AssessStage::Evidence] {
                expected.push(AssessProgress {
                    stage,
                    completed: 0,
                    total: Some(count),
                });
                if count > 0 {
                    expected.push(AssessProgress {
                        stage,
                        completed: count,
                        total: Some(count),
                    });
                }
            }
            assert_eq!(events, expected);
        }
        Ok(())
    }

    /// A known cache inside a cloud sync root is cloud-backed even though it
    /// is local itself, and so is a folder holding a placeholder (FS-2); the
    /// same cache elsewhere is not.
    #[test]
    fn caches_in_a_sync_root_are_cloud_backed() -> Result<(), Box<dyn std::error::Error>> {
        let fixture = lumen_testkit::Fixture::build(&[])?;
        let mut store = IndexStore::new(open_index(&fixture.root().join("index.db"))?);
        let scan: ScanId = "0190f0d2-6c1e-7a3b-8c4d-5e6f708192a3".parse()?;
        store.begin_scan(scan, Timestamp::UNIX_EPOCH, &[])?;
        let mut root = scanned("/sync", 1, 0);
        root.size.flags.cloud_placeholder = true;
        let mut placeholder = scanned("/held/uv/archive", 7, 10);
        placeholder.size.flags.dataless = true;
        store.insert_entries(
            scan,
            &[
                root,
                scanned("/sync/npm", 2, 0),
                scanned("/sync/npm/_cacache", 3, 0),
                scanned("/local/npm", 4, 0),
                scanned("/local/npm/_cacache", 5, 0),
                scanned("/held/uv", 6, 0),
                placeholder,
            ],
        )?;
        let inventory = PlatformInventory {
            reports: Vec::new(),
            #[cfg(target_os = "macos")]
            apps: Vec::new(),
            #[cfg(windows)]
            declarations: Vec::new(),
        };
        let kb = KnowledgeBase::builtin()?;
        let env = Environment {
            platform: Platform::Linux,
            home: None,
            local_app_data: None,
            roaming_app_data: None,
            vars: cache_vars(&kb, Platform::Linux, |name| {
                let value = match name {
                    "npm_config_cache" => "/sync/npm",
                    "NPM_CONFIG_CACHE" => "/local/npm",
                    "UV_CACHE_DIR" => "/held/uv",
                    _ => return None,
                };
                RawPath::from_unix_bytes(value).ok()
            }),
        };
        let found = find(
            &store,
            scan,
            &inventory,
            &kb.matcher(&env),
            None,
            (Timestamp::UNIX_EPOCH, Timestamp::UNIX_EPOCH),
            &mut |_| {},
        )?;
        let gathered = gather(
            &found.entries,
            &found.domains,
            &summarize(&[]),
            found.evidence,
            found.relationships,
            Timestamp::UNIX_EPOCH,
            Timestamp::UNIX_EPOCH,
        );
        let cloud_backed: BTreeSet<(u128, bool)> = gathered
            .evidence
            .iter()
            .filter_map(|record| match (&record.subject, &record.fact) {
                (Subject::File { identity }, Fact::CloudBacked(backed)) => {
                    Some((identity.file.get(), *backed))
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            cloud_backed,
            BTreeSet::from([(3, true), (5, false), (6, true)]),
            "the npm caches below and outside the sync root, and uv's cache holding a placeholder"
        );
        Ok(())
    }

    /// What the producers found in `entries`, stored as a completed scan, with
    /// the knowledge base's cache variables set as `vars` says, gathered.
    fn gathered_from(
        entries: &[FilesystemEntry],
        vars: fn(&str) -> Option<&'static str>,
    ) -> Result<
        (lumen_application::gather::Gathered, Vec<FilesystemEntry>),
        Box<dyn std::error::Error>,
    > {
        let fixture = lumen_testkit::Fixture::build(&[])?;
        let mut store = IndexStore::new(open_index(&fixture.root().join("index.db"))?);
        let scan: ScanId = "0190f0d2-6c1e-7a3b-8c4d-5e6f708192a4".parse()?;
        store.begin_scan(scan, Timestamp::UNIX_EPOCH, &[])?;
        store.insert_entries(scan, entries)?;
        let inventory = PlatformInventory {
            reports: Vec::new(),
            #[cfg(target_os = "macos")]
            apps: Vec::new(),
            #[cfg(windows)]
            declarations: Vec::new(),
        };
        let kb = KnowledgeBase::builtin()?;
        let env = Environment {
            platform: Platform::Linux,
            home: None,
            local_app_data: None,
            roaming_app_data: None,
            vars: cache_vars(&kb, Platform::Linux, |name| {
                RawPath::from_unix_bytes(vars(name)?).ok()
            }),
        };
        let found = find(
            &store,
            scan,
            &inventory,
            &kb.matcher(&env),
            None,
            (Timestamp::UNIX_EPOCH, Timestamp::UNIX_EPOCH),
            &mut |_| {},
        )?;
        let gathered = gather(
            &found.entries,
            &found.domains,
            &summarize(&[]),
            found.evidence,
            found.relationships,
            Timestamp::UNIX_EPOCH,
            Timestamp::UNIX_EPOCH,
        );
        Ok((gathered, found.entries))
    }

    /// A candidate's path, verdict and fired rules.
    type Decided = (String, Verdict, Vec<String>);

    /// Every candidate in `gathered`, decided by the policy on Linux with
    /// `/home/t` as home, nothing in use and every directory fully listed.
    fn decided_on_linux(
        entries: &[FilesystemEntry],
        gathered: &lumen_application::gather::Gathered,
        found: &[FilesystemEntry],
    ) -> Result<Vec<Decided>, Box<dyn std::error::Error>> {
        let mut graph = lumen_graph::Graph::new();
        for relationship in &gathered.relationships {
            graph.add(relationship.clone())?;
        }
        let listed = lumen_application::ports::DirOutcome {
            access: AccessState::Readable,
            complete: true,
        };
        let listings: Vec<_> = entries
            .iter()
            .filter(|e| e.kind == EntryKind::Directory)
            .map(|e| (e.path.clone(), listed))
            .collect();
        let context = Context {
            platform: Platform::Linux,
            home: RawPath::from_unix_bytes(b"/home/t".to_vec()).ok(),
            windows_dir: None,
            local_app_data: None,
            mechanisms: BTreeSet::from([CleanupMechanism::QuarantineRename]),
            complete_sources: BTreeSet::new(),
            single_user: false,
        };
        let knowledge = KnowledgeBase::builtin()?.knowledge();
        let sources = Sources {
            graph: &graph,
            gathered,
            scanned: entries,
            listings: &listings,
            context: &context,
            knowledge: &knowledge,
            store_volumes: None,
        };
        select(gathered, found)
            .into_iter()
            .map(|candidate| {
                let free = Evidence {
                    subject: Subject::File {
                        identity: candidate.identity().clone(),
                    },
                    fact: Fact::InUse(false),
                    basis: Basis::Observed,
                    provenance: Provenance {
                        source: SourceName::new("test")?,
                        observed_at: Timestamp::UNIX_EPOCH,
                    },
                };
                let input = lumen_application::assess::assemble(candidate, &[free], &sources);
                let decision = lumen_policy::evaluate(&input)?;
                let rules = decision
                    .fired_rules()
                    .iter()
                    .map(|r| r.rule.as_str().to_owned())
                    .collect();
                Ok((candidate.path.display(), decision.verdict(), rules))
            })
            .collect()
    }

    /// A known cache with a browser profile somewhere inside it, and a cache
    /// folder that holds a protected location, are kept; the same cache
    /// without either is offered for review (FS-3).
    #[test]
    fn folders_holding_a_profile_or_a_protected_location_are_kept()
    -> Result<(), Box<dyn std::error::Error>> {
        let entries = [
            scanned("/home/t/.npm-a", 1, 0),
            scanned("/home/t/.npm-a/_cacache", 2, 0),
            scanned("/home/t/.npm-a/_cacache/blob", 3, 200_000_000),
            scanned("/home/t/.npm-b", 4, 0),
            scanned("/home/t/.npm-b/_cacache", 5, 0),
            scanned("/home/t/.npm-b/_cacache/blob", 6, 200_000_000),
            scanned("/home/t/.npm-b/_cacache/app", 7, 0),
            scanned("/home/t/.npm-b/_cacache/app/Default", 8, 0),
            scanned("/home/t/.npm-b/_cacache/app/Default/Network", 9, 0),
            scanned(
                "/home/t/.npm-b/_cacache/app/Default/Network/Cookies",
                10,
                4096,
            ),
            scanned("/home/t/.config", 11, 0),
            scanned("/home/t/.config/blob", 12, 200_000_000),
        ];
        let (gathered, found) = gathered_from(&entries, |name| match name {
            "npm_config_cache" => Some("/home/t/.npm-a"),
            "NPM_CONFIG_CACHE" => Some("/home/t/.npm-b"),
            "UV_CACHE_DIR" => Some("/home/t/.config"),
            _ => None,
        })?;
        let decided = decided_on_linux(&entries, &gathered, &found)?;
        let verdict = |path: &str| {
            decided
                .iter()
                .find(|(p, _, _)| p == path)
                .map(|(_, verdict, rules)| (*verdict, rules.clone()))
        };
        assert_eq!(
            verdict("/home/t/.npm-a/_cacache").map(|(v, _)| v),
            Some(Verdict::Review),
            "{decided:?}"
        );
        let profile = verdict("/home/t/.npm-b/_cacache");
        assert_eq!(profile.as_ref().map(|(v, _)| *v), Some(Verdict::Keep));
        assert!(
            profile
                .is_some_and(|(_, rules)| rules.contains(&"classify.browser-profile".to_owned())),
            "{decided:?}"
        );
        assert_eq!(
            verdict("/home/t/.config"),
            Some((Verdict::Keep, vec!["protect.contains-sensitive".to_owned()])),
            "holds Chrome's Linux profile location"
        );
        Ok(())
    }

    /// An app's support folder holding a cache that is decided on its own:
    /// the folder's own decision observes only the rest of it, so the
    /// cache's space is counted once.
    #[test]
    fn a_nested_candidate_is_carved_out_of_its_outer_one() {
        let entries = vec![
            scanned("/s", 1, 0),
            scanned("/s/Cache", 2, 0),
            scanned("/s/Cache/data", 3, 7000),
            scanned("/s/Cache2", 4, 0),
            scanned("/s/Cache2/x", 5, 1000),
            scanned("/s/prefs", 6, 500),
        ];
        let listed = lumen_application::ports::DirOutcome {
            access: AccessState::Readable,
            complete: true,
        };
        let listings: Vec<_> = ["/s", "/s/Cache", "/s/Cache2"]
            .iter()
            .map(|p| {
                (
                    RawPath::from_unix_bytes(p.as_bytes().to_vec())
                        .unwrap_or_else(|e| unreachable!("{e}")),
                    listed,
                )
            })
            .collect();
        let mut subtree = Subtree { entries, listings };
        let cache = subtree.entries[1].path.clone();
        subtree.carve(&[&cache]);
        let left: Vec<&[u8]> = subtree.entries.iter().map(|e| e.path.as_bytes()).collect();
        assert_eq!(
            left,
            [b"/s".as_slice(), b"/s/Cache2", b"/s/Cache2/x", b"/s/prefs"],
            "a sibling sharing the name's prefix stays"
        );
        assert_eq!(subtree.listings.len(), 2);
        let observed = lumen_application::observe::observed(
            &subtree.entries[0],
            &subtree.entries,
            &subtree.listings,
        );
        assert!(observed.complete);
        assert_eq!(observed.reclaim.reclaimable_now, ByteCount::new(1500));
        let unchanged = subtree.entries.len();
        subtree.carve(&[]);
        assert_eq!(
            subtree.entries.len(),
            unchanged,
            "nothing nested, nothing carved"
        );
    }

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
                single_user: false,
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

    fn matched(entry: &str) -> Fact {
        Fact::KnowledgeBaseMatch {
            entry: SourceName::new(entry).unwrap_or_else(|e| unreachable!("{e}")),
        }
    }

    #[test]
    fn a_matched_tool_command_becomes_guidance() -> Result<(), lumen_devkb::LoadError> {
        let kb = KnowledgeBase::builtin()?;
        let input = input(&[matched("xcode-simulator-devices")]);
        assert_eq!(
            tool_guidance(&kb, &input.subject, &input.evidence, |_| true),
            Some(ToolGuidance {
                entry: "xcode-simulator-devices".to_owned(),
                tool: "Xcode".to_owned(),
                program: "xcrun".to_owned(),
                args: vec![
                    "simctl".to_owned(),
                    "delete".to_owned(),
                    "unavailable".to_owned()
                ],
                removes_data: false,
            })
        );
        Ok(())
    }

    #[test]
    fn the_first_matched_entry_with_a_command_wins() -> Result<(), lumen_devkb::LoadError> {
        let kb = KnowledgeBase::builtin()?;
        // Unknown entries and entries without a command are passed over.
        let input = input(&[
            Fact::CloudBacked(false),
            matched("no-such-entry"),
            matched("cargo-global-cache"),
            matched("npm-cache"),
            matched("xcode-simulator-devices"),
        ]);
        let guidance = tool_guidance(&kb, &input.subject, &input.evidence, |_| true);
        assert_eq!(guidance.map(|g| g.program), Some("npm".to_owned()));
        Ok(())
    }

    #[test]
    fn only_the_candidates_own_matches_count() -> Result<(), lumen_devkb::LoadError> {
        let kb = KnowledgeBase::builtin()?;
        let mut input = input(&[matched("npm-cache")]);
        let other = Subject::File {
            identity: FileIdentity {
                volume: VolumeId::new("v").unwrap_or_else(|e| unreachable!("{e}")),
                file: FileId::new(2),
            },
        };
        for record in &mut input.evidence {
            record.subject = other.clone();
        }
        assert_eq!(
            tool_guidance(&kb, &input.subject, &input.evidence, |_| true),
            None
        );
        assert_eq!(tool_guidance(&kb, &input.subject, &[], |_| true), None);
        Ok(())
    }

    #[test]
    fn items_are_grouped_by_tool_or_application() -> Result<(), lumen_devkb::LoadError> {
        use lumen_domain::{AppId, AppIdScheme, UntrustedText};
        let kb = KnowledgeBase::builtin()?;
        let grouped = |facts: &[Fact]| {
            let input = input(facts);
            group_of(Some(&kb), &input.subject, &input.evidence)
        };
        assert_eq!(
            grouped(&[matched("xcode-derived-data")]),
            Group {
                kind: GroupKind::Developer,
                source: Some("Xcode".to_owned())
            }
        );
        let owner = Fact::OwningApplication(AppId {
            scheme: AppIdScheme::BundleId,
            value: UntrustedText::new("com.example.app"),
        });
        let location = |kind| Fact::AppDataLocation { kind };
        assert_eq!(
            grouped(&[owner.clone(), location(AppDataKind::Caches)]),
            Group {
                kind: GroupKind::AppCache,
                source: Some("com.example.app".to_owned())
            }
        );
        assert_eq!(
            grouped(&[owner, location(AppDataKind::Container)]).kind,
            GroupKind::AppData
        );
        assert_eq!(
            grouped(&[Fact::CloudBacked(false)]),
            Group {
                kind: GroupKind::Other,
                source: None
            }
        );
        Ok(())
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
