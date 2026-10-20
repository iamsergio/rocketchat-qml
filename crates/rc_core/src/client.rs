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

use crate::adapters::RestConnector;
use crate::domain::{Credentials, StoredSession, room_from};
use crate::error::Error;
use crate::ports::{ApiError, Connector, SecretStore, ServerApi, StoreError};
use crate::rooms::{RoomChange, RoomList};
use crate::session::{Established, LoginFailure, SessionState, log_in, resume};

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
    /// `LoggedOut` it empties the list, unless the list was empty already.
    Rooms(Vec<RoomChange>),
}

enum Command {
    Login {
        server: String,
        username: String,
        password: SecretString,
    },
    Logout,
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
            rooms: RoomList::default(),
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
    /// The login or resume the fetch belongs to: the answer is stale if another has begun since.
    attempt: u64,
    outcome: Result<Vec<rc_api::Subscription>, ApiError>,
}

/// What the tasks tell the actor.
enum Done<A> {
    Login(LoginDone<A>),
    Rooms(RoomsDone),
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
    /// The rooms in display order, as the frontend has them.
    rooms: RoomList,
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
                self.set_state(SessionState::LoggedIn(established.account));
                self.fetch_rooms(done.attempt);
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
            self.set_state(SessionState::LoggedOut { failure: None });
            self.clear_rooms();
            self.clear_store();
            if let Some(session) = self.session.take() {
                self.revoke(session.api, session.credentials);
            }
        } else {
            tracing::debug!("logout ignored: not logged in");
        }
    }

    /// Fetches the rooms of the session that the attempt `attempt` established. Only the first
    /// fetch is made: a failure other than an expired token leaves the session without rooms until
    /// the sync engine (Phase 3) retries.
    fn fetch_rooms(&mut self, attempt: u64) {
        let Some(session) = &self.session else {
            return;
        };
        let api = session.api.clone();
        let credentials = session.credentials.clone();
        let done = self.done_tx.clone();
        self.rooms_task = Some(tokio::spawn(async move {
            let outcome = api.subscriptions(&credentials).await;
            // Fails only if the actor is gone.
            let _ = done.send(Done::Rooms(RoomsDone { attempt, outcome }));
        }));
    }

    fn handle_rooms_done(&mut self, done: RoomsDone) {
        if done.attempt != self.attempt || !matches!(self.state, SessionState::LoggedIn(_)) {
            tracing::debug!("answer of a stale room fetch dropped");
            return;
        }
        self.rooms_task = None;
        match done.outcome {
            Ok(subscriptions) => {
                let rooms = subscriptions.into_iter().filter_map(room_from).collect();
                let change = self.rooms.reset(rooms);
                (self.on_event)(Event::Rooms(vec![change]));
            }
            // The token died between the login and the fetch: same as an expired resume. It's
            // dead already, so there's nothing to revoke.
            Err(ApiError::Unauthorized) => {
                self.session = None;
                self.clear_store();
                self.set_state(SessionState::LoggedOut {
                    failure: Some(LoginFailure::SessionExpired),
                });
                self.clear_rooms();
            }
            Err(error) => tracing::warn!(%error, "can't list the rooms"),
        }
    }

    fn abort_rooms_task(&mut self) {
        if let Some(task) = self.rooms_task.take() {
            task.abort();
        }
    }

    /// Empties the room list and tells the frontend, unless it was empty already.
    fn clear_rooms(&mut self) {
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
        self.state = state.clone();
        (self.on_event)(Event::Session(state));
    }
}
