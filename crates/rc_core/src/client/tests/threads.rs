// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Opening threads, and replying in them.

use super::*;

/// A reply in the thread of `parent`, not shown in the room, in room 1.
fn reply(id: &str, secs: i64, parent: &str) -> Message {
    Message {
        thread_id: Some(parent.to_owned().into()),
        ..message(id, secs)
    }
}

/// The same reply, also shown in the room.
fn shown(message: Message) -> Message {
    Message {
        thread_shown: true,
        ..message
    }
}

fn thread_reset(room: Option<&str>, messages: Vec<Message>) -> Event {
    Event::ThreadTimeline(vec![TimelineChange::Reset {
        room: room.map(|id| id.to_owned().into()),
        messages,
    }])
}

fn thread_status(
    parent: &str,
    loading: bool,
    loading_older: bool,
    reached_start: bool,
    error: Option<TimelineError>,
) -> Event {
    Event::ThreadStatus(ThreadStatus {
        parent: Some(parent.to_owned().into()),
        loading,
        loading_older,
        reached_start,
        error,
    })
}

/// The events that close the open thread: no messages, no parent.
fn thread_closed_events() -> Vec<Event> {
    vec![
        thread_reset(None, Vec::new()),
        Event::ThreadStatus(ThreadStatus::default()),
    ]
}

impl Harness {
    /// The next event about the thread, skipping the others.
    fn next_thread(&self) -> Event {
        loop {
            let event = self.next_event();
            if matches!(event, Event::ThreadTimeline(_) | Event::ThreadStatus(_)) {
                return event;
            }
        }
    }

    /// Checks the next events about the thread, in order, skipping the others.
    fn expect_thread(&self, events: Vec<Event>) {
        for expected in events {
            assert_eq!(self.next_thread(), expected);
        }
    }

    /// The next `count` changes of the thread's messages, skipping the other events.
    fn thread_changes(&self, count: usize) -> Vec<TimelineChange> {
        let mut changes = Vec::new();
        while changes.len() < count {
            if let Event::ThreadTimeline(more) = self.next_event() {
                changes.extend(more);
            }
        }
        changes
    }
}

/// Opens the thread of `b` of room 1, which has `replies` (newest first) in its first page.
fn open_thread_with(fake: &Fake, harness: &Harness, replies: Vec<Message>, reached_start: bool) {
    fake.script_thread(None, page(replies, reached_start));
    harness.account().open_thread("b".to_owned().into());
}

/// Room 1 is open and so is the thread of `b`, whose replies are `r1` and `r2`; its events are
/// taken.
fn open_thread_of_b(fake: &Fake, harness: &Harness) {
    open_first_room(fake, harness, false);
    open_thread_with(
        fake,
        harness,
        vec![reply("r2", 4, "b"), reply("r1", 3, "b")],
        true,
    );
    harness.expect_thread(vec![
        thread_reset(Some("1"), Vec::new()),
        thread_status("b", true, false, false, None),
        thread_reset(
            Some("1"),
            vec![message("b", 2), reply("r1", 3, "b"), reply("r2", 4, "b")],
        ),
        thread_status("b", false, false, true, None),
    ]);
}

// ACT-004
#[test]
fn opening_a_thread_shows_the_parent_and_the_replies_oldest_first() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_thread_of_b(&fake, &harness);
    assert_eq!(
        fake.state().thread_calls,
        [ThreadCall {
            thread: "b".into(),
            offset: 0,
            count: crate::timeline::PAGE_SIZE,
        }]
    );
}

// ACT-004
#[test]
fn only_a_sent_text_message_of_the_open_room_can_be_opened_as_a_thread() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    // No room is open.
    harness.account().open_thread("b".to_owned().into());
    fake.script_history(
        None,
        page(
            vec![shown(reply("s", 5, "b")), message("b", 2), message("a", 1)],
            true,
        ),
    );
    harness.account().open_room("1".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("1"), Vec::new()),
        status("1", true, false, false, None),
        reset_event(
            Some("1"),
            vec![message("a", 1), message("b", 2), shown(reply("s", 5, "b"))],
        ),
        status("1", false, false, true, None),
    ]);
    // The send is held, so that the message stays pending.
    let gate = Arc::new(Notify::new());
    fake.script_send(Some(Arc::clone(&gate)), Ok(7));
    harness
        .account()
        .send_message("1".to_owned().into(), "unsent", None, None, false);
    let pending = expect_pending(&harness);
    // Unknown, a reply, and not sent yet.
    for id in ["zzz", "s", pending.id.as_str()] {
        harness.account().open_thread(id.to_owned().into());
    }
    harness.account().open_thread("a".to_owned().into());
    // The first event about a thread is the one for `a`, and only that one asked the server.
    harness.expect_thread(vec![
        thread_reset(Some("1"), Vec::new()),
        thread_status("a", true, false, false, None),
    ]);
    let calls = fake.wait_for_thread_calls(1);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].thread, "a");
    gate.notify_one();
}

// ACT-004
#[test]
fn older_replies_are_fetched_by_offset_and_the_parent_comes_with_the_start() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    open_thread_with(
        &fake,
        &harness,
        vec![reply("r4", 6, "b"), reply("r3", 5, "b")],
        false,
    );
    harness.expect_thread(vec![
        thread_reset(Some("1"), Vec::new()),
        thread_status("b", true, false, false, None),
        thread_reset(Some("1"), vec![reply("r3", 5, "b"), reply("r4", 6, "b")]),
        thread_status("b", false, false, false, None),
    ]);

    fake.script_thread(
        None,
        page(vec![reply("r2", 4, "b"), reply("r1", 3, "b")], true),
    );
    harness.account().load_older_thread();
    harness.expect_thread(vec![
        thread_status("b", false, true, false, None),
        Event::ThreadTimeline(vec![TimelineChange::Prepended(vec![
            message("b", 2),
            reply("r1", 3, "b"),
            reply("r2", 4, "b"),
        ])]),
        thread_status("b", false, false, true, None),
    ]);
    let offsets: Vec<_> = fake.state().thread_calls.iter().map(|c| c.offset).collect();
    assert_eq!(offsets, [0, 2]);

    // The start is reached: nothing more is asked.
    harness.account().load_older_thread();
    harness.account().close_thread();
    harness.expect_thread(thread_closed_events());
    assert_eq!(fake.state().thread_calls.len(), 2);
}

// ACT-004
#[test]
fn loading_older_replies_is_ignored_without_a_thread_and_while_a_fetch_runs() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    harness.account().load_older_thread();
    let gate = Arc::new(Notify::new());
    fake.script_thread(
        Some(Arc::clone(&gate)),
        page(vec![reply("r1", 3, "b")], true),
    );
    harness.account().open_thread("b".to_owned().into());
    harness.expect_thread(vec![
        thread_reset(Some("1"), Vec::new()),
        thread_status("b", true, false, false, None),
    ]);
    fake.wait_for_thread_calls(1);
    // Nothing is held yet, and a fetch runs.
    harness.account().load_older_thread();
    gate.notify_one();
    harness.expect_thread(vec![
        thread_reset(Some("1"), vec![message("b", 2), reply("r1", 3, "b")]),
        thread_status("b", false, false, true, None),
    ]);
    assert_eq!(fake.state().thread_calls.len(), 1);
}

// ACT-004
#[test]
fn the_answer_of_a_thread_that_was_closed_or_replaced_is_dropped() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    let gate = Arc::new(Notify::new());
    fake.script_thread(
        Some(Arc::clone(&gate)),
        page(vec![reply("old", 9, "b")], true),
    );
    harness.account().open_thread("b".to_owned().into());
    fake.wait_for_thread_calls(1);
    // `a` replaces `b` before `b`'s answer, which is then stale.
    fake.script_thread(None, page(vec![reply("mine", 3, "a")], true));
    harness.account().open_thread("a".to_owned().into());
    fake.wait_for_thread_calls(2);
    gate.notify_one();
    harness.expect_thread(vec![
        thread_reset(Some("1"), Vec::new()),
        thread_status("b", true, false, false, None),
        thread_reset(Some("1"), Vec::new()),
        thread_status("a", true, false, false, None),
        thread_reset(Some("1"), vec![message("a", 1), reply("mine", 3, "a")]),
        thread_status("a", false, false, true, None),
    ]);
    // The late answer of `b` is gone for good: closing shows nothing else came.
    std::thread::sleep(Duration::from_millis(50));
    harness.account().close_thread();
    harness.expect_thread(thread_closed_events());
}

// ACT-004
#[test]
fn a_failed_thread_fetch_shows_the_error_and_opening_again_retries() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    let error = ApiError::Unreachable("down".into());
    fake.script_thread(None, Err(error));
    harness.account().open_thread("b".to_owned().into());
    harness.expect_thread(vec![
        thread_reset(Some("1"), Vec::new()),
        thread_status("b", true, false, false, None),
        thread_status("b", false, false, false, Some(TimelineError::Unreachable)),
    ]);
    fake.script_thread(None, page(vec![reply("r1", 3, "b")], true));
    harness.account().open_thread("b".to_owned().into());
    harness.expect_thread(vec![
        thread_reset(Some("1"), Vec::new()),
        thread_status("b", true, false, false, None),
        thread_reset(Some("1"), vec![message("b", 2), reply("r1", 3, "b")]),
        thread_status("b", false, false, true, None),
    ]);
}

// ACT-004
#[test]
fn a_pushed_reply_goes_to_the_thread_and_not_to_the_room() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_thread_of_b(&fake, &harness);
    assert!(fake.push(0, Pushed::Message(reply("r3", 5, "b"))));
    // A reply of another thread is for nobody.
    assert!(fake.push(0, Pushed::Message(reply("x", 6, "a"))));
    assert!(fake.push(0, pushed_message("1", "c", 7)));
    harness.expect_events(vec![
        Event::ThreadTimeline(vec![TimelineChange::Appended(vec![reply("r3", 5, "b")])]),
        appended("c", 7),
    ]);
}

// ACT-004
#[test]
fn a_reply_also_sent_to_the_channel_goes_to_both() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_thread_of_b(&fake, &harness);
    let both = shown(reply("r3", 5, "b"));
    assert!(fake.push(0, Pushed::Message(both.clone())));
    harness.expect_events(vec![
        Event::ThreadTimeline(vec![TimelineChange::Appended(vec![both.clone()])]),
        Event::Timeline(vec![TimelineChange::Appended(vec![both])]),
    ]);
}

// ACT-004
#[test]
fn a_reply_pushed_while_the_thread_loads_joins_the_page() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    let gate = Arc::new(Notify::new());
    fake.script_thread(
        Some(Arc::clone(&gate)),
        page(vec![reply("r1", 3, "b")], true),
    );
    harness.account().open_thread("b".to_owned().into());
    harness.expect_thread(vec![
        thread_reset(Some("1"), Vec::new()),
        thread_status("b", true, false, false, None),
    ]);
    fake.wait_for_thread_calls(1);
    assert!(fake.push(0, Pushed::Message(reply("r2", 4, "b"))));
    flush_pushed(&fake, &harness, "1");
    gate.notify_one();
    harness.expect_thread(vec![
        thread_reset(
            Some("1"),
            vec![message("b", 2), reply("r1", 3, "b"), reply("r2", 4, "b")],
        ),
        thread_status("b", false, false, true, None),
    ]);
}

/// The parent after its reply count and text changed, as the server pushes it.
fn parent_with_two_replies() -> Message {
    Message {
        thread_replies: 2,
        thread_last_reply: jiff::Timestamp::from_second(4).ok(),
        edited: true,
        kind: MessageKind::Text("b, edited".into()),
        ..message("b", 2)
    }
}

// ACT-004
#[test]
fn the_pushed_parent_updates_the_room_and_the_thread() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_thread_of_b(&fake, &harness);
    assert!(fake.push(0, Pushed::Message(parent_with_two_replies())));
    harness.expect_events(vec![
        Event::ThreadTimeline(vec![TimelineChange::Updated {
            index: 0,
            message: parent_with_two_replies(),
        }]),
        updated(1, parent_with_two_replies()),
    ]);
}

// ACT-004
#[test]
fn a_parent_that_changes_while_the_thread_loads_comes_in_its_new_form() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    let gate = Arc::new(Notify::new());
    fake.script_thread(
        Some(Arc::clone(&gate)),
        page(vec![reply("r1", 3, "b")], true),
    );
    harness.account().open_thread("b".to_owned().into());
    harness.expect_thread(vec![
        thread_reset(Some("1"), Vec::new()),
        thread_status("b", true, false, false, None),
    ]);
    fake.wait_for_thread_calls(1);
    assert!(fake.push(0, Pushed::Message(parent_with_two_replies())));
    harness.expect_events(vec![updated(1, parent_with_two_replies())]);
    gate.notify_one();
    harness.expect_thread(vec![
        thread_reset(
            Some("1"),
            vec![parent_with_two_replies(), reply("r1", 3, "b")],
        ),
        thread_status("b", false, false, true, None),
    ]);
}

// ACT-004
#[test]
fn deleting_a_reply_removes_it_and_counts_one_less_in_the_parent() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    let parent = parent_with_two_replies();
    assert!(fake.push(0, Pushed::Message(parent.clone())));
    harness.expect_events(vec![updated(1, parent.clone())]);
    fake.script_thread(
        None,
        page(vec![reply("r2", 4, "b"), reply("r1", 3, "b")], true),
    );
    harness.account().open_thread("b".to_owned().into());
    harness.expect_thread(vec![
        thread_reset(Some("1"), Vec::new()),
        thread_status("b", true, false, false, None),
        thread_reset(
            Some("1"),
            vec![parent.clone(), reply("r1", 3, "b"), reply("r2", 4, "b")],
        ),
        thread_status("b", false, false, true, None),
    ]);

    // The server doesn't push the parent for a deletion.
    assert!(fake.push(
        0,
        Pushed::MessageDeleted {
            room: "1".to_owned().into(),
            id: "r2".to_owned().into(),
        }
    ));
    let fewer = Message {
        thread_replies: 1,
        ..parent
    };
    harness.expect_events(vec![
        Event::ThreadTimeline(vec![TimelineChange::Removed { index: 2 }]),
        updated(1, fewer.clone()),
        Event::ThreadTimeline(vec![TimelineChange::Updated {
            index: 0,
            message: fewer,
        }]),
    ]);
}

// ACT-004
#[test]
fn deleting_a_reply_shown_in_the_room_removes_it_from_both() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_thread_of_b(&fake, &harness);
    let both = shown(reply("r3", 5, "b"));
    assert!(fake.push(0, Pushed::Message(both.clone())));
    harness.expect_events(vec![
        Event::ThreadTimeline(vec![TimelineChange::Appended(vec![both.clone()])]),
        Event::Timeline(vec![TimelineChange::Appended(vec![both])]),
    ]);
    assert!(fake.push(
        0,
        Pushed::MessageDeleted {
            room: "1".to_owned().into(),
            id: "r3".to_owned().into(),
        }
    ));
    let events: Vec<_> = (0..2).map(|_| harness.next_event()).collect();
    assert!(events.contains(&Event::Timeline(vec![TimelineChange::Removed { index: 2 }])));
    assert!(
        events.contains(&Event::ThreadTimeline(vec![TimelineChange::Removed {
            index: 3
        }]))
    );
}

// ACT-004
#[test]
fn deleting_the_parent_closes_the_thread() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_thread_of_b(&fake, &harness);
    assert!(fake.push(
        0,
        Pushed::MessageDeleted {
            room: "1".to_owned().into(),
            id: "b".to_owned().into(),
        }
    ));
    let mut expected = vec![Event::Timeline(vec![TimelineChange::Removed { index: 1 }])];
    expected.extend(thread_closed_events());
    harness.expect_events(expected);
}

// ACT-004
#[test]
fn a_parent_left_as_a_removed_message_closes_the_thread() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_thread_of_b(&fake, &harness);
    // What the server does to a deleted parent that has replies.
    let removed = Message {
        kind: MessageKind::System(SystemEvent::Other("rm".into())),
        ..message("b", 2)
    };
    assert!(fake.push(0, Pushed::Message(removed.clone())));
    let mut expected = thread_closed_events();
    expected.push(updated(1, removed));
    harness.expect_events(expected);
}

// ACT-004
#[test]
fn another_room_closes_the_thread() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_thread_of_b(&fake, &harness);
    harness.account().open_room("2".to_owned().into());
    let mut expected = thread_closed_events();
    expected.extend([
        reset_event(Some("2"), Vec::new()),
        status("2", true, false, false, None),
        status("2", false, false, true, None),
    ]);
    harness.expect_events(expected);
}

// ACT-004
#[test]
fn the_room_going_away_closes_the_thread() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_thread_of_b(&fake, &harness);
    assert!(fake.push(0, removed("1")));
    let mut expected = vec![rooms_change(RoomChange::Removed { index: 0 })];
    expected.extend(thread_closed_events());
    expected.extend(closed_events());
    harness.expect_events(expected);
}

// ACT-004
#[test]
fn logout_closes_the_thread() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_thread_of_b(&fake, &harness);
    harness.logout();
    let mut closed = false;
    while !closed {
        closed = harness.next_event() == thread_reset(None, Vec::new());
    }
    assert_eq!(
        harness.next_thread(),
        Event::ThreadStatus(ThreadStatus::default())
    );
}

// ACT-004
#[test]
fn the_newest_replies_are_fetched_again_after_a_reconnect() {
    let fake = Fake::new();
    let harness = logged_in_harness_with(&fake, fast());
    open_thread_of_b(&fake, &harness);
    fake.script_fetch(
        None,
        Ok(vec![
            subscription("1", "general", rc_api::RoomType::Channel),
            subscription("2", "secret", rc_api::RoomType::Private),
        ]),
    );
    // Missed while the connection was down: r3.
    fake.script_thread(
        None,
        page(vec![reply("r3", 5, "b"), reply("r2", 4, "b")], false),
    );
    fake.end_connection(0);
    harness.expect_thread(vec![
        thread_status("b", true, false, true, None),
        Event::ThreadTimeline(vec![TimelineChange::Appended(vec![reply("r3", 5, "b")])]),
        thread_status("b", false, false, true, None),
    ]);
    let calls = fake.state().thread_calls.clone();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[1].offset, 0);
}

// ACT-004
#[test]
fn the_catch_up_changes_and_deletions_of_replies_reach_the_thread() {
    let fake = Fake::new();
    let harness = logged_in_harness_with(&fake, fast());
    open_thread_of_b(&fake, &harness);
    fake.script_fetch(
        None,
        Ok(vec![
            subscription("1", "general", rc_api::RoomType::Channel),
            subscription("2", "secret", rc_api::RoomType::Private),
        ]),
    );
    let edited = Message {
        edited: true,
        kind: MessageKind::Text("r1 edited".into()),
        ..reply("r1", 3, "b")
    };
    fake.script_changes(
        "1",
        None,
        Ok(Changes {
            updated: vec![edited.clone()],
            deleted: vec!["r2".to_owned().into()],
        }),
    );
    fake.end_connection(0);
    let changes = harness.thread_changes(2);
    assert!(changes.contains(&TimelineChange::Updated {
        index: 1,
        message: edited
    }));
    assert!(changes.contains(&TimelineChange::Removed { index: 2 }));
}

// ACT-004
#[test]
fn a_reply_shows_pending_in_the_thread_only_and_becomes_sent() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_thread_of_b(&fake, &harness);
    let gate = Arc::new(Notify::new());
    fake.script_send(Some(Arc::clone(&gate)), Ok(1_000));
    harness.account().send_message(
        "1".to_owned().into(),
        "in the thread",
        None,
        Some("b".to_owned().into()),
        false,
    );
    let Event::ThreadTimeline(changes) = harness.next_event() else {
        panic!("not about the thread");
    };
    let [TimelineChange::Appended(messages)] = changes.as_slice() else {
        panic!("not a single append: {changes:?}");
    };
    let pending = messages[0].clone();
    assert_eq!(pending.delivery, Delivery::Pending);
    assert_eq!(pending.thread_id, Some("b".to_owned().into()));
    assert!(!pending.thread_shown);
    assert_eq!(
        fake.wait_for_send_calls(1),
        [SendCall {
            room: "1".into(),
            id: pending.id.to_string(),
            text: "in the thread".into(),
            thread: Some(ThreadTarget {
                parent: "b".to_owned().into(),
                also_to_channel: false,
            }),
        }]
    );
    gate.notify_one();
    let sent = Message {
        delivery: Delivery::Sent,
        ts: jiff::Timestamp::from_second(1_000).unwrap(),
        thread_id: Some("b".to_owned().into()),
        author: message("x", 0).author,
        ..pending
    };
    harness.expect_events(vec![Event::ThreadTimeline(vec![TimelineChange::Updated {
        index: 3,
        message: sent,
    }])]);
    // The room never heard of it.
    flush_pushed(&fake, &harness, "1");
}

// ACT-004
#[test]
fn a_reply_also_sent_to_the_channel_is_pending_in_both_and_settles_in_both() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_thread_of_b(&fake, &harness);
    let gate = Arc::new(Notify::new());
    fake.script_send(Some(Arc::clone(&gate)), Ok(1_000));
    harness.account().send_message(
        "1".to_owned().into(),
        "for everybody",
        None,
        Some("b".to_owned().into()),
        true,
    );
    let Event::ThreadTimeline(changes) = harness.next_event() else {
        panic!("not about the thread");
    };
    let [TimelineChange::Appended(messages)] = changes.as_slice() else {
        panic!("not a single append: {changes:?}");
    };
    let pending = messages[0].clone();
    assert!(pending.thread_shown);
    harness.expect_events(vec![Event::Timeline(vec![TimelineChange::Appended(vec![
        pending.clone(),
    ])])]);
    assert_eq!(
        fake.wait_for_send_calls(1)[0].thread,
        Some(ThreadTarget {
            parent: "b".to_owned().into(),
            also_to_channel: true,
        })
    );
    gate.notify_one();
    let sent = Message {
        delivery: Delivery::Sent,
        ts: jiff::Timestamp::from_second(1_000).unwrap(),
        author: message("x", 0).author,
        ..pending
    };
    harness.expect_events(vec![
        updated(2, sent.clone()),
        Event::ThreadTimeline(vec![TimelineChange::Updated {
            index: 3,
            message: sent,
        }]),
    ]);
}

// ACT-004
#[test]
fn a_reply_to_a_thread_that_is_not_open_is_not_sent() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_thread_of_b(&fake, &harness);
    harness.account().send_message(
        "1".to_owned().into(),
        "astray",
        None,
        Some("a".to_owned().into()),
        true,
    );
    // Nothing comes out for it, and the next send is the first the server hears of.
    harness
        .account()
        .send_message("1".to_owned().into(), "plain", None, None, false);
    let pending = expect_pending(&harness);
    assert_eq!(fake.wait_for_send_calls(1)[0].id, pending.id.to_string());
    assert_eq!(fake.state().send_calls.len(), 1);
}

// ACT-004
#[test]
fn a_failed_reply_fails_in_the_thread_and_retries_as_a_reply() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_thread_of_b(&fake, &harness);
    fake.script_send(
        None,
        Err(ApiError::Server {
            status: 400,
            message: "nope".into(),
        }),
    );
    harness.account().send_message(
        "1".to_owned().into(),
        "in the thread",
        None,
        Some("b".to_owned().into()),
        false,
    );
    let Event::ThreadTimeline(changes) = harness.next_event() else {
        panic!("not about the thread");
    };
    let [TimelineChange::Appended(messages)] = changes.as_slice() else {
        panic!("not a single append: {changes:?}");
    };
    let pending = messages[0].clone();
    harness.expect_events(vec![Event::ThreadTimeline(vec![TimelineChange::Updated {
        index: 3,
        message: with_delivery(&pending, Delivery::Failed),
    }])]);

    harness.account().retry_message(pending.id.clone());
    harness.expect_events(vec![Event::ThreadTimeline(vec![TimelineChange::Updated {
        index: 3,
        message: pending.clone(),
    }])]);
    let calls = fake.wait_for_send_calls(2);
    assert_eq!(calls[1].id, pending.id.to_string());
    assert_eq!(
        calls[1].thread,
        Some(ThreadTarget {
            parent: "b".to_owned().into(),
            also_to_channel: false,
        })
    );
}

// ACT-004
#[test]
fn a_reply_in_the_thread_can_be_edited_reacted_to_and_deleted() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_thread_of_b(&fake, &harness);
    let id: MessageId = "r1".to_owned().into();

    fake.script_edit(None, Ok(3));
    harness.account().edit_message(id.clone(), "r1 edited");
    let Event::ThreadTimeline(changes) = harness.next_event() else {
        panic!("not about the thread");
    };
    let [TimelineChange::Updated { index: 1, message }] = changes.as_slice() else {
        panic!("not an update of the reply: {changes:?}");
    };
    assert_eq!(message.id, id);
    assert!(message.edited);
    assert!(matches!(&message.kind, MessageKind::Text(t) if t.text == "r1 edited"));
    harness.expect_events(vec![Event::MessageAction {
        id: id.clone(),
        action: MessageAction::Edit,
        outcome: Ok(()),
    }]);

    harness.account().react(id.clone(), "smile", true);
    let Event::ThreadTimeline(changes) = harness.next_event() else {
        panic!("not about the thread");
    };
    let [TimelineChange::Updated { index: 1, message }] = changes.as_slice() else {
        panic!("not an update of the reply: {changes:?}");
    };
    assert_eq!(message.reactions.len(), 1);
    assert_eq!(message.reactions[0].emoji, ":smile:");
    harness.expect_events(vec![Event::MessageAction {
        id: id.clone(),
        action: MessageAction::React,
        outcome: Ok(()),
    }]);

    harness.account().delete_message(id.clone());
    assert_eq!(
        harness.next_thread(),
        Event::ThreadTimeline(vec![TimelineChange::Removed { index: 1 }])
    );
    harness.expect_events(vec![Event::MessageAction {
        id,
        action: MessageAction::Delete,
        outcome: Ok(()),
    }]);
    assert_eq!(
        fake.state().delete_calls,
        [("1".to_owned(), "r1".to_owned())]
    );
}

// ACT-004
#[test]
fn a_reply_in_the_thread_can_be_quoted() {
    let fake = Fake::new();
    fake.state().site_url = Some(Ok(Some(SITE.to_owned())));
    let harness = logged_in_harness(&fake);
    open_thread_of_b(&fake, &harness);
    harness.account().send_message(
        "1".to_owned().into(),
        "agreed",
        Some("r1".to_owned().into()),
        Some("b".to_owned().into()),
        false,
    );
    let Event::ThreadTimeline(changes) = harness.next_event() else {
        panic!("not about the thread");
    };
    let [TimelineChange::Appended(messages)] = changes.as_slice() else {
        panic!("not a single append: {changes:?}");
    };
    let [quote] = messages[0].quotes.as_slice() else {
        panic!("no quote: {:?}", messages[0]);
    };
    assert_eq!(quote.message, Some("r1".to_owned().into()));
    let sent = fake.wait_for_send_calls(1);
    assert!(sent[0].text.ends_with(" agreed"), "{:?}", sent[0].text);
    assert!(sent[0].text.starts_with("[ ]("), "{:?}", sent[0].text);
}
