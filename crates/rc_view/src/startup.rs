// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! What a frontend needs to start the core, apart from its event callback: the secret store the
//! environment asks for, and the directory for downloaded files.

use std::path::PathBuf;

use std::ffi::OsString;

use rc_core::adapters::{
    GhProcess, MemoryGithubStore, MemoryStore, default_github_store, default_store,
};
use rc_core::github::{DEFAULT_API_URL, GithubStore};
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

/// The store of the GitHub login, which follows `RC_SECRET_STORE` like the accounts' does: memory
/// for `memory`, the keyring otherwise. A plain file isn't offered for it: `file:` gets memory,
/// and a log line saying so.
#[must_use]
pub fn github_store() -> Box<dyn GithubStore> {
    let value = std::env::var(STORE_ENV).ok();
    match store_choice(value.as_deref()) {
        StoreChoice::Memory => {
            tracing::info!("GitHub store: memory ({STORE_ENV}=memory)");
            Box::new(MemoryGithubStore::default())
        }
        StoreChoice::File(_) => {
            tracing::info!("GitHub store: memory ({STORE_ENV}=file:... has none for GitHub)");
            Box::new(MemoryGithubStore::default())
        }
        StoreChoice::Keyring => {
            tracing::info!("GitHub store: OS keyring, falling back to memory");
            default_github_store()
        }
    }
}

/// Environment variable naming the API's address, instead of GitHub's. Developer-mode builds only.
#[cfg_attr(not(feature = "developer-mode"), allow(dead_code))]
const GITHUB_API_ENV: &str = "RC_GITHUB_API";

/// Environment variable naming the program to run in place of `gh`, which a GUI test points at a
/// script. Developer-mode builds only.
#[cfg_attr(not(feature = "developer-mode"), allow(dead_code))]
const GH_PROGRAM_ENV: &str = "RC_GH_PROGRAM";

/// Where the GitHub service talks, and which program stands for the GitHub CLI.
pub struct GithubSetup {
    pub api_url: String,
    pub gh: GhProcess,
}

/// GitHub's address and the `gh` found on the `PATH`; in developer-mode builds, `RC_GITHUB_API`
/// and `RC_GH_PROGRAM` replace them, so that a GUI test never reaches the real GitHub nor runs the
/// user's `gh`.
#[must_use]
pub fn github_setup() -> GithubSetup {
    #[cfg(feature = "developer-mode")]
    {
        github_setup_from(
            std::env::var(GITHUB_API_ENV).ok(),
            std::env::var_os(GH_PROGRAM_ENV),
        )
    }
    #[cfg(not(feature = "developer-mode"))]
    {
        github_setup_from(None, None)
    }
}

fn github_setup_from(api: Option<String>, gh: Option<OsString>) -> GithubSetup {
    let api_url = api.filter(|url| !url.is_empty());
    if let Some(url) = &api_url {
        tracing::info!(%url, "{GITHUB_API_ENV}: GitHub's API is at another address");
    }
    let gh = gh.filter(|program| !program.is_empty());
    GithubSetup {
        api_url: api_url.unwrap_or_else(|| DEFAULT_API_URL.to_owned()),
        gh: gh.map_or_else(GhProcess::default, |program| {
            tracing::info!(program = ?program, "{GH_PROGRAM_ENV}: another program stands for gh");
            GhProcess::with_program(program, Vec::new())
        }),
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

    // GH-009
    #[test]
    fn github_talks_to_github_unless_told_otherwise() {
        assert_eq!(github_setup_from(None, None).api_url, DEFAULT_API_URL);
        assert_eq!(
            github_setup_from(Some(String::new()), None).api_url,
            DEFAULT_API_URL
        );
        assert_eq!(
            github_setup_from(Some("http://127.0.0.1:9".to_owned()), None).api_url,
            "http://127.0.0.1:9"
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
