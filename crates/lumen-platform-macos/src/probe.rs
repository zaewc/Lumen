//! The macOS [`InUseProbe`] on `libproc` (roadmap 6.8).
//!
//! For every running process it reads the open vnode file descriptors, the
//! working directory and the executable path, and reports the processes that use
//! the object: by identity (device and inode) for anything, and by canonical
//! path prefix for anything under a directory. The kernel reports open files in
//! the user-facing namespace (`/Users/...`) with symlinks resolved, so the target
//! directory is canonicalized too.
//!
//! An unprivileged process cannot read other users' file descriptors (measured on
//! macOS 26: 872 of 1,107 processes visible, all of them the user's own). Those
//! processes are counted as unseen and the answer is [`InUse::Partial`], never a
//! claim of non-use.

#![allow(unsafe_code)]

use std::ffi::{c_int, c_void};
use std::mem::{MaybeUninit, size_of};
use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::fs::MetadataExt as _;
use std::path::Path;

use lumen_application::ports::{InUse, InUseProbe};
use lumen_domain::{FileIdentity, PathFlavor, Process, RawPath, Timestamp, VolumeId};

/// `PROC_PIDFDVNODEPATHINFO` (`sys/proc_info.h`); not in `libc`.
const PROC_PIDFDVNODEPATHINFO: c_int = 2;

/// `struct proc_fileinfo` (`sys/proc_info.h`); not in `libc`. Field names match
/// the SDK so the mirror can be checked line by line.
#[repr(C)]
#[allow(clippy::struct_field_names, dead_code)]
#[derive(Debug, Clone, Copy)]
struct ProcFileInfo {
    fi_openflags: u32,
    fi_status: u32,
    fi_offset: i64,
    fi_type: i32,
    fi_guardflags: u32,
}

/// `struct vnode_fdinfowithpath` (`sys/proc_info.h`); not in `libc`.
#[repr(C)]
#[derive(Clone, Copy)]
struct VnodeFdInfoWithPath {
    pfi: ProcFileInfo,
    pvip: libc::vnode_info_path,
}

/// Asks the kernel which processes use an object.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessProbe;

/// The kernel's 32-bit `dev_t`, from `MetadataExt::dev` (which sign-extends it).
fn low_32(dev: u64) -> u32 {
    u32::try_from(dev & 0xFFFF_FFFF).unwrap_or_default()
}

/// What to look for.
struct Target {
    device: u32,
    inode: u64,
    /// For a directory: its canonical path followed by `/`.
    under: Option<Vec<u8>>,
}

impl Target {
    fn matches(&self, device: u32, inode: u64, path: &[u8]) -> bool {
        (device == self.device && inode == self.inode)
            || self
                .under
                .as_deref()
                .is_some_and(|dir| path.starts_with(dir))
    }
}

/// What one process revealed.
enum Seen {
    Uses(Process),
    Clean,
    Hidden,
}

/// Classifies an unavailable fd listing without consulting ambient errno.
fn fd_listing_failure(errno: Option<c_int>) -> Seen {
    if errno == Some(libc::ESRCH) {
        Seen::Clean
    } else {
        Seen::Hidden
    }
}

/// The process's thread-local errno, read immediately after a failed libproc call.
fn current_errno() -> c_int {
    // SAFETY: `__error` returns this thread's valid errno pointer on macOS.
    // Why unsafe: dereferencing the pointer returned by the libc accessor.
    // Why it holds: the accessor is defined to return the calling thread's errno.
    unsafe { *libc::__error() }
}

/// The bytes of a fixed-size C path buffer, up to its NUL.
fn c_path(path: &[[libc::c_char; 32]; 32]) -> Vec<u8> {
    path.iter()
        .flatten()
        .map(|c| c.to_ne_bytes()[0])
        .take_while(|b| *b != 0)
        .collect()
}

/// Calls `proc_pidinfo` for a fixed-size struct. `None` if the call fails or
/// returns a different size.
fn pid_info<T: Copy>(pid: c_int, flavor: c_int, arg: u64) -> Option<T> {
    let mut out = MaybeUninit::<T>::uninit();
    let size = c_int::try_from(size_of::<T>()).ok()?;
    // SAFETY: invariant: the kernel writes at most `size` bytes of `T` into
    // `out`. Why unsafe: FFI writing through a raw pointer. Why it holds: each
    // caller pairs `flavor` with the struct the header documents for it, and
    // `size` is exactly that struct's size.
    let written = unsafe { libc::proc_pidinfo(pid, flavor, arg, out.as_mut_ptr().cast(), size) };
    // SAFETY: invariant: `out` is fully initialised. Why unsafe: `assume_init`.
    // Why it holds: the kernel reported writing exactly `size` bytes; every field
    // is a plain integer or byte array.
    (written == size).then(|| unsafe { out.assume_init() })
}

/// The open file descriptors of `pid`, or the errno observed at a failed libproc call.
fn fds(pid: c_int) -> Result<Vec<libc::proc_fdinfo>, Option<c_int>> {
    // SAFETY: invariant: a null buffer of size 0 only queries the needed size.
    // Why unsafe: FFI call. Why it holds: documented sizing call.
    let needed =
        unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDLISTFDS, 0, std::ptr::null_mut(), 0) };
    let errno = (needed < 0).then(current_errno);
    let needed = usize::try_from(needed)
        .ok()
        .filter(|n| *n > 0)
        .ok_or(errno)?;
    // Room for descriptors opened between the two calls.
    let capacity = needed / size_of::<libc::proc_fdinfo>() + 32;
    let mut list: Vec<libc::proc_fdinfo> = Vec::with_capacity(capacity);
    let bytes = c_int::try_from(capacity * size_of::<libc::proc_fdinfo>()).map_err(|_| None)?;
    // SAFETY: invariant: the kernel writes at most `bytes` bytes into the
    // vector's spare capacity. Why unsafe: FFI writing into uninitialised memory.
    // Why it holds: `bytes` is exactly the allocated capacity in bytes.
    let written = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDLISTFDS,
            0,
            list.as_mut_ptr().cast::<c_void>(),
            bytes,
        )
    };
    let errno = (written < 0).then(current_errno);
    let count = usize::try_from(written).map_err(|_| errno)? / size_of::<libc::proc_fdinfo>();
    // SAFETY: invariant: the first `count` elements are initialised. Why unsafe:
    // `set_len`. Why it holds: the kernel wrote `written` bytes, which is
    // `count` whole `proc_fdinfo` records, and `count <= capacity`.
    unsafe { list.set_len(count.min(capacity)) };
    Ok(list)
}

fn vnode_of(pid: c_int, fd: i32) -> Option<VnodeFdInfoWithPath> {
    let mut info = MaybeUninit::<VnodeFdInfoWithPath>::uninit();
    let size = c_int::try_from(size_of::<VnodeFdInfoWithPath>()).ok()?;
    // SAFETY: invariant: the kernel writes at most `size` bytes of
    // `vnode_fdinfowithpath` into `info`. Why unsafe: FFI writing through a raw
    // pointer. Why it holds: `VnodeFdInfoWithPath` mirrors the SDK struct (size
    // pinned by a test) and `size` is its size.
    let written = unsafe {
        libc::proc_pidfdinfo(
            pid,
            fd,
            PROC_PIDFDVNODEPATHINFO,
            info.as_mut_ptr().cast(),
            size,
        )
    };
    // SAFETY: invariant: fully initialised. Why unsafe: `assume_init`. Why it
    // holds: the kernel wrote exactly `size` bytes of plain-old-data fields.
    (written == size).then(|| unsafe { info.assume_init() })
}

fn executable(pid: c_int) -> Option<Vec<u8>> {
    let mut buffer = vec![0_u8; 4 * 1024];
    let size = u32::try_from(buffer.len()).ok()?;
    // SAFETY: invariant: the kernel writes at most `size` bytes. Why unsafe: FFI
    // writing through a raw pointer. Why it holds: `size` is the buffer length.
    let written = unsafe { libc::proc_pidpath(pid, buffer.as_mut_ptr().cast(), size) };
    buffer.truncate(usize::try_from(written).ok().filter(|n| *n > 0)?);
    Some(buffer)
}

/// An object a process uses, as the kernel reports it.
struct Object {
    device: u32,
    inode: u64,
    path: Vec<u8>,
}

/// A process's executable: its path and, if it could be read, its device and
/// inode.
type Executable = (Vec<u8>, Option<(u32, u64)>);

/// A readable process: who it is and everything it uses.
struct Visible {
    process: Process,
    exe: Option<Executable>,
    /// The working directory and every open vnode.
    objects: Vec<Object>,
}

/// One process as read from the kernel: what it uses, or why it could not be
/// read.
enum Observed {
    /// Its executable, working directory and open files.
    Visible(Box<Visible>),
    /// It exited while being read: it uses nothing.
    Gone,
    /// Its file descriptors could not be read (another user's process).
    Hidden,
}

fn observe(pid: c_int) -> Observed {
    let open = match fds(pid) {
        Ok(open) => open,
        Err(errno) => {
            return match fd_listing_failure(errno) {
                Seen::Clean => Observed::Gone,
                _ => Observed::Hidden,
            };
        }
    };
    let exe = executable(pid).map(|path| {
        let identity = std::fs::metadata(Path::new(std::ffi::OsStr::from_bytes(&path)))
            .ok()
            .map(|m| (low_32(m.dev()), m.ino()));
        (path, identity)
    });
    let mut objects = Vec::new();
    if let Some(info) = pid_info::<libc::proc_vnodepathinfo>(pid, libc::PROC_PIDVNODEPATHINFO, 0) {
        let vi = info.pvi_cdir.vip_vi.vi_stat;
        objects.push(Object {
            device: vi.vst_dev,
            inode: vi.vst_ino,
            path: c_path(&info.pvi_cdir.vip_path),
        });
    }
    objects.extend(
        open.iter()
            .filter(|fd| i32::try_from(fd.proc_fdtype).ok() == Some(libc::PROX_FDTYPE_VNODE))
            .filter_map(|fd| vnode_of(pid, fd.proc_fd))
            .map(|info| {
                let st = info.pvip.vip_vi.vi_stat;
                Object {
                    device: st.vst_dev,
                    inode: st.vst_ino,
                    path: c_path(&info.pvip.vip_path),
                }
            }),
    );
    let started_at =
        pid_info::<libc::proc_bsdinfo>(pid, libc::PROC_PIDTBSDINFO, 0).and_then(|bsd| {
            let micros = u32::try_from(bsd.pbi_start_tvusec).ok()?;
            Timestamp::from_unix(
                i64::try_from(bsd.pbi_start_tvsec).ok()?,
                micros.checked_mul(1000)?,
            )
            .ok()
        });
    Observed::Visible(Box::new(Visible {
        process: Process {
            pid: u32::try_from(pid).unwrap_or_default(),
            started_at,
            executable: exe
                .as_ref()
                .and_then(|(path, _)| RawPath::from_unix_bytes(path.clone()).ok()),
            executable_identity: None,
            application: None,
        },
        exe,
        objects,
    }))
}

impl Observed {
    /// Whether this process uses `target`.
    fn seen(&self, target: &Target) -> Seen {
        match self {
            Self::Gone => Seen::Clean,
            Self::Hidden => Seen::Hidden,
            Self::Visible(visible) => {
                let Visible {
                    process,
                    exe,
                    objects,
                } = visible.as_ref();
                let exe_matches = exe.as_ref().is_some_and(|(path, identity)| {
                    target
                        .under
                        .as_deref()
                        .is_some_and(|dir| path.starts_with(dir))
                        || *identity == Some((target.device, target.inode))
                });
                let uses = exe_matches
                    || objects
                        .iter()
                        .any(|object| target.matches(object.device, object.inode, &object.path));
                if uses {
                    Seen::Uses(process.clone())
                } else {
                    Seen::Clean
                }
            }
        }
    }
}

fn all_pids() -> Option<Vec<c_int>> {
    // SAFETY: invariant: a null buffer only queries the count. Why unsafe: FFI.
    // Why it holds: documented sizing call.
    let count = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
    let capacity = usize::try_from(count).ok()? + 64;
    let mut pids = vec![0 as c_int; capacity];
    let bytes = c_int::try_from(capacity * size_of::<c_int>()).ok()?;
    // SAFETY: invariant: the kernel writes at most `bytes` bytes of pids. Why
    // unsafe: FFI writing through a raw pointer. Why it holds: `bytes` is the
    // buffer's size.
    let written = unsafe { libc::proc_listallpids(pids.as_mut_ptr().cast(), bytes) };
    pids.truncate(usize::try_from(written).ok()?.min(capacity));
    Some(pids)
}

/// What to look for at `path`, which must still be `expected`; the answer to
/// give instead when it cannot be established.
fn target(path: &RawPath, expected: &FileIdentity) -> Result<Target, InUse> {
    if path.flavor() != PathFlavor::Unix {
        return Err(InUse::Unknown {
            reason: "not a Unix path",
        });
    }
    let native = Path::new(std::ffi::OsStr::from_bytes(path.as_bytes()));
    let Ok(meta) = std::fs::symlink_metadata(native) else {
        return Err(InUse::Unknown {
            reason: "the object cannot be observed",
        });
    };
    // Same identity rule as the enumerators (`MetadataExt::dev` sign-extends).
    let identity_matches = VolumeId::new(format!("dev:{:x}", meta.dev()))
        .is_ok_and(|volume| volume == expected.volume)
        && u128::from(meta.ino()) == expected.file.get();
    if !identity_matches {
        return Err(InUse::Unknown {
            reason: "the object changed since it was observed",
        });
    }
    let under = meta.is_dir().then(|| {
        std::fs::canonicalize(native).ok().map(|dir| {
            let mut prefix = dir.as_os_str().as_bytes().to_vec();
            if !prefix.ends_with(b"/") {
                prefix.push(b'/');
            }
            prefix
        })
    });
    let Some(under) = under.map_or(Some(None), |p| p.map(Some)) else {
        return Err(InUse::Unknown {
            reason: "the directory cannot be resolved",
        });
    };
    Ok(Target {
        device: low_32(meta.dev()),
        inode: meta.ino(),
        under,
    })
}

/// The answer from what every process revealed.
fn answer(seen: impl Iterator<Item = Seen>) -> InUse {
    let mut users = Vec::new();
    let mut unseen = 0_u32;
    for one in seen {
        match one {
            Seen::Uses(process) => users.push(process),
            Seen::Clean => {}
            Seen::Hidden => unseen = unseen.saturating_add(1),
        }
    }
    if !users.is_empty() {
        InUse::Yes(users)
    } else if unseen > 0 {
        InUse::Partial {
            unseen: Some(unseen),
        }
    } else {
        InUse::No
    }
}

/// The process list could not be read: nothing is known.
const NO_PROCESS_LIST: InUse = InUse::Unknown {
    reason: "the process list cannot be read",
};

impl InUseProbe for ProcessProbe {
    fn probe(&self, path: &RawPath, expected: &FileIdentity) -> InUse {
        let target = match target(path, expected) {
            Ok(target) => target,
            Err(unknown) => return unknown,
        };
        let Some(pids) = all_pids() else {
            return NO_PROCESS_LIST;
        };
        answer(pids.into_iter().map(|pid| observe(pid).seen(&target)))
    }
}

impl ProcessProbe {
    /// Reads every process once, for answering many questions at one moment:
    /// an assessment's candidates. Its answers describe that moment; moving
    /// and removing ask the live [`ProcessProbe`] again, item by item.
    #[must_use]
    pub fn snapshot(self) -> ProcessSnapshot {
        ProcessSnapshot {
            processes: all_pids().map(|pids| pids.into_iter().map(observe).collect()),
        }
    }
}

/// The process table read once by [`ProcessProbe::snapshot`]; answers by the
/// same rules as [`ProcessProbe`].
pub struct ProcessSnapshot {
    /// `None` if the process list could not be read.
    processes: Option<Vec<Observed>>,
}

impl std::fmt::Debug for ProcessSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessSnapshot")
            .field("processes", &self.processes.as_ref().map(Vec::len))
            .finish()
    }
}

impl ProcessSnapshot {
    /// Whether a process runs an executable inside the application bundle at
    /// `bundle` (ADR-0059): `Some(true)` when a visible one does,
    /// `Some(false)` only when every process was visible and none does, and
    /// `None` when that cannot be known.
    #[must_use]
    pub fn running(&self, bundle: &RawPath) -> Option<bool> {
        if bundle.flavor() != PathFlavor::Unix {
            return None;
        }
        let mut prefix = bundle.as_bytes().to_vec();
        if !prefix.ends_with(b"/") {
            prefix.push(b'/');
        }
        let processes = self.processes.as_ref()?;
        let mut hidden = false;
        for process in processes {
            match process {
                Observed::Visible(visible) => {
                    if visible
                        .exe
                        .as_ref()
                        .is_some_and(|(path, _)| path.starts_with(&prefix))
                    {
                        return Some(true);
                    }
                }
                Observed::Hidden => hidden = true,
                Observed::Gone => {}
            }
        }
        (!hidden).then_some(false)
    }
}

impl InUseProbe for ProcessSnapshot {
    fn probe(&self, path: &RawPath, expected: &FileIdentity) -> InUse {
        let target = match target(path, expected) {
            Ok(target) => target,
            Err(unknown) => return unknown,
        };
        let Some(processes) = &self.processes else {
            return NO_PROCESS_LIST;
        };
        answer(processes.iter().map(|process| process.seen(&target)))
    }
}

#[cfg(test)]
mod tests {
    use std::mem::offset_of;

    use lumen_domain::{FileId, RawPath};

    use super::*;

    #[test]
    fn fd_listing_failure_only_treats_captured_esrch_as_clean() {
        assert!(matches!(fd_listing_failure(Some(libc::ESRCH)), Seen::Clean));
        assert!(matches!(
            fd_listing_failure(Some(libc::EACCES)),
            Seen::Hidden
        ));
        assert!(matches!(fd_listing_failure(None), Seen::Hidden));
    }

    #[test]
    fn the_mirrored_struct_matches_the_sdk() {
        // sizeof / offsetof measured with the macOS 26 SDK.
        assert_eq!(size_of::<ProcFileInfo>(), 24);
        assert_eq!(size_of::<libc::vnode_info_path>(), 1176);
        assert_eq!(size_of::<VnodeFdInfoWithPath>(), 1200);
        assert_eq!(offset_of!(VnodeFdInfoWithPath, pvip), 24);
    }

    fn identity(path: &Path) -> Result<(RawPath, FileIdentity), Box<dyn std::error::Error>> {
        let meta = std::fs::symlink_metadata(path)?;
        let raw = RawPath::from_unix_bytes(path.as_os_str().as_bytes().to_vec())?;
        Ok((
            raw,
            FileIdentity {
                volume: VolumeId::new(format!("dev:{:x}", meta.dev()))?,
                file: FileId::new(u128::from(meta.ino())),
            },
        ))
    }

    fn me() -> u32 {
        std::process::id()
    }

    #[test]
    fn an_open_file_and_its_directory_are_in_use_by_this_process()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let file = dir.path().join("held");
        std::fs::write(&file, b"x")?;
        let _held = std::fs::File::open(&file)?;
        let snapshot = ProcessProbe.snapshot();
        for probe in [&ProcessProbe as &dyn InUseProbe, &snapshot] {
            for target in [file.as_path(), dir.path()] {
                let (raw, id) = identity(target)?;
                let answer = probe.probe(&raw, &id);
                assert!(
                    matches!(&answer, InUse::Yes(users) if users.iter().any(|p| p.pid == me())),
                    "{target:?}: {answer:?}"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn this_test_runs_under_its_own_folder_and_nothing_else_is_claimed()
    -> Result<(), Box<dyn std::error::Error>> {
        let exe = std::fs::canonicalize(std::env::current_exe()?)?;
        let folder = exe.parent().ok_or("the test has a folder")?;
        let raw = |path: &Path| RawPath::from_unix_bytes(path.as_os_str().as_bytes().to_vec());
        let snapshot = ProcessProbe.snapshot();
        assert_eq!(snapshot.running(&raw(folder)?), Some(true));
        let elsewhere = tempfile::tempdir()?;
        assert_ne!(
            snapshot.running(&raw(elsewhere.path())?),
            Some(true),
            "nothing runs from an empty folder"
        );
        Ok(())
    }

    #[test]
    fn a_snapshot_answers_for_the_moment_it_was_taken() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let file = dir.path().join("later");
        std::fs::write(&file, b"x")?;
        let before = ProcessProbe.snapshot();
        let _held = std::fs::File::open(&file)?;
        let (raw, id) = identity(&file)?;
        assert!(
            matches!(before.probe(&raw, &id), InUse::No | InUse::Partial { .. }),
            "opened after the snapshot"
        );
        assert!(matches!(ProcessProbe.probe(&raw, &id), InUse::Yes(_)));
        assert!(matches!(
            ProcessProbe.snapshot().probe(&raw, &id),
            InUse::Yes(_)
        ));
        Ok(())
    }

    #[test]
    fn an_unopened_file_is_never_claimed_in_use_or_provably_unused_without_full_visibility()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let file = dir.path().join("idle");
        std::fs::write(&file, b"x")?;
        let (raw, id) = identity(&file)?;
        for answer in [
            ProcessProbe.probe(&raw, &id),
            ProcessProbe.snapshot().probe(&raw, &id),
        ] {
            assert!(
                matches!(answer, InUse::No | InUse::Partial { .. }),
                "{answer:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn a_changed_object_is_unknown() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        std::fs::write(dir.path().join("a"), b"1")?;
        std::fs::write(dir.path().join("b"), b"2")?;
        let (_, a) = identity(&dir.path().join("a"))?;
        let (b, _) = identity(&dir.path().join("b"))?;
        assert!(matches!(ProcessProbe.probe(&b, &a), InUse::Unknown { .. }));
        assert!(matches!(
            ProcessProbe.snapshot().probe(&b, &a),
            InUse::Unknown { .. }
        ));
        let gone =
            RawPath::from_unix_bytes(dir.path().join("gone").as_os_str().as_bytes().to_vec())?;
        assert!(matches!(
            ProcessProbe.probe(&gone, &a),
            InUse::Unknown { .. }
        ));
        Ok(())
    }
}
