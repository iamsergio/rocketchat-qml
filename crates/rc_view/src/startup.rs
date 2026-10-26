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

/// Environment variable for the GUI tests: paths (separated as `PATH` is) that the attach button
/// stages instead of opening the system file dialog, which a test can't drive. Only builds with
/// `test-attach-files` (the Spix ones) read it.
#[cfg_attr(not(feature = "test-attach-files"), allow(dead_code))]
const ATTACH_ENV: &str = "RC_TEST_ATTACH_FILES";

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

/// The paths in `value` (`RC_TEST_ATTACH_FILES`), if set.
#[cfg_attr(not(feature = "test-attach-files"), allow(dead_code))]
fn attachments_in(value: Option<&std::ffi::OsStr>) -> Vec<PathBuf> {
    value.map_or_else(Vec::new, |value| std::env::split_paths(value).collect())
}

/// The files the GUI tests ask the attach button to take: see [`ATTACH_ENV`]. Always empty without
/// `test-attach-files`, which doesn't even read the environment.
#[cfg(feature = "test-attach-files")]
#[must_use]
pub fn test_attachments() -> Vec<PathBuf> {
    attachments_in(std::env::var_os(ATTACH_ENV).as_deref())
}

/// Without the feature a user's environment means nothing.
#[cfg(not(feature = "test-attach-files"))]
#[must_use]
pub const fn test_attachments() -> Vec<PathBuf> {
    Vec::new()
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

    // ACT-009
    #[test]
    fn the_test_attachments_are_the_paths_of_the_variable() {
        assert!(attachments_in(None).is_empty());
        let value = std::env::join_paths(["/tmp/a.txt", "/tmp/b c.png"]).unwrap();
        assert_eq!(
            attachments_in(Some(&value)),
            [PathBuf::from("/tmp/a.txt"), PathBuf::from("/tmp/b c.png")]
        );
    }

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
