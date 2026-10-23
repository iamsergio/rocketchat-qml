// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Ordering the room list by last activity.

use super::*;

/// The subscription of room `id`, as the server lists it with a last message at `secs`.
fn active_subscription(id: &str, name: &str, secs: i64) -> rc_api::Subscription {
    let mut sub = subscription(id, name, rc_api::RoomType::Channel);
    sub.last_message = Some(at(secs));
    sub
}

fn active_room(id: &str, name: &str, secs: i64) -> Room {
    Room {
        last_activity: at(secs),
        ..room(id, name, RoomKind::Channel)
    }
}

/// A harness logged in with rooms `1` (`alpha`, busy at 10 s), `2` (`beta`, 30 s) and `3`
/// (`gamma`, 20 s), listed as `2`, `3`, `1`.
fn logged_in_by_activity(fake: &Fake) -> Harness {
    fake.script_login(None, Ok(()));
    fake.script_fetch(
        None,
        Ok(vec![
            active_subscription("1", "alpha", 10),
            active_subscription("2", "beta", 30),
            active_subscription("3", "gamma", 20),
        ]),
    );
    let harness = Harness::new(fake);
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![
            active_room("2", "beta", 30),
            active_room("3", "gamma", 20),
            active_room("1", "alpha", 10),
        ]),
    ]);
    harness
}

// ROOM-005
#[test]
fn login_lists_the_rooms_by_last_activity_falling_back_to_the_join_time() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    let mut joined = subscription("4", "joined", rc_api::RoomType::Channel);
    joined.created = Some(at(25));
    fake.script_fetch(
        None,
        Ok(vec![
            subscription("5", "never", rc_api::RoomType::Channel),
            active_subscription("1", "alpha", 10),
            joined,
            active_subscription("2", "beta", 30),
        ]),
    );
    let harness = Harness::new(&fake);
    harness.login("http://h", "alice");
    let mut joined = room("4", "joined", RoomKind::Channel);
    joined.last_activity = at(25);
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![
            active_room("2", "beta", 30),
            joined,
            active_room("1", "alpha", 10),
            room("5", "never", RoomKind::Channel),
        ]),
    ]);
}

// ROOM-005
#[test]
fn activity_moves_the_room_to_the_front() {
    let fake = Fake::new();
    let harness = logged_in_by_activity(&fake);

    assert!(fake.push(
        0,
        Pushed::RoomChanged {
            room: "1".to_owned().into(),
            last_message: Some(at(40)),
            topic: None,
        }
    ));

    harness.expect_events(vec![rooms_change(RoomChange::Moved {
        from: 2,
        to: 0,
        item: active_room("1", "alpha", 40),
    })]);
}

// ROOM-005
#[test]
fn a_message_raises_the_unread_count_and_then_moves_the_room() {
    let fake = Fake::new();
    let harness = logged_in_by_activity(&fake);
    let mut unread = active_subscription("3", "gamma", 20);
    unread.unread = 1;
    unread.last_message = None;

    assert!(fake.push(0, Pushed::Subscription(unread)));
    assert!(fake.push(
        0,
        Pushed::RoomChanged {
            room: "3".to_owned().into(),
            last_message: Some(at(50)),
            topic: None,
        }
    ));

    let mut updated = active_room("3", "gamma", 20);
    updated.unread = 1;
    let mut moved = active_room("3", "gamma", 50);
    moved.unread = 1;
    harness.expect_events(vec![
        rooms_change(RoomChange::Updated {
            index: 1,
            item: updated,
        }),
        rooms_change(RoomChange::Moved {
            from: 1,
            to: 0,
            item: moved,
        }),
    ]);
}

// ROOM-005
#[test]
fn a_subscription_change_keeps_the_known_activity() {
    let fake = Fake::new();
    let harness = logged_in_by_activity(&fake);
    // What the server pushes has no last message and a join time long before it.
    let mut renamed = subscription("1", "omega", rc_api::RoomType::Channel);
    renamed.created = Some(at(1));

    assert!(fake.push(0, Pushed::Subscription(renamed)));

    harness.expect_events(vec![rooms_change(RoomChange::Updated {
        index: 2,
        item: active_room("1", "omega", 10),
    })]);
}

// ROOM-005
#[test]
fn old_or_unknown_activity_changes_nothing() {
    let fake = Fake::new();
    let harness = logged_in_by_activity(&fake);
    for (room, secs) in [("2", 30), ("2", 5), ("zzz", 99)] {
        assert!(fake.push(
            0,
            Pushed::RoomChanged {
                room: room.to_owned().into(),
                last_message: Some(at(secs)),
                topic: None,
            }
        ));
    }
    // Something that does show, as the end of the line.
    assert!(fake.push(0, removed("2")));

    harness.expect_events(vec![rooms_change(RoomChange::Removed { index: 0 })]);
}

// ROOM-005
#[test]
fn a_room_change_without_a_last_message_is_no_activity() {
    let fake = Fake::new();
    let harness = logged_in_by_activity(&fake);
    assert!(fake.push(
        0,
        Pushed::RoomChanged {
            room: "1".to_owned().into(),
            last_message: None,
            topic: None,
        }
    ));
    // Something that does show, as the end of the line.
    assert!(fake.push(0, removed("2")));

    harness.expect_events(vec![rooms_change(RoomChange::Removed { index: 0 })]);
}

// ROOM-009
#[test]
fn a_push_sets_changes_and_clears_the_topic_of_a_room() {
    let fake = Fake::new();
    let harness = logged_in_by_activity(&fake);
    let topic_push = |topic: Option<&str>| Pushed::RoomChanged {
        room: "1".to_owned().into(),
        last_message: None,
        topic: topic.map(str::to_owned),
    };
    let with_topic = |topic: &str| {
        rooms_change(RoomChange::Updated {
            index: 2,
            item: Room {
                topic: topic.to_owned(),
                ..active_room("1", "alpha", 10)
            },
        })
    };

    assert!(fake.push(0, topic_push(Some("Plans"))));
    assert!(fake.push(0, topic_push(Some("Plans"))));
    assert!(fake.push(0, topic_push(Some("More plans"))));
    // A push that doesn't say changes nothing.
    assert!(fake.push(0, topic_push(None)));
    assert!(fake.push(0, topic_push(Some(""))));

    harness.expect_events(vec![
        with_topic("Plans"),
        with_topic("More plans"),
        with_topic(""),
    ]);
}

// ROOM-009
#[test]
fn a_push_with_a_message_and_a_topic_changes_both_at_once() {
    let fake = Fake::new();
    let harness = logged_in_by_activity(&fake);

    assert!(fake.push(
        0,
        Pushed::RoomChanged {
            room: "1".to_owned().into(),
            last_message: Some(at(40)),
            topic: Some("Plans".to_owned()),
        }
    ));

    // In one event, in the order of the changes.
    harness.expect_events(vec![Event::Rooms(vec![
        RoomChange::Moved {
            from: 2,
            to: 0,
            item: active_room("1", "alpha", 40),
        },
        RoomChange::Updated {
            index: 0,
            item: Room {
                topic: "Plans".to_owned(),
                ..active_room("1", "alpha", 40)
            },
        },
    ])]);
}

// ROOM-009
#[test]
fn the_topic_of_a_room_survives_a_subscription_push() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    let mut listed = active_subscription("1", "alpha", 10);
    listed.topic = "Plans".to_owned();
    fake.script_fetch(None, Ok(vec![listed.clone()]));
    let harness = Harness::new(&fake);
    harness.login("http://h", "alice");
    let alpha = Room {
        topic: "Plans".to_owned(),
        ..active_room("1", "alpha", 10)
    };
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![alpha.clone()]),
    ]);

    // The subscription doesn't carry the topic.
    listed.topic = String::new();
    listed.unread = 1;
    assert!(fake.push(0, Pushed::Subscription(listed)));

    harness.expect_events(vec![rooms_change(RoomChange::Updated {
        index: 0,
        item: Room { unread: 1, ..alpha },
    })]);
}

// ROOM-005
#[test]
fn a_new_room_comes_in_at_its_activity() {
    let fake = Fake::new();
    let harness = logged_in_by_activity(&fake);
    let mut joined = subscription("9", "fresh", rc_api::RoomType::Channel);
    joined.created = Some(at(25));

    assert!(fake.push(0, Pushed::Subscription(joined)));

    let mut fresh = room("9", "fresh", RoomKind::Channel);
    fresh.last_activity = at(25);
    harness.expect_events(vec![rooms_change(RoomChange::Inserted {
        index: 1,
        item: fresh,
    })]);
}

// ROOM-005
#[test]
fn a_snapshot_after_reconnecting_moves_the_rooms_that_were_busy() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    fake.script_fetch(
        None,
        Ok(vec![
            active_subscription("1", "alpha", 10),
            active_subscription("2", "beta", 30),
        ]),
    );
    let harness = Harness::with_backoff(&fake, MemoryStore::default(), fast());
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![
            active_room("2", "beta", 30),
            active_room("1", "alpha", 10),
        ]),
    ]);
    // While away, "alpha" had a message.
    fake.script_fetch(
        None,
        Ok(vec![
            active_subscription("1", "alpha", 50),
            active_subscription("2", "beta", 30),
        ]),
    );
    fake.end_connection(0);

    harness.expect_events(vec![Event::Rooms(vec![RoomChange::Moved {
        from: 1,
        to: 0,
        item: active_room("1", "alpha", 50),
    }])]);
}
