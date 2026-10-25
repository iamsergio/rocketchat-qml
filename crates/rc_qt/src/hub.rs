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
//! `Session`, `RoomList`, `RoomSearch`, `Room`, `MessageList`, `Thread` and `ThreadMessageList`
//! need the events of the same client, and QML creates them on its own schedule. The hub owns the
//! client, remembers what the events said, and queues each event of the current account to the
//! view models that registered. A view model registering later replays the current account's
//! cache, so it doesn't matter whether it is created before or after the first events.
//!
//! The event callback runs on a runtime thread and never waits for the GUI thread; the GUI thread
//! never waits for the runtime either (commands don't block). So queueing while holding the lock
//! is fine.

use std::pin::Pin;
use std::sync::{Mutex, MutexGuard, PoisonError};

use cxx_qt::CxxQtThread;
use cxx_qt::Threading;
use rc_core::adapters::DiskFileCache;
use rc_core::domain::{AccountId, AvatarOwner, MessageId, Room, RoomId};
use rc_core::emoji::CustomEmojis;
use rc_core::policy::{ActionError, MessageAction};
use rc_core::rooms::{RoomActionError, RoomChange};
use rc_core::threads::{ThreadFilter, ThreadListChange};
use rc_core::timeline::TimelineChange;
use rc_core::{AccountHandle, Client, Event};
use secrecy::SecretString;

use crate::account_list_model::qobject::AccountListModel;
use crate::emoji_picker_model::qobject::EmojiPickerModel;
use crate::message_list_model::qobject::MessageListModel;
use crate::room_list_model::qobject::RoomListModel;
use crate::room_search_model::qobject::RoomSearchModel;
use crate::room_vm::qobject::RoomVm;
use crate::session_vm::qobject::SessionVm;
use crate::thread_list_model::qobject::ThreadListModel;
use crate::thread_list_vm::qobject::ThreadListVm;
use crate::thread_message_list_model::qobject::ThreadMessageListModel;
use crate::thread_vm::qobject::ThreadVm;
use rc_view::accounts::{AccountView, Accounts};
use rc_view::attachment_view::ImageUrls;
use rc_view::avatar_view::AvatarUrls;
use rc_view::cache::Cache;
use rc_view::rich_html::topic_html;
use rc_view::room_search::search_rooms as search;
use rc_view::room_view::RoomState;
use rc_view::session_view::SessionView;
use rc_view::startup;
use rc_view::switchboard::{Shown, Switchboard};
use rc_view::thread_list_view::{
    PaneStep, ThreadListState, ThreadUnread, back_steps, close_pane_steps, escape_steps,
    toggle_steps,
};
use rc_view::thread_view::ThreadState;
use rc_view::viewer::Viewer;

// There is exactly one core per process, so one hub.
static HUB: Mutex<Hub> = Mutex::new(Hub::new());

struct Hub {
    client: Option<Client>,
    /// The accounts, and what to show when they change: see [`Switchboard`].
    switchboard: Switchboard,
    views: Views,
    list: Registered<AccountListModel>,
}

impl Hub {
    const fn new() -> Self {
        Self {
            client: None,
            switchboard: Switchboard::new(),
            views: Views::new(),
            list: Registered::new("AccountList"),
        }
    }

    /// Does what the switchboard says to show, in order.
    fn carry_out(&mut self, shown: Vec<Shown>) {
        let accounts = self.switchboard.accounts();
        for one in shown {
            match one {
                Shown::AccountRow(change) => self.list.queue(move |model| model.apply(&change)),
                Shown::SessionView(view) => self.views.queue_session_view(view),
                Shown::All => self.views.show_all(accounts),
                Shown::Event(event) => self.views.show(accounts, event),
            }
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
    room_search: Registered<RoomSearchModel>,
    room: Registered<RoomVm>,
    messages: Registered<MessageListModel>,
    thread: Registered<ThreadVm>,
    thread_messages: Registered<ThreadMessageListModel>,
    thread_list: Registered<ThreadListVm>,
    thread_list_rows: Registered<ThreadListModel>,
    picker: Registered<EmojiPickerModel>,
}

/// Locks the hub. A poisoned lock is recovered rather than skipped: every update of the hub is a
/// single small step that leaves it consistent, and skipping would freeze the UI for good.
fn lock() -> MutexGuard<'static, Hub> {
    HUB.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Starts the core, unless it is running already. Downloaded files are kept in `cache_dir`. Call on
/// the GUI thread.
pub fn start(cache_dir: &str) {
    if lock().client.is_some() {
        tracing::info!("the core is already running");
        return;
    }
    let store = startup::secret_store();
    let cache_dir = startup::cache_dir(cache_dir);
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
        hub.switchboard.clear();
        hub.client.take()
    };
    drop(client);
}

/// Runs `command` for the current account, or logs that it was ignored: `what` names it.
fn current_account(what: &str, command: impl FnOnce(AccountHandle<'_>)) {
    let hub = lock();
    match (&hub.client, hub.switchboard.accounts().current()) {
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
    match (&hub.client, hub.switchboard.accounts().current()) {
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
    hub.switchboard.accounts().views()
}

/// Makes the account current: the view models show it from now on.
pub fn select_account(id: &str) {
    let mut guard = lock();
    let hub = &mut *guard;
    let shown = hub.switchboard.select(&AccountId::from(id.to_owned()));
    hub.carry_out(shown);
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
    let id = client.add_account();
    let shown = hub.switchboard.add(&id);
    hub.carry_out(shown);
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
    hub.switchboard.accounts().session_view()
}

/// Registers the `RoomList` model and returns the change that brings it up to date, and the
/// avatars it shows. Call on the GUI thread; the events that follow are queued to `thread`.
pub fn register_rooms(thread: CxxQtThread<RoomListModel>) -> (RoomChange, AvatarUrls) {
    let mut hub = lock();
    hub.views.rooms.register(thread);
    (
        hub.switchboard.accounts().current_cache().rooms_snapshot(),
        hub.switchboard.accounts().current_cache().avatars(),
    )
}

/// Registers the `RoomSearch` model. Call on the GUI thread; it is told to search again when the
/// room list changes. It has nothing to replay: it asks for results with [`search_rooms`].
pub fn register_room_search(thread: CxxQtThread<RoomSearchModel>) {
    lock().views.room_search.register(thread);
}

/// The rooms of the current account that match `query`, the held-back ones too, and the avatars to
/// show them with.
pub fn search_rooms(query: &str) -> (Vec<Room>, AvatarUrls) {
    let hub = lock();
    let cache = hub.switchboard.accounts().current_cache();
    (
        search(cache.rooms(), cache.quiet_rooms(), query),
        cache.avatars(),
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

/// Lists the open room's threads, from the first page.
pub fn open_thread_list(filter: ThreadFilter) {
    current_account("open_thread_list", |account| {
        account.open_thread_list(filter);
    });
}

pub fn close_thread_list() {
    current_account("close_thread_list", |account| account.close_thread_list());
}

pub fn load_more_threads() {
    current_account("load_more_threads", |account| account.load_more_threads());
}

/// Does what a command on the pane decided, in order.
fn run_steps(account: &AccountHandle<'_>, steps: &[PaneStep]) {
    for step in steps {
        match *step {
            PaneStep::CloseThread => account.close_thread(),
            PaneStep::CloseList => account.close_thread_list(),
            PaneStep::OpenList(filter) => account.open_thread_list(filter),
        }
    }
}

/// Carries out the steps `decide` takes from the pane's state and the filter last used in the
/// open room.
fn pane_command(what: &str, decide: impl FnOnce(&ThreadListState, ThreadFilter) -> Vec<PaneStep>) {
    let hub = lock();
    let cache = hub.switchboard.accounts().current_cache();
    let steps = decide(&cache.thread_list_state(), cache.thread_list_filter());
    match (&hub.client, hub.switchboard.accounts().current()) {
        (Some(client), Some(id)) => run_steps(&client.account(id), &steps),
        (Some(_), None) => tracing::warn!("{what} ignored: there is no current account"),
        (None, _) => tracing::warn!("{what} ignored: the core isn't running"),
    }
}

/// The back button of a thread.
pub fn thread_back() {
    pane_command("thread_back", back_steps);
}

/// The pane's close button.
pub fn close_pane() {
    pane_command("close_pane", |state, _| close_pane_steps(state));
}

/// The header button.
pub fn toggle_pane() {
    pane_command("toggle_pane", toggle_steps);
}

/// Escape in the pane.
pub fn escape_pane() {
    pane_command("escape_pane", escape_steps);
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
        .switchboard
        .accounts()
        .current_cache()
        .last_editable(jiff::Timestamp::now())
        .map_or_else(String::new, |message| message.id.to_string())
}

/// The id of the newest message of the open thread that the user can edit now, or empty.
pub fn last_editable_in_thread() -> String {
    lock()
        .switchboard
        .accounts()
        .current_cache()
        .thread_last_editable(jiff::Timestamp::now())
        .map_or_else(String::new, |message| message.id.to_string())
}

/// The text of a message of the open room or thread as written, or empty if it isn't held.
pub fn message_text(message_id: &str) -> String {
    lock()
        .switchboard
        .accounts()
        .current_cache()
        .message_text(message_id)
        .map_or_else(String::new, str::to_owned)
}

/// Who wrote a message of the open room or thread, by display name else username, or empty if it
/// isn't held.
pub fn message_author(message_id: &str) -> String {
    lock()
        .switchboard
        .accounts()
        .current_cache()
        .message_author(message_id)
        .map_or_else(String::new, str::to_owned)
}

/// Registers the `Room` view model and returns what it shows now. Call on the GUI thread; the
/// events that follow the returned state are queued to `thread`.
pub fn register_room(thread: CxxQtThread<RoomVm>) -> (RoomState, String) {
    let mut hub = lock();
    hub.views.room.register(thread);
    room_shown(hub.switchboard.accounts().current_cache())
}

/// What the `Room` view model shows for `cache`: the state, and its topic as HTML for that
/// account's viewer.
fn room_shown(cache: &Cache) -> (RoomState, String) {
    let state = cache.room_state();
    let topic_html = topic_html(&state.topic, &cache.viewer());
    (state, topic_html)
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
        hub.switchboard
            .accounts()
            .current_cache()
            .timeline_snapshot(),
        hub.switchboard.accounts().current_cache().viewer(),
        hub.switchboard.accounts().current_cache().avatars(),
        hub.switchboard.accounts().current_cache().images(),
    )
}

/// Registers the `Thread` view model and returns what it shows now. Call on the GUI thread; the
/// events that follow the returned state are queued to `thread`.
pub fn register_thread(thread: CxxQtThread<ThreadVm>) -> ThreadState {
    let mut hub = lock();
    hub.views.thread.register(thread);
    hub.switchboard.accounts().current_cache().thread_state()
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
        hub.switchboard.accounts().current_cache().thread_snapshot(),
        hub.switchboard.accounts().current_cache().viewer(),
        hub.switchboard.accounts().current_cache().avatars(),
        hub.switchboard.accounts().current_cache().images(),
    )
}

/// Registers the `ThreadList` view model and returns what it shows now. Call on the GUI thread;
/// the events that follow the returned state are queued to `thread`.
pub fn register_thread_list(thread: CxxQtThread<ThreadListVm>) -> ThreadListState {
    let mut hub = lock();
    hub.views.thread_list.register(thread);
    hub.switchboard
        .accounts()
        .current_cache()
        .thread_list_state()
}

/// Registers the `ThreadListModel` model and returns the change that brings it up to date, the
/// avatars it shows and which threads are unread. Call on the GUI thread; the events that follow are queued to `thread`.
pub fn register_thread_list_model(
    thread: CxxQtThread<ThreadListModel>,
) -> (ThreadListChange, AvatarUrls, ThreadUnread) {
    let mut hub = lock();
    hub.views.thread_list_rows.register(thread);
    let cache = hub.switchboard.accounts().current_cache();
    (
        cache.thread_list_snapshot(),
        cache.avatars(),
        cache.thread_unread(),
    )
}

/// The thread's state now, for sending a reply.
pub fn thread_state() -> ThreadState {
    lock().switchboard.accounts().current_cache().thread_state()
}

/// Registers the `EmojiCatalog` model and returns the custom emoji it lists now. Call on the GUI
/// thread; the changes that follow are queued to `thread`.
pub fn register_emoji_picker(thread: CxxQtThread<EmojiPickerModel>) -> CustomEmojis {
    let mut hub = lock();
    hub.views.picker.register(thread);
    hub.switchboard.accounts().current_cache().viewer().emoji
}

/// The core's event callback, on a runtime thread.
#[allow(clippy::needless_pass_by_value)] // The callback's signature.
fn dispatch(account: AccountId, event: Event) {
    let mut guard = lock();
    let hub = &mut *guard;
    let shown = hub.switchboard.dispatch(&account, event);
    hub.carry_out(shown);
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
            Event::QuietRooms(_) => self.room_search.queue(RoomSearchModel::refresh),
            Event::Rooms(changes) => {
                self.room_search.queue(RoomSearchModel::refresh);
                self.rooms.queue(move |model| model.apply(&changes));
                self.show_thread_unread(cache);
                // The open room's name may have changed.
                self.show_room(cache);
                self.show_thread(cache);
                self.show_thread_list(cache);
            }
            Event::Timeline(changes) => self.messages.queue(move |model| model.apply(&changes)),
            Event::TimelineStatus(_) => {
                // The open room may be another: its threads are unread, or not.
                self.show_thread_unread(cache);
                self.show_room(cache);
                self.show_thread(cache);
                self.show_thread_list(cache);
            }
            Event::ThreadTimeline(changes) => self
                .thread_messages
                .queue(move |model| model.apply(&changes)),
            Event::ThreadStatus(_) => {
                self.show_thread(cache);
                // A thread hides the list, and the pane follows.
                self.show_thread_list(cache);
            }
            Event::ThreadTyping { .. } => self.show_thread(cache),
            Event::ThreadList(changes) => self
                .thread_list_rows
                .queue(move |model| model.apply(&changes)),
            Event::ThreadListStatus(_) => self.show_thread_list(cache),
            Event::Typing { .. } => self.show_room(cache),
            Event::Avatars(updates) => self.show_avatars(&cache.avatar_changes(&updates)),
            Event::Images(updates) => self.show_images(&cache.image_changes(&updates)),
            // `EmojiImages` is ignored too: the Qt app never asks for them, QML loads the custom
            // emoji by URL.
            _ => {}
        }
    }

    /// Gives every view model the whole of what the current account says, replacing what it
    /// showed: the snapshot a view model gets when it registers.
    fn show_all(&mut self, accounts: &Accounts) {
        let cache = accounts.current_cache();
        let (rooms, avatars) = (cache.rooms_snapshot(), cache.avatars());
        self.room_search.queue(RoomSearchModel::refresh);
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
        let (state, topic_html) = room_shown(cache);
        self.room.queue(move |mut vm| {
            vm.as_mut().show(&state, &topic_html);
            // What failed for the other account means nothing for this one.
            vm.dismiss_action_error();
        });
        let (threads, avatars, unread) = (
            cache.thread_list_snapshot(),
            cache.avatars(),
            cache.thread_unread(),
        );
        self.thread_list_rows
            .queue(move |model| model.show(threads, avatars, unread));
        self.show_thread(cache);
        self.show_thread_list(cache);
        self.show_session_view(accounts);
    }

    const fn new() -> Self {
        Self {
            session: Registered::new("Session"),
            rooms: Registered::new("RoomList"),
            room_search: Registered::new("RoomSearch"),
            room: Registered::new("Room"),
            messages: Registered::new("MessageList"),
            thread: Registered::new("Thread"),
            thread_messages: Registered::new("ThreadMessageList"),
            thread_list: Registered::new("ThreadList"),
            thread_list_rows: Registered::new("ThreadListModel"),
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
        self.queue_session_view(accounts.session_view());
    }

    /// Tells the `Session` view model to show `view`.
    fn queue_session_view(&mut self, view: SessionView) {
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
        let for_search = changes.to_vec();
        self.room_search
            .queue(move |model| model.set_avatars(&for_search));
        let for_messages = changes.to_vec();
        self.messages
            .queue(move |model| model.set_avatars(&for_messages));
        let for_thread = changes.to_vec();
        self.thread_messages
            .queue(move |model| model.set_avatars(&for_thread));
        let for_threads = changes.to_vec();
        self.thread_list_rows
            .queue(move |model| model.set_avatars(&for_threads));
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
        let (state, topic_html) = room_shown(cache);
        self.room.queue(move |vm| vm.show(&state, &topic_html));
    }

    /// Tells the thread list model which threads of the open room have unread replies.
    fn show_thread_unread(&mut self, cache: &Cache) {
        let unread = cache.thread_unread();
        self.thread_list_rows
            .queue(move |model| model.set_unread(unread));
    }

    /// Sends the `ThreadList` view model what `cache` says it shows now.
    fn show_thread_list(&mut self, cache: &Cache) {
        let state = cache.thread_list_state();
        self.thread_list.queue(move |vm| vm.show(&state));
    }

    /// Sends the `Thread` view model what `cache` says it shows now.
    fn show_thread(&mut self, cache: &Cache) {
        let state = cache.thread_state();
        self.thread.queue(move |vm| vm.show(&state));
    }
}
