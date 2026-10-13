//! The unprivileged Windows [`ChangeFeed`] on `ReadDirectoryChangesW` (ADR-0018).
//!
//! One thread per root keeps an overlapped read armed on the root's directory
//! handle, watching the whole subtree. Reads are armed before [`start`] returns,
//! so a scan started afterwards misses nothing. `ReadDirectoryChangesW` keeps no
//! history, so resume points are refused (the caller rescans), and an overflow
//! or `ERROR_NOTIFY_ENUM_DIR` rescans the root.
//!
//! [`start`]: ChangeFeed::start

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;

use lumen_application::ports::{Change, ChangeBatch, ChangeFeed, ChangeFeedError, ResumePoint};
use lumen_domain::{PathFlavor, RawPath};
use windows_sys::Win32::Foundation::ERROR_NOTIFY_ENUM_DIR;
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
};

use crate::notify::{changes, notices, rescan_root};
use crate::sys::{self, Access, Waited, Watch};

/// Bytes per read; 64 KiB is the documented limit for network shares.
const BUFFER: usize = 64 * 1024;
/// How often a watcher thread checks for shutdown.
const TICK_MS: u32 = 200;

#[derive(Debug, Default)]
struct Shared {
    changes: Mutex<VecDeque<Change>>,
    stop: AtomicBool,
    failed: AtomicBool,
}

impl Shared {
    fn changes(&self) -> MutexGuard<'_, VecDeque<Change>> {
        // A push or pop cannot leave the queue inconsistent, so recover.
        self.changes.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[derive(Debug)]
struct Running {
    shared: Arc<Shared>,
    threads: Vec<JoinHandle<()>>,
}

impl Drop for Running {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        for thread in self.threads.drain(..) {
            // A watcher that panicked has nothing left to clean up: its `Watch`
            // was dropped (and its read cancelled) during unwinding.
            if thread.join().is_err() {
                self.shared.failed.store(true, Ordering::Relaxed);
            }
        }
    }
}

/// Watches roots with `ReadDirectoryChangesW`.
#[derive(Debug, Default)]
pub struct DirectoryChangesFeed {
    running: Option<Running>,
}

fn unavailable(reason: &'static str) -> ChangeFeedError {
    ChangeFeedError::Unavailable { reason }
}

/// Opens and arms a watch on `root`, which must be a real directory.
fn arm(root: &RawPath) -> Result<(Vec<u16>, Watch), ChangeFeedError> {
    if root.flavor() != PathFlavor::Windows {
        return Err(unavailable("root is not a Windows path"));
    }
    let wide = root
        .to_windows_wide()
        .ok_or_else(|| unavailable("root is not a Windows path"))?;
    let handle =
        sys::open(&wide, Access::Watch).map_err(|_| unavailable("root cannot be watched"))?;
    let tag = sys::attribute_tag(&handle).map_err(|_| unavailable("root cannot be observed"))?;
    if tag.FileAttributes & FILE_ATTRIBUTE_DIRECTORY == 0
        || tag.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
    {
        return Err(unavailable("root is not a directory"));
    }
    let mut watch = Watch::new(handle, BUFFER);
    watch
        .arm()
        .map_err(|_| unavailable("root cannot be watched"))?;
    Ok((wide, watch))
}

/// A watcher thread: waits for reads, turns them into changes, and re-arms.
fn watch(root: &[u16], mut watch: Watch, shared: &Shared) {
    let rescan = || shared.changes().extend(rescan_root(root));
    while !shared.stop.load(Ordering::Relaxed) {
        match watch.wait(TICK_MS) {
            Ok(Waited::TimedOut) => continue,
            // Zero bytes: more changes than the buffer could hold.
            Ok(Waited::Done(0)) => rescan(),
            Ok(Waited::Done(len)) => match notices(watch.bytes(len)) {
                Ok(found) => shared.changes().extend(changes(root, &found)),
                Err(_) => rescan(),
            },
            Err(e) if e.raw_os_error() == i32::try_from(ERROR_NOTIFY_ENUM_DIR).ok() => rescan(),
            Err(_) => {
                shared.failed.store(true, Ordering::Relaxed);
                return;
            }
        }
        if watch.arm().is_err() {
            shared.failed.store(true, Ordering::Relaxed);
            return;
        }
    }
    // Dropping `watch` cancels the pending read.
}

impl ChangeFeed for DirectoryChangesFeed {
    fn start(
        &mut self,
        roots: &[RawPath],
        resume: Option<ResumePoint>,
    ) -> Result<(), ChangeFeedError> {
        // Restarting stops the previous watchers first.
        self.running = None;
        if let Some(point) = resume {
            // No history to replay: the caller must rescan.
            return Err(ChangeFeedError::StaleResumePoint {
                volume: point.volume,
            });
        }
        if roots.is_empty() {
            return Err(unavailable("no roots"));
        }
        // Arm every root before any thread starts, so a failure leaves nothing
        // running (dropping the armed watches cancels their reads).
        let armed = roots.iter().map(arm).collect::<Result<Vec<_>, _>>()?;
        let shared = Arc::new(Shared::default());
        let threads = armed
            .into_iter()
            .map(|(root, armed)| {
                let shared = Arc::clone(&shared);
                std::thread::spawn(move || watch(&root, armed, &shared))
            })
            .collect();
        self.running = Some(Running { shared, threads });
        Ok(())
    }

    fn poll(&mut self, max: usize) -> Result<ChangeBatch, ChangeFeedError> {
        let running = self
            .running
            .as_ref()
            .ok_or_else(|| unavailable("the feed is not started"))?;
        if running.shared.failed.load(Ordering::Relaxed) {
            return Err(unavailable("a watch failed; its roots need a rescan"));
        }
        let mut pending = running.shared.changes();
        let take = pending.len().min(max);
        Ok(ChangeBatch {
            changes: pending.drain(..take).collect(),
            resume: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::os::windows::ffi::OsStrExt as _;
    use std::path::Path;
    use std::time::{Duration, Instant};

    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn raw(path: &Path) -> Result<RawPath, Box<dyn std::error::Error>> {
        let wide: Vec<u16> = path.as_os_str().encode_wide().collect();
        Ok(RawPath::from_windows_wide(&wide)?)
    }

    fn wait_for(
        feed: &mut DirectoryChangesFeed,
        wanted: &Change,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            if feed.poll(1000)?.changes.contains(wanted) {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Err(format!("no {wanted:?} within 20 s").into())
    }

    #[test]
    fn reports_changes_in_the_root_and_subdirectories() -> TestResult {
        let dir = tempfile::tempdir()?;
        let root = dir.path().to_path_buf();
        std::fs::create_dir(root.join("sub"))?;
        let mut feed = DirectoryChangesFeed::default();
        feed.start(&[raw(&root)?], None)?;

        std::fs::write(root.join("a.txt"), b"1")?;
        wait_for(&mut feed, &Change::Directory(raw(&root)?))?;
        std::fs::write(root.join("sub").join("b.txt"), b"2")?;
        wait_for(&mut feed, &Change::Directory(raw(&root.join("sub"))?))?;

        let stopping = Instant::now();
        drop(feed);
        assert!(
            stopping.elapsed() < Duration::from_secs(5),
            "watchers stop promptly"
        );
        Ok(())
    }

    #[test]
    fn refuses_resume_points_and_unwatchable_roots() -> TestResult {
        let dir = tempfile::tempdir()?;
        std::fs::write(dir.path().join("f"), b"x")?;
        let mut feed = DirectoryChangesFeed::default();
        let point = ResumePoint {
            volume: lumen_domain::VolumeId::new("vsn:1")?,
            token: vec![1],
        };
        assert!(matches!(
            feed.start(&[raw(dir.path())?], Some(point)),
            Err(ChangeFeedError::StaleResumePoint { .. })
        ));
        for bad in [dir.path().join("missing"), dir.path().join("f")] {
            assert!(matches!(
                feed.start(&[raw(&bad)?], None),
                Err(ChangeFeedError::Unavailable { .. })
            ));
        }
        assert!(matches!(
            feed.start(&[], None),
            Err(ChangeFeedError::Unavailable { .. })
        ));
        assert!(matches!(
            feed.poll(10),
            Err(ChangeFeedError::Unavailable { .. })
        ));
        Ok(())
    }
}
