//! The macOS [`ChangeFeed`] on `FSEvents` (ADR-0018, ADR-0029).
//!
//! One stream watches every root by absolute path. Events arrive on a private
//! serial dispatch queue; the callback only copies each path, flag word and event
//! ID into an inbox, and [`poll`](ChangeFeed::poll) interprets them with
//! [`events::interpret`](crate::events::interpret). Resume points carry the
//! volume's `FSEvents` UUID and the last delivered event ID, and are checked
//! before every replay.
//!
//! This module and `sys` are the adapter's only `unsafe` code: creating,
//! starting and releasing the stream, and reading the callback's arrays.

#![allow(unsafe_code)]

use std::collections::VecDeque;
use std::ffi::{CStr, c_char, c_void};
use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::fs::MetadataExt as _;
use std::path::Path;
use std::ptr::NonNull;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use dispatch2::{DispatchQueue, DispatchQueueAttr, DispatchRetained};
use lumen_application::ports::{Change, ChangeBatch, ChangeFeed, ChangeFeedError, ResumePoint};
use lumen_domain::{PathFlavor, RawPath, VolumeId};
use objc2_core_foundation::{CFArray, CFRetained, CFString};
use objc2_core_services::{
    ConstFSEventStreamRef, FSEventStreamContext, FSEventStreamCreate, FSEventStreamEventFlags,
    FSEventStreamEventId, FSEventStreamInvalidate, FSEventStreamRef, FSEventStreamRelease,
    FSEventStreamSetDispatchQueue, FSEventStreamStart, FSEventStreamStop,
    FSEventsCopyUUIDForDevice, FSEventsGetCurrentEventId, kFSEventStreamCreateFlagWatchRoot,
};

use crate::entry::volume_id;
use crate::events::{Meaning, ResumeToken, interpret};

/// `kFSEventStreamEventIdSinceNow`.
const SINCE_NOW: FSEventStreamEventId = u64::MAX;
/// Seconds `FSEvents` may coalesce events before delivering them.
const LATENCY: f64 = 0.5;

/// One raw event: absolute path bytes, flags, event ID.
type RawEvent = (Vec<u8>, FSEventStreamEventFlags, FSEventStreamEventId);

/// Events copied by the callback, waiting for `poll`.
#[derive(Debug, Default)]
struct Inbox(Mutex<VecDeque<RawEvent>>);

impl Inbox {
    fn lock(&self) -> MutexGuard<'_, VecDeque<RawEvent>> {
        // A push or pop cannot leave the queue inconsistent, so recover.
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A started stream; stopped, invalidated and released on drop.
#[derive(Debug)]
struct Stream {
    stream: FSEventStreamRef,
    // The stream delivers on this queue; keep it alive until the stream is gone.
    _queue: DispatchRetained<DispatchQueue>,
}

// SAFETY: invariant: the stream reference is only used by `Drop`, on the thread
// that owns the feed. Why unsafe: `FSEventStreamRef` is a raw pointer. Why it
// holds: a stream scheduled on a dispatch queue may be stopped and released from
// any thread, and nothing else touches `stream`.
unsafe impl Send for Stream {}

impl Drop for Stream {
    fn drop(&mut self) {
        // SAFETY: invariant: `stream` is a started stream owned by `self`. Why
        // unsafe: FFI calls. Why it holds: `start` only builds a `Stream` from a
        // started stream. Invalidating guarantees no further callbacks, and the
        // inbox they write to is dropped only after this (see `Running`).
        unsafe {
            FSEventStreamStop(self.stream);
            FSEventStreamInvalidate(self.stream);
            FSEventStreamRelease(self.stream);
        }
    }
}

#[derive(Debug)]
struct Running {
    // Declared before `inbox`: fields drop in order, so the stream is released
    // before the inbox its callback writes to.
    _stream: Stream,
    inbox: Arc<Inbox>,
    volume: VolumeId,
    uuid: [u8; 16],
}

/// Watches roots on one volume with `FSEvents`.
#[derive(Debug, Default)]
pub struct FsEventsFeed {
    running: Option<Running>,
}

unsafe extern "C-unwind" fn on_events(
    _stream: ConstFSEventStreamRef,
    info: *mut c_void,
    count: usize,
    paths: NonNull<c_void>,
    flags: NonNull<FSEventStreamEventFlags>,
    ids: NonNull<FSEventStreamEventId>,
) {
    // SAFETY: invariant: `info` is the `Inbox` passed at creation, alive until the
    // stream is invalidated. Why unsafe: dereferencing a raw pointer. Why it
    // holds: `Running` drops the stream (stop, invalidate, release) before its
    // `Arc<Inbox>`.
    let inbox = unsafe { &*info.cast::<Inbox>() };
    // SAFETY: invariant: without `kFSEventStreamCreateFlagUseCFTypes`, `paths` is
    // a C array of `count` NUL-terminated strings, and `flags` and `ids` are arrays
    // of `count` elements, all valid during the callback. Why unsafe: slices from
    // raw pointers. Why it holds: the `FSEventStreamCallback` contract for the
    // flags `start` uses.
    let (paths, flags, ids) = unsafe {
        (
            std::slice::from_raw_parts(paths.cast::<*const c_char>().as_ptr(), count),
            std::slice::from_raw_parts(flags.as_ptr(), count),
            std::slice::from_raw_parts(ids.as_ptr(), count),
        )
    };
    let mut events = inbox.lock();
    for ((path, flag), id) in paths.iter().zip(flags).zip(ids) {
        // SAFETY: invariant: `path` is a valid NUL-terminated string during the
        // callback. Why unsafe: reading a C string. Why it holds: as above.
        let bytes = unsafe { CStr::from_ptr(*path) }.to_bytes().to_vec();
        events.push_back((bytes, *flag, *id));
    }
}

fn unavailable(reason: &'static str) -> ChangeFeedError {
    ChangeFeedError::Unavailable { reason }
}

/// The single device all roots live on, and the roots as `CFString`s.
fn roots_on_one_device(
    roots: &[RawPath],
) -> Result<(i32, Vec<CFRetained<CFString>>), ChangeFeedError> {
    let mut device = None;
    let mut paths = Vec::with_capacity(roots.len());
    for root in roots {
        if root.flavor() != PathFlavor::Unix {
            return Err(unavailable("root is not a Unix path"));
        }
        let path = Path::new(std::ffi::OsStr::from_bytes(root.as_bytes()));
        let meta =
            std::fs::symlink_metadata(path).map_err(|_| unavailable("root cannot be observed"))?;
        let dev =
            i32::try_from(meta.dev()).map_err(|_| unavailable("device number out of range"))?;
        if device.replace(dev).is_some_and(|d| d != dev) {
            return Err(unavailable("roots are on more than one volume"));
        }
        let text =
            std::str::from_utf8(root.as_bytes()).map_err(|_| unavailable("root is not UTF-8"))?;
        paths.push(CFString::from_str(text));
    }
    Ok((device.ok_or_else(|| unavailable("no roots"))?, paths))
}

/// The `FSEvents` UUID of a device's event store.
fn event_store_uuid(device: i32) -> Result<[u8; 16], ChangeFeedError> {
    // SAFETY: invariant: none beyond a valid device number. Why unsafe: FFI call.
    // Why it holds: `device` came from `lstat` of a root.
    let uuid = unsafe { FSEventsCopyUUIDForDevice(device) }
        .ok_or_else(|| unavailable("volume has no persistent event store"))?
        .uuid_bytes();
    Ok([
        uuid.byte0,
        uuid.byte1,
        uuid.byte2,
        uuid.byte3,
        uuid.byte4,
        uuid.byte5,
        uuid.byte6,
        uuid.byte7,
        uuid.byte8,
        uuid.byte9,
        uuid.byte10,
        uuid.byte11,
        uuid.byte12,
        uuid.byte13,
        uuid.byte14,
        uuid.byte15,
    ])
}

/// The event ID to replay from: now, or a resume point that is still
/// trustworthy for this volume (ADR-0029).
fn replay_from(
    resume: Option<ResumePoint>,
    volume: &VolumeId,
    uuid: [u8; 16],
) -> Result<FSEventStreamEventId, ChangeFeedError> {
    let Some(point) = resume else {
        return Ok(SINCE_NOW);
    };
    let stale = || ChangeFeedError::StaleResumePoint {
        volume: point.volume.clone(),
    };
    if point.volume != *volume {
        return Err(stale());
    }
    let token = ResumeToken::decode(&point.token).map_err(|_| stale())?;
    // SAFETY: invariant: none. Why unsafe: FFI call. Why it holds: no arguments.
    let current = unsafe { FSEventsGetCurrentEventId() };
    if token.is_valid_for(uuid, current) {
        Ok(token.event_id)
    } else {
        Err(stale())
    }
}

impl ChangeFeed for FsEventsFeed {
    fn start(
        &mut self,
        roots: &[RawPath],
        resume: Option<ResumePoint>,
    ) -> Result<(), ChangeFeedError> {
        // Restarting replaces the previous stream.
        self.running = None;
        let (device, paths) = roots_on_one_device(roots)?;
        let volume = volume_id(device).map_err(|_| unavailable("device number out of range"))?;
        let uuid = event_store_uuid(device)?;
        let since = replay_from(resume, &volume, uuid)?;

        let inbox = Arc::new(Inbox::default());
        let array: CFRetained<CFArray<CFString>> = CFArray::from_retained_objects(&paths);
        let mut context = FSEventStreamContext {
            version: 0,
            info: Arc::as_ptr(&inbox).cast_mut().cast(),
            retain: None,
            release: None,
            copyDescription: None,
        };
        // SAFETY: invariant: the callback matches `FSEventStreamCallback`, the
        // context is copied by the call, `array` holds CFStrings, and `info`
        // stays valid while the stream exists. Why unsafe: FFI call. Why it
        // holds: `on_events` has the documented signature; `inbox` is kept in
        // `Running`, which releases the stream first.
        let stream = unsafe {
            FSEventStreamCreate(
                None,
                Some(on_events),
                &raw mut context,
                array.as_ref(),
                since,
                LATENCY,
                kFSEventStreamCreateFlagWatchRoot,
            )
        };
        if stream.is_null() {
            return Err(unavailable("cannot create the event stream"));
        }
        let queue = DispatchQueue::new("dev.lumen.fsevents", DispatchQueueAttr::SERIAL);
        // SAFETY: invariant: `stream` is a valid, unscheduled stream and `queue` a
        // live serial queue. Why unsafe: FFI calls. Why it holds: both were just
        // created; ownership of `stream` moves into `Stream`, which releases it.
        let started = unsafe {
            FSEventStreamSetDispatchQueue(stream, Some(&queue));
            FSEventStreamStart(stream)
        };
        let stream = Stream {
            stream,
            _queue: queue,
        };
        if !started {
            return Err(unavailable("cannot start the event stream"));
        }
        self.running = Some(Running {
            _stream: stream,
            inbox,
            volume,
            uuid,
        });
        Ok(())
    }

    fn poll(&mut self, max: usize) -> Result<ChangeBatch, ChangeFeedError> {
        let running = self
            .running
            .as_ref()
            .ok_or_else(|| unavailable("the feed is not started"))?;
        let drained: Vec<RawEvent> = {
            let mut events = running.inbox.lock();
            let take = events.len().min(max);
            events.drain(..take).collect()
        };
        let mut changes = Vec::with_capacity(drained.len());
        let mut last = None;
        for (path, flags, id) in drained {
            last = Some(id);
            match interpret(&path, flags, &running.volume) {
                Meaning::Change(change) => changes.push(change),
                Meaning::HistoryDone => {}
                // A path Lumen cannot represent: rescan everything it might be.
                Meaning::Unusable => changes.push(Change::RescanVolume(running.volume.clone())),
            }
        }
        Ok(ChangeBatch {
            changes,
            resume: last.map(|event_id| ResumePoint {
                volume: running.volume.clone(),
                token: ResumeToken {
                    volume_uuid: running.uuid,
                    event_id,
                }
                .encode(),
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn raw(path: &Path) -> Result<RawPath, Box<dyn std::error::Error>> {
        Ok(RawPath::from_unix_bytes(
            path.as_os_str().as_bytes().to_vec(),
        )?)
    }

    /// Polls until a change satisfies `wanted`, collecting the last resume point.
    fn wait_for(
        feed: &mut FsEventsFeed,
        wanted: impl Fn(&Change) -> bool,
    ) -> Result<Option<ResumePoint>, Box<dyn std::error::Error>> {
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut resume = None;
        while Instant::now() < deadline {
            let batch = feed.poll(1000)?;
            resume = batch.resume.or(resume);
            if batch.changes.iter().any(&wanted) {
                return Ok(resume);
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Err("no matching change within 20 s".into())
    }

    fn directory(path: &RawPath) -> impl Fn(&Change) -> bool + '_ {
        move |change| matches!(change, Change::Directory(p) if p == path)
    }

    #[test]
    fn reports_changed_directories_and_resumes_from_history() -> TestResult {
        // `/var` is a symlink to `/private/var`; FSEvents reports real paths.
        let dir = tempfile::tempdir()?;
        let root = std::fs::canonicalize(dir.path())?;
        let sub = root.join("sub");
        std::fs::create_dir(&sub)?;

        let mut feed = FsEventsFeed::default();
        feed.start(&[raw(&root)?], None)?;
        std::fs::write(sub.join("a"), b"1")?;
        let resume = wait_for(&mut feed, directory(&raw(&sub)?))?.ok_or("no resume point")?;
        drop(feed);

        // A change while nothing watches is replayed from the stored point.
        std::fs::write(root.join("b"), b"2")?;
        let mut replay = FsEventsFeed::default();
        replay.start(&[raw(&root)?], Some(resume))?;
        wait_for(&mut replay, directory(&raw(&root)?))?;
        Ok(())
    }

    #[test]
    fn refuses_stale_resume_points() -> TestResult {
        let dir = tempfile::tempdir()?;
        let root = raw(&std::fs::canonicalize(dir.path())?)?;
        let mut feed = FsEventsFeed::default();
        feed.start(std::slice::from_ref(&root), None)?;
        let volume = feed.running.as_ref().ok_or("not running")?.volume.clone();
        let uuid = feed.running.as_ref().ok_or("not running")?.uuid;

        let point = |volume_uuid, event_id| ResumePoint {
            volume: volume.clone(),
            token: ResumeToken {
                volume_uuid,
                event_id,
            }
            .encode(),
        };
        let mut other = uuid;
        other[0] ^= 0xFF;
        for stale in [
            point(other, 1),
            point(uuid, u64::MAX - 1),
            ResumePoint {
                volume: volume.clone(),
                token: vec![9, 9, 9],
            },
            ResumePoint {
                volume: VolumeId::new("dev:0")?,
                token: point(uuid, 1).token,
            },
        ] {
            assert!(matches!(
                feed.start(std::slice::from_ref(&root), Some(stale)),
                Err(ChangeFeedError::StaleResumePoint { .. })
            ));
        }
        Ok(())
    }

    #[test]
    fn refuses_roots_on_several_volumes_and_polling_before_start() -> TestResult {
        let mut feed = FsEventsFeed::default();
        assert!(matches!(
            feed.poll(10),
            Err(ChangeFeedError::Unavailable { .. })
        ));
        let dir = tempfile::tempdir()?;
        let roots = [
            raw(&std::fs::canonicalize(dir.path())?)?,
            raw(Path::new("/dev"))?,
        ];
        assert!(matches!(
            feed.start(&roots, None),
            Err(ChangeFeedError::Unavailable { .. })
        ));
        assert!(matches!(
            feed.start(&[], None),
            Err(ChangeFeedError::Unavailable { .. })
        ));
        Ok(())
    }
}
