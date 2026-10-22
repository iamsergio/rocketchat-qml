// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Who is typing in the open room and the open thread, as the server pushes it. The fake clock
//! makes the typists expire.

use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;

/// The typist of the tests, not the logged-in `alice`.
const BOB: &str = "bob";
const CAROL: &str = "carol";

fn typing_pushed(room: &str, username: &str, typing: bool, thread: Option<&str>) -> Pushed {
    Pushed::Typing {
        room: room.to_owned().into(),
        username: username.to_owned(),
        typing,
        thread: thread.map(|id| id.to_owned().into()),
    }
}

fn start(fake: &Fake, username: &str) {
    assert!(fake.push(0, typing_pushed("1", username, true, None)));
}

fn stop(fake: &Fake, username: &str) {
    assert!(fake.push(0, typing_pushed("1", username, false, None)));
}

fn typing(room: &str, names: &[&str]) -> Event {
    Event::Typing {
        room: room.to_owned().into(),
        names: names.iter().map(|n| (*n).to_owned()).collect(),
    }
}

fn thread_typing(thread: &str, names: &[&str]) -> Event {
    Event::ThreadTyping {
        thread: thread.to_owned().into(),
        names: names.iter().map(|n| (*n).to_owned()).collect(),
    }
}

/// A message of the user `username` in room 1, at `secs`.
fn said_by(username: &str, id: &str, secs: i64) -> Message {
    Message {
        author: Author {
            id: format!("id-{username}").into(),
            username: username.to_owned(),
            display_name: None,
        },
        ..message(id, secs)
    }
}

fn thread_reply_by(username: &str, id: &str, secs: i64, parent: &str) -> Message {
    Message {
        thread_id: Some(parent.to_owned().into()),
        ..said_by(username, id, secs)
    }
}

/// The next event about typing, skipping the others.
fn next_typing(harness: &Harness) -> Event {
    loop {
        let event = harness.next_event();
        if matches!(event, Event::Typing { .. } | Event::ThreadTyping { .. }) {
            return event;
        }
    }
}

/// Makes sure that nothing about typing came for what was pushed so far: a message of the open
/// room is the proof that the pushes were taken in, since they are taken in order.
fn expect_no_typing_event(fake: &Fake, harness: &Harness) {
    // A message held already would not come out again: each flush has its own id.
    static FLUSHES: AtomicUsize = AtomicUsize::new(0);
    let id = format!("flush{}", FLUSHES.fetch_add(1, Ordering::Relaxed));
    assert!(fake.push(0, pushed_message("1", &id, 999)));
    let events = harness.events_until(|e| matches!(e, Event::Timeline(_)));
    assert!(
        events
            .iter()
            .all(|e| !matches!(e, Event::Typing { .. } | Event::ThreadTyping { .. })),
        "{events:?}"
    );
}

/// Room 1 is open, with its events taken.
fn open_room_1() -> (Fake, Harness) {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    (fake, harness)
}

// ACT-006
#[test]
fn a_start_shows_the_typist_and_a_stop_ends_it() {
    let (fake, harness) = open_room_1();

    start(&fake, BOB);
    assert_eq!(next_typing(&harness), typing("1", &[BOB]));
    stop(&fake, BOB);
    assert_eq!(next_typing(&harness), typing("1", &[]));
}

// ACT-006
#[test]
fn typists_come_in_the_order_they_started_and_leave_one_by_one() {
    let (fake, harness) = open_room_1();

    start(&fake, BOB);
    assert_eq!(next_typing(&harness), typing("1", &[BOB]));
    start(&fake, CAROL);
    assert_eq!(next_typing(&harness), typing("1", &[BOB, CAROL]));
    stop(&fake, BOB);
    assert_eq!(next_typing(&harness), typing("1", &[CAROL]));
}

// ACT-006
#[test]
fn the_name_is_the_display_name_when_a_held_message_has_it() {
    let (fake, harness) = open_room_1();
    let mut known = said_by(BOB, "m1", 5);
    known.author.display_name = Some("Bob B.".into());
    assert!(fake.push(0, Pushed::Message(known)));
    harness.events_until(|e| matches!(e, Event::Timeline(_)));

    start(&fake, BOB);
    assert_eq!(next_typing(&harness), typing("1", &["Bob B."]));
    start(&fake, CAROL);
    assert_eq!(next_typing(&harness), typing("1", &["Bob B.", CAROL]));
}

// ACT-006
#[test]
fn the_logged_in_user_is_ignored() {
    let (fake, harness) = open_room_1();

    // Their other clients type: the logged-in user is `alice`.
    start(&fake, "alice");
    expect_no_typing_event(&fake, &harness);
}

// ACT-006
#[test]
fn a_repeated_start_changes_nothing_and_renews_the_time() {
    let (fake, harness) = open_room_1();

    start(&fake, BOB);
    assert_eq!(next_typing(&harness), typing("1", &[BOB]));
    harness.clock.advance(Duration::from_secs(10));
    start(&fake, BOB);
    expect_no_typing_event(&fake, &harness);

    // 15 s after the first start, but only 5 after the last.
    harness.clock.advance(Duration::from_secs(5));
    expect_no_typing_event(&fake, &harness);
    harness.clock.advance(Duration::from_secs(10));
    assert_eq!(next_typing(&harness), typing("1", &[]));
}

// ACT-006
#[test]
fn a_typist_expires_after_fifteen_seconds_without_an_event() {
    let (fake, harness) = open_room_1();

    start(&fake, BOB);
    assert_eq!(next_typing(&harness), typing("1", &[BOB]));
    harness.clock.advance(Duration::from_secs(14));
    expect_no_typing_event(&fake, &harness);
    harness.clock.advance(Duration::from_secs(1));
    assert_eq!(next_typing(&harness), typing("1", &[]));
}

// ACT-006
#[test]
fn a_message_of_the_typist_in_the_room_ends_their_typing() {
    let (fake, harness) = open_room_1();
    start(&fake, BOB);
    start(&fake, CAROL);
    assert_eq!(next_typing(&harness), typing("1", &[BOB]));
    assert_eq!(next_typing(&harness), typing("1", &[BOB, CAROL]));

    assert!(fake.push(0, Pushed::Message(said_by(BOB, "m1", 5))));
    assert_eq!(next_typing(&harness), typing("1", &[CAROL]));
}

// ACT-006
#[test]
fn typing_is_not_shown_when_it_is_for_another_room() {
    let (fake, harness) = open_room_1();

    assert!(fake.push(0, typing_pushed("2", BOB, true, None)));
    expect_no_typing_event(&fake, &harness);
}

// ACT-006
#[test]
fn another_room_clears_the_typists() {
    let (fake, harness) = open_room_1();
    start(&fake, BOB);
    assert_eq!(next_typing(&harness), typing("1", &[BOB]));

    harness.account().open_room("2".to_owned().into());
    assert_eq!(next_typing(&harness), typing("1", &[]));
    // Not typing in the new room: they are gone, not moved.
    assert!(fake.push(0, typing_pushed("2", BOB, false, None)));
    assert!(fake.push(0, typing_pushed("2", CAROL, true, None)));
    assert_eq!(next_typing(&harness), typing("2", &[CAROL]));
}

// ACT-006
#[test]
fn a_typist_is_forgotten_when_the_room_comes_back() {
    let (fake, harness) = open_room_1();
    start(&fake, BOB);
    assert_eq!(next_typing(&harness), typing("1", &[BOB]));
    harness.account().open_room("2".to_owned().into());
    assert_eq!(next_typing(&harness), typing("1", &[]));
    harness.events_until(|e| matches!(e, Event::TimelineStatus(s) if !s.loading));
    fake.script_history(None, page(vec![message("b", 2), message("a", 1)], false));
    harness.account().open_room("1".to_owned().into());
    harness.events_until(|e| matches!(e, Event::TimelineStatus(s) if !s.loading));

    // Nothing is shown until they type again.
    start(&fake, CAROL);
    assert_eq!(next_typing(&harness), typing("1", &[CAROL]));
}

// ACT-006
#[test]
fn a_lost_connection_clears_the_typists() {
    let (fake, harness) = open_room_1();
    start(&fake, BOB);
    assert_eq!(next_typing(&harness), typing("1", &[BOB]));

    fake.end_connection(0);
    assert_eq!(next_typing(&harness), typing("1", &[]));
}

// ACT-006
#[test]
fn a_logout_clears_the_typists() {
    let (fake, harness) = open_room_1();
    start(&fake, BOB);
    assert_eq!(next_typing(&harness), typing("1", &[BOB]));

    harness.logout();
    assert_eq!(next_typing(&harness), typing("1", &[]));
}

// ACT-006
#[test]
fn closing_the_room_by_leaving_it_clears_the_typists() {
    let (fake, harness) = open_room_1();
    start(&fake, BOB);
    assert_eq!(next_typing(&harness), typing("1", &[BOB]));

    assert!(fake.push(0, Pushed::SubscriptionRemoved("1".to_owned().into())));
    assert_eq!(next_typing(&harness), typing("1", &[]));
}

/// Room 1 and the thread of `b` are open, with their events taken.
fn open_thread_of_b() -> (Fake, Harness) {
    let (fake, harness) = open_room_1();
    fake.script_thread(
        None,
        page(
            vec![
                thread_reply_by(BOB, "r2", 4, "b"),
                thread_reply_by(BOB, "r1", 3, "b"),
            ],
            true,
        ),
    );
    harness.account().open_thread("b".to_owned().into());
    harness
        .events_until(|e| matches!(e, Event::ThreadStatus(s) if !s.loading && s.parent.is_some()));
    (fake, harness)
}

// ACT-006
#[test]
fn a_typist_in_a_thread_is_in_the_thread_and_not_in_the_room() {
    let (fake, harness) = open_thread_of_b();

    assert!(fake.push(0, typing_pushed("1", BOB, true, Some("b"))));
    assert_eq!(next_typing(&harness), thread_typing("b", &[BOB]));
    // The same person typing in the room too is another entry.
    start(&fake, BOB);
    assert_eq!(next_typing(&harness), typing("1", &[BOB]));
    stop(&fake, BOB);
    assert_eq!(next_typing(&harness), typing("1", &[]));
    expect_no_typing_event(&fake, &harness);
    assert!(fake.push(0, typing_pushed("1", BOB, false, Some("b"))));
    assert_eq!(next_typing(&harness), thread_typing("b", &[]));
}

// ACT-006
#[test]
fn a_typist_in_a_thread_that_is_not_open_shows_when_it_opens() {
    let (fake, harness) = open_room_1();
    assert!(fake.push(0, typing_pushed("1", BOB, true, Some("b"))));
    expect_no_typing_event(&fake, &harness);

    fake.script_thread(None, page(vec![thread_reply_by(CAROL, "r1", 3, "b")], true));
    harness.account().open_thread("b".to_owned().into());
    assert_eq!(next_typing(&harness), thread_typing("b", &[BOB]));
}

// ACT-006
#[test]
fn a_reply_of_the_typist_ends_their_typing_in_the_thread_only() {
    let (fake, harness) = open_thread_of_b();
    start(&fake, BOB);
    assert_eq!(next_typing(&harness), typing("1", &[BOB]));
    assert!(fake.push(0, typing_pushed("1", BOB, true, Some("b"))));
    assert_eq!(next_typing(&harness), thread_typing("b", &[BOB]));

    assert!(fake.push(0, Pushed::Message(thread_reply_by(BOB, "r3", 5, "b"))));
    assert_eq!(next_typing(&harness), thread_typing("b", &[]));
    expect_no_typing_event(&fake, &harness);
}

// ACT-006
#[test]
fn closing_the_thread_tells_that_it_has_no_typists_now() {
    let (fake, harness) = open_thread_of_b();
    assert!(fake.push(0, typing_pushed("1", BOB, true, Some("b"))));
    assert_eq!(next_typing(&harness), thread_typing("b", &[BOB]));

    harness.account().close_thread();
    assert_eq!(next_typing(&harness), thread_typing("b", &[]));
}

// ACT-006
#[test]
fn another_room_clears_the_typists_of_the_room_and_of_the_thread() {
    let (fake, harness) = open_thread_of_b();
    start(&fake, BOB);
    assert_eq!(next_typing(&harness), typing("1", &[BOB]));
    assert!(fake.push(0, typing_pushed("1", CAROL, true, Some("b"))));
    assert_eq!(next_typing(&harness), thread_typing("b", &[CAROL]));

    harness.account().open_room("2".to_owned().into());
    let events = harness.events_until(|e| matches!(e, Event::TimelineStatus(s) if !s.loading));
    let told: Vec<_> = events
        .into_iter()
        .filter(|e| matches!(e, Event::Typing { .. } | Event::ThreadTyping { .. }))
        .collect();
    assert_eq!(told, [thread_typing("b", &[]), typing("1", &[])]);
}

// ACT-006
#[test]
fn the_expiry_of_a_thread_typist_is_told() {
    let (fake, harness) = open_thread_of_b();
    assert!(fake.push(0, typing_pushed("1", BOB, true, Some("b"))));
    assert_eq!(next_typing(&harness), thread_typing("b", &[BOB]));

    harness.clock.advance(Duration::from_secs(15));
    assert_eq!(next_typing(&harness), thread_typing("b", &[]));
}
