//! The Windows [`InUseProbe`] on Restart Manager (roadmap 6.12, research note 04
//! §8).
//!
//! Restart Manager reports the applications and services using registered
//! files; it rejects directories (`ERROR_ACCESS_DENIED`), so for a directory every
//! file beneath it is registered, without following links or reparse points, up
//! to [`MAX_FILES`]. Whether it sees other users' processes without elevation is
//! not documented, so finding nobody is [`InUse::Partial`] with an unknown unseen
//! count, never proof of non-use (ADR-0030).

#![allow(unsafe_code)]

use std::os::windows::ffi::{OsStrExt as _, OsStringExt as _};
use std::path::{Path, PathBuf};

use lumen_application::ports::{DirEnumerator, InUse, InUseProbe};
use lumen_domain::{EntryKind, FileIdentity, Process, RawPath};
use windows_sys::Win32::Foundation::{ERROR_MORE_DATA, ERROR_SUCCESS};
use windows_sys::Win32::System::RestartManager::{
    CCH_RM_SESSION_KEY, RM_PROCESS_INFO, RmEndSession, RmGetList, RmRegisterResources,
    RmStartSession,
};

use crate::entry::filetime;
use crate::handle::HandleEnumerator;
use crate::winpath::{nul_terminated, verbatim};

/// Most files registered for one directory; beyond this the answer is
/// `Unknown` rather than a guess.
pub(crate) const MAX_FILES: usize = 10_000;

/// Asks Restart Manager which processes use an object.
#[derive(Debug, Clone, Copy, Default)]
pub struct RestartManagerProbe;

/// A Restart Manager session, ended on drop.
struct Session(u32);

impl Session {
    fn start() -> Option<Self> {
        let mut handle = 0_u32;
        let mut key = [0_u16; CCH_RM_SESSION_KEY as usize + 1];
        // SAFETY: invariant: `key` holds `CCH_RM_SESSION_KEY + 1` units, as the
        // API requires, and `handle` receives the session. Why unsafe: FFI with
        // out-pointers. Why it holds: both are live locals of the documented size.
        let status = unsafe { RmStartSession(&raw mut handle, 0, key.as_mut_ptr()) };
        (status == ERROR_SUCCESS).then_some(Self(handle))
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // SAFETY: invariant: the session was started and is ended once. Why
        // unsafe: FFI call. Why it holds: only `start` builds a `Session`.
        let ended = unsafe { RmEndSession(self.0) };
        if ended != ERROR_SUCCESS {
            // Nothing useful can be done in `drop`; the session expires with the
            // process.
        }
    }
}

/// Every file under `dir`, without following links or reparse points; `None`
/// if there are more than `MAX_FILES` or a directory cannot be read.
fn files_under(dir: &Path) -> Option<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in std::fs::read_dir(&next).ok()? {
            let entry = entry.ok()?;
            let kind = entry.file_type().ok()?;
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                pending.push(entry.path());
            } else {
                files.push(entry.path());
                if files.len() > MAX_FILES {
                    return None;
                }
            }
        }
    }
    Some(files)
}

/// The processes Restart Manager reports for `files`, or `None` on failure.
fn users_of(files: &[PathBuf]) -> Option<Vec<Process>> {
    let session = Session::start()?;
    let names: Vec<Vec<u16>> = files
        .iter()
        .map(|f| nul_terminated(&verbatim(&f.as_os_str().encode_wide().collect::<Vec<_>>())))
        .collect::<Option<_>>()?;
    let pointers: Vec<*const u16> = names.iter().map(Vec::as_ptr).collect();
    for chunk in pointers.chunks(1000) {
        let count = u32::try_from(chunk.len()).ok()?;
        // SAFETY: invariant: `chunk` holds `count` pointers to NUL-terminated
        // strings that outlive the call; no applications or services are passed.
        // Why unsafe: FFI reading through raw pointers. Why it holds: the strings
        // are owned by `names`, which lives until the end of this function.
        let status = unsafe {
            RmRegisterResources(
                session.0,
                count,
                chunk.as_ptr(),
                0,
                std::ptr::null(),
                0,
                std::ptr::null(),
            )
        };
        if status != ERROR_SUCCESS {
            return None;
        }
    }
    let mut infos: Vec<RM_PROCESS_INFO> = Vec::new();
    for _ in 0..4 {
        let mut needed = 0_u32;
        let mut count = u32::try_from(infos.capacity()).ok()?;
        let mut reasons = 0_u32;
        // SAFETY: invariant: the API writes at most `count` records into the
        // vector's spare capacity and the counts into locals. Why unsafe: FFI
        // writing into uninitialised memory. Why it holds: `count` is the
        // allocated capacity; a zero capacity passes a dangling but unused
        // pointer with a count of 0, which only queries the size.
        let status = unsafe {
            RmGetList(
                session.0,
                &raw mut needed,
                &raw mut count,
                infos.as_mut_ptr(),
                &raw mut reasons,
            )
        };
        match status {
            ERROR_SUCCESS => {
                let filled = usize::try_from(count).ok()?.min(infos.capacity());
                // SAFETY: invariant: the first `filled` records are initialised.
                // Why unsafe: `set_len`. Why it holds: the API reported filling
                // `count` records, bounded by the capacity.
                unsafe { infos.set_len(filled) };
                return Some(
                    infos
                        .iter()
                        .map(|info| {
                            let start = info.Process.ProcessStartTime;
                            let ticks = (i64::from(start.dwHighDateTime) << 32)
                                | i64::from(start.dwLowDateTime);
                            Process {
                                pid: info.Process.dwProcessId,
                                started_at: filetime(ticks),
                                executable: None,
                                executable_identity: None,
                                application: None,
                            }
                        })
                        .collect(),
                );
            }
            ERROR_MORE_DATA => {
                infos = Vec::with_capacity(usize::try_from(needed).ok()? + 8);
            }
            _ => return None,
        }
    }
    None
}

impl InUseProbe for RestartManagerProbe {
    fn probe(&self, path: &RawPath, expected: &FileIdentity) -> InUse {
        let Ok(entry) = HandleEnumerator.stat(path) else {
            return InUse::Unknown {
                reason: "the object cannot be observed",
            };
        };
        if entry.identity() != expected {
            return InUse::Unknown {
                reason: "the object changed since it was observed",
            };
        }
        let Some(wide) = path.to_windows_wide() else {
            return InUse::Unknown {
                reason: "not a Windows path",
            };
        };
        let native = PathBuf::from(std::ffi::OsString::from_wide(&wide));
        let files = match entry.kind {
            EntryKind::File => vec![native],
            EntryKind::Directory => match files_under(&native) {
                Some(files) => files,
                None => {
                    return InUse::Unknown {
                        reason: "too many files, or an unreadable directory, to check",
                    };
                }
            },
            // Links and reparse points are never followed, so nothing is
            // registered for them; their targets are separate objects.
            _ => Vec::new(),
        };
        if files.is_empty() {
            return InUse::Partial { unseen: None };
        }
        match users_of(&files) {
            Some(users) if !users.is_empty() => InUse::Yes(users),
            Some(_) => InUse::Partial { unseen: None },
            None => InUse::Unknown {
                reason: "Restart Manager could not answer",
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use super::*;

    fn raw(path: &Path) -> Result<RawPath, Box<dyn std::error::Error>> {
        let wide: Vec<u16> = path.as_os_str().encode_wide().collect();
        Ok(RawPath::from_windows_wide(&wide)?)
    }

    #[test]
    fn an_open_file_and_its_directory_are_in_use_by_this_process()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let file = dir.path().join("held.txt");
        let mut held = std::fs::File::create(&file)?;
        held.write_all(b"x")?;
        for target in [file.as_path(), dir.path()] {
            let path = raw(target)?;
            let id = HandleEnumerator.stat(&path)?.size.identity;
            let answer = RestartManagerProbe.probe(&path, &id);
            assert!(
                matches!(&answer, InUse::Yes(users) if users.iter().any(|p| p.pid == std::process::id())),
                "{target:?}: {answer:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn an_idle_file_is_partial_and_a_changed_one_unknown() -> Result<(), Box<dyn std::error::Error>>
    {
        let dir = tempfile::tempdir()?;
        std::fs::write(dir.path().join("a"), b"1")?;
        std::fs::write(dir.path().join("b"), b"2")?;
        let a = raw(&dir.path().join("a"))?;
        let a_id = HandleEnumerator.stat(&a)?.size.identity;
        assert_eq!(
            RestartManagerProbe.probe(&a, &a_id),
            InUse::Partial { unseen: None }
        );
        let b = raw(&dir.path().join("b"))?;
        assert!(matches!(
            RestartManagerProbe.probe(&b, &a_id),
            InUse::Unknown { .. }
        ));
        Ok(())
    }
}
