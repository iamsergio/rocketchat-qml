// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Connects the one `rc_core::Client` to the view models.
//!
//! The core can run several accounts, but the view models show one: the current account, the first
//! one added. The events and the commands of the others are ignored for now, and when the current
//! account is removed the next one added becomes current. A logout removes the account, and the
//! core adds a new logged-out one after it, so the app goes on with that one.
//!
//! `Session`, `RoomList`, `Room`, `MessageList`, `Thread` and `ThreadMessageList` need the events
//! of the same client, and QML creates them on its own schedule. The hub owns the client,
//! remembers what the events said (see [`Cache`]), and queues each event to the view models that
//! registered. A view model registering later replays the cache, so it doesn't matter whether it
//! is created before or after the first events.
//!
//! The event callback runs on a runtime thread and never waits for the GUI thread; the GUI thread
//! never waits for the runtime either (commands don't block). So queueing while holding the lock
//! is fine.

use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Mutex, MutexGuard, PoisonError};

use cxx_qt::CxxQtThread;
use cxx_qt::Threading;
use rc_core::adapters::{DiskFileCache, MemoryStore, default_store};
use rc_core::domain::{AccountId, AvatarOwner, MessageId, RoomId};
use rc_core::emoji::CustomEmojis;
use rc_core::policy::{ActionError, MessageAction};
use rc_core::ports::SecretStore;
use rc_core::rooms::{RoomActionError, RoomChange};
use rc_core::timeline::TimelineChange;
use rc_core::{AccountHandle, Client, Event};
use secrecy::SecretString;

use crate::attachment_view::ImageUrls;
use crate::avatar_view::AvatarUrls;
use crate::cache::Cache;
use crate::emoji_picker_model::qobject::EmojiPickerModel;
use crate::message_list_model::qobject::MessageListModel;
use crate::rich_html::Viewer;
use crate::room_list_model::qobject::RoomListModel;
use crate::room_view::RoomState;
use crate::room_vm::qobject::RoomVm;
use crate::session_view::SessionView;
use crate::session_vm::qobject::SessionVm;
use crate::thread_message_list_model::qobject::ThreadMessageListModel;
use crate::thread_view::ThreadState;
use crate::thread_vm::qobject::ThreadVm;

/// Environment variable choosing where the session token is kept. `memory` keeps it in memory
/// only, and `file:<path>` in a plain file (GUI tests, in builds with `test-file-store`); anything
/// else, or unset, uses the OS keyring.
const STORE_ENV: &str = "RC_SECRET_STORE";

// There is exactly one core per process, so one hub.
static HUB: Mutex<Hub> = Mutex::new(Hub::new());

struct Hub {
    client: Option<Client>,
    /// The account the view models show, and the commands go to. `None` between the removal of
    /// the current account and the addition of the next.
    current: Option<AccountId>,
    cache: Cache,
    views: Views,
}

impl Hub {
    const fn new() -> Self {
        Self {
            client: None,
            current: None,
            cache: Cache::new(),
            views: Views::new(),
        }
    }
}

/// A view model that registered, if it did: the way to queue work to it on the GUI thread.
struct Registered<T: Threading> {
    thread: Option<CxxQtThread<T>>,
    /// The QML name of the object, for the log.
    name: &'static str,
}

impl<T: Threading> Registered<T> {
    const fn new(name: &'static str) -> Self {
        Self { thread: None, name }
    }

    fn register(&mut self, thread: CxxQtThread<T>) {
        self.thread = Some(thread);
    }

    /// Queues `update` to the object, if registered. Unregisters it if it is gone.
    fn queue(&mut self, update: impl FnOnce(Pin<&mut T>) + Send + 'static) {
        let gone = self
            .thread
            .as_ref()
            .is_some_and(|thread| thread.queue(update).is_err());
        if gone {
            tracing::debug!("the {} object is gone, unregistered", self.name);
            self.thread = None;
        }
    }
}

/// The view models that registered. What each one shows comes from the [`Cache`].
struct Views {
    session: Registered<SessionVm>,
    rooms: Registered<RoomListModel>,
    room: Registered<RoomVm>,
    messages: Registered<MessageListModel>,
    thread: Registered<ThreadVm>,
    thread_messages: Registered<ThreadMessageListModel>,
    picker: Registered<EmojiPickerModel>,
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
    let client = {
        let mut hub = lock();
        hub.current = None;
        hub.client.take()
    };
    drop(client);
}

/// Runs `command` for the current account, or logs that it was ignored: `what` names it.
fn current_account(what: &str, command: impl FnOnce(AccountHandle<'_>)) {
    let hub = lock();
    match (&hub.client, &hub.current) {
        (Some(client), Some(id)) => command(client.account(id)),
        (Some(_), None) => tracing::warn!("{what} ignored: there is no current account"),
        (None, _) => tracing::warn!("{what} ignored: the core isn't running"),
    }
}

pub fn login(server: &str, username: &str, password: SecretString) {
    current_account("login", |account| account.login(server, username, password));
}

/// Logs out and forgets the current account. A new logged-out one takes its place.
pub fn logout() {
    let hub = lock();
    match (&hub.client, &hub.current) {
        (Some(client), Some(id)) => client.remove_account(id),
        (Some(_), None) => tracing::warn!("logout ignored: there is no current account"),
        (None, _) => tracing::warn!("logout ignored: the core isn't running"),
    }
}

/// Registers the `Session` view model and returns what it shows now. Call on the GUI thread; the
/// events that follow the returned view are queued to `thread`.
pub fn register_session(thread: CxxQtThread<SessionVm>) -> SessionView {
    let mut hub = lock();
    hub.views.session.register(thread);
    hub.cache.session_view()
}

/// Registers the `RoomList` model and returns the change that brings it up to date, and the
/// avatars it shows. Call on the GUI thread; the events that follow are queued to `thread`.
pub fn register_rooms(thread: CxxQtThread<RoomListModel>) -> (RoomChange, AvatarUrls) {
    let mut hub = lock();
    hub.views.rooms.register(thread);
    (hub.cache.rooms_snapshot(), hub.cache.avatars())
}

pub fn open_room(room_id: String) {
    current_account("open_room", |account| {
        account.open_room(RoomId::from(room_id));
    });
}

pub fn load_older() {
    current_account("load_older", |account| account.load_older());
}

pub fn send_message(room: RoomId, text: &str, quote: Option<MessageId>) {
    current_account("send_message", |account| {
        account.send_message(room, text, quote, None, false);
    });
}

/// Sends a reply in the open thread `parent`, also to the room if `also_to_channel`.
pub fn send_reply(
    room: RoomId,
    text: &str,
    quote: Option<MessageId>,
    parent: MessageId,
    also_to_channel: bool,
) {
    current_account("send_reply", |account| {
        account.send_message(room, text, quote, Some(parent), also_to_channel);
    });
}

pub fn open_thread(message_id: String) {
    current_account("open_thread", |account| {
        account.open_thread(MessageId::from(message_id));
    });
}

pub fn close_thread() {
    current_account("close_thread", |account| account.close_thread());
}

pub fn load_older_thread() {
    current_account("load_older_thread", |account| account.load_older_thread());
}

pub fn retry_message(message_id: String) {
    current_account("retry_message", |account| {
        account.retry_message(MessageId::from(message_id));
    });
}

pub fn edit_message(message_id: String, text: &str) {
    current_account("edit_message", |account| {
        account.edit_message(MessageId::from(message_id), text);
    });
}

pub fn react(message_id: String, emoji: &str, react: bool) {
    current_account("react", |account| {
        account.react(MessageId::from(message_id), emoji, react);
    });
}

pub fn set_favorite(room_id: String, favorite: bool) {
    current_account("set_favorite", |account| {
        account.set_favorite(RoomId::from(room_id), favorite);
    });
}

pub fn mark_room_read(room_id: String) {
    current_account("mark_read", |account| {
        account.mark_read(RoomId::from(room_id));
    });
}

pub fn mark_room_unread(room_id: String) {
    current_account("mark_unread", |account| {
        account.mark_unread(RoomId::from(room_id));
    });
}

pub fn leave_room(room_id: String) {
    current_account("leave_room", |account| {
        account.leave_room(RoomId::from(room_id));
    });
}

pub fn delete_message(message_id: String) {
    current_account("delete_message", |account| {
        account.delete_message(MessageId::from(message_id));
    });
}

/// The id of the newest message of the open room that the user can edit now, or empty.
pub fn last_editable() -> String {
    lock()
        .cache
        .last_editable(jiff::Timestamp::now())
        .map_or_else(String::new, |message| message.id.to_string())
}

/// The id of the newest message of the open thread that the user can edit now, or empty.
pub fn last_editable_in_thread() -> String {
    lock()
        .cache
        .thread_last_editable(jiff::Timestamp::now())
        .map_or_else(String::new, |message| message.id.to_string())
}

/// The text of a message of the open room or thread as written, or empty if it isn't held.
pub fn message_text(message_id: &str) -> String {
    lock()
        .cache
        .message_text(message_id)
        .map_or_else(String::new, str::to_owned)
}

/// Who wrote a message of the open room or thread, by display name else username, or empty if it
/// isn't held.
pub fn message_author(message_id: &str) -> String {
    lock()
        .cache
        .message_author(message_id)
        .map_or_else(String::new, str::to_owned)
}

/// Registers the `Room` view model and returns what it shows now. Call on the GUI thread; the
/// events that follow the returned state are queued to `thread`.
pub fn register_room(thread: CxxQtThread<RoomVm>) -> RoomState {
    let mut hub = lock();
    hub.views.room.register(thread);
    hub.cache.room_state()
}

/// Registers the `MessageList` model and returns the change that brings it up to date, who it
/// shows the messages to, and the avatars and attachment images it shows. Call on the GUI thread;
/// the events that follow are queued to `thread`.
pub fn register_messages(
    thread: CxxQtThread<MessageListModel>,
) -> (TimelineChange, Viewer, AvatarUrls, ImageUrls) {
    let mut hub = lock();
    hub.views.messages.register(thread);
    (
        hub.cache.timeline_snapshot(),
        hub.cache.viewer(),
        hub.cache.avatars(),
        hub.cache.images(),
    )
}

/// Registers the `Thread` view model and returns what it shows now. Call on the GUI thread; the
/// events that follow the returned state are queued to `thread`.
pub fn register_thread(thread: CxxQtThread<ThreadVm>) -> ThreadState {
    let mut hub = lock();
    hub.views.thread.register(thread);
    hub.cache.thread_state()
}

/// Registers the `ThreadMessageList` model and returns the change that brings it up to date, who
/// it shows the messages to, and the avatars and attachment images it shows, as
/// [`register_messages`] does for `MessageList`.
pub fn register_thread_messages(
    thread: CxxQtThread<ThreadMessageListModel>,
) -> (TimelineChange, Viewer, AvatarUrls, ImageUrls) {
    let mut hub = lock();
    hub.views.thread_messages.register(thread);
    (
        hub.cache.thread_snapshot(),
        hub.cache.viewer(),
        hub.cache.avatars(),
        hub.cache.images(),
    )
}

/// The thread's state now, for sending a reply.
pub fn thread_state() -> ThreadState {
    lock().cache.thread_state()
}

/// Registers the `EmojiCatalog` model and returns the custom emoji it lists now. Call on the GUI
/// thread; the changes that follow are queued to `thread`.
pub fn register_emoji_picker(thread: CxxQtThread<EmojiPickerModel>) -> CustomEmojis {
    let mut hub = lock();
    hub.views.picker.register(thread);
    hub.cache.viewer().emoji
}

/// The core's event callback, on a runtime thread.
fn dispatch(account: AccountId, event: Event) {
    let mut guard = lock();
    let hub = &mut *guard;
    if hub.current.is_none() && event == Event::Added {
        hub.current = Some(account);
        return;
    }
    if hub.current.as_ref() != Some(&account) {
        tracing::debug!(%account, "event of another account ignored");
        return;
    }
    if event == Event::Removed {
        // The logout's events came before, so the cache shows logged out already; the account
        // that replaces this one starts it over with its own.
        hub.current = None;
        return;
    }
    hub.cache.apply(&event);
    let (cache, views) = (&hub.cache, &mut hub.views);
    match event {
        Event::MessagePolicy(_) => views.show_viewer(cache),
        Event::MessageAction {
            id,
            action,
            outcome,
        } => views.answer_action(id, action, outcome),
        Event::RoomAction { outcome, .. } => views.answer_room_action(outcome),
        Event::CustomEmoji(emoji) => {
            views.show_emoji(&emoji);
            views.show_session(cache);
        }
        Event::Session(_) | Event::Connection(_) => views.show_session(cache),
        Event::Rooms(changes) => {
            views.rooms.queue(move |model| model.apply(&changes));
            // The open room's name may have changed.
            views.show_room(cache);
            views.show_thread(cache);
        }
        Event::Timeline(changes) => views.messages.queue(move |model| model.apply(&changes)),
        Event::TimelineStatus(_) => {
            views.show_room(cache);
            views.show_thread(cache);
        }
        Event::ThreadTimeline(changes) => views
            .thread_messages
            .queue(move |model| model.apply(&changes)),
        Event::ThreadStatus(_) => views.show_thread(cache),
        Event::Avatars(updates) => views.show_avatars(&cache.avatar_changes(&updates)),
        Event::Images(updates) => views.show_images(&cache.image_changes(&updates)),
        _ => {}
    }
}

impl Views {
    const fn new() -> Self {
        Self {
            session: Registered::new("Session"),
            rooms: Registered::new("RoomList"),
            room: Registered::new("Room"),
            messages: Registered::new("MessageList"),
            thread: Registered::new("Thread"),
            thread_messages: Registered::new("ThreadMessageList"),
            picker: Registered::new("EmojiCatalog"),
        }
    }

    /// Tells the `Session` view model, and the `MessageList` model, what `cache` says now.
    fn show_session(&mut self, cache: &Cache) {
        self.show_viewer(cache);
        let view = cache.session_view();
        self.session.queue(move |vm| vm.show(&view));
    }

    /// Tells the `EmojiCatalog` model which custom emoji the server has.
    fn show_emoji(&mut self, emoji: &CustomEmojis) {
        let emoji = emoji.clone();
        self.picker.queue(move |model| model.set_emoji(&emoji));
    }

    /// Tells the `Room` view model how an edit, a deletion or a reaction ended.
    fn answer_action(
        &mut self,
        id: MessageId,
        action: MessageAction,
        outcome: Result<(), ActionError>,
    ) {
        self.room
            .queue(move |vm| vm.finish_action(id.as_str(), action, &outcome));
    }

    /// Tells the `RoomList` model how a command on a room ended.
    fn answer_room_action(&mut self, outcome: Result<(), RoomActionError>) {
        self.rooms.queue(move |model| model.finish_action(&outcome));
    }

    /// Tells the message models which attachment images changed.
    fn show_images(&mut self, changes: &[(String, String)]) {
        let for_room = changes.to_vec();
        self.messages
            .queue(move |model| model.set_images(&for_room));
        let for_thread = changes.to_vec();
        self.thread_messages
            .queue(move |model| model.set_images(&for_thread));
    }

    /// Tells the models that show avatars which ones changed.
    fn show_avatars(&mut self, changes: &[(AvatarOwner, String)]) {
        let for_rooms = changes.to_vec();
        self.rooms.queue(move |model| model.set_avatars(&for_rooms));
        let for_messages = changes.to_vec();
        self.messages
            .queue(move |model| model.set_avatars(&for_messages));
        let for_thread = changes.to_vec();
        self.thread_messages
            .queue(move |model| model.set_avatars(&for_thread));
    }

    /// Tells the message models who they show the messages to now, as `cache` says.
    fn show_viewer(&mut self, cache: &Cache) {
        let for_room = cache.viewer();
        self.messages.queue(move |model| model.set_viewer(for_room));
        let for_thread = cache.viewer();
        self.thread_messages
            .queue(move |model| model.set_viewer(for_thread));
    }

    /// Sends the `Room` view model what `cache` says it shows now.
    fn show_room(&mut self, cache: &Cache) {
        let state = cache.room_state();
        self.room.queue(move |vm| vm.show(&state));
    }

    /// Sends the `Thread` view model what `cache` says it shows now.
    fn show_thread(&mut self, cache: &Cache) {
        let state = cache.thread_state();
        self.thread.queue(move |vm| vm.show(&state));
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
