// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The handle frontends drive the core through, and the session actor behind it.

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
use crate::backoff::Backoff;
use crate::domain::{
    Author, Credentials, Delivery, Message, MessageId, MessageKind, Room, RoomId, RoomKind,
    StoredSession, User, room_from,
};
use crate::error::Error;
use crate::ports::{
    ApiError, Connector, MessageStore, Page, Pushed, SecretStore, ServerApi, StoreError,
};
use crate::rooms::{RoomChange, RoomList};
use crate::session::{Established, LoginFailure, SessionState, log_in, resume};
use crate::timeline::{PAGE_SIZE, Timeline, TimelineChange, TimelineError, TimelineStatus};

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
    /// The state of the realtime connection of a logged-in session, when it changes. A session
    /// starts `Connected`, and so does the frontend until told otherwise: nothing is sent at login,
    /// and a logout or an expiry resets it without an event.
    Connection(Connection),
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
    SendMessage {
        room: RoomId,
        text: String,
    },
    RetryMessage(MessageId),
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
        on_event: impl Fn(Event) + Send + Sync + 'static,
    ) -> Result<Self, Error> {
        Self::with_ports(RestConnector, store, on_event)
    }

    /// A client that reaches servers through `connector` and remembers the session in `store`;
    /// tests pass fakes. If the store holds a session, the client tries to resume it.
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
        on_event: impl Fn(Event) + Send + Sync + 'static,
    ) -> Result<Self, Error> {
        Self::with_backoff(connector, store, on_event, Backoff::default())
    }

    /// [`with_ports`](Self::with_ports) with the waits between reconnects set, so that tests don't
    /// wait for seconds.
    fn with_backoff<C: Connector, S: SecretStore>(
        connector: C,
        store: S,
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
            cycle: 0,
            reconnect_task: None,
            backoff,
            connection: Connection::Connected,
            succeeded_at: None,
            rooms_listed: false,
            backlog: None,
            rooms: RoomList::default(),
            messages: Box::new(MemoryMessageStore::default()),
            open: None,
            history_attempt: 0,
            history_task: None,
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

    /// Sends a text message to the room. It shows in the room's messages at once, as pending (see
    /// [`Delivery`]), and becomes sent when the server confirms it, or failed if it can't. It
    /// goes out whichever room is open by then. Ignored unless logged in, if the room isn't in the
    /// room list, or if the text is blank.
    pub fn send_message(&self, room: RoomId, text: &str) {
        self.send(Command::SendMessage {
            room,
            text: text.to_owned(),
        });
    }

    /// Sends a message of the open room that failed again, under the same id, so that it can't
    /// appear twice if the first attempt got through after all. Ignored unless it is a failed
    /// message of the open room.
    pub fn retry_message(&self, id: MessageId) {
        self.send(Command::RetryMessage(id));
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

/// What the realtime task tells the actor, in this order: the connection opens or fails, events
/// follow, the connection ends.
enum RealtimeDone {
    Opened(Result<(), ApiError>),
    Pushed(Pushed),
    Ended,
}

/// The realtime task's report, tagged with the sync cycle it belongs to: stale if another has begun
/// since.
struct RealtimeReport {
    cycle: u64,
    done: RealtimeDone,
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

/// What the tasks tell the actor.
enum Done<A> {
    Login(LoginDone<A>),
    Rooms(RoomsDone),
    Realtime(RealtimeReport),
    /// The wait after a lost connection is over; the cycle is the one that was lost.
    Reconnect {
        cycle: u64,
    },
    History(HistoryDone),
    Send(SendDone),
    MarkRead(MarkReadDone),
}

/// A history fetch in flight.
struct Fetch {
    attempt: u64,
    /// Older messages, not the newest.
    older: bool,
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
}

/// What the actor keeps while logged in; the next features need both.
struct Session<A> {
    api: A,
    credentials: Credentials,
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
    /// The messages of the rooms opened so far. Cleared on logout.
    messages: Box<dyn MessageStore>,
    open: Option<OpenRoom>,
    /// Counts history fetches, to tell the answer of the current one from a stale one.
    history_attempt: u64,
    /// The history fetch of the open room, while it runs.
    history_task: Option<JoinHandle<()>>,
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
                    Done::Realtime(report) => self.handle_realtime_done(report),
                    Done::Reconnect { cycle } => self.handle_reconnect(cycle),
                    Done::History(done) => self.handle_history_done(done),
                    Done::Send(done) => self.handle_send_done(done),
                    Done::MarkRead(done) => self.handle_mark_read_done(done),
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
        self.abort_realtime_task();
        if let Some(task) = self.history_task.take() {
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
            Command::SendMessage { room, text } => self.send_message(room, &text),
            Command::RetryMessage(id) => self.retry_message(&id),
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
        self.succeeded_at = None;
        self.backlog = Some(Vec::new());
        let api = session.api.clone();
        let credentials = session.credentials.clone();
        let done = self.done_tx.clone();
        self.realtime_task = Some(tokio::spawn(async move {
            let report = |done| Done::Realtime(RealtimeReport { cycle, done });
            let mut events = match api.realtime(&credentials).await {
                Ok(events) => events,
                Err(error) => {
                    // Fails only if the actor is gone.
                    let _ = done.send(report(RealtimeDone::Opened(Err(error))));
                    return;
                }
            };
            if done.send(report(RealtimeDone::Opened(Ok(())))).is_err() {
                return;
            }
            while let Some(event) = events.recv().await {
                if done.send(report(RealtimeDone::Pushed(event))).is_err() {
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
            RealtimeDone::Opened(Ok(())) => {
                self.realtime_open = true;
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
                Some(backlog) => backlog.push(event),
                None => self.handle_pushed(event),
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
                let known_activity = self.rooms.get(&id).map(|room| room.last_activity);
                match room_from(subscription) {
                    Some(mut room) => {
                        room.last_activity = known_activity.unwrap_or(room.last_activity);
                        let room = self.seen_if_open(room);
                        if let Some(change) = self.rooms.upsert(room) {
                            (self.on_event)(Event::Rooms(vec![change]));
                        }
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

    /// Adds a pushed message to the open room. Other rooms aren't held: they load their history
    /// when opened, and their unread counts arrive as subscription changes.
    fn add_live_message(&mut self, message: Message) {
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
        if self.realtime_open {
            self.succeeded_at = Some(Instant::now());
            self.set_connection(Connection::Connected);
            self.catch_up_open_room();
        }
        for event in self.backlog.take().unwrap_or_default() {
            self.handle_pushed(event);
        }
    }

    /// Fetches the newest messages of the open room again, after the connection was lost: what was
    /// said meanwhile was not pushed. The page joins what is held (see
    /// [`Timeline::merge_latest`]), and the messages pushed from now on wait for it. A room that
    /// has left the list is closed instead.
    fn catch_up_open_room(&mut self) {
        let Some(open) = &self.open else {
            return;
        };
        let id = open.timeline.room().clone();
        if self.rooms.rooms().iter().any(|room| room.id == id) {
            self.abort_history_task();
            self.fetch_history(false);
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
        self.abort_history_task();
        let history = self.messages.load(&id);
        let timeline = Timeline::new(id, history);
        (self.on_event)(Event::Timeline(vec![timeline.reset()]));
        self.open = Some(OpenRoom {
            timeline,
            kind,
            fetch: None,
            error: None,
            live: Vec::new(),
        });
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
        open.fetch = Some(Fetch { attempt, older });
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
        self.emit_timeline_status();
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
        match done.outcome {
            Ok(page) => {
                let changes = if older {
                    open.timeline.merge_older(page.messages, page.reached_start)
                } else {
                    let mut messages = page.messages;
                    messages.append(&mut open.live);
                    open.timeline.merge_latest(messages, page.reached_start)
                };
                self.messages.save(&done.room, open.timeline.history());
                if !changes.is_empty() {
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
        self.emit_timeline_status();
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
                (self.on_event)(Event::Timeline(changes));
            }
            return;
        }
        let mut timeline = Timeline::new(room.clone(), self.messages.load(room));
        if !change(&mut timeline).is_empty() {
            self.messages.save(room, timeline.history());
        }
    }

    fn send_message(&mut self, room: RoomId, text: &str) {
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
        };
        let id = message.id.clone();
        self.change_timeline(&room, |timeline| vec![timeline.add_unsent(message)]);
        self.post(room, id, text.to_owned());
    }

    fn retry_message(&mut self, id: &MessageId) {
        let Some(open) = &self.open else {
            tracing::debug!("retry_message ignored: no room is open");
            return;
        };
        let room = open.timeline.room().clone();
        let Some(Message {
            kind: MessageKind::Text(text),
            delivery: Delivery::Failed,
            ..
        }) = open.timeline.message(id)
        else {
            tracing::debug!(message = %id, "retry_message ignored: not a failed message");
            return;
        };
        let text = text.text.clone();
        self.change_timeline(&room, |timeline| {
            timeline
                .set_delivery(id, Delivery::Pending)
                .into_iter()
                .collect()
        });
        self.post(room, id.clone(), text);
    }

    /// Sends the message in a task, which reports the outcome.
    fn post(&mut self, room: RoomId, id: MessageId, text: String) {
        let Some(session) = &self.session else {
            return;
        };
        let api = session.api.clone();
        let credentials = session.credentials.clone();
        let attempt = self.attempt;
        let done = self.done_tx.clone();
        tokio::spawn(async move {
            let outcome = api.send_message(&credentials, &room, &id, &text).await;
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
                if let Some(open) = self.open.as_mut().filter(|open| {
                    open.timeline.room() == &done.room
                        && open.fetch.as_ref().is_some_and(|fetch| !fetch.older)
                }) {
                    open.live.push(message.clone());
                }
                self.change_timeline(&done.room, |timeline| {
                    timeline.settle(message).into_iter().collect()
                });
            }
            Err(ApiError::Unauthorized) => self.expire_session(),
            Err(error) => {
                tracing::warn!(%error, room = %done.room, "can't send the message");
                self.change_timeline(&done.room, |timeline| {
                    timeline
                        .set_delivery(&done.id, Delivery::Failed)
                        .into_iter()
                        .collect()
                });
            }
        }
    }

    fn abort_history_task(&mut self) {
        if let Some(task) = self.history_task.take() {
            task.abort();
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
        self.abort_history_task();
        if self.open.take().is_some() {
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
