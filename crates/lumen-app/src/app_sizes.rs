//! Installed apps' sizes from the scan that just ran (roadmap 16.2): what each
//! app's folder held in the latest completed scan, read from `index.db`, with
//! no new reads of the disk. Display only: the move preview measures the app
//! itself again and stays the authority (ADR-0062).

use std::path::Path;

use lumen_domain::{ByteCount, RawPath};
use lumen_store_sqlite::{IndexStore, StoreError, open_index};

/// What the latest scan says about one app's size. Never zero for unknown
/// (ADR-0008).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanSize {
    /// Every folder in the app was read and every file's private size is
    /// known: what removing it would free.
    Complete(ByteCount),
    /// Some folder in it could not be read, or some file may share blocks
    /// with another (a clone, or a filesystem that cannot say): at least this
    /// much.
    AtLeast(ByteCount),
    /// The scan did not cover it (outside its folders, or newer than it), or
    /// there is no completed scan.
    NotScanned,
}

/// The size of each of `locations` in the latest completed scan in `db`, in
/// the same order.
///
/// # Errors
///
/// [`StoreError`] if the index cannot be read.
pub fn scan_sizes(db: &Path, locations: &[RawPath]) -> Result<Vec<ScanSize>, StoreError> {
    if locations.is_empty() {
        return Ok(Vec::new());
    }
    let store = IndexStore::new(open_index(db)?);
    let Some(scan) = store.latest_completed_scan()? else {
        return Ok(vec![ScanSize::NotScanned; locations.len()]);
    };
    locations
        .iter()
        .map(|location| {
            Ok(match store.subtree_size(scan, location)? {
                None => ScanSize::NotScanned,
                Some(size) if size.complete && size.reclaim.exact => {
                    ScanSize::Complete(size.reclaim.reclaimable_now)
                }
                Some(size) => ScanSize::AtLeast(size.reclaim.reclaimable_now),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use lumen_application::CancelToken;
    use lumen_testkit::{Fixture, Node};

    use super::*;

    #[test]
    fn apps_get_the_size_the_scan_saw() -> Result<(), Box<dyn std::error::Error>> {
        let fx = Fixture::build(&[
            Node::File {
                path: "Applications/Big.app/Contents/MacOS/big".into(),
                contents: vec![1; 20_000],
            },
            Node::File {
                path: "Applications/Small.app/Contents/Info.plist".into(),
                contents: vec![2; 10],
            },
        ])?;
        let db = fx.path("index.db")?;
        let none = scan_sizes(&db, &[crate::raw_path(&fx.path("Applications/Big.app")?)?])?;
        assert_eq!(none, [ScanSize::NotScanned], "no completed scan yet");
        let apps = std::fs::canonicalize(fx.path("Applications")?)?;
        crate::scan(
            std::slice::from_ref(&apps),
            Some(&db),
            &crate::SystemClock,
            &crate::UuidV7Ids,
            &CancelToken::new(),
            &mut |_| {},
        )?;
        let sizes = scan_sizes(
            &db,
            &[
                crate::raw_path(&apps.join("Big.app"))?,
                crate::raw_path(&apps.join("Small.app"))?,
                crate::raw_path(&apps.join("Gone.app"))?,
            ],
        )?;
        // APFS reports each file's private size, so the size is exact there;
        // elsewhere a file may share blocks and the size is a lower bound.
        for (size, least) in [(sizes[0], 20_000), (sizes[1], 10)] {
            match size {
                ScanSize::Complete(bytes) if bytes.get() >= least => {}
                ScanSize::AtLeast(_) if !cfg!(target_os = "macos") => {}
                _ => return Err(format!("unexpected sizes: {sizes:?}").into()),
            }
        }
        if let (ScanSize::Complete(big), ScanSize::Complete(small)) = (sizes[0], sizes[1]) {
            assert!(big > small, "{sizes:?}");
        }
        assert_eq!(sizes[2], ScanSize::NotScanned, "not in the scan");
        Ok(())
    }
}
