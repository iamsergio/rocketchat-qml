// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Opening a room, and loading its history.

use super::*;

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
