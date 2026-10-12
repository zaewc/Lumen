//! The only `unsafe` code in the macOS adapter: three `getattrlist`-family system
//! calls that fill a caller-owned buffer. Everything they return is parsed by the
//! safe, bounds-checked [`attrbuf`](crate::attrbuf) module.
//!
//! All calls request `FSOPT_ATTR_CMN_EXTENDED` (the fork group carries
//! `ATTR_CMNEXT_*`) and never `FSOPT_PACK_INVAL_ATTRS` (ADR-0028's research note).
//! Lookups add `FSOPT_NOFOLLOW`: a final symlink is described, never followed.

#![allow(unsafe_code)]

use std::ffi::{CStr, c_ulong};
use std::fs::File;
use std::io;
use std::os::fd::AsRawFd as _;

use crate::attrbuf::AttributeSet;

fn attrlist(set: AttributeSet) -> libc::attrlist {
    libc::attrlist {
        bitmapcount: libc::ATTR_BIT_MAP_COUNT,
        reserved: 0,
        commonattr: set.common,
        volattr: set.volume,
        dirattr: set.directory,
        fileattr: set.file,
        forkattr: set.extended,
    }
}

/// Retries a call interrupted by a signal; any other failure is returned.
fn retry(mut call: impl FnMut() -> libc::c_int) -> io::Result<libc::c_int> {
    loop {
        let result = call();
        if result >= 0 {
            return Ok(result);
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
}

/// Reads the next batch of `dir`'s entries into `buffer` and returns how many it
/// holds; 0 means the listing is finished.
pub(crate) fn read_batch(dir: &File, set: AttributeSet, buffer: &mut [u8]) -> io::Result<usize> {
    let mut list = attrlist(set);
    let count = retry(|| {
        // SAFETY: invariant: the kernel writes at most `buffer.len()` bytes into
        // `buffer` and only reads `list`. Why unsafe: FFI call. Why it holds: the
        // descriptor is borrowed from a live `File`, `list` is a fully initialised
        // `attrlist` on this stack frame, and the pointer and length both come from
        // the same exclusive `&mut [u8]`, which outlives the call.
        unsafe {
            libc::getattrlistbulk(
                dir.as_raw_fd(),
                (&raw mut list).cast(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                u64::from(libc::FSOPT_ATTR_CMN_EXTENDED),
            )
        }
    })?;
    usize::try_from(count).map_err(io::Error::other)
}

/// Reads the attributes of `name` inside `dir` into `buffer` (one group), without
/// following a final symlink.
pub(crate) fn lookup_at(
    dir: &File,
    name: &CStr,
    set: AttributeSet,
    buffer: &mut [u8],
) -> io::Result<()> {
    let mut list = attrlist(set);
    let options = c_ulong::from(libc::FSOPT_NOFOLLOW | libc::FSOPT_ATTR_CMN_EXTENDED);
    retry(|| {
        // SAFETY: invariant: the kernel reads the NUL-terminated `name` and `list`
        // and writes at most `buffer.len()` bytes into `buffer`. Why unsafe: FFI
        // call. Why it holds: `name` is a `&CStr` (terminated, no interior NUL),
        // the descriptor comes from a live `File`, `list` is initialised on this
        // frame, and the pointer and length come from one exclusive `&mut [u8]`.
        unsafe {
            libc::getattrlistat(
                dir.as_raw_fd(),
                name.as_ptr(),
                (&raw mut list).cast(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                options,
            )
        }
    })
    .map(drop)
}

/// Reads the attributes of the object at `path` into `buffer` (one group),
/// without following a final symlink.
pub(crate) fn lookup(path: &CStr, set: AttributeSet, buffer: &mut [u8]) -> io::Result<()> {
    let mut list = attrlist(set);
    retry(|| {
        // SAFETY: invariant: the kernel reads the NUL-terminated `path` and `list`
        // and writes at most `buffer.len()` bytes into `buffer`. Why unsafe: FFI
        // call. Why it holds: `path` is a `&CStr`, `list` is initialised on this
        // frame, and the pointer and length come from one exclusive `&mut [u8]`.
        unsafe {
            libc::getattrlist(
                path.as_ptr(),
                (&raw mut list).cast(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                libc::FSOPT_NOFOLLOW | libc::FSOPT_ATTR_CMN_EXTENDED,
            )
        }
    })
    .map(drop)
}
