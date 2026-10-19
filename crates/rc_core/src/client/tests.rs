// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The session actor driven through a fake server API and in-memory stores. Plain `#[test]`s: the
//! client owns its runtime and can't be dropped inside one. Nothing here touches the OS keyring.

use std::collections::VecDeque;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use rc_api::ServerVersion;
use secrecy::{ExposeSecret, SecretString};
use tokio::sync::Notify;

use super::*;
use crate::adapters::MemoryStore;
use crate::domain::{Account, User};
use crate::ports::ApiError;

const TIMEOUT: Duration = Duration::from_secs(5);

struct LoginScript {
    /// Holds the login until the test notifies it.
    gate: Option<Arc<Notify>>,
    result: Result<(), ApiError>,
}

#[derive(Default)]
struct FakeState {
    version: Option<Result<ServerVersion, ApiError>>,
    connect_error: Option<ApiError>,
    logins: VecDeque<LoginScript>,
    logout_result: Option<Result<(), ApiError>>,
    /// How long `logout` takes.
    logout_delay: Duration,
    me_result: Option<Result<User, ApiError>>,
    me_gate: Option<Arc<Notify>>,
    login_calls: Vec<(String, String)>,
    logout_calls: Vec<(String, String)>,
    /// Logouts that ran to the end.
    logouts_done: usize,
    me_calls: Vec<(String, String)>,
}

/// Scripted results, plus a record of what it was asked.
#[derive(Clone, Default)]
struct Fake(Arc<Mutex<FakeState>>);

impl Fake {
    fn new() -> Self {
        let fake = Self::default();
        fake.state().version = Some(Ok(ServerVersion { major: 8, minor: 0 }));
        fake.state().me_result = Some(Ok(user("alice")));
        fake
    }

    fn state(&self) -> MutexGuard<'_, FakeState> {
        self.0.lock().unwrap()
    }

    fn script_login(&self, gate: Option<Arc<Notify>>, result: Result<(), ApiError>) {
        self.state().logins.push_back(LoginScript { gate, result });
    }

    fn login_calls(&self) -> Vec<(String, String)> {
        self.state().login_calls.clone()
    }

    fn wait_for_logout_call(&self) -> (String, String) {
        wait_until(|| self.state().logout_calls.first().cloned())
    }

    fn wait_for_me_call(&self) {
        wait_until(|| (!self.state().me_calls.is_empty()).then_some(()));
    }

    /// The login task has started and is inside `login`.
    fn wait_for_login_calls(&self, count: usize) {
        wait_until(|| (self.state().login_calls.len() >= count).then_some(()));
    }
}

/// Polls `check` until it returns a value.
fn wait_until<T>(check: impl Fn() -> Option<T>) -> T {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if let Some(value) = check() {
            return value;
        }
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}

impl Connector for Fake {
    type Api = Self;

    fn connect(&self, _server: &str) -> Result<Self, ApiError> {
        match self.state().connect_error.clone() {
            Some(error) => Err(error),
            None => Ok(self.clone()),
        }
    }
}

impl ServerApi for Fake {
    fn version(&self) -> impl Future<Output = Result<ServerVersion, ApiError>> + Send {
        std::future::ready(self.state().version.clone().unwrap())
    }

    async fn login(
        &self,
        username: &str,
        password: &SecretString,
    ) -> Result<(Credentials, User), ApiError> {
        let script = {
            let mut state = self.state();
            state
                .login_calls
                .push((username.to_owned(), password.expose_secret().to_owned()));
            state.logins.pop_front().unwrap()
        };
        if let Some(gate) = script.gate {
            gate.notified().await;
        }
        script.result?;
        Ok((
            Credentials {
                user_id: "uid".to_owned().into(),
                token: SecretString::from("tok"),
            },
            user(username),
        ))
    }

    async fn me(&self, credentials: &Credentials) -> Result<User, ApiError> {
        let gate = {
            let mut state = self.state();
            state.me_calls.push((
                credentials.user_id.to_string(),
                credentials.token.expose_secret().to_owned(),
            ));
            state.me_gate.clone()
        };
        if let Some(gate) = gate {
            gate.notified().await;
        }
        self.state().me_result.clone().unwrap()
    }

    async fn logout(&self, credentials: &Credentials) -> Result<(), ApiError> {
        let delay = {
            let mut state = self.state();
            state.logout_calls.push((
                credentials.user_id.to_string(),
                credentials.token.expose_secret().to_owned(),
            ));
            state.logout_delay
        };
        tokio::time::sleep(delay).await;
        let mut state = self.state();
        state.logouts_done += 1;
        state.logout_result.clone().unwrap_or(Ok(()))
    }
}

/// A memory store that can be told to fail.
#[derive(Clone, Default)]
struct FakeStore {
    inner: MemoryStore,
    fail_load: bool,
    fail_save: bool,
}

impl SecretStore for FakeStore {
    fn load(&self) -> Result<Option<StoredSession>, StoreError> {
        if self.fail_load {
            return Err(StoreError::Unavailable("locked".into()));
        }
        self.inner.load()
    }

    fn save(&self, session: &StoredSession) -> Result<(), StoreError> {
        if self.fail_save {
            return Err(StoreError::Unavailable("locked".into()));
        }
        self.inner.save(session)
    }

    fn clear(&self) -> Result<(), StoreError> {
        self.inner.clear()
    }
}

fn user(username: &str) -> User {
    User {
        id: "uid".to_owned().into(),
        username: username.to_owned(),
        display_name: Some("Display".to_owned()),
    }
}

fn logged_out(failure: Option<LoginFailure>) -> SessionState {
    SessionState::LoggedOut { failure }
}

fn logged_in(server: &str, username: &str) -> SessionState {
    SessionState::LoggedIn(Account {
        server: server.to_owned(),
        user: user(username),
    })
}

fn stored(server: &str) -> StoredSession {
    StoredSession {
        server: server.to_owned(),
        credentials: Credentials {
            user_id: "uid".to_owned().into(),
            token: SecretString::from("stored-tok"),
        },
    }
}

/// A store that already holds a session.
fn store_with(server: &str) -> MemoryStore {
    let store = MemoryStore::default();
    store.save(&stored(server)).unwrap();
    store
}

fn wait_for_empty(store: &MemoryStore) {
    wait_until(|| store.load().unwrap().is_none().then_some(()));
}

struct Harness {
    client: Client,
    events: Receiver<Event>,
    /// The first event, which every client sends.
    first: SessionState,
}

impl Harness {
    fn new(fake: &Fake) -> Self {
        Self::with_store(fake, MemoryStore::default())
    }

    fn with_store(fake: &Fake, store: impl SecretStore) -> Self {
        let (tx, events) = mpsc::channel();
        let client = Client::with_ports(fake.clone(), store, move |event| {
            let _ = tx.send(event);
        })
        .unwrap();
        let mut harness = Self {
            client,
            events,
            first: SessionState::Connecting,
        };
        harness.first = harness.next();
        harness
    }

    fn login(&self, server: &str, username: &str) {
        self.client
            .login(server, username, SecretString::from("secret"));
    }

    fn next(&self) -> SessionState {
        match self.events.recv_timeout(TIMEOUT).expect("no event") {
            Event::Session(state) => state,
        }
    }

    /// Checks the next events, in order.
    fn expect(&self, states: &[SessionState]) {
        for expected in states {
            assert_eq!(&self.next(), expected);
        }
    }
}

fn failing_login(failure: ApiError, expected: LoginFailure) {
    let fake = Fake::new();
    fake.script_login(None, Err(failure));
    let harness = Harness::new(&fake);
    harness.login("http://h", "u");
    harness.expect(&[SessionState::Connecting, logged_out(Some(expected))]);
}

// AUTH-001
#[test]
fn login_succeeds() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    let harness = Harness::new(&fake);
    harness.login(" chat.example.com ", "alice");
    harness.expect(&[
        SessionState::Connecting,
        logged_in("https://chat.example.com", "alice"),
    ]);
    assert_eq!(
        fake.login_calls(),
        [("alice".to_owned(), "secret".to_owned())]
    );
}

// AUTH-002
#[test]
fn invalid_credentials_are_reported() {
    failing_login(
        ApiError::InvalidCredentials,
        LoginFailure::InvalidCredentials,
    );
}

// AUTH-003
#[test]
fn invalid_url_is_reported() {
    let fake = Fake::new();
    fake.state().connect_error = Some(ApiError::InvalidUrl("x".into()));
    let harness = Harness::new(&fake);
    harness.login("x", "u");
    harness.expect(&[
        SessionState::Connecting,
        logged_out(Some(LoginFailure::InvalidUrl)),
    ]);
}

// AUTH-003
#[test]
fn unreachable_server_is_reported() {
    let fake = Fake::new();
    fake.state().version = Some(Err(ApiError::Unreachable("refused".into())));
    let harness = Harness::new(&fake);
    harness.login("http://h", "u");
    harness.expect(&[
        SessionState::Connecting,
        logged_out(Some(LoginFailure::Unreachable)),
    ]);
}

// AUTH-003
#[test]
fn non_rocketchat_server_is_reported() {
    let fake = Fake::new();
    fake.state().version = Some(Err(ApiError::NotRocketChat));
    let harness = Harness::new(&fake);
    harness.login("http://h", "u");
    harness.expect(&[
        SessionState::Connecting,
        logged_out(Some(LoginFailure::NotRocketChat)),
    ]);
}

// AUTH-003
#[test]
fn old_server_is_refused_without_logging_in() {
    let fake = Fake::new();
    fake.state().version = Some(Ok(ServerVersion { major: 7, minor: 9 }));
    let harness = Harness::new(&fake);
    harness.login("http://h", "u");
    let failure = LoginFailure::UnsupportedVersion {
        found: "7.9".into(),
        minimum: "8.0".into(),
    };
    harness.expect(&[SessionState::Connecting, logged_out(Some(failure))]);
    assert!(fake.login_calls().is_empty());
}

// AUTH-003
#[test]
fn server_errors_are_reported_with_their_message() {
    let error = ApiError::Server {
        status: 500,
        message: "boom".into(),
    };
    let expected = LoginFailure::Server(error.to_string());
    assert!(expected.to_string().contains("boom"));
    failing_login(error, expected);
}

// AUTH-005
#[test]
fn logout_revokes_the_token() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    let harness = Harness::new(&fake);
    harness.login("http://h", "alice");
    harness.expect(&[SessionState::Connecting, logged_in("http://h", "alice")]);
    harness.client.logout();
    harness.expect(&[logged_out(None)]);
    assert_eq!(
        fake.wait_for_logout_call(),
        ("uid".to_owned(), "tok".to_owned())
    );
}

// AUTH-005
#[test]
fn logout_ends_logged_out_even_if_the_server_call_fails() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    fake.state().logout_result = Some(Err(ApiError::Unauthorized));
    let harness = Harness::new(&fake);
    harness.login("http://h", "alice");
    harness.expect(&[SessionState::Connecting, logged_in("http://h", "alice")]);
    harness.client.logout();
    harness.expect(&[logged_out(None)]);
    fake.wait_for_logout_call();
}

// AUTH-005
#[test]
fn logout_while_connecting_cancels_the_login() {
    let fake = Fake::new();
    let gate = Arc::new(Notify::new());
    fake.script_login(Some(Arc::clone(&gate)), Ok(()));
    fake.script_login(None, Err(ApiError::InvalidCredentials));
    let harness = Harness::new(&fake);
    harness.login("http://h", "alice");
    harness.expect(&[SessionState::Connecting]);
    // Cancel only once the attempt is really waiting at the gate.
    fake.wait_for_login_calls(1);

    harness.client.logout();
    harness.expect(&[logged_out(None)]);

    // The cancelled attempt must not surface, whatever it does now.
    gate.notify_one();
    harness.login("http://h", "bob");
    harness.expect(&[
        SessionState::Connecting,
        logged_out(Some(LoginFailure::InvalidCredentials)),
    ]);
}

// AUTH-001
#[test]
fn login_while_connecting_is_ignored() {
    let fake = Fake::new();
    let gate = Arc::new(Notify::new());
    fake.script_login(Some(Arc::clone(&gate)), Ok(()));
    let harness = Harness::new(&fake);
    harness.login("http://h", "alice");
    harness.login("http://h", "bob");
    harness.expect(&[SessionState::Connecting]);

    gate.notify_one();
    harness.expect(&[logged_in("http://h", "alice")]);
    // Nothing extra came in between: the next event is the logout's.
    harness.client.logout();
    harness.expect(&[logged_out(None)]);
    assert_eq!(fake.login_calls().len(), 1);
}

// AUTH-002
#[test]
fn a_new_login_clears_the_old_failure() {
    let fake = Fake::new();
    fake.script_login(None, Err(ApiError::InvalidCredentials));
    fake.script_login(None, Ok(()));
    let harness = Harness::new(&fake);
    harness.login("http://h", "alice");
    harness.expect(&[
        SessionState::Connecting,
        logged_out(Some(LoginFailure::InvalidCredentials)),
    ]);
    harness.login("http://h", "alice");
    harness.expect(&[SessionState::Connecting, logged_in("http://h", "alice")]);
}

// AUTH-004
#[test]
fn an_empty_store_starts_logged_out() {
    let harness = Harness::new(&Fake::new());
    assert_eq!(harness.first, logged_out(None));
}

// AUTH-004
#[test]
fn a_stored_session_is_resumed_without_logging_in() {
    let fake = Fake::new();
    let harness = Harness::with_store(&fake, store_with("http://h"));
    assert_eq!(harness.first, SessionState::Connecting);
    harness.expect(&[logged_in("http://h", "alice")]);
    assert_eq!(
        fake.state().me_calls,
        [("uid".to_owned(), "stored-tok".to_owned())]
    );
    assert!(fake.login_calls().is_empty());
}

// AUTH-004
#[test]
fn a_rejected_stored_session_expires_and_is_forgotten() {
    let fake = Fake::new();
    fake.state().me_result = Some(Err(ApiError::Unauthorized));
    let store = store_with("http://h");
    let harness = Harness::with_store(&fake, store.clone());
    harness.expect(&[logged_out(Some(LoginFailure::SessionExpired))]);
    wait_for_empty(&store);
}

// AUTH-004
#[test]
fn an_unreachable_server_keeps_the_stored_session() {
    let fake = Fake::new();
    fake.state().version = Some(Err(ApiError::Unreachable("refused".into())));
    let store = store_with("http://h");
    let harness = Harness::with_store(&fake, store.clone());
    harness.expect(&[logged_out(Some(LoginFailure::Unreachable))]);
    // The state is final before any store write would have been queued; flush the queue by
    // dropping the client.
    drop(harness);
    assert!(store.load().unwrap().is_some());
}

// AUTH-004
#[test]
fn a_server_that_is_now_too_old_keeps_the_stored_session() {
    let fake = Fake::new();
    fake.state().version = Some(Ok(ServerVersion { major: 7, minor: 9 }));
    let store = store_with("http://h");
    let harness = Harness::with_store(&fake, store.clone());
    let failure = LoginFailure::UnsupportedVersion {
        found: "7.9".into(),
        minimum: "8.0".into(),
    };
    harness.expect(&[logged_out(Some(failure))]);
    drop(harness);
    assert!(store.load().unwrap().is_some());
    assert!(fake.state().me_calls.is_empty());
}

// AUTH-004
#[test]
fn a_store_that_cant_be_read_starts_logged_out_and_logins_still_work() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    let store = FakeStore {
        fail_load: true,
        ..FakeStore::default()
    };
    let harness = Harness::with_store(&fake, store);
    assert_eq!(harness.first, logged_out(None));
    harness.login("http://h", "alice");
    harness.expect(&[SessionState::Connecting, logged_in("http://h", "alice")]);
}

// AUTH-004
#[test]
fn a_session_survives_a_restart() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    let store = MemoryStore::default();

    let first = Harness::with_store(&fake, store.clone());
    first.login("http://h", "alice");
    first.expect(&[SessionState::Connecting, logged_in("http://h", "alice")]);
    // Dropping flushes the queued save.
    drop(first);

    let second = Harness::with_store(&fake, store);
    assert_eq!(second.first, SessionState::Connecting);
    second.expect(&[logged_in("http://h", "alice")]);
    assert_eq!(fake.login_calls().len(), 1);
}

// AUTH-001
#[test]
fn login_saves_the_session() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    let store = MemoryStore::default();
    let harness = Harness::with_store(&fake, store.clone());
    harness.login("chat.example.com", "alice");
    harness.expect(&[
        SessionState::Connecting,
        logged_in("https://chat.example.com", "alice"),
    ]);
    let saved = wait_until(|| store.load().unwrap());
    assert_eq!(saved.server, "https://chat.example.com");
    assert_eq!(saved.credentials.user_id.as_str(), "uid");
    assert_eq!(saved.credentials.token.expose_secret(), "tok");
}

// AUTH-001
#[test]
fn a_store_that_cant_be_written_does_not_stop_the_login() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    let store = FakeStore {
        fail_save: true,
        ..FakeStore::default()
    };
    let harness = Harness::with_store(&fake, store);
    harness.login("http://h", "alice");
    harness.expect(&[SessionState::Connecting, logged_in("http://h", "alice")]);
}

// AUTH-005
#[test]
fn logout_forgets_the_stored_session() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    let store = MemoryStore::default();
    let harness = Harness::with_store(&fake, store.clone());
    harness.login("http://h", "alice");
    harness.expect(&[SessionState::Connecting, logged_in("http://h", "alice")]);
    harness.client.logout();
    harness.expect(&[logged_out(None)]);
    // The save and the clear are applied in order, so the store ends up empty.
    wait_for_empty(&store);
    fake.wait_for_logout_call();
}

// AUTH-005
#[test]
fn dropping_the_client_after_logout_finishes_the_logout() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    fake.state().logout_delay = Duration::from_millis(200);
    let store = MemoryStore::default();
    let harness = Harness::with_store(&fake, store.clone());
    harness.login("http://h", "alice");
    harness.expect(&[SessionState::Connecting, logged_in("http://h", "alice")]);
    harness.client.logout();
    harness.expect(&[logged_out(None)]);

    drop(harness);

    assert_eq!(fake.state().logouts_done, 1);
    assert!(store.load().unwrap().is_none());
}

// AUTH-005
#[test]
fn logout_while_resuming_cancels_and_forgets() {
    let fake = Fake::new();
    let gate = Arc::new(Notify::new());
    fake.state().me_gate = Some(Arc::clone(&gate));
    let store = store_with("http://h");
    let harness = Harness::with_store(&fake, store.clone());
    assert_eq!(harness.first, SessionState::Connecting);
    fake.wait_for_me_call();

    harness.client.logout();

    harness.expect(&[logged_out(None)]);
    wait_for_empty(&store);
    assert_eq!(
        fake.wait_for_logout_call(),
        ("uid".to_owned(), "stored-tok".to_owned())
    );
}
