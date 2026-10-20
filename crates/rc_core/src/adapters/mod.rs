// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Implementations of the ports for real I/O.

mod blob;
mod disk_cache;
// The test store writes the token in clear text: it only exists with its feature, and in unit tests.
#[cfg(any(test, feature = "test-file-store"))]
mod file;
mod keyring;
mod memory;
mod message_store;
mod rest;

pub use disk_cache::DiskFileCache;
#[cfg(any(test, feature = "test-file-store"))]
pub use file::FileStore;
pub use keyring::KeyringStore;
pub use memory::MemoryStore;
pub use message_store::MemoryMessageStore;
pub use rest::RestConnector;

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
