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
    FILE_CREATE, FILE_DIRECTORY_FILE, FILE_NON_DIRECTORY_FILE, FILE_OPEN, FILE_OPEN_REPARSE_POINT,
    FILE_RENAME_INFORMATION, FILE_SYNCHRONOUS_IO_NONALERT, FileRenameInformation, NtCreateFile,
    NtSetInformationFile,
};
use windows_sys::Win32::Foundation::{
    CloseHandle, HANDLE, INVALID_HANDLE_VALUE, LocalFree, NTSTATUS, OBJ_CASE_INSENSITIVE,
    RtlNtStatusToDosError, UNICODE_STRING,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertSecurityDescriptorToStringSecurityDescriptorW, ConvertSidToStringSidW,
    ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo, SDDL_REVISION_1,
    SE_FILE_OBJECT,
};
use windows_sys::Win32::Security::{
    DACL_SECURITY_INFORMATION, GetTokenInformation, OWNER_SECURITY_INFORMATION,
    PSECURITY_DESCRIPTOR, TOKEN_QUERY, TOKEN_USER, TokenUser,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, DELETE, FILE_ATTRIBUTE_NOT_CONTENT_INDEXED, FILE_ATTRIBUTE_TAG_INFO,
    FILE_BASIC_INFO, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_WRITE,
    FILE_ID_INFO, FILE_INFO_BY_HANDLE_CLASS, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE,
    FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_STANDARD_INFO, FILE_TRAVERSE, FileAttributeTagInfo,
    FileBasicInfo, FileIdInfo, FileStandardInfo, FlushFileBuffers, GetFileInformationByHandleEx,
    OPEN_EXISTING, READ_CONTROL, SYNCHRONIZE, WriteFile,
};
use windows_sys::Win32::System::IO::IO_STATUS_BLOCK;
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

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
    /// A store directory, whose owner and DACL are checked.
    Store,
}

/// One name: not empty, `.` or `..`, and without separators or NUL.
pub(crate) fn plain_name(name: &[u16]) -> bool {
    !name.is_empty()
        && name != [u16::from(b'.')]
        && name != [u16::from(b'.'), u16::from(b'.')]
        && !name
            .iter()
            .any(|&u| u == 0 || u == u16::from(b'\\') || u == u16::from(b'/'))
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
    if !plain_name(name) {
        return Err(io::Error::from(io::ErrorKind::InvalidInput));
    }
    let bytes =
        u16::try_from(name.len() * 2).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    let (access, options) = match want {
        Want::Directory => (
            FILE_TRAVERSE | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
            FILE_DIRECTORY_FILE,
        ),
        Want::Store => (
            FILE_TRAVERSE | FILE_READ_ATTRIBUTES | READ_CONTROL | SYNCHRONIZE,
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

/// Memory the system allocated with `LocalAlloc`, freed on drop.
struct Local(*mut core::ffi::c_void);

impl Drop for Local {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: invariant: the pointer came from a system call documented
            // to allocate it with `LocalAlloc`, and nothing else frees it. Why
            // unsafe: FFI call. Why it holds: `Local` is only built from such
            // out-parameters, once each.
            let _previous = unsafe { LocalFree(self.0) };
        }
    }
}

/// The UTF-16 units of a NUL-terminated wide string the system returned.
fn wide_until_nul(text: *const u16) -> Vec<u16> {
    let mut out = Vec::new();
    if text.is_null() {
        return out;
    }
    let mut index = 0;
    loop {
        // SAFETY: invariant: `text` points to a NUL-terminated string and
        // `index` has not passed its NUL. Why unsafe: raw pointer read. Why it
        // holds: the callers pass strings the documented APIs return
        // NUL-terminated, and the loop stops at the first NUL.
        let unit = unsafe { *text.add(index) };
        if unit == 0 {
            return out;
        }
        out.push(unit);
        index += 1;
    }
}

/// The current user's SID in string form (`S-1-5-21-…`).
pub(crate) fn current_user_sid() -> io::Result<String> {
    let mut token: HANDLE = std::ptr::null_mut();
    // SAFETY: invariant: the out-pointer is writable. Why unsafe: FFI call.
    // Why it holds: `token` is a local; the pseudo-handle from
    // `GetCurrentProcess` needs no closing.
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
    // succeeded with `TokenUser`, and the buffer is aligned for it.
    let user = unsafe { buffer.as_ptr().cast::<TOKEN_USER>().read() };
    let mut text: *mut u16 = std::ptr::null_mut();
    // SAFETY: invariant: `Sid` points into `buffer`, which is alive; the
    // out-pointer is writable. Why unsafe: FFI call. Why it holds: `buffer`
    // outlives the call and `text` is a local.
    if unsafe { ConvertSidToStringSidW(user.User.Sid, &raw mut text) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let text = Local(text.cast());
    String::from_utf16(&wide_until_nul(text.0.cast())).map_err(io::Error::other)
}

/// Creates the directory `name` in `dir` with the security descriptor
/// `sddl`, not content-indexed, and opens it for walking and inspection.
pub(crate) fn create_dir_at(dir: &Handle, name: &[u16], sddl: &str) -> io::Result<Handle> {
    let mut sddl: Vec<u16> = sddl.encode_utf16().collect();
    sddl.push(0);
    let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    // SAFETY: invariant: `sddl` is NUL-terminated and the out-pointer is
    // writable; the size out-pointer may be null. Why unsafe: FFI call. Why it
    // holds: both are locals that outlive the call.
    let ok = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &raw mut descriptor,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    let descriptor = Local(descriptor);
    create_with(dir, name, descriptor.0)
}

/// `NtCreateFile(FILE_CREATE)` of a directory with `descriptor`.
fn create_with(dir: &Handle, name: &[u16], descriptor: PSECURITY_DESCRIPTOR) -> io::Result<Handle> {
    if !plain_name(name) {
        return Err(io::Error::from(io::ErrorKind::InvalidInput));
    }
    let bytes =
        u16::try_from(name.len() * 2).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
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
        SecurityDescriptor: descriptor.cast_const().cast(),
        SecurityQualityOfService: std::ptr::null(),
    };
    let mut handle: HANDLE = std::ptr::null_mut();
    let mut io_status = MaybeUninit::<IO_STATUS_BLOCK>::zeroed();
    // SAFETY: as in `open_at`; additionally `descriptor` is a live,
    // self-relative security descriptor owned by the caller for the call.
    let status = unsafe {
        NtCreateFile(
            &raw mut handle,
            FILE_TRAVERSE | FILE_READ_ATTRIBUTES | READ_CONTROL | SYNCHRONIZE,
            &raw const attributes,
            io_status.as_mut_ptr(),
            std::ptr::null(),
            FILE_ATTRIBUTE_NOT_CONTENT_INDEXED,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            FILE_CREATE,
            FILE_DIRECTORY_FILE | FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
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

/// The owner and DACL of a self-relative descriptor in SDDL form, with the
/// system's own aliases for well-known SIDs.
fn descriptor_sddl(descriptor: PSECURITY_DESCRIPTOR) -> io::Result<String> {
    let which = OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION;
    let mut text: *mut u16 = std::ptr::null_mut();
    let mut length = 0_u32;
    // SAFETY: invariant: `descriptor` is a live security descriptor; the
    // out-pointers are writable. Why unsafe: FFI call. Why it holds: callers
    // pass a descriptor they hold for the call, and the rest are locals.
    let ok = unsafe {
        ConvertSecurityDescriptorToStringSecurityDescriptorW(
            descriptor,
            SDDL_REVISION_1,
            which,
            &raw mut text,
            &raw mut length,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    let text = Local(text.cast());
    String::from_utf16(&wide_until_nul(text.0.cast())).map_err(io::Error::other)
}

/// `sddl` as the system prints it back (aliases such as `LA` or `SY` for
/// well-known SIDs), so it compares equal to [`owner_and_dacl`].
pub(crate) fn canonical_sddl(sddl: &str) -> io::Result<String> {
    let mut wide: Vec<u16> = sddl.encode_utf16().collect();
    wide.push(0);
    let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    // SAFETY: invariant: `wide` is NUL-terminated and the out-pointer is
    // writable; the size out-pointer may be null. Why unsafe: FFI call. Why it
    // holds: both are locals that outlive the call.
    let ok = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            wide.as_ptr(),
            SDDL_REVISION_1,
            &raw mut descriptor,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    let descriptor = Local(descriptor);
    descriptor_sddl(descriptor.0)
}

/// The owner and DACL of `handle`'s object in SDDL form.
pub(crate) fn owner_and_dacl(handle: &Handle) -> io::Result<String> {
    let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    let which = OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION;
    // SAFETY: invariant: the handle is open with `READ_CONTROL`, and the
    // out-pointers are writable or null. Why unsafe: FFI call. Why it holds:
    // `handle` is a live `Handle`; the owner/group/DACL/SACL pointers are not
    // requested separately (null), only the whole descriptor.
    let error = unsafe {
        GetSecurityInfo(
            handle.0,
            SE_FILE_OBJECT,
            which,
            std::ptr::null_mut(),
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
    let descriptor = Local(descriptor);
    descriptor_sddl(descriptor.0)
}

/// Renames the object of `item` to `name` in `dir`, never replacing anything
/// (`NtSetInformationFile(FileRenameInformation)` with `ReplaceIfExists =
/// FALSE` and `RootDirectory = dir`, ADR-0033). The Win32 wrapper
/// (`SetFileInformationByHandle`) rejected a `RootDirectory` with
/// `ERROR_INVALID_PARAMETER` on the Windows runner, so the native call is used.
pub(crate) fn rename_into(item: &Handle, dir: &Handle, name: &[u16]) -> io::Result<()> {
    if !plain_name(name) {
        return Err(io::Error::from(io::ErrorKind::InvalidInput));
    }
    let header = std::mem::offset_of!(FILE_RENAME_INFORMATION, FileName);
    let name_bytes = name.len() * 2;
    // The system requires at least the whole struct (whose one-unit `FileName`
    // and padding then hold the terminator) plus the name.
    let total = size_of::<FILE_RENAME_INFORMATION>() + name_bytes;
    // `u64` storage keeps the handle field aligned.
    let mut buffer = vec![0_u64; total.div_ceil(8)];
    let info = buffer.as_mut_ptr().cast::<FILE_RENAME_INFORMATION>();
    let length = u32::try_from(name_bytes).map_err(io::Error::other)?;
    // SAFETY: invariant: `buffer` holds at least `total` zeroed, 8-byte
    // aligned bytes: a `FILE_RENAME_INFORMATION` header followed by room for the
    // name and a NUL. Why unsafe: writing through raw pointers into it. Why it
    // holds: `total` is the struct size plus the name, which covers the
    // header offset, the name and a terminator, and `buffer` covers `total`.
    unsafe {
        (*info).Anonymous.ReplaceIfExists = false;
        (*info).RootDirectory = dir.0;
        (*info).FileNameLength = length;
        std::ptr::copy_nonoverlapping(
            name.as_ptr().cast::<u8>(),
            buffer.as_mut_ptr().cast::<u8>().add(header),
            name_bytes,
        );
    }
    let size = u32::try_from(total).map_err(io::Error::other)?;
    let mut io_status = MaybeUninit::<IO_STATUS_BLOCK>::zeroed();
    // SAFETY: invariant: the buffer is a complete `FILE_RENAME_INFORMATION` of
    // `size` bytes, both handles are open, and the status block is writable.
    // Why unsafe: FFI call. Why it holds: filled just above; `item` and `dir`
    // are live `Handle`s and `io_status` is a local.
    let status = unsafe {
        NtSetInformationFile(
            item.0,
            io_status.as_mut_ptr(),
            buffer.as_ptr().cast(),
            size,
            FileRenameInformation,
        )
    };
    if status < 0 {
        Err(status_error(status))
    } else {
        Ok(())
    }
}

/// Creates the file `name` in `dir` (never an existing one), writes all of
/// `bytes` and flushes it. It inherits the directory's DACL.
pub(crate) fn create_file_at(dir: &Handle, name: &[u16], bytes: &[u8]) -> io::Result<()> {
    if !plain_name(name) {
        return Err(io::Error::from(io::ErrorKind::InvalidInput));
    }
    let name_bytes =
        u16::try_from(name.len() * 2).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    let unicode = UNICODE_STRING {
        Length: name_bytes,
        MaximumLength: name_bytes,
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
    // SAFETY: as in `open_at`; `FILE_CREATE` fails if the name exists.
    let status = unsafe {
        NtCreateFile(
            &raw mut handle,
            FILE_GENERIC_WRITE | SYNCHRONIZE,
            &raw const attributes,
            io_status.as_mut_ptr(),
            std::ptr::null(),
            FILE_ATTRIBUTE_NOT_CONTENT_INDEXED,
            FILE_SHARE_READ,
            FILE_CREATE,
            FILE_NON_DIRECTORY_FILE | FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
            std::ptr::null(),
            0,
        )
    };
    if status < 0 {
        return Err(status_error(status));
    }
    let file = Handle(handle);
    let mut rest = bytes;
    while !rest.is_empty() {
        let chunk = u32::try_from(rest.len()).unwrap_or(u32::MAX);
        let mut written = 0_u32;
        // SAFETY: invariant: the system reads at most `chunk` bytes from
        // `rest` and writes `written`. Why unsafe: FFI call with raw pointers.
        // Why it holds: `chunk <= rest.len()`, both are live locals, and the
        // handle is synchronous (no `OVERLAPPED`).
        let ok = unsafe {
            WriteFile(
                file.0,
                rest.as_ptr(),
                chunk,
                &raw mut written,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        if written == 0 {
            return Err(io::Error::from(io::ErrorKind::WriteZero));
        }
        rest = rest
            .get(usize::try_from(written).unwrap_or(rest.len())..)
            .unwrap_or_default();
    }
    // SAFETY: invariant: the handle is open with write access. Why unsafe:
    // FFI call. Why it holds: opened just above with `FILE_GENERIC_WRITE`.
    if unsafe { FlushFileBuffers(file.0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
