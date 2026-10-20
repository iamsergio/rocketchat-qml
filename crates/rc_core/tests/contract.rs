// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The client against a real Rocket.Chat server, named by `RC_TEST_SERVER_URL`. Run with
//! `--ignored` (see the README).
//!
//! Only admin logs in successfully, each test with its own token. A wrong password is never tried
//! for a real account: repeated failures would lock it and break the other tests.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::net::TcpListener;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use rc_core::adapters::MemoryStore;
use rc_core::domain::{Message, MessageKind, Room, RoomId, RoomKind};
use rc_core::ports::SecretStore;
use rc_core::rooms::RoomChange;
use rc_core::session::{LoginFailure, SessionState};
use rc_core::timeline::{PAGE_SIZE, TimelineStatus};
use rc_core::{Client, Event};
use rc_test_server::{Admin, server_url, unique};
use secrecy::SecretString;

const TIMEOUT: Duration = Duration::from_secs(15);

fn admin() -> (String, SecretString) {
    let user = std::env::var("RC_ADMIN_USER").unwrap_or_else(|_| "admin".into());
    let pass = std::env::var("RC_ADMIN_PASS").unwrap_or_else(|_| "admin".into());
    (user, SecretString::from(pass))
}

/// A client with nothing stored, past its first event.
fn start() -> (Client, Receiver<Event>) {
    let (client, events) = start_with(MemoryStore::default());
    assert_eq!(next(&events), SessionState::LoggedOut { failure: None });
    (client, events)
}

/// A client on `store`; the first event is left to the caller.
fn start_with(store: MemoryStore) -> (Client, Receiver<Event>) {
    let (tx, rx) = mpsc::channel();
    let client = Client::start(store, move |event| {
        let _ = tx.send(event);
    })
    .unwrap();
    (client, rx)
}

/// The next session state. Room events are skipped: only the room test looks at them.
fn next(events: &Receiver<Event>) -> SessionState {
    loop {
        if let Event::Session(state) = events.recv_timeout(TIMEOUT).expect("no event") {
            return state;
        }
    }
}

/// Expects `Connecting`, then returns the state the login ends in.
fn outcome(events: &Receiver<Event>) -> SessionState {
    assert_eq!(next(events), SessionState::Connecting);
    next(events)
}

fn assert_failure(state: &SessionState, expected: &LoginFailure) {
    assert_eq!(
        state,
        &SessionState::LoggedOut {
            failure: Some(expected.clone())
        }
    );
}

// AUTH-001
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn admin_logs_in() {
    let (client, events) = start();
    let (user, password) = admin();
    client.login(&server_url(), &user, password);
    let SessionState::LoggedIn(account) = outcome(&events) else {
        panic!("login failed");
    };
    assert_eq!(account.user.username, user);
    assert_eq!(account.server, server_url());
}

// AUTH-002
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn unknown_user_is_refused() {
    let (client, events) = start();
    let username = format!("ct-nobody-{}", unique());
    client.login(&server_url(), &username, SecretString::from("whatever"));
    assert_failure(&outcome(&events), &LoginFailure::InvalidCredentials);
}

// AUTH-003
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn closed_port_is_unreachable() {
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let (client, events) = start();
    client.login(
        &format!("http://127.0.0.1:{port}"),
        "u",
        SecretString::from("p"),
    );
    assert_failure(&outcome(&events), &LoginFailure::Unreachable);
}

// AUTH-005
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn admin_logs_out() {
    let (client, events) = start();
    let (user, password) = admin();
    client.login(&server_url(), &user, password);
    assert!(matches!(outcome(&events), SessionState::LoggedIn(_)));
    client.logout();
    assert_eq!(next(&events), SessionState::LoggedOut { failure: None });
}

// AUTH-004
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn a_new_client_resumes_the_stored_session() {
    let (user, password) = admin();
    let store = MemoryStore::default();

    let (first, events) = start_with(store.clone());
    assert_eq!(next(&events), SessionState::LoggedOut { failure: None });
    first.login(&server_url(), &user, password);
    assert!(matches!(outcome(&events), SessionState::LoggedIn(_)));
    // Dropping flushes the queued save, like quitting the app.
    drop(first);
    drop(events);

    // No login command this time.
    let (second, events) = start_with(store.clone());
    assert_eq!(next(&events), SessionState::Resuming);
    let SessionState::LoggedIn(account) = next(&events) else {
        panic!("resume failed");
    };
    assert_eq!(account.user.username, user);

    second.logout();
    assert_eq!(next(&events), SessionState::LoggedOut { failure: None });
    // Dropping flushes the queued clear.
    drop(second);
    assert!(store.load().unwrap().is_none());
}

// ROOM-001, ROOM-002
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn admin_gets_their_rooms_after_logging_in() {
    // A channel of the test's own, which admin joins as its creator: the server may hold anything
    // else, and `general` isn't joined by default.
    let (id, name) = Admin::login().create_channel(&[]);
    let (client, events) = start();
    let (user, password) = admin();
    client.login(&server_url(), &user, password);
    assert!(matches!(outcome(&events), SessionState::LoggedIn(_)));
    let Event::Rooms(changes) = events.recv_timeout(TIMEOUT).expect("no rooms event") else {
        panic!("expected the rooms right after logging in");
    };
    let [RoomChange::Reset(rooms)] = changes.as_slice() else {
        panic!("expected a single reset, got {changes:?}");
    };
    // Only the test's own room: the server may hold any number of others.
    let room: &Room = rooms
        .iter()
        .find(|room| room.id.as_str() == id)
        .expect("the new channel isn't listed");
    assert_eq!(room.kind, RoomKind::Channel);
    assert_eq!(room.name, name);
}

/// Logs admin in and waits for the room list. Everything the client reports meanwhile is dropped.
fn logged_in_admin() -> (Client, Receiver<Event>) {
    let (client, events) = start();
    let (user, password) = admin();
    client.login(&server_url(), &user, password);
    assert!(matches!(outcome(&events), SessionState::LoggedIn(_)));
    loop {
        if let Event::Rooms(_) = events.recv_timeout(TIMEOUT).expect("no rooms event") {
            return (client, events);
        }
    }
}

/// Follows the events like a frontend until the status says that nothing is loading, and returns
/// that status.
fn settle(events: &Receiver<Event>, mirror: &mut Vec<Message>) -> TimelineStatus {
    loop {
        match events.recv_timeout(TIMEOUT).expect("no event") {
            Event::Timeline(changes) => changes.iter().for_each(|change| change.apply(mirror)),
            Event::TimelineStatus(status) if !status.loading && !status.loading_older => {
                return status;
            }
            _ => {}
        }
    }
}

fn texts(messages: &[Message]) -> Vec<&str> {
    messages
        .iter()
        .filter_map(|m| match &m.kind {
            MessageKind::Text(text) => Some(text.as_str()),
            MessageKind::System(_) => None,
        })
        .collect()
}

// MSG-001, MSG-002, MSG-003
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn the_whole_history_loads_page_by_page_in_order() {
    let admin_api = Admin::login();
    let (id, _) = admin_api.create_channel(&[]);
    let posted: Vec<String> = (0..PAGE_SIZE + 10)
        .map(|n| format!("message {n:03}"))
        .collect();
    for text in &posted {
        admin_api.post_message(&id, text);
    }
    let (client, events) = logged_in_admin();

    client.open_room(id.clone().into());
    let mut mirror = Vec::new();
    let status = settle(&events, &mut mirror);
    // Just the newest page: the older messages are still to load, and the order is oldest first.
    assert!(!status.reached_start);
    assert_eq!(status.room.as_ref().map(RoomId::as_str), Some(id.as_str()));
    assert!(mirror.len() <= PAGE_SIZE && texts(&mirror).len() < posted.len());
    let newest = posted.last().unwrap().as_str();
    assert_eq!(texts(&mirror).last().copied(), Some(newest));

    let mut pages = 0;
    while !settle_after_load_older(&client, &events, &mut mirror).reached_start {
        pages += 1;
        assert!(pages < 10, "the history never ends");
    }

    // Every message once, oldest first.
    let expected: Vec<&str> = posted.iter().map(String::as_str).collect();
    assert_eq!(texts(&mirror), expected);
    assert!(mirror.iter().all(|m| m.room_id.as_str() == id));
    let mut unique: Vec<_> = mirror.iter().map(|m| m.id.clone()).collect();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), mirror.len());
}

fn settle_after_load_older(
    client: &Client,
    events: &Receiver<Event>,
    mirror: &mut Vec<Message>,
) -> TimelineStatus {
    client.load_older();
    settle(events, mirror)
}
