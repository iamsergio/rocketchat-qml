// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Connects the one `rc_core::Client` to the view models.
//!
//! Both `Session` and `RoomList` need the events of the same client, and QML creates them on its
//! own schedule. The hub owns the client, remembers what the events said (see [`Cache`]), and
//! queues each event to the view models that registered. A view model registering later replays
//! the cache, so it doesn't matter whether it is created before or after the first events.
//!
//! The event callback runs on a runtime thread and never waits for the GUI thread; the GUI thread
//! never waits for the runtime either (commands don't block). So queueing while holding the lock
//! is fine.

use std::sync::{Mutex, MutexGuard, PoisonError};

use cxx_qt::CxxQtThread;
use rc_core::adapters::{MemoryStore, default_store};
use rc_core::ports::SecretStore;
use rc_core::rooms::RoomChange;
use rc_core::{Client, Event};
use secrecy::SecretString;

use crate::cache::Cache;
use crate::room_list_model::qobject::RoomListModel;
use crate::session_view::{SessionView, session_view};
use crate::session_vm::qobject::SessionVm;

/// Environment variable choosing where the session token is kept. `memory` keeps it in memory
/// only; anything else, or unset, uses the OS keyring.
const STORE_ENV: &str = "RC_SECRET_STORE";

// There is exactly one core per process, so one hub.
static HUB: Mutex<Hub> = Mutex::new(Hub::new());

struct Hub {
    client: Option<Client>,
    cache: Cache,
    session: Option<CxxQtThread<SessionVm>>,
    rooms: Option<CxxQtThread<RoomListModel>>,
}

impl Hub {
    const fn new() -> Self {
        Self {
            client: None,
            cache: Cache::new(),
            session: None,
            rooms: None,
        }
    }
}

/// Locks the hub. A poisoned lock is recovered rather than skipped: every update of the hub is a
/// single small step that leaves it consistent, and skipping would freeze the UI for good.
fn lock() -> MutexGuard<'static, Hub> {
    HUB.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Whether `RC_SECRET_STORE` (`value`, if set) asks for the in-memory store.
fn wants_memory_store(value: Option<&str>) -> bool {
    value == Some("memory")
}

/// Starts the core, unless it is running already. Call on the GUI thread.
pub fn start() {
    if lock().client.is_some() {
        tracing::info!("the core is already running");
        return;
    }
    let value = std::env::var(STORE_ENV).ok();
    let store: Box<dyn SecretStore> = if wants_memory_store(value.as_deref()) {
        tracing::info!("secret store: memory ({STORE_ENV}=memory)");
        Box::new(MemoryStore::default())
    } else {
        tracing::info!("secret store: OS keyring, falling back to memory");
        default_store()
    };
    match Client::start(store, dispatch) {
        Ok(client) => lock().client = Some(client),
        Err(error) => tracing::error!(%error, "can't start the core"),
    }
}

/// Stops the core. Call on the GUI thread, once the event loop has ended.
pub fn stop() {
    // Dropped outside the lock: it waits for a pending logout, and the callback locks the hub.
    let client = lock().client.take();
    drop(client);
}

pub fn login(server: &str, username: &str, password: SecretString) {
    if let Some(client) = &lock().client {
        client.login(server, username, password);
    } else {
        tracing::warn!("login ignored: the core isn't running");
    }
}

pub fn logout() {
    if let Some(client) = &lock().client {
        client.logout();
    } else {
        tracing::warn!("logout ignored: the core isn't running");
    }
}

/// Registers the `Session` view model and returns what it shows now. Call on the GUI thread; the
/// events that follow the returned view are queued to `thread`.
pub fn register_session(thread: CxxQtThread<SessionVm>) -> SessionView {
    let mut hub = lock();
    hub.session = Some(thread);
    session_view(hub.cache.state())
}

/// Registers the `RoomList` model and returns the change that brings it up to date. Call on the
/// GUI thread; the events that follow are queued to `thread`.
pub fn register_rooms(thread: CxxQtThread<RoomListModel>) -> RoomChange {
    let mut hub = lock();
    hub.rooms = Some(thread);
    hub.cache.rooms_snapshot()
}

/// The core's event callback, on a runtime thread.
fn dispatch(event: Event) {
    let mut guard = lock();
    let hub = &mut *guard;
    hub.cache.apply(&event);
    match event {
        Event::Session(state) => {
            let view = session_view(Some(&state));
            let gone = hub
                .session
                .as_ref()
                .is_some_and(|thread| thread.queue(move |vm| vm.show(&view)).is_err());
            if gone {
                tracing::debug!("the Session object is gone, unregistered");
                hub.session = None;
            }
        }
        Event::Rooms(changes) => {
            let gone = hub
                .rooms
                .as_ref()
                .is_some_and(|thread| thread.queue(move |model| model.apply(&changes)).is_err());
            if gone {
                tracing::debug!("the RoomList object is gone, unregistered");
                hub.rooms = None;
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // AUTH-004
    #[test]
    fn only_memory_selects_the_memory_store() {
        assert!(wants_memory_store(Some("memory")));
        assert!(!wants_memory_store(Some("keyring")));
        assert!(!wants_memory_store(Some("")));
        assert!(!wants_memory_store(None));
    }
}
