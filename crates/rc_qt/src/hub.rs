// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Connects the one `rc_core::Client` to the view models.
//!
//! The core can run several accounts, and the view models show one at a time: the current account.
//! The hub keeps a [`Cache`] per account (see [`Accounts`]), so that an account in the background
//! stays up to date, and lists them all in the `AccountList` model. The first account added is
//! current. Selecting another one, or removing the current one, switches: every view model gets a
//! full snapshot of the new current account's cache. Commands go to the current account, and so
//! do the answers the view models show: those of the others are dropped. A logout removes the
//! account, and the core adds a new logged-out one if it was the last, so the app goes on with
//! that one.
//!
//! `Session`, `RoomList`, `Room`, `MessageList`, `Thread` and `ThreadMessageList` need the events
//! of the same client, and QML creates them on its own schedule. The hub owns the client,
//! remembers what the events said, and queues each event of the current account to the view models
//! that registered. A view model registering later replays the current account's cache, so it
//! doesn't matter whether it is created before or after the first events.
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

use crate::account_list_model::qobject::AccountListModel;
use crate::accounts::{AccountChange, AccountView, Accounts};
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
    /// The accounts, each with its cache, and the current one: the one the view models show and
    /// the commands go to.
    accounts: Accounts,
    views: Views,
    list: Registered<AccountListModel>,
}

impl Hub {
    const fn new() -> Self {
        Self {
            client: None,
            accounts: Accounts::new(),
            views: Views::new(),
            list: Registered::new("AccountList"),
        }
    }

    /// Adds the account, unless it is known, and lists it.
    fn add_row(&mut self, id: &AccountId) {
        if self.accounts.added(id.clone()).is_some()
            && let Some((index, view)) = self.accounts.view(id)
        {
            self.list
                .queue(move |model| model.apply(&AccountChange::Inserted { index, view }));
            self.show_session_count();
        }
    }

    /// Tells the `Session` view model that the number of accounts changed, if it shows an account
    /// that has said what state it is in. Whether other accounts exist is part of what it shows.
    fn show_session_count(&mut self) {
        if self.accounts.current_cache().session_known() {
            self.views.show_session_view(&self.accounts);
        }
    }

    /// Updates the account's row in the account list.
    fn update_row(&mut self, id: &AccountId) {
        if let Some((index, view)) = self.accounts.view(id) {
            self.list
                .queue(move |model| model.apply(&AccountChange::Updated { index, view }));
        }
    }

    /// Brings the view models and the account list up to date if the current account is not
    /// `before` any more. Call it after every change to the accounts.
    fn switched_from(&mut self, before: Option<&AccountId>) {
        let current = self.accounts.current().cloned();
        if current.as_ref() == before {
            return;
        }
        // The two rows whose `current` changed. The one that was current may be gone.
        for id in before.into_iter().chain(&current) {
            self.update_row(id);
        }
        // With no account left, the view models keep showing the last one, logged out, until the
        // next account is added. An account added a moment ago is shown with its first `Session`
        // event (see `dispatch`): showing it before would flash the startup page.
        if current.is_some() && self.accounts.current_cache().session_known() {
            self.views.show_all(&self.accounts);
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
        hub.accounts = Accounts::new();
        hub.client.take()
    };
    drop(client);
}

/// Runs `command` for the current account, or logs that it was ignored: `what` names it.
fn current_account(what: &str, command: impl FnOnce(AccountHandle<'_>)) {
    let hub = lock();
    match (&hub.client, hub.accounts.current()) {
        (Some(client), Some(id)) => command(client.account(id)),
        (Some(_), None) => tracing::warn!("{what} ignored: there is no current account"),
        (None, _) => tracing::warn!("{what} ignored: the core isn't running"),
    }
}

pub fn login(server: &str, username: &str, password: SecretString) {
    current_account("login", |account| account.login(server, username, password));
}

/// Logs out and forgets the current account. The next account becomes current, or a new
/// logged-out one takes its place if it was the last.
pub fn logout() {
    let hub = lock();
    match (&hub.client, hub.accounts.current()) {
        (Some(client), Some(id)) => client.remove_account(id),
        (Some(_), None) => tracing::warn!("logout ignored: there is no current account"),
        (None, _) => tracing::warn!("logout ignored: the core isn't running"),
    }
}

/// Registers the `AccountList` model and returns its rows now. Call on the GUI thread; the changes
/// that follow are queued to `thread`.
pub fn register_accounts(thread: CxxQtThread<AccountListModel>) -> Vec<AccountView> {
    let mut hub = lock();
    hub.list.register(thread);
    hub.accounts.views()
}

/// Makes the account current: the view models show it from now on.
pub fn select_account(id: &str) {
    let mut guard = lock();
    let hub = &mut *guard;
    let before = hub.accounts.current().cloned();
    if hub.accounts.select(&AccountId::from(id.to_owned())) {
        hub.switched_from(before.as_ref());
    }
}

/// Adds an account, logged out, and makes it current, so that the login page shows for it. The
/// account is listed at once, which makes its `Added` a no-op.
pub fn add_account() {
    let mut guard = lock();
    let hub = &mut *guard;
    let Some(client) = &hub.client else {
        tracing::warn!("add ignored: the core isn't running");
        return;
    };
    let before = hub.accounts.current().cloned();
    let id = client.add_account();
    hub.add_row(&id);
    hub.accounts.select(&id);
    hub.switched_from(before.as_ref());
}

/// Asks the core to forget the account, logging it out first if it is logged in. It leaves the
/// list, and the current account if it was, when the core says `Removed`.
pub fn remove_account(id: &str) {
    if let Some(client) = &lock().client {
        client.remove_account(&AccountId::from(id.to_owned()));
    } else {
        tracing::warn!("remove ignored: the core isn't running");
    }
}

/// Registers the `Session` view model and returns what it shows now. Call on the GUI thread; the
/// events that follow the returned view are queued to `thread`.
pub fn register_session(thread: CxxQtThread<SessionVm>) -> SessionView {
    let mut hub = lock();
    hub.views.session.register(thread);
    hub.accounts.session_view()
}

/// Registers the `RoomList` model and returns the change that brings it up to date, and the
/// avatars it shows. Call on the GUI thread; the events that follow are queued to `thread`.
pub fn register_rooms(thread: CxxQtThread<RoomListModel>) -> (RoomChange, AvatarUrls) {
    let mut hub = lock();
    hub.views.rooms.register(thread);
    (
        hub.accounts.current_cache().rooms_snapshot(),
        hub.accounts.current_cache().avatars(),
    )
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
        .accounts
        .current_cache()
        .last_editable(jiff::Timestamp::now())
        .map_or_else(String::new, |message| message.id.to_string())
}

/// The id of the newest message of the open thread that the user can edit now, or empty.
pub fn last_editable_in_thread() -> String {
    lock()
        .accounts
        .current_cache()
        .thread_last_editable(jiff::Timestamp::now())
        .map_or_else(String::new, |message| message.id.to_string())
}

/// The text of a message of the open room or thread as written, or empty if it isn't held.
pub fn message_text(message_id: &str) -> String {
    lock()
        .accounts
        .current_cache()
        .message_text(message_id)
        .map_or_else(String::new, str::to_owned)
}

/// Who wrote a message of the open room or thread, by display name else username, or empty if it
/// isn't held.
pub fn message_author(message_id: &str) -> String {
    lock()
        .accounts
        .current_cache()
        .message_author(message_id)
        .map_or_else(String::new, str::to_owned)
}

/// Registers the `Room` view model and returns what it shows now. Call on the GUI thread; the
/// events that follow the returned state are queued to `thread`.
pub fn register_room(thread: CxxQtThread<RoomVm>) -> RoomState {
    let mut hub = lock();
    hub.views.room.register(thread);
    hub.accounts.current_cache().room_state()
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
        hub.accounts.current_cache().timeline_snapshot(),
        hub.accounts.current_cache().viewer(),
        hub.accounts.current_cache().avatars(),
        hub.accounts.current_cache().images(),
    )
}

/// Registers the `Thread` view model and returns what it shows now. Call on the GUI thread; the
/// events that follow the returned state are queued to `thread`.
pub fn register_thread(thread: CxxQtThread<ThreadVm>) -> ThreadState {
    let mut hub = lock();
    hub.views.thread.register(thread);
    hub.accounts.current_cache().thread_state()
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
        hub.accounts.current_cache().thread_snapshot(),
        hub.accounts.current_cache().viewer(),
        hub.accounts.current_cache().avatars(),
        hub.accounts.current_cache().images(),
    )
}

/// The thread's state now, for sending a reply.
pub fn thread_state() -> ThreadState {
    lock().accounts.current_cache().thread_state()
}

/// Registers the `EmojiCatalog` model and returns the custom emoji it lists now. Call on the GUI
/// thread; the changes that follow are queued to `thread`.
pub fn register_emoji_picker(thread: CxxQtThread<EmojiPickerModel>) -> CustomEmojis {
    let mut hub = lock();
    hub.views.picker.register(thread);
    hub.accounts.current_cache().viewer().emoji
}

/// The core's event callback, on a runtime thread.
#[allow(clippy::needless_pass_by_value)] // The callback's signature.
fn dispatch(account: AccountId, event: Event) {
    let mut guard = lock();
    let hub = &mut *guard;
    let before = hub.accounts.current().cloned();
    match event {
        Event::Added => hub.add_row(&account),
        Event::Removed => {
            // The logout's events came before, so the cache showed logged out already.
            if let Some(index) = hub.accounts.removed(&account) {
                hub.list
                    .queue(move |model| model.apply(&AccountChange::Removed { index }));
                hub.show_session_count();
            }
        }
        _ => {
            // The first state of an account that became current before it had one: show it whole.
            let first_state = matches!(event, Event::Session(_))
                && before.as_ref() == Some(&account)
                && !hub.accounts.current_cache().session_known();
            let Some(applied) = hub.accounts.apply(&account, &event) else {
                tracing::debug!(%account, "event of an unknown account ignored");
                return;
            };
            if matches!(
                event,
                Event::Identity(_) | Event::Unread(_) | Event::Session(_) | Event::Connection(_)
            ) {
                hub.update_row(&account);
            }
            if applied.current && first_state {
                hub.views.show_all(&hub.accounts);
            } else if applied.current {
                hub.views.show(&hub.accounts, event);
            } else if matches!(
                event,
                Event::MessageAction { .. } | Event::RoomAction { .. }
            ) {
                tracing::debug!(%account, "answer of another account dropped");
            }
            return;
        }
    }
    hub.switched_from(before.as_ref());
}

impl Views {
    /// Forwards the event of the current account, whose cache has taken it in already.
    fn show(&mut self, accounts: &Accounts, event: Event) {
        let cache = accounts.current_cache();
        match event {
            Event::MessagePolicy(_) => self.show_viewer(cache),
            Event::MessageAction {
                id,
                action,
                outcome,
            } => self.answer_action(id, action, outcome),
            Event::RoomAction { outcome, .. } => self.answer_room_action(outcome),
            Event::CustomEmoji(emoji) => {
                self.show_emoji(&emoji);
                self.show_session(accounts);
                // The topic of the open room may have custom emoji.
                self.show_room(cache);
            }
            Event::Session(_) | Event::Connection(_) => self.show_session(accounts),
            Event::Identity(_) => self.show_session_view(accounts),
            Event::Rooms(changes) => {
                self.rooms.queue(move |model| model.apply(&changes));
                // The open room's name may have changed.
                self.show_room(cache);
                self.show_thread(cache);
            }
            Event::Timeline(changes) => self.messages.queue(move |model| model.apply(&changes)),
            Event::TimelineStatus(_) => {
                self.show_room(cache);
                self.show_thread(cache);
            }
            Event::ThreadTimeline(changes) => self
                .thread_messages
                .queue(move |model| model.apply(&changes)),
            Event::ThreadStatus(_) | Event::ThreadTyping { .. } => self.show_thread(cache),
            Event::Typing { .. } => self.show_room(cache),
            Event::Avatars(updates) => self.show_avatars(&cache.avatar_changes(&updates)),
            Event::Images(updates) => self.show_images(&cache.image_changes(&updates)),
            _ => {}
        }
    }

    /// Gives every view model the whole of what the current account says, replacing what it
    /// showed: the snapshot a view model gets when it registers.
    fn show_all(&mut self, accounts: &Accounts) {
        let cache = accounts.current_cache();
        let (rooms, avatars) = (cache.rooms_snapshot(), cache.avatars());
        self.rooms.queue(move |model| model.show(rooms, avatars));
        let (messages, viewer, avatars, images) = (
            cache.timeline_snapshot(),
            cache.viewer(),
            cache.avatars(),
            cache.images(),
        );
        self.messages
            .queue(move |model| model.show(&messages, viewer, avatars, images));
        let (replies, viewer, avatars, images) = (
            cache.thread_snapshot(),
            cache.viewer(),
            cache.avatars(),
            cache.images(),
        );
        self.thread_messages
            .queue(move |model| model.show(&replies, viewer, avatars, images));
        self.show_emoji(&cache.viewer().emoji);
        let state = cache.room_state();
        self.room.queue(move |mut vm| {
            vm.as_mut().show(&state);
            // What failed for the other account means nothing for this one.
            vm.dismiss_action_error();
        });
        self.show_thread(cache);
        self.show_session_view(accounts);
    }

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

    /// Tells the `Session` view model, and the `MessageList` model, what the current account says
    /// now.
    fn show_session(&mut self, accounts: &Accounts) {
        self.show_viewer(accounts.current_cache());
        self.show_session_view(accounts);
    }

    /// Tells the `Session` view model what the current account says now.
    fn show_session_view(&mut self, accounts: &Accounts) {
        let view = accounts.session_view();
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
