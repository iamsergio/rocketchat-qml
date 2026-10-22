// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The room list a session fetches.

use super::*;

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
    harness.logout();
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
    harness.logout();
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

    harness.logout();
    harness.expect_events(vec![session_event(logged_out(None))]);
    gate.notify_one();

    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![room("new", "new", RoomKind::Channel)]),
    ]);
}
