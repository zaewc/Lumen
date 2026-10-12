//! Parallel breadth-first traversal (ADR-0017).
//!
//! Workers take one directory at a time from a shared queue, list it through a
//! [`DirEnumerator`], queue its subdirectories, and stream results to the caller
//! through a **bounded** channel, so a slow consumer slows the workers instead of
//! growing memory. Mount points are not crossed unless configured.
//!
//! Cancellation is cooperative: workers check the [`CancelToken`] before each
//! directory and on every entry, and the calling thread polls it while waiting for
//! results, so cancellation takes effect even while workers are blocked in a slow
//! listing. Progress snapshots are emitted on the calling thread.
//!
//! Scoped `std` threads are used rather than a work-stealing crate: work items are
//! whole directories, so a shared queue gives the same parallelism without a
//! dependency, and it allows bounded backpressure. Benchmarks (roadmap 5.19)
//! revisit this.

use std::collections::{HashSet, VecDeque};
use std::num::NonZeroUsize;
use std::ops::ControlFlow;
use std::sync::mpsc::{RecvTimeoutError, SyncSender, sync_channel};
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use lumen_application::CancelToken;
use lumen_application::ports::{DirEnumerator, DirOutcome, EnumerateError, PendingDirectory};
use lumen_domain::{ByteCount, EntryKind, FileIdentity, FilesystemEntry, RawPath};

/// How often the calling thread re-checks cancellation while no results arrive.
const CANCEL_POLL: Duration = Duration::from_millis(50);

/// Scheduler settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanConfig {
    /// Worker threads.
    pub workers: NonZeroUsize,
    /// Capacity of the result channel (backpressure bound).
    pub channel_capacity: NonZeroUsize,
    /// Whether to descend into directories on other volumes (mount points).
    pub cross_devices: bool,
    /// Emit a [`ScanItem::Progress`] after this many results.
    pub progress_every: NonZeroUsize,
}

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            workers: std::thread::available_parallelism().unwrap_or(NonZeroUsize::MIN),
            channel_capacity: NonZeroUsize::new(4096).unwrap_or(NonZeroUsize::MIN),
            cross_devices: false,
            progress_every: NonZeroUsize::new(1000).unwrap_or(NonZeroUsize::MIN),
        }
    }
}

/// Why a path was not listed.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SkipReason {
    /// The directory is on another volume and `cross_devices` is off.
    MountPoint,
    /// The directory was already reached through another path: a macOS firmlink
    /// (`/Users` and `/System/Volumes/Data/Users` share one identity), a bind
    /// mount, a directory hard link, or an overlapping root. Each directory is
    /// listed once, under the path that reached it first; this also ends loops.
    Alias,
    /// The enumerator refused the path (identity changed, not a directory,
    /// unobservable, unsupported).
    Enumerate(EnumerateError),
}

/// Running totals.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScanProgress {
    /// Entries delivered (including roots).
    pub entries: u64,
    /// Directories listed.
    pub directories: u64,
    /// Paths skipped.
    pub skipped: u64,
    /// Sum of logical sizes of regular files delivered so far (hard links counted
    /// per entry; use `ReclaimEstimate` for reclaim figures).
    pub file_bytes: ByteCount,
}

/// One result streamed to the consumer.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ScanItem {
    /// A filesystem entry: each root, and every entry found while listing.
    Entry(FilesystemEntry),
    /// A directory was listed (completely or not; see the outcome).
    Listed {
        /// The directory.
        path: RawPath,
        /// How the listing ended.
        outcome: DirOutcome,
    },
    /// A path was not listed.
    Skipped {
        /// The path.
        path: RawPath,
        /// Why.
        reason: SkipReason,
    },
    /// Totals so far; emitted periodically and once at the end.
    Progress(ScanProgress),
}

/// Why a traversal ended before visiting everything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    /// The consumer returned [`ControlFlow::Break`].
    Consumer,
    /// The cancellation token was cancelled.
    Cancelled,
}

/// Result of a traversal.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScanSummary {
    /// Final totals.
    pub totals: ScanProgress,
    /// Why the traversal stopped early, if it did. Results are then partial.
    pub stop: Option<StopReason>,
}

#[derive(Default)]
struct Queue {
    pending: VecDeque<PendingDirectory>,
    /// Identities of every directory ever queued, so none is listed twice.
    claimed: HashSet<FileIdentity>,
    in_flight: usize,
    stopped: bool,
}

struct Shared<'a> {
    queue: Mutex<Queue>,
    changed: Condvar,
    cancel: &'a CancelToken,
}

impl Shared<'_> {
    fn lock(&self) -> MutexGuard<'_, Queue> {
        // A worker that panicked cannot leave the queue logically inconsistent
        // (every mutation is a single push/pop/counter update), so recover.
        self.queue.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Records that `identity` will be listed. Returns `false` if it already was
    /// queued under another path.
    fn claim(&self, identity: &FileIdentity) -> bool {
        self.lock().claimed.insert(identity.clone())
    }

    fn stop(&self) {
        self.lock().stopped = true;
        self.changed.notify_all();
    }

    /// Takes the next directory, waiting while others may still produce work.
    /// Returns `None` when the traversal is finished, stopped or cancelled.
    fn take(&self) -> Option<PendingDirectory> {
        let mut queue = self.lock();
        loop {
            if queue.stopped || self.cancel.is_cancelled() {
                return None;
            }
            if let Some(next) = queue.pending.pop_front() {
                queue.in_flight += 1;
                return Some(next);
            }
            if queue.in_flight == 0 {
                return None;
            }
            // Bounded wait so cancellation is noticed without a notification.
            queue = self
                .changed
                .wait_timeout(queue, CANCEL_POLL)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }

    fn finish(&self, children: Vec<PendingDirectory>) {
        let mut queue = self.lock();
        queue.pending.extend(children);
        queue.in_flight -= 1;
        drop(queue);
        self.changed.notify_all();
    }
}

/// Consumer-side bookkeeping: totals, progress cadence and stop handling.
struct Delivery<'c, C> {
    consume: &'c mut C,
    totals: ScanProgress,
    since_progress: usize,
    every: usize,
}

impl<C: FnMut(ScanItem) -> ControlFlow<()>> Delivery<'_, C> {
    fn deliver(&mut self, item: ScanItem) -> ControlFlow<()> {
        match &item {
            ScanItem::Entry(entry) => {
                self.totals.entries += 1;
                if entry.kind == EntryKind::File {
                    self.totals.file_bytes =
                        self.totals.file_bytes.saturating_add(entry.size.logical);
                }
            }
            ScanItem::Listed { .. } => self.totals.directories += 1,
            ScanItem::Skipped { .. } => self.totals.skipped += 1,
            ScanItem::Progress(_) => {}
        }
        (self.consume)(item)?;
        self.since_progress += 1;
        if self.since_progress >= self.every {
            self.since_progress = 0;
            (self.consume)(ScanItem::Progress(self.totals))?;
        }
        ControlFlow::Continue(())
    }
}

/// Traverses `roots` breadth-first and streams results to `consume` on the
/// calling thread. The traversal stops early when `consume` returns
/// [`ControlFlow::Break`] or `cancel` is cancelled; a final
/// [`ScanItem::Progress`] is delivered unless the consumer stopped it.
pub fn scan<E: DirEnumerator, C: FnMut(ScanItem) -> ControlFlow<()>>(
    enumerator: &E,
    roots: &[RawPath],
    config: &ScanConfig,
    cancel: &CancelToken,
    mut consume: C,
) -> ScanSummary {
    let shared = Shared {
        queue: Mutex::new(Queue::default()),
        changed: Condvar::new(),
        cancel,
    };
    let mut delivery = Delivery {
        consume: &mut consume,
        totals: ScanProgress::default(),
        since_progress: 0,
        every: config.progress_every.get(),
    };
    let mut stop = None;

    // Roots are examined on the calling thread so their entries arrive first.
    'roots: for root in roots {
        if cancel.is_cancelled() {
            stop = Some(StopReason::Cancelled);
            break;
        }
        let items = match enumerator.stat(root) {
            Ok(entry) => {
                let mut alias = None;
                if entry.kind == EntryKind::Directory {
                    let identity = entry.size.identity.clone();
                    if shared.claim(&identity) {
                        shared.lock().pending.push_back(PendingDirectory {
                            path: root.clone(),
                            root_volume: identity.volume.clone(),
                            identity,
                        });
                    } else {
                        alias = Some(ScanItem::Skipped {
                            path: root.clone(),
                            reason: SkipReason::Alias,
                        });
                    }
                }
                [Some(ScanItem::Entry(entry)), alias]
            }
            Err(error) => [
                Some(ScanItem::Skipped {
                    path: root.clone(),
                    reason: SkipReason::Enumerate(error),
                }),
                None,
            ],
        };
        for item in items.into_iter().flatten() {
            if delivery.deliver(item).is_break() {
                stop = Some(StopReason::Consumer);
                break 'roots;
            }
        }
    }

    if stop.is_none() {
        stop = run_workers(enumerator, &shared, config, &mut delivery);
    }
    finish(&mut delivery, stop)
}

/// Continues an interrupted traversal from its frontier: directories that were
/// discovered but never listed (see the store's `frontier`). Each directory is
/// listed with its discovered identity, so one replaced while Lumen was not
/// running is reported as `IdentityChanged` instead of being listed. Nothing is
/// re-reported for roots; results stream exactly as from [`scan`].
pub fn resume<E: DirEnumerator, C: FnMut(ScanItem) -> ControlFlow<()>>(
    enumerator: &E,
    frontier: &[PendingDirectory],
    config: &ScanConfig,
    cancel: &CancelToken,
    mut consume: C,
) -> ScanSummary {
    let shared = Shared {
        queue: Mutex::new(Queue::default()),
        changed: Condvar::new(),
        cancel,
    };
    {
        let mut queue = shared.lock();
        for dir in frontier {
            if queue.claimed.insert(dir.identity.clone()) {
                queue.pending.push_back(dir.clone());
            }
        }
    }
    let mut delivery = Delivery {
        consume: &mut consume,
        totals: ScanProgress::default(),
        since_progress: 0,
        every: config.progress_every.get(),
    };
    let stop = if cancel.is_cancelled() {
        Some(StopReason::Cancelled)
    } else {
        run_workers(enumerator, &shared, config, &mut delivery)
    };
    finish(&mut delivery, stop)
}

fn finish<C: FnMut(ScanItem) -> ControlFlow<()>>(
    delivery: &mut Delivery<'_, C>,
    stop: Option<StopReason>,
) -> ScanSummary {
    if stop != Some(StopReason::Consumer) {
        let totals = delivery.totals;
        // The traversal is over either way; a Break here changes nothing.
        let _final_progress = (delivery.consume)(ScanItem::Progress(totals));
    }
    ScanSummary {
        totals: delivery.totals,
        stop,
    }
}

fn run_workers<E: DirEnumerator, C: FnMut(ScanItem) -> ControlFlow<()>>(
    enumerator: &E,
    shared: &Shared<'_>,
    config: &ScanConfig,
    delivery: &mut Delivery<'_, C>,
) -> Option<StopReason> {
    let (sender, receiver) = sync_channel::<ScanItem>(config.channel_capacity.get());
    std::thread::scope(|scope| {
        for _ in 0..config.workers.get() {
            let sender = sender.clone();
            scope.spawn(move || worker(enumerator, shared, config, &sender));
        }
        // Only workers hold senders now, so the channel closes when they finish.
        drop(sender);
        let stop = loop {
            if shared.cancel.is_cancelled() {
                break Some(StopReason::Cancelled);
            }
            match receiver.recv_timeout(CANCEL_POLL) {
                Ok(item) => {
                    if delivery.deliver(item).is_break() {
                        break Some(StopReason::Consumer);
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    break shared
                        .cancel
                        .is_cancelled()
                        .then_some(StopReason::Cancelled);
                }
            }
        };
        if stop.is_some() {
            shared.stop();
        }
        // Dropping the receiver makes any blocked send fail, releasing workers.
        drop(receiver);
        stop
    })
}

fn worker<E: DirEnumerator>(
    enumerator: &E,
    shared: &Shared<'_>,
    config: &ScanConfig,
    sender: &SyncSender<ScanItem>,
) {
    while let Some(dir) = shared.take() {
        let mut children = Vec::new();
        let mut disconnected = false;
        let result = enumerator.read_dir(&dir.path, Some(&dir.identity), &mut |entry| {
            if shared.cancel.is_cancelled() {
                return ControlFlow::Break(());
            }
            if entry.kind == EntryKind::Directory {
                let skip = if entry.size.identity.volume != dir.root_volume && !config.cross_devices
                {
                    Some(SkipReason::MountPoint)
                } else if shared.claim(&entry.size.identity) {
                    children.push(PendingDirectory {
                        path: entry.path.clone(),
                        identity: entry.size.identity.clone(),
                        root_volume: dir.root_volume.clone(),
                    });
                    None
                } else {
                    Some(SkipReason::Alias)
                };
                if let Some(reason) = skip
                    && sender
                        .send(ScanItem::Skipped {
                            path: entry.path.clone(),
                            reason,
                        })
                        .is_err()
                {
                    disconnected = true;
                    return ControlFlow::Break(());
                }
            }
            if sender.send(ScanItem::Entry(entry)).is_err() {
                disconnected = true;
                return ControlFlow::Break(());
            }
            ControlFlow::Continue(())
        });
        let item = match result {
            Ok(outcome) => ScanItem::Listed {
                path: dir.path,
                outcome,
            },
            Err(error) => ScanItem::Skipped {
                path: dir.path,
                reason: SkipReason::Enumerate(error),
            },
        };
        if disconnected || sender.send(item).is_err() {
            shared.stop();
            shared.finish(Vec::new());
            return;
        }
        shared.finish(children);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::num::NonZeroU32;

    use lumen_domain::{
        AccessState, ByteCount, DenialReason, EntryTimes, FailureKind, FileId, FileIdentity,
        Protection, SizeFacts, SizeFlags, VolumeId,
    };
    use proptest::prelude::*;

    use super::*;

    /// In-memory filesystem: absolute path -> (kind, volume, inode, denied).
    #[derive(Default)]
    struct FakeFs {
        nodes: BTreeMap<String, (EntryKind, &'static str, u128, bool)>,
    }

    impl FakeFs {
        fn add(&mut self, path: &str, kind: EntryKind, volume: &'static str, denied: bool) {
            let ino = u128::try_from(self.nodes.len()).unwrap_or(u128::MAX) + 1;
            self.nodes
                .insert(path.to_owned(), (kind, volume, ino, denied));
        }

        /// Adds `path` as another name for `target` (same identity), like a macOS
        /// firmlink or a bind mount. Listing it lists nothing of its own.
        fn alias(&mut self, path: &str, target: &str) {
            if let Some(node) = self.nodes.get(target).copied() {
                self.nodes.insert(path.to_owned(), node);
            }
        }

        fn entry(&self, path: &str) -> Option<FilesystemEntry> {
            let (kind, volume, ino, _) = *self.nodes.get(path)?;
            Some(FilesystemEntry {
                path: RawPath::from_unix_bytes(path.as_bytes().to_vec()).ok()?,
                kind,
                size: SizeFacts {
                    identity: FileIdentity {
                        volume: VolumeId::new(volume).ok()?,
                        file: FileId::new(ino),
                    },
                    logical: ByteCount::new(1),
                    allocated: ByteCount::new(1),
                    private: None,
                    clone_id: None,
                    link_count: NonZeroU32::MIN,
                    flags: SizeFlags::default(),
                },
                times: EntryTimes::default(),
                protection: Protection::default(),
                access: if kind == EntryKind::Directory {
                    AccessState::NotScanned
                } else {
                    AccessState::Readable
                },
            })
        }
    }

    fn text(path: &RawPath) -> String {
        String::from_utf8_lossy(path.as_bytes()).into_owned()
    }

    impl DirEnumerator for FakeFs {
        fn stat(&self, path: &RawPath) -> Result<FilesystemEntry, EnumerateError> {
            self.entry(&text(path))
                .ok_or_else(|| EnumerateError::Unobservable {
                    path: path.clone(),
                    access: AccessState::Failed(FailureKind::Vanished),
                })
        }

        fn read_dir(
            &self,
            path: &RawPath,
            _expected: Option<&FileIdentity>,
            sink: &mut dyn FnMut(FilesystemEntry) -> ControlFlow<()>,
        ) -> Result<DirOutcome, EnumerateError> {
            let dir = text(path);
            if self.nodes.get(&dir).is_some_and(|n| n.3) {
                return Ok(DirOutcome {
                    access: AccessState::Denied(DenialReason::Posix),
                    complete: false,
                });
            }
            let prefix = if dir == "/" {
                "/".to_owned()
            } else {
                format!("{dir}/")
            };
            for child in self.nodes.keys().filter(|k| {
                k.starts_with(&prefix) && !k[prefix.len()..].contains('/') && **k != dir
            }) {
                if let Some(entry) = self.entry(child)
                    && sink(entry).is_break()
                {
                    return Ok(DirOutcome {
                        access: AccessState::Readable,
                        complete: false,
                    });
                }
            }
            Ok(DirOutcome {
                access: AccessState::Readable,
                complete: true,
            })
        }
    }

    fn config(workers: usize, capacity: usize, cross_devices: bool) -> ScanConfig {
        ScanConfig {
            workers: NonZeroUsize::new(workers).unwrap_or(NonZeroUsize::MIN),
            channel_capacity: NonZeroUsize::new(capacity).unwrap_or(NonZeroUsize::MIN),
            cross_devices,
            progress_every: NonZeroUsize::new(1000).unwrap_or(NonZeroUsize::MIN),
        }
    }

    fn root() -> RawPath {
        RawPath::from_unix_bytes(b"/r".to_vec()).unwrap_or_else(|_| unreachable!())
    }

    fn collect(fs: &FakeFs, config: &ScanConfig) -> (Vec<ScanItem>, ScanSummary) {
        let mut items = Vec::new();
        let summary = scan(fs, &[root()], config, &CancelToken::new(), |item| {
            items.push(item);
            ControlFlow::Continue(())
        });
        (items, summary)
    }

    fn entry_paths(items: &[ScanItem]) -> Vec<String> {
        let mut paths: Vec<_> = items
            .iter()
            .filter_map(|i| match i {
                ScanItem::Entry(e) => Some(text(&e.path)),
                _ => None,
            })
            .collect();
        paths.sort();
        paths
    }

    fn sample() -> FakeFs {
        let mut fs = FakeFs::default();
        fs.add("/r", EntryKind::Directory, "v1", false);
        fs.add("/r/a", EntryKind::File, "v1", false);
        fs.add("/r/d", EntryKind::Directory, "v1", false);
        fs.add("/r/d/b", EntryKind::File, "v1", false);
        fs.add("/r/d/e", EntryKind::Directory, "v1", false);
        fs.add("/r/d/e/c", EntryKind::File, "v1", false);
        fs.add("/r/link", EntryKind::Symlink, "v1", false);
        fs
    }

    #[test]
    fn delivers_every_entry_once_for_any_worker_count() {
        let fs = sample();
        let expected: Vec<String> = fs.nodes.keys().cloned().collect();
        for workers in [1, 2, 8] {
            let (items, summary) = collect(&fs, &config(workers, 2, false));
            assert_eq!(entry_paths(&items), expected, "workers={workers}");
            let t = summary.totals;
            assert_eq!(
                (t.entries, t.directories, t.skipped, summary.stop),
                (7, 3, 0, None)
            );
            assert_eq!(t.file_bytes, ByteCount::new(3), "three one-byte files");
            assert_eq!(
                items.last(),
                Some(&ScanItem::Progress(t)),
                "final progress matches the summary"
            );
        }
    }

    #[test]
    fn mount_points_are_reported_not_entered() {
        let mut fs = sample();
        fs.add("/r/mnt", EntryKind::Directory, "v2", false);
        fs.add("/r/mnt/other", EntryKind::File, "v2", false);
        let (items, _) = collect(&fs, &config(4, 8, false));
        assert!(items.contains(&ScanItem::Skipped {
            path: RawPath::from_unix_bytes(b"/r/mnt".to_vec()).unwrap_or_else(|_| unreachable!()),
            reason: SkipReason::MountPoint,
        }));
        assert!(!entry_paths(&items).contains(&"/r/mnt/other".to_owned()));
        let (crossed, _) = collect(&fs, &config(4, 8, true));
        assert!(entry_paths(&crossed).contains(&"/r/mnt/other".to_owned()));
    }

    #[test]
    fn denied_directories_are_listed_as_denied_without_children() {
        let mut fs = sample();
        fs.nodes.entry("/r/d".to_owned()).and_modify(|n| n.3 = true);
        let (items, _) = collect(&fs, &config(3, 4, false));
        assert!(items.iter().any(|i| matches!(
            i,
            ScanItem::Listed {
                outcome: DirOutcome {
                    access: AccessState::Denied(_),
                    complete: false
                },
                ..
            }
        )));
        assert!(!entry_paths(&items).iter().any(|p| p.starts_with("/r/d/")));
    }

    #[test]
    fn missing_roots_are_skipped() {
        let fs = FakeFs::default();
        let (items, summary) = collect(&fs, &config(2, 2, false));
        assert!(matches!(
            items.as_slice(),
            [
                ScanItem::Skipped {
                    reason: SkipReason::Enumerate(EnumerateError::Unobservable { .. }),
                    ..
                },
                ScanItem::Progress(_),
            ]
        ));
        assert_eq!(summary.totals.skipped, 1);
    }

    #[test]
    fn consumer_can_stop_a_large_scan_with_a_tiny_channel() {
        let mut fs = FakeFs::default();
        fs.add("/r", EntryKind::Directory, "v1", false);
        for d in 0..20 {
            fs.add(&format!("/r/{d}"), EntryKind::Directory, "v1", false);
            for f in 0..20 {
                fs.add(&format!("/r/{d}/{f}"), EntryKind::File, "v1", false);
            }
        }
        let mut seen = 0;
        let summary = scan(
            &fs,
            &[root()],
            &config(4, 1, false),
            &CancelToken::new(),
            |_| {
                seen += 1;
                if seen == 10 {
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            },
        );
        assert_eq!(summary.stop, Some(StopReason::Consumer));
        assert_eq!(seen, 10, "nothing is delivered after the consumer stops");
    }

    #[test]
    fn cancelling_before_the_scan_delivers_nothing_but_final_progress() {
        let token = CancelToken::new();
        token.cancel();
        let mut items = Vec::new();
        let summary = scan(&sample(), &[root()], &config(2, 2, false), &token, |item| {
            items.push(item);
            ControlFlow::Continue(())
        });
        assert_eq!(summary.stop, Some(StopReason::Cancelled));
        assert_eq!(items, [ScanItem::Progress(ScanProgress::default())]);
    }

    #[test]
    fn cancelling_from_the_consumer_stops_the_scan() {
        let mut fs = FakeFs::default();
        fs.add("/r", EntryKind::Directory, "v1", false);
        for d in 0..50 {
            fs.add(&format!("/r/{d}"), EntryKind::Directory, "v1", false);
        }
        let token = CancelToken::new();
        let mut entries = 0;
        let summary = scan(&fs, &[root()], &config(4, 1, false), &token, |item| {
            if matches!(item, ScanItem::Entry(_)) {
                entries += 1;
                if entries == 5 {
                    token.cancel();
                }
            }
            ControlFlow::Continue(())
        });
        assert_eq!(summary.stop, Some(StopReason::Cancelled));
        assert!(
            summary.totals.directories < 51,
            "not every directory was listed"
        );
    }

    /// An enumerator whose listings take a long time, to prove cancellation does
    /// not wait for them.
    struct Slow(FakeFs);

    impl DirEnumerator for Slow {
        fn stat(&self, path: &RawPath) -> Result<FilesystemEntry, EnumerateError> {
            self.0.stat(path)
        }

        fn read_dir(
            &self,
            path: &RawPath,
            expected: Option<&FileIdentity>,
            sink: &mut dyn FnMut(FilesystemEntry) -> ControlFlow<()>,
        ) -> Result<DirOutcome, EnumerateError> {
            std::thread::sleep(Duration::from_millis(200));
            self.0.read_dir(path, expected, sink)
        }
    }

    #[test]
    fn cancellation_from_another_thread_is_prompt_even_during_slow_listings() {
        let mut fs = FakeFs::default();
        fs.add("/r", EntryKind::Directory, "v1", false);
        for d in 0..40 {
            fs.add(&format!("/r/{d}"), EntryKind::Directory, "v1", false);
        }
        let token = CancelToken::new();
        let canceller = token.clone();
        let started = std::time::Instant::now();
        let summary = std::thread::scope(|scope| {
            scope.spawn(move || {
                std::thread::sleep(Duration::from_millis(100));
                canceller.cancel();
            });
            scan(&Slow(fs), &[root()], &config(2, 4, false), &token, |_| {
                ControlFlow::Continue(())
            })
        });
        assert_eq!(summary.stop, Some(StopReason::Cancelled));
        // 41 directories at 200 ms over 2 workers would take ~4 s; cancellation must
        // return after at most the listings already in progress.
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "took {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn progress_is_emitted_periodically_and_never_decreases() {
        let mut fs = FakeFs::default();
        fs.add("/r", EntryKind::Directory, "v1", false);
        for f in 0..30 {
            fs.add(&format!("/r/{f}"), EntryKind::File, "v1", false);
        }
        let mut cfg = config(2, 4, false);
        cfg.progress_every = NonZeroUsize::new(5).unwrap_or(NonZeroUsize::MIN);
        let (items, summary) = collect(&fs, &cfg);
        let progress: Vec<ScanProgress> = items
            .iter()
            .filter_map(|i| {
                if let ScanItem::Progress(p) = i {
                    Some(*p)
                } else {
                    None
                }
            })
            .collect();
        assert!(progress.len() >= 6, "{} snapshots", progress.len());
        assert!(
            progress
                .windows(2)
                .all(|w| w[0].entries <= w[1].entries && w[0].file_bytes <= w[1].file_bytes)
        );
        assert_eq!(progress.last(), Some(&summary.totals));
    }

    fn pending(fs: &FakeFs, p: &str) -> Option<PendingDirectory> {
        let entry = fs.entry(p)?;
        Some(PendingDirectory {
            path: entry.path,
            root_volume: entry.size.identity.volume.clone(),
            identity: entry.size.identity,
        })
    }

    fn listed(items: &[ScanItem]) -> Vec<String> {
        let mut paths: Vec<_> = items
            .iter()
            .filter_map(|i| match i {
                ScanItem::Listed { path, .. } => Some(text(path)),
                _ => None,
            })
            .collect();
        paths.sort();
        paths
    }

    fn aliases(items: &[ScanItem]) -> Vec<String> {
        let mut paths: Vec<_> = items
            .iter()
            .filter_map(|i| match i {
                ScanItem::Skipped {
                    path,
                    reason: SkipReason::Alias,
                } => Some(text(path)),
                _ => None,
            })
            .collect();
        paths.sort();
        paths
    }

    #[test]
    fn each_directory_is_listed_once_and_alias_loops_end() {
        let mut fs = sample();
        fs.alias("/r/z-alias", "/r/d");
        // A loop back to the root through an alias.
        fs.alias("/r/d/e/up", "/r");
        for workers in [1, 4] {
            let (items, summary) = collect(&fs, &config(workers, 2, false));
            assert_eq!(summary.stop, None);
            assert_eq!(
                listed(&items),
                ["/r", "/r/d", "/r/d/e"],
                "{workers} workers"
            );
            assert_eq!(aliases(&items), ["/r/d/e/up", "/r/z-alias"]);
            assert!(
                entry_paths(&items).contains(&"/r/z-alias".to_owned()),
                "the alias itself is still reported"
            );
            let mut coverage = crate::CoverageBuilder::new(&[root()]);
            for item in &items {
                coverage.observe(item);
            }
            assert!(
                coverage.finish(&summary).roots[0].coverage.is_complete(),
                "an alias is listed elsewhere, so it is not a gap"
            );
        }
    }

    #[test]
    fn overlapping_roots_and_frontiers_are_listed_once() {
        let fs = sample();
        let d = RawPath::from_unix_bytes(b"/r/d".to_vec()).unwrap_or_else(|_| unreachable!());
        let mut items = Vec::new();
        scan(
            &fs,
            &[root(), d.clone(), d],
            &config(2, 2, false),
            &CancelToken::new(),
            |item| {
                items.push(item);
                ControlFlow::Continue(())
            },
        );
        assert_eq!(listed(&items), ["/r", "/r/d", "/r/d/e"]);

        let frontier: Vec<_> = pending(&fs, "/r/d")
            .into_iter()
            .chain(pending(&fs, "/r/d"))
            .collect();
        let mut resumed = Vec::new();
        resume(
            &fs,
            &frontier,
            &config(2, 2, false),
            &CancelToken::new(),
            |item| {
                resumed.push(item);
                ControlFlow::Continue(())
            },
        );
        assert_eq!(listed(&resumed), ["/r/d", "/r/d/e"]);
    }

    #[test]
    fn resume_lists_only_the_frontier_subtrees() {
        let fs = sample();
        let frontier: Vec<_> = pending(&fs, "/r/d").into_iter().collect();
        let mut items = Vec::new();
        let summary = resume(
            &fs,
            &frontier,
            &config(2, 2, false),
            &CancelToken::new(),
            |item| {
                items.push(item);
                ControlFlow::Continue(())
            },
        );
        assert_eq!(
            entry_paths(&items),
            ["/r/d/b", "/r/d/e", "/r/d/e/c"],
            "roots are not re-reported"
        );
        assert_eq!((summary.totals.directories, summary.stop), (2, None));
    }

    proptest! {
        #[test]
        fn interrupting_and_resuming_covers_exactly_the_full_scan(
            shape in prop::collection::vec((0usize..6, any::<bool>()), 1..40),
            stop_after in 1usize..60,
            workers in 1usize..4,
        ) {
            let mut fs = FakeFs::default();
            fs.add("/r", EntryKind::Directory, "v1", false);
            let mut dirs = vec!["/r".to_owned()];
            for (i, (parent, is_dir)) in shape.iter().enumerate() {
                let parent = dirs[parent % dirs.len()].clone();
                let path = format!("{parent}/n{i}");
                fs.add(&path, if *is_dir { EntryKind::Directory } else { EntryKind::File }, "v1", false);
                if *is_dir {
                    dirs.push(path);
                }
            }
            // First run, interrupted after `stop_after` results.
            let mut first = Vec::new();
            let mut seen = 0;
            scan(&fs, &[root()], &config(workers, 1, false), &CancelToken::new(), |item| {
                first.push(item);
                seen += 1;
                if seen >= stop_after { ControlFlow::Break(()) } else { ControlFlow::Continue(()) }
            });
            // Frontier exactly as the store derives it: directory entries without a listing.
            let listed: BTreeSet<String> = first
                .iter()
                .filter_map(|i| if let ScanItem::Listed { path, .. } = i { Some(text(path)) } else { None })
                .collect();
            let frontier: Vec<PendingDirectory> = first
                .iter()
                .filter_map(|i| match i {
                    ScanItem::Entry(e) if e.kind == EntryKind::Directory && !listed.contains(&text(&e.path)) => {
                        pending(&fs, &text(&e.path))
                    }
                    _ => None,
                })
                .collect();
            let (second, _) = {
                let mut items = Vec::new();
                let summary = resume(&fs, &frontier, &config(workers, 2, false), &CancelToken::new(), |item| {
                    items.push(item);
                    ControlFlow::Continue(())
                });
                (items, summary)
            };
            // Upsert semantics: the union of both runs is the full tree.
            let mut union: BTreeSet<String> = entry_paths(&first).into_iter().collect();
            union.extend(entry_paths(&second));
            prop_assert_eq!(union, fs.nodes.keys().cloned().collect::<BTreeSet<_>>());
        }
    }

    proptest! {
        #[test]
        fn random_trees_are_traversed_exactly_once(
            shape in prop::collection::vec((0usize..8, any::<bool>()), 1..40),
            workers in 1usize..6,
            capacity in 1usize..4,
        ) {
            // Build a random tree: node i's parent is an earlier directory.
            let mut fs = FakeFs::default();
            fs.add("/r", EntryKind::Directory, "v1", false);
            let mut dirs = vec!["/r".to_owned()];
            for (i, (parent, is_dir)) in shape.iter().enumerate() {
                let parent = dirs[parent % dirs.len()].clone();
                let path = format!("{parent}/n{i}");
                if *is_dir {
                    fs.add(&path, EntryKind::Directory, "v1", false);
                    dirs.push(path);
                } else {
                    fs.add(&path, EntryKind::File, "v1", false);
                }
            }
            let (items, summary) = collect(&fs, &config(workers, capacity, false));
            let paths = entry_paths(&items);
            let unique: BTreeSet<_> = paths.iter().collect();
            prop_assert_eq!(unique.len(), paths.len(), "no entry delivered twice");
            prop_assert_eq!(paths, fs.nodes.keys().cloned().collect::<Vec<_>>());
            prop_assert_eq!(summary.totals.directories, u64::try_from(dirs.len()).unwrap_or(u64::MAX));
        }
    }

    #[cfg(unix)]
    #[test]
    fn resume_refuses_a_directory_replaced_since_discovery()
    -> Result<(), Box<dyn std::error::Error>> {
        use std::os::unix::ffi::OsStrExt as _;

        use lumen_testkit::{Fixture, Node};

        use crate::StdFsEnumerator;

        let fx = Fixture::build(&[
            Node::Dir { path: "one".into() },
            Node::Dir { path: "two".into() },
        ])?;
        let raw = |p: &str| -> Result<RawPath, Box<dyn std::error::Error>> {
            Ok(RawPath::from_unix_bytes(
                fx.path(p)?.as_os_str().as_bytes().to_vec(),
            )?)
        };
        let one = StdFsEnumerator.stat(&raw("one")?)?;
        // Pretend "two" was discovered with one's identity, then replaced.
        let stale = PendingDirectory {
            path: raw("two")?,
            root_volume: one.size.identity.volume.clone(),
            identity: one.size.identity,
        };
        let mut items = Vec::new();
        resume(
            &StdFsEnumerator,
            &[stale],
            &ScanConfig::default(),
            &CancelToken::new(),
            |item| {
                items.push(item);
                ControlFlow::Continue(())
            },
        );
        assert!(items.iter().any(|i| matches!(
            i,
            ScanItem::Skipped {
                reason: SkipReason::Enumerate(EnumerateError::IdentityChanged { .. }),
                ..
            }
        )));
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn real_filesystem_scan_does_not_follow_symlinked_directories()
    -> Result<(), Box<dyn std::error::Error>> {
        use std::os::unix::ffi::OsStrExt as _;

        use lumen_testkit::{Fixture, Node};

        use crate::StdFsEnumerator;

        let fx = Fixture::build(&[
            Node::File {
                path: "root/a/file".into(),
                contents: b"1".to_vec(),
            },
            Node::Dir {
                path: "outside/secret-dir".into(),
            },
            Node::Symlink {
                path: "root/escape".into(),
                target: "../outside".into(),
            },
        ])?;
        let root = RawPath::from_unix_bytes(fx.path("root")?.as_os_str().as_bytes().to_vec())?;
        let mut paths = Vec::new();
        let summary = scan(
            &StdFsEnumerator,
            &[root],
            &ScanConfig::default(),
            &CancelToken::new(),
            |item| {
                if let ScanItem::Entry(e) = item {
                    paths.push(String::from_utf8_lossy(e.path.as_bytes()).into_owned());
                }
                ControlFlow::Continue(())
            },
        );
        assert!(
            paths.iter().any(|p| p.ends_with("root/escape")),
            "the link itself is reported"
        );
        assert!(
            !paths.iter().any(|p| p.contains("secret-dir")),
            "the link target is never entered"
        );
        assert_eq!(summary.totals.directories, 2, "root and root/a only");
        Ok(())
    }
}
