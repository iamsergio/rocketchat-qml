// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The secret store over the OS credential store: Keychain on macOS, Credential Manager on Windows
//! and the Secret Service on Linux (through the system libdbus).

use keyring_core::{CredentialStore, Entry};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};

use crate::domain::{Credentials, StoredSession};
use crate::ports::{SecretStore, StoreError};

const SERVICE: &str = "RocketChatQml";
const USER: &str = "session";

/// What goes into the keyring entry, as JSON.
#[derive(Serialize, Deserialize)]
struct Blob {
    server: String,
    user_id: String,
    token: String,
}

fn encode(session: &StoredSession) -> Result<String, StoreError> {
    serde_json::to_string(&Blob {
        server: session.server.clone(),
        user_id: session.credentials.user_id.as_str().to_owned(),
        token: session.credentials.token.expose_secret().to_owned(),
    })
    .map_err(|e| StoreError::Corrupt(e.to_string()))
}

fn decode(text: &str) -> Result<StoredSession, StoreError> {
    let blob: Blob = serde_json::from_str(text).map_err(|e| StoreError::Corrupt(e.to_string()))?;
    Ok(StoredSession {
        server: blob.server,
        credentials: Credentials {
            user_id: blob.user_id.into(),
            token: SecretString::from(blob.token),
        },
    })
}

fn store_error(error: &keyring_core::Error) -> StoreError {
    match error {
        keyring_core::Error::BadEncoding(_) | keyring_core::Error::BadDataFormat(..) => {
            StoreError::Corrupt(error.to_string())
        }
        _ => StoreError::Unavailable(error.to_string()),
    }
}

/// Keeps the session in one keyring entry.
pub struct KeyringStore {
    entry: Entry,
}

impl KeyringStore {
    /// # Errors
    ///
    /// [`StoreError::Unavailable`] if the platform has no credential store to use.
    pub fn new() -> Result<Self, StoreError> {
        Self::with_service(SERVICE)
    }

    fn with_service(service: &str) -> Result<Self, StoreError> {
        let entry = native_store()?
            .build(service, USER, None)
            .map_err(|e| store_error(&e))?;
        Ok(Self { entry })
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
    fn load(&self) -> Result<Option<StoredSession>, StoreError> {
        match self.entry.get_password() {
            Ok(text) => decode(&text).map(Some),
            Err(keyring_core::Error::NoEntry) => Ok(None),
            Err(error) => Err(store_error(&error)),
        }
    }

    fn save(&self, session: &StoredSession) -> Result<(), StoreError> {
        self.entry
            .set_password(&encode(session)?)
            .map_err(|e| store_error(&e))
    }

    fn clear(&self) -> Result<(), StoreError> {
        match self.entry.delete_credential() {
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

    fn session() -> StoredSession {
        StoredSession {
            server: "https://chat.example.com".to_owned(),
            credentials: Credentials {
                user_id: "uid".to_owned().into(),
                token: SecretString::from("tok"),
            },
        }
    }

    // AUTH-004
    #[test]
    fn the_blob_round_trips() {
        let text = encode(&session()).unwrap();
        let back = decode(&text).unwrap();
        assert_eq!(back.server, "https://chat.example.com");
        assert_eq!(back.credentials.user_id.as_str(), "uid");
        assert_eq!(back.credentials.token.expose_secret(), "tok");
    }

    // AUTH-004
    #[test]
    fn a_corrupt_blob_is_reported() {
        for text in ["", "not json", r#"{"server":"x"}"#] {
            assert!(
                matches!(decode(text), Err(StoreError::Corrupt(_))),
                "{text:?}"
            );
        }
    }

    /// Deletes the entry even if an assertion fails.
    struct Cleanup<'a>(&'a KeyringStore);

    impl Drop for Cleanup<'_> {
        fn drop(&mut self) {
            let _ = self.0.clear();
        }
    }

    // AUTH-004
    //
    // Talks to the real OS keyring, which may prompt the user to unlock it, so it's opt-in:
    // RC_TEST_KEYRING=1 cargo test -p rc_core --lib keyring -- --ignored
    #[test]
    #[ignore = "uses the OS keyring: set RC_TEST_KEYRING=1"]
    fn the_os_keyring_round_trips() {
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

        assert!(store.load().unwrap().is_none());
        store.save(&session()).unwrap();
        let loaded = store.load().unwrap().unwrap();
        assert_eq!(loaded.server, "https://chat.example.com");
        assert_eq!(loaded.credentials.token.expose_secret(), "tok");
        store.clear().unwrap();
        assert!(store.load().unwrap().is_none());
    }
}
