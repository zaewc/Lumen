//! The Windows [`QuarantineExecutor`] (roadmap 9.10d, ADR-0033), built from
//! [`observe`], [`verify`], [`open_store`], [`move_into`], [`restore`],
//! [`write_sidecar`] and [`finalize`], with the same contract as on Unix.
//!
//! `prepare` records the item as JSON with its UTF-16 names; roots must be
//! local drive paths (`X:\…`), used in their verbatim `\\?\` form, because
//! network and other volumes are never quarantined (ADR-0015).

use std::os::windows::ffi::OsStrExt as _;
use std::path::PathBuf;

use lumen_application::ports::{
    ExecutionFailure, Located, NotRestored, PreparedItem, QuarantineExecutor, Removal,
    RestoreFailure, RestoredItem,
};
use lumen_domain::{
    FileId, FileIdentity, ItemLocation, OperationId, PathFlavor, QuarantineFailure, RawPath,
    VolumeId,
};
use serde::{Deserialize, Serialize};

use crate::location::{Left, Removed};
use crate::win_finalize::{finalize, forget};
use crate::win_move::{Direction, WinMoveError, move_into, restore, write_sidecar};
use crate::win_store::{WinStore, open_store};
use crate::win_sys::{self, Want};
use crate::win_verify::{WinExpected, WinIdentity, WinPlanItem, check_held, observe, verify};

/// The JSON form of a prepared item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Prepared {
    root: Vec<u16>,
    root_identity: (u64, String),
    components: Vec<Vec<u16>>,
    parent_identity: (u64, String),
    leaf: Vec<u16>,
    directory: bool,
    identity: (u64, String),
    size: i64,
    modified: i64,
    links: u32,
}

fn pair(id: WinIdentity) -> (u64, String) {
    (id.volume, format!("{:032x}", id.file))
}

fn unpair((volume, file): &(u64, String)) -> Option<WinIdentity> {
    Some(WinIdentity {
        volume: *volume,
        file: u128::from_str_radix(file, 16).ok()?,
    })
}

impl Prepared {
    fn from_plan(plan: &WinPlanItem) -> Self {
        Self {
            root: plan.root.clone(),
            root_identity: pair(plan.root_identity),
            components: plan.components.clone(),
            parent_identity: pair(plan.parent_identity),
            leaf: plan.leaf.clone(),
            directory: plan.expected.directory,
            identity: pair(plan.expected.identity),
            size: plan.expected.size,
            modified: plan.expected.modified,
            links: plan.expected.links,
        }
    }

    fn plan(&self) -> Option<WinPlanItem> {
        Some(WinPlanItem {
            root: self.root.clone(),
            root_identity: unpair(&self.root_identity)?,
            components: self.components.clone(),
            parent_identity: unpair(&self.parent_identity)?,
            leaf: self.leaf.clone(),
            expected: WinExpected {
                identity: unpair(&self.identity)?,
                directory: self.directory,
                size: self.size,
                modified: self.modified,
                links: self.links,
            },
        })
    }
}

/// One store location: a base directory and the store's names below it.
#[derive(Debug, Clone)]
struct Location {
    base: Vec<u16>,
    names: Vec<Vec<u16>>,
}

/// The executor for one or more store locations (ADR-0015), at most one per
/// volume (ADR-0046): an item goes into, and is found in, the store on its
/// own volume.
#[derive(Debug, Clone)]
pub struct Executor {
    stores: Vec<Location>,
}

/// `path` in verbatim form, if it is a local drive path.
fn verbatim(path: &[u16]) -> Option<Vec<u16>> {
    let prefix: Vec<u16> = r"\\?\".encode_utf16().collect();
    if path.starts_with(&prefix) {
        return Some(path.to_vec());
    }
    let drive = path
        .first()
        .is_some_and(|&u| u8::try_from(u).is_ok_and(|b| b.is_ascii_alphabetic()));
    let rooted = path.get(1) == Some(&u16::from(b':')) && path.get(2) == Some(&u16::from(b'\\'));
    (drive && rooted).then(|| [prefix.as_slice(), path].concat())
}

impl Location {
    fn new(base: impl Into<PathBuf>, names: Vec<Vec<u8>>) -> Self {
        let base: Vec<u16> = base.into().as_os_str().encode_wide().collect();
        Self {
            base: verbatim(&base).unwrap_or(base),
            names: names
                .into_iter()
                .map(|n| String::from_utf8_lossy(&n).encode_utf16().collect())
                .collect(),
        }
    }
}

impl Executor {
    /// An executor quarantining into `names` (UTF-8) below `base`.
    pub fn new(base: impl Into<PathBuf>, names: Vec<Vec<u8>>) -> Self {
        Self {
            stores: vec![Location::new(base, names)],
        }
    }

    /// The same executor with one more store location, `names` (UTF-8) below
    /// `base`, for items on that base's volume (ADR-0046).
    #[must_use]
    pub fn with_store(mut self, base: impl Into<PathBuf>, names: Vec<Vec<u8>>) -> Self {
        self.stores.push(Location::new(base, names));
        self
    }

    /// The store on `volume`: the first location whose base is on that
    /// volume and opens safely. `open_store` creates nothing below a base on
    /// another volume.
    fn store(&self, volume: u64) -> Option<WinStore> {
        self.store_at(volume).map(|(store, _)| store)
    }

    /// [`Self::store`] and the location it was opened at.
    fn store_at(&self, volume: u64) -> Option<(WinStore, &Location)> {
        self.stores.iter().find_map(|location| {
            let names: Vec<&[u16]> = location.names.iter().map(Vec::as_slice).collect();
            open_store(&location.base, &names, volume)
                .ok()
                .map(|store| (store, location))
        })
    }
}

fn failure(reason: QuarantineFailure, left: ItemLocation) -> ExecutionFailure {
    ExecutionFailure { reason, left }
}

fn at_source(reason: QuarantineFailure) -> ExecutionFailure {
    failure(reason, ItemLocation::Source)
}

fn parse(item: &PreparedItem) -> Option<WinPlanItem> {
    serde_json::from_slice::<Prepared>(&item.0).ok()?.plan()
}

/// The path `item` was prepared from, for showing quarantined items (roadmap
/// 10.15). `None` if the record is not this executor's.
pub fn original_path(item: &PreparedItem) -> Option<PathBuf> {
    use std::os::windows::ffi::OsStringExt as _;
    let plan = parse(item)?;
    let mut path = PathBuf::from(std::ffi::OsString::from_wide(&plan.root));
    for component in &plan.components {
        path.push(std::ffi::OsString::from_wide(component));
    }
    path.push(std::ffi::OsString::from_wide(&plan.leaf));
    Some(path)
}

fn name_of(operation: OperationId) -> Vec<u16> {
    operation.to_string().encode_utf16().collect()
}

const fn location(left: Left) -> ItemLocation {
    match left {
        Left::AtSource => ItemLocation::Source,
        Left::InStore => ItemLocation::Quarantine,
    }
}

impl QuarantineExecutor for Executor {
    fn prepare(
        &self,
        root: &RawPath,
        relative: &[Vec<u8>],
    ) -> Result<PreparedItem, ExecutionFailure> {
        let refused = || at_source(QuarantineFailure::IdentityMismatch);
        if root.flavor() != PathFlavor::Windows {
            return Err(refused());
        }
        let root = root
            .to_windows_wide()
            .and_then(|r| verbatim(&r))
            .ok_or_else(refused)?;
        let relative = relative
            .iter()
            .map(|c| {
                RawPath::from_stored(PathFlavor::Windows, c.clone())
                    .ok()
                    .and_then(|p| p.to_windows_wide())
            })
            .collect::<Option<Vec<_>>>()
            .ok_or_else(refused)?;
        let plan = observe(&root, &relative).map_err(|_| refused())?;
        serde_json::to_vec(&Prepared::from_plan(&plan))
            .map(PreparedItem)
            .map_err(|_| refused())
    }

    fn quarantine(
        &self,
        item: &PreparedItem,
        operation: OperationId,
        manifest: &[u8],
    ) -> Result<(), ExecutionFailure> {
        let plan = parse(item).ok_or_else(|| at_source(QuarantineFailure::IdentityMismatch))?;
        let verified = verify(&plan).map_err(|_| at_source(QuarantineFailure::IdentityMismatch))?;
        let store = self
            .store(plan.expected.identity.volume)
            .ok_or_else(|| at_source(QuarantineFailure::VolumeUnsupported))?;
        let name = name_of(operation);
        move_into(&verified, &store, &name, &plan.expected).map_err(|error| {
            let reason = match error {
                WinMoveError::OtherVolume => QuarantineFailure::VolumeUnsupported,
                WinMoveError::InUse => QuarantineFailure::InUse,
                WinMoveError::SourceRemains | WinMoveError::Unverified(_) => {
                    QuarantineFailure::VerificationFailed
                }
                _ => QuarantineFailure::MoveFailed,
            };
            failure(reason, location(error.left(Direction::Quarantine)))
        })?;
        write_sidecar(&store, &name, manifest).map_err(|_| {
            failure(
                QuarantineFailure::VerificationFailed,
                ItemLocation::Quarantine,
            )
        })
    }

    fn locate(&self, item: &PreparedItem, operation: OperationId) -> Located {
        let Some(plan) = parse(item) else {
            return Located::Unknown;
        };
        if verify(&plan).is_ok() {
            return Located::AtSource;
        }
        let Some(store) = self.store(plan.expected.identity.volume) else {
            return Located::Unknown;
        };
        let want = Want::Movable {
            directory: plan.expected.directory,
        };
        match win_sys::open_at(&store.dir, &name_of(operation), want)
            .and_then(|handle| win_sys::facts(&handle))
        {
            Ok(facts) if check_held(&facts, &plan.expected).is_ok() => Located::InStore,
            _ => Located::Unknown,
        }
    }

    fn restore(
        &self,
        item: &PreparedItem,
        operation: OperationId,
    ) -> Result<RestoredItem, RestoreFailure> {
        let in_store = RestoreFailure {
            reason: NotRestored::Failed,
            left: ItemLocation::Quarantine,
        };
        let plan = parse(item).ok_or(in_store)?;
        let store = self.store(plan.expected.identity.volume).ok_or(in_store)?;
        restore(&store, &name_of(operation), &plan)
            .map(|restored| RestoredItem {
                parent_replaced: restored.parent_replaced,
            })
            .map_err(|error| RestoreFailure {
                reason: match error {
                    WinMoveError::NameTaken => NotRestored::NameTaken,
                    WinMoveError::ParentUnreachable(_) => NotRestored::OriginUnreachable,
                    WinMoveError::NotInStore(_) => NotRestored::ItemChanged,
                    _ => NotRestored::Failed,
                },
                left: location(error.left(Direction::Restore)),
            })
    }

    fn finalize(&self, item: &PreparedItem, operation: OperationId) -> Result<(), Removal> {
        let plan = parse(item).ok_or(Removal::Nothing)?;
        let store = self
            .store(plan.expected.identity.volume)
            .ok_or(Removal::Nothing)?;
        finalize(&store, &name_of(operation), &plan.expected).map_err(|error| {
            match error.removed() {
                Removed::Nothing => Removal::Nothing,
                Removed::Partly => Removal::Partly,
                Removed::Item => Removal::Item,
            }
        })
    }

    fn forget(&self, item: &PreparedItem, operation: OperationId) -> bool {
        let Some(plan) = parse(item) else {
            return false;
        };
        self.store(plan.expected.identity.volume)
            .is_some_and(|store| forget(&store, &name_of(operation)).is_ok())
    }

    fn held(&self, item: &PreparedItem, operation: OperationId) -> Option<(RawPath, FileIdentity)> {
        let plan = parse(item)?;
        let (store, location) = self.store_at(plan.expected.identity.volume)?;
        let name = name_of(operation);
        let want = Want::Movable {
            directory: plan.expected.directory,
        };
        let facts = win_sys::open_at(&store.dir, &name, want)
            .and_then(|handle| win_sys::facts(&handle))
            .ok()?;
        check_held(&facts, &plan.expected).ok()?;
        let separator = u16::from(b'\\');
        let mut path = location.base.clone();
        for component in location.names.iter().chain([&name]) {
            if path.last() != Some(&separator) {
                path.push(separator);
            }
            path.extend_from_slice(component);
        }
        // The enumerators' identity rule (`vsn:<serial>`, 128-bit file ID).
        Some((
            RawPath::from_windows_wide(&path).ok()?,
            FileIdentity {
                volume: VolumeId::new(format!("vsn:{:016x}", facts.volume)).ok()?,
                file: FileId::new(facts.file),
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op(n: u8) -> OperationId {
        format!("0190f0d2-6c1e-7a3b-8c4d-5e6f708192{n:02x}")
            .parse()
            .unwrap_or_else(|_| unreachable!())
    }

    fn setup() -> (lumen_testkit::Fixture, PathBuf, Executor, RawPath) {
        let fx = lumen_testkit::Fixture::build(&[lumen_testkit::Node::File {
            path: "cache/app/data.bin".into(),
            contents: b"0123456789".to_vec(),
        }])
        .unwrap_or_else(|e| unreachable!("{e}"));
        let root = std::fs::canonicalize(fx.root()).unwrap_or_else(|e| unreachable!("{e}"));
        let executor = Executor::new(root.clone(), vec![b"Quarantine".to_vec()]);
        let units: Vec<u16> = root.as_os_str().encode_wide().collect();
        let raw = RawPath::from_windows_wide(&units).unwrap_or_else(|e| unreachable!("{e}"));
        (fx, root, executor, raw)
    }

    fn relative() -> Vec<Vec<u8>> {
        vec![b"cache".to_vec(), b"app".to_vec(), b"data.bin".to_vec()]
    }

    #[test]
    fn the_whole_lifecycle_runs_through_the_port() {
        let (_fx, root, executor, raw) = setup();
        let item = executor
            .prepare(&raw, &relative())
            .unwrap_or_else(|e| unreachable!("{e:?}"));
        assert_eq!(executor.locate(&item, op(1)), Located::AtSource);
        assert_eq!(executor.quarantine(&item, op(1), b"{\"m\":1}"), Ok(()));
        assert_eq!(executor.locate(&item, op(1)), Located::InStore);
        assert!(!root.join("cache/app/data.bin").exists());
        assert_eq!(
            executor.restore(&item, op(1)),
            Ok(RestoredItem {
                parent_replaced: false
            })
        );
        assert_eq!(executor.locate(&item, op(1)), Located::AtSource);
        assert_eq!(executor.quarantine(&item, op(2), b"{}"), Ok(()));
        assert_eq!(executor.finalize(&item, op(2)), Ok(()));
        assert_eq!(executor.locate(&item, op(2)), Located::Unknown);
        assert_eq!(executor.finalize(&item, op(2)), Err(Removal::Nothing));
    }

    #[test]
    fn a_location_that_cannot_hold_the_store_is_passed_over() {
        let (_fx, root, _executor, raw) = setup();
        let executor = Executor::new(root.join("missing-base"), vec![b"Quarantine".to_vec()])
            .with_store(root.clone(), vec![b"Quarantine".to_vec()]);
        let item = executor
            .prepare(&raw, &relative())
            .unwrap_or_else(|e| unreachable!("{e:?}"));
        assert_eq!(executor.quarantine(&item, op(4), b"{}"), Ok(()));
        assert_eq!(executor.locate(&item, op(4)), Located::InStore);
        assert!(root.join("Quarantine").is_dir());
        assert!(!root.join("missing-base").exists(), "nothing created there");
        assert_eq!(
            executor.restore(&item, op(4)),
            Ok(RestoredItem {
                parent_replaced: false
            })
        );
    }

    #[test]
    fn a_changed_item_is_left_at_its_source() {
        let (_fx, root, executor, raw) = setup();
        let item = executor
            .prepare(&raw, &relative())
            .unwrap_or_else(|e| unreachable!("{e:?}"));
        std::fs::write(root.join("cache/app/data.bin"), b"changed!!!!")
            .unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(
            executor.quarantine(&item, op(3), b"{}"),
            Err(ExecutionFailure {
                reason: QuarantineFailure::IdentityMismatch,
                left: ItemLocation::Source
            })
        );
        assert!(root.join("cache/app/data.bin").exists());
    }

    #[test]
    fn network_and_unix_roots_are_refused() {
        let (_fx, _root, executor, _raw) = setup();
        let unc: Vec<u16> = r"\\server\share".encode_utf16().collect();
        let unix =
            RawPath::from_unix_bytes(b"/tmp".to_vec()).unwrap_or_else(|e| unreachable!("{e}"));
        for root in [
            RawPath::from_windows_wide(&unc).unwrap_or_else(|e| unreachable!("{e}")),
            unix,
        ] {
            assert!(executor.prepare(&root, &relative()).is_err());
        }
    }
}
