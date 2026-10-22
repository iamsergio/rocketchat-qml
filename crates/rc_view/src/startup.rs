// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! What a frontend needs to start the core, apart from its event callback: the secret store the
//! environment asks for, and the directory for downloaded files.

use std::path::PathBuf;

use rc_core::adapters::{MemoryStore, default_store};
use rc_core::ports::SecretStore;

/// Environment variable choosing where the session token is kept. `memory` keeps it in memory
/// only, and `file:<path>` in a plain file (GUI tests, in builds with `test-file-store`); anything
/// else, or unset, uses the OS keyring.
const STORE_ENV: &str = "RC_SECRET_STORE";

/// Where `RC_SECRET_STORE` says the session is kept.
#[derive(Debug, PartialEq, Eq)]
enum StoreChoice {
    Keyring,
    Memory,
    File(PathBuf),
}

/// Reads `RC_SECRET_STORE` (`value`, if set).
fn store_choice(value: Option<&str>) -> StoreChoice {
    match value {
        Some("memory") => StoreChoice::Memory,
        Some(value) => value
            .strip_prefix("file:")
            .map_or(StoreChoice::Keyring, |path| StoreChoice::File(path.into())),
        None => StoreChoice::Keyring,
    }
}

/// The plain-file store, which only builds with `test-file-store` have.
#[cfg(feature = "test-file-store")]
fn file_store(path: PathBuf) -> Box<dyn SecretStore> {
    tracing::info!(path = %path.display(), "secret store: plain file ({STORE_ENV}=file:...)");
    Box::new(rc_core::adapters::FileStore::new(path))
}

/// Without the feature the request can't be met. Never the keyring: whoever asked for a file
/// didn't want the user's real keyring touched.
#[cfg(not(feature = "test-file-store"))]
fn file_store(_path: PathBuf) -> Box<dyn SecretStore> {
    tracing::warn!("{STORE_ENV}=file:... needs a build with test-file-store: using memory");
    Box::new(MemoryStore::default())
}

/// The secret store that `RC_SECRET_STORE` asks for, logging which one it is.
#[must_use]
pub fn secret_store() -> Box<dyn SecretStore> {
    let value = std::env::var(STORE_ENV).ok();
    match store_choice(value.as_deref()) {
        StoreChoice::Memory => {
            tracing::info!("secret store: memory ({STORE_ENV}=memory)");
            Box::new(MemoryStore::default())
        }
        StoreChoice::File(path) => file_store(path),
        StoreChoice::Keyring => {
            tracing::info!("secret store: OS keyring, falling back to memory");
            default_store()
        }
    }
}

/// Where downloads go when the host gives no directory.
fn fallback_cache_dir() -> PathBuf {
    std::env::temp_dir().join("rocketchat-qml-cache")
}

/// The directory for downloaded files: `given`, or the temporary directory (with a warning) if it
/// is empty.
#[must_use]
pub fn cache_dir(given: &str) -> PathBuf {
    let dir = if given.is_empty() {
        tracing::warn!("no cache directory given, using the temporary directory");
        fallback_cache_dir()
    } else {
        PathBuf::from(given)
    };
    tracing::info!(dir = %dir.display(), "file cache");
    dir
}

#[cfg(test)]
mod tests {
    use super::*;

    // AUTH-004
    #[test]
    fn memory_selects_the_memory_store() {
        assert_eq!(store_choice(Some("memory")), StoreChoice::Memory);
    }

    // AUTH-004
    #[test]
    fn file_selects_the_file_store_with_its_path() {
        assert_eq!(
            store_choice(Some("file:/tmp/session.json")),
            StoreChoice::File("/tmp/session.json".into())
        );
        // Everything after the first colon is the path.
        assert_eq!(
            store_choice(Some("file:C:\\tmp\\s.json")),
            StoreChoice::File("C:\\tmp\\s.json".into())
        );
    }

    // AUTH-004
    #[test]
    fn anything_else_selects_the_keyring() {
        for value in [
            Some("keyring"),
            Some(""),
            Some("file"),
            Some("Memory"),
            None,
        ] {
            assert_eq!(store_choice(value), StoreChoice::Keyring, "{value:?}");
        }
    }
}
