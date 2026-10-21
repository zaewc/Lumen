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
    CloseHandle, ERROR_NO_MORE_FILES, HANDLE, INVALID_HANDLE_VALUE, LocalFree, WAIT_TIMEOUT,
};
use windows_sys::Win32::Security::Authorization::{GetSecurityInfo, SE_FILE_OBJECT};
use windows_sys::Win32::Security::{
    EqualSid, GetTokenInformation, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID,
    TOKEN_ELEVATION, TOKEN_QUERY, TOKEN_USER, TokenElevation, TokenUser,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_TAG_INFO, FILE_BASIC_INFO, FILE_COMPRESSION_INFO,
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_FLAG_OVERLAPPED, FILE_ID_INFO,
    FILE_INFO_BY_HANDLE_CLASS, FILE_LIST_DIRECTORY, FILE_NOTIFY_CHANGE_ATTRIBUTES,
    FILE_NOTIFY_CHANGE_CREATION, FILE_NOTIFY_CHANGE_DIR_NAME, FILE_NOTIFY_CHANGE_FILE_NAME,
    FILE_NOTIFY_CHANGE_LAST_WRITE, FILE_NOTIFY_CHANGE_SECURITY, FILE_NOTIFY_CHANGE_SIZE,
    FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_STANDARD_INFO,
    FileAttributeTagInfo, FileBasicInfo, FileCompressionInfo, FileIdExtdDirectoryInfo, FileIdInfo,
    FileStandardInfo, GetFileInformationByHandleEx, GetVolumeInformationByHandleW, OPEN_EXISTING,
    READ_CONTROL, ReadDirectoryChangesW, SYNCHRONIZE,
};
use windows_sys::Win32::System::IO::{
    CancelIoEx, GetOverlappedResult, GetOverlappedResultEx, OVERLAPPED,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

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
    /// Watch a directory for changes (overlapped I/O).
    Watch,
    /// Read the object's attributes and owner.
    Owner,
}

/// Opens the object at `path` itself (never a link target).
///
/// # Errors
///
/// Returns the OS error, or `InvalidInput` if the path contains NUL.
pub(crate) fn open(path: &[u16], access: Access) -> io::Result<Handle> {
    let path = nul_terminated(&verbatim(path))
        .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
    let (rights, overlapped) = match access {
        Access::List => (FILE_LIST_DIRECTORY | SYNCHRONIZE, 0),
        Access::Attributes => (FILE_READ_ATTRIBUTES | SYNCHRONIZE, 0),
        Access::Owner => (FILE_READ_ATTRIBUTES | READ_CONTROL | SYNCHRONIZE, 0),
        Access::Watch => (FILE_LIST_DIRECTORY, FILE_FLAG_OVERLAPPED),
    };
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
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT | overlapped,
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

impl InfoClass for FILE_COMPRESSION_INFO {
    const CLASS: FILE_INFO_BY_HANDLE_CLASS = FileCompressionInfo;
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

/// Compressed size of the opened object.
pub(crate) fn compression(handle: &Handle) -> io::Result<FILE_COMPRESSION_INFO> {
    query(handle)
}

/// Name of the filesystem the opened object lives on (`NTFS`, `ReFS`, …).
pub(crate) fn filesystem_name(handle: &Handle) -> io::Result<String> {
    // The documented maximum is MAX_PATH + 1 characters.
    let mut name = [0_u16; 261];
    let size = u32::try_from(name.len()).map_err(io::Error::other)?;
    // SAFETY: invariant: the kernel writes at most `size` UTF-16 units into
    // `name`; every other output pointer is null, which the API allows. Why
    // unsafe: FFI call writing through a raw pointer. Why it holds: `name` is a
    // local array of exactly `size` units that outlives the call.
    let ok = unsafe {
        GetVolumeInformationByHandleW(
            handle.0,
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            name.as_mut_ptr(),
            size,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    let end = name.iter().position(|u| *u == 0).unwrap_or(name.len());
    Ok(String::from_utf16_lossy(&name[..end]))
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

/// An overlapped `ReadDirectoryChangesW` on one directory handle.
///
/// The kernel writes into `buffer` and `overlapped` while a read is pending, so
/// both live on the heap at fixed addresses, and `Drop` cancels a pending read and
/// waits for it to finish before either is freed.
pub(crate) struct Watch {
    handle: Handle,
    overlapped: Box<OVERLAPPED>,
    buffer: Box<[u32]>,
    pending: bool,
}

// SAFETY: invariant: a `Watch` is used by one thread at a time. Why unsafe:
// `OVERLAPPED` holds raw pointers, so `Send` is not derived. Why it holds: the
// feed moves each `Watch` into exactly one watcher thread.
unsafe impl Send for Watch {}

/// How a wait for changes ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Waited {
    /// The read completed with this many bytes (0: the buffer overflowed).
    Done(usize),
    /// Nothing yet.
    TimedOut,
}

impl Watch {
    /// Watches the directory open on `handle` ([`Access::Watch`]).
    pub(crate) fn new(handle: Handle, bytes: usize) -> Self {
        Self {
            handle,
            // SAFETY: invariant: an all-zero `OVERLAPPED` is a valid initial
            // state (no event, offset 0). Why unsafe: `zeroed`. Why it holds: the
            // struct is plain integers, pointers and a union of them.
            overlapped: Box::new(unsafe { std::mem::zeroed() }),
            buffer: vec![0; bytes.div_ceil(4)].into_boxed_slice(),
            pending: false,
        }
    }

    /// Starts reading the next batch of changes in the whole subtree.
    pub(crate) fn arm(&mut self) -> io::Result<()> {
        let size = u32::try_from(self.buffer.len() * 4).map_err(io::Error::other)?;
        let filter = FILE_NOTIFY_CHANGE_FILE_NAME
            | FILE_NOTIFY_CHANGE_DIR_NAME
            | FILE_NOTIFY_CHANGE_ATTRIBUTES
            | FILE_NOTIFY_CHANGE_SIZE
            | FILE_NOTIFY_CHANGE_LAST_WRITE
            | FILE_NOTIFY_CHANGE_CREATION
            | FILE_NOTIFY_CHANGE_SECURITY;
        // SAFETY: invariant: `buffer` (DWORD-aligned, `size` bytes) and
        // `overlapped` stay valid and unmoved until the read completes or is
        // cancelled. Why unsafe: FFI call that writes asynchronously. Why it
        // holds: both are boxed fields of `self`, never reallocated, and `Drop`
        // and `cancel` wait for a pending read before they can be freed.
        let ok = unsafe {
            ReadDirectoryChangesW(
                self.handle.0,
                self.buffer.as_mut_ptr().cast(),
                size,
                1,
                filter,
                std::ptr::null_mut(),
                &raw mut *self.overlapped,
                None,
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        self.pending = true;
        Ok(())
    }

    /// Waits up to `millis` for the pending read.
    pub(crate) fn wait(&mut self, millis: u32) -> io::Result<Waited> {
        let mut bytes = 0_u32;
        // SAFETY: invariant: `overlapped` belongs to the read on this handle.
        // Why unsafe: FFI call. Why it holds: `arm` issued it with these fields.
        let ok = unsafe {
            GetOverlappedResultEx(
                self.handle.0,
                &raw const *self.overlapped,
                &raw mut bytes,
                millis,
                0,
            )
        };
        if ok != 0 {
            self.pending = false;
            return Ok(Waited::Done(
                usize::try_from(bytes).map_err(io::Error::other)?,
            ));
        }
        let error = io::Error::last_os_error();
        if error.raw_os_error() == i32::try_from(WAIT_TIMEOUT).ok() {
            Ok(Waited::TimedOut)
        } else {
            self.pending = false;
            Err(error)
        }
    }

    /// The bytes of the last completed read.
    pub(crate) fn bytes(&self, len: usize) -> &[u8] {
        // SAFETY: invariant: the slice covers at most the buffer's bytes. Why
        // unsafe: reinterpreting memory. Why it holds: `u8` has alignment 1 and no
        // invalid values, and the slice is the buffer's exact byte length; callers
        // only read after `wait` returned `Done`, when the kernel is done writing.
        let all = unsafe {
            std::slice::from_raw_parts(self.buffer.as_ptr().cast::<u8>(), self.buffer.len() * 4)
        };
        &all[..len.min(all.len())]
    }

    /// Cancels a pending read and waits until the kernel has let go of the buffer.
    pub(crate) fn cancel(&mut self) {
        if !self.pending {
            return;
        }
        let mut bytes = 0_u32;
        // SAFETY: invariant: `overlapped` identifies the pending read. Why unsafe:
        // FFI calls. Why it holds: `pending` is only true after a successful
        // `arm`; the blocking `GetOverlappedResult` returns once the read is
        // cancelled or finished, after which the kernel no longer touches it.
        let (_cancelled, _finished) = unsafe {
            (
                CancelIoEx(self.handle.0, &raw const *self.overlapped),
                GetOverlappedResult(
                    self.handle.0,
                    &raw const *self.overlapped,
                    &raw mut bytes,
                    1,
                ),
            )
        };
        self.pending = false;
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        self.cancel();
    }
}

impl std::fmt::Debug for Watch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Watch")
            .field("pending", &self.pending)
            .finish_non_exhaustive()
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

/// Memory a system call allocated with `LocalAlloc`, freed on drop.
struct LocalMemory(*mut core::ffi::c_void);

impl Drop for LocalMemory {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: invariant: the pointer came from a call documented to
            // allocate it with `LocalAlloc`, and nothing else frees it. Why
            // unsafe: FFI call. Why it holds: `LocalMemory` is only built from
            // such out-parameters, once each.
            let _previous = unsafe { LocalFree(self.0) };
        }
    }
}

/// Whether the owner of `handle`'s object (opened with [`Access::Owner`]) is
/// the user of this process's token. An object owned by a group (for example
/// Administrators, the default owner under an elevated token) is not.
pub(crate) fn owned_by_current_user(handle: &Handle) -> io::Result<bool> {
    let mut owner: PSID = std::ptr::null_mut();
    let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    // SAFETY: invariant: the handle is open with `READ_CONTROL` and the
    // out-pointers are writable; unrequested parts are null. Why unsafe: FFI
    // call. Why it holds: `handle` is a live `Handle` and the out-pointers are
    // locals; `owner` points into `descriptor`, which is kept alive below.
    let error = unsafe {
        GetSecurityInfo(
            handle.0,
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &raw mut owner,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &raw mut descriptor,
        )
    };
    if error != 0 {
        return Err(io::Error::from_raw_os_error(
            i32::try_from(error).unwrap_or(i32::MAX),
        ));
    }
    let descriptor = LocalMemory(descriptor);
    let mut token: HANDLE = std::ptr::null_mut();
    // SAFETY: invariant: the out-pointer is writable. Why unsafe: FFI call.
    // Why it holds: `token` is a local; the process pseudo-handle needs no
    // closing.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &raw mut token) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let token = Handle(token);
    let mut needed = 0_u32;
    // SAFETY: invariant: a null buffer of length 0 only asks for the size.
    // Why unsafe: FFI call. Why it holds: the API allows it and writes only
    // `needed`.
    let _sizing = unsafe {
        GetTokenInformation(token.0, TokenUser, std::ptr::null_mut(), 0, &raw mut needed)
    };
    let words = usize::try_from(needed)
        .map_err(io::Error::other)?
        .div_ceil(8);
    let mut buffer = vec![0_u64; words.max(1)];
    let length = u32::try_from(buffer.len() * 8).map_err(io::Error::other)?;
    // SAFETY: invariant: the system writes at most `length` bytes into
    // `buffer`. Why unsafe: FFI call writing through a raw pointer. Why it
    // holds: `buffer` is `length` bytes, 8-byte aligned for `TOKEN_USER`.
    let ok = unsafe {
        GetTokenInformation(
            token.0,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            length,
            &raw mut needed,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: invariant: the buffer starts with a `TOKEN_USER`. Why unsafe:
    // reading a struct through a cast pointer. Why it holds: the call above
    // succeeded with `TokenUser` and the buffer is aligned for it.
    let user = unsafe { buffer.as_ptr().cast::<TOKEN_USER>().read() };
    // SAFETY: invariant: both SIDs are valid. Why unsafe: FFI call. Why it
    // holds: `owner` points into `descriptor` and `user.User.Sid` into
    // `buffer`, both alive here.
    let equal = unsafe { EqualSid(owner, user.User.Sid) } != 0;
    drop(descriptor);
    Ok(equal)
}

/// Whether this process runs with an elevated token (ADR-0005): the
/// command line acts only as the user, never as an administrator.
pub(crate) fn process_is_elevated() -> io::Result<bool> {
    let mut token: HANDLE = std::ptr::null_mut();
    // SAFETY: invariant: the out-pointer is writable. Why unsafe: FFI call.
    // Why it holds: `token` is a local; the process pseudo-handle needs no
    // closing.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &raw mut token) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let token = Handle(token);
    let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
    let length = u32::try_from(size_of::<TOKEN_ELEVATION>()).map_err(io::Error::other)?;
    let mut written = 0_u32;
    // SAFETY: invariant: the system writes at most `length` bytes into
    // `elevation`. Why unsafe: FFI call writing through a raw pointer. Why it
    // holds: `elevation` is a `TOKEN_ELEVATION` of exactly `length` bytes, the
    // structure `TokenElevation` returns.
    let ok = unsafe {
        GetTokenInformation(
            token.0,
            TokenElevation,
            (&raw mut elevation).cast(),
            length,
            &raw mut written,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(elevation.TokenIsElevated != 0)
}

/// Bytes this user may still write on the volume holding the directory
/// `path`, after quotas, and the volume's total size.
pub(crate) fn disk_space(path: &std::path::Path) -> io::Result<(u64, u64)> {
    use std::os::windows::ffi::OsStrExt as _;

    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut available = 0_u64;
    let mut total = 0_u64;
    // SAFETY: invariant: the name is NUL-terminated and the two requested
    // outputs are writable; the third is null, which the API allows. Why
    // unsafe: FFI call. Why it holds: `wide` ends with the NUL chained above
    // and outlives the call; `available` and `total` are local `u64`s.
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &raw mut available,
            &raw mut total,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok((available, total))
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
    fn the_token_elevation_can_be_read() {
        assert!(process_is_elevated().is_ok());
    }

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
