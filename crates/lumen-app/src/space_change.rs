//! Observed volume free-space changes, not a claim that cleanup caused them.
//! Missing readings and changed directory identities remain unknown.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use lumen_application::ports::DirEnumerator as _;
use lumen_domain::{EntryKind, FileIdentity, RawPath, VolumeId};

/// Two readings of one volume around a confirmed operation. Other processes,
/// snapshots and delayed filesystem reclamation can affect the difference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpaceChange {
    /// The volume actually measured.
    pub volume: VolumeId,
    /// Bytes available to this user before the operation.
    pub before_available: u64,
    /// Bytes available at the subsequent reading (may be smaller).
    pub after_available: u64,
}

#[derive(Debug)]
struct Sample {
    identity: FileIdentity,
    available: u64,
}

fn sample(path: &Path) -> Option<Sample> {
    let enumerator = crate::scan::enumerator();
    let raw = crate::raw_path(path).ok()?;
    let entry = enumerator.stat(&raw).ok()?;
    if entry.kind != EntryKind::Directory {
        return None;
    }
    let available = crate::space::volume(path).ok()?.available;
    // A swapped directory or mount must not mix readings from different objects.
    let after = enumerator.stat(&raw).ok()?;
    (entry.identity() == after.identity()).then_some(Sample {
        identity: entry.identity().clone(),
        available,
    })
}

#[cfg(unix)]
fn native(path: &RawPath) -> Option<PathBuf> {
    use std::os::unix::ffi::OsStrExt as _;
    (path.flavor() == lumen_domain::PathFlavor::Unix)
        .then(|| PathBuf::from(std::ffi::OsStr::from_bytes(path.as_bytes())))
}

#[cfg(windows)]
fn native(path: &RawPath) -> Option<PathBuf> {
    use std::os::windows::ffi::OsStringExt as _;
    Some(PathBuf::from(std::ffi::OsString::from_wide(
        &path.to_windows_wide()?,
    )))
}

/// A parent directory that remains after its cleanup target is removed.
#[derive(Debug)]
pub struct SpaceWatch {
    path: PathBuf,
    before: Sample,
}

/// Watch every matched target volume once, with at most 64 target paths.
/// Partial coverage is unknown, never a measurement of the home disk instead.
pub fn watch_targets(targets: &[(RawPath, VolumeId)]) -> Option<Vec<SpaceWatch>> {
    if targets.is_empty() || targets.len() > 64 {
        return None;
    }
    let mut watches = BTreeMap::new();
    for (path, volume) in targets {
        let watch = SpaceWatch::parent(path, volume)?;
        watches.entry(volume.clone()).or_insert(watch);
    }
    Some(watches.into_values().collect())
}

impl SpaceWatch {
    /// Begin at the target's parent on its expected volume. A cache that is a
    /// mount root or an unavailable reading is not replaced by the home disk.
    pub fn parent(target: &RawPath, volume: &VolumeId) -> Option<Self> {
        let path = native(target)?.parent()?.to_owned();
        let before = sample(&path)?;
        (before.identity.volume == *volume).then_some(Self { path, before })
    }

    /// Read again immediately after the operation. Failure affects this optional
    /// observation only, never the operation's outcome.
    pub fn finish(self) -> Option<SpaceChange> {
        self.finish_sample(sample(&self.path)?)
    }

    fn finish_sample(&self, after: Sample) -> Option<SpaceChange> {
        if after.identity != self.before.identity {
            return None;
        }
        tracing::info!(
            before_available = self.before.available,
            after_available = after.available,
            "volume free space observed"
        );
        Some(SpaceChange {
            volume: after.identity.volume,
            before_available: self.before.available,
            after_available: after.available,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumen_domain::FileId;
    use lumen_testkit::{Fixture, Node};

    #[test]
    fn reads_the_fixture_volume_and_refuses_a_different_one()
    -> Result<(), Box<dyn std::error::Error>> {
        let fx = Fixture::build(&[Node::Dir {
            path: "cache".into(),
        }])?;
        let target = crate::raw_path(&fx.path("cache")?)?;
        let before = sample(fx.root()).ok_or("fixture volume is unreadable")?;
        let watch = SpaceWatch::parent(&target, &before.identity.volume)
            .ok_or("fixture watch is unavailable")?;
        let change = watch.finish().ok_or("fixture volume changed")?;
        assert_eq!(change.volume, before.identity.volume);
        assert!(SpaceWatch::parent(&target, &VolumeId::new("different-volume")?).is_none());
        let pair = (target.clone(), before.identity.volume.clone());
        assert_eq!(
            watch_targets(&[pair.clone(), pair])
                .ok_or("watch failed")?
                .len(),
            1
        );
        assert!(
            watch_targets(&[
                (target.clone(), before.identity.volume),
                (target, VolumeId::new("different-volume")?),
            ])
            .is_none(),
            "partial measurement coverage must remain unknown"
        );
        Ok(())
    }

    #[test]
    fn changed_directory_is_unknown_and_negative_changes_are_preserved()
    -> Result<(), Box<dyn std::error::Error>> {
        let identity = FileIdentity {
            volume: VolumeId::new("volume")?,
            file: FileId::new(1),
        };
        let watch = SpaceWatch {
            path: PathBuf::new(),
            before: Sample {
                identity: identity.clone(),
                available: 1000,
            },
        };
        let change = watch
            .finish_sample(Sample {
                identity: identity.clone(),
                available: 500,
            })
            .ok_or("same volume should be comparable")?;
        assert_eq!(
            (change.before_available, change.after_available),
            (1000, 500)
        );
        let swapped = FileIdentity {
            file: FileId::new(2),
            ..identity
        };
        assert!(
            watch
                .finish_sample(Sample {
                    identity: swapped,
                    available: 9000
                })
                .is_none()
        );
        Ok(())
    }
}
