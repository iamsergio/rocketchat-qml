// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Editing, deleting and reacting to messages.

use super::*;

/// A message of room `1` written by somebody else.
fn message_of_bob(id: &str, secs: i64) -> Message {
    let mut message = message(id, secs);
    message.author.id = "bob-id".to_owned().into();
    message.author.username = "bob".to_owned();
    message
}

/// The message as an edit to `text` stores it, sent at `secs`.
fn edited_copy(id: &str, text: &str, secs: i64) -> Message {
    Message {
        edited: true,
        ..server_copy("1", id, text, secs)
    }
}

// ACT-001
#[test]
fn an_edit_replaces_the_message_and_is_answered() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    fake.script_edit(None, Ok(2));

    harness
        .account()
        .edit_message("b".to_owned().into(), "  bee\n");

    harness.expect_events(vec![
        updated(1, edited_copy("b", "bee", 2)),
        action_event("b", MessageAction::Edit, Ok(())),
    ]);
    assert_eq!(
        fake.state().edit_calls,
        [EditCall {
            room: "1".into(),
            id: "b".into(),
            text: "bee".into()
        }]
    );
    // The push of the same copy changes nothing.
    assert!(fake.push(0, Pushed::Message(edited_copy("b", "bee", 2))));
    flush_pushed(&fake, &harness, "1");
}

// ACT-001
#[test]
fn a_refused_edit_is_reported_and_changes_nothing() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    fake.script_edit(None, Err(refused()));

    harness.account().edit_message("b".to_owned().into(), "bee");

    harness.expect_events(vec![action_event(
        "b",
        MessageAction::Edit,
        Err(ActionError::Server(refused().to_string())),
    )]);
    flush_pushed(&fake, &harness, "1");
}

// ACT-001
#[test]
fn an_edit_that_cannot_reach_the_server_is_reported_as_such() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    fake.script_edit(None, Err(ApiError::Unreachable("refused".into())));

    harness.account().edit_message("b".to_owned().into(), "bee");

    harness.expect_events(vec![action_event(
        "b",
        MessageAction::Edit,
        Err(ActionError::Unreachable),
    )]);
}

// ACT-001
#[test]
fn a_blank_edit_is_refused_and_the_same_text_succeeds_without_a_request() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);

    harness
        .account()
        .edit_message("b".to_owned().into(), " \n ");
    harness.account().edit_message("b".to_owned().into(), " b ");

    harness.expect_events(vec![
        action_event("b", MessageAction::Edit, Err(ActionError::Empty)),
        action_event("b", MessageAction::Edit, Ok(())),
    ]);
    assert!(fake.state().edit_calls.is_empty());
}

// ACT-001
#[test]
fn only_sent_text_messages_of_the_user_in_the_open_room_are_edited() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    assert!(fake.push(0, Pushed::Message(message_of_bob("c", 3))));
    harness.expect_events(vec![Event::Timeline(vec![TimelineChange::Appended(vec![
        message_of_bob("c", 3),
    ])])]);
    let gate = Arc::new(Notify::new());
    fake.script_send(Some(gate), Ok(7));
    harness
        .account()
        .send_message("1".to_owned().into(), "unsent", None, None, false);
    let pending = expect_pending(&harness);

    for id in ["c", pending.id.as_str(), "nope"] {
        harness.account().edit_message(id.to_owned().into(), "x");
        harness.expect_events(vec![action_event(
            id,
            MessageAction::Edit,
            Err(ActionError::NotAllowed(MessageAction::Edit)),
        )]);
    }
    assert!(fake.state().edit_calls.is_empty());
}

// ACT-001
#[test]
fn a_message_of_a_room_that_is_not_open_is_not_edited() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    open_the_other_room(&harness);

    harness.account().edit_message("b".to_owned().into(), "bee");

    harness.expect_events(vec![action_event(
        "b",
        MessageAction::Edit,
        Err(ActionError::NotAllowed(MessageAction::Edit)),
    )]);
    assert!(fake.state().edit_calls.is_empty());
}

// ACT-001, ACT-002
#[test]
fn the_servers_settings_decide_what_can_be_changed() {
    let fake = Fake::new();
    fake.state().settings = Some(Ok(rc_api::MessageSettings {
        editing_allowed: false,
        edit_minutes: 0,
        deleting_allowed: true,
        delete_minutes: 5,
    }));
    let harness = logged_in_harness(&fake);
    let policy = MessagePolicy::from(fake.state().settings.clone().unwrap().unwrap());
    harness.expect_events(vec![Event::MessagePolicy(policy)]);
    open_first_room(&fake, &harness, false);

    // Editing is off, and the messages of 1970 are past the five minutes for deleting.
    harness.account().edit_message("b".to_owned().into(), "bee");
    harness.account().delete_message("b".to_owned().into());

    harness.expect_events(vec![
        action_event(
            "b",
            MessageAction::Edit,
            Err(ActionError::NotAllowed(MessageAction::Edit)),
        ),
        action_event(
            "b",
            MessageAction::Delete,
            Err(ActionError::NotAllowed(MessageAction::Delete)),
        ),
    ]);
    assert!(fake.state().edit_calls.is_empty());
    assert!(fake.state().delete_calls.is_empty());
}

// ACT-001, ACT-002
#[test]
fn the_policy_is_sent_when_it_changes_and_forgotten_at_logout() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    fake.script_login(None, Ok(()));
    let limited = rc_api::MessageSettings {
        edit_minutes: 15,
        ..rc_api::MessageSettings::default()
    };
    fake.state().settings = Some(Ok(limited));
    let harness = Harness::new(&fake);
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![]),
        Event::MessagePolicy(MessagePolicy::from(limited)),
    ]);

    harness.logout();
    harness.expect_events(vec![
        session_event(logged_out(None)),
        Event::MessagePolicy(MessagePolicy::default()),
    ]);
}

// ACT-001, ACT-002
#[test]
fn a_failed_settings_fetch_keeps_the_policy_and_the_session() {
    let fake = Fake::new();
    fake.state().settings = Some(Err(ApiError::Unreachable("refused".into())));
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    fake.script_edit(None, Ok(2));

    harness.account().edit_message("b".to_owned().into(), "bee");

    harness.expect_events(vec![
        updated(1, edited_copy("b", "bee", 2)),
        action_event("b", MessageAction::Edit, Ok(())),
    ]);
}

// ACT-002
#[test]
fn a_deletion_removes_the_message_and_is_answered() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);

    harness.account().delete_message("a".to_owned().into());

    harness.expect_events(vec![
        removal(0),
        action_event("a", MessageAction::Delete, Ok(())),
    ]);
    assert_eq!(
        fake.state().delete_calls,
        [("1".to_owned(), "a".to_owned())]
    );
    // The push of the deletion changes nothing.
    assert!(fake.push(0, deleted("1", "a")));
    flush_pushed(&fake, &harness, "1");
}

// ACT-002
#[test]
fn a_refused_deletion_is_reported_and_the_message_stays() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    fake.script_delete(None, Err(refused()));

    harness.account().delete_message("a".to_owned().into());

    harness.expect_events(vec![action_event(
        "a",
        MessageAction::Delete,
        Err(ActionError::Server(refused().to_string())),
    )]);
    flush_pushed(&fake, &harness, "1");
}

// ACT-002
#[test]
fn only_sent_messages_of_the_user_in_the_open_room_are_deleted() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    assert!(fake.push(0, Pushed::Message(message_of_bob("c", 3))));
    harness.expect_events(vec![Event::Timeline(vec![TimelineChange::Appended(vec![
        message_of_bob("c", 3),
    ])])]);
    let gate = Arc::new(Notify::new());
    fake.script_send(Some(gate), Ok(7));
    harness
        .account()
        .send_message("1".to_owned().into(), "unsent", None, None, false);
    let pending = expect_pending(&harness);

    for id in ["c", pending.id.as_str(), "nope"] {
        harness.account().delete_message(id.to_owned().into());
        harness.expect_events(vec![action_event(
            id,
            MessageAction::Delete,
            Err(ActionError::NotAllowed(MessageAction::Delete)),
        )]);
    }
    assert!(fake.state().delete_calls.is_empty());
}

// ACT-001, ACT-002
#[test]
fn nothing_happens_when_logged_out() {
    let fake = Fake::new();
    let harness = Harness::new(&fake);

    harness.account().edit_message("b".to_owned().into(), "bee");
    harness.account().delete_message("b".to_owned().into());

    // The first event after these is the login's.
    fake.script_login(None, Ok(()));
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
    ]);
    assert!(fake.state().edit_calls.is_empty());
    assert!(fake.state().delete_calls.is_empty());
}

// ACT-001
#[test]
fn an_edit_answered_after_logout_is_dropped() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    let gate = Arc::new(Notify::new());
    fake.script_edit(Some(Arc::clone(&gate)), Ok(2));
    harness.account().edit_message("b".to_owned().into(), "bee");
    wait_until(|| (!fake.state().edit_calls.is_empty()).then_some(()));

    harness.logout();
    harness.expect_events(vec![session_event(logged_out(None)), rooms_event(vec![])]);
    harness.expect_events(closed_events());
    gate.notify_one();
    fake.wait_for_edits_returned(1);

    // Nothing came out of the answer: the next events are the new session's.
    fake.script_login(None, Ok(()));
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
    ]);
}

// ACT-002
#[test]
fn a_deletion_answered_after_logout_is_dropped() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    let gate = Arc::new(Notify::new());
    fake.script_delete(Some(Arc::clone(&gate)), Ok(()));
    harness.account().delete_message("a".to_owned().into());
    wait_until(|| (!fake.state().delete_calls.is_empty()).then_some(()));

    harness.logout();
    harness.expect_events(vec![session_event(logged_out(None)), rooms_event(vec![])]);
    harness.expect_events(closed_events());
    gate.notify_one();
    fake.wait_for_deletes_returned(1);

    fake.script_login(None, Ok(()));
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
    ]);
}

// ACT-001, ACT-002
#[test]
fn a_dead_token_on_an_edit_or_a_deletion_expires_the_session() {
    for delete in [false, true] {
        let fake = Fake::new();
        let harness = logged_in_harness(&fake);
        open_first_room(&fake, &harness, false);
        if delete {
            fake.script_delete(None, Err(ApiError::Unauthorized));
            harness.account().delete_message("a".to_owned().into());
        } else {
            fake.script_edit(None, Err(ApiError::Unauthorized));
            harness.account().edit_message("b".to_owned().into(), "bee");
        }
        harness.expect_events(vec![
            session_event(logged_out(Some(LoginFailure::SessionExpired))),
            rooms_event(vec![]),
        ]);
        harness.expect_events(closed_events());
    }
}

/// The message `id` of the first room as it is with these reactions, `(emoji, users)`.
fn with_reactions(id: &str, secs: i64, reactions: &[(&str, &[&str])]) -> Message {
    Message {
        reactions: reactions
            .iter()
            .map(|(emoji, users)| Reaction {
                emoji: (*emoji).to_owned(),
                usernames: users.iter().map(|u| (*u).to_owned()).collect(),
            })
            .collect(),
        ..message(id, secs)
    }
}

fn react_call(id: &str, emoji: &str, react: bool) -> ReactCall {
    ReactCall {
        id: id.to_owned(),
        emoji: emoji.to_owned(),
        react,
    }
}

// ACT-003
#[test]
fn a_reaction_shows_in_the_message_at_once_and_is_answered() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);

    // Colons or not, the request has them. Reacting to a message of somebody else works too.
    harness.account().react("b".to_owned().into(), "tada", true);

    harness.expect_events(vec![
        updated(1, with_reactions("b", 2, &[(":tada:", &["alice"])])),
        action_event("b", MessageAction::React, Ok(())),
    ]);
    assert_eq!(fake.state().react_calls, [react_call("b", ":tada:", true)]);
}

// ACT-003
#[test]
fn a_reaction_joins_the_users_of_the_emoji_and_starts_new_ones_at_the_end() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    let held = with_reactions("b", 2, &[(":a:", &["bob"]), (":b:", &["bob"])]);
    assert!(fake.push(0, Pushed::Message(held.clone())));
    harness.expect_events(vec![updated(1, held)]);

    harness.account().react("b".to_owned().into(), ":a:", true);
    harness.expect_events(vec![
        updated(
            1,
            with_reactions("b", 2, &[(":a:", &["bob", "alice"]), (":b:", &["bob"])]),
        ),
        action_event("b", MessageAction::React, Ok(())),
    ]);
    harness.account().react("b".to_owned().into(), ":c:", true);
    harness.expect_events(vec![
        updated(
            1,
            with_reactions(
                "b",
                2,
                &[
                    (":a:", &["bob", "alice"]),
                    (":b:", &["bob"]),
                    (":c:", &["alice"]),
                ],
            ),
        ),
        action_event("b", MessageAction::React, Ok(())),
    ]);
}

// ACT-003
#[test]
fn taking_a_reaction_back_removes_the_user_and_drops_an_emoji_nobody_is_left_with() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    let held = with_reactions("b", 2, &[(":a:", &["bob", "alice"]), (":b:", &["alice"])]);
    assert!(fake.push(0, Pushed::Message(held.clone())));
    harness.expect_events(vec![updated(1, held)]);

    harness.account().react("b".to_owned().into(), ":a:", false);
    harness.expect_events(vec![
        updated(
            1,
            with_reactions("b", 2, &[(":a:", &["bob"]), (":b:", &["alice"])]),
        ),
        action_event("b", MessageAction::React, Ok(())),
    ]);
    harness.account().react("b".to_owned().into(), ":b:", false);
    harness.expect_events(vec![
        updated(1, with_reactions("b", 2, &[(":a:", &["bob"])])),
        action_event("b", MessageAction::React, Ok(())),
    ]);
    assert_eq!(
        fake.state().react_calls,
        [react_call("b", ":a:", false), react_call("b", ":b:", false)]
    );
}

// ACT-003
#[test]
fn the_push_of_a_reaction_after_the_answer_changes_nothing() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    harness
        .account()
        .react("b".to_owned().into(), ":tada:", true);
    harness.expect_events(vec![
        updated(1, with_reactions("b", 2, &[(":tada:", &["alice"])])),
        action_event("b", MessageAction::React, Ok(())),
    ]);

    assert!(fake.push(
        0,
        Pushed::Message(with_reactions("b", 2, &[(":tada:", &["alice"])]))
    ));

    flush_pushed(&fake, &harness, "1");
}

// ACT-003
#[test]
fn the_push_of_a_reaction_before_the_answer_leaves_nothing_for_the_answer_to_change() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    let gate = Arc::new(Notify::new());
    fake.script_react(Some(Arc::clone(&gate)), Ok(()));
    harness
        .account()
        .react("b".to_owned().into(), ":tada:", true);
    wait_until(|| (!fake.state().react_calls.is_empty()).then_some(()));

    let pushed = with_reactions("b", 2, &[(":tada:", &["alice"])]);
    assert!(fake.push(0, Pushed::Message(pushed.clone())));
    harness.expect_events(vec![updated(1, pushed)]);
    gate.notify_one();

    harness.expect_events(vec![action_event("b", MessageAction::React, Ok(()))]);
    flush_pushed(&fake, &harness, "1");
}

// ACT-003
#[test]
fn an_answer_applies_to_the_message_as_it_is_by_then() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    let gate = Arc::new(Notify::new());
    fake.script_react(Some(Arc::clone(&gate)), Ok(()));
    harness
        .account()
        .react("b".to_owned().into(), ":tada:", true);
    wait_until(|| (!fake.state().react_calls.is_empty()).then_some(()));

    // Somebody else reacted meanwhile.
    let pushed = with_reactions("b", 2, &[(":a:", &["bob"])]);
    assert!(fake.push(0, Pushed::Message(pushed.clone())));
    harness.expect_events(vec![updated(1, pushed)]);
    gate.notify_one();

    harness.expect_events(vec![
        updated(
            1,
            with_reactions("b", 2, &[(":a:", &["bob"]), (":tada:", &["alice"])]),
        ),
        action_event("b", MessageAction::React, Ok(())),
    ]);
}

// ACT-003
#[test]
fn a_reaction_to_a_message_deleted_meanwhile_is_answered_and_changes_nothing() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    let gate = Arc::new(Notify::new());
    fake.script_react(Some(Arc::clone(&gate)), Ok(()));
    harness
        .account()
        .react("b".to_owned().into(), ":tada:", true);
    wait_until(|| (!fake.state().react_calls.is_empty()).then_some(()));
    assert!(fake.push(0, deleted("1", "b")));
    harness.expect_events(vec![removal(1)]);
    gate.notify_one();

    harness.expect_events(vec![action_event("b", MessageAction::React, Ok(()))]);
}

// ACT-003
#[test]
fn a_refused_reaction_is_reported_and_changes_nothing() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    fake.script_react(None, Err(refused()));
    fake.script_react(None, Err(ApiError::Unreachable("refused".into())));

    harness
        .account()
        .react("b".to_owned().into(), ":nope:", true);
    harness.expect_events(vec![action_event(
        "b",
        MessageAction::React,
        Err(ActionError::Server(refused().to_string())),
    )]);
    harness
        .account()
        .react("b".to_owned().into(), ":nope:", true);
    harness.expect_events(vec![action_event(
        "b",
        MessageAction::React,
        Err(ActionError::Unreachable),
    )]);
    flush_pushed(&fake, &harness, "1");
}

// ACT-003
#[test]
fn only_sent_text_messages_of_the_open_room_are_reacted_to() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    let system = Message {
        kind: MessageKind::System(SystemEvent::Joined),
        ..message_of_bob("s", 3)
    };
    assert!(fake.push(0, Pushed::Message(system.clone())));
    harness.expect_events(vec![Event::Timeline(vec![TimelineChange::Appended(vec![
        system,
    ])])]);
    let gate = Arc::new(Notify::new());
    fake.script_send(Some(gate), Ok(7));
    harness
        .account()
        .send_message("1".to_owned().into(), "unsent", None, None, false);
    let pending = expect_pending(&harness);

    for id in ["s", pending.id.as_str(), "nope"] {
        harness
            .account()
            .react(id.to_owned().into(), ":tada:", true);
        harness.expect_events(vec![action_event(
            id,
            MessageAction::React,
            Err(ActionError::NotAllowed(MessageAction::React)),
        )]);
    }
    harness.account().react("b".to_owned().into(), " : ", true);
    harness.expect_events(vec![action_event(
        "b",
        MessageAction::React,
        Err(ActionError::NotAllowed(MessageAction::React)),
    )]);
    assert!(fake.state().react_calls.is_empty());
}

// ACT-003
#[test]
fn a_failed_message_is_not_reacted_to() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    fake.script_send(None, Err(ApiError::Unreachable("refused".into())));
    harness
        .account()
        .send_message("1".to_owned().into(), "unsent", None, None, false);
    let pending = expect_pending(&harness);
    harness.expect_events(vec![updated(2, with_delivery(&pending, Delivery::Failed))]);

    harness.account().react(pending.id.clone(), ":tada:", true);

    harness.expect_events(vec![action_event(
        pending.id.as_str(),
        MessageAction::React,
        Err(ActionError::NotAllowed(MessageAction::React)),
    )]);
    assert!(fake.state().react_calls.is_empty());
}

// ACT-003
#[test]
fn a_message_of_a_room_that_is_not_open_is_not_reacted_to() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    open_the_other_room(&harness);

    harness
        .account()
        .react("b".to_owned().into(), ":tada:", true);

    harness.expect_events(vec![action_event(
        "b",
        MessageAction::React,
        Err(ActionError::NotAllowed(MessageAction::React)),
    )]);
    assert!(fake.state().react_calls.is_empty());
}

// ACT-003
#[test]
fn nothing_is_reacted_to_when_logged_out() {
    let fake = Fake::new();
    let harness = Harness::new(&fake);

    harness
        .account()
        .react("b".to_owned().into(), ":tada:", true);

    // The first event after this is the login's.
    fake.script_login(None, Ok(()));
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
    ]);
    assert!(fake.state().react_calls.is_empty());
}

// ACT-003
#[test]
fn a_reaction_answered_after_logout_is_dropped() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    let gate = Arc::new(Notify::new());
    fake.script_react(Some(Arc::clone(&gate)), Ok(()));
    harness
        .account()
        .react("b".to_owned().into(), ":tada:", true);
    wait_until(|| (!fake.state().react_calls.is_empty()).then_some(()));

    harness.logout();
    harness.expect_events(vec![session_event(logged_out(None)), rooms_event(vec![])]);
    harness.expect_events(closed_events());
    gate.notify_one();
    fake.wait_for_reacts_returned(1);

    fake.script_login(None, Ok(()));
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
    ]);
}

// ACT-003
#[test]
fn a_dead_token_on_a_reaction_expires_the_session() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    fake.script_react(None, Err(ApiError::Unauthorized));

    harness
        .account()
        .react("b".to_owned().into(), ":tada:", true);

    harness.expect_events(vec![
        session_event(logged_out(Some(LoginFailure::SessionExpired))),
        rooms_event(vec![]),
    ]);
    harness.expect_events(closed_events());
}
