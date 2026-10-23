// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The list of the open room's threads: its pages, filters and live updates.

use super::threads::{reply, thread_closed_events, thread_reset, thread_status};
use super::*;
use crate::list_change::ListChange;

/// The parent `id` of room 1, whose thread has `replies` replies, the last at `last` seconds, and
/// which the user (id `uid`) follows. It predates what the room holds.
fn tparent(id: &str, last: i64, replies: u32) -> Message {
    Message {
        thread_replies: replies,
        thread_last_reply: jiff::Timestamp::from_second(last).ok(),
        thread_followers: vec!["uid".to_owned().into()],
        ..message(id, 0)
    }
}

// Scripts take the result of the call, which is what the other scripts pass.
#[allow(clippy::unnecessary_wraps)]
fn threads_page(threads: Vec<Message>, reached_end: bool) -> Result<ThreadsPage, ApiError> {
    Ok(ThreadsPage {
        threads,
        reached_end,
    })
}

fn list_reset(threads: Vec<Message>) -> Event {
    Event::ThreadList(vec![ListChange::Reset(threads)])
}

fn list_status(
    filter: ThreadFilter,
    loading: bool,
    loading_more: bool,
    reached_end: bool,
    error: Option<TimelineError>,
) -> Event {
    Event::ThreadListStatus(ThreadListStatus {
        room: Some("1".to_owned().into()),
        filter,
        loading,
        loading_more,
        reached_end,
        error,
    })
}

/// The events that close the open list: nothing in it, no room.
fn list_closed_events() -> Vec<Event> {
    vec![
        list_reset(Vec::new()),
        Event::ThreadListStatus(ThreadListStatus::default()),
    ]
}

impl Harness {
    /// The next event about the thread list, skipping the others.
    fn next_list(&self) -> Event {
        loop {
            let event = self.next_event();
            if matches!(event, Event::ThreadList(_) | Event::ThreadListStatus(_)) {
                return event;
            }
        }
    }

    /// Checks the next events about the thread list, in order, skipping the others.
    fn expect_list(&self, events: Vec<Event>) {
        for expected in events {
            assert_eq!(self.next_list(), expected);
        }
    }
}

/// Room 1 is open and so is its thread list, with `p1`, `p2` and `p3` (the most recent last
/// reply first) as its only page; the list events are taken.
fn open_list_of_three(fake: &Fake, harness: &Harness) -> Vec<Message> {
    open_first_room(fake, harness, false);
    let threads = vec![
        tparent("p3", 9, 1),
        tparent("p2", 8, 2),
        tparent("p1", 7, 1),
    ];
    fake.script_threads_list(None, threads_page(threads.clone(), true));
    harness.account().open_thread_list(ThreadFilter::All);
    harness.expect_list(vec![
        list_reset(Vec::new()),
        list_status(ThreadFilter::All, true, false, false, None),
        list_reset(threads.clone()),
        list_status(ThreadFilter::All, false, false, true, None),
    ]);
    threads
}

// ACT-010
#[test]
fn opening_the_list_loads_the_first_page_of_the_rooms_threads() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_list_of_three(&fake, &harness);
    assert_eq!(
        fake.state().thread_list_calls,
        [ThreadsListCall {
            room: "1".to_owned(),
            filter: ThreadFilter::All,
            offset: 0,
            count: crate::timeline::PAGE_SIZE,
        }]
    );
    // Open already: nothing is asked again.
    harness.account().open_thread_list(ThreadFilter::All);
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(fake.state().thread_list_calls.len(), 1);
}

// ACT-010
#[test]
fn the_list_is_ignored_without_a_room_and_while_logged_out() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    harness.account().open_thread_list(ThreadFilter::All);
    harness.account().load_more_threads();
    harness.account().close_thread_list();
    open_first_room(&fake, &harness, false);
    std::thread::sleep(Duration::from_millis(50));
    assert!(fake.state().thread_list_calls.is_empty());
    // Asking for more of a list that is not open does nothing, and nothing was announced.
    harness.account().open_thread_list(ThreadFilter::All);
    harness.expect_list(vec![list_reset(Vec::new())]);
}

// ACT-010
#[test]
fn later_pages_go_after_the_threads_held_by_offset_until_the_end() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    fake.script_threads_list(None, threads_page(vec![tparent("p3", 9, 1)], false));
    harness.account().open_thread_list(ThreadFilter::All);
    harness.expect_list(vec![
        list_reset(Vec::new()),
        list_status(ThreadFilter::All, true, false, false, None),
        list_reset(vec![tparent("p3", 9, 1)]),
        list_status(ThreadFilter::All, false, false, false, None),
    ]);

    // `p3` is on the next page again: it moved to the front meanwhile.
    fake.script_threads_list(
        None,
        threads_page(vec![tparent("p3", 9, 1), tparent("p2", 8, 1)], true),
    );
    harness.account().load_more_threads();
    harness.expect_list(vec![
        list_status(ThreadFilter::All, false, true, false, None),
        Event::ThreadList(vec![ListChange::Inserted {
            index: 1,
            item: tparent("p2", 8, 1),
        }]),
        list_status(ThreadFilter::All, false, false, true, None),
    ]);
    assert_eq!(fake.state().thread_list_calls[1].offset, 1);

    // Nothing after the end.
    harness.account().load_more_threads();
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(fake.state().thread_list_calls.len(), 2);
}

// ACT-010
#[test]
fn more_threads_are_not_asked_while_a_request_runs() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    fake.script_threads_list(None, threads_page(vec![tparent("p3", 9, 1)], false));
    harness.account().open_thread_list(ThreadFilter::All);
    harness.expect_list(vec![
        list_reset(Vec::new()),
        list_status(ThreadFilter::All, true, false, false, None),
        list_reset(vec![tparent("p3", 9, 1)]),
        list_status(ThreadFilter::All, false, false, false, None),
    ]);
    let gate = Arc::new(Notify::new());
    fake.script_threads_list(Some(Arc::clone(&gate)), threads_page(Vec::new(), true));
    harness.account().load_more_threads();
    fake.wait_for_threads_list_calls(2);
    harness.account().load_more_threads();
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(fake.state().thread_list_calls.len(), 2);
    gate.notify_one();
}

// ACT-010
#[test]
fn another_filter_starts_again_from_the_first_page_and_drops_the_stale_answer() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    let gate = Arc::new(Notify::new());
    fake.script_threads_list(
        Some(Arc::clone(&gate)),
        threads_page(vec![tparent("all", 9, 1)], true),
    );
    harness.account().open_thread_list(ThreadFilter::All);
    fake.wait_for_threads_list_calls(1);
    fake.script_threads_list(None, threads_page(vec![tparent("mine", 8, 1)], true));
    harness.account().open_thread_list(ThreadFilter::Following);
    fake.wait_for_threads_list_calls(2);
    gate.notify_one();
    harness.expect_list(vec![
        list_reset(Vec::new()),
        list_status(ThreadFilter::All, true, false, false, None),
        list_reset(Vec::new()),
        list_status(ThreadFilter::Following, true, false, false, None),
        list_reset(vec![tparent("mine", 8, 1)]),
        list_status(ThreadFilter::Following, false, false, true, None),
    ]);
    let calls = fake.state().thread_list_calls.clone();
    assert_eq!(calls[1].filter, ThreadFilter::Following);
    assert_eq!(calls[1].offset, 0);
    // The late answer of the first filter is gone for good.
    std::thread::sleep(Duration::from_millis(50));
    harness.account().close_thread_list();
    harness.expect_list(list_closed_events());
}

// ACT-010
#[test]
fn a_failed_first_page_shows_the_error_and_opening_again_retries() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    fake.script_threads_list(None, Err(ApiError::Unreachable("down".to_owned())));
    harness.account().open_thread_list(ThreadFilter::All);
    harness.expect_list(vec![
        list_reset(Vec::new()),
        list_status(ThreadFilter::All, true, false, false, None),
        list_status(
            ThreadFilter::All,
            false,
            false,
            false,
            Some(TimelineError::Unreachable),
        ),
    ]);
    // Nothing is held, so there is nothing to load more of.
    harness.account().load_more_threads();
    fake.script_threads_list(None, threads_page(vec![tparent("p1", 9, 1)], true));
    harness.account().open_thread_list(ThreadFilter::All);
    harness.expect_list(vec![
        list_reset(Vec::new()),
        list_status(ThreadFilter::All, true, false, false, None),
        list_reset(vec![tparent("p1", 9, 1)]),
        list_status(ThreadFilter::All, false, false, true, None),
    ]);
    assert_eq!(fake.state().thread_list_calls.len(), 2);
}

// ACT-010
#[test]
fn a_failed_later_page_keeps_the_threads_and_loading_more_tries_again() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    fake.script_threads_list(None, threads_page(vec![tparent("p3", 9, 1)], false));
    harness.account().open_thread_list(ThreadFilter::All);
    harness.expect_list(vec![
        list_reset(Vec::new()),
        list_status(ThreadFilter::All, true, false, false, None),
        list_reset(vec![tparent("p3", 9, 1)]),
        list_status(ThreadFilter::All, false, false, false, None),
    ]);
    fake.script_threads_list(None, Err(ApiError::Unreachable("down".to_owned())));
    harness.account().load_more_threads();
    harness.expect_list(vec![
        list_status(ThreadFilter::All, false, true, false, None),
        list_status(
            ThreadFilter::All,
            false,
            false,
            false,
            Some(TimelineError::Unreachable),
        ),
    ]);
    fake.script_threads_list(None, threads_page(vec![tparent("p2", 8, 1)], true));
    harness.account().load_more_threads();
    harness.expect_list(vec![
        list_status(ThreadFilter::All, false, true, false, None),
        Event::ThreadList(vec![ListChange::Inserted {
            index: 1,
            item: tparent("p2", 8, 1),
        }]),
        list_status(ThreadFilter::All, false, false, true, None),
    ]);
}

// ACT-010
#[test]
fn an_expired_token_on_a_page_expires_the_session() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    fake.script_threads_list(None, Err(ApiError::Unauthorized));
    harness.account().open_thread_list(ThreadFilter::All);
    loop {
        if let Event::Session(state) = harness.next_event() {
            assert_eq!(
                state,
                SessionState::LoggedOut {
                    failure: Some(LoginFailure::SessionExpired)
                }
            );
            break;
        }
    }
}

// ACT-010
#[test]
fn closing_the_list_while_it_loads_drops_the_answer() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    let gate = Arc::new(Notify::new());
    fake.script_threads_list(
        Some(Arc::clone(&gate)),
        threads_page(vec![tparent("p1", 9, 1)], true),
    );
    harness.account().open_thread_list(ThreadFilter::All);
    fake.wait_for_threads_list_calls(1);
    harness.account().close_thread_list();
    gate.notify_one();
    harness.expect_list(vec![
        list_reset(Vec::new()),
        list_status(ThreadFilter::All, true, false, false, None),
    ]);
    harness.expect_list(list_closed_events());
    std::thread::sleep(Duration::from_millis(50));
    // Closed again says nothing: the next thing about the list is the next open.
    harness.account().close_thread_list();
    harness.account().open_thread_list(ThreadFilter::All);
    harness.expect_list(vec![list_reset(Vec::new())]);
}

// ACT-010
#[test]
fn another_room_the_room_going_away_and_logout_close_the_list() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_list_of_three(&fake, &harness);
    harness.account().open_room("2".to_owned().into());
    harness.expect_list(list_closed_events());

    // The room going away.
    let harness = logged_in_harness(&Fake::new());
    let fake = Fake::new();
    drop(harness);
    let harness = logged_in_harness(&fake);
    open_list_of_three(&fake, &harness);
    assert!(fake.push(0, removed("1")));
    harness.expect_list(list_closed_events());

    // Logout.
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_list_of_three(&fake, &harness);
    harness.logout();
    harness.expect_list(list_closed_events());
}

// ACT-010
#[test]
fn the_answer_of_a_closed_room_or_a_logout_is_dropped() {
    for how in 0..2 {
        let fake = Fake::new();
        let harness = logged_in_harness(&fake);
        open_first_room(&fake, &harness, false);
        let gate = Arc::new(Notify::new());
        fake.script_threads_list(
            Some(Arc::clone(&gate)),
            threads_page(vec![tparent("p1", 9, 1)], true),
        );
        harness.account().open_thread_list(ThreadFilter::All);
        fake.wait_for_threads_list_calls(1);
        if how == 0 {
            harness.account().open_room("2".to_owned().into());
        } else {
            harness.logout();
        }
        gate.notify_one();
        harness.expect_list(vec![
            list_reset(Vec::new()),
            list_status(ThreadFilter::All, true, false, false, None),
        ]);
        harness.expect_list(list_closed_events());
        std::thread::sleep(Duration::from_millis(50));
    }
}

// ACT-010
#[test]
fn a_pushed_parent_updates_its_row_and_a_new_reply_moves_it_to_the_front() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_list_of_three(&fake, &harness);

    // An edit: same place.
    let edited = Message {
        edited: true,
        ..tparent("p2", 8, 2)
    };
    assert!(fake.push(0, Pushed::Message(edited.clone())));
    harness.expect_list(vec![Event::ThreadList(vec![ListChange::Updated {
        index: 1,
        item: edited,
    }])]);

    // A new reply to the oldest thread.
    let counted = tparent("p1", 20, 2);
    assert!(fake.push(0, Pushed::Message(counted.clone())));
    harness.expect_list(vec![Event::ThreadList(vec![ListChange::Moved {
        from: 2,
        to: 0,
        item: counted,
    }])]);
}

// ACT-010
#[test]
fn a_message_that_gets_its_first_reply_is_inserted_at_the_front() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_list_of_three(&fake, &harness);
    let first = tparent("fresh", 30, 1);
    assert!(fake.push(0, Pushed::Message(first.clone())));
    // The same message, the room's: the parent is in the room's messages too, if it is new.
    harness.expect_list(vec![Event::ThreadList(vec![ListChange::Inserted {
        index: 0,
        item: first,
    }])]);
}

// ACT-010
#[test]
fn a_deleted_parent_and_a_parent_left_as_a_removed_message_leave_the_list() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_list_of_three(&fake, &harness);
    assert!(fake.push(0, deleted("1", "p2")));
    harness.expect_list(vec![Event::ThreadList(vec![ListChange::Removed {
        index: 1,
    }])]);
    let removed_parent = Message {
        kind: MessageKind::System(SystemEvent::Other("rm".into())),
        ..tparent("p1", 7, 1)
    };
    assert!(fake.push(0, Pushed::Message(removed_parent)));
    harness.expect_list(vec![Event::ThreadList(vec![ListChange::Removed {
        index: 1,
    }])]);
}

// ACT-010
#[test]
fn the_followed_list_takes_a_thread_the_user_joins_and_drops_one_they_left() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    fake.script_threads_list(None, threads_page(vec![tparent("mine", 5, 1)], true));
    harness.account().open_thread_list(ThreadFilter::Following);
    harness.expect_list(vec![
        list_reset(Vec::new()),
        list_status(ThreadFilter::Following, true, false, false, None),
        list_reset(vec![tparent("mine", 5, 1)]),
        list_status(ThreadFilter::Following, false, false, true, None),
    ]);

    // A thread of somebody else's, that the user doesn't follow, is not for it.
    let theirs = Message {
        thread_followers: vec!["other".to_owned().into()],
        ..tparent("theirs", 10, 1)
    };
    assert!(fake.push(0, Pushed::Message(theirs)));
    // Once the user replies, they follow it.
    let joined = Message {
        thread_followers: vec!["other".to_owned().into(), "uid".to_owned().into()],
        ..tparent("theirs", 11, 2)
    };
    assert!(fake.push(0, Pushed::Message(joined.clone())));
    harness.expect_list(vec![Event::ThreadList(vec![ListChange::Inserted {
        index: 0,
        item: joined,
    }])]);

    // A copy without the user among the followers takes the thread out.
    let left = Message {
        thread_followers: vec!["other".to_owned().into()],
        ..tparent("mine", 12, 2)
    };
    assert!(fake.push(0, Pushed::Message(left)));
    harness.expect_list(vec![Event::ThreadList(vec![ListChange::Removed {
        index: 1,
    }])]);
}

// ACT-010
#[test]
fn the_list_ignores_the_messages_of_other_rooms_and_replies() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_list_of_three(&fake, &harness);
    let other_room = Message {
        room_id: "2".to_owned().into(),
        ..tparent("x", 50, 1)
    };
    assert!(fake.push(0, Pushed::Message(other_room)));
    let reply = Message {
        thread_id: Some("p1".to_owned().into()),
        ..message("r", 51)
    };
    assert!(fake.push(0, Pushed::Message(reply)));
    assert!(fake.push(0, pushed_message("1", "c", 52)));
    // The ordinary message reaches the room; nothing reached the list before it.
    harness.expect_events(vec![appended("c", 52)]);
    harness.account().close_thread_list();
    harness.expect_list(list_closed_events());
}

// ACT-010
#[test]
fn a_thread_opens_from_the_list_without_fetching_its_parent() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_list_of_three(&fake, &harness);
    fake.script_thread(None, page(vec![reply("x", 3, "p2")], true));
    harness.account().open_thread("p2".to_owned().into());
    harness.expect_thread(vec![
        thread_reset(Some("1"), Vec::new()),
        thread_status("p2", true, false, false, None),
        thread_reset(Some("1"), vec![tparent("p2", 8, 2), reply("x", 3, "p2")]),
        thread_status("p2", false, false, true, None),
    ]);
    assert!(fake.state().message_calls.is_empty());
    // The list stays under the thread.
    harness.account().close_thread();
    harness.expect_thread(thread_closed_events());
    harness.account().open_thread("p1".to_owned().into());
    harness.expect_thread(vec![
        thread_reset(Some("1"), Vec::new()),
        thread_status("p1", true, false, false, None),
    ]);
}

// ACT-010
#[test]
fn a_deleted_reply_counts_one_less_in_the_listed_parent() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_list_of_three(&fake, &harness);
    fake.script_thread(
        None,
        page(vec![reply("r2", 4, "p2"), reply("r1", 3, "p2")], true),
    );
    harness.account().open_thread("p2".to_owned().into());
    harness.expect_thread(vec![
        thread_reset(Some("1"), Vec::new()),
        thread_status("p2", true, false, false, None),
        thread_reset(
            Some("1"),
            vec![
                tparent("p2", 8, 2),
                reply("r1", 3, "p2"),
                reply("r2", 4, "p2"),
            ],
        ),
        thread_status("p2", false, false, true, None),
    ]);
    assert!(fake.push(0, deleted("1", "r2")));
    // The server doesn't push the parent for a deletion: the core counts, and the list hears.
    harness.expect_list(vec![Event::ThreadList(vec![ListChange::Updated {
        index: 1,
        item: tparent("p2", 8, 1),
    }])]);
}

// ACT-010
#[test]
fn a_reconnect_with_the_same_first_page_changes_nothing_and_a_different_one_resets() {
    let fake = Fake::new();
    let harness = logged_in_harness_with(&fake, fast());
    let threads = open_list_of_three(&fake, &harness);
    let subscriptions = || {
        Ok(vec![
            subscription("1", "general", rc_api::RoomType::Channel),
            subscription("2", "secret", rc_api::RoomType::Private),
        ])
    };
    fake.script_fetch(None, subscriptions());
    fake.script_threads_list(None, threads_page(threads.clone(), true));
    fake.end_connection(0);
    fake.wait_for_threads_list_calls(2);
    let calls = fake.state().thread_list_calls.clone();
    assert_eq!((calls[1].offset, calls[1].filter), (0, ThreadFilter::All));
    // The same page: a chance to see that nothing was said, then the second reconnect.
    std::thread::sleep(Duration::from_millis(100));

    let mut changed = threads.clone();
    changed.insert(0, tparent("missed", 30, 1));
    fake.script_fetch(None, subscriptions());
    fake.script_threads_list(None, threads_page(changed.clone(), true));
    fake.end_connection(1);
    // The first event about the list since is the reset to the new page.
    harness.expect_list(vec![
        list_reset(changed),
        list_status(ThreadFilter::All, false, false, true, None),
    ]);
}

// ACT-010
#[test]
fn a_refresh_that_fails_leaves_the_list_as_it_is() {
    let fake = Fake::new();
    let harness = logged_in_harness_with(&fake, fast());
    open_list_of_three(&fake, &harness);
    fake.script_fetch(
        None,
        Ok(vec![
            subscription("1", "general", rc_api::RoomType::Channel),
            subscription("2", "secret", rc_api::RoomType::Private),
        ]),
    );
    fake.script_threads_list(None, Err(ApiError::Unreachable("down".to_owned())));
    fake.end_connection(0);
    fake.wait_for_threads_list_calls(2);
    std::thread::sleep(Duration::from_millis(100));
    harness.account().close_thread_list();
    harness.expect_list(list_closed_events());
}
