// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The secret store over the OS credential store: Keychain on macOS, Credential Manager on Windows
//! and the Secret Service on Linux (through the system libdbus).

use super::blob::{decode, encode};
use crate::domain::{AccountId, StoredAccount};
use crate::ports::{SecretStore, StoreError};
use keyring_core::{CredentialStore, Entry};
use std::sync::Arc;

const SERVICE: &str = "RocketChatQml";
/// The entry with the ids of the accounts, in order, as a JSON list.
const INDEX: &str = "accounts";

fn store_error(error: &keyring_core::Error) -> StoreError {
    match error {
        keyring_core::Error::BadEncoding(_) | keyring_core::Error::BadDataFormat(..) => {
            StoreError::Corrupt(error.to_string())
        }
        _ => StoreError::Unavailable(error.to_string()),
    }
}

/// Keeps each account in a keyring entry of its own, and the order of the accounts in an index
/// entry.
pub struct KeyringStore {
    store: Arc<CredentialStore>,
    service: String,
}

impl KeyringStore {
    /// # Errors
    ///
    /// [`StoreError::Unavailable`] if the platform has no credential store to use.
    pub fn new() -> Result<Self, StoreError> {
        Self::with_service(SERVICE)
    }

    fn with_service(service: &str) -> Result<Self, StoreError> {
        Ok(Self {
            store: native_store()?,
            service: service.to_owned(),
        })
    }

    fn entry(&self, user: &str) -> Result<Entry, StoreError> {
        self.store
            .build(&self.service, user, None)
            .map_err(|e| store_error(&e))
    }

    fn account_entry(id: &str) -> String {
        format!("account:{id}")
    }

    /// The ids of the accounts, in order. Nothing if there is no index yet.
    fn read_index(&self) -> Result<Vec<String>, StoreError> {
        match self.entry(INDEX)?.get_password() {
            Ok(text) => serde_json::from_str(&text).map_err(|e| StoreError::Corrupt(e.to_string())),
            Err(keyring_core::Error::NoEntry) => Ok(Vec::new()),
            Err(error) => Err(store_error(&error)),
        }
    }

    fn write_index(&self, ids: &[String]) -> Result<(), StoreError> {
        let text = serde_json::to_string(ids).map_err(|e| StoreError::Corrupt(e.to_string()))?;
        self.entry(INDEX)?
            .set_password(&text)
            .map_err(|e| store_error(&e))
    }
}

/// The credential store of this platform.
#[cfg(target_os = "linux")]
fn native_store() -> Result<std::sync::Arc<CredentialStore>, StoreError> {
    let store = dbus_secret_service_keyring_store::Store::new().map_err(|e| store_error(&e))?;
    Ok(store)
}

#[cfg(target_os = "macos")]
fn native_store() -> Result<std::sync::Arc<CredentialStore>, StoreError> {
    let store = apple_native_keyring_store::keychain::Store::new().map_err(|e| store_error(&e))?;
    Ok(store)
}

#[cfg(target_os = "windows")]
fn native_store() -> Result<std::sync::Arc<CredentialStore>, StoreError> {
    let store = windows_native_keyring_store::Store::new().map_err(|e| store_error(&e))?;
    Ok(store)
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn native_store() -> Result<std::sync::Arc<CredentialStore>, StoreError> {
    Err(StoreError::Unavailable(
        "no credential store on this platform".to_owned(),
    ))
}

impl SecretStore for KeyringStore {
    fn load(&self) -> Result<Vec<StoredAccount>, StoreError> {
        let mut accounts = Vec::new();
        for id in self.read_index()? {
            // One missing or bad entry doesn't hide the others; a keyring that can't be read fails
            // the load.
            match self.entry(&Self::account_entry(&id))?.get_password() {
                Ok(text) => match decode(&text) {
                    Ok(account) => accounts.push(account),
                    Err(error) => {
                        tracing::warn!(%error, id, "skipping an unreadable stored account");
                    }
                },
                Err(keyring_core::Error::NoEntry) => {
                    tracing::warn!(id, "skipping a stored account that has no entry");
                }
                Err(error) => return Err(store_error(&error)),
            }
        }
        Ok(accounts)
    }

    fn save(&self, account: &StoredAccount) -> Result<(), StoreError> {
        let id = account.id.as_str();
        self.entry(&Self::account_entry(id))?
            .set_password(&encode(account)?)
            .map_err(|e| store_error(&e))?;
        let mut ids = self.read_index()?;
        if !ids.iter().any(|stored| stored == id) {
            ids.push(id.to_owned());
            self.write_index(&ids)?;
        }
        Ok(())
    }

    fn remove(&self, id: &AccountId) -> Result<(), StoreError> {
        let mut ids = self.read_index()?;
        let before = ids.len();
        ids.retain(|stored| stored != id.as_str());
        if ids.len() != before {
            self.write_index(&ids)?;
        }
        match self
            .entry(&Self::account_entry(id.as_str()))?
            .delete_credential()
        {
            Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
            Err(error) => Err(store_error(&error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::process;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::adapters::blob::tests::{account, fields};

    /// Deletes every entry the test may have made, even if an assertion fails.
    struct Cleanup<'a>(&'a KeyringStore);

    impl Drop for Cleanup<'_> {
        fn drop(&mut self) {
            for name in ["a", "b", "c"] {
                let _ = self.0.remove(&account(name).id);
            }
            if let Ok(entry) = self.0.entry(INDEX) {
                let _ = entry.delete_credential();
            }
        }
    }

    // AUTH-004
    //
    // Talks to the real OS keyring, which may prompt the user to unlock it, so it's opt-in:
    // RC_TEST_KEYRING=1 cargo test -p rc_core --lib keyring -- --ignored
    #[test]
    #[ignore = "uses the OS keyring: set RC_TEST_KEYRING=1"]
    fn the_os_keyring_keeps_a_list_of_accounts() {
        assert_eq!(
            std::env::var("RC_TEST_KEYRING").as_deref(),
            Ok("1"),
            "this test uses the OS keyring: set RC_TEST_KEYRING=1"
        );
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let service = format!("RocketChatQml-test-{}-{nanos}", process::id());
        let store = KeyringStore::with_service(&service).unwrap();
        let _cleanup = Cleanup(&store);
        let ids = |store: &KeyringStore| -> Vec<String> {
            store
                .load()
                .unwrap()
                .iter()
                .map(|a| a.id.to_string())
                .collect()
        };

        // Removing from an empty store is fine.
        assert!(store.load().unwrap().is_empty());
        store.remove(&account("a").id).unwrap();

        // Two accounts, in order.
        store.save(&account("a")).unwrap();
        store.save(&account("b")).unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(fields(&loaded[0]), fields(&account("a")));
        assert_eq!(fields(&loaded[1]), fields(&account("b")));

        // Saving an existing id replaces it in place, and a signed-out account keeps no token.
        let mut signed_out = account("a");
        signed_out.token = None;
        store.save(&signed_out).unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(ids(&store), ["id-a", "id-b"]);
        assert_eq!(fields(&loaded[0]), fields(&signed_out));

        // An unreadable entry, and one that's gone, are skipped.
        store
            .entry(&KeyringStore::account_entry("id-c"))
            .unwrap()
            .set_password("not json")
            .unwrap();
        store
            .write_index(&["id-a".to_owned(), "id-c".to_owned(), "id-b".to_owned()])
            .unwrap();
        assert_eq!(ids(&store), ["id-a", "id-b"]);
        store
            .entry(&KeyringStore::account_entry("id-a"))
            .unwrap()
            .delete_credential()
            .unwrap();
        assert_eq!(ids(&store), ["id-b"]);

        // Removing leaves the other, and removing again is fine.
        store.remove(&account("c").id).unwrap();
        store.remove(&account("a").id).unwrap();
        store.remove(&account("a").id).unwrap();
        assert_eq!(ids(&store), ["id-b"]);
    }
}
