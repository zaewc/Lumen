//! An in-memory [`SecretStore`] for tests: nothing touches a real keychain.

use std::collections::BTreeMap;
use std::sync::{Mutex, PoisonError};

use lumen_application::ports::{Secret, SecretError, SecretName, SecretStore};

/// Secrets kept in memory for the life of the value.
#[derive(Debug, Default)]
pub struct MemorySecretStore {
    secrets: Mutex<BTreeMap<SecretName, Secret>>,
}

impl SecretStore for MemorySecretStore {
    fn get(&self, name: SecretName) -> Result<Option<Secret>, SecretError> {
        let secrets = self.secrets.lock().unwrap_or_else(PoisonError::into_inner);
        Ok(secrets.get(&name).cloned())
    }

    fn set(&self, name: SecretName, secret: &Secret) -> Result<(), SecretError> {
        let mut secrets = self.secrets.lock().unwrap_or_else(PoisonError::into_inner);
        secrets.insert(name, secret.clone());
        Ok(())
    }

    fn delete(&self, name: SecretName) -> Result<bool, SecretError> {
        let mut secrets = self.secrets.lock().unwrap_or_else(PoisonError::into_inner);
        Ok(secrets.remove(&name).is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_replaces_and_deletes() -> Result<(), SecretError> {
        let store = MemorySecretStore::default();
        let name = SecretName::OPENROUTER_API_KEY;
        assert_eq!(store.get(name)?, None);
        store.set(name, &Secret::new("a"))?;
        store.set(name, &Secret::new("b"))?;
        assert_eq!(
            store.get(name)?.map(|s| s.expose().to_owned()),
            Some("b".to_owned())
        );
        assert!(store.delete(name)?);
        assert!(!store.delete(name)?);
        assert_eq!(store.get(name)?, None);
        Ok(())
    }
}
