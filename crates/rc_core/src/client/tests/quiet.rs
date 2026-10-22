// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Direct messages that had no activity for a long time are left out of the room list. The fake
//! clock makes them quiet.

use std::sync::mpsc::RecvTimeoutError;

use super::*;

/// Seconds in a day, for the times of the rooms.
const DAY: i64 = 24 * 3600;

/// A direct message with `peer` as its name, whose last activity was at `secs`.
fn dm_room(id: &str, peer: &str, secs: i64) -> Room {
    Room {
        last_activity: at(secs),
        ..room(id, peer, RoomKind::Direct)
    }
}

/// The subscription of a direct message with a last message at `secs`.
fn dm_subscription(id: &str, peer: &str, secs: i64) -> rc_api::Subscription {
    rc_api::Subscription {
        last_message: Some(at(secs)),
        ..subscription(id, peer, rc_api::RoomType::Direct)
    }
}

/// The room changes of the next `Rooms` event, skipping the other events.
fn next_rooms(harness: &Harness) -> Vec<RoomChange> {
    loop {
        if let Event::Rooms(changes) = harness.next_event() {
            return changes;
        }
    }
}

/// The names of the rooms.
fn names_of(rooms: &[Room]) -> Vec<&str> {
    rooms.iter().map(|room| room.name.as_str()).collect()
}

/// Logs in at day 30 to a server that lists these subscriptions, and takes the first room list.
fn logged_in_at_day_30(
    fake: &Fake,
    harness: &Harness,
    subscriptions: Vec<rc_api::Subscription>,
) -> Vec<Room> {
    harness.clock.set(at(30 * DAY));
    fake.script_login(None, Ok(()));
    fake.script_fetch(None, Ok(subscriptions));
    harness.login("http://h", "alice");
    harness.expect(&[SessionState::Connecting, logged_in("http://h", "alice")]);
    let changes = next_rooms(harness);
    let [RoomChange::Reset(rooms)] = &changes[..] else {
        panic!("not a reset: {changes:?}");
    };
    rooms.clone()
}

// ROOM-008
#[test]
fn a_direct_message_that_was_quiet_for_longer_than_the_limit_is_left_out_at_login() {
    let fake = Fake::new();
    let harness = Harness::new(&fake);
    let rooms = logged_in_at_day_30(
        &fake,
        &harness,
        vec![
            dm_subscription("1", "quiet", 0),
            dm_subscription("2", "recent", 29 * DAY),
            subscription("3", "general", rc_api::RoomType::Channel),
        ],
    );
    assert_eq!(
        rooms,
        [
            dm_room("2", "recent", 29 * DAY),
            room("3", "general", RoomKind::Channel)
        ]
    );
}

// ROOM-008
#[test]
fn the_time_passing_hides_a_shown_direct_message_with_no_command_sent() {
    let fake = Fake::new();
    let harness = Harness::new(&fake);
    logged_in_at_day_30(
        &fake,
        &harness,
        vec![
            dm_subscription("1", "bob", 29 * DAY),
            subscription("2", "general", rc_api::RoomType::Channel),
        ],
    );
    // Ten days after its last message it is still shown, a second later it is not.
    harness.clock.advance(Duration::from_secs(9 * DAY as u64));
    assert_eq!(
        harness.recv_timeout(Duration::from_millis(200)),
        Err(RecvTimeoutError::Timeout)
    );
    harness.clock.advance(Duration::from_secs(1));
    assert_eq!(next_rooms(&harness), [RoomChange::Removed { index: 0 }]);
}

// ROOM-008
#[test]
fn a_message_brings_a_hidden_direct_message_back_and_its_avatar_is_wanted_then() {
    let fake = Fake::new();
    let harness = Harness::with_cache(&fake, FakeCache::default());
    let rooms = logged_in_at_day_30(
        &fake,
        &harness,
        vec![
            rc_api::Subscription {
                last_message: Some(at(0)),
                ..avatar_subscription("1", "bob", rc_api::RoomType::Direct)
            },
            avatar_subscription("2", "general", rc_api::RoomType::Channel),
        ],
    );
    assert_eq!(names_of(&rooms), ["general"]);
    // The avatar of the room that is shown, and none of the hidden one.
    assert_eq!(
        fake.wait_for_avatar_calls(1),
        [AvatarOwner::Room("2".to_owned().into())]
    );

    assert!(fake.push(
        0,
        Pushed::RoomChanged {
            room: "1".to_owned().into(),
            last_message: Some(at(30 * DAY)),
            topic: None,
        }
    ));

    assert_eq!(
        next_rooms(&harness),
        [RoomChange::Inserted {
            index: 0,
            room: Room {
                avatar_etag: Some("etag".to_owned()),
                ..dm_room("1", "bob", 30 * DAY)
            }
        }]
    );
    assert_eq!(
        fake.wait_for_avatar_calls(2),
        [
            AvatarOwner::Room("2".to_owned().into()),
            AvatarOwner::User("bob".to_owned())
        ]
    );
}

// ROOM-008
#[test]
fn marking_a_quiet_direct_message_read_from_the_menu_hides_it() {
    let fake = Fake::new();
    let harness = Harness::new(&fake);
    let unread = rc_api::Subscription {
        unread: 3,
        ..dm_subscription("1", "bob", 0)
    };
    let rooms = logged_in_at_day_30(
        &fake,
        &harness,
        vec![
            unread,
            subscription("2", "general", rc_api::RoomType::Channel),
        ],
    );
    assert_eq!(names_of(&rooms), ["bob", "general"]);

    harness.account().mark_read("1".to_owned().into());

    assert_eq!(next_rooms(&harness), [RoomChange::Removed { index: 0 }]);
}

// ROOM-008
#[test]
fn leaving_a_quiet_open_direct_message_by_opening_another_room_hides_it() {
    let fake = Fake::new();
    let harness = Harness::new(&fake);
    let unread = rc_api::Subscription {
        unread: 3,
        ..dm_subscription("1", "bob", 0)
    };
    logged_in_at_day_30(
        &fake,
        &harness,
        vec![
            unread,
            subscription("2", "general", rc_api::RoomType::Channel),
        ],
    );
    // Opened, it is read, and stays because it is open.
    harness.account().open_room("1".to_owned().into());
    let read = dm_room("1", "bob", 0);
    assert_eq!(
        next_rooms(&harness),
        [RoomChange::Updated {
            index: 0,
            room: read
        }]
    );

    harness.account().open_room("2".to_owned().into());

    assert_eq!(next_rooms(&harness), [RoomChange::Removed { index: 0 }]);
}

// ROOM-008
#[test]
fn logging_out_forgets_the_hidden_rooms_too() {
    let fake = Fake::new();
    let harness = Harness::new(&fake);
    let rooms = logged_in_at_day_30(&fake, &harness, vec![dm_subscription("1", "bob", 0)]);
    assert_eq!(rooms, []);

    harness.logout();

    harness.expect_events(vec![session_event(logged_out(None)), rooms_event(vec![])]);
}

// ROOM-008
#[test]
fn hiding_a_room_does_not_change_the_unread_totals() {
    let fake = Fake::new();
    let harness = Harness::new(&fake);
    logged_in_at_day_30(
        &fake,
        &harness,
        vec![
            dm_subscription("1", "bob", 29 * DAY),
            rc_api::Subscription {
                unread: 2,
                ..subscription("2", "general", rc_api::RoomType::Channel)
            },
        ],
    );
    harness.clock.advance(Duration::from_secs(11 * DAY as u64));
    assert_eq!(next_rooms(&harness), [RoomChange::Removed { index: 0 }]);
    // Nothing else comes, in particular no new totals.
    while let Ok(event) = harness.recv_timeout(Duration::from_millis(300)) {
        assert!(!matches!(event, Event::Unread(_)), "{event:?}");
    }
}
