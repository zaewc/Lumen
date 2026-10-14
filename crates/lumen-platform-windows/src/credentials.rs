//! The Windows [`SecretStore`]: generic credentials in Credential Manager
//! (roadmap 8.10c).
//!
//! Each secret is a `CRED_TYPE_GENERIC` credential named `app.lumen/<name>`,
//! persisted for this user on this machine only (`CRED_PERSIST_LOCAL_MACHINE`,
//! never roaming). Values are stored as UTF-8 bytes and never logged. Failures
//! are [`SecretError::Unavailable`], so callers continue without the secret.

#![allow(unsafe_code)]

use std::ptr::{self, NonNull};

use lumen_application::ports::{Secret, SecretError, SecretName, SecretStore};
use windows_sys::Win32::Foundation::{ERROR_NOT_FOUND, GetLastError};
use windows_sys::Win32::Security::Credentials::{
    CRED_MAX_CREDENTIAL_BLOB_SIZE, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW,
    CredDeleteW, CredFree, CredReadW, CredWriteW,
};

/// Prefix of every credential Lumen owns.
pub const TARGET_PREFIX: &str = "app.lumen/";

/// Lumen's secrets in Windows Credential Manager.
#[derive(Debug, Clone)]
pub struct CredentialStore {
    prefix: String,
}

impl Default for CredentialStore {
    fn default() -> Self {
        Self::new()
    }
}

impl CredentialStore {
    /// The store for Lumen's own credentials.
    pub fn new() -> Self {
        Self {
            prefix: TARGET_PREFIX.to_owned(),
        }
    }

    /// A store under another prefix, so tests never touch Lumen's credentials.
    pub fn with_prefix(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
        }
    }

    /// The NUL-terminated UTF-16 target name for `name`.
    fn target(&self, name: SecretName) -> Vec<u16> {
        format!("{}{}", self.prefix, name.as_str())
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect()
    }
}

/// A credential allocated by `CredReadW`, freed exactly once when dropped.
struct Allocated(NonNull<CREDENTIALW>);

impl Allocated {
    /// A copy of the credential's blob.
    fn blob(&self) -> Vec<u8> {
        // SAFETY: invariant: the pointer came from a successful `CredReadW` and
        // is freed only in `drop`. Why unsafe: turning a raw pointer into a
        // reference. Why it holds: `self` owns the allocation, and the reference
        // does not outlive `self`.
        let credential = unsafe { self.0.as_ref() };
        let size = usize::try_from(credential.CredentialBlobSize).unwrap_or(0);
        if credential.CredentialBlob.is_null() || size == 0 {
            return Vec::new();
        }
        // SAFETY: invariant: the blob holds `CredentialBlobSize` bytes. Why
        // unsafe: building a slice from a raw pointer. Why it holds: the system
        // reports the blob's size with the allocation that `self` keeps alive,
        // and the slice is copied before `self` can be dropped.
        unsafe { std::slice::from_raw_parts(credential.CredentialBlob, size) }.to_vec()
    }
}

impl Drop for Allocated {
    fn drop(&mut self) {
        // SAFETY: invariant: the allocation came from `CredReadW` and is freed
        // once. Why unsafe: FFI call. Why it holds: only `get` builds an
        // `Allocated`, from a successful read, and `drop` runs once.
        unsafe { CredFree(self.0.as_ptr().cast_const().cast()) };
    }
}

fn unavailable() -> SecretError {
    SecretError::Unavailable("Credential Manager refused the request")
}

impl SecretStore for CredentialStore {
    fn get(&self, name: SecretName) -> Result<Option<Secret>, SecretError> {
        let target = self.target(name);
        let mut credential: *mut CREDENTIALW = ptr::null_mut();
        // SAFETY: invariant: `target` is NUL-terminated UTF-16 and `credential`
        // a valid out-pointer. Why unsafe: FFI with an out-pointer. Why it holds:
        // on success the system allocates the credential, which is read and then
        // freed exactly once with `CredFree` below.
        let ok = unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &raw mut credential) };
        let Some(credential) = NonNull::new(credential).filter(|_| ok != 0) else {
            // SAFETY: invariant: called right after the failing API on this
            // thread. Why unsafe: FFI call. Why it holds: no other call intervened.
            return if unsafe { GetLastError() } == ERROR_NOT_FOUND {
                Ok(None)
            } else {
                Err(unavailable())
            };
        };
        let credential = Allocated(credential);
        String::from_utf8(credential.blob())
            .map(|text| Some(Secret::new(text)))
            .map_err(|_| SecretError::Corrupt)
    }

    fn set(&self, name: SecretName, secret: &Secret) -> Result<(), SecretError> {
        let mut target = self.target(name);
        let mut blob = secret.expose().as_bytes().to_vec();
        let size = u32::try_from(blob.len())
            .ok()
            .filter(|size| *size <= CRED_MAX_CREDENTIAL_BLOB_SIZE)
            .ok_or(SecretError::Unavailable(
                "the secret is too large for Credential Manager",
            ))?;
        let credential = CREDENTIALW {
            Type: CRED_TYPE_GENERIC,
            TargetName: target.as_mut_ptr(),
            CredentialBlobSize: size,
            CredentialBlob: blob.as_mut_ptr(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            ..CREDENTIALW::default()
        };
        // SAFETY: invariant: `credential` points to buffers (`target`, `blob`)
        // that live until the call returns. Why unsafe: FFI reading through raw
        // pointers. Why it holds: both vectors are locals that outlive the call,
        // and the system copies what it stores.
        let ok = unsafe { CredWriteW(&raw const credential, 0) };
        if ok == 0 { Err(unavailable()) } else { Ok(()) }
    }

    fn delete(&self, name: SecretName) -> Result<bool, SecretError> {
        let target = self.target(name);
        // SAFETY: invariant: `target` is NUL-terminated UTF-16. Why unsafe: FFI
        // call. Why it holds: the buffer outlives the call.
        let ok = unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) };
        if ok != 0 {
            return Ok(true);
        }
        // SAFETY: as in `get`: right after the failing call on this thread.
        if unsafe { GetLastError() } == ERROR_NOT_FOUND {
            Ok(false)
        } else {
            Err(unavailable())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_are_prefixed_and_terminated() {
        let target = CredentialStore::new().target(SecretName::OPENROUTER_API_KEY);
        assert_eq!(target.last(), Some(&0));
        assert_eq!(
            String::from_utf16_lossy(&target[..target.len() - 1]),
            "app.lumen/openrouter-api-key"
        );
    }

    /// Writes to the user's Credential Manager, so it is opt-in locally and run
    /// in CI (`cargo nextest run --run-ignored ignored-only -p lumen-platform-windows`).
    #[test]
    #[ignore = "touches the real Credential Manager"]
    fn round_trips_through_credential_manager() -> Result<(), SecretError> {
        let store = CredentialStore::with_prefix(format!("app.lumen.test.{}/", std::process::id()));
        let name = SecretName::OPENROUTER_API_KEY;
        let cleanup = store.delete(name);
        assert!(cleanup.is_ok(), "{cleanup:?}");
        assert_eq!(store.get(name)?, None);
        store.set(name, &Secret::new("sk-or-first"))?;
        store.set(name, &Secret::new("sk-or-second"))?;
        assert_eq!(
            store.get(name)?.map(|s| s.expose().to_owned()),
            Some("sk-or-second".to_owned())
        );
        assert!(store.delete(name)?);
        assert!(!store.delete(name)?);
        assert_eq!(
            store.set(name, &Secret::new("x".repeat(2561))),
            Err(SecretError::Unavailable(
                "the secret is too large for Credential Manager"
            ))
        );
        Ok(())
    }
}
