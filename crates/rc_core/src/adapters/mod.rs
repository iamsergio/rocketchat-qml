// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Implementations of the ports for real I/O.

mod keyring;
mod memory;
mod rest;

pub use keyring::KeyringStore;
pub use memory::MemoryStore;
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
