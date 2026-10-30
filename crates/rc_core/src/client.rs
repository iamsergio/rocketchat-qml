// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The handle frontends drive the core through.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc as std_mpsc;
use std::time::Duration;

use secrecy::SecretString;
use tokio::runtime::{Builder, Handle, Runtime};
use tokio::sync::mpsc::{self, UnboundedSender};

use crate::adapters::{GhProcess, RestConnector, RestGithubConnector, SystemClock};
use crate::avatars::AvatarUpdate;
use crate::core::{self, ActorSenders, CoreCommand};
use crate::directory::{ActionState, Directory, DirectoryAction};
use crate::domain::{AccountId, Identity, MessageId, Room, RoomId};
use crate::emoji::{self, CustomEmojis};
use crate::emoji_images::EmojiImageUpdate;
use crate::error::Error;
use crate::github::{GitHub, GithubConfig, GithubEvent, GithubPorts, GithubStore};
use crate::images::ImageUpdate;
use crate::members::MemberSearch;
use crate::message_search::MessageSearch;
use crate::policy::{ActionError, MessageAction, MessagePolicy};
use crate::ports::{Clock, Connector, FileCache, SecretStore};
use crate::rooms::{RoomAction, RoomActionError, RoomChange, Unread};
use crate::session::SessionState;
use crate::threads::{ThreadFilter, ThreadListChange, ThreadListStatus};
use crate::timeline::{ThreadStatus, TimelineChange, TimelineStatus};
use crate::tuning::Tuning;
use crate::uploads::{Upload, UploadId};

#[cfg(test)]
mod tests;

/// Worker threads of the async runtime: networking only, the GUI has its own thread.
const WORKER_THREADS: usize = 2;
/// How long dropping a [`Client`] waits for the actor to finish draining.
const SHUTDOWN_WAIT: Duration = Duration::from_secs(3);
/// How long dropping a [`Client`] waits for the runtime's threads after that.
const RUNTIME_WAIT: Duration = Duration::from_secs(1);

/// What the core tells the frontend about an account. Every event comes with the account's id.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Event {
    /// The first event of an account. The accounts are listed in the order of their `Added`.
    Added,
    /// The last event of an account: it is gone, and nothing comes for it afterwards. Comes after
    /// everything the account emitted, the events of its logout included.
    Removed,
    /// Who the account is, so that a frontend can list it and prefill its login while it is not
    /// logged in. Comes when it becomes known or changes: at startup for a stored account, right
    /// after `Added` and before its first `Session`, and after a login, before `LoggedIn`, if it
    /// differs from what was sent. Never unset while the account lives.
    Identity(Identity),
    Session(SessionState),
    /// Changes to the room list, to apply in order. After `LoggedIn` it carries the rooms; after
    /// `LoggedOut` it empties the list, unless the list was empty already. Reconnecting brings
    /// what changed meanwhile as inserts, updates, moves and removals, never as a reset.
    Rooms(Vec<RoomChange>),
    /// What the frontend shows as the account's badge: how many rooms show as unread, and the sum
    /// of their mention badges. Comes after the `Rooms` event that changes it, and after a
    /// logout empties the list; not while it stays the same, nor while it is zero from the start.
    Unread(Unread),
    /// Whether the room list holds channel news back (ROOM-011), for
    /// [`AccountHandle::release_channels`] to show (ROOM-012). Comes after the `Rooms` event that
    /// starts or ends the holding, only when the answer flips: `true` with the first held change,
    /// `false` after a release, after the last held room is read or opened, and after a reset or a
    /// logout forgets the news; not while it stays the same, nor while it is `false` from the
    /// start.
    HoldingNews(bool),
    /// The direct messages the room list holds back because they went quiet (ROOM-008), all of
    /// them, the most recently active first, then by id: a frontend that searches the rooms looks
    /// among these too, and never sorts. The `Rooms` changes never mention them. Comes after the
    /// `Rooms` event that changes the set, or the content of one of its rooms, and after a logout
    /// empties it; not while it stays the same, nor while it is empty from the start.
    QuietRooms(Vec<Room>),
    /// Changes to the message list of the open room, to apply in order. Opening a room starts with
    /// a reset, to what is held of it already. Logging out resets to nothing, if a room was open.
    Timeline(Vec<TimelineChange>),
    /// What the frontend shows about the message list. Comes after the changes it goes with, except
    /// when a detached window replaces the messages: the status comes first then, so that the
    /// frontend knows not to stick to the bottom.
    TimelineStatus(TimelineStatus),
    /// The open room holds the message that [`AccountHandle::show_message`] was asked for, for the
    /// frontend to scroll to. Comes once, after the change that brought the message in (or at
    /// once if the room held it). Not state: a frontend that attaches later doesn't get it.
    ShowMessage {
        room: RoomId,
        message: MessageId,
    },
    /// Changes to the list of the open room's threads, to apply in order: the parent messages, the
    /// one with the most recent reply last-replied-to first. Opening the list starts with a reset
    /// to nothing, then a reset to the first page; later pages are inserts at the end, and live
    /// replies move or insert threads at the front. Closing it, and anything that closes it
    /// (another room, a logout, the room leaving), resets it to nothing.
    ThreadList(Vec<ThreadListChange>),
    /// What the frontend shows about the thread list. Comes after the changes it goes with.
    ThreadListStatus(ThreadListStatus),
    /// Changes to the message list of the open thread, to apply in order: the parent first, then
    /// its replies, oldest first. Opening a thread starts with a reset to nothing, as does
    /// closing it, and so does anything that closes it: another room, a logout, the room or the
    /// parent going away.
    ThreadTimeline(Vec<TimelineChange>),
    /// What the frontend shows about the thread's message list. Comes after the changes it goes
    /// with.
    ThreadStatus(ThreadStatus),
    /// The state of the realtime connection of a logged-in session, when it changes. A session
    /// starts `Connected`, and so does the frontend until told otherwise: nothing is sent at login,
    /// and a logout or an expiry resets it without an event.
    Connection(Connection),
    /// The custom emoji of the server, whenever the list changes. Logging out empties it, if it
    /// held any.
    CustomEmoji(CustomEmojis),
    /// The files to show as avatars, for the users and rooms that changed: the first one a user or
    /// room gets, or a new one when the server's differs. After a logout, the ones that go.
    Avatars(Vec<AvatarUpdate>),
    /// The files to show as attachment images, for the images that got one. After a logout, the
    /// ones that go.
    Images(Vec<ImageUpdate>),
    /// The files to show as the images of custom emoji, for the ones a frontend asked for with
    /// [`AccountHandle::want_emoji_images`]: the cached file at once if there is one, a new one
    /// when the download brings different bytes. After a logout, the ones that go.
    EmojiImages(Vec<EmojiImageUpdate>),
    /// What the server lets users do with their own messages, whenever it changes. Logging out
    /// resets it to the default (everything allowed), if it differed.
    MessagePolicy(MessagePolicy),
    /// Who types in the open room, by the names to show, in the order they started. Comes when
    /// the list changes: empty once nobody types, and for the room that closes or is replaced.
    /// Typists in a thread are not in it. The account's own user is never in it.
    Typing {
        room: RoomId,
        names: Vec<String>,
    },
    /// Who types in the open thread (`thread` is its parent message's id), as for `Typing`: empty
    /// for the thread that closes or is replaced by another.
    ThreadTyping {
        thread: MessageId,
        names: Vec<String>,
    },
    /// How an edit, a deletion or a reaction the frontend asked for ended. A refusal that needs no
    /// request (not the user's message, past the time limit...) is answered at once. Nothing comes
    /// if the session ends first.
    MessageAction {
        id: MessageId,
        action: MessageAction,
        outcome: Result<(), ActionError>,
    },
    /// How a command on a room (a favorite, mark as read or unread, leaving) ended. A refusal that
    /// needs no request is answered at once. Nothing comes if the session ends first.
    RoomAction {
        room: RoomId,
        action: RoomAction,
        outcome: Result<(), RoomActionError>,
    },
    /// What the search of the server's directory (see [`AccountHandle::search_directory`]) holds,
    /// whenever it changes: a new query comes at once as `Searching` with no results, then the
    /// answer or the failure. The empty query clears it. A logout clears it, if it held any.
    Directory(Directory),
    /// What the search of a room's members (see [`AccountHandle::search_members`]) holds, whenever
    /// it changes: a new query comes at once as `Searching`, then the answer or the failure. `None`
    /// when the search is cancelled, and on a logout, if there was one.
    MemberSearch(Option<MemberSearch>),
    /// What the search of a room's messages (see [`AccountHandle::search_messages`]) holds,
    /// whenever it changes: a new query comes at once as `Searching`, then the answer or the
    /// failure, and the next page (see [`AccountHandle::more_message_search`]) as `Found` again:
    /// with `more: Loading` at once, then with the messages appended, or with the same messages
    /// and `more: Failed`. `None` when the search is cancelled, and on a logout, if there was one.
    MessageSearch(Option<MessageSearch>),
    /// How something the user asked of a person or channel of the directory (see
    /// [`AccountHandle::open_direct`], [`preview_channel`](AccountHandle::preview_channel) and
    /// [`join_channel`](AccountHandle::join_channel)) goes: `Working` when it starts, then `Done`
    /// or `Failed`. Only one runs at a time. Nothing comes if the session ends first.
    DirectoryAction {
        action: DirectoryAction,
        state: ActionState,
    },
    /// The channel the user looks at without being in it, or `None` when that ends. It is the open
    /// room, but it is not in the room list, does not count as unread, is not marked read on the
    /// server, and can't be written to until the user joins. Comes before the timeline events of
    /// the room it opens, and ends (after the `Rooms` event that lists the channel, if it
    /// does) when another room is opened, the room is closed, the channel is joined, or the
    /// session ends.
    Preview(Option<Room>),
    /// The account's uploads (see [`AccountHandle::upload_file`]), all of them, in the order they
    /// were asked for. Comes whenever one is added, changes state or goes: with its message in
    /// the room's or thread's messages, cancelled, or at a logout or an expired session, which
    /// end them all (an empty list, if there were any). The progress of the one under way does
    /// not come this way: see `UploadProgress`.
    Uploads(Vec<Upload>),
    /// How much of the upload under way has gone, in bytes. Comes at most once for every whole
    /// percent of the file, and only while the upload is `Uploading`. An id the frontend doesn't
    /// know (yet) is to be ignored.
    UploadProgress {
        id: UploadId,
        sent: u64,
    },
}

/// Whether the session is live. The held rooms and messages stay on show while it is not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Connection {
    Connected,
    /// The connection was lost, or couldn't be opened; the next try comes after `retry_in`.
    Waiting {
        retry_in: Duration,
    },
    /// A try is under way.
    Reconnecting,
}

pub(crate) enum Command {
    Login {
        server: String,
        username: String,
        password: SecretString,
    },
    /// Ends the session or the attempt, and forgets the stored account, logged in or not.
    Logout,
    OpenRoom(RoomId),
    LoadOlder,
    ShowMessage(MessageId),
    LoadNewer,
    ShowLatest,
    OpenThread(MessageId),
    CloseThread,
    LoadOlderThread,
    OpenThreadList(ThreadFilter),
    CloseThreadList,
    LoadMoreThreads,
    SendMessage {
        room: RoomId,
        text: String,
        quote: Option<MessageId>,
        thread: Option<MessageId>,
        also_to_channel: bool,
    },
    RetryMessage(MessageId),
    EditMessage {
        id: MessageId,
        text: String,
    },
    DeleteMessage(MessageId),
    React {
        id: MessageId,
        /// The shortcode with its colons.
        emoji: String,
        react: bool,
    },
    RoomAction {
        room: RoomId,
        action: RoomAction,
    },
    ReleaseChannels,
    /// Without colons.
    WantEmojiImages(Vec<String>),
    SearchDirectory(String),
    SearchMembers {
        room: RoomId,
        query: String,
    },
    CancelMemberSearch,
    SearchMessages {
        room: RoomId,
        query: String,
    },
    MoreMessageSearch,
    CancelMessageSearch,
    OpenDirect(String),
    PreviewChannel(RoomId),
    JoinChannel(RoomId),
    UploadFile {
        room: RoomId,
        path: PathBuf,
        text: String,
        thread: Option<MessageId>,
    },
    CancelUpload(UploadId),
    RetryUpload(UploadId),
}

/// The core, as seen from a frontend: commands go in, events come out through the callback given
/// at creation. Commands never block.
///
/// The core runs any number of accounts, and there is always at least one: those stored, or else
/// a new logged-out one at startup, and a new one when the last is removed. See
/// [`add_account`](Self::add_account), [`remove_account`](Self::remove_account) and
/// [`account`](Self::account).
///
/// Dropping the client stops the core. It first lets a pending logout finish (the token revocation
/// and clearing the stored session), which blocks the dropping thread for two seconds at most, and
/// a little longer in the worst case. Don't drop it from async code; the GUI thread owns it.
pub struct Client {
    // Ending the commands is what stops the core.
    commands: Option<UnboundedSender<CoreCommand>>,
    /// Where the accounts' commands go, straight to their actors.
    senders: Arc<ActorSenders>,
    /// Signalled by the core when it has drained and is about to end.
    finished: std_mpsc::Receiver<()>,
    runtime: Option<Runtime>,
    /// The runtime's, for the services that run beside the accounts.
    handle: Handle,
}

impl Client {
    /// A client for real servers. See [`with_ports`](Self::with_ports) for `on_event`. The
    /// environment variable `RC_QUIET_AFTER_SECS` shortens the time after which a quiet direct
    /// message leaves the room list, and `RC_CHANNEL_BATCH_SECS` how often the news of channels
    /// shows (`0` for at once): testing aids, for the GUI tests.
    ///
    /// # Errors
    ///
    /// If the async runtime can't be started.
    pub fn start(
        store: impl SecretStore,
        cache: impl FileCache,
        on_event: impl Fn(AccountId, Event) + Send + Sync + 'static,
    ) -> Result<Self, Error> {
        Self::with_tuning(
            RestConnector,
            store,
            cache,
            SystemClock,
            on_event,
            Tuning::from_env(),
        )
    }

    /// [`start`](Self::start) with the open room's history asking for `page_size` messages at a
    /// time, so that a test of the real server needn't post hundreds of messages to get a window
    /// that doesn't hold the whole room.
    ///
    /// # Errors
    ///
    /// If the async runtime can't be started.
    #[doc(hidden)]
    pub fn start_with_page_size(
        store: impl SecretStore,
        cache: impl FileCache,
        on_event: impl Fn(AccountId, Event) + Send + Sync + 'static,
        page_size: usize,
    ) -> Result<Self, Error> {
        Self::with_tuning(
            RestConnector,
            store,
            cache,
            SystemClock,
            on_event,
            Tuning {
                page_size,
                ..Tuning::from_env()
            },
        )
    }

    /// A client that reaches servers through `connector`, remembers the accounts in `store` and
    /// keeps downloaded files in `cache`, and tells the time by `clock`; tests pass fakes. Every
    /// stored account is added, and resumed if it holds a token.
    ///
    /// `on_event` is called once per event, with the account it belongs to, from a runtime thread.
    /// The events of one account come in order, from `Added` to `Removed`; those of different
    /// accounts may interleave. An account's first session state follows its `Added`: `Resuming`
    /// if a stored session is being resumed, `LoggedOut` otherwise. Until then the state is
    /// unknown. It must return quickly and must not touch GUI objects: a Qt frontend hops to its
    /// GUI thread inside it, e.g. with `CxxQtThread::queue`.
    ///
    /// # Errors
    ///
    /// If the async runtime can't be started.
    pub fn with_ports<C: Connector, S: SecretStore>(
        connector: C,
        store: S,
        cache: impl FileCache,
        clock: impl Clock,
        on_event: impl Fn(AccountId, Event) + Send + Sync + 'static,
    ) -> Result<Self, Error> {
        Self::with_tuning(connector, store, cache, clock, on_event, Tuning::default())
    }

    /// [`with_ports`](Self::with_ports) with the timings set, so that tests don't wait for seconds.
    fn with_tuning<C: Connector, S: SecretStore>(
        connector: C,
        store: S,
        cache: impl FileCache,
        clock: impl Clock,
        on_event: impl Fn(AccountId, Event) + Send + Sync + 'static,
        tuning: Tuning,
    ) -> Result<Self, Error> {
        let runtime = Builder::new_multi_thread()
            .worker_threads(WORKER_THREADS)
            .enable_all()
            .build()
            .map_err(Error::Runtime)?;
        let handle = runtime.handle().clone();
        let (commands, commands_rx) = mpsc::unbounded_channel();
        let senders = Arc::new(ActorSenders::default());
        let finished = core::spawn(
            &runtime,
            core::Ports {
                connector,
                store,
                cache: Arc::new(cache),
                clock: Arc::new(clock),
            },
            on_event,
            tuning,
            core::Link {
                senders: Arc::clone(&senders),
                commands: commands_rx,
            },
        );
        Ok(Self {
            commands: Some(commands),
            senders,
            finished,
            runtime: Some(runtime),
            handle,
        })
    }

    /// Adds an account that is logged out, and returns its id. `Added` comes, then the session
    /// state `LoggedOut`. Log in with [`account`](Self::account).
    pub fn add_account(&self) -> AccountId {
        let id = AccountId::generate();
        // The sender is in place before the core hears of the account, so the commands that
        // follow wait in the channel until the actor runs.
        let (sender, receiver) = mpsc::unbounded_channel();
        self.senders.insert(id.clone(), sender);
        self.send(CoreCommand::Add(id.clone(), receiver));
        id
    }

    /// Ends an account: cancels a login in progress, or logs out and revokes the token, and
    /// forgets the stored account, whether it was logged in or not. The events of the logout
    /// come, then `Removed`, once the account is gone. If it was the last account, a new
    /// logged-out one is added after that. Ignored if there is no such account.
    pub fn remove_account(&self, id: &AccountId) {
        let Some(actor) = self.senders.take(id) else {
            tracing::debug!(%id, "removal of an unknown account dropped");
            return;
        };
        // Fails only if the actor is gone. Dropping the sender ends the actor once it has handled
        // what is queued, the logout last.
        let _ = actor.send(Command::Logout);
        drop(actor);
        self.send(CoreCommand::Remove(id.clone()));
    }

    /// The commands for one account. They are dropped, with a message in the log, if the id isn't
    /// an account (any more).
    #[must_use]
    pub fn account<'a>(&'a self, id: &'a AccountId) -> AccountHandle<'a> {
        AccountHandle { client: self, id }
    }

    /// Starts the GitHub pull requests service beside the accounts, on this client's runtime. It
    /// talks to the API at `api_url` (`github::DEFAULT_API_URL`, or a stub's), reads the token
    /// from `gh` (the process `gh`, or another program in a GUI test) or the `store`, and tells the frontend through `on_event`, once per event in
    /// order, from a runtime thread; it must return quickly and not touch GUI objects, as
    /// `Client`'s must.
    ///
    /// Drop the returned handle before the client: the service stops with the runtime.
    pub fn github(
        &self,
        api_url: &str,
        gh: GhProcess,
        store: impl GithubStore,
        config: GithubConfig,
        on_event: impl Fn(GithubEvent) + Send + Sync + 'static,
    ) -> GitHub {
        let ports = GithubPorts {
            connector: RestGithubConnector::new(api_url),
            gh,
            store,
            clock: SystemClock,
        };
        GitHub::start(&self.handle, ports, config, on_event)
    }

    fn send(&self, command: CoreCommand) {
        let sent = self
            .commands
            .as_ref()
            .is_some_and(|c| c.send(command).is_ok());
        if !sent {
            tracing::error!("the core is gone, command dropped");
        }
    }
}

/// The commands for one account, borrowed from the [`Client`]. They never block, and their
/// effects arrive as that account's events.
#[derive(Clone, Copy)]
pub struct AccountHandle<'a> {
    client: &'a Client,
    id: &'a AccountId,
}

impl AccountHandle<'_> {
    /// Starts logging in. `server` may lack the scheme, https is assumed. The outcome arrives as
    /// events. Ignored unless logged out.
    pub fn login(&self, server: &str, username: &str, password: SecretString) {
        self.send(Command::Login {
            server: server.to_owned(),
            username: username.to_owned(),
            password,
        });
    }

    /// Shows the room's messages: what is held of them at once, then the newest ones as they
    /// arrive. Replaces the room that was open. The room is marked read, on the server and in the
    /// room list, and stays read while it is open. Ignored unless logged in, if the room isn't in the
    /// room list, or if it is the open room already (unless its last load failed: then it tries
    /// again).
    pub fn open_room(&self, room: RoomId) {
        self.send(Command::OpenRoom(room));
    }

    /// Fetches the messages before the oldest one of the open room. Ignored when no room is open,
    /// when a fetch is in flight, or when the start of the room is reached.
    pub fn load_older(&self) {
        self.send(Command::LoadOlder);
    }

    /// Shows a message of the open room, which may be far older than the messages the room holds
    /// (MSG-008). If the room holds it, `found` is `Shown` and `Event::ShowMessage` follows at
    /// once. Else the window around it is fetched, replacing the messages, and the room is
    /// detached from the newest ones (see [`load_newer`](Self::load_newer)), unless the window
    /// joins what is held or reaches the newest message. `found` is `Loading` meanwhile, and
    /// `Missing` if the server has no such message, or it is a thread reply the room doesn't show.
    /// A later call replaces an earlier one that is still loading. Ignored when no room is open.
    pub fn show_message(&self, message: MessageId) {
        self.send(Command::ShowMessage(message));
    }

    /// Fetches the messages after the newest one of a detached window. When they reach the
    /// newest messages of the room the window joins them and the room is attached again. Ignored
    /// when no room is open, when it isn't detached, or when a fetch is in flight.
    pub fn load_newer(&self) {
        self.send(Command::LoadNewer);
    }

    /// Replaces a detached window with what is held of the room, and fetches its newest messages,
    /// as opening the room does. Clears `found`. Sending a message does this first. Ignored when
    /// no room is open; when it isn't detached, it only clears `found`.
    pub fn show_latest(&self) {
        self.send(Command::ShowLatest);
    }

    /// Shows the replies of a message of the open room, in a thread beside the room: a reset to
    /// nothing at once, then the parent and its newest replies as they arrive. Replaces the
    /// thread that was open. The parent must be a sent text message that the open room holds and
    /// that isn't a reply itself; the room needn't hold it (it is fetched if neither the room nor the
    /// thread list does). Anything else is logged at debug and ignored. So is a thread
    /// that is open already, unless its last load failed: then it tries again. Ignored unless
    /// logged in. The thread closes with the room (see [`close_thread`](Self::close_thread)).
    pub fn open_thread(&self, parent: MessageId) {
        self.send(Command::OpenThread(parent));
    }

    /// Closes the open thread, and tells the frontend with a reset to nothing. It closes by itself
    /// when another room is opened, the room is closed or leaves the list, the parent is deleted
    /// and on logout. Ignored when no thread is open.
    pub fn close_thread(&self) {
        self.send(Command::CloseThread);
    }

    /// Fetches the replies before the oldest one the open thread holds. Ignored when no thread is
    /// open, when a fetch is in flight, or when the start of the thread is reached.
    pub fn load_older_thread(&self) {
        self.send(Command::LoadOlderThread);
    }

    /// Lists the threads of the open room, beside it: a reset to nothing at once, then the first
    /// page of the threads that `filter` says, the one with the most recent reply first, as it
    /// arrives, and more when [`load_more_threads`](Self::load_more_threads) asks. Opening it
    /// with another filter switches to that one and starts again from the first page; with the
    /// same filter it does nothing, unless the last request failed: then it tries again. Ignored
    /// unless logged in, and with no room open. The list closes with its room.
    pub fn open_thread_list(&self, filter: ThreadFilter) {
        self.send(Command::OpenThreadList(filter));
    }

    /// Closes the thread list, and tells the frontend with a reset to nothing. It closes by itself
    /// when another room is opened, the room is closed or leaves the list, and on logout. Ignored
    /// when it is not open. The open thread is another matter and stays.
    pub fn close_thread_list(&self) {
        self.send(Command::CloseThreadList);
    }

    /// Fetches the next page of the thread list. Ignored when it is not open, while a request is
    /// in flight, and when its last thread is there; after a failed request it tries again.
    pub fn load_more_threads(&self) {
        self.send(Command::LoadMoreThreads);
    }

    /// Sends a text message to the room. It shows in the room's messages at once, as pending (see
    /// [`Delivery`]), and becomes sent when the server confirms it, or failed if it can't. It
    /// goes out whichever room is open by then. Ignored unless logged in, if the room isn't in the
    /// room list, or if the text is blank.
    ///
    /// With `quote`, the message quotes that one, which shows at once too. It must be a sent text
    /// message that the open room holds, else the message goes out without a quote. The server
    /// makes the quote from a link in the text, but only when the link begins with its own
    /// `Site_Url`, so that is what the links are built from (it is read at login; the address of the
    /// login stands in for a server that has none).
    ///
    /// With `thread`, the message is a reply in that thread, which must be the open one (else the
    /// message is not sent). It shows in the thread's messages at once, as pending, and also in
    /// the room's if `also_to_channel`, which asks the server to show it there too. Without a
    /// thread, `also_to_channel` does nothing.
    pub fn send_message(
        &self,
        room: RoomId,
        text: &str,
        quote: Option<MessageId>,
        thread: Option<MessageId>,
        also_to_channel: bool,
    ) {
        self.send(Command::SendMessage {
            room,
            text: text.to_owned(),
            quote,
            thread,
            also_to_channel,
        });
    }

    /// Sends a message of the open room or thread that failed again, under the same id, so that it
    /// can't appear twice if the first attempt got through after all. Ignored unless it is a
    /// failed message of the open room or thread.
    pub fn retry_message(&self, id: MessageId) {
        self.send(Command::RetryMessage(id));
    }

    /// Uploads the file at `path` to the room, as a message with the file attached and `text`
    /// (trimmed, may be empty). The upload is listed at once (see [`Event::Uploads`]). The
    /// account's uploads run one at a time, in the order they were asked for, so the messages come
    /// in that order; the file is streamed from disk. A file that is too large for the server, a
    /// server that takes no uploads, and a file that can't be read fail at once, without the
    /// network; a failed upload stays listed until it is retried or cancelled, and doesn't hold
    /// up the others. The message appears in the messages like one that arrives from the server,
    /// and the upload leaves the list with it.
    ///
    /// With `thread`, the message is a reply in that thread, which must be the open one (else
    /// nothing happens); it is not also shown in the room. Ignored unless logged in, and if the
    /// room isn't in the room list.
    pub fn upload_file(&self, room: RoomId, path: PathBuf, text: &str, thread: Option<MessageId>) {
        self.send(Command::UploadFile {
            room,
            path,
            text: text.to_owned(),
            thread,
        });
    }

    /// Takes an upload out of the list: one that is queued or failed goes at once, one that is
    /// uploading is stopped. Ignored while the upload is being confirmed, since the message may
    /// be posted already, and for an id that isn't listed.
    pub fn cancel_upload(&self, id: UploadId) {
        self.send(Command::CancelUpload(id));
    }

    /// Puts a failed upload back in the queue, in its place among the others. If the server has
    /// the file already (the failure was the posting of the message), only the posting is tried
    /// again. Ignored unless the upload failed.
    pub fn retry_upload(&self, id: UploadId) {
        self.send(Command::RetryUpload(id));
    }

    /// Replaces the text of a message of the user's in the open room. The answer comes as
    /// [`Event::MessageAction`]. The text is trimmed; a blank one is refused, and one that is the
    /// message's text already succeeds without asking the server. The message keeps its quotes.
    /// Refused at once too if the message isn't in the open room, isn't the user's, isn't sent,
    /// isn't text, or the server's settings don't allow editing it (any more). Ignored unless
    /// logged in.
    pub fn edit_message(&self, id: MessageId, text: &str) {
        self.send(Command::EditMessage {
            id,
            text: text.to_owned(),
        });
    }

    /// Deletes a message of the user's in the open room. The answer comes as
    /// [`Event::MessageAction`], and the message leaves the room's messages with it. Refused at
    /// once under the same conditions as [`edit_message`](Self::edit_message). Ignored unless
    /// logged in.
    pub fn delete_message(&self, id: MessageId) {
        self.send(Command::DeleteMessage(id));
    }

    /// Adds the user's reaction with `emoji` to a message of the open room, or takes it back. The
    /// emoji is a shortcode, with or without colons. The answer comes as [`Event::MessageAction`],
    /// and the reaction shows in the message with it, without waiting for the server's push.
    /// Asking for what is so already succeeds. Refused at once, without a request, if the message
    /// isn't in the open room, isn't sent or isn't text, or the emoji is blank. Ignored unless
    /// logged in.
    pub fn react(&self, id: MessageId, emoji: &str, react: bool) {
        self.send(Command::React {
            id,
            emoji: emoji::shortcode(emoji),
            react,
        });
    }

    /// Marks the room as one of the user's favorites, or takes it back. The answer comes as
    /// [`Event::RoomAction`], and the room moves in the list with it (favorites come first),
    /// without waiting for the server's push. Asking for what is so already succeeds. Refused at
    /// once, without a request, if the room isn't in the list. Ignored unless logged in.
    pub fn set_favorite(&self, room: RoomId, favorite: bool) {
        let action = if favorite {
            RoomAction::Favorite
        } else {
            RoomAction::Unfavorite
        };
        self.send(Command::RoomAction { room, action });
    }

    /// Marks the room read, on the server and in the room list, as opening it does. The answer
    /// comes as [`Event::RoomAction`]. Refused at once, without a request, if the room isn't in
    /// the list. Ignored unless logged in.
    pub fn mark_read(&self, room: RoomId) {
        self.send(Command::RoomAction {
            room,
            action: RoomAction::MarkRead,
        });
    }

    /// Marks the room unread: the server counts one unread message and raises the alert, and the
    /// room list shows that at once. The answer comes as [`Event::RoomAction`]. Refused at once,
    /// without a request, if the room isn't in the list or is the open room, which stays read
    /// while it is open. Ignored unless logged in.
    pub fn mark_unread(&self, room: RoomId) {
        self.send(Command::RoomAction {
            room,
            action: RoomAction::MarkUnread,
        });
    }

    /// Leaves a channel or a private group. The answer comes as [`Event::RoomAction`], and the
    /// room leaves the list with it, and is closed if it was open. The server refuses the last
    /// owner of a room, and that is the answer. Refused at once, without a request, if the room
    /// isn't in the list or is a direct message. Ignored unless logged in.
    pub fn leave_room(&self, room: RoomId) {
        self.send(Command::RoomAction {
            room,
            action: RoomAction::Leave,
        });
    }

    /// Shows the news the room list holds back for channels (ROOM-011) now, whatever the deadline:
    /// the rows turn bold and move into place as at the deadline. That ends the batch, so the next
    /// interval counts from the next held change. Nothing happens if nothing is held. Ignored
    /// unless logged in.
    pub fn release_channels(&self) {
        self.send(Command::ReleaseChannels);
    }

    /// Asks for the image files of the custom emoji `names` (without colons; names or aliases of
    /// the server's list). They come as [`Event::EmojiImages`]. Nothing is downloaded unless a
    /// frontend asks, so one that shows the emoji by URL never does. A name the list doesn't hold
    /// is ignored, and asked for again after the list changes it is looked at anew: ask again
    /// when [`Event::CustomEmoji`] comes.
    pub fn want_emoji_images(&self, names: Vec<String>) {
        self.send(Command::WantEmojiImages(names));
    }

    /// Searches the server's directory for people and public channels matching `text`, for the
    /// room list's search field to add to the user's own rooms. Call it on every change of the
    /// field's text. The text is trimmed; the answer comes as [`Event::Directory`].
    ///
    /// A new text replaces the search that was under way, and its answer is dropped. The server is
    /// asked once the text has not changed for a while (300 ms; `RC_DIRECTORY_DEBOUNCE_MS` sets
    /// it for tests), and the event with the query and `Searching` comes at once. The same text
    /// again does nothing, unless the last search for it failed: then it asks again without
    /// waiting, which is how a frontend retries. An empty text cancels the search and clears the
    /// results. Ignored unless logged in.
    pub fn search_directory(&self, text: &str) {
        self.send(Command::SearchDirectory(text.to_owned()));
    }

    /// Searches the members of `room` whose username or name contain `query` (ignoring case), for
    /// the composer's mention popup: the first [`MEMBERS_PER_SEARCH`](crate::members::MEMBERS_PER_SEARCH)
    /// in the server's order and how many there are in all, as [`Event::MemberSearch`]. Call it
    /// on every change of the text after the `@`; an empty `query` lists the room's first members.
    ///
    /// A new search replaces the one under way, and its answer is dropped. The server is asked once
    /// the query has not changed for a while (300 ms; `RC_MEMBERS_DEBOUNCE_MS` sets it for tests),
    /// and the event with the query and `Searching` comes at once. The same search again does
    /// nothing, unless the last one failed: then it asks again without waiting. A direct message
    /// is answered at once and without a request, with the other person if they match. Ignored
    /// unless logged in.
    pub fn search_members(&self, room: &RoomId, query: &str) {
        self.send(Command::SearchMembers {
            room: room.clone(),
            query: query.to_owned(),
        });
    }

    /// Ends the member search and drops its answer, when the popup closes. The event
    /// `MemberSearch(None)` comes if there was a search.
    pub fn cancel_member_search(&self) {
        self.send(Command::CancelMemberSearch);
    }

    /// Searches the messages of `room` on the server for `query` (trimmed), newest first: the
    /// first [`MESSAGES_PER_SEARCH`](crate::message_search::MESSAGES_PER_SEARCH) of them, and
    /// whether there are more (`More`), as [`Event::MessageSearch`]. The server matches whole words,
    /// ignoring case, and takes `/regex/i` too. The results are a snapshot: they don't follow
    /// edits and deletions.
    ///
    /// A new search replaces the one under way, and its answer is dropped. The server is asked once
    /// the query has not changed for a while (300 ms; `RC_MESSAGE_SEARCH_DEBOUNCE_MS` sets it for
    /// tests), and the event with the query and `Searching` comes at once. The same search again
    /// does nothing, unless the last one failed: then it asks again without waiting. An empty
    /// query asks nothing and is answered with no messages. Ignored unless logged in.
    pub fn search_messages(&self, room: &RoomId, query: &str) {
        self.send(Command::SearchMessages {
            room: room.clone(),
            query: query.to_owned(),
        });
    }

    /// Asks for the next page of the message search, which is appended to the results in the next
    /// `MessageSearch` event, told at once as the same messages with `more: Loading`. A page that
    /// failed (`more: Failed`) is asked for again by calling it again. Does nothing while a page
    /// is on its way, when there is no more, and when there is no search or its first page isn't
    /// here.
    pub fn more_message_search(&self) {
        self.send(Command::MoreMessageSearch);
    }

    /// Ends the message search and drops its answer, when the panel closes. The event
    /// `MessageSearch(None)` comes if there was a search.
    pub fn cancel_message_search(&self) {
        self.send(Command::CancelMessageSearch);
    }

    /// Opens the direct message with the user `username`, which the server creates the first time:
    /// it joins the room list, if it was not in it, and is opened like any room. Give it a
    /// username the directory found: the server makes an empty room for a name nobody has. The
    /// progress comes as [`Event::DirectoryAction`]. Ignored unless logged in, or while another
    /// directory action is under way.
    pub fn open_direct(&self, username: &str) {
        self.send(Command::OpenDirect(username.to_owned()));
    }

    /// Shows a public channel the user is not in as the open room, as a preview (see
    /// [`Event::Preview`]). A channel that is in the room list is just opened. The progress comes
    /// as [`Event::DirectoryAction`]. Ignored unless logged in, while another directory action is
    /// under way, or if the channel is the preview already (then it is opened again, which
    /// retries a load that failed).
    pub fn preview_channel(&self, room: RoomId) {
        self.send(Command::PreviewChannel(room));
    }

    /// Joins a public channel. The channel joins the room list; if it is the preview, it stays
    /// the open room with its messages as they are, and the preview ends. The progress comes as
    /// [`Event::DirectoryAction`]. Ignored unless logged in, while another directory action is
    /// under way, or if the channel is in the room list already.
    pub fn join_channel(&self, room: RoomId) {
        self.send(Command::JoinChannel(room));
    }

    fn send(&self, command: Command) {
        if !self.client.senders.send(self.id, command) {
            tracing::debug!(id = %self.id, "command for an unknown account dropped");
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        self.commands.take();
        self.senders.clear();
        if self.finished.recv_timeout(SHUTDOWN_WAIT).is_err() {
            tracing::warn!("the core didn't finish in time, pending work is lost");
        }
        // A store call stuck on a locked keyring must not hang the exit.
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_timeout(RUNTIME_WAIT);
        }
    }
}
