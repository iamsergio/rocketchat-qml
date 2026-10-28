// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The task behind [`GitHub`](super::GitHub): it owns the connection, the lists and the clock for
//! the polling, and does the I/O in tasks of their own that report back through an inbox, so that
//! it never waits on the network. Commands are handled before the inbox, the inbox before the
//! timer.

use std::collections::{BTreeSet, HashMap};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use jiff::Timestamp;
use secrecy::{ExposeSecret, SecretString};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use super::domain::{CiState, GithubError, PrKey, PullRequest, RepoRunners, Runner};
use super::list::PrList;
use super::ports::{
    GhCli, GhError, GithubApi, GithubAuth, GithubConnector, GithubPorts, GithubStore,
};
use super::{
    AttemptId, Command, CommandState, ConnectFailure, Connection, DEFAULT_RATE_LIMIT_WAIT,
    GithubConfig, GithubEvent, POLL_BACKGROUND, POLL_ON_SCREEN, PrAction, REFRESH_AFTER_COMMAND,
    REPOS_MAX_AGE, RefreshState,
};
use crate::ports::Clock;

type OnEvent = Arc<dyn Fn(GithubEvent) + Send + Sync>;

/// A rate limit never holds the polling back for less than this, so that a reset time that has
/// passed already doesn't make a loop of requests.
const MIN_RATE_LIMIT_WAIT: Duration = Duration::from_secs(5);

/// What the tasks tell the actor. `session` says which connection they worked for: the answer of
/// an older one is dropped.
enum Inbox<A> {
    Connected {
        session: u64,
        result: Result<Option<Linked<A>>, ConnectFailure>,
    },
    Fetched {
        session: u64,
        fetched: Fetched,
    },
    Command {
        session: u64,
        pr: PrKey,
        action: PrAction,
        attempt: AttemptId,
        result: Result<(), GithubError>,
    },
}

/// A connection that works.
struct Linked<A> {
    api: A,
    login: String,
    auth: GithubAuth,
    /// The choice is new: keep it.
    save: bool,
}

/// What one refresh brought. A list that failed is an error, and stays as it was.
struct Fetched {
    mine: Result<Vec<PullRequest>, GithubError>,
    /// The watched repositories, when this refresh fetched them.
    repos: Option<Vec<String>>,
    watched: Result<Vec<PullRequest>, GithubError>,
    runners: Vec<(String, Result<Vec<Runner>, GithubError>)>,
}

/// What to connect with.
enum Connect {
    /// The stored choice, if any.
    Stored,
    Auth {
        auth: GithubAuth,
        save: bool,
    },
}

struct Actor<C: GithubConnector, G, S, K> {
    connector: Arc<C>,
    gh: Arc<G>,
    store: Arc<S>,
    store_tx: UnboundedSender<StoreOp>,
    clock: Arc<K>,
    config: GithubConfig,
    on_event: OnEvent,
    inbox_tx: UnboundedSender<Inbox<C::Api>>,
    /// Counts the connections, so that a late answer for an older one is recognised.
    session: u64,
    connection: Connection,
    /// The choice that is stored, for [`retry`](super::GitHub::retry).
    stored: Option<GithubAuth>,
    api: Option<C::Api>,
    on_screen: bool,
    mine: PrList,
    watched: PrList,
    failing: usize,
    runners: Vec<RepoRunners>,
    /// The watched repositories and when they were fetched.
    repos: Option<(Vec<String>, Timestamp)>,
    /// The next refresh fetches the repositories whatever their age.
    force_repos: bool,
    refreshing: bool,
    last_started: Option<Timestamp>,
    last_good: Option<Timestamp>,
    /// A refresh that is due before the regular one: after connecting, a command, or a request.
    soon: Option<Timestamp>,
    /// No request until then: GitHub's rate limit.
    blocked_until: Option<Timestamp>,
    busy: HashMap<PrKey, AttemptId>,
    next_attempt: u64,
    notify: BTreeSet<PrKey>,
}

pub(super) async fn run<C, G, S, K>(
    ports: GithubPorts<C, G, S, K>,
    config: GithubConfig,
    on_event: OnEvent,
    mut commands: UnboundedReceiver<Command>,
) where
    C: GithubConnector,
    G: GhCli,
    S: GithubStore,
    K: Clock,
{
    let (inbox_tx, mut inbox) = unbounded_channel();
    let store = Arc::new(ports.store);
    let (store_tx, store_ops) = unbounded_channel();
    tokio::spawn(store_worker(Arc::clone(&store), store_ops));
    let mut actor = Actor {
        connector: Arc::new(ports.connector),
        gh: Arc::new(ports.gh),
        store,
        store_tx,
        clock: Arc::new(ports.clock),
        config,
        on_event,
        inbox_tx,
        session: 0,
        connection: Connection::NotConnected,
        stored: None,
        api: None,
        on_screen: false,
        mine: PrList::default(),
        watched: PrList::default(),
        failing: 0,
        runners: Vec::new(),
        repos: None,
        force_repos: false,
        refreshing: false,
        last_started: None,
        last_good: None,
        soon: None,
        blocked_until: None,
        busy: HashMap::new(),
        next_attempt: 1,
        notify: BTreeSet::new(),
    };
    actor.start().await;
    loop {
        let wait: Pin<Box<dyn Future<Output = ()> + Send>> = match actor.due() {
            Some(at) => actor.clock.sleep_until(at),
            None => Box::pin(std::future::pending()),
        };
        tokio::select! {
            biased;
            command = commands.recv() => match command {
                Some(command) => actor.handle(command),
                None => break,
            },
            // The actor holds a sender, so the inbox never ends.
            Some(message) = inbox.recv() => actor.apply(message),
            () = wait => actor.start_refresh(),
        }
    }
}

enum StoreOp {
    Save(GithubAuth),
    Clear,
}

/// Runs the store's updates one at a time, in the order they were queued, off the async threads:
/// they block, and a keyring can wait for the user to unlock it.
async fn store_worker<S: GithubStore>(store: Arc<S>, mut ops: UnboundedReceiver<StoreOp>) {
    while let Some(op) = ops.recv().await {
        let store = Arc::clone(&store);
        let done = blocking(move || match op {
            StoreOp::Save(auth) => store.save(&auth),
            StoreOp::Clear => store.clear(),
        })
        .await;
        match done {
            Ok(Ok(())) => {}
            Ok(Err(error)) => tracing::warn!(%error, "can't update the stored GitHub login"),
            Err(error) => tracing::error!(error, "updating the stored GitHub login failed"),
        }
    }
}

/// Plain `tokio::task::spawn_blocking`, with a failed task reported as the error.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, String> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|error| error.to_string())
}

fn after(time: Timestamp, wait: Duration) -> Timestamp {
    time.checked_add(wait).unwrap_or(time)
}

impl<C: GithubConnector, G: GhCli, S: GithubStore, K: Clock> Actor<C, G, S, K> {
    fn emit(&self, event: GithubEvent) {
        (self.on_event)(event);
    }

    fn now(&self) -> Timestamp {
        self.clock.now()
    }

    /// Reads the stored choice and, if there is one, connects with it.
    async fn start(&mut self) {
        let store = Arc::clone(&self.store);
        let stored = match blocking(move || store.load()).await {
            Ok(Ok(stored)) => stored,
            Ok(Err(error)) => {
                tracing::warn!(%error, "can't read the stored GitHub login");
                None
            }
            Err(error) => {
                tracing::error!(error, "reading the stored GitHub login failed");
                None
            }
        };
        match stored {
            Some(auth) => {
                self.stored = Some(auth.clone());
                self.begin_connect(Connect::Auth { auth, save: false });
            }
            None => self.emit(GithubEvent::Connection(Connection::NotConnected)),
        }
    }

    fn handle(&mut self, command: Command) {
        match command {
            Command::ConnectGh => self.begin_connect(Connect::Auth {
                auth: GithubAuth::GhCli,
                save: true,
            }),
            Command::ConnectToken(token) => self.begin_connect(Connect::Auth {
                // Pasting brings a newline along.
                auth: GithubAuth::Token(SecretString::from(token.expose_secret().trim())),
                save: true,
            }),
            Command::Disconnect => self.disconnect(),
            Command::Retry => {
                if matches!(self.connection, Connection::Failed(_)) && self.stored.is_some() {
                    self.begin_connect(Connect::Stored);
                }
            }
            Command::SetOnScreen(on_screen) => self.on_screen = on_screen,
            Command::Refresh => {
                if self.api.is_some() {
                    self.force_repos = true;
                    self.soon = Some(self.now());
                }
            }
            Command::Run { pr, action } => self.run_command(pr, action),
            Command::NotifyWhenDone { pr, on } => self.set_notify(pr, on),
            #[cfg(test)]
            Command::Barrier(done) => {
                let _ = done.send(());
            }
        }
    }

    fn apply(&mut self, message: Inbox<C::Api>) {
        match message {
            Inbox::Connected { session, result } if session == self.session => {
                self.connected(result);
            }
            Inbox::Fetched { session, fetched } if session == self.session => {
                self.fetched(fetched);
            }
            Inbox::Command {
                session,
                pr,
                action,
                attempt,
                result,
            } if session == self.session => self.command_ended(pr, action, attempt, &result),
            _ => tracing::debug!("an answer for an older GitHub connection dropped"),
        }
    }

    // --- connecting ---

    /// Drops what the connection held and connects anew; the answer is [`Inbox::Connected`].
    fn begin_connect(&mut self, connect: Connect) {
        self.drop_session();
        self.set_connection(Connection::Connecting);
        let session = self.session;
        let inbox = self.inbox_tx.clone();
        let connector = Arc::clone(&self.connector);
        let gh = Arc::clone(&self.gh);
        let store = Arc::clone(&self.store);
        tokio::spawn(async move {
            let result = connect_with(&*connector, &gh, &store, connect).await;
            // Fails only if the service ended.
            let _ = inbox.send(Inbox::Connected { session, result });
        });
    }

    fn connected(&mut self, result: Result<Option<Linked<C::Api>>, ConnectFailure>) {
        match result {
            Ok(Some(Linked {
                api,
                login,
                auth,
                save,
            })) => {
                if save {
                    self.store_op(StoreOp::Save(auth.clone()));
                }
                self.api = Some(api);
                self.stored = Some(auth);
                self.soon = Some(self.now());
                self.set_connection(Connection::Connected { login });
            }
            Ok(None) => self.set_connection(Connection::NotConnected),
            Err(failure) => self.set_connection(Connection::Failed(failure)),
        }
    }

    fn store_op(&self, op: StoreOp) {
        if self.store_tx.send(op).is_err() {
            tracing::warn!("the GitHub store worker is gone, the login wasn't updated");
        }
    }

    fn set_connection(&mut self, connection: Connection) {
        self.connection = connection.clone();
        self.emit(GithubEvent::Connection(connection));
    }

    fn disconnect(&mut self) {
        self.drop_session();
        self.stored = None;
        self.store_op(StoreOp::Clear);
        self.set_connection(Connection::NotConnected);
    }

    /// GitHub refused the token we hold: nothing works until the user connects again. The stored
    /// choice stays, for [`retry`](super::GitHub::retry) and for the page to say what failed.
    fn token_refused(&mut self) {
        self.drop_session();
        self.set_connection(Connection::Failed(ConnectFailure::BadToken));
    }

    /// Ends the connection and everything that came from it. The lists, the count, the runners
    /// and the notify set are emptied with events, if they held anything.
    fn drop_session(&mut self) {
        self.session += 1;
        self.api = None;
        self.refreshing = false;
        self.last_started = None;
        self.last_good = None;
        self.soon = None;
        self.blocked_until = None;
        self.repos = None;
        self.force_repos = false;
        self.busy.clear();
        if let Some(change) = self.mine.clear() {
            self.emit(GithubEvent::Mine(vec![change]));
        }
        if let Some(change) = self.watched.clear() {
            self.emit(GithubEvent::Watched(vec![change]));
        }
        self.update_failing();
        if !self.runners.is_empty() {
            self.runners.clear();
            self.emit(GithubEvent::Runners(Vec::new()));
        }
        if !self.notify.is_empty() {
            self.notify.clear();
            self.emit(GithubEvent::Notifying(Vec::new()));
        }
    }

    // --- polling ---

    /// When the next refresh is due: never while disconnected or while one runs.
    fn due(&self) -> Option<Timestamp> {
        if self.api.is_none() || self.refreshing {
            return None;
        }
        let every = if self.on_screen {
            POLL_ON_SCREEN
        } else {
            POLL_BACKGROUND
        };
        let regular = self.last_started.map(|started| after(started, every));
        let due = [regular, self.soon].into_iter().flatten().min()?;
        Some(self.blocked_until.map_or(due, |until| due.max(until)))
    }

    fn start_refresh(&mut self) {
        let Some(api) = self.api.clone() else {
            return;
        };
        let now = self.now();
        self.refreshing = true;
        self.last_started = Some(now);
        self.soon = None;
        self.emit(GithubEvent::Refresh(RefreshState::Started));

        let known_repos = self
            .repos
            .as_ref()
            .filter(|(_, at)| {
                !self.force_repos && now.duration_since(*at).unsigned_abs() < REPOS_MAX_AGE
            })
            .map(|(repos, _)| repos.clone());
        self.force_repos = false;
        let since = since(now, self.config.watched_days);
        let runner_repos = self.config.runner_repos.clone();
        let session = self.session;
        let inbox = self.inbox_tx.clone();
        tokio::spawn(async move {
            let mine = api.my_pull_requests();
            let watched = async {
                let (repos, fresh) = match known_repos {
                    Some(repos) => (Ok(repos), None),
                    None => match api.watched_repos().await {
                        Ok(repos) => (Ok(repos.clone()), Some(repos)),
                        Err(error) => (Err(error), None),
                    },
                };
                let prs = match repos {
                    Ok(repos) => api.watched_pull_requests(&repos, since).await,
                    Err(error) => Err(error),
                };
                (fresh, prs)
            };
            let runners = async {
                let mut all = Vec::new();
                for repo in runner_repos {
                    let result = api.runners(&repo).await;
                    all.push((repo, result));
                }
                all
            };
            let (mine, (repos, watched), runners) = tokio::join!(mine, watched, runners);
            let fetched = Fetched {
                mine,
                repos,
                watched,
                runners,
            };
            let _ = inbox.send(Inbox::Fetched { session, fetched });
        });
    }

    fn fetched(&mut self, fetched: Fetched) {
        self.refreshing = false;
        let now = self.now();
        if let Some(repos) = fetched.repos {
            self.repos = Some((repos, now));
        }
        let mut errors = Vec::new();
        match fetched.mine {
            Ok(prs) => {
                let changes = self.mine.sync(prs);
                if !changes.is_empty() {
                    self.emit(GithubEvent::Mine(changes));
                }
            }
            Err(error) => errors.push(error),
        }
        match fetched.watched {
            Ok(prs) => {
                let changes = self.watched.sync(prs);
                if !changes.is_empty() {
                    self.emit(GithubEvent::Watched(changes));
                }
            }
            Err(error) => errors.push(error),
        }
        self.update_runners(fetched.runners);
        self.update_failing();
        self.check_notifications();

        // A refused token and a rate limit matter more than the rest.
        let error = errors
            .iter()
            .find(|e| matches!(e, GithubError::Unauthorized))
            .or_else(|| {
                errors
                    .iter()
                    .find(|e| matches!(e, GithubError::RateLimited { .. }))
            })
            .or(errors.first())
            .cloned();
        let Some(error) = error else {
            self.last_good = Some(now);
            self.emit(GithubEvent::Refresh(RefreshState::Done { at: now }));
            return;
        };
        // The state is in place before the event says so.
        self.note_rate_limit(&error);
        self.emit(GithubEvent::Refresh(RefreshState::Failed {
            error: error.clone(),
            last_good: self.last_good,
        }));
        if error == GithubError::Unauthorized {
            self.token_refused();
        }
    }

    /// A rate limit holds the requests back until its reset, or its `retry-after`, or a minute
    /// if GitHub says neither.
    fn note_rate_limit(&mut self, error: &GithubError) {
        let GithubError::RateLimited { reset, retry_after } = error else {
            return;
        };
        let now = self.now();
        let until = [*reset, retry_after.map(|wait| after(now, wait))]
            .into_iter()
            .flatten()
            .max()
            .unwrap_or_else(|| after(now, DEFAULT_RATE_LIMIT_WAIT));
        let until = until.max(after(now, MIN_RATE_LIMIT_WAIT));
        self.blocked_until = Some(self.blocked_until.map_or(until, |old| old.max(until)));
    }

    fn update_runners(&mut self, results: Vec<(String, Result<Vec<Runner>, GithubError>)>) {
        if results.is_empty() {
            return;
        }
        let mut runners = Vec::new();
        for (repo, result) in results {
            match result {
                Ok(list) => runners.push(RepoRunners {
                    repo,
                    runners: list,
                }),
                Err(error) => {
                    // A repository we can't read (no admin rights) has none; a failed fetch keeps
                    // what was there.
                    tracing::debug!(%error, repo, "can't list the runners");
                    runners.extend(self.runners.iter().find(|old| old.repo == repo).cloned());
                }
            }
        }
        if runners != self.runners {
            self.runners.clone_from(&runners);
            self.emit(GithubEvent::Runners(runners));
        }
    }

    fn update_failing(&mut self) {
        let failing = self
            .mine
            .items()
            .iter()
            .filter(|pr| !pr.is_draft && pr.ci == CiState::Failure)
            .count();
        if failing != self.failing {
            self.failing = failing;
            self.emit(GithubEvent::Failing(failing));
        }
    }

    // --- commands ---

    fn run_command(&mut self, pr: PrKey, action: PrAction) {
        let Some(api) = self.api.clone() else {
            tracing::debug!(%pr, "a command while not connected dropped");
            return;
        };
        if let Some(&running) = self.busy.get(&pr) {
            self.emit(GithubEvent::Command {
                pr,
                action,
                attempt: running,
                state: CommandState::Refused,
            });
            return;
        }
        let attempt = AttemptId(self.next_attempt);
        self.next_attempt += 1;
        self.busy.insert(pr.clone(), attempt);
        self.emit(GithubEvent::Command {
            pr: pr.clone(),
            action,
            attempt,
            state: CommandState::Started,
        });
        let session = self.session;
        let inbox = self.inbox_tx.clone();
        tokio::spawn(async move {
            let result = match action {
                PrAction::Merge { force } => api.merge(&pr, force).await,
                PrAction::UpdateBranch => api.update_branch(&pr).await,
                PrAction::RerunFailed => api.rerun_failed(&pr, false).await,
                PrAction::RerunJob { job_id } => api.rerun_job(&pr.repo, job_id).await,
                PrAction::Approve => api.approve_runs(&pr).await,
            };
            let _ = inbox.send(Inbox::Command {
                session,
                pr,
                action,
                attempt,
                result,
            });
        });
    }

    fn command_ended(
        &mut self,
        pr: PrKey,
        action: PrAction,
        attempt: AttemptId,
        result: &Result<(), GithubError>,
    ) {
        // Only the attempt that is running reports: the connection can have changed under it.
        if self.busy.get(&pr) != Some(&attempt) {
            return;
        }
        self.busy.remove(&pr);
        let state = match result {
            Ok(()) => CommandState::Done,
            Err(error) => CommandState::Failed(error.clone()),
        };
        let refused = matches!(result, Err(GithubError::Unauthorized));
        if let Err(error) = result {
            self.note_rate_limit(error);
        }
        if !refused {
            // GitHub's state lags behind: a rerun does not show as running at once.
            let soon = after(self.now(), REFRESH_AFTER_COMMAND);
            self.soon = Some(self.soon.map_or(soon, |old| old.min(soon)));
        }
        self.emit(GithubEvent::Command {
            pr,
            action,
            attempt,
            state,
        });
        if refused {
            self.token_refused();
        }
    }

    // --- notify when done ---

    fn set_notify(&mut self, pr: PrKey, on: bool) {
        let changed = if on {
            let running = self
                .mine
                .get(&pr)
                .or_else(|| self.watched.get(&pr))
                .is_some_and(|known| known.ci == CiState::Pending);
            if !running {
                tracing::debug!(%pr, "notify when done ignored: no such pull request, or no CI running");
            }
            running && self.notify.insert(pr)
        } else {
            self.notify.remove(&pr)
        };
        if changed {
            self.emit_notifying();
        }
    }

    /// Drops the pull requests of the notify set that went away, and tells of those whose CI
    /// ended.
    fn check_notifications(&mut self) {
        let mut ended = Vec::new();
        let mut changed = false;
        for key in self.notify.clone() {
            let state = self
                .mine
                .get(&key)
                .or_else(|| self.watched.get(&key))
                .map(|pr| pr.ci);
            match state {
                Some(state @ (CiState::Success | CiState::Failure)) => {
                    ended.push((key.clone(), state));
                }
                Some(CiState::Pending | CiState::None) => continue,
                // Merged or closed: nothing left to wait for.
                None => {}
            }
            self.notify.remove(&key);
            changed = true;
        }
        for (pr, state) in ended {
            self.emit(GithubEvent::CiEnded { pr, state });
        }
        if changed {
            self.emit_notifying();
        }
    }

    fn emit_notifying(&self) {
        self.emit(GithubEvent::Notifying(
            self.notify.iter().cloned().collect(),
        ));
    }
}

/// The first day of the watched window: `days` before today, in UTC.
fn since(now: Timestamp, days: u32) -> jiff::civil::Date {
    let today = now.to_zoned(jiff::tz::TimeZone::UTC).date();
    today
        .checked_sub(jiff::Span::new().days(i64::from(days)))
        .unwrap_or(today)
}

/// Finds the token, opens the API and asks who the user is.
async fn connect_with<C: GithubConnector, G: GhCli, S: GithubStore>(
    connector: &C,
    gh: &Arc<G>,
    store: &Arc<S>,
    connect: Connect,
) -> Result<Option<Linked<C::Api>>, ConnectFailure> {
    let (auth, save) = match connect {
        Connect::Auth { auth, save } => (auth, save),
        Connect::Stored => {
            let store = Arc::clone(store);
            match blocking(move || store.load()).await {
                Ok(Ok(Some(auth))) => (auth, false),
                Ok(Ok(None)) => return Ok(None),
                Ok(Err(error)) => return Err(ConnectFailure::Other(error.to_string())),
                Err(error) => return Err(ConnectFailure::Other(error)),
            }
        }
    };
    let token = match &auth {
        GithubAuth::Token(token) => token.clone(),
        GithubAuth::GhCli => {
            let gh = Arc::clone(gh);
            match blocking(move || gh.token()).await {
                Ok(Ok(token)) => token,
                Ok(Err(GhError::NotInstalled)) => return Err(ConnectFailure::GhMissing),
                Ok(Err(GhError::NotLoggedIn)) => return Err(ConnectFailure::GhNotLoggedIn),
                Ok(Err(GhError::Failed(message))) => return Err(ConnectFailure::Other(message)),
                Err(error) => return Err(ConnectFailure::Other(error)),
            }
        }
    };
    if token.expose_secret().is_empty() {
        return Err(ConnectFailure::BadToken);
    }
    let api = connector.connect(&token).map_err(connect_failure)?;
    let login = api.viewer().await.map_err(connect_failure)?;
    Ok(Some(Linked {
        api,
        login,
        auth,
        save,
    }))
}

fn connect_failure(error: GithubError) -> ConnectFailure {
    match error {
        GithubError::Unauthorized => ConnectFailure::BadToken,
        GithubError::RateLimited { .. } => ConnectFailure::RateLimited,
        GithubError::Unreachable => ConnectFailure::Unreachable,
        other => ConnectFailure::Other(other.to_string()),
    }
}
