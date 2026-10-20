//! Storage per entry for a synthetic, realistically shaped scan (roadmap 13.c).
//!
//! Ignored by default (it writes up to 120 MB to a temporary fixture); run with
//! `cargo nextest run -p lumen-store-sqlite --run-ignored only bytes_per_entry
//! --no-capture`.

use std::fmt::Write as _;
use std::num::NonZeroU32;

use lumen_application::ports::DirOutcome;
use lumen_domain::{
    AccessState, ByteCount, EntryKind, EntryTimes, FileId, FileIdentity, FilesystemEntry,
    Protection, RawPath, ScanId, SizeFacts, SizeFlags, Timestamp, VolumeId,
};
use lumen_testkit::Fixture;

use super::IndexStore;
use crate::open_index;

/// A deterministic xorshift generator, so every run stores the same tree.
struct Rng(u64);

impl Rng {
    fn next(&mut self, below: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % below
    }

    fn name(&mut self, min: u64, spread: u64) -> Vec<u8> {
        let len = min + self.next(spread);
        (0..len)
            .map(|_| {
                b"abcdefghijklmnopqrstuvwxyz0123456789._-"
                    [usize::try_from(self.next(39)).unwrap_or(0)]
            })
            .collect()
    }
}

fn entry(
    path: Vec<u8>,
    kind: EntryKind,
    file: u128,
) -> Result<FilesystemEntry, Box<dyn std::error::Error>> {
    Ok(FilesystemEntry {
        path: RawPath::from_unix_bytes(path)?,
        kind,
        size: SizeFacts {
            identity: FileIdentity {
                volume: VolumeId::new("dev:16777232")?,
                file: FileId::new(file),
            },
            logical: ByteCount::new(u64::try_from(file % 100_000)?),
            allocated: ByteCount::new(4096),
            private: None,
            clone_id: None,
            link_count: NonZeroU32::MIN,
            flags: SizeFlags::default(),
        },
        times: EntryTimes {
            modified: Some(Timestamp::from_unix(
                1_700_000_000 + i64::try_from(file)?,
                123_456_789,
            )?),
            accessed: Some(Timestamp::from_unix(1_700_000_100, 1)?),
            changed: Some(Timestamp::from_unix(1_700_000_200, 2)?),
            created: Some(Timestamp::from_unix(1_600_000_000, 3)?),
        },
        protection: Protection::default(),
        access: AccessState::Readable,
    })
}

/// About `total` entries: a random walk keeps directories 9 to 15 levels
/// deep, each holding 6 to 9 files (about 12% directories).
fn synthetic(
    total: usize,
) -> Result<(Vec<FilesystemEntry>, Vec<RawPath>), Box<dyn std::error::Error>> {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut stack: Vec<Vec<u8>> = vec![b"/Users/ana".to_vec()];
    let mut entries = Vec::with_capacity(total);
    let mut dirs = Vec::new();
    let mut file = 0u128;
    while entries.len() < total {
        let depth = stack.len();
        if depth > 15 || (depth > 9 && rng.next(2) == 0) {
            stack.pop();
            continue;
        }
        let parent = stack.last().cloned().unwrap_or_default();
        let mut dir = parent;
        dir.push(b'/');
        dir.extend(rng.name(9, 16));
        file += 1;
        entries.push(entry(dir.clone(), EntryKind::Directory, file)?);
        dirs.push(RawPath::from_unix_bytes(dir.clone())?);
        for _ in 0..6 + rng.next(4) {
            let mut path = dir.clone();
            path.push(b'/');
            path.extend(rng.name(12, 24));
            file += 1;
            entries.push(entry(path, EntryKind::File, file)?);
        }
        stack.push(dir);
    }
    Ok((entries, dirs))
}

#[test]
#[ignore = "writes up to 120 MB; run explicitly to measure"]
fn bytes_per_entry() -> Result<(), Box<dyn std::error::Error>> {
    let (entries, dirs) = synthetic(200_000)?;
    let path_bytes: usize = entries.iter().map(|e| e.path.as_bytes().len()).sum();
    let fx = Fixture::build(&[])?;
    let mut store = IndexStore::new(open_index(&fx.path("index.db")?)?);
    let scan: ScanId = "0190f1c2-7a3b-7c4d-8e5f-6a7b8c9d0e1f".parse()?;
    store.begin_scan(scan, Timestamp::UNIX_EPOCH, &[])?;
    for batch in entries.chunks(2048) {
        store.insert_entries(scan, batch)?;
    }
    let readable = DirOutcome {
        access: AccessState::Readable,
        complete: true,
    };
    let listings: Vec<(RawPath, DirOutcome)> = dirs.into_iter().map(|d| (d, readable)).collect();
    for batch in listings.chunks(2048) {
        store.insert_listings(scan, batch)?;
    }
    store
        .conn
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")?;
    let pages: i64 = store
        .conn
        .query_row("PRAGMA page_count", [], |r| r.get(0))?;
    let free: i64 = store
        .conn
        .query_row("PRAGMA freelist_count", [], |r| r.get(0))?;
    let page_size: i64 = store.conn.query_row("PRAGMA page_size", [], |r| r.get(0))?;
    let used = (pages - free) * page_size;
    let count = i64::try_from(entries.len())?;
    let mut report = format!(
        "{count} entries ({} directories), average path {} B: {used} B used, {} B per entry",
        listings.len(),
        path_bytes / entries.len(),
        used / count
    );
    if let Ok(mut by_table) = store
        .conn
        .prepare("SELECT name, sum(pgsize) FROM dbstat GROUP BY name ORDER BY 2 DESC")
    {
        let rows = by_table.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
        for row in rows {
            let (name, bytes) = row?;
            write!(
                report,
                "\n  {name}: {bytes} B ({} B per entry)",
                bytes / count
            )?;
        }
    }
    // The measurement is the point of this ignored test.
    #[allow(clippy::print_stdout)]
    {
        println!("{report}");
    }
    Ok(())
}
