// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Favorites, marking rooms read or unread, and leaving rooms.

use super::*;

fn room_action_event(
    room: &str,
    action: RoomAction,
    outcome: Result<(), RoomActionError>,
) -> Event {
    Event::RoomAction {
        room: room.to_owned().into(),
        action,
        outcome,
    }
}

fn favorite_room(room: Room) -> Room {
    Room {
        favorite: true,
        ..room
    }
}

fn favorite_subscription(subscription: rc_api::Subscription) -> rc_api::Subscription {
    rc_api::Subscription {
        favorite: true,
        ..subscription
    }
}

fn zed_subscription() -> rc_api::Subscription {
    subscription("3", "zed", rc_api::RoomType::Direct)
}

/// A harness logged in with these subscriptions, which list as `rooms` (in display order).
fn logged_in_with_rooms(
    fake: &Fake,
    subscriptions: Vec<rc_api::Subscription>,
    rooms: Vec<Room>,
) -> Harness {
    fake.script_login(None, Ok(()));
    fake.script_fetch(None, Ok(subscriptions));
    let harness = Harness::new(fake);
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(rooms),
    ]);
    harness
}

/// A harness logged in with `general` (a channel), `secret` (a private group) and `zed` (a direct
/// message), in that order.
fn logged_in_with_a_dm(fake: &Fake) -> Harness {
    logged_in_with_rooms(
        fake,
        vec![
            subscription("1", "general", rc_api::RoomType::Channel),
            subscription("2", "secret", rc_api::RoomType::Private),
            zed_subscription(),
        ],
        vec![
            room("1", "general", RoomKind::Channel),
            room("2", "secret", RoomKind::Private),
            room("3", "zed", RoomKind::Direct),
        ],
    )
}

/// A harness like [`logged_in_with_a_dm`] where `secret` is a favorite, and so comes first.
fn logged_in_with_a_favorite(fake: &Fake) -> Harness {
    logged_in_with_rooms(
        fake,
        vec![
            subscription("1", "general", rc_api::RoomType::Channel),
            favorite_subscription(subscription("2", "secret", rc_api::RoomType::Private)),
            zed_subscription(),
        ],
        vec![
            favorite_room(room("2", "secret", RoomKind::Private)),
            room("1", "general", RoomKind::Channel),
            room("3", "zed", RoomKind::Direct),
        ],
    )
}

fn favorite_call(room: &str, favorite: bool) -> RoomCall {
    RoomCall::SetFavorite {
        room: room.to_owned(),
        favorite,
    }
}

/// Makes sure the actor has taken in everything pushed so far, for pushes that have no output and
/// no room open: pushes are taken in order, so a room that changes in the list is the proof.
fn flush_pushed_rooms(fake: &Fake, harness: &Harness) {
    let mut busy = subscription("3", "zed", rc_api::RoomType::Direct);
    busy.unread = 99;
    assert!(fake.push(0, Pushed::Subscription(busy)));
    let Event::Rooms(changes) = harness.next_event() else {
        panic!("not a room change");
    };
    assert!(
        matches!(&changes[..], [RoomChange::Updated { room, .. }] if room.unread == 99),
        "{changes:?}"
    );
}

// ROOM-006
#[test]
fn a_new_favorite_moves_to_the_top_at_once_and_is_answered() {
    let fake = Fake::new();
    let harness = logged_in_with_a_dm(&fake);

    harness.client.set_favorite("2".to_owned().into(), true);

    harness.expect_events(vec![
        rooms_change(RoomChange::Moved {
            from: 1,
            to: 0,
            room: favorite_room(room("2", "secret", RoomKind::Private)),
        }),
        room_action_event("2", RoomAction::Favorite, Ok(())),
    ]);
    assert_eq!(fake.state().room_calls, [favorite_call("2", true)]);
}

// ROOM-006
#[test]
fn a_favorite_taken_back_moves_down_at_once_and_is_answered() {
    let fake = Fake::new();
    let harness = logged_in_with_a_favorite(&fake);

    harness.client.set_favorite("2".to_owned().into(), false);

    harness.expect_events(vec![
        rooms_change(RoomChange::Moved {
            from: 0,
            to: 1,
            room: room("2", "secret", RoomKind::Private),
        }),
        room_action_event("2", RoomAction::Unfavorite, Ok(())),
    ]);
    assert_eq!(fake.state().room_calls, [favorite_call("2", false)]);
}

// ROOM-006
#[test]
fn asking_for_a_favorite_that_is_one_already_asks_the_server_and_changes_nothing() {
    let fake = Fake::new();
    let harness = logged_in_with_a_favorite(&fake);

    harness.client.set_favorite("2".to_owned().into(), true);

    harness.expect_events(vec![room_action_event("2", RoomAction::Favorite, Ok(()))]);
    assert_eq!(fake.state().room_calls, [favorite_call("2", true)]);
    flush_pushed_rooms(&fake, &harness);
}

// ROOM-006
#[test]
fn a_favorite_that_stays_first_updates_in_place() {
    let fake = Fake::new();
    let harness = logged_in_with_a_dm(&fake);

    harness.client.set_favorite("1".to_owned().into(), true);

    harness.expect_events(vec![
        rooms_change(RoomChange::Updated {
            index: 0,
            room: favorite_room(room("1", "general", RoomKind::Channel)),
        }),
        room_action_event("1", RoomAction::Favorite, Ok(())),
    ]);
}

// ROOM-006
#[test]
fn the_push_of_a_favorite_after_the_answer_changes_nothing() {
    let fake = Fake::new();
    let harness = logged_in_with_a_dm(&fake);
    harness.client.set_favorite("2".to_owned().into(), true);
    harness.expect_events(vec![
        rooms_change(RoomChange::Moved {
            from: 1,
            to: 0,
            room: favorite_room(room("2", "secret", RoomKind::Private)),
        }),
        room_action_event("2", RoomAction::Favorite, Ok(())),
    ]);

    assert!(fake.push(
        0,
        Pushed::Subscription(favorite_subscription(subscription(
            "2",
            "secret",
            rc_api::RoomType::Private
        )))
    ));

    flush_pushed_rooms(&fake, &harness);
}

// ROOM-006
#[test]
fn the_push_of_a_favorite_before_the_answer_leaves_nothing_for_the_answer_to_change() {
    let fake = Fake::new();
    let harness = logged_in_with_a_dm(&fake);
    let gate = Arc::new(Notify::new());
    fake.script_room_action(Some(Arc::clone(&gate)), Ok(()));
    harness.client.set_favorite("2".to_owned().into(), true);
    wait_until(|| (!fake.state().room_calls.is_empty()).then_some(()));

    let moved = RoomChange::Moved {
        from: 1,
        to: 0,
        room: favorite_room(room("2", "secret", RoomKind::Private)),
    };
    assert!(fake.push(
        0,
        Pushed::Subscription(favorite_subscription(subscription(
            "2",
            "secret",
            rc_api::RoomType::Private
        )))
    ));
    harness.expect_events(vec![rooms_change(moved)]);
    gate.notify_one();

    harness.expect_events(vec![room_action_event("2", RoomAction::Favorite, Ok(()))]);
    flush_pushed_rooms(&fake, &harness);
}

// ROOM-006
#[test]
fn a_favorite_set_elsewhere_arrives_as_a_push_and_moves_the_room() {
    let fake = Fake::new();
    let harness = logged_in_with_a_dm(&fake);

    assert!(fake.push(
        0,
        Pushed::Subscription(favorite_subscription(zed_subscription()))
    ));

    harness.expect_events(vec![rooms_change(RoomChange::Moved {
        from: 2,
        to: 0,
        room: favorite_room(room("3", "zed", RoomKind::Direct)),
    })]);
}

// ROOM-006
#[test]
fn activity_in_another_room_does_not_lift_it_above_the_favorites() {
    let fake = Fake::new();
    let harness = logged_in_with_a_favorite(&fake);

    assert!(fake.push(
        0,
        Pushed::Activity {
            room: "3".to_owned().into(),
            at: at(50),
        }
    ));

    // Above the other rooms, below the favorite.
    harness.expect_events(vec![rooms_change(RoomChange::Moved {
        from: 2,
        to: 1,
        room: Room {
            last_activity: at(50),
            ..room("3", "zed", RoomKind::Direct)
        },
    })]);
}

// ROOM-006
#[test]
fn a_refused_favorite_is_reported_and_changes_nothing() {
    let fake = Fake::new();
    let harness = logged_in_with_a_dm(&fake);
    fake.script_room_action(None, Err(refused()));
    fake.script_room_action(None, Err(ApiError::Unreachable("refused".into())));

    harness.client.set_favorite("2".to_owned().into(), true);
    harness.expect_events(vec![room_action_event(
        "2",
        RoomAction::Favorite,
        Err(RoomActionError::Server(refused().to_string())),
    )]);
    harness.client.set_favorite("2".to_owned().into(), true);
    harness.expect_events(vec![room_action_event(
        "2",
        RoomAction::Favorite,
        Err(RoomActionError::Unreachable),
    )]);
    flush_pushed_rooms(&fake, &harness);
}

// ROOM-007
#[test]
fn marking_a_room_read_clears_its_counts_at_once_and_is_answered() {
    let fake = Fake::new();
    let harness = logged_in_with_unread(&fake);

    harness.client.mark_read("1".to_owned().into());

    harness.expect_events(vec![
        rooms_change(RoomChange::Updated {
            index: 0,
            room: room("1", "general", RoomKind::Channel),
        }),
        room_action_event("1", RoomAction::MarkRead, Ok(())),
    ]);
    assert_eq!(fake.state().mark_read_calls, ["1"]);
}

// ROOM-007
#[test]
fn the_push_of_a_mark_read_after_the_answer_changes_nothing() {
    let fake = Fake::new();
    let harness = logged_in_with_unread(&fake);
    harness.client.mark_read("1".to_owned().into());
    harness.expect_events(vec![
        rooms_change(RoomChange::Updated {
            index: 0,
            room: room("1", "general", RoomKind::Channel),
        }),
        room_action_event("1", RoomAction::MarkRead, Ok(())),
    ]);

    assert!(fake.push(0, Pushed::Subscription(general_with(0))));

    // The room is read already; the unread secret is the one to change.
    let mut busy = unread_secret();
    busy.unread = 9;
    assert!(fake.push(0, Pushed::Subscription(busy.clone())));
    expect_rooms_updated(&harness, 1, room_from(busy).unwrap());
}

// ROOM-007
#[test]
fn a_refused_mark_read_is_reported_and_changes_nothing() {
    let fake = Fake::new();
    let harness = logged_in_with_unread(&fake);
    fake.script_mark_read(Err(refused()));

    harness.client.mark_read("1".to_owned().into());

    harness.expect_events(vec![room_action_event(
        "1",
        RoomAction::MarkRead,
        Err(RoomActionError::Server(refused().to_string())),
    )]);
}

// ROOM-007
#[test]
fn marking_a_room_unread_shows_one_unread_message_and_the_alert_at_once() {
    let fake = Fake::new();
    let harness = logged_in_with_a_dm(&fake);

    harness.client.mark_unread("2".to_owned().into());

    let mut unread = room("2", "secret", RoomKind::Private);
    unread.unread = 1;
    unread.alert = true;
    harness.expect_events(vec![
        rooms_change(RoomChange::Updated {
            index: 1,
            room: unread,
        }),
        room_action_event("2", RoomAction::MarkUnread, Ok(())),
    ]);
    assert_eq!(
        fake.state().room_calls,
        [RoomCall::MarkUnread { room: "2".into() }]
    );
}

// ROOM-007
#[test]
fn the_push_of_a_mark_unread_before_or_after_the_answer_changes_nothing() {
    let fake = Fake::new();
    let harness = logged_in_with_a_dm(&fake);
    let gate = Arc::new(Notify::new());
    fake.script_room_action(Some(Arc::clone(&gate)), Ok(()));
    harness.client.mark_unread("2".to_owned().into());
    wait_until(|| (!fake.state().room_calls.is_empty()).then_some(()));

    // What the server pushes: one unread message and the alert.
    let mut pushed = subscription("2", "secret", rc_api::RoomType::Private);
    pushed.unread = 1;
    pushed.alert = true;
    let mut unread = room("2", "secret", RoomKind::Private);
    unread.unread = 1;
    unread.alert = true;
    assert!(fake.push(0, Pushed::Subscription(pushed.clone())));
    expect_rooms_updated(&harness, 1, unread);
    gate.notify_one();
    harness.expect_events(vec![room_action_event("2", RoomAction::MarkUnread, Ok(()))]);

    assert!(fake.push(0, Pushed::Subscription(pushed)));
    flush_pushed_rooms(&fake, &harness);
}

// ROOM-007
#[test]
fn marking_unread_keeps_the_mentions_and_a_room_that_shows_unread_already_does_not_change() {
    let fake = Fake::new();
    let mut mentioned = subscription("1", "general", rc_api::RoomType::Channel);
    mentioned.user_mentions = 2;
    let harness = logged_in_with_rooms(
        &fake,
        vec![mentioned.clone(), unread_secret()],
        vec![
            room_from(mentioned).unwrap(),
            room_from(unread_secret()).unwrap(),
        ],
    );

    harness.client.mark_unread("1".to_owned().into());
    let mut general = room("1", "general", RoomKind::Channel);
    general.user_mentions = 2;
    general.unread = 1;
    general.alert = true;
    harness.expect_events(vec![
        rooms_change(RoomChange::Updated {
            index: 0,
            room: general,
        }),
        room_action_event("1", RoomAction::MarkUnread, Ok(())),
    ]);

    // Two unread messages stay two, and the room needs no change.
    harness.client.mark_unread("2".to_owned().into());
    harness.expect_events(vec![room_action_event("2", RoomAction::MarkUnread, Ok(()))]);
}

// ROOM-007
#[test]
fn a_room_opened_while_it_was_being_marked_unread_stays_read() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    let gate = Arc::new(Notify::new());
    fake.script_room_action(Some(Arc::clone(&gate)), Ok(()));
    harness.client.mark_unread("1".to_owned().into());
    wait_until(|| (!fake.state().room_calls.is_empty()).then_some(()));
    open_first_room(&fake, &harness, false);

    gate.notify_one();

    harness.expect_events(vec![room_action_event("1", RoomAction::MarkUnread, Ok(()))]);
    // The server is told the room is read, as for any change of the open room.
    assert_eq!(fake.wait_for_mark_read_calls(2), ["1", "1"]);
}

// ROOM-007
#[test]
fn the_open_room_is_not_marked_unread() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);

    harness.client.mark_unread("1".to_owned().into());

    harness.expect_events(vec![room_action_event(
        "1",
        RoomAction::MarkUnread,
        Err(RoomActionError::NotAllowed(RoomAction::MarkUnread)),
    )]);
    assert!(fake.state().room_calls.is_empty());
    // Another room can be.
    harness.client.mark_unread("2".to_owned().into());
    let Event::Rooms(_) = harness.next_event() else {
        panic!("not a room change");
    };
}

// ROOM-007
#[test]
fn leaving_a_room_removes_it_at_once_and_is_answered() {
    let fake = Fake::new();
    let harness = logged_in_with_a_dm(&fake);

    harness.client.leave_room("2".to_owned().into());

    harness.expect_events(vec![
        rooms_change(RoomChange::Removed { index: 1 }),
        room_action_event("2", RoomAction::Leave, Ok(())),
    ]);
    assert_eq!(
        fake.state().room_calls,
        [RoomCall::Leave {
            room: "2".into(),
            kind: RoomKind::Private
        }]
    );
    // The push that follows has nothing left to remove.
    assert!(fake.push(0, removed("2")));
    flush_pushed_rooms(&fake, &harness);
}

// ROOM-007
#[test]
fn the_removal_pushed_before_the_answer_leaves_nothing_for_the_answer_to_change() {
    let fake = Fake::new();
    let harness = logged_in_with_a_dm(&fake);
    let gate = Arc::new(Notify::new());
    fake.script_room_action(Some(Arc::clone(&gate)), Ok(()));
    harness.client.leave_room("1".to_owned().into());
    wait_until(|| (!fake.state().room_calls.is_empty()).then_some(()));

    assert!(fake.push(0, removed("1")));
    harness.expect_events(vec![rooms_change(RoomChange::Removed { index: 0 })]);
    gate.notify_one();

    harness.expect_events(vec![room_action_event("1", RoomAction::Leave, Ok(()))]);
    flush_pushed_rooms_after_removal(&fake, &harness);
}

/// Like [`flush_pushed_rooms`], for a list from which `general` is gone: `zed` is at index 1.
fn flush_pushed_rooms_after_removal(fake: &Fake, harness: &Harness) {
    let mut busy = subscription("3", "zed", rc_api::RoomType::Direct);
    busy.unread = 99;
    assert!(fake.push(0, Pushed::Subscription(busy)));
    let Event::Rooms(changes) = harness.next_event() else {
        panic!("not a room change");
    };
    assert!(
        matches!(&changes[..], [RoomChange::Updated { index: 1, room }] if room.unread == 99),
        "{changes:?}"
    );
}

// ROOM-007
#[test]
fn leaving_the_open_room_closes_it() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);

    harness.client.leave_room("1".to_owned().into());

    let mut events = vec![rooms_change(RoomChange::Removed { index: 0 })];
    events.extend(closed_events());
    events.push(room_action_event("1", RoomAction::Leave, Ok(())));
    harness.expect_events(events);
}

// ROOM-007
#[test]
fn leaving_a_room_the_server_refuses_to_let_go_is_reported_and_keeps_the_room() {
    let fake = Fake::new();
    let harness = logged_in_with_a_dm(&fake);
    let last_owner = ApiError::Server {
        status: 400,
        message: "You are the last owner. Please set new owner before leaving the room. \
                  [error-you-are-last-owner]"
            .to_owned(),
    };
    fake.script_room_action(None, Err(last_owner.clone()));

    harness.client.leave_room("2".to_owned().into());

    harness.expect_events(vec![room_action_event(
        "2",
        RoomAction::Leave,
        Err(RoomActionError::Server(last_owner.to_string())),
    )]);
    flush_pushed_rooms(&fake, &harness);
}

// ROOM-007
#[test]
fn a_direct_message_is_not_left_and_the_server_is_not_asked() {
    let fake = Fake::new();
    let harness = logged_in_with_a_dm(&fake);

    harness.client.leave_room("3".to_owned().into());

    harness.expect_events(vec![room_action_event(
        "3",
        RoomAction::Leave,
        Err(RoomActionError::NotAllowed(RoomAction::Leave)),
    )]);
    assert!(fake.state().room_calls.is_empty());
}

// ROOM-006, ROOM-007
#[test]
fn a_room_that_is_not_in_the_list_gets_no_command_and_the_server_is_not_asked() {
    let fake = Fake::new();
    let harness = logged_in_with_a_dm(&fake);
    let client = &harness.client;

    client.set_favorite("nope".to_owned().into(), true);
    client.set_favorite("nope".to_owned().into(), false);
    client.mark_read("nope".to_owned().into());
    client.mark_unread("nope".to_owned().into());
    client.leave_room("nope".to_owned().into());

    harness.expect_events(
        [
            RoomAction::Favorite,
            RoomAction::Unfavorite,
            RoomAction::MarkRead,
            RoomAction::MarkUnread,
            RoomAction::Leave,
        ]
        .map(|action| room_action_event("nope", action, Err(RoomActionError::NotAllowed(action))))
        .to_vec(),
    );
    assert!(fake.state().room_calls.is_empty());
    assert!(fake.state().mark_read_calls.is_empty());
}

// ROOM-006, ROOM-007
#[test]
fn room_commands_are_ignored_when_logged_out() {
    let fake = Fake::new();
    let harness = Harness::new(&fake);

    harness.client.set_favorite("1".to_owned().into(), true);
    harness.client.mark_read("1".to_owned().into());
    harness.client.mark_unread("1".to_owned().into());
    harness.client.leave_room("1".to_owned().into());

    // The first event after this is the login's.
    fake.script_login(None, Ok(()));
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
    ]);
    assert!(fake.state().room_calls.is_empty());
    assert!(fake.state().mark_read_calls.is_empty());
}

// ROOM-006, ROOM-007
#[test]
fn a_room_command_answered_after_logout_is_dropped() {
    let fake = Fake::new();
    let harness = logged_in_with_a_dm(&fake);
    let gate = Arc::new(Notify::new());
    fake.script_room_action(Some(Arc::clone(&gate)), Ok(()));
    harness.client.leave_room("1".to_owned().into());
    wait_until(|| (!fake.state().room_calls.is_empty()).then_some(()));

    harness.client.logout();
    harness.expect_events(vec![session_event(logged_out(None)), rooms_event(vec![])]);
    gate.notify_one();
    fake.wait_for_room_actions_returned(1);

    fake.script_login(None, Ok(()));
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
    ]);
}

// ROOM-006, ROOM-007
#[test]
fn a_dead_token_on_a_room_command_expires_the_session() {
    let fake = Fake::new();
    let harness = logged_in_with_a_dm(&fake);
    fake.script_room_action(None, Err(ApiError::Unauthorized));

    harness.client.set_favorite("2".to_owned().into(), true);

    harness.expect_events(vec![
        session_event(logged_out(Some(LoginFailure::SessionExpired))),
        rooms_event(vec![]),
    ]);
}
