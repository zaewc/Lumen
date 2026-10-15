//! The only `unsafe` code in `lumen-fs-exec` (ADR-0033): Windows system calls
//! behind safe functions.
//!
//! Objects are opened one name at a time relative to a directory handle
//! (`NtCreateFile` with `RootDirectory`), always with `FILE_OPEN_REPARSE_POINT`
//! so a link, junction or placeholder is opened itself and never followed, and
//! with all share modes so Lumen never blocks other programs. Nothing here
//! decides anything; callers compare what these functions report.

#![allow(unsafe_code)]

use std::io;
use std::mem::{MaybeUninit, size_of};

use windows_sys::Wdk::Foundation::OBJECT_ATTRIBUTES;
use windows_sys::Wdk::Storage::FileSystem::{
    FILE_DIRECTORY_FILE, FILE_NON_DIRECTORY_FILE, FILE_OPEN, FILE_OPEN_REPARSE_POINT,
    FILE_SYNCHRONOUS_IO_NONALERT, NtCreateFile,
};
use windows_sys::Win32::Foundation::{
    CloseHandle, HANDLE, INVALID_HANDLE_VALUE, NTSTATUS, OBJ_CASE_INSENSITIVE,
    RtlNtStatusToDosError, UNICODE_STRING,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, DELETE, FILE_ATTRIBUTE_TAG_INFO, FILE_BASIC_INFO, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_ID_INFO, FILE_INFO_BY_HANDLE_CLASS, FILE_READ_ATTRIBUTES,
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_STANDARD_INFO, FILE_TRAVERSE,
    FileAttributeTagInfo, FileBasicInfo, FileIdInfo, FileStandardInfo,
    GetFileInformationByHandleEx, OPEN_EXISTING, SYNCHRONIZE,
};
use windows_sys::Win32::System::IO::IO_STATUS_BLOCK;

/// An open handle, closed on drop.
#[derive(Debug)]
pub(crate) struct Handle(HANDLE);

// SAFETY: invariant: a file handle may be used and closed from any thread. Why
// unsafe: `HANDLE` is a raw pointer, so `Send` is not derived. Why it holds:
// kernel handles are process-wide; nothing here is thread-affine.
unsafe impl Send for Handle {}

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: invariant: the handle is open and owned by `self`. Why unsafe:
        // FFI call. Why it holds: `Handle` is only built from a handle a call
        // here just opened, and nothing else closes it.
        let closed = unsafe { CloseHandle(self.0) };
        if closed == 0 {
            // Nothing useful can be done in `drop`; the handle leaks at worst.
        }
    }
}

/// What an object must be when opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Want {
    /// A directory, to walk through or hold entries.
    Directory,
    /// A directory or file that may be renamed or deleted.
    Movable { directory: bool },
}

/// The error for a failed native call, as a Win32 error code.
fn status_error(status: NTSTATUS) -> io::Error {
    // SAFETY: invariant: none; the call maps one integer to another. Why
    // unsafe: FFI call. Why it holds: it reads no memory of ours.
    let code = unsafe { RtlNtStatusToDosError(status) };
    io::Error::from_raw_os_error(i32::try_from(code).unwrap_or(i32::MAX))
}

/// Opens the directory at the full, verbatim, NUL-terminated `path` itself.
pub(crate) fn open_root(path: &[u16]) -> io::Result<Handle> {
    if path.last() != Some(&0) || path.iter().filter(|&&u| u == 0).count() != 1 {
        return Err(io::Error::from(io::ErrorKind::InvalidInput));
    }
    // SAFETY: invariant: `path` is a NUL-terminated UTF-16 string that outlives
    // the call; security attributes and template are null, which the API
    // allows. Why unsafe: FFI call. Why it holds: checked just above.
    let handle = unsafe {
        CreateFileW(
            path.as_ptr(),
            FILE_TRAVERSE | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        Err(io::Error::last_os_error())
    } else {
        Ok(Handle(handle))
    }
}

/// Opens `name` (one component, no separators) relative to `dir`.
pub(crate) fn open_at(dir: &Handle, name: &[u16], want: Want) -> io::Result<Handle> {
    if name.is_empty()
        || name
            .iter()
            .any(|&u| u == 0 || u == u16::from(b'\\') || u == u16::from(b'/'))
    {
        return Err(io::Error::from(io::ErrorKind::InvalidInput));
    }
    let bytes =
        u16::try_from(name.len() * 2).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    let (access, options) = match want {
        Want::Directory => (
            FILE_TRAVERSE | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
            FILE_DIRECTORY_FILE,
        ),
        Want::Movable { directory } => (
            DELETE | FILE_TRAVERSE | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
            if directory {
                FILE_DIRECTORY_FILE
            } else {
                FILE_NON_DIRECTORY_FILE
            },
        ),
    };
    let unicode = UNICODE_STRING {
        Length: bytes,
        MaximumLength: bytes,
        Buffer: name.as_ptr().cast_mut(),
    };
    let attributes = OBJECT_ATTRIBUTES {
        Length: u32::try_from(size_of::<OBJECT_ATTRIBUTES>()).unwrap_or(u32::MAX),
        RootDirectory: dir.0,
        ObjectName: &raw const unicode,
        Attributes: OBJ_CASE_INSENSITIVE,
        SecurityDescriptor: std::ptr::null(),
        SecurityQualityOfService: std::ptr::null(),
    };
    let mut handle: HANDLE = std::ptr::null_mut();
    let mut io_status = MaybeUninit::<IO_STATUS_BLOCK>::zeroed();
    // SAFETY: invariant: the attributes point to a live `UNICODE_STRING` whose
    // buffer holds `Length` bytes, `RootDirectory` is an open handle, and the
    // out-pointers are writable. Why unsafe: FFI call with raw pointers. Why it
    // holds: `unicode`, `attributes`, `handle` and `io_status` are locals that
    // outlive the call, `name` is borrowed for it, and `dir` is a live `Handle`;
    // the allocation size and EA buffer are null with length 0, as allowed.
    let status = unsafe {
        NtCreateFile(
            &raw mut handle,
            access,
            &raw const attributes,
            io_status.as_mut_ptr(),
            std::ptr::null(),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            FILE_OPEN,
            options | FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
            std::ptr::null(),
            0,
        )
    };
    if status < 0 {
        Err(status_error(status))
    } else {
        Ok(Handle(handle))
    }
}

/// A fixed-size information class and the struct it fills.
trait InfoClass: Copy {
    const CLASS: FILE_INFO_BY_HANDLE_CLASS;
}

impl InfoClass for FILE_ATTRIBUTE_TAG_INFO {
    const CLASS: FILE_INFO_BY_HANDLE_CLASS = FileAttributeTagInfo;
}

impl InfoClass for FILE_ID_INFO {
    const CLASS: FILE_INFO_BY_HANDLE_CLASS = FileIdInfo;
}

impl InfoClass for FILE_BASIC_INFO {
    const CLASS: FILE_INFO_BY_HANDLE_CLASS = FileBasicInfo;
}

impl InfoClass for FILE_STANDARD_INFO {
    const CLASS: FILE_INFO_BY_HANDLE_CLASS = FileStandardInfo;
}

fn query<T: InfoClass>(handle: &Handle) -> io::Result<T> {
    let mut info = MaybeUninit::<T>::uninit();
    let size = u32::try_from(size_of::<T>()).map_err(io::Error::other)?;
    // SAFETY: invariant: the kernel writes at most `size` bytes of `T` into
    // `info`. Why unsafe: FFI call writing through a raw pointer. Why it holds:
    // `T::CLASS` is the information class whose output struct is `T` (each impl
    // above pairs them as documented), and `size` is `size_of::<T>()`.
    let ok =
        unsafe { GetFileInformationByHandleEx(handle.0, T::CLASS, info.as_mut_ptr().cast(), size) };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: invariant: `info` is initialized. Why unsafe: `assume_init`. Why
    // it holds: the call succeeded, and on success it fills the whole struct.
    Ok(unsafe { info.assume_init() })
}

/// What a handle's object is, as the executor compares it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Facts {
    /// Volume serial number.
    pub volume: u64,
    /// 128-bit file ID.
    pub file: u128,
    /// File attributes.
    pub attributes: u32,
    /// Logical size (`EndOfFile`).
    pub size: i64,
    /// Last write time (100 ns since 1601).
    pub modified: i64,
    /// Hard link count.
    pub links: u32,
    /// A directory.
    pub directory: bool,
}

/// Queries what `handle` refers to.
pub(crate) fn facts(handle: &Handle) -> io::Result<Facts> {
    let id: FILE_ID_INFO = query(handle)?;
    let tag: FILE_ATTRIBUTE_TAG_INFO = query(handle)?;
    let basic: FILE_BASIC_INFO = query(handle)?;
    let standard: FILE_STANDARD_INFO = query(handle)?;
    Ok(Facts {
        volume: id.VolumeSerialNumber,
        file: u128::from_le_bytes(id.FileId.Identifier),
        attributes: tag.FileAttributes,
        size: standard.EndOfFile,
        modified: basic.LastWriteTime,
        links: standard.NumberOfLinks,
        directory: standard.Directory,
    })
}
