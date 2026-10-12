//! The single writer for `index.db` (ADR-0012).
//!
//! SQLite allows one writer at a time, so scan results go through one dedicated
//! thread that owns the connection and commits in batches. Producers send through
//! a bounded channel and block when the writer falls behind (backpressure).
//!
//! Failure semantics:
//!
//! - the first write error stops the writer; later sends fail with
//!   [`StoreError::WriterStopped`] and [`IndexWriter::finish`] returns the
//!   original error;
//! - dropping the writer without `finish` waits until everything already sent is
//!   flushed, but leaves the scan in the `running` state, so it can be recognised
//!   as unfinished and resumed or discarded later. Errors on that path cannot be
//!   reported; call `finish` to observe them.

use std::num::NonZeroUsize;
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::thread::JoinHandle;

use lumen_application::ports::DirOutcome;
use lumen_domain::{FilesystemEntry, RawPath, ScanId, ScanState, Timestamp};

use crate::{IndexStore, StoreError};

/// Writer settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriterConfig {
    /// Records per transaction.
    pub batch_size: NonZeroUsize,
    /// Pending records the channel holds before producers block.
    pub channel_capacity: NonZeroUsize,
}

impl Default for WriterConfig {
    fn default() -> Self {
        Self {
            batch_size: NonZeroUsize::new(2048).unwrap_or(NonZeroUsize::MIN),
            channel_capacity: NonZeroUsize::new(8192).unwrap_or(NonZeroUsize::MIN),
        }
    }
}

/// What the writer stored.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WriteStats {
    /// Entries stored.
    pub entries: u64,
    /// Directory listings stored.
    pub listings: u64,
    /// Transactions committed.
    pub batches: u64,
}

enum Command {
    Entry(Box<FilesystemEntry>),
    Listing(RawPath, DirOutcome),
    Finish(ScanState, Timestamp),
}

/// Handle to the writer thread for one scan.
#[derive(Debug)]
pub struct IndexWriter {
    /// `None` only after `finish` or during `drop`.
    sender: Option<SyncSender<Command>>,
    thread: Option<JoinHandle<Result<(IndexStore, WriteStats), StoreError>>>,
}

impl Drop for IndexWriter {
    fn drop(&mut self) {
        // Closing the channel lets the thread flush what it has and exit; waiting
        // makes "everything sent is stored" hold when drop returns.
        drop(self.sender.take());
        if let Some(thread) = self.thread.take() {
            let flushed = thread.join();
            debug_assert!(flushed.is_ok(), "index writer panicked during drop");
        }
    }
}

impl IndexWriter {
    /// Records the scan start, then starts the writer thread.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] if the scan cannot be recorded or the thread
    /// cannot be spawned.
    pub fn start(
        mut store: IndexStore,
        scan: ScanId,
        started_at: Timestamp,
        roots: &[RawPath],
        config: WriterConfig,
    ) -> Result<Self, StoreError> {
        store.begin_scan(scan, started_at, roots)?;
        let (sender, receiver) = sync_channel(config.channel_capacity.get());
        let thread = std::thread::Builder::new()
            .name("lumen-index-writer".into())
            .spawn(move || run(store, scan, &receiver, config.batch_size.get()))
            .map_err(|_| StoreError::WriterStopped)?;
        Ok(Self {
            sender: Some(sender),
            thread: Some(thread),
        })
    }

    fn send(&self, command: Command) -> Result<(), StoreError> {
        let sender = self.sender.as_ref().ok_or(StoreError::WriterStopped)?;
        sender.send(command).map_err(|_| StoreError::WriterStopped)
    }

    /// Queues an entry; blocks while the writer is behind.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::WriterStopped`] if the writer has stopped after an
    /// error; [`IndexWriter::finish`] then reports that error.
    pub fn entry(&self, entry: FilesystemEntry) -> Result<(), StoreError> {
        self.send(Command::Entry(Box::new(entry)))
    }

    /// Queues a directory listing outcome; blocks while the writer is behind.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::WriterStopped`] if the writer has stopped.
    pub fn listing(&self, path: RawPath, outcome: DirOutcome) -> Result<(), StoreError> {
        self.send(Command::Listing(path, outcome))
    }

    /// Flushes everything, records how the scan ended, and returns the store.
    ///
    /// # Errors
    ///
    /// Returns the first error the writer hit, or [`StoreError::WriterPanicked`].
    pub fn finish(
        mut self,
        state: ScanState,
        finished_at: Timestamp,
    ) -> Result<(IndexStore, WriteStats), StoreError> {
        // If the writer already stopped, the send fails and the join reports why.
        let _sent = self.send(Command::Finish(state, finished_at));
        drop(self.sender.take());
        let thread = self.thread.take().ok_or(StoreError::WriterStopped)?;
        thread.join().map_err(|_| StoreError::WriterPanicked)?
    }
}

fn run(
    mut store: IndexStore,
    scan: ScanId,
    receiver: &Receiver<Command>,
    batch_size: usize,
) -> Result<(IndexStore, WriteStats), StoreError> {
    let mut stats = WriteStats::default();
    let mut entries = Vec::with_capacity(batch_size);
    let mut listings = Vec::new();
    let mut finish = None;

    // `recv` ends when every sender is gone (finish or drop).
    for command in receiver {
        match command {
            Command::Entry(entry) => entries.push(*entry),
            Command::Listing(path, outcome) => listings.push((path, outcome)),
            Command::Finish(end_state, finished_at) => {
                finish = Some((end_state, finished_at));
                break;
            }
        }
        if entries.len() + listings.len() >= batch_size {
            flush(&mut store, scan, &mut entries, &mut listings, &mut stats)?;
        }
    }
    flush(&mut store, scan, &mut entries, &mut listings, &mut stats)?;
    if let Some((end_state, finished_at)) = finish {
        store.finish_scan(scan, end_state, finished_at)?;
    }
    Ok((store, stats))
}

fn flush(
    store: &mut IndexStore,
    scan: ScanId,
    entries: &mut Vec<FilesystemEntry>,
    listings: &mut Vec<(RawPath, DirOutcome)>,
    stats: &mut WriteStats,
) -> Result<(), StoreError> {
    if entries.is_empty() && listings.is_empty() {
        return Ok(());
    }
    if !entries.is_empty() {
        store.insert_entries(scan, entries)?;
        stats.entries += u64::try_from(entries.len()).unwrap_or(u64::MAX);
        stats.batches += 1;
        entries.clear();
    }
    if !listings.is_empty() {
        store.insert_listings(scan, listings)?;
        stats.listings += u64::try_from(listings.len()).unwrap_or(u64::MAX);
        stats.batches += 1;
        listings.clear();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use lumen_domain::{
        AccessState, ByteCount, EntryKind, EntryTimes, FileId, FileIdentity, Protection, SizeFacts,
        SizeFlags, VolumeId,
    };
    use lumen_testkit::Fixture;

    use super::*;
    use crate::open_index;

    fn scan() -> ScanId {
        "0190f1c2-7a3b-7c4d-8e5f-6a7b8c9d0e1f"
            .parse()
            .unwrap_or_else(|_| unreachable!())
    }

    fn entry(n: u64, allocated: u64) -> FilesystemEntry {
        FilesystemEntry {
            path: RawPath::from_unix_bytes(format!("/r/{n}").into_bytes())
                .unwrap_or_else(|_| unreachable!()),
            kind: EntryKind::File,
            size: SizeFacts {
                identity: FileIdentity {
                    volume: VolumeId::new("v").unwrap_or_else(|_| unreachable!()),
                    file: FileId::new(n.into()),
                },
                logical: ByteCount::new(1),
                allocated: ByteCount::new(allocated),
                private: None,
                clone_id: None,
                link_count: NonZeroU32::MIN,
                flags: SizeFlags::default(),
            },
            times: EntryTimes::default(),
            protection: Protection::default(),
            access: AccessState::Readable,
        }
    }

    fn config(batch: usize) -> WriterConfig {
        WriterConfig {
            batch_size: NonZeroUsize::new(batch).unwrap_or(NonZeroUsize::MIN),
            channel_capacity: NonZeroUsize::new(4).unwrap_or(NonZeroUsize::MIN),
        }
    }

    fn open(fx: &Fixture) -> Result<IndexStore, Box<dyn std::error::Error>> {
        Ok(IndexStore::new(open_index(&fx.path("index.db")?)?))
    }

    #[test]
    fn writes_in_batches_and_records_the_end() -> Result<(), Box<dyn std::error::Error>> {
        let fx = Fixture::build(&[])?;
        let writer = IndexWriter::start(open(&fx)?, scan(), Timestamp::UNIX_EPOCH, &[], config(7))?;
        for n in 0..50 {
            writer.entry(entry(n, 4096))?;
        }
        writer.listing(
            RawPath::from_unix_bytes(b"/r".to_vec())?,
            DirOutcome {
                access: AccessState::Readable,
                complete: true,
            },
        )?;
        let (store, stats) =
            writer.finish(ScanState::Completed, "2026-10-12T00:00:00Z".parse()?)?;
        assert_eq!((stats.entries, stats.listings), (50, 1));
        assert!(stats.batches >= 7, "50 records in batches of 7: {stats:?}");
        assert_eq!(store.entries(scan())?.len(), 50);
        assert_eq!(store.scan_state(scan())?, Some(ScanState::Completed));
        Ok(())
    }

    #[test]
    fn a_write_error_stops_the_writer_and_is_reported_by_finish()
    -> Result<(), Box<dyn std::error::Error>> {
        let fx = Fixture::build(&[])?;
        let writer = IndexWriter::start(open(&fx)?, scan(), Timestamp::UNIX_EPOCH, &[], config(1))?;
        writer.entry(entry(1, u64::MAX))?; // out of SQLite's range
        // Later sends either fail (writer already stopped) or are discarded.
        let mut stopped = false;
        for n in 2..200 {
            if matches!(writer.entry(entry(n, 1)), Err(StoreError::WriterStopped)) {
                stopped = true;
                break;
            }
        }
        assert!(stopped, "producers learn that the writer stopped");
        assert!(matches!(
            writer.finish(ScanState::Completed, Timestamp::UNIX_EPOCH),
            Err(StoreError::OutOfRange {
                column: "allocated"
            })
        ));
        let store = open(&fx)?;
        assert_eq!(
            store.scan_state(scan())?,
            Some(ScanState::Running),
            "a failed scan is never marked completed"
        );
        Ok(())
    }

    #[test]
    fn dropping_without_finish_flushes_but_leaves_the_scan_running()
    -> Result<(), Box<dyn std::error::Error>> {
        let fx = Fixture::build(&[])?;
        {
            let writer =
                IndexWriter::start(open(&fx)?, scan(), Timestamp::UNIX_EPOCH, &[], config(100))?;
            for n in 0..10 {
                writer.entry(entry(n, 1))?;
            }
        } // dropped: simulates an interrupted scan
        let store = open(&fx)?;
        assert_eq!(
            store.entries(scan())?.len(),
            10,
            "everything sent was flushed"
        );
        assert_eq!(
            store.scan_state(scan())?,
            Some(ScanState::Running),
            "recognisable as unfinished"
        );
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn stores_a_real_scan_end_to_end() -> Result<(), Box<dyn std::error::Error>> {
        use std::ops::ControlFlow;
        use std::os::unix::ffi::OsStrExt as _;

        use lumen_application::CancelToken;
        use lumen_scan::{ScanConfig, ScanItem, StdFsEnumerator, scan as run_scan};
        use lumen_testkit::Node;

        let fx = Fixture::build(&[
            Node::File {
                path: "root/a/one".into(),
                contents: b"1".to_vec(),
            },
            Node::File {
                path: "root/a/b/two".into(),
                contents: b"22".to_vec(),
            },
            Node::Symlink {
                path: "root/link".into(),
                target: "a".into(),
            },
        ])?;
        let root = RawPath::from_unix_bytes(fx.path("root")?.as_os_str().as_bytes().to_vec())?;
        let writer = IndexWriter::start(
            open(&fx)?,
            scan(),
            Timestamp::UNIX_EPOCH,
            std::slice::from_ref(&root),
            WriterConfig::default(),
        )?;
        let mut write_error = None;
        let summary = run_scan(
            &StdFsEnumerator,
            &[root],
            &ScanConfig::default(),
            &CancelToken::new(),
            |item| {
                let result = match item {
                    ScanItem::Entry(e) => writer.entry(e),
                    ScanItem::Listed { path, outcome } => writer.listing(path, outcome),
                    _ => Ok(()),
                };
                match result {
                    Ok(()) => ControlFlow::Continue(()),
                    Err(e) => {
                        write_error = Some(e);
                        ControlFlow::Break(())
                    }
                }
            },
        );
        assert!(write_error.is_none());
        let (store, stats) = writer.finish(ScanState::Completed, Timestamp::UNIX_EPOCH)?;
        assert_eq!(stats.entries, summary.totals.entries);
        assert_eq!(stats.listings, summary.totals.directories);
        assert_eq!(
            u64::try_from(store.entries(scan())?.len())?,
            summary.totals.entries
        );
        Ok(())
    }
}
