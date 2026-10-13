//! Read-only registry access: the only `unsafe` code for the Windows inventories.
//! Keys are opened with `KEY_READ` and closed on drop; values are read with
//! `RegGetValueW`, which checks the value type and NUL-terminates strings.

#![allow(unsafe_code)]

use windows_sys::Win32::Foundation::{
    ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA, ERROR_NO_MORE_ITEMS, ERROR_SUCCESS,
};
use windows_sys::Win32::System::Registry::{
    HKEY, KEY_READ, REG_SAM_FLAGS, RRF_NOEXPAND, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ, RegCloseKey,
    RegEnumKeyExW, RegGetValueW, RegOpenKeyExW,
};

use crate::winpath::nul_terminated;

/// Why a key could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Missing {
    /// The key does not exist.
    NotFound,
    /// It exists but could not be read (access denied or another error).
    Unreadable,
}

/// An open registry key, closed on drop.
#[derive(Debug)]
pub(crate) struct Key(HKEY);

impl Drop for Key {
    fn drop(&mut self) {
        // SAFETY: invariant: the key is open and owned by `self`. Why unsafe:
        // FFI call. Why it holds: `open` only builds a `Key` from a successfully
        // opened handle, and nothing else closes it.
        let closed = unsafe { RegCloseKey(self.0) };
        if closed != ERROR_SUCCESS {
            // Nothing useful can be done in `drop`; the handle leaks at worst.
        }
    }
}

fn wide(text: &str) -> Option<Vec<u16>> {
    nul_terminated(&text.encode_utf16().collect::<Vec<_>>())
}

/// Opens `root\path` for reading in the given registry view
/// (`KEY_WOW64_64KEY`, `KEY_WOW64_32KEY` or 0).
pub(crate) fn open(root: HKEY, path: &str, view: REG_SAM_FLAGS) -> Result<Key, Missing> {
    let path = wide(path).ok_or(Missing::Unreadable)?;
    let mut key: HKEY = std::ptr::null_mut();
    // SAFETY: invariant: `path` is NUL-terminated and `key` receives a handle
    // only on success. Why unsafe: FFI with an out-pointer. Why it holds: both
    // are locals that outlive the call; the handle is adopted only on success.
    let status = unsafe { RegOpenKeyExW(root, path.as_ptr(), 0, KEY_READ | view, &raw mut key) };
    match status {
        ERROR_SUCCESS => Ok(Key(key)),
        ERROR_FILE_NOT_FOUND => Err(Missing::NotFound),
        _ => Err(Missing::Unreadable),
    }
}

impl Key {
    /// The names of the direct subkeys. Errors other than the end of the list
    /// make the result `Err` with the names read so far.
    pub(crate) fn subkeys(&self) -> Result<Vec<String>, Vec<String>> {
        let mut names = Vec::new();
        // Key names are at most 255 characters.
        let mut buffer = [0_u16; 256];
        for index in 0.. {
            let mut len = u32::try_from(buffer.len()).unwrap_or(0);
            // SAFETY: invariant: the API writes at most `len` UTF-16 units into
            // `buffer` and the length back into `len`; the other out-pointers may
            // be null. Why unsafe: FFI writing through raw pointers. Why it holds:
            // `len` is the buffer's capacity and both are locals.
            let status = unsafe {
                RegEnumKeyExW(
                    self.0,
                    index,
                    buffer.as_mut_ptr(),
                    &raw mut len,
                    std::ptr::null(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            };
            match status {
                ERROR_SUCCESS => {
                    let len = usize::try_from(len).unwrap_or(0).min(buffer.len());
                    names.push(String::from_utf16_lossy(&buffer[..len]));
                }
                ERROR_NO_MORE_ITEMS => return Ok(names),
                _ => return Err(names),
            }
        }
        Err(names)
    }

    /// A string value of `subkey` (`REG_SZ` or unexpanded `REG_EXPAND_SZ`), or
    /// `None` if it is missing, another type, or unreadable.
    pub(crate) fn string(&self, subkey: &str, value: &str) -> Option<String> {
        let subkey = wide(subkey)?;
        let value = wide(value)?;
        let flags = RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ | RRF_NOEXPAND;
        let mut data = vec![0_u16; 260];
        loop {
            let mut bytes = u32::try_from(data.len() * 2).ok()?;
            // SAFETY: invariant: the API writes at most `bytes` bytes into `data`
            // and the size back into `bytes`; the type out-pointer may be null.
            // Why unsafe: FFI writing through raw pointers. Why it holds: `bytes`
            // is the buffer's size in bytes and all pointers are to locals.
            let status = unsafe {
                RegGetValueW(
                    self.0,
                    subkey.as_ptr(),
                    value.as_ptr(),
                    flags,
                    std::ptr::null_mut(),
                    data.as_mut_ptr().cast(),
                    &raw mut bytes,
                )
            };
            match status {
                ERROR_SUCCESS => {
                    let units = usize::try_from(bytes).ok()? / 2;
                    let text = &data[..units.min(data.len())];
                    let end = text.iter().position(|u| *u == 0).unwrap_or(text.len());
                    return Some(String::from_utf16_lossy(&text[..end]));
                }
                ERROR_MORE_DATA if data.len() < 64 * 1024 => {
                    let needed = usize::try_from(bytes).ok()? / 2 + 1;
                    data.resize(needed.max(data.len() * 2), 0);
                }
                _ => return None,
            }
        }
    }
}
