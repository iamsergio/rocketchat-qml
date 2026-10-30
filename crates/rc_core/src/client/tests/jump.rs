// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Showing a message the room doesn't hold: the detached window, the pages after it and the way
//! back to the newest messages. The windows and pages are of four messages, so that a few messages
//! on the fake server make a long way.

use super::*;
use crate::timeline::{Found, FoundState};

const PAGE: usize = 4;

/// The message `m<n>`, sent `n` seconds after the epoch.
fn m(n: i64) -> Message {
    message(&format!("m{n:02}"), n)
}

fn edited_m(n: i64) -> Message {
    Message {
        kind: MessageKind::Text(format!("m{n:02} edited").as_str().into()),
        edited: true,
        ..m(n)
    }
}

fn reacted_m(n: i64) -> Message {
    Message {
        reactions: vec![Reaction {
            emoji: ":thumbsup:".to_owned(),
            usernames: vec!["bob".to_owned()],
        }],
        ..m(n)
    }
}

fn ms(range: std::ops::RangeInclusive<i64>) -> Vec<Message> {
    range.map(m).collect()
}

fn id(n: i64) -> MessageId {
    format!("m{n:02}").into()
}

/// The status of room `1` with this window state and nothing loading.
fn window(detached: bool, found: Option<(i64, FoundState)>, reached_start: bool) -> TimelineStatus {
    TimelineStatus {
        room: Some("1".to_owned().into()),
        detached,
        found: found.map(|(n, state)| Found {
            message: id(n),
            state,
        }),
        reached_start,
        ..TimelineStatus::default()
    }
}

fn status_of(status: TimelineStatus) -> Event {
    Event::TimelineStatus(status)
}

// What `window` takes, as the status has it.
#[allow(clippy::unnecessary_wraps)]
fn shown(n: i64) -> Option<(i64, FoundState)> {
    Some((n, FoundState::Shown))
}

#[allow(clippy::unnecessary_wraps)]
fn loading(n: i64) -> Option<(i64, FoundState)> {
    Some((n, FoundState::Loading))
}

fn show_event(n: i64) -> Event {
    Event::ShowMessage {
        room: "1".to_owned().into(),
        message: id(n),
    }
}

fn timeline_of(changes: Vec<TimelineChange>) -> Event {
    Event::Timeline(changes)
}

fn reset_to(messages: Vec<Message>) -> Event {
    reset_event(Some("1"), messages)
}

/// The server holds `m01`..`m<last>` in room `1`, and the harness has opened the room at its
/// newest two, with `backoff` for the connection.
fn open_at_newest(fake: &Fake, last: i64, backoff: Backoff) -> Harness {
    fake.set_room_messages("1", ms(1..=last));
    let harness = logged_in_harness_paged(fake, backoff, PAGE);
    fake.script_history(None, page(vec![m(last), m(last - 1)], false));
    harness.account().open_room("1".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("1"), Vec::new()),
        status("1", true, false, false, None),
        reset_to(vec![m(last - 1), m(last)]),
        status("1", false, false, false, None),
    ]);
    harness
}

fn quiet_backoff() -> Backoff {
    Backoff::new(NEVER, NEVER, NEVER)
}

/// Shows `target` and takes the events of the jump, whose window is `window`.
fn jump(harness: &Harness, target: i64, window: Vec<Message>, reached_start: bool) {
    harness.account().show_message(id(target));
    harness.expect_events(vec![
        status_of(self::window(false, loading(target), false)),
        status_of(self::window(true, shown(target), reached_start)),
        timeline_of(vec![TimelineChange::Reset {
            room: Some("1".to_owned().into()),
            messages: window,
        }]),
        show_event(target),
    ]);
}

/// Loads the page after the window and takes the events of a page that doesn't join.
fn load_newer(harness: &Harness, target: i64, appended: Vec<Message>, reached_start: bool) {
    harness.account().load_newer();
    harness.expect_events(vec![
        status_of(TimelineStatus {
            loading_newer: true,
            ..window(true, shown(target), reached_start)
        }),
        timeline_of(vec![TimelineChange::Appended(appended)]),
        status_of(window(true, shown(target), reached_start)),
    ]);
}

/// Waits until the actor has taken in what was pushed before: the edit of `m01`, of a window that
/// starts there, comes out as an update of its first row.
fn flush(fake: &Fake, harness: &Harness) {
    assert!(fake.push(0, Pushed::Message(edited_m(1))));
    harness.expect_events(vec![timeline_of(vec![TimelineChange::Updated {
        index: 0,
        message: edited_m(1),
    }])]);
}

// MSG-008
#[test]
fn a_message_the_room_holds_is_shown_without_asking_the_server() {
    let fake = Fake::new();
    let harness = open_at_newest(&fake, 20, quiet_backoff());

    harness.account().show_message(id(20));
    harness.expect_events(vec![
        status_of(window(false, shown(20), false)),
        show_event(20),
    ]);
    // Another one replaces the mark.
    harness.account().show_message(id(19));
    harness.expect_events(vec![
        status_of(window(false, shown(19), false)),
        show_event(19),
    ]);
    assert!(fake.state().around_calls.is_empty());
    assert_eq!(fake.state().history_calls.len(), 1);
}

// MSG-008
#[test]
fn a_message_the_room_doesnt_hold_comes_with_the_window_around_it_and_detaches_the_room() {
    let fake = Fake::new();
    let harness = open_at_newest(&fake, 20, quiet_backoff());

    jump(&harness, 5, ms(3..=6), false);

    assert_eq!(
        fake.state().around_calls,
        [AroundCall {
            room: "1".into(),
            message: "m05".into(),
            limit: PAGE,
        }]
    );
    assert_eq!(fake.state().history_calls.len(), 1);
}

// MSG-008
#[test]
fn a_window_from_the_start_of_the_room_says_so() {
    let fake = Fake::new();
    let harness = open_at_newest(&fake, 20, quiet_backoff());

    jump(&harness, 3, ms(1..=4), true);
}

// MSG-008
#[test]
fn a_window_that_shares_a_message_with_the_room_joins_it_and_stays_attached() {
    let fake = Fake::new();
    let harness = open_at_newest(&fake, 20, quiet_backoff());
    // m18 is before what is held, and its window, m16..m19, ends with the oldest message held.
    harness.account().show_message(id(18));

    harness.expect_events(vec![
        status_of(window(false, loading(18), false)),
        status_of(window(false, shown(18), false)),
        timeline_of(vec![TimelineChange::Prepended(ms(16..=18))]),
        show_event(18),
    ]);
}

// MSG-008
#[test]
fn a_second_jump_drops_the_answer_of_the_first() {
    let fake = Fake::new();
    let harness = open_at_newest(&fake, 20, quiet_backoff());
    let gate = Arc::new(Notify::new());
    fake.script_around(
        Some(Arc::clone(&gate)),
        Ok(Some(MessageWindow {
            messages: ms(3..=6),
            more_before: true,
            more_after: true,
        })),
    );

    harness.account().show_message(id(5));
    harness.expect_events(vec![status_of(window(false, loading(5), false))]);
    fake.wait_for_around_calls(1);
    harness.account().show_message(id(10));
    harness.expect_events(vec![
        status_of(window(false, loading(10), false)),
        status_of(window(true, shown(10), false)),
        timeline_of(vec![TimelineChange::Reset {
            room: Some("1".to_owned().into()),
            messages: ms(8..=11),
        }]),
        show_event(10),
    ]);
    gate.notify_one();
    std::thread::sleep(Duration::from_millis(150));
    // The first answer would have replaced the window by now; a message of the second is held.
    harness.account().show_message(id(9));
    harness.expect_events(vec![
        status_of(window(true, shown(9), false)),
        show_event(9),
    ]);
}

// MSG-008
#[test]
fn a_message_the_server_doesnt_have_is_missing_and_the_room_stays_as_it_is() {
    let fake = Fake::new();
    let harness = open_at_newest(&fake, 20, quiet_backoff());
    fake.script_around(None, Ok(None));

    harness.account().show_message(id(5));
    harness.expect_events(vec![
        status_of(window(false, loading(5), false)),
        status_of(window(false, Some((5, FoundState::Missing)), false)),
    ]);
    // The room holds what it held.
    harness.account().show_message(id(20));
    harness.expect_events(vec![
        status_of(window(false, shown(20), false)),
        show_event(20),
    ]);
}

// MSG-008
#[test]
fn a_thread_reply_the_room_doesnt_show_is_missing_for_now() {
    // Step 3e shows the parent instead.
    let fake = Fake::new();
    let harness = open_at_newest(&fake, 20, quiet_backoff());
    let mut messages = ms(1..=20);
    messages.push(Message {
        thread_id: Some(id(1)),
        ..message("reply", 10)
    });
    fake.set_room_messages("1", messages);

    harness.account().show_message("reply".to_owned().into());

    harness.expect_events(vec![
        Event::TimelineStatus(TimelineStatus {
            found: Some(Found {
                message: "reply".to_owned().into(),
                state: FoundState::Loading,
            }),
            ..window(false, None, false)
        }),
        Event::TimelineStatus(TimelineStatus {
            found: Some(Found {
                message: "reply".to_owned().into(),
                state: FoundState::Missing,
            }),
            ..window(false, None, false)
        }),
    ]);
}

// MSG-008
#[test]
fn a_failed_jump_shows_the_error_and_marks_nothing() {
    let fake = Fake::new();
    let harness = open_at_newest(&fake, 20, quiet_backoff());
    fake.script_around(None, Err(ApiError::Unreachable("refused".into())));

    harness.account().show_message(id(5));

    harness.expect_events(vec![
        status_of(window(false, loading(5), false)),
        status("1", false, false, false, Some(TimelineError::Unreachable)),
    ]);
}

// MSG-008
#[test]
fn an_expired_token_in_a_jump_ends_the_session() {
    let fake = Fake::new();
    let harness = open_at_newest(&fake, 20, quiet_backoff());
    fake.script_around(None, Err(ApiError::Unauthorized));

    harness.account().show_message(id(5));

    harness.expect_events(vec![
        status_of(window(false, loading(5), false)),
        session_event(logged_out(Some(LoginFailure::SessionExpired))),
        rooms_event(vec![]),
    ]);
    harness.expect_events(closed_events());
}

// MSG-008
#[test]
fn a_jump_while_the_newest_page_loads_fetches_it_again_if_the_room_ends_up_attached() {
    let fake = Fake::new();
    fake.set_room_messages("1", ms(1..=20));
    let harness = logged_in_harness_paged(&fake, quiet_backoff(), PAGE);
    let gate = Arc::new(Notify::new());
    fake.script_history(Some(gate), page(vec![m(20), m(19)], false));
    harness.account().open_room("1".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("1"), Vec::new()),
        status("1", true, false, false, None),
    ]);
    fake.wait_for_history_calls(1);
    fake.script_history(None, page(vec![m(20), m(19)], false));

    // The window reaches the newest message, so the room is attached and the page is still due.
    harness.account().show_message(id(19));

    harness.expect_events(vec![
        status_of(window(false, loading(19), false)),
        status_of(window(false, shown(19), false)),
        reset_to(ms(17..=20)),
        show_event(19),
        status_of(TimelineStatus {
            loading: true,
            ..window(false, shown(19), false)
        }),
        status_of(window(false, shown(19), false)),
    ]);
    assert_eq!(fake.state().history_calls.len(), 2);
}

// MSG-008
#[test]
fn pushed_messages_stay_out_of_the_window_and_are_there_after_showing_the_latest() {
    let fake = Fake::new();
    let harness = open_at_newest(&fake, 20, quiet_backoff());
    jump(&harness, 3, ms(1..=4), true);

    assert!(fake.push(0, pushed_message("1", "m21", 21)));
    flush(&fake, &harness);

    fake.script_history(None, page(vec![m(21), m(20), m(19)], false));
    harness.account().show_latest();
    harness.expect_events(vec![
        status_of(window(false, None, false)),
        // The store has the pushed message.
        reset_to(vec![m(19), m(20), m(21)]),
        status("1", true, false, false, None),
        status("1", false, false, false, None),
    ]);
}

// MSG-008
#[test]
fn edits_deletions_and_reactions_reach_the_window_and_the_stored_copy() {
    let fake = Fake::new();
    let harness = open_at_newest(&fake, 20, quiet_backoff());
    jump(&harness, 5, ms(3..=6), false);

    // In the window.
    assert!(fake.push(0, Pushed::Message(edited_m(5))));
    assert!(fake.push(0, Pushed::Message(reacted_m(6))));
    assert!(fake.push(0, deleted("1", "m04")));
    // In the stored copy only.
    assert!(fake.push(0, Pushed::Message(edited_m(20))));
    assert!(fake.push(0, deleted("1", "m19")));
    assert!(fake.push(0, Pushed::Message(edited_m(3))));
    harness.expect_events(vec![
        timeline_of(vec![TimelineChange::Updated {
            index: 2,
            message: edited_m(5),
        }]),
        timeline_of(vec![TimelineChange::Updated {
            index: 3,
            message: reacted_m(6),
        }]),
        removal(1),
        // Proves the pushes before it are taken in.
        timeline_of(vec![TimelineChange::Updated {
            index: 0,
            message: edited_m(3),
        }]),
    ]);

    fake.script_history(None, page(vec![edited_m(20)], false));
    harness.account().show_latest();
    harness.expect_events(vec![
        status_of(window(false, None, false)),
        reset_to(vec![edited_m(20)]),
        status("1", true, false, false, None),
        status("1", false, false, false, None),
    ]);
}

// MSG-008
#[test]
fn the_deletion_of_the_found_message_clears_the_mark() {
    let fake = Fake::new();
    let harness = open_at_newest(&fake, 20, quiet_backoff());
    jump(&harness, 5, ms(3..=6), false);

    assert!(fake.push(0, deleted("1", "m05")));

    harness.expect_events(vec![removal(2), status_of(window(true, None, false))]);
}

// MSG-008
#[test]
fn showing_the_latest_with_an_attached_room_only_clears_the_mark() {
    let fake = Fake::new();
    let harness = open_at_newest(&fake, 20, quiet_backoff());
    harness.account().show_message(id(20));
    harness.expect_events(vec![
        status_of(window(false, shown(20), false)),
        show_event(20),
    ]);

    harness.account().show_latest();

    harness.expect_events(vec![status_of(window(false, None, false))]);
    assert_eq!(fake.state().history_calls.len(), 1);
}

// MSG-008
#[test]
fn the_newer_pages_extend_the_window_until_one_reaches_the_end_and_the_newest_page_follows() {
    let fake = Fake::new();
    let harness = open_at_newest(&fake, 12, quiet_backoff());
    jump(&harness, 3, ms(1..=4), true);

    load_newer(&harness, 3, ms(5..=7), true);
    load_newer(&harness, 3, ms(8..=10), true);
    // m10, m11, m12: the end of the room.
    fake.script_history(None, page(vec![m(12), m(11)], false));
    harness.account().load_newer();
    harness.expect_events(vec![
        status_of(TimelineStatus {
            loading_newer: true,
            ..window(true, shown(3), true)
        }),
        timeline_of(vec![TimelineChange::Appended(ms(11..=12))]),
        // The newest page, as opening the room does, and the room is attached.
        status_of(TimelineStatus {
            loading: true,
            ..window(false, shown(3), true)
        }),
        status_of(window(false, shown(3), true)),
    ]);

    let after = |n: i64| m(n).ts.checked_sub(Duration::from_millis(1)).ok();
    assert_eq!(
        fake.state().newer_calls,
        [
            NewerCall {
                room: "1".into(),
                after: after(4),
                limit: PAGE,
            },
            NewerCall {
                room: "1".into(),
                after: after(7),
                limit: PAGE,
            },
            NewerCall {
                room: "1".into(),
                after: after(10),
                limit: PAGE,
            },
        ]
    );
    // Attached again: what is pushed is shown.
    assert!(fake.push(0, pushed_message("1", "m13", 13)));
    harness.expect_events(vec![appended("m13", 13)]);
}

// MSG-008
#[test]
fn a_page_that_reaches_a_message_the_stored_copy_holds_joins_the_window_with_the_rest_of_it() {
    let fake = Fake::new();
    let harness = open_at_newest(&fake, 9, quiet_backoff());
    jump(&harness, 3, ms(1..=4), true);
    // While detached the room gets m10 and m11, which only the stored copy takes.
    fake.set_room_messages("1", ms(1..=11));
    assert!(fake.push(0, Pushed::Message(m(10))));
    assert!(fake.push(0, Pushed::Message(m(11))));
    flush(&fake, &harness);

    load_newer(&harness, 3, ms(5..=7), true);
    // m07..m10: not the end of the room, but m08 and m09 are in the stored copy, and so are the
    // rest.
    fake.script_history(None, page(vec![m(11), m(10)], false));
    harness.account().load_newer();
    harness.expect_events(vec![
        status_of(TimelineStatus {
            loading_newer: true,
            ..window(true, shown(3), true)
        }),
        timeline_of(vec![TimelineChange::Appended(ms(8..=11))]),
        status_of(TimelineStatus {
            loading: true,
            ..window(false, shown(3), true)
        }),
        status_of(window(false, shown(3), true)),
    ]);
}

// MSG-008
#[test]
fn loading_newer_is_ignored_when_attached_and_while_a_page_is_loading() {
    let fake = Fake::new();
    let harness = open_at_newest(&fake, 20, quiet_backoff());
    harness.account().load_newer();
    jump(&harness, 3, ms(1..=4), true);
    assert!(fake.state().newer_calls.is_empty());

    let gate = Arc::new(Notify::new());
    fake.script_newer(
        Some(Arc::clone(&gate)),
        Ok(NewerPage {
            messages: vec![m(4), m(5)],
            reached_end: false,
            last: Some(m(5).ts),
        }),
    );
    harness.account().load_newer();
    harness.expect_events(vec![status_of(TimelineStatus {
        loading_newer: true,
        ..window(true, shown(3), true)
    })]);
    harness.account().load_newer();
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(fake.state().newer_calls.len(), 1);
    gate.notify_one();
    harness.expect_events(vec![
        timeline_of(vec![TimelineChange::Appended(vec![m(5)])]),
        status_of(window(true, shown(3), true)),
    ]);
}

// MSG-008
#[test]
fn a_failed_newer_page_shows_the_error_and_loading_again_retries() {
    let fake = Fake::new();
    let harness = open_at_newest(&fake, 20, quiet_backoff());
    jump(&harness, 3, ms(1..=4), true);
    fake.script_newer(None, Err(ApiError::Unreachable("refused".into())));

    harness.account().load_newer();

    harness.expect_events(vec![
        status_of(TimelineStatus {
            loading_newer: true,
            ..window(true, shown(3), true)
        }),
        status_of(TimelineStatus {
            error: Some(TimelineError::Unreachable),
            ..window(true, shown(3), true)
        }),
    ]);
    load_newer(&harness, 3, ms(5..=7), true);
}

// MSG-008
#[test]
fn loading_older_extends_the_window_backwards() {
    let fake = Fake::new();
    let harness = open_at_newest(&fake, 20, quiet_backoff());
    jump(&harness, 9, ms(7..=10), false);

    // `reached_start` is the page's, and was not known for the window.
    fake.script_history(None, page(vec![m(7), m(6), m(5)], true));
    harness.account().load_older();

    harness.expect_events(vec![
        status_of(TimelineStatus {
            loading_older: true,
            ..window(true, shown(9), false)
        }),
        timeline_of(vec![TimelineChange::Prepended(ms(5..=6))]),
        status_of(window(true, shown(9), true)),
    ]);
    let calls = fake.state().history_calls.clone();
    assert_eq!(calls[1].before, Some(m(7).ts));
}

// MSG-008
#[test]
fn sending_while_detached_shows_the_latest_first() {
    let fake = Fake::new();
    let harness = open_at_newest(&fake, 20, quiet_backoff());
    jump(&harness, 5, ms(3..=6), false);
    let page_gate = Arc::new(Notify::new());
    fake.script_history(
        Some(Arc::clone(&page_gate)),
        page(vec![m(20), m(19)], false),
    );
    let send_gate = Arc::new(Notify::new());
    fake.script_send(Some(Arc::clone(&send_gate)), Ok(1000));

    harness
        .account()
        .send_message("1".to_owned().into(), "hello", None, None, false);

    harness.expect_events(vec![
        status_of(window(false, None, false)),
        reset_to(vec![m(19), m(20)]),
        status("1", true, false, false, None),
    ]);
    let pending = expect_pending(&harness);
    page_gate.notify_one();
    harness.expect_events(vec![status("1", false, false, false, None)]);
    send_gate.notify_one();
    harness.expect_events(vec![updated(
        2,
        server_copy("1", pending.id.as_str(), "hello", 1000),
    )]);
}

// MSG-008
#[test]
fn sending_clears_the_mark_of_an_attached_room() {
    let fake = Fake::new();
    let harness = open_at_newest(&fake, 20, quiet_backoff());
    harness.account().show_message(id(20));
    harness.expect_events(vec![
        status_of(window(false, shown(20), false)),
        show_event(20),
    ]);
    fake.script_send(None, Ok(7));

    harness
        .account()
        .send_message("1".to_owned().into(), "hello", None, None, false);

    harness.expect_events(vec![status_of(window(false, None, false))]);
    expect_pending(&harness);
}

// MSG-008
#[test]
fn reconnecting_while_detached_catches_up_the_window_and_the_stored_copy_only() {
    let fake = Fake::new();
    let harness = open_at_newest(&fake, 20, fast());
    jump(&harness, 5, ms(3..=6), false);
    fake.script_fetch(
        None,
        Ok(vec![
            subscription("1", "general", rc_api::RoomType::Channel),
            subscription("2", "secret", rc_api::RoomType::Private),
        ]),
    );
    fake.script_changes(
        "1",
        None,
        Ok(Changes {
            updated: vec![edited_m(4), edited_m(20)],
            deleted: vec![id(5)],
        }),
    );
    fake.end_connection(0);

    harness.expect_events(vec![
        timeline_of(vec![
            TimelineChange::Updated {
                index: 1,
                message: edited_m(4),
            },
            TimelineChange::Removed { index: 2 },
        ]),
        // The found message is gone.
        status_of(window(true, None, false)),
    ]);
    // No newest page: it would have replaced the window.
    assert_eq!(fake.state().history_calls.len(), 1);
    assert_eq!(fake.state().changes_calls.len(), 1);

    fake.script_history(None, page(vec![edited_m(20), m(19)], false));
    harness.account().show_latest();
    harness.expect_events(vec![
        status_of(window(false, None, false)),
        reset_to(vec![m(19), edited_m(20)]),
        status("1", true, false, false, None),
        status("1", false, false, false, None),
    ]);
}

// MSG-008
#[test]
fn another_room_and_back_opens_at_the_newest_and_the_window_never_reached_the_store() {
    let fake = Fake::new();
    let harness = open_at_newest(&fake, 20, quiet_backoff());
    jump(&harness, 5, ms(3..=6), false);
    assert!(fake.push(0, Pushed::Message(edited_m(5))));
    harness.expect_events(vec![timeline_of(vec![TimelineChange::Updated {
        index: 2,
        message: edited_m(5),
    }])]);
    load_newer(&harness, 5, ms(7..=9), false);

    open_the_other_room(&harness);
    fake.script_history(None, page(vec![m(20), m(19)], false));
    harness.account().open_room("1".to_owned().into());

    // What the store holds: the newest two, as before the jump.
    harness.expect_events(vec![
        reset_to(vec![m(19), m(20)]),
        status("1", true, false, false, None),
        status("1", false, false, false, None),
    ]);
}

// MSG-008
#[test]
fn a_pushed_message_while_detached_changes_no_unread_count() {
    let fake = Fake::new();
    let harness = open_at_newest(&fake, 20, quiet_backoff());
    fake.wait_for_mark_read_calls(1);
    jump(&harness, 5, ms(3..=6), false);

    // As with an attached room: the server counts the message, the room list doesn't show it.
    assert!(fake.push(0, pushed_message("1", "m21", 21)));
    assert!(fake.push(0, Pushed::Subscription(general_with(1))));
    // Another room's count does rise.
    let mut secret = subscription("2", "secret", rc_api::RoomType::Private);
    secret.unread = 4;
    assert!(fake.push(0, Pushed::Subscription(secret)));

    let mut secret_room = room("2", "secret", RoomKind::Private);
    secret_room.unread = 4;
    harness.expect_events(vec![rooms_change(RoomChange::Updated {
        index: 1,
        item: secret_room,
    })]);
    // And the server is told again, once.
    assert_eq!(fake.wait_for_mark_read_calls(2), ["1", "1"]);
}

// MSG-008
#[test]
fn a_newer_page_of_hidden_replies_moves_the_next_request_on() {
    let fake = Fake::new();
    let harness = open_at_newest(&fake, 20, quiet_backoff());
    jump(&harness, 3, ms(1..=4), true);
    // Every message of the page is a reply the room doesn't show: it ends at 4.8 s, not at m04.
    let hidden_end = jiff::Timestamp::from_millisecond(4800).unwrap();
    fake.script_newer(
        None,
        Ok(NewerPage {
            messages: Vec::new(),
            reached_end: false,
            last: Some(hidden_end),
        }),
    );
    harness.account().load_newer();
    harness.expect_events(vec![
        status_of(TimelineStatus {
            loading_newer: true,
            ..window(true, shown(3), true)
        }),
        status_of(window(true, shown(3), true)),
    ]);

    // The next page starts where the last one ended, and brings the visible messages.
    load_newer(&harness, 3, ms(5..=8), true);

    let calls = fake.state().newer_calls.clone();
    let after = |ts: jiff::Timestamp| ts.checked_sub(Duration::from_millis(1)).ok();
    assert_eq!(
        calls.iter().map(|call| call.after).collect::<Vec<_>>(),
        [after(m(4).ts), after(hidden_end)]
    );
}
