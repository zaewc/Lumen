//! The computer as the helper sees it, as root (ADR-0063 rules 4 to 6).
//!
//! [`Mac`] implements [`HelperHost`] with the reviewed pieces Lumen already
//! uses, so the helper adds no destructive code of its own:
//!
//! - evidence: `lstat` of the bundle (identity, owner, flags), its helper
//!   folders from metadata only, the process table for anything running from
//!   it, and the background items of the system and of the caller, whose home
//!   is looked up by user ID ([`accounts`]), never from the helper's `HOME`;
//! - the move: `lumen-fs-exec`'s executor, by handle and without replacing,
//!   into `/Library/Application Support/Lumen/Quarantine/<uid>/`. The store's
//!   directories are created mode 0700 and must belong to the user the helper
//!   runs as, root, and be writable by no one else, or the executor refuses
//!   them (ADR-0015); the store is kept out of Time Machine like Lumen's own;
//! - what it holds: its record next to each item, read from the caller's
//!   store, and the same executor to locate, restore (by handle, without
//!   replacing) and finalize (after checking the whole held tree) the item.
//!
//! The paths are fields so that tests run the same code on fixtures, without
//! privileges (ADR-0063 rule 9).

use std::ffi::OsStr;
use std::os::macos::fs::MetadataExt as _;
use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::fs::MetadataExt as _;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use lumen_application::CancelToken;
use lumen_application::helper::HelperHost;
use lumen_application::helper::gate::{APPLICATIONS, HelperEvidence};
use lumen_application::ports::{
    ExecutionFailure, InUse, InUseProbe as _, InventoryReport, InventorySource as _, Located,
    PreparedItem, QuarantineExecutor as _, Removal, RestoreFailure, RestoredItem,
};
use lumen_application::uninstall::BundleEvidence;
use lumen_application::uninstall::evidence::{Lookup, helper_folders, linked_services};
use lumen_domain::{
    AppId, AppIdScheme, Application, FileId, FileIdentity, ItemLocation, OperationId,
    QuarantineFailure, RawPath, Timestamp, UntrustedText, VolumeId,
};
use lumen_fs_exec::{Executor, StoreError, open_store, read_sidecar, recorded_identity};
use lumen_platform_macos::{
    LaunchdInventory, ProcessProbe, accounts, bundle_identifier, management,
};

/// Where the helper's stores live.
const STORE_BASE: &str = "/Library/Application Support";

/// The flags that keep an item where it is, from `<sys/stat.h>`:
/// `UF_IMMUTABLE`, `UF_APPEND`, `SF_IMMUTABLE`, `SF_APPEND`, `SF_RESTRICTED`
/// (System Integrity Protection), `SF_NOUNLINK`, and `SF_DATALESS` (a cloud
/// placeholder, ADR-0063 rule 1).
const PROTECTING: u32 =
    0x0000_0002 | 0x0000_0004 | 0x0002_0000 | 0x0004_0000 | 0x0008_0000 | 0x0010_0000 | 0x4000_0000;

/// This Mac, or a fixture standing in for it.
pub(crate) struct Mac {
    /// The folder standing for `/Applications`.
    pub(crate) applications: PathBuf,
    /// The folder the stores are created in.
    pub(crate) store_base: PathBuf,
    /// An account's home folder.
    pub(crate) home: fn(u32) -> Option<PathBuf>,
    /// The background items of the system and of the account with this home.
    pub(crate) services: fn(&Path) -> InventoryReport,
    /// Whether a process runs from inside a bundle.
    pub(crate) running: fn(&RawPath) -> Option<bool>,
    /// Whether any process uses an item.
    pub(crate) in_use: fn(&RawPath, &FileIdentity) -> InUse,
    /// Whether a bundle's signature carries the Endpoint Security
    /// entitlement (security software).
    pub(crate) security_software: fn(&Path) -> Option<bool>,
    /// Whether an organization may manage the app with this bundle
    /// identifier (MDM enrollment or managed preferences).
    pub(crate) managed: fn(&str) -> Option<bool>,
}

impl std::fmt::Debug for Mac {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mac")
            .field("applications", &self.applications)
            .field("store_base", &self.store_base)
            .finish_non_exhaustive()
    }
}

/// Nothing moved: the item is where it was.
const AT_SOURCE: ExecutionFailure = ExecutionFailure {
    reason: QuarantineFailure::IdentityMismatch,
    left: ItemLocation::Source,
};

/// The store's names below the base for the account `caller`.
fn names(caller: u32) -> Vec<Vec<u8>> {
    vec![
        b"Lumen".to_vec(),
        b"Quarantine".to_vec(),
        caller.to_string().into_bytes(),
    ]
}

fn raw(path: &Path) -> Option<RawPath> {
    RawPath::from_unix_bytes(path.as_os_str().as_bytes().to_vec()).ok()
}

/// The identity, in the enumerators' form (`MetadataExt::dev` sign-extends).
fn identity(meta: &std::fs::Metadata) -> Option<FileIdentity> {
    Some(FileIdentity {
        volume: VolumeId::new(format!("dev:{:x}", meta.dev())).ok()?,
        file: FileId::new(u128::from(meta.ino())),
    })
}

fn lookup(path: &Path) -> Lookup {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => Lookup::Directory,
        Ok(_) => Lookup::Other,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Lookup::Missing,
        Err(_) => Lookup::Unknown,
    }
}

impl Mac {
    /// This Mac.
    pub(crate) fn system() -> Self {
        Self {
            applications: PathBuf::from(OsStr::from_bytes(APPLICATIONS)),
            store_base: PathBuf::from(STORE_BASE),
            home: accounts::home,
            services: |home| {
                LaunchdInventory::for_home(Some(home))
                    .without_signatures()
                    .collect(&CancelToken::new())
            },
            running: |bundle| ProcessProbe.snapshot().running(bundle),
            in_use: |path, identity| ProcessProbe.probe(path, identity),
            // `codesign` runs as `nobody`, so root never parses a binary
            // another account could have written.
            security_software: |bundle| {
                management::endpoint_security_client(bundle, management::Run::AsNobody)
            },
            managed: management::managed,
        }
    }

    /// The executor for `caller`'s store.
    fn executor(&self, caller: u32) -> Executor {
        Executor::new(&self.store_base, names(caller))
    }

    /// `path`, one name directly in `/Applications`, on this host.
    fn native(&self, path: &RawPath) -> Option<PathBuf> {
        let leaf = path
            .as_bytes()
            .strip_prefix(APPLICATIONS)?
            .strip_prefix(b"/")?;
        let plain = !leaf.is_empty() && !leaf.contains(&b'/') && leaf != b"." && leaf != b"..";
        plain.then(|| self.applications.join(OsStr::from_bytes(leaf)))
    }

    /// How many background items may run the bundle at `bundle`, known to
    /// Lumen as `path`, from the system's and the caller's own: unknown if the
    /// caller has no home, or the bundle's identifier cannot be read by handle
    /// ([`bundle_identifier`]).
    fn linked(&self, caller: u32, path: &RawPath, bundle: &Path) -> Option<usize> {
        let id = bundle_identifier(bundle)?;
        let app = Application {
            ids: [AppId {
                scheme: AppIdScheme::BundleId,
                value: UntrustedText::new(id),
            }]
            .into(),
            name: None,
            version: None,
            vendor: None,
            signature: None,
            install_locations: vec![path.clone()],
            sources: std::collections::BTreeSet::new(),
            uninstall: None,
        };
        let services = (self.services)(&(self.home)(caller)?);
        linked_services(std::slice::from_ref(&app), &app, &services, |path| {
            std::fs::canonicalize(OsStr::from_bytes(path.as_bytes()))
                .ok()
                .and_then(|p| raw(&p))
        })
    }
}

impl HelperHost for Mac {
    fn observe(&self, caller: u32, path: &RawPath) -> HelperEvidence {
        let native = self.native(path);
        let meta = native
            .as_deref()
            .and_then(|p| std::fs::symlink_metadata(p).ok());
        let bundle = native
            .as_deref()
            .filter(|_| meta.as_ref().is_some_and(std::fs::Metadata::is_dir));
        let store = std::fs::metadata(&self.store_base).ok().map(|m| m.dev());
        HelperEvidence {
            bundle: BundleEvidence {
                path: path.clone(),
                folders: Vec::new(),
                directory: meta.as_ref().map(std::fs::Metadata::is_dir),
                owned_by_user: None,
                linked_services: bundle.and_then(|b| self.linked(caller, path, b)),
                helpers: bundle.map_or_else(Default::default, |b| {
                    helper_folders(|parts| {
                        lookup(&parts.iter().fold(b.to_path_buf(), |p, n| p.join(n)))
                    })
                }),
                running: (self.running)(path),
                store_on_volume: store
                    .zip(meta.as_ref())
                    .map(|(store, meta)| store == meta.dev()),
                security_software: bundle.and_then(self.security_software),
                // The identifier read by handle, as for the background items.
                managed: bundle
                    .and_then(bundle_identifier)
                    .and_then(|id| (self.managed)(&id)),
            },
            identity: meta.as_ref().and_then(identity),
            owner: meta.as_ref().map(std::os::unix::fs::MetadataExt::uid),
            restricted: meta.as_ref().map(|m| m.st_flags() & PROTECTING != 0),
        }
    }

    fn prepare(
        &self,
        root: &RawPath,
        relative: &[Vec<u8>],
    ) -> Result<(PreparedItem, FileIdentity), ExecutionFailure> {
        let ([_], true) = (relative, root.as_bytes() == APPLICATIONS) else {
            return Err(AT_SOURCE);
        };
        let root = raw(&self.applications).ok_or(AT_SOURCE)?;
        // Recording needs no store; the move names the caller's.
        let item = Executor::new(&self.store_base, Vec::new()).prepare(&root, relative)?;
        // The identity the executor will check before it moves, from its own
        // record, not from another look at the path.
        let recorded = recorded_identity(&item).ok_or(AT_SOURCE)?;
        Ok((item, recorded))
    }

    fn in_use(&self, path: &RawPath, identity: &FileIdentity) -> InUse {
        (self.in_use)(path, identity)
    }

    fn quarantine(
        &self,
        caller: u32,
        item: &PreparedItem,
        operation: OperationId,
        record: &[u8],
    ) -> Result<(), ExecutionFailure> {
        let names = names(caller);
        let refs: Vec<&[u8]> = names.iter().map(Vec::as_slice).collect();
        let unavailable = |reason| ExecutionFailure {
            reason,
            left: ItemLocation::Source,
        };
        let volume = std::fs::metadata(&self.store_base)
            .map_err(|_| unavailable(QuarantineFailure::StoreUnavailable))?
            .dev();
        match open_store(&self.store_base, &refs, volume) {
            Ok(store) => {
                if let Err(error) = lumen_platform_macos::exclude_from_backup(
                    std::os::fd::AsFd::as_fd(store.handle()),
                ) {
                    eprintln!(
                        "lumen-helper: the store could not be excluded from Time Machine: {error}"
                    );
                }
            }
            Err(error) => {
                eprintln!("lumen-helper: the store cannot be used: {error}");
                // As the executor's own store check reports it (ADR-0066).
                return Err(unavailable(match error {
                    StoreError::OtherVolume => QuarantineFailure::VolumeUnsupported,
                    _ => QuarantineFailure::StoreUnavailable,
                }));
            }
        }
        self.executor(caller).quarantine(item, operation, record)
    }

    fn now(&self) -> Timestamp {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .and_then(|since| {
                let secs = i64::try_from(since.as_secs()).ok()?;
                Timestamp::from_unix(secs, since.subsec_nanos()).ok()
            })
            .unwrap_or(Timestamp::UNIX_EPOCH)
    }

    fn record(&self, caller: u32, operation: OperationId) -> Option<Vec<u8>> {
        let names = names(caller);
        let refs: Vec<&[u8]> = names.iter().map(Vec::as_slice).collect();
        let volume = std::fs::metadata(&self.store_base).ok()?.dev();
        let store = open_store(&self.store_base, &refs, volume).ok()?;
        read_sidecar(&store, operation.to_string().as_bytes()).ok()
    }

    fn locate(&self, caller: u32, item: &PreparedItem, operation: OperationId) -> Located {
        self.executor(caller).locate(item, operation)
    }

    fn restore(
        &self,
        caller: u32,
        item: &PreparedItem,
        operation: OperationId,
    ) -> Result<RestoredItem, RestoreFailure> {
        self.executor(caller).restore(item, operation)
    }

    fn held(
        &self,
        caller: u32,
        item: &PreparedItem,
        operation: OperationId,
    ) -> Option<(RawPath, FileIdentity)> {
        self.executor(caller).held(item, operation)
    }

    fn finalize(
        &self,
        caller: u32,
        item: &PreparedItem,
        operation: OperationId,
    ) -> Result<(), Removal> {
        self.executor(caller).finalize(item, operation)
    }

    fn forget(&self, caller: u32, item: &PreparedItem, operation: OperationId) -> bool {
        self.executor(caller).forget(item, operation)
    }
}

#[cfg(test)]
mod tests {
    //! The helper's whole `quarantine` path on fixtures, unprivileged: the
    //! store then belongs to the test's user, as it belongs to root on a Mac.

    use std::os::unix::fs::PermissionsExt as _;

    use lumen_application::helper::gate::Reason;
    use lumen_application::helper::{
        FinalizeRefusal, HeldRecord, HeldRequest, HelperRequest, HelperResponse, PROTOCOL,
        QuarantineRequest, RestoreRefusal, Whereabouts, answer, request, response,
    };
    use lumen_application::uninstall::Ineligible;
    use lumen_domain::{EvidenceHash, SourceName, SourceStatus};
    use lumen_testkit::{Fixture, Node};

    use super::*;

    const OPERATION: &str = "0190f1c2-7a3b-7c4d-8e5f-6a7b8c9d0e1f";

    /// The test's user, who owns the fixtures (the temporary folder is its
    /// own).
    fn me() -> u32 {
        std::fs::metadata(std::env::temp_dir()).map_or(0, |m| m.uid())
    }

    /// An account that is not the fixture's owner.
    fn caller() -> u32 {
        me().wrapping_add(1)
    }

    fn file(path: &str) -> Node {
        Node::File {
            path: path.into(),
            contents: b"x".to_vec(),
        }
    }

    /// A fixture with `Applications/` and a private `Support/` store base.
    fn computer(nodes: &[Node]) -> (Fixture, Mac) {
        let mut all = vec![Node::Dir {
            path: "Support".into(),
        }];
        all.extend_from_slice(nodes);
        all.push(Node::Dir {
            path: "Applications".into(),
        });
        let fx = Fixture::build(&all).unwrap_or_else(|e| unreachable!("{e}"));
        let root = std::fs::canonicalize(fx.root()).unwrap_or_else(|e| unreachable!("{e}"));
        for dir in [root.clone(), root.join("Support")] {
            #[allow(
                clippy::disallowed_methods,
                reason = "test fixture inside its own temporary directory"
            )]
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755))
                .unwrap_or_else(|e| unreachable!("{e}"));
        }
        let mac = Mac {
            applications: root.join("Applications"),
            store_base: root.join("Support"),
            home: |_| Some(PathBuf::from("/var/empty")),
            services: |_| InventoryReport {
                source: SourceName::new("test").unwrap_or_else(|_| unreachable!()),
                status: SourceStatus::Complete,
                applications: Vec::new(),
                services: Vec::new(),
                packages: Vec::new(),
            },
            running: |_| Some(false),
            in_use: |_, _| InUse::No,
            security_software: |_| Some(false),
            managed: |_| Some(false),
        };
        (fx, mac)
    }

    fn app(name: &str) -> Vec<Node> {
        vec![
            file(&format!("Applications/{name}/Contents/MacOS/word")),
            Node::File {
                path: format!("Applications/{name}/Contents/Info.plist"),
                contents:
                    b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<plist version=\"1.0\"><dict>\
                    <key>CFBundleIdentifier</key><string>com.example.word</string>\
                    </dict></plist>"
                        .to_vec(),
            },
        ]
    }

    fn path(name: &str) -> RawPath {
        RawPath::from_unix_bytes(format!("/Applications/{name}").into_bytes())
            .unwrap_or_else(|_| unreachable!())
    }

    fn identity_of(mac: &Mac, name: &str) -> FileIdentity {
        std::fs::symlink_metadata(mac.applications.join(name))
            .ok()
            .as_ref()
            .and_then(identity)
            .unwrap_or_else(|| unreachable!("{name} exists"))
    }

    fn ask(mac: &Mac, caller: u32, name: &str, identity: FileIdentity) -> Option<HelperResponse> {
        let body = request(&HelperRequest::Quarantine(QuarantineRequest {
            protocol: PROTOCOL,
            operation: OPERATION.parse().unwrap_or_else(|_| unreachable!()),
            path: path(name),
            identity,
            evidence_hash: EvidenceHash::from_bytes([9; 32]),
        }));
        response(&answer(&body, caller, "0.11.0", mac))
    }

    fn store(mac: &Mac, caller: u32) -> PathBuf {
        mac.store_base
            .join("Lumen/Quarantine")
            .join(caller.to_string())
    }

    #[test]
    fn the_system_host_uses_the_adrs_locations() {
        let mac = Mac::system();
        assert_eq!(mac.applications, Path::new("/Applications"));
        assert_eq!(mac.store_base, Path::new("/Library/Application Support"));
        assert_eq!(
            names(501),
            [b"Lumen".to_vec(), b"Quarantine".to_vec(), b"501".to_vec()]
        );
        assert_eq!(
            mac.native(&path("Word.app")),
            Some(PathBuf::from("/Applications/Word.app"))
        );
        for outside in [
            "/Applications/a/b.app",
            "/Applications/..",
            "/Users/a/Word.app",
        ] {
            let outside = RawPath::from_unix_bytes(outside.as_bytes().to_vec())
                .unwrap_or_else(|_| unreachable!());
            assert_eq!(mac.native(&outside), None);
        }
    }

    #[test]
    fn an_app_another_account_owns_moves_into_that_callers_private_store() {
        let (_fx, mac) = computer(&app("Word.app"));
        let id = identity_of(&mac, "Word.app");
        assert_eq!(
            ask(&mac, caller(), "Word.app", id.clone()),
            Some(HelperResponse::Quarantined {
                operation: OPERATION.parse().unwrap_or_else(|_| unreachable!())
            })
        );
        assert!(!mac.applications.join("Word.app").exists());
        let store = store(&mac, caller());
        let held = std::fs::symlink_metadata(store.join(OPERATION)).ok();
        assert_eq!(held.as_ref().and_then(identity), Some(id.clone()));
        for dir in ["Lumen", "Lumen/Quarantine"] {
            let mode = std::fs::metadata(mac.store_base.join(dir)).map(|m| m.mode() & 0o777);
            assert_eq!(mode.ok(), Some(0o700), "{dir}");
        }
        assert_eq!(
            std::fs::metadata(&store).map(|m| m.mode() & 0o777).ok(),
            Some(0o700)
        );
        let record = std::fs::read(store.join(format!("{OPERATION}.manifest.json")))
            .unwrap_or_else(|e| unreachable!("{e}"));
        let record: HeldRecord =
            serde_json::from_slice(&record).unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(
            (record.caller, record.path, record.identity),
            (caller(), path("Word.app"), id)
        );
        assert!(record.prepared.get("leaf").is_some());
    }

    #[test]
    fn the_callers_own_app_stays_for_lumen_to_move_itself() {
        let (_fx, mac) = computer(&app("Word.app"));
        let id = identity_of(&mac, "Word.app");
        assert_eq!(
            ask(&mac, me(), "Word.app", id),
            Some(HelperResponse::Ineligible {
                reasons: vec![Reason::OwnedByCaller]
            })
        );
        assert!(mac.applications.join("Word.app").is_dir());
    }

    #[test]
    fn a_swapped_linked_or_helper_carrying_bundle_is_not_moved() {
        let mut nodes = app("Word.app");
        nodes.extend(app("Helped.app"));
        nodes.push(Node::Dir {
            path: "Applications/Helped.app/Contents/Library/LaunchDaemons".into(),
        });
        nodes.push(Node::Symlink {
            path: "Applications/Link.app".into(),
            target: "Word.app".into(),
        });
        let (_fx, mac) = computer(&nodes);
        let word = identity_of(&mac, "Word.app");
        let other = identity_of(&mac, "Helped.app");
        let cases = [
            ("Word.app", other.clone(), Reason::Changed),
            ("Helped.app", other, Reason::Gate(Ineligible::HasHelpers)),
            (
                "Link.app",
                identity_of(&mac, "Link.app"),
                Reason::Gate(Ineligible::NotABundle),
            ),
        ];
        for (name, id, reason) in cases {
            let answer = ask(&mac, caller(), name, id);
            let Some(HelperResponse::Ineligible { reasons }) = answer else {
                unreachable!("{name}: {answer:?}")
            };
            assert!(reasons.contains(&reason), "{name}: {reasons:?}");
        }
        assert_eq!(identity_of(&mac, "Word.app"), word);
        assert!(!store(&mac, caller()).exists());
    }

    #[test]
    fn an_immutable_app_is_restricted() {
        let (_fx, mac) = computer(&app("Word.app"));
        let bundle = mac.applications.join("Word.app");
        let flag = |value: &str| {
            std::process::Command::new("/usr/bin/chflags")
                .arg(value)
                .arg(&bundle)
                .status()
                .is_ok_and(|status| status.success())
        };
        assert!(flag("uchg"));
        let answer = ask(&mac, caller(), "Word.app", identity_of(&mac, "Word.app"));
        assert!(flag("nouchg"), "the fixture must be removable again");
        assert_eq!(
            answer,
            Some(HelperResponse::Ineligible {
                reasons: vec![Reason::Restricted]
            })
        );
    }

    #[test]
    fn security_software_and_managed_apps_stay_and_unknown_refuses() {
        type Case = (fn(&mut Mac), Ineligible);
        let cases: [Case; 4] = [
            (
                |m| m.security_software = |_| Some(true),
                Ineligible::SecuritySoftware,
            ),
            (
                |m| m.security_software = |_| None,
                Ineligible::SecurityUnknown,
            ),
            (|m| m.managed = |_| Some(true), Ineligible::Managed),
            (|m| m.managed = |_| None, Ineligible::ManagedUnknown),
        ];
        for (change, reason) in cases {
            let (_fx, mut mac) = computer(&app("Word.app"));
            change(&mut mac);
            assert_eq!(
                ask(&mac, caller(), "Word.app", identity_of(&mac, "Word.app")),
                Some(HelperResponse::Ineligible {
                    reasons: vec![Reason::Gate(reason)]
                }),
                "{reason:?}"
            );
            assert!(mac.applications.join("Word.app").is_dir());
        }
    }

    #[test]
    fn a_store_someone_else_could_reach_is_refused_and_nothing_moves() {
        let (_fx, mac) = computer(&app("Word.app"));
        std::fs::create_dir(mac.store_base.join("Lumen")).unwrap_or_else(|e| unreachable!("{e}"));
        #[allow(
            clippy::disallowed_methods,
            reason = "test fixture inside its own temporary directory"
        )]
        std::fs::set_permissions(
            mac.store_base.join("Lumen"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(
            ask(&mac, caller(), "Word.app", identity_of(&mac, "Word.app")),
            Some(HelperResponse::Failed {
                reason: QuarantineFailure::StoreUnavailable,
                left: ItemLocation::Source
            })
        );
        assert!(mac.applications.join("Word.app").is_dir());
    }

    #[test]
    fn background_items_are_matched_by_the_identifier_read_by_handle() {
        let mut nodes = app("Word.app");
        nodes.push(file("Applications/Linked.app/Contents/MacOS/x"));
        nodes.push(Node::Symlink {
            path: "Applications/Linked.app/Contents/Info.plist".into(),
            target: "../../Word.app/Contents/Info.plist".into(),
        });
        let (_fx, mut mac) = computer(&nodes);
        // The Info.plist that is a link is not read: without the identifier,
        // the background items and management checks are unknown.
        let linked = ask(
            &mac,
            caller(),
            "Linked.app",
            identity_of(&mac, "Linked.app"),
        );
        assert_eq!(
            linked,
            Some(HelperResponse::Ineligible {
                reasons: vec![
                    Reason::Gate(Ineligible::ServicesUnknown),
                    Reason::Gate(Ineligible::ManagedUnknown)
                ]
            })
        );
        // An item labelled with the bundle's identifier keeps it in place.
        mac.services = |_| InventoryReport {
            source: SourceName::new("test").unwrap_or_else(|_| unreachable!()),
            status: SourceStatus::Complete,
            applications: Vec::new(),
            services: vec![lumen_domain::Service {
                kind: lumen_domain::ServiceKind::LaunchAgent,
                scope: lumen_domain::ServiceScope::User,
                label: UntrustedText::new("com.example.word.helper".to_owned()),
                definition: None,
                program: None,
                enabled: None,
                signature: None,
            }],
            packages: Vec::new(),
        };
        let word = ask(&mac, caller(), "Word.app", identity_of(&mac, "Word.app"));
        assert_eq!(
            word,
            Some(HelperResponse::Ineligible {
                reasons: vec![Reason::Gate(Ineligible::ServiceLinked)]
            })
        );
        assert!(mac.applications.join("Word.app").is_dir());
    }

    fn held(
        mac: &Mac,
        caller: u32,
        verb: fn(HeldRequest) -> HelperRequest,
    ) -> Option<HelperResponse> {
        let body = request(&verb(HeldRequest {
            protocol: PROTOCOL,
            operation: OPERATION.parse().unwrap_or_else(|_| unreachable!()),
        }));
        response(&answer(&body, caller, "0.11.0", mac))
    }

    #[test]
    fn a_held_app_is_located_and_restored_from_the_helpers_record() {
        let (_fx, mac) = computer(&app("Word.app"));
        let id = identity_of(&mac, "Word.app");
        assert!(matches!(
            ask(&mac, caller(), "Word.app", id.clone()),
            Some(HelperResponse::Quarantined { .. })
        ));
        let located = |whereabouts| Some(HelperResponse::Located { whereabouts });
        assert_eq!(
            held(&mac, caller(), HelperRequest::Locate),
            located(Whereabouts::InStore)
        );
        // Another account holds nothing under that operation.
        for verb in [
            HelperRequest::Locate,
            HelperRequest::Restore,
            HelperRequest::Finalize,
        ] {
            assert_eq!(
                held(&mac, caller() + 1, verb),
                Some(HelperResponse::NotHeld)
            );
        }
        assert_eq!(
            held(&mac, caller(), HelperRequest::Restore),
            Some(HelperResponse::Restored {
                parent_replaced: false,
                matches_record: true
            })
        );
        assert_eq!(identity_of(&mac, "Word.app"), id);
        assert_eq!(
            held(&mac, caller(), HelperRequest::Locate),
            located(Whereabouts::AtSource)
        );
        // Another account cannot drop this caller's record.
        assert_eq!(
            held(&mac, caller() + 1, HelperRequest::Forget),
            Some(HelperResponse::NotHeld)
        );
        assert_eq!(
            held(&mac, caller(), HelperRequest::Finalize),
            Some(HelperResponse::NotFinalized {
                reason: FinalizeRefusal::AtSource
            })
        );
        assert!(mac.applications.join("Word.app").is_dir());
        // Once back, its record can be dropped; then nothing is held.
        assert_eq!(
            held(&mac, caller(), HelperRequest::Forget),
            Some(HelperResponse::Forgotten)
        );
        assert_eq!(
            held(&mac, caller(), HelperRequest::Locate),
            Some(HelperResponse::NotHeld)
        );
        assert!(mac.applications.join("Word.app").is_dir());
    }

    #[test]
    fn a_held_item_keeps_its_record() {
        let (_fx, mac) = computer(&app("Word.app"));
        let id = identity_of(&mac, "Word.app");
        assert!(matches!(
            ask(&mac, caller(), "Word.app", id),
            Some(HelperResponse::Quarantined { .. })
        ));
        assert_eq!(
            held(&mac, caller(), HelperRequest::Forget),
            Some(HelperResponse::NotForgotten)
        );
        assert_eq!(
            held(&mac, caller(), HelperRequest::Locate),
            Some(HelperResponse::Located {
                whereabouts: Whereabouts::InStore
            })
        );
    }

    #[test]
    fn a_restore_never_replaces_what_now_has_the_name() {
        let (_fx, mac) = computer(&app("Word.app"));
        let id = identity_of(&mac, "Word.app");
        assert!(matches!(
            ask(&mac, caller(), "Word.app", id),
            Some(HelperResponse::Quarantined { .. })
        ));
        std::fs::create_dir(mac.applications.join("Word.app"))
            .unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(
            held(&mac, caller(), HelperRequest::Restore),
            Some(HelperResponse::NotRestored {
                reason: RestoreRefusal::NameTaken,
                left: ItemLocation::Quarantine
            })
        );
        assert!(store(&mac, caller()).join(OPERATION).is_dir());
    }

    #[test]
    fn a_held_app_nothing_uses_is_removed_for_good_with_its_record() {
        let (_fx, mut mac) = computer(&app("Word.app"));
        let id = identity_of(&mac, "Word.app");
        assert!(matches!(
            ask(&mac, caller(), "Word.app", id),
            Some(HelperResponse::Quarantined { .. })
        ));
        mac.in_use = |_, _| InUse::Partial { unseen: Some(1) };
        assert_eq!(
            held(&mac, caller(), HelperRequest::Finalize),
            Some(HelperResponse::NotFinalized {
                reason: FinalizeRefusal::InUse
            })
        );
        assert!(store(&mac, caller()).join(OPERATION).is_dir());
        mac.in_use = |_, _| InUse::No;
        assert_eq!(
            held(&mac, caller(), HelperRequest::Finalize),
            Some(HelperResponse::Finalized)
        );
        let left = std::fs::read_dir(store(&mac, caller()))
            .unwrap_or_else(|e| unreachable!("{e}"))
            .count();
        assert_eq!(left, 0, "the item and the helper's record are gone");
        assert_eq!(
            held(&mac, caller(), HelperRequest::Locate),
            Some(HelperResponse::NotHeld)
        );
    }

    #[test]
    fn a_caller_without_a_home_leaves_the_background_items_unknown() {
        let (_fx, mut mac) = computer(&app("Word.app"));
        mac.home = |_| None;
        let answer = ask(&mac, caller(), "Word.app", identity_of(&mac, "Word.app"));
        assert_eq!(
            answer,
            Some(HelperResponse::Ineligible {
                reasons: vec![Reason::Gate(Ineligible::ServicesUnknown)]
            })
        );
    }
}
