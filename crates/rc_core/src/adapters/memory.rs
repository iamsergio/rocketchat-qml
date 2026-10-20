// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! A secret store that lives as long as the process: for tests, and as the fallback when there's
//! no keyring.

use std::sync::{Arc, Mutex, MutexGuard};

use crate::domain::StoredSession;
use crate::ports::{SecretStore, StoreError};

/// Clones share the same content, so two clients can be given "the same disk" to simulate a restart.
#[derive(Clone, Default)]
pub struct MemoryStore(Arc<Mutex<Option<StoredSession>>>);

impl MemoryStore {
    fn lock(&self) -> Result<MutexGuard<'_, Option<StoredSession>>, StoreError> {
        self.0
            .lock()
            .map_err(|_| StoreError::Unavailable("the in-memory store is poisoned".to_owned()))
    }
}

impl SecretStore for MemoryStore {
    fn load(&self) -> Result<Option<StoredSession>, StoreError> {
        Ok(self.lock()?.clone())
    }

    fn save(&self, session: &StoredSession) -> Result<(), StoreError> {
        *self.lock()? = Some(session.clone());
        Ok(())
    }

    fn clear(&self) -> Result<(), StoreError> {
        *self.lock()? = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use secrecy::{ExposeSecret, SecretString};

    use super::*;
    use crate::domain::Credentials;

    // AUTH-004
    #[test]
    fn saves_loads_and_clears() {
        let store = MemoryStore::default();
        assert!(store.load().unwrap().is_none());
        store
            .save(&StoredSession {
                server: "http://h".to_owned(),
                credentials: Credentials {
                    user_id: "uid".to_owned().into(),
                    token: SecretString::from("tok"),
                },
            })
            .unwrap();
        // A clone sees the same content.
        let loaded = store.clone().load().unwrap().unwrap();
        assert_eq!(loaded.server, "http://h");
        assert_eq!(loaded.credentials.token.expose_secret(), "tok");
        store.clear().unwrap();
        assert!(store.load().unwrap().is_none());
    }

    // AUTH-004
    #[test]
    fn clearing_an_empty_store_is_fine() {
        MemoryStore::default().clear().unwrap();
    }
}
