// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The unread counts, and marking the open room read.

use super::*;

// ROOM-004
#[test]
fn opening_a_room_marks_it_read_on_the_server_and_in_the_list() {
    let fake = Fake::new();
    let harness = logged_in_with_unread(&fake);

    harness.client.open_room("1".to_owned().into());

    harness.expect_events(vec![reset_event(Some("1"), Vec::new())]);
    expect_rooms_updated(&harness, 0, room("1", "general", RoomKind::Channel));
    assert_eq!(fake.wait_for_mark_read_calls(1), ["1"]);
}

// ROOM-004
#[test]
fn opening_a_room_that_shows_read_still_tells_the_server() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    harness.client.open_room("1".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("1"), Vec::new()),
        status("1", true, false, false, None),
    ]);
    assert_eq!(fake.wait_for_mark_read_calls(1), ["1"]);
}

// ROOM-004
#[test]
fn only_the_opened_room_is_marked_read() {
    let fake = Fake::new();
    let harness = logged_in_with_unread(&fake);
    harness.client.open_room("1".to_owned().into());
    fake.wait_for_mark_read_calls(1);
    harness.client.open_room("2".to_owned().into());

    assert_eq!(fake.wait_for_mark_read_calls(2), ["1", "2"]);
    let mut changes_seen = 0;
    while let Ok(event) = harness.events.recv_timeout(Duration::from_millis(300)) {
        if let Event::Rooms(changes) = event {
            changes_seen += changes.len();
        }
    }
    // Room 1 cleared when it opened, then room 2.
    assert_eq!(changes_seen, 2);
}

// ROOM-004
#[test]
fn a_message_in_the_open_room_does_not_raise_its_unread_count() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, true);
    fake.wait_for_mark_read_calls(1);

    // The server counts it as unread, as it doesn't know the room is on screen.
    assert!(fake.push(0, pushed_message("1", "c", 3)));
    assert!(fake.push(0, Pushed::Subscription(general_with(1))));
    // Another room's count does rise.
    let mut secret = subscription("2", "secret", rc_api::RoomType::Private);
    secret.unread = 4;
    assert!(fake.push(0, Pushed::Subscription(secret)));

    let mut secret_room = room("2", "secret", RoomKind::Private);
    secret_room.unread = 4;
    harness.expect_events(vec![
        appended("c", 3),
        rooms_change(RoomChange::Updated {
            index: 1,
            room: secret_room,
        }),
    ]);
    // And the server is told again, once.
    assert_eq!(fake.wait_for_mark_read_calls(2), ["1", "1"]);
}

// ROOM-004
#[test]
fn a_subscription_change_of_the_open_room_that_shows_read_is_not_marked_again() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, true);
    fake.wait_for_mark_read_calls(1);

    let mut renamed = subscription("1", "main", rc_api::RoomType::Channel);
    renamed.display_name = Some("main".to_owned());
    assert!(fake.push(0, Pushed::Subscription(renamed)));
    harness.expect_events(vec![rooms_change(RoomChange::Updated {
        index: 0,
        room: room("1", "main", RoomKind::Channel),
    })]);
    assert_eq!(fake.state().mark_read_calls, ["1"]);
}

// ROOM-004
#[test]
fn a_closed_room_counts_new_messages_again() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, true);
    // Leaving the room closes it.
    assert!(fake.push(0, removed("1")));
    harness.expect_events(vec![
        rooms_change(RoomChange::Removed { index: 0 }),
        Event::Timeline(vec![TimelineChange::Reset {
            room: None,
            messages: Vec::new(),
        }]),
        Event::TimelineStatus(TimelineStatus::default()),
    ]);
    assert!(fake.push(0, Pushed::Subscription(general_with(2))));
    harness.expect_events(vec![rooms_change(RoomChange::Inserted {
        index: 0,
        room: general_room(2),
    })]);
    assert_eq!(fake.state().mark_read_calls, ["1"]);
}

// ROOM-004
#[test]
fn a_snapshot_after_reconnecting_does_not_show_the_open_room_unread() {
    let fake = Fake::new();
    let harness = logged_in_harness_with(&fake, fast());
    open_first_room(&fake, &harness, true);
    fake.wait_for_mark_read_calls(1);
    // A message came in while away: the snapshot counts it.
    fake.script_fetch(
        None,
        Ok(vec![
            general_with(1),
            subscription("2", "secret", rc_api::RoomType::Private),
        ]),
    );
    fake.end_connection(0);

    assert_eq!(fake.wait_for_mark_read_calls(2), ["1", "1"]);
    // The list needs no change: room 1 shows read already.
    while let Ok(event) = harness.events.recv_timeout(Duration::from_millis(300)) {
        assert!(!matches!(event, Event::Rooms(_)), "{event:?}");
    }
}

// ROOM-004
#[test]
fn a_failed_mark_read_is_not_fatal() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    fake.script_mark_read(Err(ApiError::Server {
        status: 400,
        message: "error-invalid-subscription".to_owned(),
    }));
    open_first_room(&fake, &harness, true);
    fake.wait_for_mark_read_calls(1);

    // Still logged in, and the room still gets live messages.
    assert!(fake.push(0, pushed_message("1", "c", 3)));
    harness.expect_events(vec![appended("c", 3)]);
}

// ROOM-004
#[test]
fn an_expired_token_in_mark_read_ends_the_session() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    let gate = Arc::new(Notify::new());
    fake.script_history(Some(Arc::clone(&gate)), page(Vec::new(), true));
    fake.script_mark_read(Err(ApiError::Unauthorized));

    harness.client.open_room("1".to_owned().into());

    harness.expect_events(vec![
        reset_event(Some("1"), Vec::new()),
        status("1", true, false, false, None),
        session_event(logged_out(Some(LoginFailure::SessionExpired))),
        rooms_event(vec![]),
    ]);
    harness.expect_events(closed_events());
}
