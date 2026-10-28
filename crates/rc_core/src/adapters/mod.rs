// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Implementations of the ports for real I/O.

mod blob;
mod clock;
mod disk_cache;
// The test store writes the token in clear text: it only exists with its feature, and in unit tests.
#[cfg(any(test, feature = "test-file-store"))]
mod file;
mod github;
mod keyring;
mod memory;
mod message_store;
mod rest;

pub use clock::SystemClock;
pub use disk_cache::DiskFileCache;
#[cfg(any(test, feature = "test-file-store"))]
pub use file::FileStore;
pub use github::{GhProcess, RestGithub, RestGithubConnector};
pub use keyring::{KeyringGithubStore, KeyringStore};
pub use memory::{MemoryGithubStore, MemoryStore};
pub use message_store::MemoryMessageStore;
pub use rest::RestConnector;

use crate::github::GithubStore;
use crate::ports::SecretStore;

/// The OS keyring, or memory (the session is then forgotten at exit) if there's none.
#[must_use]
pub fn default_store() -> Box<dyn SecretStore> {
    match KeyringStore::new() {
        Ok(store) => Box::new(store),
        Err(error) => {
            tracing::warn!(%error, "no keyring, the session won't be remembered");
            Box::new(MemoryStore::default())
        }
    }
}

/// The OS keyring for the GitHub login, or memory (it is then forgotten at exit) if there's none.
#[must_use]
pub fn default_github_store() -> Box<dyn GithubStore> {
    match KeyringGithubStore::new() {
        Ok(store) => Box::new(store),
        Err(error) => {
            tracing::warn!(%error, "no keyring, the GitHub login won't be remembered");
            Box::new(MemoryGithubStore::default())
        }
    }
}
