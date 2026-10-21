// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! A secret store that lives as long as the process: for tests, and as the fallback when there's
//! no keyring.

use std::sync::{Arc, Mutex, MutexGuard};

use crate::domain::{AccountId, StoredAccount};
use crate::ports::{SecretStore, StoreError};

/// Clones share the same content, so two clients can be given "the same disk" to simulate a restart.
#[derive(Clone, Default)]
pub struct MemoryStore(Arc<Mutex<Vec<StoredAccount>>>);

impl MemoryStore {
    fn lock(&self) -> Result<MutexGuard<'_, Vec<StoredAccount>>, StoreError> {
        self.0
            .lock()
            .map_err(|_| StoreError::Unavailable("the in-memory store is poisoned".to_owned()))
    }
}

impl SecretStore for MemoryStore {
    fn load(&self) -> Result<Vec<StoredAccount>, StoreError> {
        Ok(self.lock()?.clone())
    }

    fn save(&self, account: &StoredAccount) -> Result<(), StoreError> {
        let mut accounts = self.lock()?;
        match accounts.iter_mut().find(|stored| stored.id == account.id) {
            Some(stored) => *stored = account.clone(),
            None => accounts.push(account.clone()),
        }
        Ok(())
    }

    fn remove(&self, id: &AccountId) -> Result<(), StoreError> {
        self.lock()?.retain(|stored| &stored.id != id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::blob::tests::{account, fields};

    fn ids(store: &MemoryStore) -> Vec<String> {
        let accounts = store.load().unwrap();
        accounts.iter().map(|a| a.id.to_string()).collect()
    }

    // AUTH-004
    #[test]
    fn two_accounts_round_trip_in_order() {
        let store = MemoryStore::default();
        assert!(store.load().unwrap().is_empty());
        store.save(&account("a")).unwrap();
        store.save(&account("b")).unwrap();
        // A clone sees the same content.
        let loaded = store.clone().load().unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(fields(&loaded[0]), fields(&account("a")));
        assert_eq!(fields(&loaded[1]), fields(&account("b")));
    }

    // AUTH-004
    #[test]
    fn saving_an_existing_id_replaces_it_in_place() {
        let store = MemoryStore::default();
        store.save(&account("a")).unwrap();
        store.save(&account("b")).unwrap();
        let mut changed = account("a");
        changed.username = "renamed".to_owned();
        store.save(&changed).unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(ids(&store), ["id-a", "id-b"]);
        assert_eq!(loaded[0].username, "renamed");
    }

    // AUTH-004
    #[test]
    fn removing_leaves_the_other_account() {
        let store = MemoryStore::default();
        store.save(&account("a")).unwrap();
        store.save(&account("b")).unwrap();
        store.remove(&account("a").id).unwrap();
        assert_eq!(ids(&store), ["id-b"]);
    }

    // AUTH-004
    #[test]
    fn removing_an_absent_id_is_fine() {
        let store = MemoryStore::default();
        store.remove(&account("a").id).unwrap();
        store.save(&account("b")).unwrap();
        store.remove(&account("a").id).unwrap();
        assert_eq!(ids(&store), ["id-b"]);
    }

    // AUTH-004
    #[test]
    fn an_account_without_a_token_round_trips() {
        let store = MemoryStore::default();
        let mut signed_out = account("a");
        signed_out.token = None;
        store.save(&signed_out).unwrap();
        assert_eq!(fields(&store.load().unwrap()[0]), fields(&signed_out));
    }
}
