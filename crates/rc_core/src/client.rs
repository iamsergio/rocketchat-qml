// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The handle frontends drive the core through, and the session actor behind it.

use std::sync::Arc;

use secrecy::SecretString;
use tokio::runtime::{Builder, Runtime};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::task::JoinHandle;

use crate::adapters::RestConnector;
use crate::domain::Credentials;
use crate::error::Error;
use crate::ports::{Connector, ServerApi};
use crate::session::{Established, LoginFailure, SessionState, log_in};

#[cfg(test)]
mod tests;

/// Worker threads of the async runtime: networking only, the GUI has its own thread.
const WORKER_THREADS: usize = 2;

/// What the core tells the frontend.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Event {
    Session(SessionState),
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
pub struct Client {
    // Declared first so that it's dropped first: the actor ends when its commands stop.
    commands: UnboundedSender<Command>,
    _runtime: Runtime,
}

impl Client {
    /// A client for real servers. See [`with_connector`](Self::with_connector) for `on_event`.
    ///
    /// # Errors
    ///
    /// If the async runtime can't be started.
    pub fn start(on_event: impl Fn(Event) + Send + Sync + 'static) -> Result<Self, Error> {
        Self::with_connector(RestConnector, on_event)
    }

    /// A client that reaches servers through `connector`; tests pass a fake.
    ///
    /// `on_event` is called once per state change, in order, from a runtime thread. The initial
    /// state is `LoggedOut { failure: None }` and has no event. It must return quickly and must
    /// not touch GUI objects: a Qt frontend hops to its GUI thread inside it, e.g. with
    /// `CxxQtThread::queue`.
    ///
    /// # Errors
    ///
    /// If the async runtime can't be started.
    pub fn with_connector<C: Connector>(
        connector: C,
        on_event: impl Fn(Event) + Send + Sync + 'static,
    ) -> Result<Self, Error> {
        let runtime = Builder::new_multi_thread()
            .worker_threads(WORKER_THREADS)
            .enable_all()
            .build()
            .map_err(Error::Runtime)?;
        let (commands, commands_rx) = mpsc::unbounded_channel();
        let (done_tx, done_rx) = mpsc::unbounded_channel();
        let actor = Actor {
            connector: Arc::new(connector),
            state: SessionState::LoggedOut { failure: None },
            session: None,
            attempt: 0,
            login_task: None,
            done_tx,
            on_event: Arc::new(on_event),
        };
        runtime.spawn(actor.run(commands_rx, done_rx));
        Ok(Self {
            commands,
            _runtime: runtime,
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

    /// Cancels a login in progress, or logs out. Ignored when logged out.
    pub fn logout(&self) {
        self.send(Command::Logout);
    }

    fn send(&self, command: Command) {
        if self.commands.send(command).is_err() {
            tracing::error!("the session actor is gone, command dropped");
        }
    }
}

// Dropping the client stops the actor and shuts the runtime down, which waits for its threads. Don't
// drop it from async code; the GUI thread owns it.

/// A login task's answer.
struct LoginDone<A> {
    attempt: u64,
    outcome: Result<Established<A>, LoginFailure>,
}

/// What the actor keeps while logged in; the next features need both.
struct Session<A> {
    api: A,
    credentials: Credentials,
}

/// Owns the session state and handles one message at a time. Network calls run in spawned tasks
/// that report back through `done_tx`.
struct Actor<C: Connector> {
    connector: Arc<C>,
    state: SessionState,
    session: Option<Session<C::Api>>,
    /// Identifies the current login attempt, so that the answer of a cancelled one is dropped.
    attempt: u64,
    login_task: Option<JoinHandle<()>>,
    done_tx: UnboundedSender<LoginDone<C::Api>>,
    on_event: Arc<dyn Fn(Event) + Send + Sync>,
}

impl<C: Connector> Actor<C> {
    async fn run(
        mut self,
        mut commands: UnboundedReceiver<Command>,
        mut done: UnboundedReceiver<LoginDone<C::Api>>,
    ) {
        loop {
            tokio::select! {
                biased;
                command = commands.recv() => match command {
                    Some(command) => self.handle_command(command),
                    None => break,
                },
                Some(done) = done.recv() => self.handle_login_done(done),
            }
        }
        if let Some(task) = self.login_task.take() {
            task.abort();
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
        self.attempt += 1;
        let attempt = self.attempt;
        self.set_state(SessionState::Connecting);
        let connector = Arc::clone(&self.connector);
        let done = self.done_tx.clone();
        // The password lives and dies in this task.
        self.login_task = Some(tokio::spawn(async move {
            let outcome = log_in(&*connector, &server, &username, &password).await;
            // Fails only if the actor is gone.
            let _ = done.send(LoginDone { attempt, outcome });
        }));
    }

    fn handle_login_done(&mut self, done: LoginDone<C::Api>) {
        if done.attempt != self.attempt || self.state != SessionState::Connecting {
            tracing::debug!("answer of a cancelled login dropped");
            return;
        }
        self.login_task = None;
        match done.outcome {
            Ok(established) => {
                self.session = Some(Session {
                    api: established.api,
                    credentials: established.credentials,
                });
                self.set_state(SessionState::LoggedIn(established.account));
            }
            Err(failure) => self.set_state(SessionState::LoggedOut {
                failure: Some(failure),
            }),
        }
    }

    fn logout(&mut self) {
        match self.state {
            SessionState::Connecting => {
                if let Some(task) = self.login_task.take() {
                    task.abort();
                }
                // Whatever the aborted task still sends is stale.
                self.attempt += 1;
                self.set_state(SessionState::LoggedOut { failure: None });
            }
            SessionState::LoggedIn(_) => {
                self.set_state(SessionState::LoggedOut { failure: None });
                if let Some(session) = self.session.take() {
                    // Logged out locally either way; revoking the token is best effort.
                    tokio::spawn(async move {
                        if let Err(error) = session.api.logout(&session.credentials).await {
                            tracing::warn!(%error, "couldn't revoke the session on the server");
                        }
                    });
                }
            }
            SessionState::LoggedOut { .. } => tracing::debug!("logout ignored: not logged in"),
        }
    }

    fn set_state(&mut self, state: SessionState) {
        match &state {
            SessionState::LoggedOut { failure } => {
                tracing::info!(?failure, "session: logged out");
            }
            SessionState::Connecting => tracing::info!("session: connecting"),
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
