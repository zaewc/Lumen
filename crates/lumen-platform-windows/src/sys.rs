//! The only `unsafe` code in the Windows adapter: opening handles, querying
//! `GetFileInformationByHandleEx`, and the thread placeholder mode. Directory
//! buffers are parsed by the safe, bounds-checked [`dirinfo`](crate::dirinfo)
//! module.
//!
//! Every object is opened with `FILE_FLAG_OPEN_REPARSE_POINT` (a link, junction
//! or placeholder is opened itself, never followed), `FILE_FLAG_BACKUP_SEMANTICS`
//! (required for directories), all share modes (Lumen never blocks other
//! programs), and no data access rights: only `FILE_LIST_DIRECTORY` or
//! `FILE_READ_ATTRIBUTES`, plus `SYNCHRONIZE`.

#![allow(unsafe_code)]

use std::io;
use std::mem::{MaybeUninit, size_of};

use windows_sys::Wdk::Storage::FileSystem::RtlSetThreadPlaceholderCompatibilityMode;
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_NO_MORE_FILES, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_TAG_INFO, FILE_BASIC_INFO, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_ID_INFO, FILE_INFO_BY_HANDLE_CLASS, FILE_LIST_DIRECTORY,
    FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_STANDARD_INFO,
    FileAttributeTagInfo, FileBasicInfo, FileIdExtdDirectoryInfo, FileIdInfo, FileStandardInfo,
    GetFileInformationByHandleEx, OPEN_EXISTING, SYNCHRONIZE,
};

use crate::winpath::{nul_terminated, verbatim};

/// An open handle, closed on drop.
#[derive(Debug)]
pub(crate) struct Handle(HANDLE);

// SAFETY: invariant: a Win32 file handle may be used and closed from any
// thread. Why unsafe: `HANDLE` is a raw pointer, so `Send` is not derived. Why it
// holds: kernel handles are process-wide; nothing here is thread-affine.
unsafe impl Send for Handle {}

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: invariant: the handle is open and owned by `self`. Why unsafe:
        // FFI call. Why it holds: `open` only builds a `Handle` from a valid handle
        // and nothing else closes it.
        let closed = unsafe { CloseHandle(self.0) };
        if closed == 0 {
            // Nothing useful can be done in `drop`; the handle leaks at worst.
        }
    }
}

/// What an opened object is used for.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Access {
    /// List a directory's entries.
    List,
    /// Read the object's own attributes.
    Attributes,
}

/// Opens the object at `path` itself (never a link target).
///
/// # Errors
///
/// Returns the OS error, or `InvalidInput` if the path contains NUL.
pub(crate) fn open(path: &[u16], access: Access) -> io::Result<Handle> {
    let path = nul_terminated(&verbatim(path))
        .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
    let rights = match access {
        Access::List => FILE_LIST_DIRECTORY,
        Access::Attributes => FILE_READ_ATTRIBUTES,
    } | SYNCHRONIZE;
    // SAFETY: invariant: `path` is a NUL-terminated UTF-16 string that outlives
    // the call; the security attributes and template are null, which the API
    // allows. Why unsafe: FFI call. Why it holds: `path` is a local `Vec` with a
    // pushed NUL, and the other arguments are plain integers.
    let handle = unsafe {
        CreateFileW(
            path.as_ptr(),
            rights,
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
    // SAFETY: invariant: `info` is initialised. Why unsafe: `assume_init`. Why it
    // holds: the call succeeded, and on success it fills the whole fixed-size
    // struct; every field is a plain integer, bool or byte array.
    Ok(unsafe { info.assume_init() })
}

/// Attributes and reparse tag of the opened object.
pub(crate) fn attribute_tag(handle: &Handle) -> io::Result<FILE_ATTRIBUTE_TAG_INFO> {
    query(handle)
}

/// Volume serial number and 128-bit file ID of the opened object.
pub(crate) fn id(handle: &Handle) -> io::Result<FILE_ID_INFO> {
    query(handle)
}

/// Times and attributes of the opened object.
pub(crate) fn basic(handle: &Handle) -> io::Result<FILE_BASIC_INFO> {
    query(handle)
}

/// Sizes and link count of the opened object.
pub(crate) fn standard(handle: &Handle) -> io::Result<FILE_STANDARD_INFO> {
    query(handle)
}

/// A buffer for directory records, 8-byte aligned as their `i64` fields need.
#[derive(Debug)]
pub(crate) struct RecordBuffer(Vec<u64>);

impl RecordBuffer {
    /// A buffer of `bytes` bytes (rounded up to 8).
    pub(crate) fn new(bytes: usize) -> Self {
        Self(vec![0; bytes.div_ceil(8)])
    }

    /// The buffer as bytes, for the parser.
    pub(crate) fn bytes(&self) -> &[u8] {
        // SAFETY: invariant: the slice covers exactly the `u64`s owned by `self`.
        // Why unsafe: reinterpreting memory. Why it holds: `u8` has alignment 1
        // and no invalid bit patterns, the length is the same allocation's size
        // in bytes, and the borrow ties the slice to `&self`.
        unsafe { std::slice::from_raw_parts(self.0.as_ptr().cast::<u8>(), self.0.len() * 8) }
    }
}

/// Reads the next batch of directory records into `buffer`. Returns `false` when
/// the listing is finished.
pub(crate) fn read_batch(dir: &Handle, buffer: &mut RecordBuffer) -> io::Result<bool> {
    let size = u32::try_from(buffer.0.len() * 8).map_err(io::Error::other)?;
    // SAFETY: invariant: the kernel writes at most `size` bytes into the buffer.
    // Why unsafe: FFI call writing through a raw pointer. Why it holds: `size` is
    // the byte length of the exclusively borrowed, 8-byte aligned allocation.
    let ok = unsafe {
        GetFileInformationByHandleEx(
            dir.0,
            FileIdExtdDirectoryInfo,
            buffer.0.as_mut_ptr().cast(),
            size,
        )
    };
    if ok != 0 {
        return Ok(true);
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == i32::try_from(ERROR_NO_MORE_FILES).ok() {
        Ok(false)
    } else {
        Err(error)
    }
}

/// `PHCM_EXPOSE_PLACEHOLDERS`.
const PHCM_EXPOSE_PLACEHOLDERS: i8 = 2;

/// The current thread's placeholder compatibility mode (negative on error).
#[cfg(test)]
pub(crate) fn thread_placeholder_mode() -> i8 {
    // SAFETY: invariant: none; no arguments, no memory of ours. Why unsafe: FFI
    // call. Why it holds: documented for Windows 10 1709 and later.
    unsafe { windows_sys::Wdk::Storage::FileSystem::RtlQueryThreadPlaceholderCompatibilityMode() }
}

/// While alive, the current thread sees cloud placeholders as what they are
/// (reparse points with recall attributes) instead of disguised as ordinary
/// files, so the scanner can classify and avoid them. The previous mode is
/// restored on drop.
#[derive(Debug)]
pub(crate) struct PlaceholdersExposed {
    previous: i8,
}

impl PlaceholdersExposed {
    /// Exposes placeholders for this thread.
    ///
    /// # Errors
    ///
    /// Returns an error if the mode cannot be set; callers must then not touch the
    /// filesystem.
    pub(crate) fn enter() -> io::Result<Self> {
        // SAFETY: invariant: none; takes one integer. Why unsafe: FFI call. Why it
        // holds: the argument is a documented `PHCM_*` value.
        let previous =
            unsafe { RtlSetThreadPlaceholderCompatibilityMode(PHCM_EXPOSE_PLACEHOLDERS) };
        if previous < 0 {
            return Err(io::Error::other(
                "cannot expose cloud placeholders on this thread",
            ));
        }
        Ok(Self { previous })
    }
}

impl Drop for PlaceholdersExposed {
    fn drop(&mut self) {
        // SAFETY: invariant: none; takes one integer. Why unsafe: FFI call. Why it
        // holds: `previous` was returned by the same API, so it is a valid mode.
        let restored = unsafe { RtlSetThreadPlaceholderCompatibilityMode(self.previous) };
        if restored < 0 {
            // The thread keeps placeholders exposed: the safe direction.
        }
    }
}

#[cfg(test)]
mod tests {
    use std::mem::offset_of;
    use std::os::windows::ffi::OsStrExt as _;

    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_DIRECTORY, FILE_ID_EXTD_DIR_INFO,
    };

    use super::*;
    use crate::dirinfo::{HEADER, records};

    #[test]
    fn the_parser_layout_matches_the_sdk() {
        assert_eq!(offset_of!(FILE_ID_EXTD_DIR_INFO, NextEntryOffset), 0);
        assert_eq!(offset_of!(FILE_ID_EXTD_DIR_INFO, CreationTime), 8);
        assert_eq!(offset_of!(FILE_ID_EXTD_DIR_INFO, EndOfFile), 40);
        assert_eq!(offset_of!(FILE_ID_EXTD_DIR_INFO, AllocationSize), 48);
        assert_eq!(offset_of!(FILE_ID_EXTD_DIR_INFO, FileAttributes), 56);
        assert_eq!(offset_of!(FILE_ID_EXTD_DIR_INFO, FileNameLength), 60);
        assert_eq!(offset_of!(FILE_ID_EXTD_DIR_INFO, ReparsePointTag), 68);
        assert_eq!(offset_of!(FILE_ID_EXTD_DIR_INFO, FileId), 72);
        assert_eq!(offset_of!(FILE_ID_EXTD_DIR_INFO, FileName), HEADER);
    }

    fn wide(path: &std::path::Path) -> Vec<u16> {
        path.as_os_str().encode_wide().collect()
    }

    #[test]
    fn lists_a_directory_by_handle() -> io::Result<()> {
        let dir = tempfile::tempdir()?;
        std::fs::write(dir.path().join("a.txt"), b"hello")?;
        std::fs::create_dir(dir.path().join("sub"))?;
        let handle = open(&wide(dir.path()), Access::List)?;
        let tag = attribute_tag(&handle)?;
        assert_ne!(tag.FileAttributes & FILE_ATTRIBUTE_DIRECTORY, 0);
        assert_ne!(id(&handle)?.VolumeSerialNumber, 0);

        let mut buffer = RecordBuffer::new(64 * 1024);
        let mut names = Vec::new();
        while read_batch(&handle, &mut buffer)? {
            for record in records(buffer.bytes()) {
                let record = record.map_err(io::Error::other)?;
                names.push(String::from_utf16_lossy(&record.name));
            }
        }
        names.sort();
        assert_eq!(names, [".", "..", "a.txt", "sub"]);

        let file = open(&wide(&dir.path().join("a.txt")), Access::Attributes)?;
        assert_eq!(standard(&file)?.EndOfFile, 5);
        assert!(basic(&file)?.LastWriteTime > 0);
        Ok(())
    }

    #[test]
    fn missing_paths_fail_to_open() -> io::Result<()> {
        let dir = tempfile::tempdir()?;
        let missing = open(&wide(&dir.path().join("nope")), Access::Attributes);
        assert_eq!(
            missing.map(drop).map_err(|e| e.kind()),
            Err(io::ErrorKind::NotFound)
        );
        Ok(())
    }

    #[test]
    fn the_guard_exposes_placeholders_and_restores_the_thread_mode() -> io::Result<()> {
        std::thread::spawn(|| -> io::Result<()> {
            let before = thread_placeholder_mode();
            {
                let _guard = PlaceholdersExposed::enter()?;
                assert_eq!(thread_placeholder_mode(), PHCM_EXPOSE_PLACEHOLDERS);
            }
            assert_eq!(thread_placeholder_mode(), before);
            Ok(())
        })
        .join()
        .map_err(|_| io::Error::other("test thread panicked"))?
    }
}
