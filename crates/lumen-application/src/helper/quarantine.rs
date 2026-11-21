//! The helper's `quarantine` verb (ADR-0063 rules 2, 5 and 6, roadmap 16.3c).
//!
//! In order, each step refusing before anything moves:
//!
//! 1. The request must name the helper's own [`PROTOCOL`], and the caller
//!    (from the connection) must be an administrator.
//! 2. The helper gathers fresh evidence about the item at the requested path
//!    and decides again itself ([`decide`]).
//! 3. The executor records the item by handle from `/Applications`, and the
//!    item it recorded must have the confirmed identity.
//! 4. Every process is asked whether it uses the item; only `No` passes.
//! 5. The executor moves it, without replacing anything, into the caller's
//!    store (`/Library/Application Support/Lumen/Quarantine/<uid>/`, owned by
//!    root, mode 0700), verifies it there and writes the helper's
//!    [`HeldRecord`] next to it.
//!
//! Lumen journals the operation before it asks and records the outcome after
//! (ADR-0015); the record is the helper's own account of what it holds, so
//! restoring, finalizing and locating the item (16.3d) act on it, never on
//! paths from a request.

use lumen_domain::{
    EvidenceHash, FileIdentity, ItemLocation, OperationId, QuarantineFailure, RawPath, Timestamp,
};
use serde::{Deserialize, Serialize};

use super::gate::{APPLICATIONS, HelperEvidence, Reason, decide};
use super::{HelperRefusal, HelperResponse, PROTOCOL, QuarantineRequest};
use crate::ports::{
    ExecutionFailure, InUse, Located, NotRestored, PreparedItem, Removal, RestoreFailure,
    RestoredItem,
};

/// The version of [`HeldRecord`]'s form.
pub const HELD_SCHEMA: u32 = 1;

/// What the helper reads and does on the computer, with full rights. The
/// daemon implements it as root; tests implement it in memory.
pub trait HelperHost {
    /// Whether the account `uid` is an administrator; `None` if unknown.
    fn administrator(&self, uid: u32) -> Option<bool>;

    /// Fresh evidence about the item at `path`, for the account `caller`.
    fn observe(&self, caller: u32, path: &RawPath) -> HelperEvidence;

    /// The executor's record of `relative` below `root`, by handle, with the
    /// identity of the item it recorded.
    ///
    /// # Errors
    ///
    /// An [`ExecutionFailure`] (left at the source) if it cannot be recorded.
    fn prepare(
        &self,
        root: &RawPath,
        relative: &[Vec<u8>],
    ) -> Result<(PreparedItem, FileIdentity), ExecutionFailure>;

    /// Whether any process uses the item at `path` with `identity`.
    fn in_use(&self, path: &RawPath, identity: &FileIdentity) -> InUse;

    /// Moves the recorded item into `caller`'s store as `operation`, without
    /// replacing anything, verifies it there and writes `record` next to it.
    ///
    /// # Errors
    ///
    /// An [`ExecutionFailure`] saying why and where the item is.
    fn quarantine(
        &self,
        caller: u32,
        item: &PreparedItem,
        operation: OperationId,
        record: &[u8],
    ) -> Result<(), ExecutionFailure>;

    /// Now.
    fn now(&self) -> Timestamp;

    /// The helper's record of `operation` in `caller`'s store, as written
    /// there, or `None` if there is none or it cannot be read. A host that
    /// holds nothing keeps the default.
    fn record(&self, caller: u32, operation: OperationId) -> Option<Vec<u8>> {
        let _ = (caller, operation);
        None
    }

    /// Where the item `item` records, held as `operation` in `caller`'s
    /// store, is now.
    fn locate(&self, caller: u32, item: &PreparedItem, operation: OperationId) -> Located {
        let _ = (caller, item, operation);
        Located::Unknown
    }

    /// Moves the held item back to where it was, without replacing anything.
    ///
    /// # Errors
    ///
    /// A [`RestoreFailure`] saying why and where the item is.
    fn restore(
        &self,
        caller: u32,
        item: &PreparedItem,
        operation: OperationId,
    ) -> Result<RestoredItem, RestoreFailure> {
        let _ = (caller, item, operation);
        Err(RestoreFailure {
            reason: NotRestored::Failed,
            left: ItemLocation::Quarantine,
        })
    }

    /// Where the held item is in the store, and its identity, to ask every
    /// process about it; `None` unless it is there, unchanged.
    fn held(
        &self,
        caller: u32,
        item: &PreparedItem,
        operation: OperationId,
    ) -> Option<(RawPath, FileIdentity)> {
        let _ = (caller, item, operation);
        None
    }

    /// Removes the held item for good, after checking its whole tree, then
    /// its record.
    ///
    /// # Errors
    ///
    /// How much a failed removal removed.
    fn finalize(
        &self,
        caller: u32,
        item: &PreparedItem,
        operation: OperationId,
    ) -> Result<(), Removal> {
        let _ = (caller, item, operation);
        Err(Removal::Nothing)
    }
}

/// The helper's record of an item it holds, written next to it in the store,
/// which only root can read or change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeldRecord {
    /// [`HELD_SCHEMA`].
    pub schema: u32,
    /// The operation; the item's name in the store.
    pub operation: OperationId,
    /// The account it was moved for, from the connection. Only that account
    /// may restore or finalize it.
    pub caller: u32,
    /// Where it was.
    pub path: RawPath,
    /// What it was.
    pub identity: FileIdentity,
    /// The confirmed decision's evidence hash, as Lumen sent it.
    pub evidence_hash: EvidenceHash,
    /// The executor's record, which restoring and locating act on.
    pub prepared: serde_json::Value,
    /// When it was moved.
    pub moved_at: Timestamp,
    /// The helper's version.
    pub helper_version: String,
}

/// `path` as `/Applications` and its one name there, or `None`.
fn split(path: &RawPath) -> Option<(RawPath, Vec<Vec<u8>>)> {
    let leaf = path
        .as_bytes()
        .strip_prefix(APPLICATIONS)?
        .strip_prefix(b"/")?;
    if leaf.is_empty() || leaf.contains(&b'/') || leaf == b"." || leaf == b".." {
        return None;
    }
    let root = RawPath::from_unix_bytes(APPLICATIONS.to_vec()).ok()?;
    Some((root, vec![leaf.to_vec()]))
}

fn ineligible(reason: Reason) -> HelperResponse {
    HelperResponse::Ineligible {
        reasons: vec![reason],
    }
}

fn failed(failure: ExecutionFailure) -> HelperResponse {
    HelperResponse::Failed {
        reason: failure.reason,
        left: failure.left,
    }
}

/// Answers a `quarantine` request from `caller`, a non-root user ID from the
/// connection, by a helper of `version`.
pub(super) fn quarantine(
    caller: u32,
    request: &QuarantineRequest,
    version: &str,
    host: &dyn HelperHost,
) -> HelperResponse {
    let refused = |reason| HelperResponse::Refused { reason };
    if request.protocol != PROTOCOL {
        return refused(HelperRefusal::OtherProtocol);
    }
    if host.administrator(caller) != Some(true) {
        return refused(HelperRefusal::NotAdministrator);
    }
    let evidence = host.observe(caller, &request.path);
    if let Err(reasons) = decide(caller, &request.path, &request.identity, &evidence) {
        return HelperResponse::Ineligible { reasons };
    }
    let Some((root, relative)) = split(&request.path) else {
        // `decide` admits only a name directly in /Applications.
        return ineligible(Reason::Gate(
            crate::uninstall::Ineligible::NotInApplications,
        ));
    };
    let item = match host.prepare(&root, &relative) {
        Ok((item, identity)) if identity == request.identity => item,
        Ok(_) => return ineligible(Reason::Changed),
        Err(failure) => return failed(failure),
    };
    // With full rights every process is seen, so only `No` passes.
    if host.in_use(&request.path, &request.identity) != InUse::No {
        return ineligible(Reason::InUse);
    }
    // The executor's record is its own JSON; one it cannot have written
    // stops here, at the source.
    let unrecorded = || {
        failed(ExecutionFailure {
            reason: QuarantineFailure::MoveFailed,
            left: ItemLocation::Source,
        })
    };
    let Ok(prepared) = serde_json::from_slice(&item.0) else {
        return unrecorded();
    };
    let record = HeldRecord {
        schema: HELD_SCHEMA,
        operation: request.operation,
        caller,
        path: request.path.clone(),
        identity: request.identity.clone(),
        evidence_hash: request.evidence_hash,
        prepared,
        moved_at: host.now(),
        helper_version: version.to_owned(),
    };
    let Ok(record) = serde_json::to_vec(&record) else {
        return unrecorded();
    };
    match host.quarantine(caller, &item, request.operation, &record) {
        Ok(()) => HelperResponse::Quarantined {
            operation: request.operation,
        },
        Err(failure) => failed(failure),
    }
}

#[cfg(test)]
pub(crate) mod fake {
    //! A computer in memory for the helper's tests.

    use std::sync::Mutex;

    use lumen_domain::{FileId, VolumeId};

    use super::*;
    use crate::uninstall::{BundleEvidence, HelperFolder};

    pub(crate) const CALLER: u32 = 501;
    pub(crate) const APP: &str = "/Applications/Word.app";

    pub(crate) fn raw(path: &str) -> RawPath {
        RawPath::from_unix_bytes(path.as_bytes().to_vec()).unwrap_or_else(|_| unreachable!())
    }

    pub(crate) fn identity(file: u128) -> FileIdentity {
        FileIdentity {
            volume: VolumeId::new("dev:1000012").unwrap_or_else(|_| unreachable!()),
            file: FileId::new(file),
        }
    }

    /// What the host is asked to do, in order.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(crate) enum Call {
        Administrator(u32),
        Observe(u32, RawPath),
        Prepare(RawPath, Vec<Vec<u8>>),
        InUse,
        Quarantine(u32, OperationId, Vec<u8>),
        Record(u32, OperationId),
        Locate(u32, Vec<u8>, OperationId),
        Restore(u32, Vec<u8>, OperationId),
        Held(u32, OperationId),
        Finalize(u32, Vec<u8>, OperationId),
    }

    pub(crate) struct Host {
        pub(crate) admin: Option<bool>,
        pub(crate) evidence: HelperEvidence,
        pub(crate) prepared: Result<(PreparedItem, FileIdentity), ExecutionFailure>,
        pub(crate) in_use: InUse,
        pub(crate) moved: Result<(), ExecutionFailure>,
        pub(crate) record: Option<Vec<u8>>,
        pub(crate) located: Located,
        pub(crate) restored: Result<RestoredItem, RestoreFailure>,
        pub(crate) held: Option<(RawPath, FileIdentity)>,
        pub(crate) finalized: Result<(), Removal>,
        pub(crate) calls: Mutex<Vec<Call>>,
    }

    impl Host {
        /// An administrator asking about a quiet root-owned app.
        pub(crate) fn eligible() -> Self {
            Self {
                admin: Some(true),
                evidence: HelperEvidence {
                    bundle: BundleEvidence {
                        path: raw(APP),
                        folders: Vec::new(),
                        directory: Some(true),
                        owned_by_user: None,
                        linked_services: Some(0),
                        helpers: HelperFolder::ALL.map(|f| (f, Some(false))).into(),
                        running: Some(false),
                        store_on_volume: Some(true),
                        security_software: Some(false),
                        managed: Some(false),
                    },
                    identity: Some(identity(7)),
                    owner: Some(0),
                    restricted: Some(false),
                },
                prepared: Ok((PreparedItem(br#"{"leaf":[87]}"#.to_vec()), identity(7))),
                in_use: InUse::No,
                moved: Ok(()),
                record: None,
                located: Located::InStore,
                restored: Ok(RestoredItem {
                    parent_replaced: false,
                    matches_record: true,
                }),
                held: Some((raw("/Library/held"), identity(7))),
                finalized: Ok(()),
                calls: Mutex::new(Vec::new()),
            }
        }

        fn call(&self, call: Call) {
            self.calls
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(call);
        }

        pub(crate) fn calls(&self) -> Vec<Call> {
            self.calls
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone()
        }
    }

    impl HelperHost for Host {
        fn administrator(&self, uid: u32) -> Option<bool> {
            self.call(Call::Administrator(uid));
            self.admin
        }

        fn observe(&self, caller: u32, path: &RawPath) -> HelperEvidence {
            self.call(Call::Observe(caller, path.clone()));
            self.evidence.clone()
        }

        fn prepare(
            &self,
            root: &RawPath,
            relative: &[Vec<u8>],
        ) -> Result<(PreparedItem, FileIdentity), ExecutionFailure> {
            self.call(Call::Prepare(root.clone(), relative.to_vec()));
            self.prepared.clone()
        }

        fn in_use(&self, _path: &RawPath, _identity: &FileIdentity) -> InUse {
            self.call(Call::InUse);
            self.in_use.clone()
        }

        fn quarantine(
            &self,
            caller: u32,
            _item: &PreparedItem,
            operation: OperationId,
            record: &[u8],
        ) -> Result<(), ExecutionFailure> {
            self.call(Call::Quarantine(caller, operation, record.to_vec()));
            self.moved
        }

        fn now(&self) -> Timestamp {
            Timestamp::from_unix(1_800_000_000, 0).unwrap_or_else(|_| unreachable!())
        }

        fn record(&self, caller: u32, operation: OperationId) -> Option<Vec<u8>> {
            self.call(Call::Record(caller, operation));
            self.record.clone()
        }

        fn locate(&self, caller: u32, item: &PreparedItem, operation: OperationId) -> Located {
            self.call(Call::Locate(caller, item.0.clone(), operation));
            self.located
        }

        fn restore(
            &self,
            caller: u32,
            item: &PreparedItem,
            operation: OperationId,
        ) -> Result<RestoredItem, RestoreFailure> {
            self.call(Call::Restore(caller, item.0.clone(), operation));
            self.restored
        }

        fn held(
            &self,
            caller: u32,
            _item: &PreparedItem,
            operation: OperationId,
        ) -> Option<(RawPath, FileIdentity)> {
            self.call(Call::Held(caller, operation));
            self.held.clone()
        }

        fn finalize(
            &self,
            caller: u32,
            item: &PreparedItem,
            operation: OperationId,
        ) -> Result<(), Removal> {
            self.call(Call::Finalize(caller, item.0.clone(), operation));
            self.finalized
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fake::{APP, CALLER, Call, Host, identity, raw};
    use super::*;
    use crate::helper::{HelperRequest, answer, request as encode, response};
    use crate::uninstall::Ineligible;

    const OPERATION: &str = "0190f1c2-7a3b-7c4d-8e5f-6a7b8c9d0e1f";

    fn operation() -> OperationId {
        OPERATION.parse().unwrap_or_else(|_| unreachable!())
    }

    fn request() -> QuarantineRequest {
        QuarantineRequest {
            protocol: PROTOCOL,
            operation: operation(),
            path: raw(APP),
            identity: identity(7),
            evidence_hash: EvidenceHash::from_bytes([3; 32]),
        }
    }

    fn ask(host: &Host, request: &QuarantineRequest, caller: u32) -> Option<HelperResponse> {
        let body = encode(&HelperRequest::Quarantine(request.clone()));
        response(&answer(&body, caller, "0.11.0", host))
    }

    fn moved(host: &Host) -> bool {
        host.calls()
            .iter()
            .any(|call| matches!(call, Call::Quarantine(..)))
    }

    #[test]
    fn an_admitted_app_is_moved_into_the_callers_store_with_the_helpers_record() {
        let host = Host::eligible();
        assert_eq!(
            ask(&host, &request(), CALLER),
            Some(HelperResponse::Quarantined {
                operation: operation()
            })
        );
        let calls = host.calls();
        assert_eq!(calls[0], Call::Administrator(CALLER));
        assert_eq!(calls[1], Call::Observe(CALLER, raw(APP)));
        assert_eq!(
            calls[2],
            Call::Prepare(raw("/Applications"), vec![b"Word.app".to_vec()])
        );
        assert_eq!(calls[3], Call::InUse);
        let Some(Call::Quarantine(caller, op, record)) = calls.get(4) else {
            unreachable!("moved last: {calls:?}")
        };
        assert_eq!((*caller, *op, calls.len()), (CALLER, operation(), 5));
        let record: HeldRecord =
            serde_json::from_slice(record).unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(record.schema, HELD_SCHEMA);
        assert_eq!(record.caller, CALLER);
        assert_eq!(record.path, raw(APP));
        assert_eq!(record.identity, identity(7));
        assert_eq!(record.prepared, serde_json::json!({"leaf": [87]}));
        assert_eq!(record.helper_version, "0.11.0");
    }

    #[test]
    fn another_protocol_or_a_non_administrator_is_refused_before_looking() {
        let host = Host::eligible();
        let mut old = request();
        old.protocol = 1;
        assert_eq!(
            ask(&host, &old, CALLER),
            Some(HelperResponse::Refused {
                reason: HelperRefusal::OtherProtocol
            })
        );
        assert_eq!(host.calls(), []);
        for admin in [Some(false), None] {
            let host = Host {
                admin,
                ..Host::eligible()
            };
            assert_eq!(
                ask(&host, &request(), CALLER),
                Some(HelperResponse::Refused {
                    reason: HelperRefusal::NotAdministrator
                })
            );
            assert_eq!(host.calls(), [Call::Administrator(CALLER)]);
        }
    }

    #[test]
    fn a_root_caller_is_refused_whatever_it_asks() {
        let host = Host::eligible();
        assert_eq!(
            ask(&host, &request(), 0),
            Some(HelperResponse::Refused {
                reason: HelperRefusal::RootCaller
            })
        );
        assert_eq!(host.calls(), []);
    }

    #[test]
    fn the_helpers_own_evidence_decides_and_nothing_moves_on_refusal() {
        let mut host = Host::eligible();
        host.evidence.owner = Some(CALLER);
        host.evidence.bundle.running = Some(true);
        assert_eq!(
            ask(&host, &request(), CALLER),
            Some(HelperResponse::Ineligible {
                reasons: vec![Reason::OwnedByCaller, Reason::Gate(Ineligible::Running)]
            })
        );
        assert_eq!(host.calls().len(), 2);
    }

    #[test]
    fn a_path_outside_applications_is_never_prepared() {
        for path in [
            "/Users/ana/Applications/Word.app",
            "/Applications/Office/Word.app",
            "/System/Applications/Calculator.app",
            "/Applicationsx/Word.app",
        ] {
            let mut host = Host::eligible();
            host.evidence.bundle.path = raw(path);
            let mut asked = request();
            asked.path = raw(path);
            let answer = ask(&host, &asked, CALLER);
            assert!(
                matches!(answer, Some(HelperResponse::Ineligible { .. })),
                "{path}: {answer:?}"
            );
            assert_eq!(host.calls().len(), 2, "{path}");
        }
    }

    #[test]
    fn an_item_the_executor_records_differently_is_not_moved() {
        let host = Host {
            prepared: Ok((PreparedItem(b"{}".to_vec()), identity(8))),
            ..Host::eligible()
        };
        assert_eq!(
            ask(&host, &request(), CALLER),
            Some(HelperResponse::Ineligible {
                reasons: vec![Reason::Changed]
            })
        );
        assert!(!moved(&host));
        let unreadable = Host {
            prepared: Ok((PreparedItem(b"not json".to_vec()), identity(7))),
            ..Host::eligible()
        };
        assert_eq!(
            ask(&unreadable, &request(), CALLER),
            Some(HelperResponse::Failed {
                reason: QuarantineFailure::MoveFailed,
                left: ItemLocation::Source
            })
        );
        assert!(!moved(&unreadable));
        let failure = ExecutionFailure {
            reason: QuarantineFailure::IdentityMismatch,
            left: ItemLocation::Source,
        };
        let unprepared = Host {
            prepared: Err(failure),
            ..Host::eligible()
        };
        assert_eq!(
            ask(&unprepared, &request(), CALLER),
            Some(HelperResponse::Failed {
                reason: QuarantineFailure::IdentityMismatch,
                left: ItemLocation::Source
            })
        );
        assert!(!moved(&unprepared));
    }

    #[test]
    fn only_a_complete_no_lets_it_move() {
        for in_use in [
            InUse::Partial { unseen: Some(1) },
            InUse::Unknown { reason: "test" },
            InUse::Yes(Vec::new()),
        ] {
            let host = Host {
                in_use: in_use.clone(),
                ..Host::eligible()
            };
            assert_eq!(
                ask(&host, &request(), CALLER),
                Some(HelperResponse::Ineligible {
                    reasons: vec![Reason::InUse]
                }),
                "{in_use:?}"
            );
            assert!(!moved(&host), "{in_use:?}");
        }
    }

    #[test]
    fn a_failed_move_says_where_the_item_is() {
        for left in [ItemLocation::Source, ItemLocation::Quarantine] {
            let host = Host {
                moved: Err(ExecutionFailure {
                    reason: QuarantineFailure::VerificationFailed,
                    left,
                }),
                ..Host::eligible()
            };
            assert_eq!(
                ask(&host, &request(), CALLER),
                Some(HelperResponse::Failed {
                    reason: QuarantineFailure::VerificationFailed,
                    left
                })
            );
        }
    }

    #[test]
    fn requests_with_unknown_or_missing_fields_are_malformed() {
        let host = Host::eligible();
        let body = encode(&HelperRequest::Quarantine(request()));
        let extra = body.replacen('{', r#"{"caller":0,"#, 1);
        let missing = body.replace(&format!(r#""protocol":{PROTOCOL},"#), "");
        assert_ne!(missing, body);
        for body in [extra, missing] {
            assert_eq!(
                response(&answer(&body, CALLER, "1", &host)),
                Some(HelperResponse::Refused {
                    reason: HelperRefusal::Malformed
                }),
                "{body}"
            );
        }
        assert_eq!(host.calls(), []);
    }

    #[test]
    fn the_split_takes_one_name_directly_in_applications() {
        assert_eq!(
            split(&raw("/Applications/Word.app")),
            Some((raw("/Applications"), vec![b"Word.app".to_vec()]))
        );
        for path in [
            "/Applications",
            "/Applications/",
            "/Applications/a/b.app",
            "/Applications/..",
            "/Applicationsx/a.app",
        ] {
            assert_eq!(split(&raw(path)), None, "{path}");
        }
    }
}
