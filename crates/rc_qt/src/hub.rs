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
use rc_core::directory::{ActionState, DirectoryAction};
use rc_core::domain::{AccountId, AvatarOwner, MessageId, RoomId, RoomKind};
use rc_core::emoji::CustomEmojis;
use rc_core::github::GithubConfig;
use rc_core::policy::{ActionError, MessageAction};
use rc_core::rooms::{RoomActionError, RoomChange};
use rc_core::threads::{ThreadFilter, ThreadListChange};
use rc_core::timeline::{Found, TimelineChange};
use rc_core::uploads::UploadId;
use rc_core::{AccountHandle, Client, Event};
use secrecy::SecretString;

use crate::account_list_model::qobject::AccountListModel;
use crate::emoji_picker_model::qobject::EmojiPickerModel;
use crate::mentions_model::qobject::MentionsModel;
use crate::message_list_model::qobject::MessageListModel;
use crate::message_search_model::qobject::MessageSearchModel;
use crate::room_list_model::qobject::RoomListModel;
use crate::room_search_model::qobject::RoomSearchModel;
use crate::room_vm::qobject::RoomVm;
use crate::session_vm::qobject::SessionVm;
use crate::thread_list_model::qobject::ThreadListModel;
use crate::thread_list_vm::qobject::ThreadListVm;
use crate::thread_message_list_model::qobject::ThreadMessageListModel;
use crate::thread_vm::qobject::ThreadVm;
use crate::upload_list_model::qobject::{RoomUploadsModel, ThreadUploadsModel};
use rc_view::accounts::{AccountView, Accounts};
use rc_view::attachment_view::ImageUrls;
use rc_view::avatar_view::AvatarUrls;
use rc_view::cache::Cache;
use rc_view::directory::{SearchRow, search_rows};
use rc_view::mention::Popup;
use rc_view::message_search_view::{PickTarget, View as MessageSearchView};
use rc_view::rich_html::topic_html;
use rc_view::room_search::{search_rooms as search, trimmed};
use rc_view::room_view::RoomState;
use rc_view::session_view::SessionView;
use rc_view::staged_view::{StagedFile, plan};
use rc_view::startup;
use rc_view::switchboard::{Screen, Shown, Switchboard};
use rc_view::thread_list_view::{
    PaneStep, ThreadListState, ThreadUnread, back_steps, close_pane_steps, escape_steps,
    open_search_steps, open_thread_steps, pick_steps, toggle_search_steps, toggle_steps,
};
use rc_view::thread_view::ThreadState;
use rc_view::upload_view::UploadRow;
use rc_view::viewer::Viewer;

// There is exactly one core per process, so one hub.
mod github;

use github::GithubHub;
pub use github::{
    github_approve_runs, github_ask_merge, github_cancel_confirm, github_connect_with_gh,
    github_connect_with_token, github_disconnect, github_dismiss_command, github_merge,
    github_refresh, github_rerun_failed, github_rerun_job, github_select, github_set_notify,
    github_show, github_show_tab, github_tick, github_update_branch, register_github,
    register_jobs, register_pull_requests, register_runners,
};

static HUB: Mutex<Hub> = Mutex::new(Hub::new());

struct Hub {
    client: Option<Client>,
    /// The accounts, and what to show when they change: see [`Switchboard`].
    switchboard: Switchboard,
    views: Views,
    list: Registered<AccountListModel>,
    /// The GitHub pull requests, once the core has started.
    github: Option<GithubHub>,
}

impl Hub {
    const fn new() -> Self {
        Self {
            client: None,
            switchboard: Switchboard::new(),
            views: Views::new(),
            list: Registered::new("AccountList"),
            github: None,
        }
    }

    /// Ends the message search of the current account when the user goes to another account
    /// (`to`, or a new one): the pane closes with the room it was about.
    fn close_search_before_leaving(&self, to: Option<&str>) {
        let current = self.switchboard.accounts().current();
        if current.is_some_and(|current| to == Some(current.as_str())) {
            return;
        }
        if let (Some(client), Some(current)) = (&self.client, current) {
            client.account(current).cancel_message_search();
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
                Shown::Screen(screen) => {
                    if let Some(github) = self.github.as_mut() {
                        github.set_shown(screen == Screen::GitHub);
                    }
                }
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
    mentions: Registered<MentionsModel>,
    message_search: Registered<MessageSearchModel>,
    room: Registered<RoomVm>,
    messages: Registered<MessageListModel>,
    thread: Registered<ThreadVm>,
    thread_messages: Registered<ThreadMessageListModel>,
    thread_list: Registered<ThreadListVm>,
    thread_list_rows: Registered<ThreadListModel>,
    picker: Registered<EmojiPickerModel>,
    room_uploads: Registered<RoomUploadsModel>,
    thread_uploads: Registered<ThreadUploadsModel>,
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
    crate::clipboard::init(&cache_dir);
    // Before the client exists: the GitHub service's first event can come at once.
    lock().github = Some(GithubHub::new());
    match Client::start(store, DiskFileCache::new(cache_dir), dispatch) {
        Ok(client) => {
            // Only with `--gh`: without the service, the GitHub commands are ignored.
            let service = crate::lifecycle::github_enabled().then(|| {
                let setup = startup::github_setup();
                client.github(
                    &setup.api_url,
                    setup.gh,
                    startup::github_store(),
                    GithubConfig::default(),
                    github::dispatch,
                )
            });
            let mut hub = lock();
            if let Some(github) = hub.github.as_mut() {
                github.service = service;
            }
            hub.client = Some(client);
        }
        Err(error) => tracing::error!(%error, "can't start the core"),
    }
}

/// Stops the core. Call on the GUI thread, once the event loop has ended.
pub fn stop() {
    // Dropped outside the lock: it waits for a pending logout, and the callback locks the hub.
    let (service, client) = {
        let mut hub = lock();
        hub.switchboard.clear();
        let service = hub.github.as_mut().and_then(|github| github.service.take());
        (service, hub.client.take())
    };
    // The service stops with the client's runtime, so it goes first.
    drop(service);
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
    hub.close_search_before_leaving(Some(id));
    let shown = hub.switchboard.select(&AccountId::from(id.to_owned()));
    hub.carry_out(shown);
}

/// Adds an account, logged out, and makes it current, so that the login page shows for it. The
/// account is listed at once, which makes its `Added` a no-op.
pub fn add_account() {
    let mut guard = lock();
    let hub = &mut *guard;
    hub.close_search_before_leaving(None);
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

/// Registers the `Mentions` model. Call on the GUI thread; it is told to show its rows again when
/// the member search says something. It has nothing to replay: it asks with [`mention_popup`].
pub fn register_mentions(thread: CxxQtThread<MentionsModel>) {
    lock().views.mentions.register(thread);
}

/// The room the composer of the open room (or of the open thread, which is in it) writes to, and
/// whether it is a direct message. `None` while no room is open.
pub fn mention_target(for_thread: bool) -> Option<(RoomId, bool)> {
    let hub = lock();
    let cache = hub.switchboard.accounts().current_cache();
    let state = cache.room_state();
    let id = if for_thread {
        cache.thread_state().room_id
    } else {
        state.room_id
    };
    (!id.is_empty()).then(|| (RoomId::from(id), state.kind == "direct"))
}

/// The rows of the mention popup for what is typed after the `@`, with the avatars to show them
/// with.
pub fn mention_popup(direct: bool, query: &str) -> (Popup, AvatarUrls) {
    let hub = lock();
    let cache = hub.switchboard.accounts().current_cache();
    let kind = if direct {
        RoomKind::Direct
    } else {
        RoomKind::Channel
    };
    (cache.mentions().popup(kind, query), cache.avatars())
}

/// Searches the members of `room` for the mention popup.
pub fn search_members(room: &RoomId, query: &str) {
    current_account("search_members", |account| {
        account.search_members(room, query);
    });
}

/// Ends the member search, when the mention popup closes.
pub fn cancel_member_search() {
    current_account("cancel_member_search", |account| {
        account.cancel_member_search();
    });
}

/// Registers the `MessageSearch` model. Call on the GUI thread; it is told to show the pane again
/// when the core says something of the search, and asks for it with [`message_search_shown`].
pub fn register_message_search(thread: CxxQtThread<MessageSearchModel>) {
    lock().views.message_search.register(thread);
}

/// What the search pane shows now, with the avatars to show it with, the open room if the search
/// is of another one (the pane is to move to it), and the message the room was asked to show,
/// which the rows mark as picked.
pub fn message_search_shown(
    zone: &jiff::tz::TimeZone,
    now: jiff::Timestamp,
) -> (MessageSearchView, AvatarUrls, Option<RoomId>, Option<Found>) {
    let hub = lock();
    let cache = hub.switchboard.accounts().current_cache();
    (
        cache.message_search_view(zone, now),
        cache.avatars(),
        cache.message_search_stale_room(),
        cache.found().cloned(),
    )
}

/// The open room, while the search pane is open: the room the field searches.
pub fn message_search_room() -> Option<RoomId> {
    let hub = lock();
    let cache = hub.switchboard.accounts().current_cache();
    cache.message_search_room()?;
    cache.open_room().cloned()
}

/// Searches the messages of `room` for `query`.
pub fn search_messages(room: &RoomId, query: &str) {
    current_account("search_messages", |account| {
        account.search_messages(room, query);
    });
}

/// Asks for the next page of the search.
pub fn more_message_search() {
    current_account("more_message_search", |account| {
        account.more_message_search();
    });
}

/// What the room search shows for a query.
pub struct SearchResults {
    /// Top to bottom, as `rc_view::directory::search_rows` says.
    pub rows: Vec<SearchRow>,
    pub avatars: AvatarUrls,
    /// The directory action under way, if any.
    pub busy: Option<DirectoryAction>,
}

/// What the current account has for `query`: its rooms that match, the held-back ones too, and
/// what the directory found, with the avatars to show them with.
pub fn search_results(query: &str) -> SearchResults {
    let hub = lock();
    let cache = hub.switchboard.accounts().current_cache();
    let query = trimmed(query);
    let rows = if query.is_empty() {
        Vec::new()
    } else {
        let rooms = search(cache.rooms(), cache.quiet_rooms(), query);
        search_rows(query, &rooms, cache.directory(), &cache.directory_view())
    };
    SearchResults {
        rows,
        avatars: cache.avatars(),
        busy: cache.directory_action().and_then(working),
    }
}

/// The action, if the state says it is under way.
fn working((action, state): &(DirectoryAction, ActionState)) -> Option<DirectoryAction> {
    (*state == ActionState::Working).then(|| action.clone())
}

/// Searches the server's directory for the text of the search field.
pub fn search_directory(text: &str) {
    current_account("search_directory", |account| account.search_directory(text));
}

pub fn open_direct(username: &str) {
    current_account("open_direct", |account| account.open_direct(username));
}

pub fn preview_channel(room_id: String) {
    current_account("preview_channel", |account| {
        account.preview_channel(RoomId::from(room_id));
    });
}

pub fn join_channel(room_id: String) {
    current_account("join_channel", |account| {
        account.join_channel(RoomId::from(room_id));
    });
}

pub fn open_room(room_id: String) {
    current_account("open_room", |account| {
        account.open_room(RoomId::from(room_id));
    });
}

pub fn load_older() {
    current_account("load_older", |account| account.load_older());
}

/// Fetches the messages after the newest one of a detached window.
pub fn load_newer() {
    current_account("load_newer", |account| account.load_newer());
}

/// Leaves the detached window for the newest messages.
pub fn show_latest() {
    current_account("show_latest", |account| account.show_latest());
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

/// Sends the text with the staged files to the open room, or to the open thread: see
/// [`plan`] for what goes first and what goes with which file. Returns whether anything was sent,
/// i.e. whether a room (a thread) is open.
pub fn send_with_files(
    for_thread: bool,
    text: &str,
    quoted_id: &str,
    also_to_channel: bool,
    files: &[StagedFile],
) -> bool {
    let (room, parent) = if for_thread {
        let state = thread_state();
        if state.parent_id.is_empty() || state.room_id.is_empty() {
            return false;
        }
        (
            RoomId::from(state.room_id),
            Some(MessageId::from(state.parent_id)),
        )
    } else {
        let room_id = lock()
            .switchboard
            .accounts()
            .current_cache()
            .room_state()
            .room_id;
        if room_id.is_empty() {
            return false;
        }
        (RoomId::from(room_id), None)
    };
    let plan = plan(text, !quoted_id.is_empty(), files);
    let quote = (!quoted_id.is_empty()).then(|| MessageId::from(quoted_id.to_owned()));
    if let Some(message) = plan.message {
        match parent.clone() {
            Some(parent) => send_reply(room.clone(), &message, quote, parent, also_to_channel),
            None => send_message(room.clone(), &message, quote),
        }
    }
    for (path, text) in plan.uploads {
        current_account("upload_file", |account| {
            account.upload_file(room.clone(), path, &text, parent.clone());
        });
    }
    true
}

pub fn cancel_upload(id: UploadId) {
    current_account("cancel_upload", |account| account.cancel_upload(id));
}

pub fn retry_upload(id: UploadId) {
    current_account("retry_upload", |account| account.retry_upload(id));
}

/// Registers the `RoomUploads` model and returns its rows now. Call on the GUI thread; the changes
/// that follow are queued to `thread`.
pub fn register_room_uploads(thread: CxxQtThread<RoomUploadsModel>) -> Vec<UploadRow> {
    let mut hub = lock();
    hub.views.room_uploads.register(thread);
    hub.switchboard
        .accounts()
        .current_cache()
        .room_upload_rows()
}

/// Registers the `ThreadUploads` model, as [`register_room_uploads`] does.
pub fn register_thread_uploads(thread: CxxQtThread<ThreadUploadsModel>) -> Vec<UploadRow> {
    let mut hub = lock();
    hub.views.thread_uploads.register(thread);
    hub.switchboard
        .accounts()
        .current_cache()
        .thread_upload_rows()
}

pub fn open_thread(message_id: String) {
    // The thread takes the pane from the search.
    let steps = {
        let hub = lock();
        open_thread_steps(
            &hub.switchboard
                .accounts()
                .current_cache()
                .thread_list_state(),
        )
    };
    current_account("open_thread", |account| {
        run_steps(&account, &steps, None);
        account.open_thread(MessageId::from(message_id));
    });
}

/// Shows the message a search result stands for in the room and, for a reply, opens its thread
/// over the results (the search stays). A result that isn't a reply closes the thread an earlier
/// one opened.
pub fn show_result(target: &PickTarget) {
    let steps = {
        let hub = lock();
        pick_steps(
            &hub.switchboard
                .accounts()
                .current_cache()
                .thread_list_state(),
            target.thread.is_some(),
        )
    };
    current_account("show_result", |account| {
        run_steps(&account, &steps, None);
        account.show_message(target.show.clone());
        if let Some(thread) = &target.thread {
            account.open_thread(thread.clone());
        }
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

/// Does what a command on the pane decided, in order. A search opens in `room`, the open one.
fn run_steps(account: &AccountHandle<'_>, steps: &[PaneStep], room: Option<&RoomId>) {
    for step in steps {
        match *step {
            PaneStep::CloseThread => account.close_thread(),
            PaneStep::CloseList => account.close_thread_list(),
            PaneStep::OpenList(filter) => account.open_thread_list(filter),
            PaneStep::OpenSearch => {
                if let Some(room) = room {
                    account.search_messages(room, "");
                }
            }
            PaneStep::CloseSearch => account.cancel_message_search(),
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
        (Some(client), Some(id)) => run_steps(&client.account(id), &steps, cache.open_room()),
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

/// The header button of the search.
pub fn toggle_search() {
    pane_command("toggle_search", |state, _| toggle_search_steps(state));
}

/// Ctrl+F: opens the search unless it is open.
pub fn open_search() {
    pane_command("open_search", |state, _| open_search_steps(state));
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

/// F5: shows the held channel news of every account (ROOM-012). Returns whether any account held
/// some, going by the caches: if not, the caller says there is nothing new.
pub fn release_news() -> bool {
    let hub = lock();
    let Some(client) = &hub.client else {
        tracing::warn!("release_news ignored: the core isn't running");
        return true;
    };
    hub.switchboard
        .release_news(|id| client.account(id).release_channels())
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
pub fn register_messages(thread: CxxQtThread<MessageListModel>) -> MessagesShown {
    let mut hub = lock();
    hub.views.messages.register(thread);
    let cache = hub.switchboard.accounts().current_cache();
    MessagesShown {
        snapshot: cache.timeline_snapshot(),
        viewer: cache.viewer(),
        avatars: cache.avatars(),
        images: cache.images(),
        found: found_id(cache),
    }
}

/// What the `MessageList` model starts with: [`register_messages`]'s answer.
pub struct MessagesShown {
    pub snapshot: TimelineChange,
    pub viewer: Viewer,
    pub avatars: AvatarUrls,
    pub images: ImageUrls,
    /// The message to mark.
    pub found: Option<MessageId>,
}

/// The id of the message the open room was asked to show, which the list marks.
fn found_id(cache: &Cache) -> Option<MessageId> {
    cache.found().map(|found| found.message.clone())
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
            Event::DirectoryAction { action, state } => {
                self.answer_directory_action(&action, &state);
                self.show_room(cache);
            }
            Event::MemberSearch(_) => self.mentions.queue(MentionsModel::refresh),
            Event::MessageSearch(_) => {
                self.message_search.queue(MessageSearchModel::refresh);
                // The search takes the place of the threads in the pane.
                self.show_thread_list(cache);
            }
            Event::Directory(_) | Event::QuietRooms(_) => {
                self.room_search.queue(RoomSearchModel::refresh);
            }
            Event::Rooms(changes) => {
                self.room_search.queue(RoomSearchModel::refresh);
                // The name of the room in the pane's header.
                self.message_search.queue(MessageSearchModel::refresh);
                self.rooms.queue(move |model| model.apply(&changes));
                self.show_thread_unread(cache);
                // The open room's name may have changed.
                self.show_room(cache);
                self.show_thread(cache);
                self.show_thread_list(cache);
            }
            Event::Uploads(_) | Event::UploadProgress { .. } => self.show_uploads(cache),
            Event::Timeline(changes) => self.messages.queue(move |model| model.apply(&changes)),
            // After the change that brought the message in, so the model has its row.
            Event::ShowMessage { message, .. } => {
                self.messages
                    .queue(move |model| model.request_show(&message));
            }
            Event::TimelineStatus(_) => {
                // In the queue of the messages, before the window's reset that follows.
                let found = found_id(cache);
                self.messages.queue(move |model| model.set_found(found));
                // The search follows the room that is open.
                self.message_search.queue(MessageSearchModel::refresh);
                self.show_uploads(cache);
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
                self.show_uploads(cache);
                self.show_thread(cache);
                // A thread hides the list, and the pane follows.
                self.show_thread_list(cache);
            }
            Event::ThreadTyping { .. } => self.show_thread(cache),
            Event::ThreadList(changes) => self
                .thread_list_rows
                .queue(move |model| model.apply(&changes)),
            Event::ThreadListStatus(_) => self.show_thread_list(cache),
            // The previewed channel is the open room's header, and the composer's Join.
            Event::Typing { .. } | Event::Preview(_) => self.show_room(cache),
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
        self.room_search.queue(RoomSearchModel::switched);
        self.mentions.queue(MentionsModel::refresh);
        self.message_search.queue(MessageSearchModel::refresh);
        self.rooms.queue(move |model| model.show(rooms, avatars));
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
        // After the room's state, which the list reads when it resets: whether to follow the
        // newest.
        let (messages, viewer, avatars, images) = (
            cache.timeline_snapshot(),
            cache.viewer(),
            cache.avatars(),
            cache.images(),
        );
        let found = found_id(cache);
        self.messages.queue(move |mut model| {
            model.as_mut().show(&messages, viewer, avatars, images);
            model.set_found(found);
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
        self.show_uploads(cache);
        self.show_session_view(accounts);
    }

    const fn new() -> Self {
        Self {
            session: Registered::new("Session"),
            rooms: Registered::new("RoomList"),
            room_search: Registered::new("RoomSearch"),
            mentions: Registered::new("Mentions"),
            message_search: Registered::new("MessageSearch"),
            room: Registered::new("Room"),
            messages: Registered::new("MessageList"),
            thread: Registered::new("Thread"),
            thread_messages: Registered::new("ThreadMessageList"),
            thread_list: Registered::new("ThreadList"),
            thread_list_rows: Registered::new("ThreadListModel"),
            picker: Registered::new("EmojiCatalog"),
            room_uploads: Registered::new("RoomUploads"),
            thread_uploads: Registered::new("ThreadUploads"),
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

    /// Tells the search which row is busy, and the `RoomList` model, whose error strip shows why
    /// the action failed.
    fn answer_directory_action(&mut self, action: &DirectoryAction, state: &ActionState) {
        let busy = working(&(action.clone(), state.clone()));
        self.room_search.queue(move |model| model.set_busy(busy));
        let outcome = match state {
            ActionState::Failed(error) => Err(error.clone()),
            ActionState::Working | ActionState::Done => Ok(()),
        };
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
        let for_mentions = changes.to_vec();
        self.mentions
            .queue(move |model| model.set_avatars(&for_mentions));
        let for_found = changes.to_vec();
        self.message_search
            .queue(move |model| model.set_avatars(&for_found));
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

    /// Tells the upload lists of the room and of the thread the rows `cache` has for them now.
    fn show_uploads(&mut self, cache: &Cache) {
        let rows = cache.room_upload_rows();
        self.room_uploads.queue(move |model| model.show(&rows));
        let rows = cache.thread_upload_rows();
        self.thread_uploads.queue(move |model| model.show(&rows));
    }

    /// Sends the `Thread` view model what `cache` says it shows now.
    fn show_thread(&mut self, cache: &Cache) {
        let state = cache.thread_state();
        self.thread.queue(move |vm| vm.show(&state));
    }
}
