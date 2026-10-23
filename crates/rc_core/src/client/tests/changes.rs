// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Live edits, reactions and deletions, and the catch-up after a gap.

use super::*;

fn edited_message(id: &str, secs: i64) -> Message {
    Message {
        kind: MessageKind::Text(format!("{id} edited").as_str().into()),
        edited: true,
        ..message(id, secs)
    }
}

fn reacted_message(id: &str, secs: i64) -> Message {
    Message {
        reactions: vec![crate::domain::Reaction {
            emoji: ":thumbsup:".to_owned(),
            usernames: vec!["bob".to_owned()],
        }],
        ..message(id, secs)
    }
}

// Scripts take the result of the call, which is what the other scripts pass.
#[allow(clippy::unnecessary_wraps)]
fn changes(updated: Vec<Message>, deleted: &[&str]) -> Result<Changes, ApiError> {
    Ok(Changes {
        updated,
        deleted: deleted.iter().map(|&id| id.to_owned().into()).collect(),
    })
}

/// Opens room 2 and then room 1 again, past the events of both. Room 2 is empty. The newest page of
/// room 1 is `newest`, newest first, after `page_gate`.
fn leave_and_come_back(
    fake: &Fake,
    harness: &Harness,
    page_gate: Option<Arc<Notify>>,
    newest: Vec<Message>,
) {
    let visited = fake
        .state()
        .history_calls
        .iter()
        .any(|call| call.room == "2");
    harness.account().open_room("2".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("2"), Vec::new()),
        status("2", true, false, visited, None),
        status("2", false, false, true, None),
    ]);
    fake.script_history(page_gate, page(newest, false));
    harness.account().open_room("1".to_owned().into());
}

// RICH-006
#[test]
fn the_connection_follows_the_open_room() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    // Nothing is open yet: nothing to tell.
    assert_eq!(fake.watched(0), []);
    open_first_room(&fake, &harness, false);
    harness.account().open_room("2".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("2"), Vec::new()),
        status("2", true, false, false, None),
        status("2", false, false, true, None),
    ]);
    assert!(fake.push(0, removed("2")));
    harness.expect_events(vec![rooms_change(RoomChange::Removed { index: 1 })]);
    harness.expect_events(closed_events());

    let id = |room: &str| Some(RoomId::from(room.to_owned()));
    assert_eq!(fake.wait_for_watched(0, 3), [id("1"), id("2"), None]);
}

// RICH-006
#[test]
fn a_new_connection_is_told_which_room_is_open() {
    let fake = Fake::new();
    let harness = open_room_then_lose_the_connection(&fake);
    fake.script_fetch(
        None,
        Ok(vec![
            subscription("1", "general", rc_api::RoomType::Channel),
            subscription("2", "secret", rc_api::RoomType::Private),
        ]),
    );
    fake.end_connection(0);
    harness.expect_events(vec![
        status("1", true, false, false, None),
        status("1", false, false, true, None),
    ]);

    assert_eq!(
        fake.wait_for_watched(1, 1),
        [Some(RoomId::from("1".to_owned()))]
    );
}

// RICH-006
#[test]
fn a_deletion_removes_the_message_from_the_open_room_and_from_what_is_kept() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    assert!(fake.push(0, deleted("1", "a")));
    harness.expect_events(vec![removal(0)]);

    leave_and_come_back(&fake, &harness, None, vec![message("b", 2)]);
    // What was kept has no `a`.
    harness.expect_events(vec![
        reset_event(Some("1"), vec![message("b", 2)]),
        status("1", true, false, false, None),
        status("1", false, false, false, None),
    ]);
}

// RICH-006
#[test]
fn a_deletion_of_a_message_not_held_changes_nothing() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    assert!(fake.push(0, deleted("1", "nope")));
    assert!(fake.push(0, deleted("2", "a")));
    assert!(fake.push(0, pushed_message("1", "c", 3)));
    harness.expect_events(vec![appended("c", 3)]);
}

// RICH-006
#[test]
fn a_deletion_in_a_room_that_is_not_open_is_applied_to_what_is_kept() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    harness.account().open_room("2".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("2"), Vec::new()),
        status("2", true, false, false, None),
        status("2", false, false, true, None),
    ]);
    assert!(fake.push(0, deleted("1", "b")));
    // In order after the push: once this has come, the deletion was taken in.
    assert!(fake.push(0, pushed_message("2", "x", 9)));
    harness.expect_events(vec![Event::Timeline(vec![TimelineChange::Appended(vec![
        Message {
            room_id: "2".to_owned().into(),
            ..message("x", 9)
        },
    ])])]);

    harness.account().open_room("1".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("1"), vec![message("a", 1)]),
        status("1", true, false, false, None),
        status("1", false, false, true, None),
    ]);
}

// RICH-006
#[test]
fn a_message_deleted_while_the_page_loads_does_not_come_back_with_the_page() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    let gate = Arc::new(Notify::new());
    fake.script_history(
        Some(Arc::clone(&gate)),
        page(
            vec![message("c", 3), message("b", 2), message("a", 1)],
            false,
        ),
    );
    harness.account().open_room("1".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("1"), Vec::new()),
        status("1", true, false, false, None),
    ]);
    fake.wait_for_history_calls(1);
    // One that is waiting for the page, and one that is only in the page.
    assert!(fake.push(0, pushed_message("1", "d", 4)));
    assert!(fake.push(0, deleted("1", "d")));
    assert!(fake.push(0, deleted("1", "b")));
    assert!(fake.push(
        0,
        pushed_subscription("3", "alpha", rc_api::RoomType::Channel)
    ));
    harness.expect_events(vec![rooms_change(RoomChange::Inserted {
        index: 0,
        item: room("3", "alpha", RoomKind::Channel),
    })]);
    gate.notify_one();

    harness.expect_events(vec![
        reset_event(Some("1"), vec![message("a", 1), message("c", 3)]),
        status("1", false, false, false, None),
    ]);
}

// RICH-006
#[test]
fn a_live_edit_updates_the_message_in_place_and_is_kept() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    assert!(fake.push(0, Pushed::Message(edited_message("a", 1))));
    harness.expect_events(vec![updated(0, edited_message("a", 1))]);

    let newest = vec![message("b", 2), edited_message("a", 1)];
    leave_and_come_back(&fake, &harness, None, newest);
    harness.expect_events(vec![
        reset_event(Some("1"), vec![edited_message("a", 1), message("b", 2)]),
        status("1", true, false, false, None),
        status("1", false, false, false, None),
    ]);
}

// RICH-007
#[test]
fn a_live_reaction_updates_the_message_and_taking_it_back_updates_it_again() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    assert!(fake.push(0, Pushed::Message(reacted_message("b", 2))));
    harness.expect_events(vec![updated(1, reacted_message("b", 2))]);
    assert!(fake.push(0, Pushed::Message(message("b", 2))));
    harness.expect_events(vec![updated(1, message("b", 2))]);
}

// RICH-006
#[test]
fn a_live_edit_of_a_message_further_back_than_what_is_held_is_not_shown() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    assert!(fake.push(0, Pushed::Message(edited_message("old", 0))));
    assert!(fake.push(0, pushed_message("1", "c", 3)));
    harness.expect_events(vec![appended("c", 3)]);
}

// RICH-006
#[test]
fn a_room_that_was_never_up_to_date_asks_for_no_changes() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    harness.account().open_room("2".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("2"), Vec::new()),
        status("2", true, false, false, None),
        status("2", false, false, true, None),
    ]);
    assert_eq!(fake.state().changes_calls, []);
}

// RICH-006
#[test]
fn reopening_a_room_catches_up_with_what_changed_while_it_was_closed() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    fake.script_changes("1", None, changes(vec![edited_message("a", 1)], &["b"]));

    let gate = Arc::new(Notify::new());
    leave_and_come_back(
        &fake,
        &harness,
        Some(Arc::clone(&gate)),
        vec![edited_message("a", 1)],
    );
    harness.expect_events(vec![
        reset_event(Some("1"), vec![message("a", 1), message("b", 2)]),
        status("1", true, false, false, None),
        Event::Timeline(vec![
            TimelineChange::Updated {
                index: 0,
                message: edited_message("a", 1),
            },
            TimelineChange::Removed { index: 1 },
        ]),
    ]);
    // A minute earlier than the room was last up to date, for the difference of the clocks.
    let calls = fake.wait_for_changes_calls("1", 1);
    assert_eq!(calls[0].room, "1");
    let margin = Duration::from_secs(60);
    assert_eq!(calls[0].since, harness.clock.now() - margin);
    gate.notify_one();
    // The page, made after the deletion, has the new copy only.
    harness.expect_events(vec![status("1", false, false, false, None)]);
}

// RICH-006
#[test]
fn reconnecting_catches_up_the_open_room_with_edits_deletions_and_reactions() {
    let fake = Fake::new();
    let harness = open_room_then_lose_the_connection(&fake);
    fake.script_fetch(
        None,
        Ok(vec![
            subscription("1", "general", rc_api::RoomType::Channel),
            subscription("2", "secret", rc_api::RoomType::Private),
        ]),
    );
    // Held: a, b. Meanwhile b was edited and reacted to, a deleted, and c written (which comes
    // with the page, not with the changes).
    fake.script_changes(
        "1",
        None,
        changes(vec![edited_message("b", 2), message("c", 3)], &["a"]),
    );
    let gate = Arc::new(Notify::new());
    fake.script_history(
        Some(Arc::clone(&gate)),
        page(vec![message("c", 3), edited_message("b", 2)], false),
    );
    fake.end_connection(0);

    // Not a reset: the rows of the list stay.
    harness.expect_events(vec![
        status("1", true, false, false, None),
        Event::Timeline(vec![
            TimelineChange::Updated {
                index: 1,
                message: edited_message("b", 2),
            },
            TimelineChange::Removed { index: 0 },
        ]),
    ]);
    gate.notify_one();
    harness.expect_events(vec![
        Event::Timeline(vec![TimelineChange::Appended(vec![message("c", 3)])]),
        status("1", false, false, false, None),
    ]);
    assert_eq!(fake.state().changes_calls.len(), 1);
}

// RICH-006
#[test]
fn a_catch_up_changes_only_what_is_held() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    fake.script_changes(
        "1",
        None,
        changes(
            vec![message("new", 9), edited_message("old", 0)],
            &["unheard-of"],
        ),
    );
    let gate = Arc::new(Notify::new());
    leave_and_come_back(
        &fake,
        &harness,
        Some(Arc::clone(&gate)),
        vec![message("b", 2), message("a", 1)],
    );
    harness.expect_events(vec![
        reset_event(Some("1"), vec![message("a", 1), message("b", 2)]),
        status("1", true, false, false, None),
    ]);
    fake.wait_for_changes_calls("1", 1);
    // Pushes are taken in order, so once this one is out the changes have been applied too.
    assert!(fake.push(
        0,
        pushed_subscription("3", "alpha", rc_api::RoomType::Channel)
    ));
    harness.expect_events(vec![rooms_change(RoomChange::Inserted {
        index: 0,
        item: room("3", "alpha", RoomKind::Channel),
    })]);
    gate.notify_one();
    harness.expect_events(vec![status("1", false, false, false, None)]);
}

// RICH-006
#[test]
fn a_catch_up_answer_for_a_room_that_was_closed_or_a_previous_request_is_dropped() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    let stale = Arc::new(Notify::new());
    fake.script_changes("1", Some(Arc::clone(&stale)), changes(vec![], &["a"]));
    leave_and_come_back(
        &fake,
        &harness,
        None,
        vec![message("b", 2), message("a", 1)],
    );
    harness.expect_events(vec![
        reset_event(Some("1"), vec![message("a", 1), message("b", 2)]),
        status("1", true, false, false, None),
        status("1", false, false, false, None),
    ]);
    fake.wait_for_changes_calls("1", 1);

    // Away again and back: a second request, answered at once.
    leave_and_come_back(
        &fake,
        &harness,
        None,
        vec![message("b", 2), message("a", 1)],
    );
    harness.expect_events(vec![
        reset_event(Some("1"), vec![message("a", 1), message("b", 2)]),
        status("1", true, false, false, None),
        status("1", false, false, false, None),
    ]);
    fake.wait_for_changes_calls("1", 2);

    // The first one is answered now: it belongs to a visit that is over.
    stale.notify_one();
    assert!(fake.push(0, pushed_message("1", "c", 3)));
    harness.expect_events(vec![appended("c", 3)]);

    // And one for a room that is not open any more.
    let closed = Arc::new(Notify::new());
    fake.script_changes("1", Some(Arc::clone(&closed)), changes(vec![], &["b"]));
    harness.account().open_room("2".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("2"), Vec::new()),
        status("2", true, false, true, None),
        status("2", false, false, true, None),
    ]);
    harness.account().open_room("1".to_owned().into());
    harness.expect_events(vec![
        reset_event(
            Some("1"),
            vec![message("a", 1), message("b", 2), message("c", 3)],
        ),
        status("1", true, false, false, None),
        status("1", false, false, true, None),
    ]);
    fake.wait_for_changes_calls("1", 3);
    harness.account().open_room("2".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("2"), Vec::new()),
        status("2", true, false, true, None),
        status("2", false, false, true, None),
    ]);
    closed.notify_one();
    assert!(fake.push(0, pushed_message("2", "x", 9)));
    harness.expect_events(vec![Event::Timeline(vec![TimelineChange::Appended(vec![
        Message {
            room_id: "2".to_owned().into(),
            ..message("x", 9)
        },
    ])])]);
}

// RICH-006
#[test]
fn a_failed_catch_up_is_tried_again_from_the_same_time_and_a_successful_one_moves_it_on() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    fake.script_changes(
        "1",
        None,
        Err(ApiError::Unreachable("no connection".to_owned())),
    );
    // A failed answer moves nothing, whether it comes before the room is left or after.
    leave_and_come_back(
        &fake,
        &harness,
        None,
        vec![message("b", 2), message("a", 1)],
    );
    harness.expect_events(vec![
        reset_event(Some("1"), vec![message("a", 1), message("b", 2)]),
        status("1", true, false, false, None),
        status("1", false, false, false, None),
    ]);
    // A successful one must be taken in before the room is left, or it is stale: it brings an
    // edit, which shows when it has come, and the page waits for it.
    // Time passes, so that the page the successful one is made after is newer than the first's.
    harness.clock.advance(Duration::from_secs(1));
    fake.script_changes("1", None, changes(vec![edited_message("b", 2)], &[]));
    let gate = Arc::new(Notify::new());
    leave_and_come_back(
        &fake,
        &harness,
        Some(Arc::clone(&gate)),
        vec![edited_message("b", 2), message("a", 1)],
    );
    harness.expect_events(vec![
        reset_event(Some("1"), vec![message("a", 1), message("b", 2)]),
        status("1", true, false, false, None),
        updated(1, edited_message("b", 2)),
    ]);
    gate.notify_one();
    harness.expect_events(vec![status("1", false, false, false, None)]);
    leave_and_come_back(
        &fake,
        &harness,
        None,
        vec![edited_message("b", 2), message("a", 1)],
    );
    harness.expect_events(vec![
        reset_event(Some("1"), vec![message("a", 1), edited_message("b", 2)]),
        status("1", true, false, false, None),
        status("1", false, false, false, None),
    ]);
    let calls = fake.wait_for_changes_calls("1", 3);
    assert_eq!(calls[0].since, calls[1].since);
    assert!(calls[2].since > calls[1].since);
}

// RICH-006
#[test]
fn a_catch_up_with_a_dead_token_expires_the_session() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    fake.script_changes("1", None, Err(ApiError::Unauthorized));
    let gate = Arc::new(Notify::new());
    leave_and_come_back(&fake, &harness, Some(gate), vec![message("a", 1)]);
    harness.expect_events(vec![
        reset_event(Some("1"), vec![message("a", 1), message("b", 2)]),
        status("1", true, false, false, None),
        session_event(logged_out(Some(LoginFailure::SessionExpired))),
        rooms_event(vec![]),
    ]);
    harness.expect_events(closed_events());
}
