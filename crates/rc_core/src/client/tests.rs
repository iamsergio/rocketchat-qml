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
use crate::domain::{Account, Author, Message, MessageKind, Room, RoomKind, User};
use crate::ports::{ApiError, Page};
use crate::rooms::RoomChange;
use crate::timeline::{TimelineChange, TimelineError, TimelineStatus};

const TIMEOUT: Duration = Duration::from_secs(5);

struct LoginScript {
    /// Holds the login until the test notifies it.
    gate: Option<Arc<Notify>>,
    result: Result<(), ApiError>,
}

struct FetchScript {
    /// Holds the fetch until the test notifies it.
    gate: Option<Arc<Notify>>,
    result: Result<Vec<rc_api::Subscription>, ApiError>,
}

struct HistoryScript {
    /// Holds the fetch until the test notifies it.
    gate: Option<Arc<Notify>>,
    result: Result<Page, ApiError>,
}

/// What `history` was asked.
#[derive(Clone, Debug, PartialEq, Eq)]
struct HistoryCall {
    room: String,
    kind: RoomKind,
    before: Option<jiff::Timestamp>,
    count: usize,
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
    /// One per room fetch; an empty room list once they run out.
    fetches: VecDeque<FetchScript>,
    subscriptions_calls: Vec<(String, String)>,
    /// One per history fetch; an empty page at the start once they run out.
    histories: VecDeque<HistoryScript>,
    history_calls: Vec<HistoryCall>,
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

    fn script_fetch(
        &self,
        gate: Option<Arc<Notify>>,
        result: Result<Vec<rc_api::Subscription>, ApiError>,
    ) {
        self.state().fetches.push_back(FetchScript { gate, result });
    }

    fn script_history(&self, gate: Option<Arc<Notify>>, result: Result<Page, ApiError>) {
        self.state()
            .histories
            .push_back(HistoryScript { gate, result });
    }

    /// The history fetch task has started and is inside `history`.
    fn wait_for_history_calls(&self, count: usize) -> Vec<HistoryCall> {
        wait_until(|| {
            let calls = self.state().history_calls.clone();
            (calls.len() >= count).then_some(calls)
        })
    }

    /// The fetch task has started and is inside `subscriptions`.
    fn wait_for_subscriptions_calls(&self, count: usize) {
        wait_until(|| (self.state().subscriptions_calls.len() >= count).then_some(()));
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

    async fn subscriptions(
        &self,
        credentials: &Credentials,
    ) -> Result<Vec<rc_api::Subscription>, ApiError> {
        let script = {
            let mut state = self.state();
            state.subscriptions_calls.push((
                credentials.user_id.to_string(),
                credentials.token.expose_secret().to_owned(),
            ));
            state.fetches.pop_front()
        };
        let Some(script) = script else {
            return Ok(Vec::new());
        };
        if let Some(gate) = script.gate {
            gate.notified().await;
        }
        script.result
    }

    async fn history(
        &self,
        _credentials: &Credentials,
        room: &RoomId,
        kind: RoomKind,
        before: Option<jiff::Timestamp>,
        count: usize,
    ) -> Result<Page, ApiError> {
        let script = {
            let mut state = self.state();
            state.history_calls.push(HistoryCall {
                room: room.to_string(),
                kind,
                before,
                count,
            });
            state.histories.pop_front()
        };
        let Some(script) = script else {
            return Ok(Page {
                messages: Vec::new(),
                reached_start: true,
            });
        };
        if let Some(gate) = script.gate {
            gate.notified().await;
        }
        script.result
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

fn subscription(id: &str, name: &str, room_type: rc_api::RoomType) -> rc_api::Subscription {
    rc_api::Subscription {
        room_id: id.to_owned(),
        room_type,
        name: name.to_owned(),
        display_name: None,
        unread: 0,
        user_mentions: 0,
        group_mentions: 0,
        alert: false,
        open: true,
    }
}

fn room(id: &str, name: &str, kind: RoomKind) -> Room {
    Room {
        id: id.to_owned().into(),
        kind,
        name: name.to_owned(),
        unread: 0,
        user_mentions: 0,
        group_mentions: 0,
        alert: false,
    }
}

fn session_event(state: SessionState) -> Event {
    Event::Session(state)
}

fn rooms_event(rooms: Vec<Room>) -> Event {
    Event::Rooms(vec![RoomChange::Reset(rooms)])
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

    fn next_event(&self) -> Event {
        self.events.recv_timeout(TIMEOUT).expect("no event")
    }

    /// The next session state. Room events in between are skipped: the session tests don't care
    /// about them, the room tests use [`next_event`](Self::next_event).
    fn next(&self) -> SessionState {
        loop {
            if let Event::Session(state) = self.next_event() {
                return state;
            }
        }
    }

    /// Checks the next session states, in order.
    fn expect(&self, states: &[SessionState]) {
        for expected in states {
            assert_eq!(&self.next(), expected);
        }
    }

    /// Checks the next events, in order, room events included.
    fn expect_events(&self, events: Vec<Event>) {
        for expected in events {
            assert_eq!(self.next_event(), expected);
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
    assert_eq!(harness.first, SessionState::Resuming);
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
fn a_login_after_a_failed_resume_is_connecting_not_resuming() {
    let fake = Fake::new();
    fake.state().me_result = Some(Err(ApiError::Unauthorized));
    fake.script_login(None, Ok(()));
    let harness = Harness::with_store(&fake, store_with("http://h"));
    assert_eq!(harness.first, SessionState::Resuming);
    harness.expect(&[logged_out(Some(LoginFailure::SessionExpired))]);
    harness.login("http://h", "alice");
    harness.expect(&[SessionState::Connecting, logged_in("http://h", "alice")]);
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
    assert_eq!(second.first, SessionState::Resuming);
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
    assert_eq!(harness.first, SessionState::Resuming);
    fake.wait_for_me_call();

    harness.client.logout();

    harness.expect(&[logged_out(None)]);
    wait_for_empty(&store);
    assert_eq!(
        fake.wait_for_logout_call(),
        ("uid".to_owned(), "stored-tok".to_owned())
    );
}

// ROOM-001
#[test]
fn login_lists_the_rooms_sorted() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    fake.script_fetch(
        None,
        Ok(vec![
            subscription("2", "random", rc_api::RoomType::Channel),
            subscription("3", "bob", rc_api::RoomType::Direct),
            subscription("1", "General", rc_api::RoomType::Channel),
        ]),
    );
    let harness = Harness::new(&fake);
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![
            room("3", "bob", RoomKind::Direct),
            room("1", "General", RoomKind::Channel),
            room("2", "random", RoomKind::Channel),
        ]),
    ]);
    assert_eq!(
        fake.state().subscriptions_calls,
        [("uid".to_owned(), "tok".to_owned())]
    );
}

// ROOM-001
#[test]
fn hidden_and_unlisted_rooms_are_not_in_the_list() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    let mut hidden = subscription("h", "hidden", rc_api::RoomType::Channel);
    hidden.open = false;
    fake.script_fetch(
        None,
        Ok(vec![
            hidden,
            subscription("l", "livechat", rc_api::RoomType::Other("l".into())),
            subscription("p", "private", rc_api::RoomType::Private),
        ]),
    );
    let harness = Harness::new(&fake);
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![room("p", "private", RoomKind::Private)]),
    ]);
}

// ROOM-001
#[test]
fn a_resumed_session_lists_the_rooms() {
    let fake = Fake::new();
    fake.script_fetch(
        None,
        Ok(vec![subscription(
            "1",
            "general",
            rc_api::RoomType::Channel,
        )]),
    );
    let harness = Harness::with_store(&fake, store_with("http://h"));
    assert_eq!(harness.first, SessionState::Resuming);
    harness.expect_events(vec![
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![room("1", "general", RoomKind::Channel)]),
    ]);
    assert_eq!(
        fake.state().subscriptions_calls,
        [("uid".to_owned(), "stored-tok".to_owned())]
    );
}

// ROOM-001
#[test]
fn logout_empties_the_room_list() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    fake.script_fetch(
        None,
        Ok(vec![subscription(
            "1",
            "general",
            rc_api::RoomType::Channel,
        )]),
    );
    let harness = Harness::new(&fake);
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![room("1", "general", RoomKind::Channel)]),
    ]);
    harness.client.logout();
    harness.expect_events(vec![session_event(logged_out(None)), rooms_event(vec![])]);
}

// AUTH-004
#[test]
fn a_token_that_dies_before_the_fetch_expires_the_session() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    fake.script_fetch(None, Err(ApiError::Unauthorized));
    let store = MemoryStore::default();
    let harness = Harness::with_store(&fake, store.clone());
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        session_event(logged_out(Some(LoginFailure::SessionExpired))),
    ]);
    wait_for_empty(&store);
    // Dropping flushes the actor's work: a revoke would show by now.
    drop(harness);
    assert!(fake.state().logout_calls.is_empty());
}

// ROOM-001
#[test]
fn a_failed_fetch_leaves_the_session_logged_in_without_rooms() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    fake.script_fetch(None, Err(ApiError::Unreachable("refused".into())));
    let harness = Harness::new(&fake);
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
    ]);
    fake.wait_for_subscriptions_calls(1);
    // No rooms event so far, and none for the empty list on logout: the next event is the
    // logout's own, and the one after it is the next login's.
    harness.client.logout();
    fake.script_login(None, Ok(()));
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(logged_out(None)),
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![]),
    ]);
}

// ROOM-001
#[test]
fn a_fetch_that_finishes_after_logout_is_dropped() {
    let fake = Fake::new();
    let gate = Arc::new(Notify::new());
    fake.script_login(None, Ok(()));
    fake.script_login(None, Ok(()));
    fake.script_fetch(
        Some(Arc::clone(&gate)),
        Ok(vec![subscription("old", "old", rc_api::RoomType::Channel)]),
    );
    fake.script_fetch(
        None,
        Ok(vec![subscription("new", "new", rc_api::RoomType::Channel)]),
    );
    let harness = Harness::new(&fake);
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
    ]);
    fake.wait_for_subscriptions_calls(1);

    harness.client.logout();
    harness.expect_events(vec![session_event(logged_out(None))]);
    gate.notify_one();

    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![room("new", "new", RoomKind::Channel)]),
    ]);
}

fn message(id: &str, secs: i64) -> Message {
    Message {
        id: id.to_owned().into(),
        room_id: "1".to_owned().into(),
        author: Author {
            id: "uid".to_owned().into(),
            username: "alice".to_owned(),
            display_name: None,
        },
        ts: jiff::Timestamp::from_second(secs).unwrap(),
        kind: MessageKind::Text(id.to_owned()),
    }
}

// Scripts take the result of the call, which is what the other scripts pass.
#[allow(clippy::unnecessary_wraps)]
fn page(messages: Vec<Message>, reached_start: bool) -> Result<Page, ApiError> {
    Ok(Page {
        messages,
        reached_start,
    })
}

fn reset_event(room: Option<&str>, messages: Vec<Message>) -> Event {
    Event::Timeline(vec![TimelineChange::Reset {
        room: room.map(|id| id.to_owned().into()),
        messages,
    }])
}

fn status(
    room: &str,
    loading: bool,
    loading_older: bool,
    reached_start: bool,
    error: Option<TimelineError>,
) -> Event {
    Event::TimelineStatus(TimelineStatus {
        room: Some(room.to_owned().into()),
        loading,
        loading_older,
        reached_start,
        error,
    })
}

/// The event that closes the open room: no messages, no room.
fn closed_events() -> Vec<Event> {
    vec![
        reset_event(None, Vec::new()),
        Event::TimelineStatus(TimelineStatus::default()),
    ]
}

/// A harness logged in with the rooms `1` (a channel) and `2` (a private group), past their events.
fn logged_in_harness(fake: &Fake) -> Harness {
    fake.script_login(None, Ok(()));
    fake.script_fetch(
        None,
        Ok(vec![
            subscription("1", "general", rc_api::RoomType::Channel),
            subscription("2", "secret", rc_api::RoomType::Private),
        ]),
    );
    let harness = Harness::new(fake);
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![
            room("1", "general", RoomKind::Channel),
            room("2", "secret", RoomKind::Private),
        ]),
    ]);
    harness
}

/// Opens room 1 and takes the events of its first page, which has `a` and `b`.
fn open_first_room(fake: &Fake, harness: &Harness, reached_start: bool) {
    fake.script_history(
        None,
        page(vec![message("b", 2), message("a", 1)], reached_start),
    );
    harness.client.open_room("1".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("1"), Vec::new()),
        status("1", true, false, false, None),
        reset_event(Some("1"), vec![message("a", 1), message("b", 2)]),
        status("1", false, false, reached_start, None),
    ]);
}

// MSG-001
#[test]
fn opening_a_room_shows_the_newest_messages_oldest_first() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    assert_eq!(
        fake.state().history_calls,
        [HistoryCall {
            room: "1".into(),
            kind: RoomKind::Channel,
            before: None,
            count: crate::timeline::PAGE_SIZE,
        }]
    );
}

// MSG-001
#[test]
fn the_kind_of_the_room_is_passed_to_the_server() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    harness.client.open_room("2".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("2"), Vec::new()),
        status("2", true, false, false, None),
        status("2", false, false, true, None),
    ]);
    assert_eq!(fake.state().history_calls[0].kind, RoomKind::Private);
}

// MSG-001
#[test]
fn opening_an_unknown_room_or_when_logged_out_is_ignored() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    let harness = Harness::new(&fake);
    // Logged out.
    harness.client.open_room("1".to_owned().into());
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![]),
    ]);
    // Logged in, but not in the room list.
    harness.client.open_room("nope".to_owned().into());
    harness.client.logout();
    harness.expect_events(vec![session_event(logged_out(None))]);
    drop(harness);
    assert!(fake.state().history_calls.is_empty());
}

// MSG-002
#[test]
fn loading_older_messages_prepends_them() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);

    // The page starts at the oldest message held, which comes back as the duplicate.
    fake.script_history(None, page(vec![message("a", 1), message("z", 0)], true));
    harness.client.load_older();
    harness.expect_events(vec![
        status("1", false, true, false, None),
        Event::Timeline(vec![TimelineChange::Prepended(vec![message("z", 0)])]),
        status("1", false, false, true, None),
    ]);
    let calls = fake.state().history_calls.clone();
    assert_eq!(calls[1].before, Some(message("a", 1).ts));
}

// MSG-002
#[test]
fn loading_older_is_ignored_while_a_fetch_is_in_flight() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    let gate = Arc::new(Notify::new());
    fake.script_history(Some(Arc::clone(&gate)), page(vec![message("z", 0)], false));
    harness.client.load_older();
    harness.expect_events(vec![status("1", false, true, false, None)]);
    fake.wait_for_history_calls(2);

    harness.client.load_older();
    harness.client.load_older();
    gate.notify_one();
    harness.expect_events(vec![
        Event::Timeline(vec![TimelineChange::Prepended(vec![message("z", 0)])]),
        status("1", false, false, false, None),
    ]);
    // Dropping flushes the actor's work: a second fetch would have been made by now.
    drop(harness);
    assert_eq!(fake.state().history_calls.len(), 2);
}

// MSG-002
#[test]
fn loading_older_is_ignored_once_the_start_is_reached() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, true);
    harness.client.load_older();
    drop(harness);
    assert_eq!(fake.state().history_calls.len(), 1);
}

// MSG-002
#[test]
fn loading_older_without_an_open_room_is_ignored() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    harness.client.load_older();
    drop(harness);
    assert!(fake.state().history_calls.is_empty());
}

// MSG-001
#[test]
fn opening_the_open_room_again_does_nothing() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    harness.client.open_room("1".to_owned().into());
    drop(harness);
    assert_eq!(fake.state().history_calls.len(), 1);
}

// MSG-001
#[test]
fn switching_rooms_replaces_the_messages_and_drops_the_stale_answer() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    let gate = Arc::new(Notify::new());
    fake.script_history(Some(Arc::clone(&gate)), page(vec![message("a", 1)], true));
    harness.client.open_room("1".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("1"), Vec::new()),
        status("1", true, false, false, None),
    ]);
    fake.wait_for_history_calls(1);

    let mut other = message("x", 9);
    other.room_id = "2".to_owned().into();
    fake.script_history(None, page(vec![other.clone()], true));
    harness.client.open_room("2".to_owned().into());
    gate.notify_one();
    harness.expect_events(vec![
        reset_event(Some("2"), Vec::new()),
        status("2", true, false, false, None),
        reset_event(Some("2"), vec![other]),
        status("2", false, false, true, None),
    ]);
    // Nothing of room 1 follows: closing the room is the next event.
    harness.client.logout();
    harness.expect_events(vec![session_event(logged_out(None)), rooms_event(vec![])]);
    harness.expect_events(closed_events());
}

// MSG-001
#[test]
fn reopening_a_room_shows_what_was_loaded_first() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    harness.client.open_room("2".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("2"), Vec::new()),
        status("2", true, false, false, None),
        status("2", false, false, true, None),
    ]);

    // A message arrived meanwhile.
    fake.script_history(
        None,
        page(
            vec![message("c", 3), message("b", 2), message("a", 1)],
            false,
        ),
    );
    harness.client.open_room("1".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("1"), vec![message("a", 1), message("b", 2)]),
        status("1", true, false, false, None),
        Event::Timeline(vec![TimelineChange::Appended(vec![message("c", 3)])]),
        status("1", false, false, false, None),
    ]);
}

// MSG-001
#[test]
fn logout_closes_the_room_and_forgets_the_messages() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    harness.client.logout();
    harness.expect_events(vec![session_event(logged_out(None)), rooms_event(vec![])]);
    harness.expect_events(closed_events());

    fake.script_login(None, Ok(()));
    fake.script_fetch(
        None,
        Ok(vec![subscription(
            "1",
            "general",
            rc_api::RoomType::Channel,
        )]),
    );
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![room("1", "general", RoomKind::Channel)]),
    ]);
    // Nothing is held any more: the room opens empty.
    harness.client.open_room("1".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("1"), Vec::new()),
        status("1", true, false, false, None),
    ]);
}

// MSG-001
#[test]
fn an_expired_token_ends_the_session_and_closes_the_room() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    fake.script_history(None, Err(ApiError::Unauthorized));
    harness.client.open_room("1".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("1"), Vec::new()),
        status("1", true, false, false, None),
        session_event(logged_out(Some(LoginFailure::SessionExpired))),
        rooms_event(vec![]),
    ]);
    harness.expect_events(closed_events());
    // It's dead already, so there's nothing to revoke.
    drop(harness);
    assert!(fake.state().logout_calls.is_empty());
}

// MSG-001
#[test]
fn a_failed_fetch_shows_the_error_and_opening_again_retries() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    fake.script_history(None, Err(ApiError::Unreachable("refused".into())));
    harness.client.open_room("1".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("1"), Vec::new()),
        status("1", true, false, false, None),
        status("1", false, false, false, Some(TimelineError::Unreachable)),
    ]);
    // Older messages can't be loaded when there are none.
    harness.client.load_older();

    fake.script_history(None, page(vec![message("a", 1)], true));
    harness.client.open_room("1".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("1"), Vec::new()),
        status("1", true, false, false, None),
        reset_event(Some("1"), vec![message("a", 1)]),
        status("1", false, false, true, None),
    ]);
    assert_eq!(fake.state().history_calls.len(), 2);
}

// MSG-002
#[test]
fn a_failed_older_fetch_keeps_the_messages() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    fake.script_history(
        None,
        Err(ApiError::Server {
            status: 500,
            message: "boom".into(),
        }),
    );
    harness.client.load_older();
    harness.expect_events(vec![
        status("1", false, true, false, None),
        status(
            "1",
            false,
            false,
            false,
            Some(TimelineError::Server(
                "the server answered 500: boom".into(),
            )),
        ),
    ]);
    // Trying again clears the error.
    fake.script_history(None, page(vec![message("z", 0)], true));
    harness.client.load_older();
    harness.expect_events(vec![
        status("1", false, true, false, None),
        Event::Timeline(vec![TimelineChange::Prepended(vec![message("z", 0)])]),
        status("1", false, false, true, None),
    ]);
}
