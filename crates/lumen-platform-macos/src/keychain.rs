//! The macOS [`SecretStore`]: generic-password items in the user's keychain
//! (roadmap 8.10b), through the Security framework's `SecItem` API.
//!
//! Items use service `app.lumen` and the secret's fixed name as the account.
//! Values are stored as UTF-8 bytes and never logged. A locked keychain or a
//! denied access is [`SecretError::Unavailable`], so callers continue without
//! the secret (for Jev: without Jev).

#![allow(unsafe_code)]

use std::ptr;

use lumen_application::ports::{Secret, SecretError, SecretName, SecretStore};
use objc2_core_foundation::{CFBoolean, CFData, CFDictionary, CFRetained, CFString, CFType};
use objc2_security::{
    SecItemAdd, SecItemCopyMatching, SecItemDelete, SecItemUpdate, errSecAuthFailed,
    errSecDuplicateItem, errSecInteractionNotAllowed, errSecItemNotFound, errSecSuccess,
    kSecAttrAccount, kSecAttrService, kSecClass, kSecClassGenericPassword, kSecMatchLimit,
    kSecMatchLimitOne, kSecReturnData, kSecValueData,
};

/// The keychain service all of Lumen's items use.
pub const SERVICE: &str = "app.lumen";

/// Lumen's secrets in the user's keychain.
#[derive(Debug, Clone)]
pub struct KeychainStore {
    service: String,
}

impl Default for KeychainStore {
    fn default() -> Self {
        Self::new()
    }
}

impl KeychainStore {
    /// The store for Lumen's own service.
    pub fn new() -> Self {
        Self {
            service: SERVICE.to_owned(),
        }
    }

    /// A store under another service name, so tests never touch Lumen's items.
    pub fn with_service(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }
}

/// The Security framework's key constants.
struct Keys {
    class: &'static CFString,
    generic_password: &'static CFString,
    service: &'static CFString,
    account: &'static CFString,
    value_data: &'static CFString,
    return_data: &'static CFString,
    match_limit: &'static CFString,
    match_limit_one: &'static CFString,
}

fn keys() -> Keys {
    // SAFETY: invariant: reading immutable extern statics. Why unsafe: Rust
    // cannot see that the Security framework initialises them before use. Why
    // it holds: they are constant CFStrings exported by the linked framework
    // and never written.
    unsafe {
        Keys {
            class: kSecClass,
            generic_password: kSecClassGenericPassword,
            service: kSecAttrService,
            account: kSecAttrAccount,
            value_data: kSecValueData,
            return_data: kSecReturnData,
            match_limit: kSecMatchLimit,
            match_limit_one: kSecMatchLimitOne,
        }
    }
}

/// A query identifying one item, plus `extra` key/value pairs.
fn query(
    service: &str,
    name: SecretName,
    extra: &[(&CFString, &CFType)],
) -> CFRetained<CFDictionary<CFString, CFType>> {
    let k = keys();
    let service = CFString::from_str(service);
    let account = CFString::from_str(name.as_str());
    let mut pairs: Vec<(&CFString, &CFType)> = vec![
        (k.class, k.generic_password.as_ref()),
        (k.service, service.as_ref()),
        (k.account, account.as_ref()),
    ];
    pairs.extend_from_slice(extra);
    let (keys, values): (Vec<&CFString>, Vec<&CFType>) = pairs.into_iter().unzip();
    CFDictionary::from_slices(&keys, &values)
}

/// The error for a failed `SecItem` call.
fn unavailable(status: i32) -> SecretError {
    SecretError::Unavailable(match status {
        s if s == errSecInteractionNotAllowed => "the keychain is locked",
        s if s == errSecAuthFailed => "keychain access was denied",
        _ => "the keychain refused the request",
    })
}

impl SecretStore for KeychainStore {
    fn get(&self, name: SecretName) -> Result<Option<Secret>, SecretError> {
        let k = keys();
        let query = query(
            &self.service,
            name,
            &[
                (k.return_data, CFBoolean::new(true).as_ref()),
                (k.match_limit, k.match_limit_one.as_ref()),
            ],
        );
        let mut raw: *const CFType = ptr::null();
        // SAFETY: invariant: `query` is a live dictionary of CFString keys and CF
        // values, and `raw` a valid out-pointer. Why unsafe: FFI with an
        // out-pointer. Why it holds: on success the call stores a +1 reference
        // in `raw`, adopted exactly once below; on failure `raw` stays null.
        let status = unsafe { SecItemCopyMatching(query.as_opaque(), &raw mut raw) };
        if status == errSecItemNotFound {
            return Ok(None);
        }
        let Some(raw) = ptr::NonNull::new(raw.cast_mut()).filter(|_| status == errSecSuccess)
        else {
            return Err(unavailable(status));
        };
        // SAFETY: invariant: `raw` is an owned +1 reference. Why unsafe: adopting
        // a raw CF reference. Why it holds: the Copy rule; nothing else releases
        // it.
        let value: CFRetained<CFType> = unsafe { CFRetained::from_raw(raw) };
        let data = value.downcast_ref::<CFData>().ok_or(SecretError::Corrupt)?;
        let text = String::from_utf8(data.to_vec()).map_err(|_| SecretError::Corrupt)?;
        Ok(Some(Secret::new(text)))
    }

    fn set(&self, name: SecretName, secret: &Secret) -> Result<(), SecretError> {
        let k = keys();
        let data = CFData::from_bytes(secret.expose().as_bytes());
        let update =
            CFDictionary::<CFString, CFType>::from_slices(&[k.value_data], &[data.as_ref()]);
        let existing = query(&self.service, name, &[]);
        // SAFETY: invariant: both dictionaries are live, with CFString keys. Why
        // unsafe: FFI call. Why it holds: the references outlive the call.
        let status = unsafe { SecItemUpdate(existing.as_opaque(), update.as_opaque()) };
        if status == errSecSuccess {
            return Ok(());
        }
        if status != errSecItemNotFound {
            return Err(unavailable(status));
        }
        let attributes = query(&self.service, name, &[(k.value_data, data.as_ref())]);
        // SAFETY: invariant: `attributes` is a live dictionary and no result is
        // requested. Why unsafe: FFI call. Why it holds: the result out-pointer
        // is null, so nothing is written back.
        let status = unsafe { SecItemAdd(attributes.as_opaque(), ptr::null_mut()) };
        match status {
            s if s == errSecSuccess => Ok(()),
            // Added concurrently between the update and the add: update again.
            s if s == errSecDuplicateItem => {
                // SAFETY: as for the first update above.
                let retry = unsafe { SecItemUpdate(existing.as_opaque(), update.as_opaque()) };
                if retry == errSecSuccess {
                    Ok(())
                } else {
                    Err(unavailable(retry))
                }
            }
            s => Err(unavailable(s)),
        }
    }

    fn delete(&self, name: SecretName) -> Result<bool, SecretError> {
        let query = query(&self.service, name, &[]);
        // SAFETY: invariant: `query` is a live dictionary identifying one item.
        // Why unsafe: FFI call. Why it holds: the reference outlives the call.
        let status = unsafe { SecItemDelete(query.as_opaque()) };
        match status {
            s if s == errSecSuccess => Ok(true),
            s if s == errSecItemNotFound => Ok(false),
            s => Err(unavailable(s)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queries_identify_one_generic_password_item() {
        let q = query("app.lumen.test", SecretName::OPENROUTER_API_KEY, &[]);
        let k = keys();
        let text = |key: &CFString| {
            crate::cf::get::<CFString>(q.as_opaque(), key).map(ToString::to_string)
        };
        assert_eq!(text(k.class), Some(k.generic_password.to_string()));
        assert_eq!(text(k.service).as_deref(), Some("app.lumen.test"));
        assert_eq!(text(k.account).as_deref(), Some("openrouter-api-key"));
        assert_eq!(q.len(), 3);
    }

    #[test]
    fn errors_name_the_reason() {
        assert_eq!(
            unavailable(errSecInteractionNotAllowed),
            SecretError::Unavailable("the keychain is locked")
        );
        assert_eq!(
            unavailable(-1),
            SecretError::Unavailable("the keychain refused the request")
        );
    }

    /// Writes to the login keychain, so it is opt-in locally and run in CI
    /// (`cargo nextest run --run-ignored ignored-only -p lumen-platform-macos`).
    #[test]
    #[ignore = "touches the real login keychain"]
    fn round_trips_through_the_real_keychain() -> Result<(), SecretError> {
        let store = KeychainStore::with_service(format!("app.lumen.test.{}", std::process::id()));
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
        assert_eq!(store.get(name)?, None);
        Ok(())
    }
}
