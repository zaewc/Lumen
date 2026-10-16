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
    FileId, FileIdentity, ItemLocation, OperationId, QuarantineFailure, RawPath, VolumeId,
};
use rustix::fs::{AtFlags, FileType, Mode, OFlags, fstat, open, openat, statat};
use serde::{Deserialize, Serialize};

use crate::finalize::{Removed, finalize};
use crate::move_into::{Left, MoveError, move_into};
use crate::restore::{RestoreError, restore};
use crate::sidecar::write_sidecar;
use crate::store::{Store, open_store};
use crate::verify::{ExpectedLeaf, LeafKind, PlanItem, UnixIdentity, check_held, identity, verify};

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
}

fn pair(id: UnixIdentity) -> (u64, u64) {
    (id.dev, id.ino)
}

fn unpair((dev, ino): (u64, u64)) -> UnixIdentity {
    UnixIdentity { dev, ino }
}

impl Prepared {
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
        let names: Vec<&[u8]> = self.store_names.iter().map(Vec::as_slice).collect();
        open_store(&self.store_base, &names, volume).ok()
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
            .store(plan.expected.identity.dev)
            .ok_or_else(|| at_source(QuarantineFailure::VolumeUnsupported))?;
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
        let plan = prepared.plan();
        if verify(&plan).is_ok() {
            return Located::AtSource;
        }
        let Some(store) = self.store(plan.expected.identity.dev) else {
            return Located::Unknown;
        };
        let name = operation.to_string();
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
        let plan = parse(item).ok_or(in_store)?.plan();
        let store = self.store(plan.expected.identity.dev).ok_or(in_store)?;
        let name = operation.to_string();
        restore(&store, name.as_bytes(), &plan)
            .map(|restored| RestoredItem {
                parent_replaced: restored.parent_replaced,
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
        let plan = parse(item).ok_or(Removal::Nothing)?.plan();
        let store = self
            .store(plan.expected.identity.dev)
            .ok_or(Removal::Nothing)?;
        let name = operation.to_string();
        finalize(&store, name.as_bytes(), &plan.expected).map_err(|error| match error.removed() {
            Removed::Nothing => Removal::Nothing,
            Removed::Partly => Removal::Partly,
            Removed::Item => Removal::Item,
        })
    }

    fn held(&self, item: &PreparedItem, operation: OperationId) -> Option<(RawPath, FileIdentity)> {
        let plan = parse(item)?.plan();
        let store = self.store(plan.expected.identity.dev)?;
        let name = operation.to_string();
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
                parent_replaced: false
            })
        );
        assert_eq!(executor.locate(&prepared, op(3)), Located::AtSource);
        assert_eq!(
            std::fs::read(root.join("cache/app/data.bin")).unwrap_or_default(),
            b"0123456789"
        );
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
