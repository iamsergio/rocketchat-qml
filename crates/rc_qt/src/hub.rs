// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Connects the one `rc_core::Client` to the view models.
//!
//! `Session`, `RoomList`, `Room` and `MessageList` need the events of the same client, and QML
//! creates them on its own schedule. The hub owns the client, remembers what the events said (see
//! [`Cache`]), and queues each event to the view models that registered. A view model registering
//! later replays the cache, so it doesn't matter whether it is created before or after the first
//! events.
//!
//! The event callback runs on a runtime thread and never waits for the GUI thread; the GUI thread
//! never waits for the runtime either (commands don't block). So queueing while holding the lock
//! is fine.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, PoisonError};

use cxx_qt::CxxQtThread;
use rc_core::adapters::{DiskFileCache, MemoryStore, default_store};
use rc_core::domain::{AvatarOwner, MessageId, RoomId};
use rc_core::ports::SecretStore;
use rc_core::rooms::RoomChange;
use rc_core::timeline::TimelineChange;
use rc_core::{Client, Event};
use secrecy::SecretString;

use crate::attachment_view::ImageUrls;
use crate::avatar_view::AvatarUrls;
use crate::cache::Cache;
use crate::message_list_model::qobject::MessageListModel;
use crate::rich_html::Viewer;
use crate::room_list_model::qobject::RoomListModel;
use crate::room_view::RoomState;
use crate::room_vm::qobject::RoomVm;
use crate::session_view::SessionView;
use crate::session_vm::qobject::SessionVm;

/// Environment variable choosing where the session token is kept. `memory` keeps it in memory
/// only, and `file:<path>` in a plain file (GUI tests, in builds with `test-file-store`); anything
/// else, or unset, uses the OS keyring.
const STORE_ENV: &str = "RC_SECRET_STORE";

// There is exactly one core per process, so one hub.
static HUB: Mutex<Hub> = Mutex::new(Hub::new());

struct Hub {
    client: Option<Client>,
    cache: Cache,
    session: Option<CxxQtThread<SessionVm>>,
    rooms: Option<CxxQtThread<RoomListModel>>,
    room: Option<CxxQtThread<RoomVm>>,
    messages: Option<CxxQtThread<MessageListModel>>,
}

impl Hub {
    const fn new() -> Self {
        Self {
            client: None,
            cache: Cache::new(),
            session: None,
            rooms: None,
            room: None,
            messages: None,
        }
    }
}

/// Locks the hub. A poisoned lock is recovered rather than skipped: every update of the hub is a
/// single small step that leaves it consistent, and skipping would freeze the UI for good.
fn lock() -> MutexGuard<'static, Hub> {
    HUB.lock().unwrap_or_else(PoisonError::into_inner)
}

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

/// Where downloads go when the host gives no directory.
fn fallback_cache_dir() -> PathBuf {
    std::env::temp_dir().join("rocketchat-qml-cache")
}

/// Starts the core, unless it is running already. Downloaded files are kept in `cache_dir`. Call on
/// the GUI thread.
pub fn start(cache_dir: &str) {
    if lock().client.is_some() {
        tracing::info!("the core is already running");
        return;
    }
    let value = std::env::var(STORE_ENV).ok();
    let store: Box<dyn SecretStore> = match store_choice(value.as_deref()) {
        StoreChoice::Memory => {
            tracing::info!("secret store: memory ({STORE_ENV}=memory)");
            Box::new(MemoryStore::default())
        }
        StoreChoice::File(path) => file_store(path),
        StoreChoice::Keyring => {
            tracing::info!("secret store: OS keyring, falling back to memory");
            default_store()
        }
    };
    let cache_dir = if cache_dir.is_empty() {
        tracing::warn!("no cache directory given, using the temporary directory");
        fallback_cache_dir()
    } else {
        PathBuf::from(cache_dir)
    };
    tracing::info!(dir = %cache_dir.display(), "file cache");
    match Client::start(store, DiskFileCache::new(cache_dir), dispatch) {
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
    hub.cache.session_view()
}

/// Registers the `RoomList` model and returns the change that brings it up to date, and the
/// avatars it shows. Call on the GUI thread; the events that follow are queued to `thread`.
pub fn register_rooms(thread: CxxQtThread<RoomListModel>) -> (RoomChange, AvatarUrls) {
    let mut hub = lock();
    hub.rooms = Some(thread);
    (hub.cache.rooms_snapshot(), hub.cache.avatars())
}

pub fn open_room(room_id: String) {
    if let Some(client) = &lock().client {
        client.open_room(RoomId::from(room_id));
    } else {
        tracing::warn!("open_room ignored: the core isn't running");
    }
}

pub fn load_older() {
    if let Some(client) = &lock().client {
        client.load_older();
    } else {
        tracing::warn!("load_older ignored: the core isn't running");
    }
}

pub fn send_message(room: RoomId, text: &str) {
    if let Some(client) = &lock().client {
        client.send_message(room, text);
    } else {
        tracing::warn!("send_message ignored: the core isn't running");
    }
}

pub fn retry_message(message_id: String) {
    if let Some(client) = &lock().client {
        client.retry_message(MessageId::from(message_id));
    } else {
        tracing::warn!("retry_message ignored: the core isn't running");
    }
}

/// Registers the `Room` view model and returns what it shows now. Call on the GUI thread; the
/// events that follow the returned state are queued to `thread`.
pub fn register_room(thread: CxxQtThread<RoomVm>) -> RoomState {
    let mut hub = lock();
    hub.room = Some(thread);
    hub.cache.room_state()
}

/// Registers the `MessageList` model and returns the change that brings it up to date, who it
/// shows the messages to, and the avatars and attachment images it shows. Call on the GUI thread;
/// the events that follow are queued to `thread`.
pub fn register_messages(
    thread: CxxQtThread<MessageListModel>,
) -> (TimelineChange, Viewer, AvatarUrls, ImageUrls) {
    let mut hub = lock();
    hub.messages = Some(thread);
    (
        hub.cache.timeline_snapshot(),
        hub.cache.viewer(),
        hub.cache.avatars(),
        hub.cache.images(),
    )
}

/// The core's event callback, on a runtime thread.
fn dispatch(event: Event) {
    let mut guard = lock();
    let hub = &mut *guard;
    hub.cache.apply(&event);
    match event {
        Event::Session(_) | Event::Connection(_) | Event::CustomEmoji(_) => {
            show_viewer(hub);
            let view = hub.cache.session_view();
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
            // The open room's name may have changed.
            show_room(hub);
        }
        Event::Timeline(changes) => {
            let gone = hub
                .messages
                .as_ref()
                .is_some_and(|thread| thread.queue(move |model| model.apply(&changes)).is_err());
            if gone {
                tracing::debug!("the MessageList object is gone, unregistered");
                hub.messages = None;
            }
        }
        Event::TimelineStatus(_) => show_room(hub),
        Event::Avatars(updates) => {
            let changes = hub.cache.avatar_changes(&updates);
            show_avatars(hub, &changes);
        }
        Event::Images(updates) => {
            let changes = hub.cache.image_changes(&updates);
            show_images(hub, &changes);
        }
        _ => {}
    }
}

/// Tells the `MessageList` model which attachment images changed.
fn show_images(hub: &mut Hub, changes: &[(String, String)]) {
    let changes = changes.to_vec();
    let gone = hub.messages.as_ref().is_some_and(|thread| {
        thread
            .queue(move |model| model.set_images(&changes))
            .is_err()
    });
    if gone {
        tracing::debug!("the MessageList object is gone, unregistered");
        hub.messages = None;
    }
}

/// Tells the models that show avatars which ones changed.
fn show_avatars(hub: &mut Hub, changes: &[(AvatarOwner, String)]) {
    let for_rooms = changes.to_vec();
    let rooms_gone = hub.rooms.as_ref().is_some_and(|thread| {
        thread
            .queue(move |model| model.set_avatars(&for_rooms))
            .is_err()
    });
    if rooms_gone {
        tracing::debug!("the RoomList object is gone, unregistered");
        hub.rooms = None;
    }
    let for_messages = changes.to_vec();
    let messages_gone = hub.messages.as_ref().is_some_and(|thread| {
        thread
            .queue(move |model| model.set_avatars(&for_messages))
            .is_err()
    });
    if messages_gone {
        tracing::debug!("the MessageList object is gone, unregistered");
        hub.messages = None;
    }
}

/// Tells the `MessageList` model who it shows the messages to now.
fn show_viewer(hub: &mut Hub) {
    let viewer = hub.cache.viewer();
    let gone = hub
        .messages
        .as_ref()
        .is_some_and(|thread| thread.queue(move |model| model.set_viewer(viewer)).is_err());
    if gone {
        tracing::debug!("the MessageList object is gone, unregistered");
        hub.messages = None;
    }
}

/// Sends the `Room` view model what the cache says it shows now.
fn show_room(hub: &mut Hub) {
    let state = hub.cache.room_state();
    let gone = hub
        .room
        .as_ref()
        .is_some_and(|thread| thread.queue(move |vm| vm.show(&state)).is_err());
    if gone {
        tracing::debug!("the Room object is gone, unregistered");
        hub.room = None;
    }
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
