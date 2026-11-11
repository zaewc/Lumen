//! The [`QuarantineExecutor`] adapter (roadmap 9.6b): the application's only
//! way to move user files, built from [`verify`], [`open_store`],
//! [`move_into`] and [`write_sidecar`].
//!
//! `prepare` walks to the item by handle and records exactly what it is now as
//! a [`PreparedItem`] (JSON, with raw path bytes). `quarantine` acts only if the
//! item still matches that record, and names it in the store after the
//! operation. `locate` tells reconciliation where an interrupted item is, and
//! `restore` moves a quarantined item back without replacing anything, and
//! `finalize` removes it permanently from the store. Every
//! failure says where it left the item: untouched at the source, or in the
//! store and restorable.

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt as _;
use std::path::{Path, PathBuf};

use lumen_application::ports::{
    ExecutionFailure, Located, NotRestored, PreparedItem, QuarantineExecutor, Removal,
    RestoreFailure, RestoredItem,
};
use lumen_domain::{
    FileId, FileIdentity, ItemLocation, OperationId, QuarantineFailure, RawPath, UnixOwnership,
    VolumeId,
};
use rustix::fs::{AtFlags, FileType, Mode, OFlags, fstat, open, openat, statat};
use serde::{Deserialize, Serialize};

use crate::finalize::{Removed, finalize, forget};
use crate::move_into::{Left, MoveError, move_into};
use crate::restore::{RestoreError, restore};
use crate::sidecar::write_sidecar;
use crate::store::{Store, StoreError, open_store};
use crate::verify::{
    ExpectedLeaf, LeafKind, PlanItem, UnixIdentity, check_held, identity, ownership, verify,
};

/// The JSON form of a prepared item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Prepared {
    root: Vec<u8>,
    root_identity: (u64, u64),
    components: Vec<Vec<u8>>,
    parent_identity: (u64, u64),
    leaf: Vec<u8>,
    directory: bool,
    identity: (u64, u64),
    size: u64,
    modified: (i64, i64),
    links: u64,
    /// `(st_mode, st_uid, st_gid)` when prepared; absent in records from
    /// before roadmap 10.22.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    unix: Option<(u32, u32, u32)>,
}

fn pair(id: UnixIdentity) -> (u64, u64) {
    (id.dev, id.ino)
}

fn unpair((dev, ino): (u64, u64)) -> UnixIdentity {
    UnixIdentity { dev, ino }
}

impl Prepared {
    /// This record with its volume's device number changed to `device`;
    /// `None` unless every identity in it is on one device, as `verify`
    /// guarantees for a record it prepared.
    fn renumbered(&self, device: u64) -> Option<Self> {
        let recorded = self.identity.0;
        if self.root_identity.0 != recorded || self.parent_identity.0 != recorded {
            return None;
        }
        let mut renumbered = self.clone();
        renumbered.root_identity.0 = device;
        renumbered.parent_identity.0 = device;
        renumbered.identity.0 = device;
        Some(renumbered)
    }

    fn plan(&self) -> PlanItem {
        PlanItem {
            root: PathBuf::from(OsStr::from_bytes(&self.root)),
            root_identity: unpair(self.root_identity),
            components: self.components.clone(),
            parent_identity: unpair(self.parent_identity),
            leaf: self.leaf.clone(),
            expected: ExpectedLeaf {
                identity: unpair(self.identity),
                kind: if self.directory {
                    LeafKind::Directory
                } else {
                    LeafKind::File
                },
                size: self.size,
                modified: self.modified,
                links: self.links,
            },
        }
    }
}

/// The executor for one store location (a base directory and the store's
/// names below it, ADR-0015).
#[derive(Debug, Clone)]
pub struct Executor {
    store_base: PathBuf,
    store_names: Vec<Vec<u8>>,
}

impl Executor {
    /// An executor quarantining into `names` below `base`.
    pub fn new(base: impl Into<PathBuf>, names: Vec<Vec<u8>>) -> Self {
        Self {
            store_base: base.into(),
            store_names: names,
        }
    }

    fn store(&self, volume: u64) -> Option<Store> {
        self.open_on(volume).ok()
    }

    /// The store on `volume`, or why it cannot be used:
    /// [`QuarantineFailure::VolumeUnsupported`] if the store is on another
    /// volume, [`QuarantineFailure::StoreUnavailable`] if it failed a check
    /// or cannot be reached (ADR-0066).
    fn open_on(&self, volume: u64) -> Result<Store, QuarantineFailure> {
        let names: Vec<&[u8]> = self.store_names.iter().map(Vec::as_slice).collect();
        open_store(&self.store_base, &names, volume).map_err(|error| match error {
            StoreError::OtherVolume => QuarantineFailure::VolumeUnsupported,
            _ => QuarantineFailure::StoreUnavailable,
        })
    }

    /// The store on the volume the store base is on now.
    fn current_store(&self) -> Option<(Store, u64)> {
        // The device number in the form every record carries it (`identity`).
        let device = identity(&rustix::fs::stat(&self.store_base).ok()?).dev;
        Some((self.store(device)?, device))
    }

    /// The store for `prepared`'s volume: the one on its recorded device, or,
    /// if there is none, the one on the device the store base is on now.
    fn store_for(&self, prepared: &Prepared) -> Option<Store> {
        self.store(prepared.identity.0)
            .or_else(|| self.current_store().map(|(store, _)| store))
    }

    /// The store holding `prepared`'s item under `name`, and the plan to act
    /// on it with.
    ///
    /// A device number names a volume only while it stays mounted: macOS and
    /// Linux assign `st_dev` when a volume is mounted, so a restart or an
    /// update can renumber the volume the store and its items are on, and no
    /// store is then on the recorded device. In that case the store on the
    /// base's current device is opened, with the same checks as always, and
    /// the item held there under its operation's name is accepted only if
    /// `check_held` matches it with the device number changed to that one. A
    /// quarantined item never leaves the store's volume, so every identity in
    /// the record is renumbered together, and each later check (the original
    /// parent's identity included) runs unchanged on the new number.
    fn held_store(&self, prepared: &Prepared, name: &[u8]) -> Option<(Store, PlanItem)> {
        if let Some(store) = self.store(prepared.identity.0) {
            return Some((store, prepared.plan()));
        }
        let (store, device) = self.current_store()?;
        let plan = prepared.renumbered(device)?.plan();
        let stat = statat(store.handle(), name, AtFlags::SYMLINK_NOFOLLOW).ok()?;
        check_held(&stat, &plan.expected).ok()?;
        Some((store, plan))
    }
}

fn failure(reason: QuarantineFailure, left: ItemLocation) -> ExecutionFailure {
    ExecutionFailure { reason, left }
}

fn at_source(reason: QuarantineFailure) -> ExecutionFailure {
    failure(reason, ItemLocation::Source)
}

fn directory_flags() -> OFlags {
    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC;
    #[cfg(target_vendor = "apple")]
    let flags = flags | OFlags::NOFOLLOW_ANY;
    #[cfg(not(target_vendor = "apple"))]
    let flags = flags | OFlags::NOFOLLOW;
    flags
}

/// Walks to the item by handle and records it as it is now.
fn prepare(root: &Path, relative: &[Vec<u8>]) -> Option<Prepared> {
    let (leaf, components) = relative.split_last()?;
    let mut dir = open(root, directory_flags(), Mode::empty()).ok()?;
    let root_identity = identity(&fstat(&dir).ok()?);
    for component in components {
        dir = openat(&dir, component.as_slice(), directory_flags(), Mode::empty()).ok()?;
    }
    let parent_identity = identity(&fstat(&dir).ok()?);
    let stat = statat(&dir, leaf.as_slice(), AtFlags::SYMLINK_NOFOLLOW).ok()?;
    let directory = match FileType::from_raw_mode(stat.st_mode) {
        FileType::Directory => true,
        FileType::RegularFile => false,
        _ => return None,
    };
    let expected = crate::verify::expected_from(&stat, directory);
    Some(Prepared {
        root: root.as_os_str().as_bytes().to_vec(),
        root_identity: pair(root_identity),
        components: components.to_vec(),
        parent_identity: pair(parent_identity),
        leaf: leaf.clone(),
        directory,
        identity: pair(expected.identity),
        size: expected.size,
        modified: expected.modified,
        links: expected.links,
        unix: Some(ownership(&stat)),
    })
}

fn parse(item: &PreparedItem) -> Option<Prepared> {
    serde_json::from_slice(&item.0).ok()
}

/// The path `item` was prepared from, for showing quarantined items (roadmap
/// 10.15). `None` if the record is not this executor's.
pub fn original_path(item: &PreparedItem) -> Option<PathBuf> {
    let prepared = parse(item)?;
    let mut path = PathBuf::from(OsStr::from_bytes(&prepared.root));
    for component in &prepared.components {
        path.push(OsStr::from_bytes(component));
    }
    path.push(OsStr::from_bytes(&prepared.leaf));
    Some(path)
}

impl QuarantineExecutor for Executor {
    fn prepare(
        &self,
        root: &RawPath,
        relative: &[Vec<u8>],
    ) -> Result<PreparedItem, ExecutionFailure> {
        let root = PathBuf::from(OsStr::from_bytes(root.as_bytes()));
        let prepared = prepare(&root, relative)
            .ok_or_else(|| at_source(QuarantineFailure::IdentityMismatch))?;
        serde_json::to_vec(&prepared)
            .map(PreparedItem)
            .map_err(|_| at_source(QuarantineFailure::IdentityMismatch))
    }

    fn quarantine(
        &self,
        item: &PreparedItem,
        operation: OperationId,
        manifest: &[u8],
    ) -> Result<(), ExecutionFailure> {
        let prepared = parse(item).ok_or_else(|| at_source(QuarantineFailure::IdentityMismatch))?;
        let plan = prepared.plan();
        let verified = verify(&plan).map_err(|_| at_source(QuarantineFailure::IdentityMismatch))?;
        let store = self
            .open_on(plan.expected.identity.dev)
            .map_err(at_source)?;
        let name = operation.to_string();
        move_into(&verified, &store, name.as_bytes(), &plan.expected).map_err(|error| {
            let reason = match error {
                MoveError::OtherVolume | MoveError::NoReplaceUnsupported => {
                    QuarantineFailure::VolumeUnsupported
                }
                MoveError::Sync(_) | MoveError::SourceRemains | MoveError::Unverified(_) => {
                    QuarantineFailure::VerificationFailed
                }
                _ => QuarantineFailure::MoveFailed,
            };
            let left = match error.left() {
                Left::AtSource => ItemLocation::Source,
                Left::InStore => ItemLocation::Quarantine,
            };
            failure(reason, left)
        })?;
        write_sidecar(&store, name.as_bytes(), manifest).map_err(|_| {
            failure(
                QuarantineFailure::VerificationFailed,
                ItemLocation::Quarantine,
            )
        })
    }

    fn locate(&self, item: &PreparedItem, operation: OperationId) -> Located {
        let Some(prepared) = parse(item) else {
            return Located::Unknown;
        };
        if verify(&prepared.plan()).is_ok() {
            return Located::AtSource;
        }
        let name = operation.to_string();
        let Some((store, plan)) = self.held_store(&prepared, name.as_bytes()) else {
            return Located::Unknown;
        };
        match statat(store.handle(), name.as_bytes(), AtFlags::SYMLINK_NOFOLLOW) {
            Ok(stat) if check_held(&stat, &plan.expected).is_ok() => Located::InStore,
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
        let prepared = parse(item).ok_or(in_store)?;
        let name = operation.to_string();
        let (store, plan) = self
            .held_store(&prepared, name.as_bytes())
            .ok_or(in_store)?;
        restore(&store, name.as_bytes(), &plan)
            .map(|restored| RestoredItem {
                parent_replaced: restored.parent_replaced,
                matches_record: restored.matches_record,
            })
            .map_err(|error| RestoreFailure {
                reason: match error {
                    RestoreError::NameTaken => NotRestored::NameTaken,
                    RestoreError::ParentUnreachable(_) => NotRestored::OriginUnreachable,
                    RestoreError::NotInStore(_) => NotRestored::ItemChanged,
                    _ => NotRestored::Failed,
                },
                left: match error.left() {
                    Left::InStore => ItemLocation::Quarantine,
                    Left::AtSource => ItemLocation::Source,
                },
            })
    }

    fn finalize(&self, item: &PreparedItem, operation: OperationId) -> Result<(), Removal> {
        let prepared = parse(item).ok_or(Removal::Nothing)?;
        let name = operation.to_string();
        let (store, plan) = self
            .held_store(&prepared, name.as_bytes())
            .ok_or(Removal::Nothing)?;
        finalize(&store, name.as_bytes(), &plan.expected).map_err(|error| match error.removed() {
            Removed::Nothing => Removal::Nothing,
            Removed::Partly => Removal::Partly,
            Removed::Item => Removal::Item,
        })
    }

    fn unix_ownership(&self, item: &PreparedItem) -> Option<UnixOwnership> {
        let (mode, uid, gid) = parse(item)?.unix?;
        Some(UnixOwnership { mode, uid, gid })
    }

    fn forget(&self, item: &PreparedItem, operation: OperationId) -> bool {
        let Some(prepared) = parse(item) else {
            return false;
        };
        self.store_for(&prepared)
            .is_some_and(|store| forget(&store, operation.to_string().as_bytes()).is_ok())
    }

    fn held(&self, item: &PreparedItem, operation: OperationId) -> Option<(RawPath, FileIdentity)> {
        let prepared = parse(item)?;
        let name = operation.to_string();
        let (store, plan) = self.held_store(&prepared, name.as_bytes())?;
        let stat = statat(store.handle(), name.as_bytes(), AtFlags::SYMLINK_NOFOLLOW).ok()?;
        check_held(&stat, &plan.expected).ok()?;
        let mut path = self.store_base.clone();
        path.extend(self.store_names.iter().map(|n| OsStr::from_bytes(n)));
        path.push(&name);
        // The enumerators' identity rule (`dev:<hex>`, inode).
        let held = identity(&stat);
        Some((
            RawPath::from_unix_bytes(path.as_os_str().as_bytes().to_vec()).ok()?,
            FileIdentity {
                volume: VolumeId::new(format!("dev:{:x}", held.dev)).ok()?,
                file: FileId::new(u128::from(held.ino)),
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt as _;

    use lumen_testkit::{Fixture, Node};

    use super::*;

    fn op(n: u8) -> OperationId {
        format!("0190f0d2-6c1e-7a3b-8c4d-5e6f708192{n:02x}")
            .parse()
            .unwrap_or_else(|_| unreachable!())
    }

    /// A private fixture holding `cache/app/data.bin`, and an executor whose
    /// store is `Quarantine` in the same fixture.
    fn setup() -> (Fixture, PathBuf, Executor) {
        let fx = Fixture::build(&[Node::File {
            path: "cache/app/data.bin".into(),
            contents: b"0123456789".to_vec(),
        }])
        .unwrap_or_else(|e| unreachable!("{e}"));
        let mut perms = std::fs::metadata(fx.root())
            .unwrap_or_else(|e| unreachable!("{e}"))
            .permissions();
        perms.set_mode(0o755);
        #[allow(
            clippy::disallowed_methods,
            reason = "test fixture inside its own temporary directory"
        )]
        std::fs::set_permissions(fx.root(), perms).unwrap_or_else(|e| unreachable!("{e}"));
        let root = std::fs::canonicalize(fx.root()).unwrap_or_else(|e| unreachable!("{e}"));
        let executor = Executor::new(root.clone(), vec![b"Quarantine".to_vec()]);
        (fx, root, executor)
    }

    fn raw(path: &Path) -> RawPath {
        RawPath::from_unix_bytes(path.as_os_str().as_bytes().to_vec())
            .unwrap_or_else(|_| unreachable!())
    }

    fn relative() -> Vec<Vec<u8>> {
        vec![b"cache".to_vec(), b"app".to_vec(), b"data.bin".to_vec()]
    }

    #[test]
    fn a_store_that_fails_its_checks_is_reported_as_unavailable() {
        let (_fx, root, _executor) = setup();
        let shared = root.join("Shared");
        std::fs::create_dir(&shared).unwrap_or_else(|e| unreachable!("{e}"));
        let mut perms = std::fs::metadata(&shared)
            .unwrap_or_else(|e| unreachable!("{e}"))
            .permissions();
        perms.set_mode(0o777);
        #[allow(
            clippy::disallowed_methods,
            reason = "test fixture inside its own temporary directory"
        )]
        std::fs::set_permissions(&shared, perms).unwrap_or_else(|e| unreachable!("{e}"));
        let executor = Executor::new(root.clone(), vec![b"Shared".to_vec()]);
        let prepared = executor
            .prepare(&raw(&root), &relative())
            .unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(
            executor.quarantine(&prepared, op(12), b"{}"),
            Err(ExecutionFailure {
                reason: QuarantineFailure::StoreUnavailable,
                left: ItemLocation::Source
            })
        );
        assert!(root.join("cache/app/data.bin").exists());
    }

    #[test]
    fn a_prepared_item_is_quarantined_with_its_manifest() {
        let (_fx, root, executor) = setup();
        let prepared = executor
            .prepare(&raw(&root), &relative())
            .unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(executor.locate(&prepared, op(1)), Located::AtSource);
        assert_eq!(executor.quarantine(&prepared, op(1), b"{\"m\":1}"), Ok(()));
        let stored = root.join("Quarantine").join(op(1).to_string());
        assert!(stored.exists());
        assert!(!root.join("cache/app/data.bin").exists());
        let sidecar = root
            .join("Quarantine")
            .join(format!("{}.manifest.json", op(1)));
        assert_eq!(std::fs::read(sidecar).unwrap_or_default(), b"{\"m\":1}");
        assert_eq!(executor.locate(&prepared, op(1)), Located::InStore);
    }

    #[test]
    fn a_quarantined_item_is_restored_once_and_never_over_another() {
        let (_fx, root, executor) = setup();
        let prepared = executor
            .prepare(&raw(&root), &relative())
            .unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(executor.quarantine(&prepared, op(3), b"{}"), Ok(()));
        let sidecar = root
            .join("Quarantine")
            .join(format!("{}.manifest.json", op(3)));
        assert!(
            !executor.forget(&prepared, op(3)),
            "nothing is forgotten while the item is held"
        );
        assert!(sidecar.exists());
        std::fs::write(root.join("cache/app/data.bin"), b"newcomer")
            .unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(
            executor.restore(&prepared, op(3)),
            Err(RestoreFailure {
                reason: NotRestored::NameTaken,
                left: ItemLocation::Quarantine
            })
        );
        #[allow(
            clippy::disallowed_methods,
            reason = "test fixture inside its own temporary directory"
        )]
        std::fs::remove_file(root.join("cache/app/data.bin"))
            .unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(
            executor.restore(&prepared, op(3)),
            Ok(RestoredItem {
                parent_replaced: false,
                matches_record: true
            })
        );
        assert_eq!(executor.locate(&prepared, op(3)), Located::AtSource);
        assert_eq!(
            std::fs::read(root.join("cache/app/data.bin")).unwrap_or_default(),
            b"0123456789"
        );
        assert!(executor.forget(&prepared, op(3)), "restored: forgotten");
        assert!(!sidecar.exists());
        assert!(
            executor.forget(&prepared, op(3)),
            "forgetting twice is fine"
        );
    }

    #[test]
    fn a_prepared_item_records_its_unix_ownership() {
        let (_fx, root, executor) = setup();
        let prepared = executor
            .prepare(&raw(&root), &relative())
            .unwrap_or_else(|e| unreachable!("{e}"));
        let meta = std::fs::symlink_metadata(root.join("cache/app/data.bin"))
            .unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(
            executor.unix_ownership(&prepared),
            Some(UnixOwnership {
                mode: std::os::unix::fs::MetadataExt::mode(&meta),
                uid: std::os::unix::fs::MetadataExt::uid(&meta),
                gid: std::os::unix::fs::MetadataExt::gid(&meta),
            })
        );
        let mut older: serde_json::Value =
            serde_json::from_slice(&prepared.0).unwrap_or_else(|e| unreachable!("{e}"));
        if let Some(fields) = older.as_object_mut() {
            let _removed = fields.remove("unix");
        }
        let older = PreparedItem(serde_json::to_vec(&older).unwrap_or_default());
        assert_eq!(
            executor.unix_ownership(&older),
            None,
            "a record from before still parses"
        );
        assert_eq!(original_path(&older), original_path(&prepared));
    }

    #[test]
    fn a_prepared_item_names_its_original_path() {
        let (_fx, root, executor) = setup();
        let prepared = executor
            .prepare(&raw(&root), &relative())
            .unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(
            original_path(&prepared),
            Some(root.join("cache/app/data.bin"))
        );
        assert_eq!(original_path(&PreparedItem(b"{}".to_vec())), None);
    }

    #[test]
    fn a_quarantined_item_is_finalized_with_its_sidecar() {
        let (_fx, root, executor) = setup();
        let prepared = executor
            .prepare(&raw(&root), &relative())
            .unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(executor.held(&prepared, op(4)), None, "not yet held");
        assert_eq!(executor.quarantine(&prepared, op(4), b"{}"), Ok(()));
        let (path, identity) = executor
            .held(&prepared, op(4))
            .unwrap_or_else(|| unreachable!("held"));
        let stored = root.join("Quarantine").join(op(4).to_string());
        assert_eq!(path, raw(&stored), "named where it is held");
        let meta = std::fs::symlink_metadata(&stored).unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(
            (identity.volume.as_str(), identity.file.get()),
            (
                format!("dev:{:x}", std::os::unix::fs::MetadataExt::dev(&meta)).as_str(),
                u128::from(std::os::unix::fs::MetadataExt::ino(&meta))
            ),
            "in the enumerators' form"
        );
        assert_eq!(executor.finalize(&prepared, op(4)), Ok(()));
        assert_eq!(executor.held(&prepared, op(4)), None, "gone");
        assert_eq!(executor.locate(&prepared, op(4)), Located::Unknown);
        let left = std::fs::read_dir(root.join("Quarantine"))
            .unwrap_or_else(|e| unreachable!("{e}"))
            .count();
        assert_eq!(left, 0, "item and sidecar are gone");
        assert_eq!(
            executor.finalize(&prepared, op(4)),
            Err(Removal::Nothing),
            "finalizing twice removes nothing"
        );
    }

    /// `prepared` as it reads after its volume was renumbered: every device
    /// number moved by `by`, and the item's inode moved by `inode_by`.
    fn renumbered(prepared: &PreparedItem, by: u64, inode_by: u64) -> PreparedItem {
        let mut record: serde_json::Value =
            serde_json::from_slice(&prepared.0).unwrap_or_else(|e| unreachable!("{e}"));
        for field in ["root_identity", "parent_identity", "identity"] {
            let dev = record[field][0].as_u64().unwrap_or_else(|| unreachable!());
            record[field][0] = (dev ^ by).into();
        }
        let ino = record["identity"][1]
            .as_u64()
            .unwrap_or_else(|| unreachable!());
        record["identity"][1] = (ino + inode_by).into();
        PreparedItem(serde_json::to_vec(&record).unwrap_or_default())
    }

    #[test]
    fn an_item_on_a_renumbered_volume_is_found_restored_and_finalized() {
        let (_fx, root, executor) = setup();
        let prepared = executor
            .prepare(&raw(&root), &relative())
            .unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(executor.quarantine(&prepared, op(7), b"{}"), Ok(()));
        // The volume was mounted again under another device number.
        let moved = renumbered(&prepared, 0x40, 0);
        assert_eq!(executor.locate(&moved, op(7)), Located::InStore);
        assert!(executor.held(&moved, op(7)).is_some());
        assert_eq!(
            executor.restore(&moved, op(7)),
            Ok(RestoredItem {
                parent_replaced: false,
                matches_record: true
            })
        );
        assert_eq!(
            std::fs::read(root.join("cache/app/data.bin")).unwrap_or_default(),
            b"0123456789"
        );
        assert!(executor.forget(&moved, op(7)), "restored: forgotten");

        let prepared = executor
            .prepare(&raw(&root), &relative())
            .unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(executor.quarantine(&prepared, op(8), b"{}"), Ok(()));
        let moved = renumbered(&prepared, 0x40, 0);
        assert_eq!(executor.finalize(&moved, op(8)), Ok(()));
        let left = std::fs::read_dir(root.join("Quarantine"))
            .unwrap_or_else(|e| unreachable!("{e}"))
            .count();
        assert_eq!(left, 0, "item and sidecar are gone");
    }

    #[test]
    fn a_renumbered_record_that_does_not_match_the_held_item_is_refused() {
        let (_fx, root, executor) = setup();
        let prepared = executor
            .prepare(&raw(&root), &relative())
            .unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(executor.quarantine(&prepared, op(9), b"{}"), Ok(()));
        let stored = root.join("Quarantine").join(op(9).to_string());
        // Another device number and another inode: not this item.
        let other = renumbered(&prepared, 0x40, 1);
        assert_eq!(executor.locate(&other, op(9)), Located::Unknown);
        assert_eq!(executor.held(&other, op(9)), None);
        assert_eq!(
            executor.restore(&other, op(9)),
            Err(RestoreFailure {
                reason: NotRestored::Failed,
                left: ItemLocation::Quarantine
            })
        );
        assert_eq!(executor.finalize(&other, op(9)), Err(Removal::Nothing));
        assert!(stored.exists(), "nothing moved or removed");

        // A record whose identities are not all on one device is not renumbered.
        let mut mixed: serde_json::Value =
            serde_json::from_slice(&prepared.0).unwrap_or_else(|e| unreachable!("{e}"));
        let dev = mixed["root_identity"][0]
            .as_u64()
            .unwrap_or_else(|| unreachable!());
        mixed["root_identity"][0] = (dev ^ 0x80).into();
        mixed["parent_identity"][0] = (dev ^ 0x40).into();
        mixed["identity"][0] = (dev ^ 0x40).into();
        let mixed = PreparedItem(serde_json::to_vec(&mixed).unwrap_or_default());
        assert_eq!(executor.locate(&mixed, op(9)), Located::Unknown);
        assert_eq!(executor.finalize(&mixed, op(9)), Err(Removal::Nothing));
        assert!(stored.exists(), "nothing moved or removed");
        assert_eq!(executor.locate(&prepared, op(9)), Located::InStore);
    }

    #[test]
    fn a_changed_item_is_left_at_its_source() {
        let (_fx, root, executor) = setup();
        let prepared = executor
            .prepare(&raw(&root), &relative())
            .unwrap_or_else(|e| unreachable!("{e}"));
        std::fs::write(root.join("cache/app/data.bin"), b"changed!!!!")
            .unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(
            executor.quarantine(&prepared, op(2), b"{}"),
            Err(ExecutionFailure {
                reason: QuarantineFailure::IdentityMismatch,
                left: ItemLocation::Source
            })
        );
        assert!(root.join("cache/app/data.bin").exists());
        assert_eq!(executor.locate(&prepared, op(2)), Located::Unknown);
    }

    #[test]
    fn links_and_unreadable_records_are_refused() {
        let (_fx, root, executor) = setup();
        std::os::unix::fs::symlink(root.join("cache/app"), root.join("cache/link"))
            .unwrap_or_else(|e| unreachable!("{e}"));
        let through_link = vec![b"cache".to_vec(), b"link".to_vec(), b"data.bin".to_vec()];
        assert!(executor.prepare(&raw(&root), &through_link).is_err());
        assert!(
            executor
                .prepare(&raw(&root), &[b"cache".to_vec(), b"link".to_vec()])
                .is_err()
        );
        assert_eq!(
            executor.locate(&PreparedItem(b"not json".to_vec()), op(3)),
            Located::Unknown
        );
        assert!(
            executor
                .quarantine(&PreparedItem(b"{}".to_vec()), op(3), b"{}")
                .is_err()
        );
    }
}
