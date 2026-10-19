// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The client against a real Rocket.Chat server, named by `RC_TEST_SERVER_URL`. Run with
//! `--ignored` (see the README).
//!
//! Only admin logs in successfully, each test with its own token. A wrong password is never tried
//! for a real account: repeated failures would lock it and break the other tests.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::net::TcpListener;
use std::process;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rc_core::adapters::MemoryStore;
use rc_core::ports::SecretStore;
use rc_core::session::{LoginFailure, SessionState};
use rc_core::{Client, Event};
use secrecy::SecretString;

const TIMEOUT: Duration = Duration::from_secs(15);

fn server_url() -> String {
    std::env::var("RC_TEST_SERVER_URL")
        .expect("RC_TEST_SERVER_URL is not set: point it at a Rocket.Chat server, e.g. http://127.0.0.1:3000")
}

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

fn next(events: &Receiver<Event>) -> SessionState {
    match events.recv_timeout(TIMEOUT).expect("no event") {
        Event::Session(state) => state,
        other => panic!("unexpected event {other:?}"),
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
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let username = format!("ct-nobody-{}-{nanos}", process::id());
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
    assert_eq!(next(&events), SessionState::Connecting);
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
