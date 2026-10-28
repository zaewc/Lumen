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
use lumen_application::collect::{Inventory, summarize};
use lumen_application::gather::gather;
use lumen_application::observe::below;
use lumen_application::ports::{Clock, InUseProbe, InventoryReport, OwnershipProbe};
use lumen_devkb::{Environment, KnowledgeBase, Markers};
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
    std::env::var_os(name).and_then(|value| crate::raw_path(Path::new(&value)).ok())
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
    entries: &[lumen_domain::FilesystemEntry],
    home: Option<&RawPath>,
    now: lumen_domain::Timestamp,
) -> Contributions {
    use lumen_platform_macos::support::link_support_locations;
    let mut contributions = Contributions::default();
    if let Some(home) = home {
        for link in link_support_locations(home, &inventory.apps, entries, now) {
            contributions.evidence.push(link.evidence);
            contributions.evidence.push(link.location);
            contributions.relationships.push(link.relationship);
        }
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
    entries: &[lumen_domain::FilesystemEntry],
    _home: Option<&RawPath>,
    now: lumen_domain::Timestamp,
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
    _entries: &[lumen_domain::FilesystemEntry],
    _home: Option<&RawPath>,
    _now: lumen_domain::Timestamp,
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

/// What the producers found in the whole scan, and the entries it is about.
struct Found {
    evidence: Vec<Evidence>,
    relationships: Vec<Relationship>,
    entries: Vec<lumen_domain::FilesystemEntry>,
}

/// Runs the platform's producers and the knowledge base over the scan, a page
/// at a time: first the knowledge base's markers (a marker can qualify an
/// entry in another page), then the producers. Keeps only the entries their
/// output names; no other entry's facts could reach a decision (roadmap 10.8).
fn find(
    store: &IndexStore,
    scan: ScanId,
    inventory: &PlatformInventory,
    matcher: &lumen_devkb::Matcher<'_>,
    home: Option<&RawPath>,
    times: (lumen_domain::Timestamp, lumen_domain::Timestamp),
) -> Result<Found, StoreError> {
    let (scanned_at, now) = times;
    let mut markers = Markers::new();
    for_each_page(store, scan, |page| matcher.add_markers(page, &mut markers))?;
    let mut found = Found {
        evidence: Vec::new(),
        relationships: Vec::new(),
        entries: Vec::new(),
    };
    for_each_page(store, scan, |page| {
        let produced = platform_contributions(inventory, page, home, now);
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
    let env = Environment {
        platform,
        home: home.clone(),
        local_app_data: env_path("LOCALAPPDATA"),
        roaming_app_data: env_path("APPDATA"),
        vars: Vec::new(),
    };
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
    )?;
    let gathered = gather(
        &found.entries,
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
}

/// The built-in knowledge base, loaded once; `None` if it does not load.
fn builtin_knowledge_base() -> Option<&'static KnowledgeBase> {
    static KNOWLEDGE_BASE: OnceLock<Option<KnowledgeBase>> = OnceLock::new();
    KNOWLEDGE_BASE
        .get_or_init(|| KnowledgeBase::builtin().ok())
        .as_ref()
}

/// The cleanup command of the tool whose knowledge-base entry `assessed`
/// matched, if that entry has one: the first such entry in evidence order.
/// `None` if no entry with a command matched, or the knowledge base does
/// not load.
pub fn tool_action(assessed: &Assessed) -> Option<ToolGuidance> {
    let subject = Subject::File {
        identity: assessed.candidate.identity().clone(),
    };
    tool_guidance(
        builtin_knowledge_base()?,
        &subject,
        &assessed.input.evidence,
    )
}

/// The first knowledge-base entry matched by `subject` itself in `evidence`
/// that has an official cleanup command, as guidance.
fn tool_guidance(
    kb: &KnowledgeBase,
    subject: &Subject,
    evidence: &[Evidence],
) -> Option<ToolGuidance> {
    evidence
        .iter()
        .filter(|record| &record.subject == subject)
        .filter_map(|record| match &record.fact {
            Fact::KnowledgeBaseMatch { entry } => kb.get(entry.as_str()),
            _ => None,
        })
        .find_map(|entry| {
            entry.official_action.as_ref().map(|action| ToolGuidance {
                entry: entry.id.clone(),
                tool: entry.tool.clone(),
                program: action.program.clone(),
                args: action.args.clone(),
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
    let timer = std::time::Instant::now();
    let assessment = prepare(db, clock)?;
    let candidates = select(&assessment.gathered, &assessment.entries);
    let counter = std::cell::Cell::new(0_u32);
    let now = clock.now();
    let request = |input: &lumen_policy::PolicyInput| {
        counter.set(counter.get().saturating_add(1));
        jev_request(input, counter.get(), assessment.inventories_complete, now)
    };
    let assessed = with_probes(|probes| -> Result<Vec<Assessed>, AssessError> {
        let mut assessed = Vec::new();
        for candidate in &candidates {
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
        }
        Ok(assessed)
    })??;
    let mut assessed = assessed;
    block_on(lumen_application::describe::describe(
        describer,
        clock,
        &mut assessed,
        &request,
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
        SizeFacts, SizeFlags, Subject, Timestamp, VolumeId,
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
            tool_guidance(&kb, &input.subject, &input.evidence),
            Some(ToolGuidance {
                entry: "xcode-simulator-devices".to_owned(),
                tool: "Xcode".to_owned(),
                program: "xcrun".to_owned(),
                args: vec![
                    "simctl".to_owned(),
                    "delete".to_owned(),
                    "unavailable".to_owned()
                ],
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
        let guidance = tool_guidance(&kb, &input.subject, &input.evidence);
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
        assert_eq!(tool_guidance(&kb, &input.subject, &input.evidence), None);
        assert_eq!(tool_guidance(&kb, &input.subject, &[]), None);
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
