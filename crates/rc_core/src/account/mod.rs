// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The session actor: owns the session state, and the tasks that report back to it.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use secrecy::SecretString;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::task::{JoinHandle, JoinSet};
use tokio::time::Instant;

use crate::adapters::MemoryMessageStore;
use crate::backoff::Backoff;
use crate::client::{Command, Connection, Event};
use crate::core::Shared;
use crate::domain::{
    AccountId, AvatarOwner, Credentials, Identity, Message, MessageId, Room, RoomId, RoomKind,
    StoredAccount, User,
};
use crate::downloads::Refresh;
use crate::emoji::CustomEmojis;
use crate::policy::{MessageAction, MessagePolicy};
use crate::ports::{
    ApiError, Changes, Clock, Connector, FileCache, MessageStore, Page, Pushed, ServerApi,
    ThreadsPage,
};
use crate::rooms::{RoomAction, RoomList, Unread};
use crate::session::{Established, LoginFailure, SessionState, log_in, resume};
use crate::store_worker::StoreHandle;
use crate::timeline::{Timeline, TimelineError};
use crate::tuning::Tuning;
use crate::typing::Typists;
use crate::users::Users;

mod directory;
mod media;
mod message_actions;
mod open_room;
mod room_actions;
mod sending;
mod sync;
mod thread;
mod thread_list;
mod typing;
mod uploads;

use directory::{DirectoryActionDone, DirectoryDone, DirectorySearch, RunningAction};
use media::{DownloadDone, Media};
use thread::{OpenThread, PendingThread};
use thread_list::OpenThreadList;
use uploads::{UploadConfirmedDone, UploadProgressDone, UploadStoredDone, Uploads};

/// How long, once the actor is told to end, it and the store worker may take to finish what the
/// user asked already: the token revocations and the queued store writes. One bound for both: the
/// actor's deadline is also the store worker's (see [`crate::core::spawn`]). The core stops all
/// the actors at once, so it is one bound for all of them.
pub(crate) const DRAIN_TIMEOUT: Duration = Duration::from_secs(2);

/// A login or resume task's answer.
struct LoginDone<A> {
    attempt: u64,
    /// Whether to remember the session: logins do, resumes already have it stored.
    persist: bool,
    outcome: Result<Established<A>, LoginFailure>,
}

/// A room fetch's answer.
struct RoomsDone {
    /// The sync cycle the fetch belongs to: the answer is stale if another has begun since.
    cycle: u64,
    outcome: Result<Vec<rc_api::Subscription>, ApiError>,
}

/// A custom emoji fetch's answer.
struct EmojiDone {
    /// The sync cycle the fetch belongs to: the answer is stale if another has begun since.
    cycle: u64,
    outcome: Result<Vec<rc_api::CustomEmoji>, ApiError>,
}

/// A message settings fetch's answer.
struct PolicyDone {
    /// The sync cycle the fetch belongs to: the answer is stale if another has begun since.
    cycle: u64,
    outcome: Result<rc_api::MessageSettings, ApiError>,
}

/// What the actor tells an open realtime connection with.
struct RealtimeLinks {
    /// Which room is open.
    watch: UnboundedSender<Option<RoomId>>,
    /// Which thread the user has read.
    read_threads: UnboundedSender<MessageId>,
}

/// What the realtime task tells the actor, in this order: the connection opens or fails, events
/// follow, the connection ends.
enum RealtimeDone {
    /// Opened, with the ways to command the connection.
    Opened(Result<RealtimeLinks, ApiError>),
    Pushed(Box<Pushed>),
    Ended,
}

/// The realtime task's report, tagged with the sync cycle it belongs to: stale if another has begun
/// since.
struct RealtimeReport {
    cycle: u64,
    done: RealtimeDone,
}

/// A catch-up's answer.
struct ChangesDone {
    /// Identifies the request: the answer is stale if the room was closed or another began.
    attempt: u64,
    room: RoomId,
    /// The client's clock when the request was sent.
    started: jiff::Timestamp,
    outcome: Result<Changes, ApiError>,
}

/// A thread fetch's answer.
struct ThreadDone {
    /// Identifies the fetch: the answer is stale if the thread was closed or another fetch began.
    attempt: u64,
    parent: MessageId,
    outcome: Result<Page, ApiError>,
}

/// The answer of the fetch of a thread's parent, which the open room doesn't hold.
struct ThreadParentDone {
    /// Identifies the fetch: the answer is stale if another thread was asked for or the room
    /// changed since.
    attempt: u64,
    outcome: Result<Message, ApiError>,
}

/// The answer of a request for a page of the thread list.
struct ThreadsListDone {
    /// Identifies the request: the answer is stale if the list was closed or another request began.
    attempt: u64,
    outcome: Result<ThreadsPage, ApiError>,
}

/// The answer of the fetch of a parent that became one of the unread threads, for the thread list.
struct ThreadsListParentDone {
    id: MessageId,
    outcome: Result<Message, ApiError>,
}

/// A history fetch's answer.
struct HistoryDone {
    /// Identifies the fetch: the answer is stale if the room was closed or another fetch began.
    attempt: u64,
    room: RoomId,
    outcome: Result<Page, ApiError>,
}

/// A send's answer.
struct SendDone {
    /// The login attempt the send belongs to: the answer is stale if another has begun since.
    attempt: u64,
    room: RoomId,
    id: MessageId,
    outcome: Result<Message, ApiError>,
}

/// A mark-as-read call's answer.
struct MarkReadDone {
    /// The login attempt the call belongs to: the answer is stale if the session has changed.
    attempt: u64,
    room: RoomId,
    outcome: Result<(), ApiError>,
}

/// A command on a room's answer.
struct RoomActionDone {
    /// The login attempt the call belongs to: the answer is stale if the session has changed.
    attempt: u64,
    room: RoomId,
    action: RoomAction,
    outcome: Result<(), ApiError>,
}

/// What an edit, a deletion or a reaction did on the server.
enum Applied {
    /// The message as the server stored it.
    Edited(Box<Message>),
    Deleted,
    Reacted {
        emoji: String,
        react: bool,
    },
}

/// An edit's, a deletion's or a reaction's answer.
struct ActionDone {
    /// The login attempt the call belongs to: the answer is stale if the session has changed.
    attempt: u64,
    room: RoomId,
    id: MessageId,
    action: MessageAction,
    outcome: Result<Applied, ApiError>,
}

/// What the tasks tell the actor.
enum Done<A> {
    Login(LoginDone<A>),
    Rooms(RoomsDone),
    Emoji(EmojiDone),
    Policy(PolicyDone),
    Avatar(DownloadDone<AvatarOwner>),
    Image(DownloadDone<String>),
    EmojiImage(DownloadDone<String>),
    Realtime(RealtimeReport),
    /// The wait after a lost connection is over; the cycle is the one that was lost.
    Reconnect {
        cycle: u64,
    },
    History(HistoryDone),
    Thread(ThreadDone),
    ThreadParent(ThreadParentDone),
    ThreadsList(ThreadsListDone),
    ThreadsListParent(ThreadsListParentDone),
    Changes(ChangesDone),
    Send(SendDone),
    MarkRead(MarkReadDone),
    Action(ActionDone),
    RoomAction(RoomActionDone),
    Directory(DirectoryDone),
    DirectoryAction(DirectoryActionDone),
    UploadProgress(UploadProgressDone),
    UploadStored(UploadStoredDone),
    UploadConfirmed(UploadConfirmedDone),
}

/// A history fetch in flight.
struct Fetch {
    attempt: u64,
    /// Older messages, not the newest.
    older: bool,
    /// The client's clock when the request was sent.
    started: jiff::Timestamp,
    /// A catch-up goes with it (see [`Actor::fetch_changes`]), which is what brings the held
    /// messages up to date, not the page.
    with_changes: bool,
}

/// The room whose messages the frontend shows.
struct OpenRoom {
    timeline: Timeline,
    kind: RoomKind,
    fetch: Option<Fetch>,
    /// Why the last fetch failed.
    error: Option<TimelineError>,
    /// Messages pushed while the newest messages were still to come: the page may predate them,
    /// and merging one first could make the page drop it. They join the page when it arrives.
    live: Vec<Message>,
    /// The attempt id of the catch-up in flight.
    changes: Option<u64>,
    /// The messages deleted while the room was open. An answer to a request sent before the
    /// deletion may still hold them.
    deleted: HashSet<MessageId>,
}

/// What the actor keeps while logged in; the next features need both.
struct Session<A> {
    api: A,
    credentials: Credentials,
    /// What the links of quotes must begin with (see [`Established::site_url`]).
    site_url: String,
}

/// Owns the session state and handles one message at a time. Network calls run in spawned tasks
/// that report back through `done_tx`; secret-store calls are queued for the store worker, which the
/// core shares between accounts.
struct Actor<C: Connector> {
    /// The key of this account in the secret store.
    id: AccountId,
    connector: Arc<C>,
    state: SessionState,
    session: Option<Session<C::Api>>,
    /// What is stored about the account, if anything: what it was started with, or what its last
    /// login saved. Kept so that an expired account can be saved again without its token.
    stored: Option<StoredAccount>,
    /// Who the frontend was last told the account is.
    identity: Option<Identity>,
    /// The users of all the accounts, to claim the one who logs in.
    users: Arc<Users>,
    /// What the frontend was last told the unread totals are.
    unread: Unread,
    /// What the frontend was last told the held-back rooms are.
    quiet: Vec<Room>,
    /// Whether the frontend was last told the room list holds channel news back.
    holding_news: bool,
    /// Identifies the current attempt, so that the answer of a cancelled one is dropped.
    attempt: u64,
    login_task: Option<JoinHandle<()>>,
    /// The room fetch of the current session, while it runs.
    rooms_task: Option<JoinHandle<()>>,
    /// The realtime connection of the current session, while it is open or opening. The task owns
    /// the receiver of the pushed events: ending it closes the connection.
    realtime_task: Option<JoinHandle<()>>,
    /// The connection of the current cycle is open.
    realtime_open: bool,
    /// Tells the open connection which room is open, so that it follows its deletions and typing.
    /// `None` while there is no connection.
    realtime_watch: Option<UnboundedSender<Option<RoomId>>>,
    /// The same connection's way to be told that a thread was read.
    realtime_read: Option<UnboundedSender<MessageId>>,
    /// Counts sync cycles (see [`open_realtime`](Self::open_realtime)), to tell the reports of the
    /// current one from stale ones.
    cycle: u64,
    /// The wait before the next cycle, while it runs.
    reconnect_task: Option<JoinHandle<()>>,
    /// How long to wait before the next cycle.
    backoff: Backoff,
    /// What the frontend was last told about the connection.
    connection: Connection,
    /// When the current cycle had the connection open and the rooms listed. The wait starts over
    /// only if it lasted: see [`Backoff::lasted`].
    succeeded_at: Option<Instant>,
    /// The room list of the session holds a server snapshot: later snapshots are merged into it,
    /// not reset onto it.
    rooms_listed: bool,
    /// What the server pushed since the realtime connection opened, until the rooms are listed:
    /// they are replayed on top of the list then. `None` once they are, and outside a session.
    backlog: Option<Vec<Pushed>>,
    /// The rooms in display order, as the frontend has them.
    rooms: RoomList,
    /// The custom emoji the frontend was last told about.
    emoji: CustomEmojis,
    /// The custom emoji fetch of the current cycle, while it runs.
    emoji_task: Option<JoinHandle<()>>,
    /// What the frontend was last told users may do with their messages.
    policy: MessagePolicy,
    /// The message settings fetch of the current cycle, while it runs.
    policy_task: Option<JoinHandle<()>>,
    cache: Arc<dyn FileCache>,
    /// The time of day.
    clock: Arc<dyn Clock>,
    /// The avatars: which are wanted, queued and shown, and the downloads in flight.
    avatars: Media<AvatarOwner>,
    /// The attachment images, likewise.
    images: Media<String>,
    /// The custom emoji images a frontend asked for, likewise.
    emoji_images: Media<String>,
    /// The messages of the rooms opened so far. Cleared on logout.
    messages: Box<dyn MessageStore>,
    open: Option<OpenRoom>,
    /// The public channel the user looks at without being in it: the open room, though the room
    /// list lacks it. It ends when the channel is listed or another room opens.
    preview: Option<Room>,
    /// The files being uploaded.
    uploads: Uploads,
    /// The search of the server's directory.
    directory: DirectorySearch,
    /// The directory action under way: only one runs at a time.
    directory_action: Option<RunningAction>,
    /// Counts the directory actions, to tell the answer of the current one from a stale one.
    directory_actions: u64,
    /// The thread beside the open room: always one of its messages, and closed with it.
    open_thread: Option<OpenThread>,
    /// Who types in the open room and its threads (see [`Actor::handle_typing`]).
    typists: Typists,
    /// The open room and the names the frontend was last told type in it; `None` while it was
    /// last told that nobody does.
    typing_told: Option<(RoomId, Vec<String>)>,
    /// The same for the open thread, by its parent.
    thread_typing_told: Option<(MessageId, Vec<String>)>,
    /// Counts history and thread fetches, to tell the answer of the current one from a stale one.
    history_attempt: u64,
    /// The history fetch of the open room, while it runs.
    history_task: Option<JoinHandle<()>>,
    /// The thread fetch of the open thread, while it runs.
    thread_task: Option<JoinHandle<()>>,
    /// The thread asked for whose parent the open room doesn't hold, while the parent is fetched.
    /// The thread that is open stays until the parent arrives.
    pending_thread: Option<PendingThread>,
    /// The fetch of that parent, while it runs.
    parent_task: Option<JoinHandle<()>>,
    /// The list of the open room's threads, if the frontend asked for it.
    thread_list: Option<OpenThreadList>,
    /// The request for a page of that list, while it runs.
    thread_list_task: Option<JoinHandle<()>>,
    /// How many threads a request of the thread list asks for.
    threads_page_size: usize,
    /// The fetches of the parents that became unread threads, for the "Unread" list.
    thread_list_parents: JoinSet<()>,
    /// The open thread the connection was asked to mark read, until it is no longer unread.
    read_requested: Option<MessageId>,
    /// The catch-up of the open room (changes since it was last up to date), while it runs.
    changes_task: Option<JoinHandle<()>>,
    /// Token revocations in flight, so that stopping can wait for them.
    revokes: JoinSet<()>,
    /// `None` once stopping.
    store: Option<StoreHandle>,
    done_tx: UnboundedSender<Done<C::Api>>,
    on_event: Arc<dyn Fn(Event) + Send + Sync>,
}

/// An account actor that uses the services of `shared`: the future handles commands and reports
/// until the commands end, then stops, and returns the deadline by which the store worker must be
/// done too (see [`Actor::stop`]).
pub(crate) fn run<C: Connector>(
    shared: &Shared<C>,
    id: AccountId,
    stored: Option<StoredAccount>,
    on_event: impl Fn(Event) + Send + Sync + 'static,
    tuning: Tuning,
    commands: UnboundedReceiver<Command>,
) -> impl Future<Output = Instant> + Send + 'static {
    let (done_tx, done_rx) = mpsc::unbounded_channel();
    Actor::new(shared, id, on_event, tuning, done_tx).run(stored, commands, done_rx)
}

impl<C: Connector> Actor<C> {
    fn new(
        shared: &Shared<C>,
        id: AccountId,
        on_event: impl Fn(Event) + Send + Sync + 'static,
        tuning: Tuning,
        done_tx: UnboundedSender<Done<C::Api>>,
    ) -> Self {
        Self {
            id,
            connector: Arc::clone(&shared.connector),
            state: SessionState::LoggedOut { failure: None },
            session: None,
            stored: None,
            identity: None,
            users: Arc::clone(&shared.users),
            unread: Unread::default(),
            quiet: Vec::new(),
            holding_news: false,
            attempt: 0,
            login_task: None,
            rooms_task: None,
            realtime_task: None,
            realtime_open: false,
            realtime_watch: None,
            realtime_read: None,
            cycle: 0,
            reconnect_task: None,
            backoff: tuning.backoff,
            connection: Connection::Connected,
            succeeded_at: None,
            rooms_listed: false,
            backlog: None,
            rooms: RoomList::new(tuning.quiet_after, tuning.channel_batch),
            emoji: CustomEmojis::default(),
            emoji_task: None,
            policy: MessagePolicy::default(),
            policy_task: None,
            cache: Arc::clone(&shared.cache),
            clock: Arc::clone(&shared.clock),
            avatars: Media::new(Refresh::OncePerSession),
            images: Media::new(Refresh::Never),
            emoji_images: Media::new(Refresh::OncePerSession),
            messages: Box::new(MemoryMessageStore::default()),
            open: None,
            preview: None,
            uploads: Uploads::new(),
            directory: DirectorySearch::new(tuning.directory_debounce),
            directory_action: None,
            directory_actions: 0,
            open_thread: None,
            typists: Typists::default(),
            typing_told: None,
            thread_typing_told: None,
            history_attempt: 0,
            history_task: None,
            thread_task: None,
            pending_thread: None,
            parent_task: None,
            thread_list: None,
            thread_list_task: None,
            threads_page_size: tuning.threads_page_size,
            thread_list_parents: JoinSet::new(),
            read_requested: None,
            changes_task: None,
            revokes: JoinSet::new(),
            store: Some(shared.store.clone()),
            done_tx,
            on_event: Arc::new(on_event),
        }
    }

    async fn run(
        mut self,
        stored: Option<StoredAccount>,
        mut commands: UnboundedReceiver<Command>,
        mut done: UnboundedReceiver<Done<C::Api>>,
    ) -> Instant {
        self.start(stored);
        loop {
            // Woken at the earliest of the actor's deadlines (see `next_deadline`). Made anew every
            // time round, as anything can move it.
            let mut deadline = self.next_deadline().map(|at| self.clock.sleep_until(at));
            tokio::select! {
                biased;
                command = commands.recv() => match command {
                    Some(command) => self.handle_command(command),
                    None => break,
                },
                Some(done) = done.recv() => match done {
                    Done::Login(done) => self.handle_login_done(done),
                    Done::Rooms(done) => self.handle_rooms_done(done),
                    Done::Emoji(done) => self.handle_emoji_done(done),
                    Done::Policy(done) => self.handle_policy_done(done),
                    Done::Avatar(done) => self.handle_avatar_done(done),
                    Done::Image(done) => self.handle_image_done(done),
                    Done::EmojiImage(done) => self.handle_emoji_image_done(done),
                    Done::Realtime(report) => self.handle_realtime_done(report),
                    Done::Reconnect { cycle } => self.handle_reconnect(cycle),
                    Done::History(done) => self.handle_history_done(done),
                    Done::Thread(done) => self.handle_thread_done(done),
                    Done::ThreadParent(done) => self.handle_thread_parent_done(done),
                    Done::ThreadsList(done) => self.handle_threads_list_done(done),
                    Done::ThreadsListParent(done) => self.handle_threads_list_parent_done(done),
                    Done::Changes(done) => self.handle_changes_done(done),
                    Done::Send(done) => self.handle_send_done(done),
                    Done::MarkRead(done) => self.handle_mark_read_done(done),
                    Done::Action(done) => self.handle_action_done(done),
                    Done::RoomAction(done) => self.handle_room_action_done(done),
                    Done::Directory(done) => self.handle_directory_done(done),
                    Done::DirectoryAction(done) => self.handle_directory_action_done(done),
                    Done::UploadProgress(done) => self.handle_upload_progress(&done),
                    Done::UploadStored(done) => self.handle_upload_stored(done),
                    Done::UploadConfirmed(done) => self.handle_upload_confirmed(done),
                },
                () = async {
                    match &mut deadline {
                        Some(sleep) => sleep.await,
                        None => std::future::pending().await,
                    }
                } => self.handle_deadline(),
            }
        }
        self.stop().await
    }

    /// When the actor next has something to do without being asked: the earliest deadline of the
    /// concerns that have one. `None` if none has.
    fn next_deadline(&self) -> Option<jiff::Timestamp> {
        [
            self.rooms.next_quiet(),
            self.rooms.next_release(),
            self.typists.next_deadline(),
            self.directory.due(),
        ]
        .into_iter()
        .flatten()
        .min()
    }

    /// Does what the deadlines that have passed ask for. Each concern checks its own against the
    /// clock, so one that is not due changes nothing.
    fn handle_deadline(&mut self) {
        self.hide_quiet_rooms();
        self.release_channels();
        self.expire_typists();
        self.start_due_directory_search();
    }

    /// Tells who the stored account is, if there is one, and resumes it if it holds a token. A stored
    /// account without one has expired: it starts logged out with `SessionExpired`. With nothing
    /// stored it is logged out for no reason. Before any command is handled.
    fn start(&mut self, stored: Option<StoredAccount>) {
        let Some(stored) = stored else {
            self.set_state(SessionState::LoggedOut { failure: None });
            return;
        };
        self.announce(Identity {
            server: stored.server.clone(),
            username: stored.username.clone(),
        });
        match stored.credentials() {
            Some(credentials) => self.resume(&stored.server, credentials),
            None => self.set_state(SessionState::LoggedOut {
                failure: Some(LoginFailure::SessionExpired),
            }),
        }
        self.stored = Some(stored);
    }

    /// Tells the frontend who the account is, unless it knows already.
    fn announce(&mut self, identity: Identity) {
        if self.identity.as_ref() != Some(&identity) {
            self.identity = Some(identity.clone());
            (self.on_event)(Event::Identity(identity));
        }
    }

    /// Ends the attempt in flight, drops its handle on the store worker, and waits for the token
    /// revocations the user asked for already. Returns the deadline of that wait, [`DRAIN_TIMEOUT`]
    /// from now: the store worker has until then to empty its queue.
    async fn stop(&mut self) -> Instant {
        if let Some(task) = self.login_task.take() {
            task.abort();
        }
        if let Some(task) = self.rooms_task.take() {
            task.abort();
        }
        if let Some(task) = self.emoji_task.take() {
            task.abort();
        }
        if let Some(task) = self.policy_task.take() {
            task.abort();
        }
        self.abort_realtime_task();
        self.avatars.abort();
        self.images.abort();
        self.emoji_images.abort();
        self.abort_history_task();
        self.abort_directory();
        self.abort_uploads();
        if let Some(task) = self.thread_task.take() {
            task.abort();
        }
        self.cancel_pending_thread();
        if let Some(task) = self.thread_list_task.take() {
            task.abort();
        }
        self.thread_list_parents.abort_all();
        drop(self.store.take());
        let deadline = Instant::now() + DRAIN_TIMEOUT;
        let revokes = &mut self.revokes;
        let drain = async { while revokes.join_next().await.is_some() {} };
        if tokio::time::timeout_at(deadline, drain).await.is_err() {
            tracing::warn!("pending token revocations didn't finish in time");
        }
        deadline
    }

    fn handle_command(&mut self, command: Command) {
        match command {
            Command::Login {
                server,
                username,
                password,
            } => self.login(server, username, password),
            Command::Logout => self.logout(),
            Command::OpenRoom(room) => self.open_room(room),
            Command::LoadOlder => self.load_older(),
            Command::OpenThread(parent) => self.open_thread(&parent),
            Command::CloseThread => self.close_thread(),
            Command::LoadOlderThread => self.load_older_thread(),
            Command::OpenThreadList(filter) => self.open_thread_list(filter),
            Command::CloseThreadList => self.close_thread_list(),
            Command::LoadMoreThreads => self.load_more_threads(),
            Command::SendMessage {
                room,
                text,
                quote,
                thread,
                also_to_channel,
            } => self.send_message(room, &text, quote, thread, also_to_channel),
            Command::RetryMessage(id) => self.retry_message(&id),
            Command::EditMessage { id, text } => self.edit_own_message(id, &text),
            Command::DeleteMessage(id) => self.delete_own_message(id),
            Command::React { id, emoji, react } => self.react_to_message(id, emoji, react),
            Command::RoomAction { room, action } => self.act_on_room(room, action),
            Command::ReleaseChannels => {
                tracing::info!("F5: releasing the held channel news");
                self.release_held_channels();
            }
            Command::WantEmojiImages(names) => self.want_emoji_images(names),
            Command::SearchDirectory(text) => self.search_directory(&text),
            Command::OpenDirect(username) => self.open_direct(username),
            Command::PreviewChannel(room) => self.preview_channel(room),
            Command::JoinChannel(room) => self.join_channel(room),
            Command::UploadFile {
                room,
                path,
                text,
                thread,
            } => self.upload_file(room, path, &text, thread),
            Command::CancelUpload(id) => self.cancel_upload(id),
            Command::RetryUpload(id) => self.retry_upload(id),
        }
    }

    fn login(&mut self, server: String, username: String, password: SecretString) {
        if !matches!(self.state, SessionState::LoggedOut { .. }) {
            tracing::debug!("login ignored: not logged out");
            return;
        }
        let connector = Arc::clone(&self.connector);
        // The password lives and dies in this task.
        self.begin_attempt(SessionState::Connecting, true, async move {
            log_in(&*connector, &server, &username, &password).await
        });
    }

    fn resume(&mut self, server: &str, credentials: Credentials) {
        let connector = Arc::clone(&self.connector);
        let server = server.to_owned();
        self.begin_attempt(SessionState::Resuming, false, async move {
            resume(&*connector, &server, &credentials).await
        });
    }

    /// Goes to `state`, `Connecting` or `Resuming`, and runs `attempt` in a task that reports back.
    fn begin_attempt(
        &mut self,
        state: SessionState,
        persist: bool,
        attempt: impl Future<Output = Result<Established<C::Api>, LoginFailure>> + Send + 'static,
    ) {
        self.attempt += 1;
        let id = self.attempt;
        self.set_state(state);
        let done = self.done_tx.clone();
        self.login_task = Some(tokio::spawn(async move {
            let outcome = attempt.await;
            // Fails only if the actor is gone.
            let _ = done.send(Done::Login(LoginDone {
                attempt: id,
                persist,
                outcome,
            }));
        }));
    }

    fn handle_login_done(&mut self, done: LoginDone<C::Api>) {
        if done.attempt != self.attempt || !self.state.attempting() {
            tracing::debug!("answer of a cancelled login dropped");
            return;
        }
        self.login_task = None;
        match done.outcome {
            Ok(established) => self.logged_in(established, done.persist),
            Err(failure) => {
                // The server refused the stored token for good; other failures may pass.
                if failure == LoginFailure::SessionExpired {
                    self.keep_without_token();
                }
                self.set_state(SessionState::LoggedOut {
                    failure: Some(failure),
                });
            }
        }
    }

    /// The login or the resume succeeded. A login of a user another account is already is undone:
    /// the new token is revoked and nothing is saved.
    fn logged_in(&mut self, established: Established<C::Api>, persist: bool) {
        let Established {
            api,
            credentials,
            account,
            site_url,
        } = established;
        if persist
            && !self
                .users
                .claim(&self.id, &account.server, &credentials.user_id)
        {
            self.revoke(api, credentials);
            self.set_state(SessionState::LoggedOut {
                failure: Some(LoginFailure::AlreadyAdded),
            });
            return;
        }
        self.announce(Identity {
            server: account.server.clone(),
            username: account.user.username.clone(),
        });
        if persist {
            let stored = StoredAccount {
                id: self.id.clone(),
                server: account.server.clone(),
                username: account.user.username.clone(),
                user_id: credentials.user_id.clone(),
                token: Some(credentials.token.clone()),
            };
            if let Some(store) = &self.store {
                store.save(stored.clone());
            }
            self.stored = Some(stored);
        }
        self.session = Some(Session {
            api,
            credentials,
            site_url,
        });
        self.backoff.reset();
        self.set_state(SessionState::LoggedIn(account));
        self.open_realtime();
    }

    /// Ends the session or the attempt in flight, and forgets the account, which is no error if
    /// it isn't stored. Logged out already, it changes nothing else.
    fn logout(&mut self) {
        let stored = self.forget();
        if self.state.attempting() {
            if let Some(task) = self.login_task.take() {
                task.abort();
            }
            // Whatever the aborted task still sends is stale.
            self.attempt += 1;
            let resuming = matches!(self.state, SessionState::Resuming);
            self.set_state(SessionState::LoggedOut { failure: None });
            // Cancelling a resume is a logout of the stored session.
            if resuming
                && let Some(stored) = stored
                && let (Some(credentials), Ok(api)) =
                    (stored.credentials(), self.connector.connect(&stored.server))
            {
                self.revoke(api, credentials);
            }
        } else if matches!(self.state, SessionState::LoggedIn(_)) {
            self.abort_rooms_task();
            self.abort_realtime_task();
            self.set_state(SessionState::LoggedOut { failure: None });
            self.clear_directory();
            self.clear_uploads();
            self.clear_rooms();
            self.clear_emoji();
            self.clear_policy();
            self.clear_avatars();
            self.clear_images();
            self.clear_emoji_images();
            self.clear_timeline();
            if let Some(session) = self.session.take() {
                self.revoke(session.api, session.credentials);
            }
        }
    }

    /// The server refused the token of a logged-in session: drop it, and everything it showed.
    /// It's dead already, so there's nothing to revoke. The account stays listed without a token.
    fn expire_session(&mut self) {
        self.abort_rooms_task();
        self.abort_realtime_task();
        self.session = None;
        self.keep_without_token();
        self.set_state(SessionState::LoggedOut {
            failure: Some(LoginFailure::SessionExpired),
        });
        self.clear_directory();
        self.clear_uploads();
        self.clear_rooms();
        self.clear_emoji();
        self.clear_policy();
        self.clear_avatars();
        self.clear_images();
        self.clear_emoji_images();
        self.clear_timeline();
    }

    /// The logged-in user.
    fn own_user(&self) -> Option<&User> {
        match &self.state {
            SessionState::LoggedIn(account) => Some(&account.user),
            _ => None,
        }
    }

    /// The server refused the token: the account stays stored, without it, so that the frontend
    /// can list it and prefill its login.
    fn keep_without_token(&mut self) {
        if let (Some(store), Some(stored)) = (&self.store, &mut self.stored) {
            stored.token = None;
            store.save(stored.clone());
        }
    }

    /// Forgets the account: its stored entry and its user, so that another account may be that
    /// user. Returns what was stored.
    fn forget(&mut self) -> Option<StoredAccount> {
        if let Some(store) = &self.store {
            store.remove(self.id.clone());
        }
        self.users.release(&self.id);
        self.stored.take()
    }

    /// Revokes the token on the server. Best effort: we're logged out locally either way.
    fn revoke(&mut self, api: C::Api, credentials: Credentials) {
        self.revokes.spawn(async move {
            if let Err(error) = api.logout(&credentials).await {
                tracing::warn!(%error, "couldn't revoke the session on the server");
            }
        });
    }

    fn set_state(&mut self, state: SessionState) {
        match &state {
            SessionState::LoggedOut { failure } => {
                tracing::info!(?failure, "session: logged out");
            }
            SessionState::Connecting => tracing::info!("session: connecting"),
            SessionState::Resuming => tracing::info!("session: resuming"),
            SessionState::LoggedIn(account) => tracing::info!(
                server = %account.server,
                user = %account.user.username,
                "session: logged in"
            ),
        }
        self.connection = Connection::Connected;
        self.state = state.clone();
        (self.on_event)(Event::Session(state));
    }
}
