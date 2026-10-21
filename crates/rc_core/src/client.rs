// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The handle frontends drive the core through, and the session actor behind it.

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::mpsc as std_mpsc;
use std::time::Duration;

use secrecy::SecretString;
use tokio::runtime::{Builder, Runtime};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::sync::{mpsc, oneshot};
use tokio::task::{JoinHandle, JoinSet};
use tokio::time::Instant;

use crate::adapters::{MemoryMessageStore, RestConnector};
use crate::avatars::{AvatarUpdate, Avatars, Fetched, authors_of};
use crate::backoff::Backoff;
use crate::domain::{
    Author, AvatarOwner, Credentials, Delivery, Message, MessageId, MessageKind, Quote, Room,
    RoomId, RoomKind, StoredSession, User, room_from,
};
use crate::emoji::{self, CustomEmojis};
use crate::error::Error;
use crate::images::{self, ImageUpdate, Images, images_of};
use crate::policy::{ActionError, MessageAction, MessagePolicy, can_react};
use crate::ports::{
    ApiError, CacheError, CacheKey, Changes, Connector, FileCache, MessageStore, Page, Pushed,
    Realtime, RoomHistory, SecretStore, ServerApi, StoreError, ThreadTarget,
};
use crate::quote::{permalink, wire_text};
use crate::rooms::{RoomAction, RoomActionError, RoomChange, RoomList};
use crate::session::{Established, LoginFailure, SessionState, log_in, resume};
use crate::timeline::{
    PAGE_SIZE, ThreadStatus, Timeline, TimelineChange, TimelineError, TimelineStatus,
};

#[cfg(test)]
mod tests;

/// Worker threads of the async runtime: networking only, the GUI has its own thread.
const WORKER_THREADS: usize = 2;
/// How long the actor waits, when stopping, for queued store writes and token revocations.
const DRAIN_TIMEOUT: Duration = Duration::from_secs(2);
/// How long dropping a [`Client`] waits for the actor to finish draining.
const SHUTDOWN_WAIT: Duration = Duration::from_secs(3);
/// How long dropping a [`Client`] waits for the runtime's threads after that.
const RUNTIME_WAIT: Duration = Duration::from_secs(1);
/// How much earlier than the time a room was last up to date its changes are asked for: the
/// server's clock may differ from ours.
const CATCH_UP_MARGIN: Duration = Duration::from_secs(60);

/// What the core tells the frontend.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Event {
    Session(SessionState),
    /// Changes to the room list, to apply in order. After `LoggedIn` it carries the rooms; after
    /// `LoggedOut` it empties the list, unless the list was empty already. Reconnecting brings
    /// what changed meanwhile as inserts, updates, moves and removals, never as a reset.
    Rooms(Vec<RoomChange>),
    /// Changes to the message list of the open room, to apply in order. Opening a room starts with
    /// a reset, to what is held of it already. Logging out resets to nothing, if a room was open.
    Timeline(Vec<TimelineChange>),
    /// What the frontend shows about the message list. Comes after the changes it goes with.
    TimelineStatus(TimelineStatus),
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
    /// What the server lets users do with their own messages, whenever it changes. Logging out
    /// resets it to the default (everything allowed), if it differed.
    MessagePolicy(MessagePolicy),
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

enum Command {
    Login {
        server: String,
        username: String,
        password: SecretString,
    },
    Logout,
    OpenRoom(RoomId),
    LoadOlder,
    OpenThread(MessageId),
    CloseThread,
    LoadOlderThread,
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
}

/// The core, as seen from a frontend: commands go in, events come out through the callback given
/// at creation. Commands never block.
///
/// Dropping the client stops the core. It first lets a pending logout finish (the token revocation
/// and clearing the stored session), which blocks the dropping thread for two seconds at most, and
/// a little longer in the worst case. Don't drop it from async code; the GUI thread owns it.
pub struct Client {
    // Ending the commands is what stops the actor.
    commands: Option<UnboundedSender<Command>>,
    /// Signalled by the actor when it has drained and is about to end.
    finished: std_mpsc::Receiver<()>,
    runtime: Option<Runtime>,
}

impl Client {
    /// A client for real servers. See [`with_ports`](Self::with_ports) for `on_event`.
    ///
    /// # Errors
    ///
    /// If the async runtime can't be started.
    pub fn start(
        store: impl SecretStore,
        cache: impl FileCache,
        on_event: impl Fn(Event) + Send + Sync + 'static,
    ) -> Result<Self, Error> {
        Self::with_ports(RestConnector, store, cache, on_event)
    }

    /// A client that reaches servers through `connector`, remembers the session in `store` and
    /// keeps downloaded files in `cache`; tests pass fakes. If the store holds a session, the
    /// client tries to resume it.
    ///
    /// `on_event` is called once per state change, in order, from a runtime thread. The first event
    /// always comes: `Resuming` if a stored session is being resumed, `LoggedOut` otherwise.
    /// Until then the state is unknown. It must return quickly and must not touch GUI objects: a Qt
    /// frontend hops to its GUI thread inside it, e.g. with `CxxQtThread::queue`.
    ///
    /// # Errors
    ///
    /// If the async runtime can't be started.
    pub fn with_ports<C: Connector, S: SecretStore>(
        connector: C,
        store: S,
        cache: impl FileCache,
        on_event: impl Fn(Event) + Send + Sync + 'static,
    ) -> Result<Self, Error> {
        Self::with_backoff(connector, store, cache, on_event, Backoff::default())
    }

    /// [`with_ports`](Self::with_ports) with the waits between reconnects set, so that tests don't
    /// wait for seconds.
    fn with_backoff<C: Connector, S: SecretStore>(
        connector: C,
        store: S,
        cache: impl FileCache,
        on_event: impl Fn(Event) + Send + Sync + 'static,
        backoff: Backoff,
    ) -> Result<Self, Error> {
        let runtime = Builder::new_multi_thread()
            .worker_threads(WORKER_THREADS)
            .enable_all()
            .build()
            .map_err(Error::Runtime)?;
        let (commands, commands_rx) = mpsc::unbounded_channel();
        let (done_tx, done_rx) = mpsc::unbounded_channel();
        let (finished_tx, finished) = std_mpsc::channel();
        let (store_tx, store_rx) = mpsc::unbounded_channel();
        let store_worker = runtime.spawn(run_store_worker(Arc::new(store), store_rx));
        let actor = Actor {
            connector: Arc::new(connector),
            state: SessionState::LoggedOut { failure: None },
            session: None,
            resuming: None,
            attempt: 0,
            login_task: None,
            rooms_task: None,
            realtime_task: None,
            realtime_open: false,
            realtime_watch: None,
            cycle: 0,
            reconnect_task: None,
            backoff,
            connection: Connection::Connected,
            succeeded_at: None,
            rooms_listed: false,
            backlog: None,
            rooms: RoomList::default(),
            emoji: CustomEmojis::default(),
            emoji_task: None,
            policy: MessagePolicy::default(),
            policy_task: None,
            cache: Arc::new(cache),
            avatars: Avatars::default(),
            avatar_tasks: JoinSet::new(),
            images: Images::default(),
            image_tasks: JoinSet::new(),
            messages: Box::new(MemoryMessageStore::default()),
            open: None,
            open_thread: None,
            history_attempt: 0,
            history_task: None,
            thread_task: None,
            changes_task: None,
            revokes: JoinSet::new(),
            store: Some(StoreHandle(store_tx)),
            store_worker,
            done_tx,
            on_event: Arc::new(on_event),
        };
        runtime.spawn(async move {
            actor.run(commands_rx, done_rx).await;
            // Fails only if the client is gone already.
            let _ = finished_tx.send(());
        });
        Ok(Self {
            commands: Some(commands),
            finished,
            runtime: Some(runtime),
        })
    }

    /// Starts logging in. `server` may lack the scheme, https is assumed. The outcome arrives as
    /// events. Ignored unless logged out.
    pub fn login(&self, server: &str, username: &str, password: SecretString) {
        self.send(Command::Login {
            server: server.to_owned(),
            username: username.to_owned(),
            password,
        });
    }

    /// Cancels a login in progress, or logs out and forgets the stored session. Ignored when logged
    /// out.
    pub fn logout(&self) {
        self.send(Command::Logout);
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

    /// Shows the replies of a message of the open room, in a thread beside the room: a reset to
    /// nothing at once, then the parent and its newest replies as they arrive. Replaces the
    /// thread that was open. The parent must be a sent text message that the open room holds and
    /// that isn't a reply itself; anything else is logged at debug and ignored. So is a thread
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

    fn send(&self, command: Command) {
        let sent = self
            .commands
            .as_ref()
            .is_some_and(|c| c.send(command).is_ok());
        if !sent {
            tracing::error!("the session actor is gone, command dropped");
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        self.commands.take();
        if self.finished.recv_timeout(SHUTDOWN_WAIT).is_err() {
            tracing::warn!("the core didn't finish in time, pending work is lost");
        }
        // A store call stuck on a locked keyring must not hang the exit.
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_timeout(RUNTIME_WAIT);
        }
    }
}

enum StoreOp {
    Load(oneshot::Sender<Result<Option<StoredSession>, StoreError>>),
    Save(StoredSession),
    Clear,
}

/// Queues secret-store calls for the store worker.
struct StoreHandle(UnboundedSender<StoreOp>);

impl StoreHandle {
    async fn load(&self) -> Result<Option<StoredSession>, StoreError> {
        let (answer, answered) = oneshot::channel();
        let gone = || StoreError::Unavailable("the store worker is gone".to_owned());
        self.0.send(StoreOp::Load(answer)).map_err(|_| gone())?;
        answered.await.map_err(|_| gone())?
    }

    fn save(&self, session: StoredSession) {
        self.queue(StoreOp::Save(session));
    }

    fn clear(&self) {
        self.queue(StoreOp::Clear);
    }

    fn queue(&self, op: StoreOp) {
        if self.0.send(op).is_err() {
            tracing::warn!("the store worker is gone, the secret store wasn't updated");
        }
    }
}

/// Runs the store calls one at a time, in the order they were queued, off the async threads: they
/// block, and a keyring can wait for the user to unlock it. Ends when the queue is closed and
/// empty.
async fn run_store_worker<S: SecretStore>(store: Arc<S>, mut ops: UnboundedReceiver<StoreOp>) {
    while let Some(op) = ops.recv().await {
        let store = Arc::clone(&store);
        let result = tokio::task::spawn_blocking(move || match op {
            StoreOp::Load(answer) => {
                // The asker may have given up.
                let _ = answer.send(store.load());
                Ok(())
            }
            StoreOp::Save(session) => store.save(&session),
            StoreOp::Clear => store.clear(),
        })
        .await;
        match result {
            Ok(Ok(())) => {}
            Ok(Err(error)) => tracing::warn!(%error, "secret store update failed"),
            Err(error) => tracing::error!(%error, "secret store call panicked"),
        }
    }
}

/// Downloads an avatar and stores it. Without one of its own, a cached file goes.
async fn fetch_avatar<A: ServerApi>(
    api: &A,
    credentials: &Credentials,
    owner: &AvatarOwner,
    key: CacheKey,
    cache: Arc<dyn FileCache>,
) -> Fetched {
    let image = match api.avatar(credentials, owner).await {
        Ok(Some(image)) => image,
        Ok(None) => return forget_avatar(owner, key, cache).await,
        Err(error @ ApiError::Unreachable(_)) => {
            tracing::debug!(%error, ?owner, "can't download the avatar");
            return Fetched::Unreachable;
        }
        // Not the end of the session: the rooms and messages meet the same refusal if the token
        // is dead, and the avatar route may refuse for other reasons.
        Err(error) => {
            tracing::debug!(%error, ?owner, "no avatar");
            return Fetched::Nothing;
        }
    };
    let stored = tokio::task::spawn_blocking(move || {
        cache.put(&key, image.content_type.as_deref(), &image.bytes)
    })
    .await;
    match stored {
        Ok(Ok(stored)) => Fetched::Stored {
            path: stored.path,
            changed: stored.changed,
        },
        Ok(Err(error @ CacheError::UnsupportedType(_))) => {
            tracing::debug!(%error, ?owner, "avatar not kept");
            Fetched::Nothing
        }
        Ok(Err(error)) => {
            tracing::warn!(%error, ?owner, "can't keep the avatar");
            Fetched::Nothing
        }
        Err(error) => {
            tracing::error!(%error, "storing an avatar panicked");
            Fetched::Nothing
        }
    }
}

/// Removes the cached file of an avatar the server doesn't have.
async fn forget_avatar(owner: &AvatarOwner, key: CacheKey, cache: Arc<dyn FileCache>) -> Fetched {
    match tokio::task::spawn_blocking(move || cache.remove(&key)).await {
        Ok(Ok(_)) => {}
        Ok(Err(error)) => tracing::warn!(%error, ?owner, "can't remove the avatar"),
        Err(error) => tracing::error!(%error, "removing an avatar panicked"),
    }
    Fetched::Missing
}

/// Downloads an attachment image and stores it.
async fn fetch_image<A: ServerApi>(
    api: &A,
    credentials: &Credentials,
    path: &str,
    key: CacheKey,
    cache: Arc<dyn FileCache>,
) -> images::Fetched {
    let image = match api.file(credentials, path).await {
        Ok(image) => image,
        Err(error @ ApiError::Unreachable(_)) => {
            tracing::debug!(%error, path, "can't download the image");
            return images::Fetched::Unreachable;
        }
        // Not the end of the session: the rooms and messages meet the same refusal if the token
        // is dead.
        Err(error) => {
            tracing::debug!(%error, path, "no image");
            return images::Fetched::Nothing;
        }
    };
    let stored = tokio::task::spawn_blocking(move || {
        cache.put(&key, image.content_type.as_deref(), &image.bytes)
    })
    .await;
    match stored {
        Ok(Ok(stored)) => images::Fetched::Stored(stored.path),
        Ok(Err(error @ CacheError::UnsupportedType(_))) => {
            tracing::debug!(%error, path, "image not kept");
            images::Fetched::Nothing
        }
        Ok(Err(error)) => {
            tracing::warn!(%error, path, "can't keep the image");
            images::Fetched::Nothing
        }
        Err(error) => {
            tracing::error!(%error, "storing an image panicked");
            images::Fetched::Nothing
        }
    }
}

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

/// What the realtime task tells the actor, in this order: the connection opens or fails, events
/// follow, the connection ends.
enum RealtimeDone {
    /// Opened, with the way to tell the connection which room is open.
    Opened(Result<UnboundedSender<Option<RoomId>>, ApiError>),
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

/// An avatar download's answer.
struct AvatarDone {
    /// The login attempt the download belongs to: the answer is stale if another has begun since.
    attempt: u64,
    owner: AvatarOwner,
    outcome: Fetched,
}

/// An attachment image download's answer.
struct ImageDone {
    /// The login attempt the download belongs to: the answer is stale if another has begun since.
    attempt: u64,
    path: String,
    outcome: images::Fetched,
}

/// A thread fetch's answer.
struct ThreadDone {
    /// Identifies the fetch: the answer is stale if the thread was closed or another fetch began.
    attempt: u64,
    parent: MessageId,
    outcome: Result<Page, ApiError>,
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
    Avatar(AvatarDone),
    Image(ImageDone),
    Realtime(RealtimeReport),
    /// The wait after a lost connection is over; the cycle is the one that was lost.
    Reconnect {
        cycle: u64,
    },
    History(HistoryDone),
    Thread(ThreadDone),
    Changes(ChangesDone),
    Send(SendDone),
    MarkRead(MarkReadDone),
    Action(ActionDone),
    RoomAction(RoomActionDone),
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

/// The thread whose messages the frontend shows beside the open room.
struct OpenThread {
    /// The latest copy of the parent, which joins the timeline with the page that reaches the
    /// start, and follows edits, reactions and reply counts meanwhile.
    parent: Message,
    /// The parent first, then the replies. Never kept: a thread is fetched on every open.
    timeline: Timeline,
    fetch: Option<Fetch>,
    /// Why the last fetch failed.
    error: Option<TimelineError>,
    /// Replies pushed while the newest ones were still to come, as for the room's.
    live: Vec<Message>,
    /// The messages deleted while the thread was open.
    deleted: HashSet<MessageId>,
}

impl OpenThread {
    /// Whether the message is the parent or one of the replies.
    fn concerns(&self, message: &Message) -> bool {
        message.id == self.parent.id || message.thread_id.as_ref() == Some(&self.parent.id)
    }

    /// The replies the server confirmed that are held: where the next page starts.
    fn replies_held(&self) -> usize {
        self.timeline
            .messages()
            .iter()
            .filter(|m| !m.is_unsent() && m.id != self.parent.id)
            .count()
    }
}

/// What the actor keeps while logged in; the next features need both.
struct Session<A> {
    api: A,
    credentials: Credentials,
    /// What the links of quotes must begin with (see [`Established::site_url`]).
    site_url: String,
}

/// Owns the session state and handles one message at a time. Network calls run in spawned tasks
/// that report back through `done_tx`; secret-store calls are queued for the store worker.
struct Actor<C: Connector> {
    connector: Arc<C>,
    state: SessionState,
    session: Option<Session<C::Api>>,
    /// The stored session being resumed while `Resuming`.
    resuming: Option<StoredSession>,
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
    /// Tells the open connection which room is open, so that it follows its deletions. `None`
    /// while there is no connection.
    realtime_watch: Option<UnboundedSender<Option<RoomId>>>,
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
    /// Which avatars are wanted, queued and shown.
    avatars: Avatars,
    /// The avatar downloads in flight.
    avatar_tasks: JoinSet<()>,
    /// Which attachment images are wanted, queued and shown.
    images: Images,
    /// The attachment image downloads in flight.
    image_tasks: JoinSet<()>,
    /// The messages of the rooms opened so far. Cleared on logout.
    messages: Box<dyn MessageStore>,
    open: Option<OpenRoom>,
    /// The thread beside the open room: always one of its messages, and closed with it.
    open_thread: Option<OpenThread>,
    /// Counts history and thread fetches, to tell the answer of the current one from a stale one.
    history_attempt: u64,
    /// The history fetch of the open room, while it runs.
    history_task: Option<JoinHandle<()>>,
    /// The thread fetch of the open thread, while it runs.
    thread_task: Option<JoinHandle<()>>,
    /// The catch-up of the open room (changes since it was last up to date), while it runs.
    changes_task: Option<JoinHandle<()>>,
    /// Token revocations in flight, so that stopping can wait for them.
    revokes: JoinSet<()>,
    /// `None` once stopping.
    store: Option<StoreHandle>,
    store_worker: JoinHandle<()>,
    done_tx: UnboundedSender<Done<C::Api>>,
    on_event: Arc<dyn Fn(Event) + Send + Sync>,
}

impl<C: Connector> Actor<C> {
    async fn run(
        mut self,
        mut commands: UnboundedReceiver<Command>,
        mut done: UnboundedReceiver<Done<C::Api>>,
    ) {
        self.start().await;
        loop {
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
                    Done::Realtime(report) => self.handle_realtime_done(report),
                    Done::Reconnect { cycle } => self.handle_reconnect(cycle),
                    Done::History(done) => self.handle_history_done(done),
                    Done::Thread(done) => self.handle_thread_done(done),
                    Done::Changes(done) => self.handle_changes_done(done),
                    Done::Send(done) => self.handle_send_done(done),
                    Done::MarkRead(done) => self.handle_mark_read_done(done),
                    Done::Action(done) => self.handle_action_done(done),
                    Done::RoomAction(done) => self.handle_room_action_done(done),
                },
            }
        }
        self.stop().await;
    }

    /// Looks for a stored session, before any command is handled.
    async fn start(&mut self) {
        let loaded = match &self.store {
            Some(store) => store.load().await,
            None => Ok(None),
        };
        let stored = loaded.unwrap_or_else(|error| {
            tracing::warn!(%error, "can't read the stored session");
            None
        });
        match stored {
            Some(stored) => self.resume(stored),
            None => self.set_state(SessionState::LoggedOut { failure: None }),
        }
    }

    /// Ends the attempt in flight, then waits, for a bounded time, for what the user asked
    /// already: store writes and token revocations.
    async fn stop(&mut self) {
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
        self.avatar_tasks.abort_all();
        self.image_tasks.abort_all();
        self.abort_history_task();
        if let Some(task) = self.thread_task.take() {
            task.abort();
        }
        drop(self.store.take());
        let revokes = &mut self.revokes;
        let worker = &mut self.store_worker;
        let drain = async {
            while revokes.join_next().await.is_some() {}
            let _ = worker.await;
        };
        if tokio::time::timeout(DRAIN_TIMEOUT, drain).await.is_err() {
            tracing::warn!("pending logout work didn't finish in time");
        }
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

    fn resume(&mut self, stored: StoredSession) {
        let connector = Arc::clone(&self.connector);
        self.resuming = Some(stored.clone());
        self.begin_attempt(SessionState::Resuming, false, async move {
            resume(&*connector, &stored).await
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
        self.resuming = None;
        match done.outcome {
            Ok(established) => {
                if done.persist
                    && let Some(store) = &self.store
                {
                    store.save(StoredSession {
                        server: established.account.server.clone(),
                        credentials: established.credentials.clone(),
                    });
                }
                self.session = Some(Session {
                    api: established.api,
                    credentials: established.credentials,
                    site_url: established.site_url,
                });
                self.backoff.reset();
                self.set_state(SessionState::LoggedIn(established.account));
                self.open_realtime();
            }
            Err(failure) => {
                // The server refused the stored token for good; other failures may pass.
                if failure == LoginFailure::SessionExpired {
                    self.clear_store();
                }
                self.set_state(SessionState::LoggedOut {
                    failure: Some(failure),
                });
            }
        }
    }

    fn logout(&mut self) {
        if self.state.attempting() {
            if let Some(task) = self.login_task.take() {
                task.abort();
            }
            // Whatever the aborted task still sends is stale.
            self.attempt += 1;
            self.set_state(SessionState::LoggedOut { failure: None });
            // Cancelling a resume is a logout of the stored session.
            if let Some(stored) = self.resuming.take() {
                self.clear_store();
                if let Ok(api) = self.connector.connect(&stored.server) {
                    self.revoke(api, stored.credentials);
                }
            }
            return;
        }
        if matches!(self.state, SessionState::LoggedIn(_)) {
            self.abort_rooms_task();
            self.abort_realtime_task();
            self.set_state(SessionState::LoggedOut { failure: None });
            self.clear_rooms();
            self.clear_emoji();
            self.clear_policy();
            self.clear_avatars();
            self.clear_images();
            self.clear_timeline();
            self.clear_store();
            if let Some(session) = self.session.take() {
                self.revoke(session.api, session.credentials);
            }
        } else {
            tracing::debug!("logout ignored: not logged in");
        }
    }

    /// Starts a sync cycle: opens the realtime connection of the session, then (when it is open)
    /// fetches the rooms and, if a room is open, its newest messages. Logging in runs one, and so
    /// does every reconnect. The connection comes before the rooms: what happens between the
    /// fetch and the subscription would be lost otherwise. What the server pushes meanwhile waits
    /// in the backlog.
    ///
    /// A cycle ends when the connection does, or when a fetch fails: the actor waits, with
    /// [`Backoff`], and starts another.
    fn open_realtime(&mut self) {
        let Some(session) = &self.session else {
            return;
        };
        self.cycle += 1;
        let cycle = self.cycle;
        self.realtime_open = false;
        self.realtime_watch = None;
        self.succeeded_at = None;
        self.backlog = Some(Vec::new());
        let api = session.api.clone();
        let credentials = session.credentials.clone();
        let done = self.done_tx.clone();
        self.realtime_task = Some(tokio::spawn(async move {
            let report = |done| Done::Realtime(RealtimeReport { cycle, done });
            let Realtime { mut events, watch } = match api.realtime(&credentials).await {
                Ok(realtime) => realtime,
                Err(error) => {
                    // Fails only if the actor is gone.
                    let _ = done.send(report(RealtimeDone::Opened(Err(error))));
                    return;
                }
            };
            if done.send(report(RealtimeDone::Opened(Ok(watch)))).is_err() {
                return;
            }
            while let Some(event) = events.recv().await {
                if done
                    .send(report(RealtimeDone::Pushed(Box::new(event))))
                    .is_err()
                {
                    return;
                }
            }
            let _ = done.send(report(RealtimeDone::Ended));
        }));
    }

    fn handle_realtime_done(&mut self, report: RealtimeReport) {
        // A cycle that ended is waiting for the next one: its queued reports are moot.
        if report.cycle != self.cycle
            || self.reconnect_task.is_some()
            || !matches!(self.state, SessionState::LoggedIn(_))
        {
            tracing::debug!("report of a stale realtime connection dropped");
            return;
        }
        match report.done {
            RealtimeDone::Opened(Ok(watch)) => {
                self.realtime_open = true;
                self.realtime_watch = Some(watch);
                if let Some(open) = &self.open {
                    self.watch_room(Some(open.timeline.room().clone()));
                }
                self.fetch_rooms();
            }
            RealtimeDone::Opened(Err(ApiError::Unauthorized)) => self.expire_session(),
            RealtimeDone::Opened(Err(error)) => {
                tracing::warn!(%error, "can't open the realtime connection");
                self.end_cycle();
                // The first rooms are worth having without live updates.
                if !self.rooms_listed {
                    self.fetch_rooms();
                }
            }
            RealtimeDone::Pushed(event) => match &mut self.backlog {
                Some(backlog) => backlog.push(*event),
                None => self.handle_pushed(*event),
            },
            RealtimeDone::Ended => {
                tracing::warn!("the realtime connection ended");
                self.end_cycle();
            }
        }
    }

    /// Gives up the current cycle: drops the connection and whatever else it was doing, and starts
    /// the wait for the next one. The held rooms and messages stay on show meanwhile. What the
    /// cycle's tasks still had queued is dropped as stale while the wait lasts.
    fn end_cycle(&mut self) {
        self.abort_rooms_task();
        self.abort_realtime_task();
        self.realtime_open = false;
        // A connection that dropped soon after it worked is no better than one that never did.
        if let Some(since) = self.succeeded_at.take()
            && self.backoff.lasted(since.elapsed())
        {
            self.backoff.reset();
        }
        let delay = self.backoff.next_delay();
        tracing::info!(?delay, "reconnecting after a wait");
        self.set_connection(Connection::Waiting { retry_in: delay });
        let done = self.done_tx.clone();
        let cycle = self.cycle;
        self.reconnect_task = Some(tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            // Fails only if the actor is gone.
            let _ = done.send(Done::Reconnect { cycle });
        }));
    }

    fn handle_reconnect(&mut self, cycle: u64) {
        if cycle != self.cycle || !matches!(self.state, SessionState::LoggedIn(_)) {
            tracing::debug!("end of a stale wait dropped");
            return;
        }
        self.reconnect_task = None;
        self.set_connection(Connection::Reconnecting);
        self.open_realtime();
    }

    fn set_connection(&mut self, connection: Connection) {
        if self.connection != connection {
            self.connection = connection;
            (self.on_event)(Event::Connection(connection));
        }
    }

    /// Puts what the server pushed into the state and tells the frontend.
    fn handle_pushed(&mut self, event: Pushed) {
        match event {
            Pushed::Subscription(subscription) => {
                let id = RoomId::from(subscription.room_id.clone());
                // The server doesn't push the time of the last message with the subscription.
                let known = self.rooms.get(&id).cloned();
                match room_from(subscription) {
                    Some(mut room) => {
                        // Nor does it push the avatar of the room.
                        if let Some(known) = known {
                            room.last_activity = known.last_activity;
                            room.avatar_etag = known.avatar_etag;
                        }
                        let room = self.seen_if_open(room);
                        let owner = room.avatar_to_fetch();
                        if let Some(change) = self.rooms.upsert(room) {
                            (self.on_event)(Event::Rooms(vec![change]));
                        }
                        self.want_avatars(owner);
                    }
                    // Hidden now, or not a kind we list.
                    None => self.remove_room(&id),
                }
            }
            Pushed::SubscriptionRemoved(id) => self.remove_room(&id),
            Pushed::Activity { room, at } => {
                if let Some(change) = self.rooms.bump(&room, at) {
                    (self.on_event)(Event::Rooms(vec![change]));
                }
            }
            Pushed::Message(message) => self.add_live_message(message),
            Pushed::MessageDeleted { room, id } => self.delete_message(&room, &id),
            // The server announces everybody's: only what was looked at is looked at again, and
            // the rooms of the list, as a room that had no avatar may have one now.
            Pushed::AvatarChanged(owner) => {
                let seen = self.avatars.invalidate(&owner);
                let listed =
                    matches!(&owner, AvatarOwner::Room(id) if self.rooms.get(id).is_some());
                if seen || listed {
                    self.want_avatars([owner]);
                }
            }
        }
    }

    /// The room as the user sees it: nothing is unread in the open room, whatever the server counts.
    /// A message that arrives while the room is open is read at once, so the server is told too.
    fn seen_if_open(&self, room: Room) -> Room {
        let is_open = self
            .open
            .as_ref()
            .is_some_and(|open| open.timeline.room() == &room.id);
        if !is_open || !room.has_unread() {
            return room;
        }
        self.mark_read(room.id.clone());
        room.read()
    }

    /// Asks the server to mark the room read. The answer only matters if it fails.
    fn mark_read(&self, room: RoomId) {
        let Some(session) = &self.session else {
            return;
        };
        let attempt = self.attempt;
        let api = session.api.clone();
        let credentials = session.credentials.clone();
        let done = self.done_tx.clone();
        tokio::spawn(async move {
            let outcome = api.mark_read(&credentials, &room).await;
            // Fails only if the actor is gone.
            let _ = done.send(Done::MarkRead(MarkReadDone {
                attempt,
                room,
                outcome,
            }));
        });
    }

    fn handle_mark_read_done(&mut self, done: MarkReadDone) {
        if done.attempt != self.attempt || !matches!(self.state, SessionState::LoggedIn(_)) {
            tracing::debug!("answer of a stale mark-as-read dropped");
            return;
        }
        match done.outcome {
            Ok(()) => {}
            Err(ApiError::Unauthorized) => self.expire_session(),
            Err(error) => {
                tracing::warn!(%error, room = %done.room, "can't mark the room read");
            }
        }
    }

    /// Answers a command on a room the frontend asked for.
    fn answer_room(&self, room: RoomId, action: RoomAction, outcome: Result<(), RoomActionError>) {
        (self.on_event)(Event::RoomAction {
            room,
            action,
            outcome,
        });
    }

    /// Starts a command on a room, unless the rules refuse it: the room must be in the list, a
    /// direct message can't be left, and the open room, which stays read, can't be marked unread.
    fn act_on_room(&mut self, room: RoomId, action: RoomAction) {
        if self.own_user().is_none() {
            tracing::debug!(?action, "room command ignored: not logged in");
            return;
        }
        let is_open = self
            .open
            .as_ref()
            .is_some_and(|open| open.timeline.room() == &room);
        let allowed = self.rooms.get(&room).is_some_and(|held| match action {
            RoomAction::Leave => held.can_leave(),
            RoomAction::MarkUnread => !is_open,
            _ => true,
        });
        if !allowed {
            tracing::debug!(%room, ?action, "refused: not a room the user can do this to");
            self.answer_room(room, action, Err(RoomActionError::NotAllowed(action)));
            return;
        }
        self.request_room_action(room, action);
    }

    /// Asks the server for the command, in a task that reports the outcome.
    fn request_room_action(&self, room: RoomId, action: RoomAction) {
        let (Some(session), Some(held)) = (&self.session, self.rooms.get(&room)) else {
            return;
        };
        let kind = held.kind;
        let api = session.api.clone();
        let credentials = session.credentials.clone();
        let attempt = self.attempt;
        let done = self.done_tx.clone();
        tokio::spawn(async move {
            let outcome = match action {
                RoomAction::Favorite => api.set_favorite(&credentials, &room, true).await,
                RoomAction::Unfavorite => api.set_favorite(&credentials, &room, false).await,
                RoomAction::MarkRead => api.mark_read(&credentials, &room).await,
                RoomAction::MarkUnread => api.mark_unread(&credentials, &room).await,
                RoomAction::Leave => api.leave_room(&credentials, &room, kind).await,
            };
            // Fails only if the actor is gone.
            let _ = done.send(Done::RoomAction(RoomActionDone {
                attempt,
                room,
                action,
                outcome,
            }));
        });
    }

    fn handle_room_action_done(&mut self, done: RoomActionDone) {
        if done.attempt != self.attempt || !matches!(self.state, SessionState::LoggedIn(_)) {
            tracing::debug!("answer of a stale room command dropped");
            return;
        }
        match done.outcome {
            Ok(()) => {
                self.apply_room_action(&done.room, done.action);
                self.answer_room(done.room, done.action, Ok(()));
            }
            Err(ApiError::Unauthorized) => self.expire_session(),
            Err(error) => {
                tracing::warn!(%error, room = %done.room, action = ?done.action, "refused");
                self.answer_room(done.room, done.action, Err(RoomActionError::from(&error)));
            }
        }
    }

    /// Puts what the server did in the room list without waiting for its push, which then changes
    /// nothing: the room is taken as it is now, and what is set is set, not toggled. Nothing if
    /// the room has gone meanwhile.
    fn apply_room_action(&mut self, room: &RoomId, action: RoomAction) {
        match action {
            RoomAction::Favorite => self.change_room(room, |held| Room {
                favorite: true,
                ..held
            }),
            RoomAction::Unfavorite => self.change_room(room, |held| Room {
                favorite: false,
                ..held
            }),
            RoomAction::MarkRead => self.change_room(room, Room::read),
            // What the server's push says: one unread message and the alert, the mentions as
            // they are.
            RoomAction::MarkUnread => self.change_room(room, |held| Room {
                unread: held.unread.max(1),
                alert: true,
                ..held
            }),
            RoomAction::Leave => self.remove_room(room),
        }
    }

    /// Changes a listed room and tells the frontend, unless nothing changes. The open room stays
    /// read, as for a push.
    fn change_room(&mut self, id: &RoomId, change: impl FnOnce(Room) -> Room) {
        let Some(held) = self.rooms.get(id).cloned() else {
            return;
        };
        let room = self.seen_if_open(change(held));
        if let Some(change) = self.rooms.upsert(room) {
            (self.on_event)(Event::Rooms(vec![change]));
        }
    }

    /// Drops the room from the list, and closes it if it was open.
    fn remove_room(&mut self, id: &RoomId) {
        if let Some(change) = self.rooms.remove(id) {
            (self.on_event)(Event::Rooms(vec![change]));
        }
        if self
            .open
            .as_ref()
            .is_some_and(|open| open.timeline.room() == id)
        {
            self.close_room();
        }
    }

    /// Tells the realtime connection, if there is one, which room is open.
    fn watch_room(&self, room: Option<RoomId>) {
        if let Some(watch) = &self.realtime_watch {
            // Fails only if the connection is gone, which the actor hears of separately.
            let _ = watch.send(room);
        }
    }

    /// Takes a deleted message out of the room's messages, open or not, and out of the open
    /// thread. A reply that was held counts one less in its parent, as the server's does: it
    /// doesn't push the parent for a deletion. Deleting the parent closes the thread.
    fn delete_message(&mut self, room: &RoomId, id: &MessageId) {
        let reply_of = self
            .find_message(id)
            .filter(|held| &held.room_id == room)
            .and_then(|held| held.thread_id.clone());
        if let Some(open) = self
            .open
            .as_mut()
            .filter(|open| open.timeline.room() == room)
        {
            open.live.retain(|m| &m.id != id);
            open.deleted.insert(id.clone());
        }
        self.change_timeline(room, |timeline| timeline.remove(id).into_iter().collect());
        if self
            .open_thread
            .as_ref()
            .is_some_and(|thread| &thread.parent.id == id)
        {
            self.close_thread();
        } else if let Some(thread) = self
            .open_thread
            .as_mut()
            .filter(|thread| thread.timeline.room() == room)
        {
            thread.live.retain(|m| &m.id != id);
            thread.deleted.insert(id.clone());
            self.change_thread_timeline(|timeline| timeline.remove(id).into_iter().collect());
        }
        if let Some(parent) = reply_of {
            self.reply_removed(room, &parent);
        }
    }

    /// Counts one reply less in the held copies of the parent. The time of the last reply stays,
    /// as the server's does.
    fn reply_removed(&mut self, room: &RoomId, parent: &MessageId) {
        let Some(mut copy) = self.find_message(parent).cloned() else {
            return;
        };
        if let Some(thread) = self.open_thread.as_ref().filter(|t| &t.parent.id == parent) {
            copy = thread.parent.clone();
        }
        copy.thread_replies = copy.thread_replies.saturating_sub(1);
        self.replace_everywhere(room, &copy);
    }

    /// The held message with this id, in the open room or in the open thread.
    fn find_message(&self, id: &MessageId) -> Option<&Message> {
        self.open
            .as_ref()
            .and_then(|open| open.timeline.message(id))
            .or_else(|| (self.open_thread.as_ref()).and_then(|thread| thread.timeline.message(id)))
    }

    /// Takes in the server's newer copy of a message that is held as confirmed, wherever it is
    /// held: in the room, in the thread, and as the thread's parent.
    fn replace_everywhere(&mut self, room: &RoomId, message: &Message) {
        if let Some(thread) = self
            .open_thread
            .as_mut()
            .filter(|thread| thread.parent.id == message.id)
        {
            thread.parent.clone_from(message);
        }
        self.change_timelines(room, |timeline| {
            timeline.replace(message.clone()).into_iter().collect()
        });
    }

    /// Adds a pushed message to the open room, or to the open thread, or both. Other rooms aren't
    /// held: they load their history when opened, and their unread counts arrive as subscription
    /// changes.
    fn add_live_message(&mut self, message: Message) {
        self.add_live_thread_message(&message);
        if message.shows_in_room() {
            self.add_live_room_message(message);
        }
    }

    /// Adds a pushed message to the open thread: a reply, or a new copy of the parent.
    fn add_live_thread_message(&mut self, message: &Message) {
        let Some(thread) = self
            .open_thread
            .as_mut()
            .filter(|thread| thread.concerns(message))
        else {
            return;
        };
        if message.id == thread.parent.id {
            // A parent with replies isn't deleted but left as a placeholder, and nothing is
            // announced as deleted: the thread has no parent any more.
            if message.is_deleted_parent() {
                self.close_thread();
                return;
            }
            // The room takes its copy as it takes any message's.
            thread.parent.clone_from(message);
            self.change_thread_timeline(|timeline| {
                timeline.replace(message.clone()).into_iter().collect()
            });
            return;
        }
        let confirms = thread
            .timeline
            .message(&message.id)
            .is_some_and(Message::is_unsent);
        let newest_to_come = thread.fetch.as_ref().is_some_and(|fetch| !fetch.older)
            || (!thread.timeline.has_sent() && !thread.timeline.reached_start());
        if newest_to_come {
            thread.live.retain(|m| m.id != message.id);
            thread.live.push(message.clone());
            if !confirms {
                return;
            }
        }
        self.change_thread_timeline(|timeline| timeline.merge_live(message.clone()));
    }

    /// Adds a pushed message to the open room.
    fn add_live_room_message(&mut self, message: Message) {
        let room = message.room_id.clone();
        let echo = self.own_user().is_some_and(|me| me.id == message.author.id);
        let Some(open) = self
            .open
            .as_mut()
            .filter(|open| open.timeline.room() == &room)
        else {
            // The echo of a message sent from a room that isn't open any more.
            if echo {
                self.change_timeline(&room, |timeline| {
                    timeline.settle(message).into_iter().collect()
                });
            }
            return;
        };
        let confirms = open
            .timeline
            .message(&message.id)
            .is_some_and(Message::is_unsent);
        let newest_to_come = open.fetch.as_ref().is_some_and(|fetch| !fetch.older)
            || (!open.timeline.has_sent() && !open.timeline.reached_start());
        if newest_to_come {
            // A later copy of a message replaces the one waiting.
            open.live.retain(|m| m.id != message.id);
            open.live.push(message.clone());
            if !confirms {
                return;
            }
        }
        self.change_timeline(&room, |timeline| timeline.merge_live(message));
    }

    /// Fetches the rooms of the current cycle.
    fn fetch_rooms(&mut self) {
        let Some(session) = &self.session else {
            return;
        };
        let cycle = self.cycle;
        let api = session.api.clone();
        let credentials = session.credentials.clone();
        let done = self.done_tx.clone();
        self.rooms_task = Some(tokio::spawn(async move {
            let outcome = api.subscriptions(&credentials).await;
            // Fails only if the actor is gone.
            let _ = done.send(Done::Rooms(RoomsDone { cycle, outcome }));
        }));
    }

    fn handle_rooms_done(&mut self, done: RoomsDone) {
        if done.cycle != self.cycle || !matches!(self.state, SessionState::LoggedIn(_)) {
            tracing::debug!("answer of a stale room fetch dropped");
            return;
        }
        self.rooms_task = None;
        let subscriptions = match done.outcome {
            Ok(subscriptions) => subscriptions,
            // The token died between the login and the fetch: same as an expired resume. It's
            // dead already, so there's nothing to revoke.
            Err(ApiError::Unauthorized) => {
                self.expire_session();
                return;
            }
            Err(error) => {
                tracing::warn!(%error, "can't list the rooms");
                // A connection that opened but can't be followed by the rooms is no use: the
                // next cycle starts over. If the cycle ended already (the connection never
                // opened), its wait is under way.
                if self.realtime_open {
                    self.end_cycle();
                }
                return;
            }
        };
        let rooms = subscriptions
            .into_iter()
            .filter_map(room_from)
            .map(|room| self.seen_if_open(room))
            .collect();
        let changes = if self.rooms_listed {
            // Reconnecting: the frontend keeps its rows, and gets what changed meanwhile.
            self.rooms.sync(rooms)
        } else {
            vec![self.rooms.reset(rooms)]
        };
        self.rooms_listed = true;
        if !changes.is_empty() {
            (self.on_event)(Event::Rooms(changes));
        }
        self.want_avatars(
            self.rooms
                .rooms()
                .iter()
                .filter_map(Room::avatar_to_fetch)
                .collect::<Vec<_>>(),
        );
        self.fetch_emoji();
        self.fetch_policy();
        if self.realtime_open {
            self.succeeded_at = Some(Instant::now());
            self.set_connection(Connection::Connected);
            self.catch_up_open_room();
        }
        for event in self.backlog.take().unwrap_or_default() {
            self.handle_pushed(event);
        }
    }

    /// Fetches the custom emoji of the current cycle. Messages show without them until they come.
    fn fetch_emoji(&mut self) {
        let Some(session) = &self.session else {
            return;
        };
        if let Some(task) = self.emoji_task.take() {
            task.abort();
        }
        let cycle = self.cycle;
        let api = session.api.clone();
        let credentials = session.credentials.clone();
        let done = self.done_tx.clone();
        self.emoji_task = Some(tokio::spawn(async move {
            let outcome = api.custom_emoji(&credentials).await;
            // Fails only if the actor is gone.
            let _ = done.send(Done::Emoji(EmojiDone { cycle, outcome }));
        }));
    }

    fn handle_emoji_done(&mut self, done: EmojiDone) {
        let SessionState::LoggedIn(account) = &self.state else {
            return;
        };
        if done.cycle != self.cycle {
            tracing::debug!("answer of a stale custom emoji fetch dropped");
            return;
        }
        self.emoji_task = None;
        let list = match done.outcome {
            Ok(list) => list,
            // The rooms fetch of the cycle meets the same refusal and ends the session.
            Err(error) => {
                tracing::warn!(%error, "can't list the custom emoji");
                return;
            }
        };
        let emoji = CustomEmojis::new(&account.server, &list);
        if emoji != self.emoji {
            self.emoji = emoji.clone();
            (self.on_event)(Event::CustomEmoji(emoji));
        }
    }

    /// Fetches what users may do with their messages in the current cycle. Until it comes, and if
    /// it fails, the policy stays as it was: the server refuses what isn't allowed anyway.
    fn fetch_policy(&mut self) {
        let Some(session) = &self.session else {
            return;
        };
        if let Some(task) = self.policy_task.take() {
            task.abort();
        }
        let cycle = self.cycle;
        let api = session.api.clone();
        let credentials = session.credentials.clone();
        let done = self.done_tx.clone();
        self.policy_task = Some(tokio::spawn(async move {
            let outcome = api.message_settings(&credentials).await;
            // Fails only if the actor is gone.
            let _ = done.send(Done::Policy(PolicyDone { cycle, outcome }));
        }));
    }

    fn handle_policy_done(&mut self, done: PolicyDone) {
        if done.cycle != self.cycle || !matches!(self.state, SessionState::LoggedIn(_)) {
            tracing::debug!("answer of a stale message settings fetch dropped");
            return;
        }
        self.policy_task = None;
        match done.outcome {
            Ok(settings) => {
                let policy = MessagePolicy::from(settings);
                if policy != self.policy {
                    self.policy = policy;
                    (self.on_event)(Event::MessagePolicy(policy));
                }
            }
            // The rooms fetch of the cycle meets the same refusal and ends the session.
            Err(error) => tracing::warn!(%error, "can't read the message settings"),
        }
    }

    /// Wants the avatars of `owners`: what the cache holds goes to the frontend at once, and each
    /// avatar is downloaded again, once per session, in the background.
    fn want_avatars(&mut self, owners: impl IntoIterator<Item = AvatarOwner>) {
        let SessionState::LoggedIn(account) = &self.state else {
            return;
        };
        let server = account.server.clone();
        let cache = &self.cache;
        let updates: Vec<_> = owners
            .into_iter()
            .filter_map(|owner| {
                self.avatars
                    .want(owner, |owner| cache.get(&CacheKey::avatar(&server, owner)))
            })
            .collect();
        if !updates.is_empty() {
            (self.on_event)(Event::Avatars(updates));
        }
        self.start_avatar_downloads(&server);
    }

    fn start_avatar_downloads(&mut self, server: &str) {
        let Some(session) = &self.session else {
            return;
        };
        while let Some(owner) = self.avatars.next_download() {
            let api = session.api.clone();
            let credentials = session.credentials.clone();
            let key = CacheKey::avatar(server, &owner);
            let cache = Arc::clone(&self.cache);
            let attempt = self.attempt;
            let done = self.done_tx.clone();
            self.avatar_tasks.spawn(async move {
                let outcome = fetch_avatar(&api, &credentials, &owner, key, cache).await;
                // Fails only if the actor is gone.
                let _ = done.send(Done::Avatar(AvatarDone {
                    attempt,
                    owner,
                    outcome,
                }));
            });
        }
    }

    fn handle_avatar_done(&mut self, done: AvatarDone) {
        let SessionState::LoggedIn(account) = &self.state else {
            return;
        };
        if done.attempt != self.attempt {
            tracing::debug!("answer of a stale avatar download dropped");
            return;
        }
        let server = account.server.clone();
        if let Some(update) = self.avatars.finished(done.owner, done.outcome) {
            (self.on_event)(Event::Avatars(vec![update]));
        }
        self.start_avatar_downloads(&server);
    }

    /// Forgets the avatars and tells the frontend, unless it had none. The cache keeps its files.
    fn clear_avatars(&mut self) {
        self.avatar_tasks.abort_all();
        let forgotten = self.avatars.reset();
        if !forgotten.is_empty() {
            (self.on_event)(Event::Avatars(forgotten));
        }
    }

    /// Wants the images at `paths`: what the cache holds goes to the frontend at once, and the
    /// others are downloaded in the background.
    fn want_images(&mut self, paths: impl IntoIterator<Item = String>) {
        let SessionState::LoggedIn(account) = &self.state else {
            return;
        };
        let server = account.server.clone();
        let cache = &self.cache;
        let updates: Vec<_> = paths
            .into_iter()
            .filter_map(|path| {
                self.images
                    .want(path, |path| cache.get(&CacheKey::attachment(&server, path)))
            })
            .collect();
        if !updates.is_empty() {
            (self.on_event)(Event::Images(updates));
        }
        self.start_image_downloads(&server);
    }

    fn start_image_downloads(&mut self, server: &str) {
        let Some(session) = &self.session else {
            return;
        };
        while let Some(path) = self.images.next_download() {
            let api = session.api.clone();
            let credentials = session.credentials.clone();
            let key = CacheKey::attachment(server, &path);
            let cache = Arc::clone(&self.cache);
            let attempt = self.attempt;
            let done = self.done_tx.clone();
            self.image_tasks.spawn(async move {
                let outcome = fetch_image(&api, &credentials, &path, key, cache).await;
                // Fails only if the actor is gone.
                let _ = done.send(Done::Image(ImageDone {
                    attempt,
                    path,
                    outcome,
                }));
            });
        }
    }

    fn handle_image_done(&mut self, done: ImageDone) {
        let SessionState::LoggedIn(account) = &self.state else {
            return;
        };
        if done.attempt != self.attempt {
            tracing::debug!("answer of a stale image download dropped");
            return;
        }
        let server = account.server.clone();
        if let Some(update) = self.images.finished(done.path, done.outcome) {
            (self.on_event)(Event::Images(vec![update]));
        }
        self.start_image_downloads(&server);
    }

    /// Forgets the attachment images and tells the frontend, unless it had none. The cache keeps
    /// its files.
    fn clear_images(&mut self) {
        self.image_tasks.abort_all();
        let forgotten = self.images.reset();
        if !forgotten.is_empty() {
            (self.on_event)(Event::Images(forgotten));
        }
    }

    /// Forgets the custom emoji and tells the frontend, unless there were none.
    fn clear_emoji(&mut self) {
        if let Some(task) = self.emoji_task.take() {
            task.abort();
        }
        if self.emoji != CustomEmojis::default() {
            self.emoji = CustomEmojis::default();
            (self.on_event)(Event::CustomEmoji(CustomEmojis::default()));
        }
    }

    /// Resets the message policy to the default and tells the frontend, unless it was that.
    fn clear_policy(&mut self) {
        if let Some(task) = self.policy_task.take() {
            task.abort();
        }
        if self.policy != MessagePolicy::default() {
            self.policy = MessagePolicy::default();
            (self.on_event)(Event::MessagePolicy(MessagePolicy::default()));
        }
    }

    /// Fetches the newest messages of the open room again, after the connection was lost: what was
    /// said meanwhile was not pushed. The page joins what is held (see
    /// [`Timeline::merge_latest`]), and the messages pushed from now on wait for it. The edits and
    /// deletions of the messages held are fetched too (see [`fetch_changes`](Self::fetch_changes)).
    /// A room that has left the list is closed instead.
    fn catch_up_open_room(&mut self) {
        let Some(open) = &self.open else {
            return;
        };
        let id = open.timeline.room().clone();
        if self.rooms.rooms().iter().any(|room| room.id == id) {
            self.abort_history_task();
            self.fetch_history(false);
            if self.open_thread.is_some() {
                self.fetch_thread(false);
            }
        } else {
            self.close_room();
        }
    }

    /// The server refused the token of a logged-in session: forget it, and everything it showed.
    /// It's dead already, so there's nothing to revoke.
    fn expire_session(&mut self) {
        self.abort_rooms_task();
        self.abort_realtime_task();
        self.session = None;
        self.clear_store();
        self.set_state(SessionState::LoggedOut {
            failure: Some(LoginFailure::SessionExpired),
        });
        self.clear_rooms();
        self.clear_emoji();
        self.clear_policy();
        self.clear_avatars();
        self.clear_images();
        self.clear_timeline();
    }

    /// Shows the room, from what is held of it, and fetches its newest messages.
    fn open_room(&mut self, id: RoomId) {
        if !matches!(self.state, SessionState::LoggedIn(_)) {
            tracing::debug!("open_room ignored: not logged in");
            return;
        }
        if self
            .open
            .as_ref()
            .is_some_and(|open| open.timeline.room() == &id && open.error.is_none())
        {
            return;
        }
        let Some(kind) = self
            .rooms
            .rooms()
            .iter()
            .find(|room| room.id == id)
            .map(|room| room.kind)
        else {
            tracing::debug!(room = %id, "open_room ignored: not in the room list");
            return;
        };
        self.close_thread();
        self.abort_history_task();
        let history = self.messages.load(&id);
        let timeline = Timeline::new(id.clone(), history);
        let reset = timeline.reset();
        let authors = authors_of(std::slice::from_ref(&reset));
        let images = images_of(std::slice::from_ref(&reset));
        (self.on_event)(Event::Timeline(vec![reset]));
        self.want_avatars(authors);
        self.want_images(images);
        self.open = Some(OpenRoom {
            timeline,
            kind,
            fetch: None,
            error: None,
            live: Vec::new(),
            changes: None,
            deleted: HashSet::new(),
        });
        self.watch_room(Some(id));
        self.mark_open_room_read();
        self.fetch_history(false);
    }

    /// The user is looking at the open room: nothing in it is unread. The server is told even if
    /// the list shows it read already, as the counts may lag behind.
    fn mark_open_room_read(&mut self) {
        let Some(open) = &self.open else {
            return;
        };
        let id = open.timeline.room().clone();
        self.mark_read(id.clone());
        let Some(room) = self.rooms.get(&id).filter(|room| room.has_unread()) else {
            return;
        };
        if let Some(change) = self.rooms.upsert(room.clone().read()) {
            (self.on_event)(Event::Rooms(vec![change]));
        }
    }

    fn load_older(&mut self) {
        let Some(open) = &self.open else {
            tracing::debug!("load_older ignored: no room is open");
            return;
        };
        // Nothing is held when the first fetch failed: opening the room again retries that.
        if open.fetch.is_some() || open.timeline.reached_start() || open.timeline.oldest().is_none()
        {
            return;
        }
        self.fetch_history(true);
    }

    /// Fetches a page of the open room: the newest, or the one before the oldest message held.
    fn fetch_history(&mut self, older: bool) {
        let (Some(session), Some(open)) = (&self.session, &mut self.open) else {
            return;
        };
        self.history_attempt += 1;
        let attempt = self.history_attempt;
        let started = jiff::Timestamp::now();
        // Only the newest page of a room that was up to date once needs the catch-up.
        let with_changes = !older && open.timeline.history().synced_at.is_some();
        open.fetch = Some(Fetch {
            attempt,
            older,
            started,
            with_changes,
        });
        open.error = None;
        let before = if older { open.timeline.oldest() } else { None };
        let room = open.timeline.room().clone();
        let kind = open.kind;
        let api = session.api.clone();
        let credentials = session.credentials.clone();
        let done = self.done_tx.clone();
        self.history_task = Some(tokio::spawn(async move {
            let outcome = api
                .history(&credentials, &room, kind, before, PAGE_SIZE)
                .await;
            // Fails only if the actor is gone.
            let _ = done.send(Done::History(HistoryDone {
                attempt,
                room,
                outcome,
            }));
        }));
        if with_changes {
            self.fetch_changes();
        }
        self.emit_timeline_status();
    }

    /// Asks for what happened to the open room's messages since it was last up to date: the
    /// newest page can't tell of an edit or a deletion further back, and the connection didn't
    /// push those that happened while it was closed or lost. The request starts a minute earlier
    /// than that, as the server's clock may differ from ours; applying a change twice does
    /// nothing.
    fn fetch_changes(&mut self) {
        let (Some(session), Some(open)) = (&self.session, &mut self.open) else {
            return;
        };
        let Some(synced_at) = open.timeline.history().synced_at else {
            return;
        };
        if let Some(task) = self.changes_task.take() {
            task.abort();
        }
        self.history_attempt += 1;
        let attempt = self.history_attempt;
        open.changes = Some(attempt);
        let started = jiff::Timestamp::now();
        let since = synced_at.checked_sub(CATCH_UP_MARGIN).unwrap_or(synced_at);
        let room = open.timeline.room().clone();
        let api = session.api.clone();
        let credentials = session.credentials.clone();
        let done = self.done_tx.clone();
        self.changes_task = Some(tokio::spawn(async move {
            let outcome = api.changes_since(&credentials, &room, since).await;
            // Fails only if the actor is gone.
            let _ = done.send(Done::Changes(ChangesDone {
                attempt,
                room,
                started,
                outcome,
            }));
        }));
    }

    fn handle_changes_done(&mut self, done: ChangesDone) {
        let Some(open) = &mut self.open else {
            tracing::debug!("answer of a catch-up for a closed room dropped");
            return;
        };
        if open.timeline.room() != &done.room
            || open.changes.take_if(|a| *a == done.attempt).is_none()
        {
            tracing::debug!("answer of a stale catch-up dropped");
            return;
        }
        self.changes_task = None;
        let changes = match done.outcome {
            Ok(changes) => changes,
            Err(ApiError::Unauthorized) => {
                self.expire_session();
                return;
            }
            Err(error) => {
                tracing::warn!(%error, room = %done.room, "can't catch up with the room's changes");
                return;
            }
        };
        open.deleted.extend(changes.deleted.iter().cloned());
        let timeline = &mut open.timeline;
        let mut applied = Vec::new();
        let for_thread = self.open_thread.is_some().then(|| changes.clone());
        // Messages that aren't held come with the newest page.
        for message in changes.updated {
            if message.shows_in_room() && !open.deleted.contains(&message.id) {
                applied.extend(timeline.replace(message));
            }
        }
        for id in &changes.deleted {
            applied.extend(timeline.remove(id));
        }
        timeline.set_synced_at(done.started);
        self.messages.save(&done.room, timeline.history());
        if !applied.is_empty() {
            let authors = authors_of(&applied);
            let images = images_of(&applied);
            (self.on_event)(Event::Timeline(applied));
            self.want_avatars(authors);
            self.want_images(images);
        }
        if let Some(changes) = for_thread {
            self.apply_changes_to_thread(&done.room, changes);
        }
    }

    /// Puts what the catch-up found into the open thread: the new copies of the parent and of the
    /// replies it holds, and the replies deleted. The parent's copy has the count the server has
    /// now, so a deleted reply is not counted again.
    fn apply_changes_to_thread(&mut self, room: &RoomId, changes: Changes) {
        let Some(thread) = self
            .open_thread
            .as_mut()
            .filter(|thread| thread.timeline.room() == room)
        else {
            return;
        };
        thread.deleted.extend(changes.deleted.iter().cloned());
        let mut applied = Vec::new();
        for message in changes.updated {
            if !thread.concerns(&message) || thread.deleted.contains(&message.id) {
                continue;
            }
            if message.id == thread.parent.id {
                thread.parent = message.clone();
            }
            applied.extend(thread.timeline.replace(message));
        }
        for id in &changes.deleted {
            applied.extend(thread.timeline.remove(id));
        }
        self.emit_thread_changes(applied);
    }

    fn handle_history_done(&mut self, done: HistoryDone) {
        let Some(open) = &mut self.open else {
            tracing::debug!("answer of a history fetch for a closed room dropped");
            return;
        };
        let Some(fetch) = open.fetch.take_if(|fetch| fetch.attempt == done.attempt) else {
            tracing::debug!("answer of a stale history fetch dropped");
            return;
        };
        let older = fetch.older;
        self.history_task = None;
        let mut authors = Vec::new();
        let mut images = Vec::new();
        match done.outcome {
            Ok(page) => {
                let mut messages = page.messages;
                messages.retain(|m| !open.deleted.contains(&m.id));
                let changes = if older {
                    open.timeline.merge_older(messages, page.reached_start)
                } else {
                    // What was pushed meanwhile is newer than the page's copy of a message.
                    messages.retain(|m| open.live.iter().all(|live| live.id != m.id));
                    messages.append(&mut open.live);
                    let changes = open.timeline.merge_latest(messages, page.reached_start);
                    // Without a catch-up, the page is what brought the room up to date.
                    if !fetch.with_changes {
                        open.timeline.set_synced_at(fetch.started);
                    }
                    changes
                };
                self.messages.save(&done.room, open.timeline.history());
                if !changes.is_empty() {
                    authors = authors_of(&changes);
                    images = images_of(&changes);
                    (self.on_event)(Event::Timeline(changes));
                }
            }
            Err(ApiError::Unauthorized) => {
                self.expire_session();
                return;
            }
            Err(error) => {
                tracing::warn!(%error, room = %done.room, "can't load the messages");
                open.error = Some(TimelineError::from(&error));
            }
        }
        self.want_avatars(authors);
        self.want_images(images);
        self.emit_timeline_status();
    }

    /// Shows the replies of a message of the open room in a thread, and fetches the newest.
    fn open_thread(&mut self, parent: &MessageId) {
        if !matches!(self.state, SessionState::LoggedIn(_)) {
            tracing::debug!("open_thread ignored: not logged in");
            return;
        }
        if self
            .open_thread
            .as_ref()
            .is_some_and(|thread| &thread.parent.id == parent && thread.error.is_none())
        {
            return;
        }
        let held = self
            .open
            .as_ref()
            .and_then(|open| open.timeline.message(parent))
            .filter(|message| {
                !message.is_unsent()
                    && message.thread_id.is_none()
                    && matches!(message.kind, MessageKind::Text(_))
            })
            .cloned();
        let Some(message) = held else {
            tracing::debug!(%parent, "open_thread ignored: not a sent text message of the open room");
            return;
        };
        // The reset below tells the frontend; the thread that was open has nothing more to say.
        if let Some(task) = self.thread_task.take() {
            task.abort();
        }
        let timeline = Timeline::new(message.room_id.clone(), RoomHistory::default());
        (self.on_event)(Event::ThreadTimeline(vec![timeline.reset()]));
        self.open_thread = Some(OpenThread {
            parent: message,
            timeline,
            fetch: None,
            error: None,
            live: Vec::new(),
            deleted: HashSet::new(),
        });
        self.fetch_thread(false);
    }

    fn load_older_thread(&mut self) {
        let Some(thread) = &self.open_thread else {
            tracing::debug!("load_older_thread ignored: no thread is open");
            return;
        };
        // Nothing is held when the first fetch failed: opening the thread again retries that.
        if thread.fetch.is_some() || thread.timeline.reached_start() || thread.replies_held() == 0 {
            return;
        }
        self.fetch_thread(true);
    }

    /// Fetches a page of the open thread's replies: the newest, or the one after the replies held.
    /// The server counts from the newest, and the replies that arrived since shift it, which the
    /// timeline absorbs by de-duplicating.
    fn fetch_thread(&mut self, older: bool) {
        let (Some(session), Some(thread)) = (&self.session, &mut self.open_thread) else {
            return;
        };
        self.history_attempt += 1;
        let attempt = self.history_attempt;
        thread.fetch = Some(Fetch {
            attempt,
            older,
            started: jiff::Timestamp::now(),
            with_changes: false,
        });
        thread.error = None;
        let offset = if older { thread.replies_held() } else { 0 };
        let parent = thread.parent.id.clone();
        let api = session.api.clone();
        let credentials = session.credentials.clone();
        let done = self.done_tx.clone();
        if let Some(task) = self.thread_task.take() {
            task.abort();
        }
        self.thread_task = Some(tokio::spawn(async move {
            let outcome = api
                .thread_messages(&credentials, &parent, offset, PAGE_SIZE)
                .await;
            // Fails only if the actor is gone.
            let _ = done.send(Done::Thread(ThreadDone {
                attempt,
                parent,
                outcome,
            }));
        }));
        self.emit_thread_status();
    }

    fn handle_thread_done(&mut self, done: ThreadDone) {
        let Some(thread) = &mut self.open_thread else {
            tracing::debug!("answer of a thread fetch for a closed thread dropped");
            return;
        };
        let Some(fetch) = thread.fetch.take_if(|fetch| fetch.attempt == done.attempt) else {
            tracing::debug!("answer of a stale thread fetch dropped");
            return;
        };
        self.thread_task = None;
        let changes = match done.outcome {
            Ok(page) => {
                let mut messages = page.messages;
                messages.retain(|m| !thread.deleted.contains(&m.id));
                // The parent is the first row, and comes with the page that reaches the start.
                if page.reached_start {
                    messages.push(thread.parent.clone());
                }
                if fetch.older {
                    thread.timeline.merge_older(messages, page.reached_start)
                } else {
                    // What was pushed meanwhile is newer than the page's copy of a message.
                    messages.retain(|m| thread.live.iter().all(|live| live.id != m.id));
                    messages.append(&mut thread.live);
                    thread.timeline.merge_latest(messages, page.reached_start)
                }
            }
            Err(ApiError::Unauthorized) => {
                self.expire_session();
                return;
            }
            Err(error) => {
                tracing::warn!(%error, thread = %done.parent, "can't load the thread");
                thread.error = Some(TimelineError::from(&error));
                Vec::new()
            }
        };
        self.emit_thread_changes(changes);
        self.emit_thread_status();
    }

    /// Changes the open thread's messages, which the frontend hears about. Nothing is kept: a
    /// thread is fetched on every open.
    fn change_thread_timeline(
        &mut self,
        change: impl FnOnce(&mut Timeline) -> Vec<TimelineChange>,
    ) {
        let Some(thread) = &mut self.open_thread else {
            return;
        };
        let changes = change(&mut thread.timeline);
        self.emit_thread_changes(changes);
    }

    fn emit_thread_changes(&mut self, changes: Vec<TimelineChange>) {
        if changes.is_empty() {
            return;
        }
        let authors = authors_of(&changes);
        let images = images_of(&changes);
        (self.on_event)(Event::ThreadTimeline(changes));
        self.want_avatars(authors);
        self.want_images(images);
    }

    /// Changes the held copies of messages of `room`, in the open room (or what the store holds
    /// of it) and in the open thread, if it is that room's. For changes to messages that are held
    /// already: a message that isn't is left alone.
    fn change_timelines(
        &mut self,
        room: &RoomId,
        change: impl Fn(&mut Timeline) -> Vec<TimelineChange>,
    ) {
        self.change_timeline(room, &change);
        if self
            .open_thread
            .as_ref()
            .is_some_and(|thread| thread.timeline.room() == room)
        {
            self.change_thread_timeline(&change);
        }
    }

    /// A page of the newest messages may be on its way and predate `message`, which would make it
    /// drop the message: the open room's and the open thread's pages take it in with them.
    fn hold_for_page(&mut self, message: &Message) {
        if message.shows_in_room()
            && let Some(open) = self.open.as_mut().filter(|open| {
                open.timeline.room() == &message.room_id
                    && open.fetch.as_ref().is_some_and(|fetch| !fetch.older)
            })
        {
            open.live.retain(|m| m.id != message.id);
            open.live.push(message.clone());
        }
        if let Some(thread) = self.open_thread.as_mut().filter(|thread| {
            thread.concerns(message)
                && thread.parent.id != message.id
                && thread.fetch.as_ref().is_some_and(|fetch| !fetch.older)
        }) {
            thread.live.retain(|m| m.id != message.id);
            thread.live.push(message.clone());
        }
    }

    fn emit_thread_status(&self) {
        let status = match &self.open_thread {
            Some(thread) => ThreadStatus {
                parent: Some(thread.parent.id.clone()),
                loading: thread.fetch.as_ref().is_some_and(|f| !f.older),
                loading_older: thread.fetch.as_ref().is_some_and(|f| f.older),
                reached_start: thread.timeline.reached_start(),
                error: thread.error.clone(),
            },
            None => ThreadStatus::default(),
        };
        (self.on_event)(Event::ThreadStatus(status));
    }

    /// Closes the open thread and tells the frontend, unless no thread was open.
    fn close_thread(&mut self) {
        if let Some(task) = self.thread_task.take() {
            task.abort();
        }
        if self.open_thread.take().is_some() {
            (self.on_event)(Event::ThreadTimeline(vec![TimelineChange::Reset {
                room: None,
                messages: Vec::new(),
            }]));
            self.emit_thread_status();
        }
    }

    /// The logged-in user.
    fn own_user(&self) -> Option<&User> {
        match &self.state {
            SessionState::LoggedIn(account) => Some(&account.user),
            _ => None,
        }
    }

    /// Changes the messages of `room` and keeps the result: in the open timeline if it is the
    /// room's, which the frontend hears about, else in what the store holds of the room, which
    /// nobody shows.
    fn change_timeline(
        &mut self,
        room: &RoomId,
        change: impl FnOnce(&mut Timeline) -> Vec<TimelineChange>,
    ) {
        if let Some(open) = self
            .open
            .as_mut()
            .filter(|open| open.timeline.room() == room)
        {
            let changes = change(&mut open.timeline);
            if !changes.is_empty() {
                self.messages.save(room, open.timeline.history());
                let authors = authors_of(&changes);
                let images = images_of(&changes);
                (self.on_event)(Event::Timeline(changes));
                self.want_avatars(authors);
                self.want_images(images);
            }
            return;
        }
        let mut timeline = Timeline::new(room.clone(), self.messages.load(room));
        if !change(&mut timeline).is_empty() {
            self.messages.save(room, timeline.history());
        }
    }

    fn send_message(
        &mut self,
        room: RoomId,
        text: &str,
        quote: Option<MessageId>,
        thread: Option<MessageId>,
        also_to_channel: bool,
    ) {
        let Some(me) = self.own_user() else {
            tracing::debug!("send_message ignored: not logged in");
            return;
        };
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        if !self.rooms.rooms().iter().any(|known| known.id == room) {
            tracing::debug!(%room, "send_message ignored: not in the room list");
            return;
        }
        // A reply goes only to the thread that is open.
        let target = match thread {
            Some(parent) => {
                let open = self.open_thread.as_ref().is_some_and(|thread| {
                    thread.parent.id == parent && thread.timeline.room() == &room
                });
                if !open {
                    tracing::debug!(%parent, "send_message ignored: not the open thread");
                    return;
                }
                Some(ThreadTarget {
                    parent,
                    also_to_channel,
                })
            }
            None => None,
        };
        let quotes: Vec<Quote> = quote
            .and_then(|quoted| self.quote_of(&room, &quoted))
            .into_iter()
            .collect();
        let message = Message {
            id: MessageId::generate(),
            room_id: room.clone(),
            author: Author {
                id: me.id.clone(),
                username: me.username.clone(),
                display_name: me.display_name.clone(),
            },
            ts: jiff::Timestamp::now(),
            kind: MessageKind::Text(text.into()),
            delivery: Delivery::Pending,
            attachments: Vec::new(),
            quotes,
            edited: false,
            reactions: Vec::new(),
            thread_id: target.as_ref().map(|target| target.parent.clone()),
            thread_shown: target.as_ref().is_some_and(|target| target.also_to_channel),
            thread_replies: 0,
            thread_last_reply: None,
        };
        let id = message.id.clone();
        let wire = wire_text(&message.quotes, text);
        if target.is_some() {
            self.change_thread_timeline(|timeline| vec![timeline.add_unsent(message.clone())]);
        }
        if message.shows_in_room() {
            self.change_timeline(&room, |timeline| vec![timeline.add_unsent(message)]);
        }
        self.post(room, id, wire, target);
    }

    /// The quote of a message of the open room or thread, as a copy to show until the server's
    /// comes. `None` if the room doesn't hold the message, or the message isn't sent text:
    /// the server can't quote what it doesn't have, and a system message says nothing.
    fn quote_of(&self, room: &RoomId, quoted: &MessageId) -> Option<Quote> {
        let message = self
            .find_message(quoted)
            .filter(|message| &message.room_id == room && !message.is_unsent());
        let Some(Message {
            kind: MessageKind::Text(text),
            author,
            ts,
            ..
        }) = message
        else {
            tracing::debug!(message = %quoted, "no quote: not sent text of the open room");
            return None;
        };
        let Some(site) = self.quote_site() else {
            tracing::debug!("no quote: no server address to make the link from");
            return None;
        };
        let room_info = self.rooms.rooms().iter().find(|known| &known.id == room)?;
        Some(Quote {
            message: Some(quoted.clone()),
            link: permalink(site, room_info, quoted),
            author: author.username.clone(),
            text: text.clone(),
            ts: Some(*ts),
        })
    }

    /// What the quote links must begin with: the server's own `Site_Url`, which is known from the
    /// login on, else the address that was used to log in.
    fn quote_site(&self) -> Option<&str> {
        self.session
            .as_ref()
            .map(|session| session.site_url.as_str())
    }

    fn retry_message(&mut self, id: &MessageId) {
        let Some(Message {
            kind: MessageKind::Text(text),
            delivery: Delivery::Failed,
            quotes,
            room_id,
            thread_id,
            thread_shown,
            ..
        }) = self.find_message(id)
        else {
            tracing::debug!(message = %id, "retry_message ignored: not a failed message");
            return;
        };
        let room = room_id.clone();
        let text = wire_text(quotes, &text.text);
        let target = thread_id.clone().map(|parent| ThreadTarget {
            parent,
            also_to_channel: *thread_shown,
        });
        self.change_timelines(&room, |timeline| {
            timeline
                .set_delivery(id, Delivery::Pending)
                .into_iter()
                .collect()
        });
        self.post(room, id.clone(), text, target);
    }

    /// Sends the message in a task, which reports the outcome.
    fn post(&mut self, room: RoomId, id: MessageId, text: String, thread: Option<ThreadTarget>) {
        let Some(session) = &self.session else {
            return;
        };
        let api = session.api.clone();
        let credentials = session.credentials.clone();
        let attempt = self.attempt;
        let done = self.done_tx.clone();
        tokio::spawn(async move {
            let outcome = api
                .send_message(&credentials, &room, &id, &text, thread.as_ref())
                .await;
            // Fails only if the actor is gone.
            let _ = done.send(Done::Send(SendDone {
                attempt,
                room,
                id,
                outcome,
            }));
        });
    }

    fn handle_send_done(&mut self, done: SendDone) {
        if done.attempt != self.attempt || !matches!(self.state, SessionState::LoggedIn(_)) {
            tracing::debug!("answer of a stale send dropped");
            return;
        }
        match done.outcome {
            Ok(message) => {
                // A page that is on its way may predate the message, and would drop it.
                self.hold_for_page(&message);
                self.change_timelines(&done.room, |timeline| {
                    timeline.settle(message.clone()).into_iter().collect()
                });
            }
            Err(ApiError::Unauthorized) => self.expire_session(),
            Err(error) => {
                tracing::warn!(%error, room = %done.room, "can't send the message");
                self.change_timelines(&done.room, |timeline| {
                    timeline
                        .set_delivery(&done.id, Delivery::Failed)
                        .into_iter()
                        .collect()
                });
            }
        }
    }

    /// Answers an edit or a deletion the frontend asked for.
    fn answer(&self, id: MessageId, action: MessageAction, outcome: Result<(), ActionError>) {
        (self.on_event)(Event::MessageAction {
            id,
            action,
            outcome,
        });
    }

    /// The message of the open room or thread, if it is one the user may do `action` to. Answers
    /// with a refusal otherwise.
    fn changeable(&self, id: &MessageId, action: MessageAction) -> Option<&Message> {
        let me = self.own_user()?;
        let message = self.find_message(id).filter(|message| {
            let now = jiff::Timestamp::now();
            match action {
                MessageAction::Edit => self.policy.can_edit(message, &me.id, now),
                MessageAction::Delete => self.policy.can_delete(message, &me.id, now),
                MessageAction::React => can_react(message),
            }
        });
        if message.is_none() {
            tracing::debug!(message = %id, ?action, "refused: not a message the user can change");
            self.answer(id.clone(), action, Err(ActionError::NotAllowed(action)));
        }
        message
    }

    fn edit_own_message(&mut self, id: MessageId, text: &str) {
        let action = MessageAction::Edit;
        if self.own_user().is_none() {
            tracing::debug!("edit_message ignored: not logged in");
            return;
        }
        let Some(message) = self.changeable(&id, action) else {
            return;
        };
        let room = message.room_id.clone();
        let text = text.trim();
        if text.is_empty() {
            self.answer(id, action, Err(ActionError::Empty));
        } else if matches!(&message.kind, MessageKind::Text(held) if held.text == text) {
            self.answer(id, action, Ok(()));
        } else {
            // The links of the quotes are part of the text on the server: without them it
            // drops the quotes.
            let wire = wire_text(&message.quotes, text);
            self.request_action(room, id, action, Some(wire));
        }
    }

    fn delete_own_message(&mut self, id: MessageId) {
        let action = MessageAction::Delete;
        if self.own_user().is_none() {
            tracing::debug!("delete_message ignored: not logged in");
            return;
        }
        if let Some(message) = self.changeable(&id, action) {
            let room = message.room_id.clone();
            self.request_action(room, id, action, None);
        }
    }

    fn react_to_message(&mut self, id: MessageId, emoji: String, react: bool) {
        let action = MessageAction::React;
        if self.own_user().is_none() {
            tracing::debug!("react ignored: not logged in");
            return;
        }
        if emoji.is_empty() {
            self.answer(id, action, Err(ActionError::NotAllowed(action)));
            return;
        }
        if let Some(message) = self.changeable(&id, action) {
            let room = message.room_id.clone();
            self.request_reaction(room, id, emoji, react);
        }
    }

    /// Asks the server for the reaction, in a task that reports the outcome.
    fn request_reaction(&self, room: RoomId, id: MessageId, emoji: String, react: bool) {
        let Some(session) = &self.session else {
            return;
        };
        let api = session.api.clone();
        let credentials = session.credentials.clone();
        let attempt = self.attempt;
        let done = self.done_tx.clone();
        tokio::spawn(async move {
            let outcome = api
                .react(&credentials, &id, &emoji, react)
                .await
                .map(|()| Applied::Reacted { emoji, react });
            // Fails only if the actor is gone.
            let _ = done.send(Done::Action(ActionDone {
                attempt,
                room,
                id,
                action: MessageAction::React,
                outcome,
            }));
        });
    }

    /// Edits the message to `text`, or deletes it without one, in a task that reports the outcome.
    fn request_action(
        &self,
        room: RoomId,
        id: MessageId,
        action: MessageAction,
        text: Option<String>,
    ) {
        let Some(session) = &self.session else {
            return;
        };
        let api = session.api.clone();
        let credentials = session.credentials.clone();
        let attempt = self.attempt;
        let done = self.done_tx.clone();
        tokio::spawn(async move {
            let outcome = match text {
                Some(text) => api
                    .edit_message(&credentials, &room, &id, &text)
                    .await
                    .map(|message| Applied::Edited(Box::new(message))),
                None => api
                    .delete_message(&credentials, &room, &id)
                    .await
                    .map(|()| Applied::Deleted),
            };
            // Fails only if the actor is gone.
            let _ = done.send(Done::Action(ActionDone {
                attempt,
                room,
                id,
                action,
                outcome,
            }));
        });
    }

    fn handle_action_done(&mut self, done: ActionDone) {
        if done.attempt != self.attempt || !matches!(self.state, SessionState::LoggedIn(_)) {
            tracing::debug!("answer of a stale message action dropped");
            return;
        }
        match done.outcome {
            Ok(Applied::Edited(message)) => {
                // The push of the same copy follows, which changes nothing. A page that is on its
                // way may predate the edit, so the copy joins it, as a sent message does.
                self.hold_for_page(&message);
                self.replace_everywhere(&done.room, &message);
                self.answer(done.id, done.action, Ok(()));
            }
            Ok(Applied::Deleted) => {
                self.delete_message(&done.room, &done.id);
                self.answer(done.id, done.action, Ok(()));
            }
            Ok(Applied::Reacted { emoji, react }) => {
                self.apply_reaction(&done.room, &done.id, &emoji, react);
                self.answer(done.id, done.action, Ok(()));
            }
            Err(ApiError::Unauthorized) => self.expire_session(),
            Err(error) => {
                tracing::warn!(%error, message = %done.id, action = ?done.action, "refused");
                self.answer(done.id, done.action, Err(ActionError::from(&error)));
            }
        }
    }

    /// Puts the user's reaction in the held message without waiting for the server's push, which
    /// then changes nothing. The message is taken as it is now, not as it was when the reaction was
    /// asked for. Nothing if it has gone meanwhile.
    fn apply_reaction(&mut self, room: &RoomId, id: &MessageId, emoji: &str, react: bool) {
        let Some(username) = self.own_user().map(|user| user.username.clone()) else {
            return;
        };
        let reacted = self
            .find_message(id)
            .filter(|held| &held.room_id == room)
            .and_then(|held| {
                let mut copy = held.clone();
                copy.set_reaction(emoji, &username, react).then_some(copy)
            });
        let Some(message) = reacted else {
            return;
        };
        // A page that is on its way may predate the reaction, so the copy joins it, as for an edit.
        self.hold_for_page(&message);
        self.replace_everywhere(room, &message);
    }

    fn abort_history_task(&mut self) {
        if let Some(task) = self.history_task.take() {
            task.abort();
        }
        if let Some(task) = self.changes_task.take() {
            task.abort();
        }
        if let Some(open) = &mut self.open {
            open.changes = None;
        }
    }

    fn emit_timeline_status(&self) {
        let status = match &self.open {
            Some(open) => TimelineStatus {
                room: Some(open.timeline.room().clone()),
                loading: open.fetch.as_ref().is_some_and(|f| !f.older),
                loading_older: open.fetch.as_ref().is_some_and(|f| f.older),
                reached_start: open.timeline.reached_start(),
                error: open.error.clone(),
            },
            None => TimelineStatus::default(),
        };
        (self.on_event)(Event::TimelineStatus(status));
    }

    /// Closes the open room and forgets every held message, and tells the frontend, unless no room
    /// was open.
    fn clear_timeline(&mut self) {
        self.messages.clear();
        self.close_room();
    }

    /// Closes the open room, keeping what is held of it, and tells the frontend, unless no room was
    /// open.
    fn close_room(&mut self) {
        self.close_thread();
        self.abort_history_task();
        if self.open.take().is_some() {
            self.watch_room(None);
            (self.on_event)(Event::Timeline(vec![TimelineChange::Reset {
                room: None,
                messages: Vec::new(),
            }]));
            self.emit_timeline_status();
        }
    }

    fn abort_rooms_task(&mut self) {
        if let Some(task) = self.rooms_task.take() {
            task.abort();
        }
    }

    /// Ends the realtime connection, if any, and a wait for the next one, and forgets what the
    /// connection pushed that wasn't applied yet.
    fn abort_realtime_task(&mut self) {
        self.backlog = None;
        self.realtime_open = false;
        self.realtime_watch = None;
        if let Some(task) = self.realtime_task.take() {
            task.abort();
        }
        if let Some(task) = self.reconnect_task.take() {
            task.abort();
        }
    }

    /// Empties the room list and tells the frontend, unless it was empty already.
    fn clear_rooms(&mut self) {
        self.rooms_listed = false;
        if self.rooms.rooms().is_empty() {
            return;
        }
        let change = self.rooms.reset(Vec::new());
        (self.on_event)(Event::Rooms(vec![change]));
    }

    fn clear_store(&self) {
        if let Some(store) = &self.store {
            store.clear();
        }
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
