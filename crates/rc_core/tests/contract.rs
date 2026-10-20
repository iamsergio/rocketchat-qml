// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The client against a real Rocket.Chat server, named by `RC_TEST_SERVER_URL`. Run with
//! `--ignored` (see the README).
//!
//! Every test makes its own user and rooms with unique names and only looks at those, so the tests
//! hold on a dirty server, in any order and in parallel. Server settings are never changed. Admin
//! only sets things up: no test logs a shared account in or out through the client.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::net::TcpListener;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use rc_core::adapters::MemoryStore;
use rc_core::domain::{Message, MessageKind, Room, RoomId, RoomKind};
use rc_core::ports::SecretStore;
use rc_core::rooms::RoomChange;
use rc_core::session::{LoginFailure, SessionState};
use rc_core::timeline::{PAGE_SIZE, TimelineChange, TimelineStatus};
use rc_core::{Client, Event};
use rc_test_server::{Admin, TestUser, server_url, unique};
use secrecy::SecretString;

const TIMEOUT: Duration = Duration::from_secs(15);

/// A new user, who can log in at once.
fn new_user() -> TestUser {
    Admin::login().create_user()
}

fn password(user: &TestUser) -> SecretString {
    SecretString::from(user.password.clone())
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
fn a_user_logs_in() {
    let user = new_user();
    let (client, events) = start();
    client.login(&server_url(), &user.username, password(&user));
    let SessionState::LoggedIn(account) = outcome(&events) else {
        panic!("login failed");
    };
    assert_eq!(account.user.username, user.username);
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
fn a_user_logs_out() {
    let user = new_user();
    let (client, events) = start();
    client.login(&server_url(), &user.username, password(&user));
    assert!(matches!(outcome(&events), SessionState::LoggedIn(_)));
    client.logout();
    assert_eq!(next(&events), SessionState::LoggedOut { failure: None });
}

// AUTH-004
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn a_new_client_resumes_the_stored_session() {
    let user = new_user();
    let store = MemoryStore::default();

    let (first, events) = start_with(store.clone());
    assert_eq!(next(&events), SessionState::LoggedOut { failure: None });
    first.login(&server_url(), &user.username, password(&user));
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
    assert_eq!(account.user.username, user.username);

    second.logout();
    assert_eq!(next(&events), SessionState::LoggedOut { failure: None });
    // Dropping flushes the queued clear.
    drop(second);
    assert!(store.load().unwrap().is_none());
}

// ROOM-001, ROOM-002
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn a_user_gets_their_rooms_after_logging_in() {
    // A channel of the test's own for the new user: the server may hold anything else, and the
    // user may be in rooms the server adds by itself.
    let admin = Admin::login();
    let user = admin.create_user();
    let (id, name) = admin.create_channel(&[&user.username]);
    let (client, events) = start();
    client.login(&server_url(), &user.username, password(&user));
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

/// Logs `user` in and waits for the room list. Everything the client reports meanwhile is dropped.
fn logged_in(user: &TestUser) -> (Client, Receiver<Event>) {
    let (client, events) = start();
    client.login(&server_url(), &user.username, password(user));
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
    let user = admin_api.create_user();
    let (id, _) = admin_api.create_channel(&[&user.username]);
    let posted: Vec<String> = (0..PAGE_SIZE + 10)
        .map(|n| format!("message {n:03}"))
        .collect();
    for text in &posted {
        admin_api.post_message(&id, text);
    }
    let (client, events) = logged_in(&user);

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

/// Reads events like a frontend until `found` returns something. Fails if that takes too long.
fn wait_for<T>(events: &Receiver<Event>, mut found: impl FnMut(&Event) -> Option<T>) -> T {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        let event = events.recv_timeout(left).expect("the event never came");
        if let Some(value) = found(&event) {
            return value;
        }
    }
}

/// The room a change inserts, updates or moves, as it is afterwards.
fn changed_room(change: &RoomChange) -> Option<&Room> {
    match change {
        RoomChange::Inserted { room, .. }
        | RoomChange::Updated { room, .. }
        | RoomChange::Moved { room, .. } => Some(room),
        _ => None,
    }
}

/// The first room that `event` changes and `wanted` accepts.
fn room_changed(event: &Event, wanted: impl Fn(&Room) -> bool) -> Option<Room> {
    let Event::Rooms(changes) = event else {
        return None;
    };
    changes
        .iter()
        .filter_map(changed_room)
        .find(|room| wanted(room))
        .cloned()
}

// RT-001
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn a_message_posted_to_the_open_room_arrives() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (id, _) = admin.create_channel(&[&user.username]);
    let (client, events) = logged_in(&user);
    client.open_room(id.clone().into());
    let mut mirror = Vec::new();
    settle(&events, &mut mirror);

    let posted = admin.post_message(&id, "hello from admin");
    wait_for(&events, |event| {
        if let Event::Timeline(changes) = event {
            for change in changes {
                assert!(
                    matches!(change, TimelineChange::Appended(_)),
                    "expected an append, got {change:?}"
                );
                change.apply(&mut mirror);
            }
        }
        mirror
            .iter()
            .any(|m| m.id.as_str() == posted.id)
            .then_some(())
    });
    assert_eq!(texts(&mirror), ["hello from admin"]);
    assert!(mirror.iter().all(|m| m.room_id.as_str() == id));
}

// RT-002, ROOM-001
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn a_channel_created_with_the_user_appears_in_the_room_list() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (client, events) = logged_in(&user);

    let (id, name) = admin.create_channel(&[&user.username]);
    let room = wait_for(&events, |event| {
        let Event::Rooms(changes) = event else {
            return None;
        };
        changes.iter().find_map(|change| match change {
            RoomChange::Inserted { room, .. } if room.id.as_str() == id => Some(room.clone()),
            _ => None,
        })
    });
    assert_eq!(room.kind, RoomKind::Channel);
    assert_eq!(room.name, name);
    drop(client);
}

// RT-002, ROOM-003
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn a_direct_message_raises_the_unread_count() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (client, events) = logged_in(&user);

    let dm = admin.create_dm(&user.username);
    admin.post_message(&dm, "one");
    let room = wait_for(&events, |event| {
        room_changed(event, |room| room.id.as_str() == dm && room.unread == 1)
    });
    assert_eq!(room.kind, RoomKind::Direct);
    admin.post_message(&dm, "two");
    wait_for(&events, |event| {
        room_changed(event, |room| room.id.as_str() == dm && room.unread == 2)
    });
    drop(client);
}
