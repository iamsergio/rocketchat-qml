// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Sending messages, and retrying the ones that failed.

use super::*;

/// The messages of the next event, which resets the message list.
fn reset_messages(harness: &Harness) -> Vec<Message> {
    let Event::Timeline(changes) = harness.next_event() else {
        panic!("not a timeline event");
    };
    let [TimelineChange::Reset { messages, .. }] = changes.as_slice() else {
        panic!("not a reset: {changes:?}");
    };
    messages.clone()
}

// SEND-001
#[test]
fn a_pending_message_carries_the_time_of_the_clock() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    let now = FakeClock::start() + Duration::from_secs(90);
    harness.clock.set(FakeClock::start());
    harness.clock.advance(Duration::from_secs(90));
    harness
        .account()
        .send_message("1".to_owned().into(), "hello", None, None, false);
    assert_eq!(expect_pending(&harness).ts, now);
}

// SEND-001
#[test]
fn a_sent_message_shows_as_pending_and_becomes_sent_when_the_server_answers() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    let gate = Arc::new(Notify::new());
    fake.script_send(Some(Arc::clone(&gate)), Ok(7));

    harness
        .account()
        .send_message("1".to_owned().into(), "  hello\n", None, None, false);

    let pending = expect_pending(&harness);
    assert_eq!(pending.kind, MessageKind::Text("hello".into()));
    assert_eq!(pending.room_id.as_str(), "1");
    assert_eq!(pending.author.id.as_str(), "uid");
    assert_eq!(pending.author.username, "alice");
    // Sent under the id that was shown.
    assert_eq!(
        fake.wait_for_send_calls(1),
        [SendCall {
            room: "1".into(),
            id: pending.id.to_string(),
            text: "hello".into(),
            thread: None,
        }]
    );
    gate.notify_one();
    harness.expect_events(vec![updated(
        2,
        server_copy("1", pending.id.as_str(), "hello", 7),
    )]);
}

// SEND-001
#[test]
fn blank_text_unknown_rooms_and_logged_out_clients_send_nothing() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    let harness = Harness::new(&fake);
    // Logged out.
    harness
        .account()
        .send_message("1".to_owned().into(), "hi", None, None, false);
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![]),
    ]);
    // Not in the room list.
    harness
        .account()
        .send_message("1".to_owned().into(), "hi", None, None, false);
    harness.logout();
    harness.expect_events(vec![session_event(logged_out(None))]);
    drop(harness);

    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    harness
        .account()
        .send_message("1".to_owned().into(), " \n\t ", None, None, false);
    harness
        .account()
        .send_message("1".to_owned().into(), "", None, None, false);
    drop(harness);
    assert!(fake.state().send_calls.is_empty());
}

// SEND-001
#[test]
fn the_echo_that_comes_first_settles_the_message_and_the_answer_is_ignored() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    let gate = Arc::new(Notify::new());
    fake.script_send(Some(Arc::clone(&gate)), Ok(7));
    harness
        .account()
        .send_message("1".to_owned().into(), "hello", None, None, false);
    let pending = expect_pending(&harness);
    fake.wait_for_send_calls(1);

    let copy = server_copy("1", pending.id.as_str(), "hello", 7);
    assert!(fake.push(0, Pushed::Message(copy.clone())));
    harness.expect_events(vec![updated(2, copy)]);

    gate.notify_one();
    fake.wait_for_sends_returned(1);
    // Nothing came out for the answer, and the message is there once.
    assert!(fake.push(0, pushed_message("1", "c", 8)));
    harness.expect_events(vec![appended("c", 8)]);
}

// SEND-002
#[test]
fn a_refused_message_fails_and_retrying_sends_it_under_the_same_id() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    fake.script_send(
        None,
        Err(ApiError::Server {
            status: 400,
            message: "no".into(),
        }),
    );
    harness
        .account()
        .send_message("1".to_owned().into(), "hello", None, None, false);
    let pending = expect_pending(&harness);
    harness.expect_events(vec![updated(2, with_delivery(&pending, Delivery::Failed))]);

    fake.script_send(None, Ok(9));
    harness.account().retry_message(pending.id.clone());
    harness.expect_events(vec![
        updated(2, pending.clone()),
        updated(2, server_copy("1", pending.id.as_str(), "hello", 9)),
    ]);
    let calls = fake.wait_for_send_calls(2);
    assert_eq!(calls[0], calls[1]);
    assert_eq!(calls[1].id, pending.id.as_str());
}

// SEND-002
#[test]
fn an_unreachable_server_fails_the_message_too() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    fake.script_send(None, Err(ApiError::Unreachable("refused".into())));
    harness
        .account()
        .send_message("1".to_owned().into(), "hello", None, None, false);
    let pending = expect_pending(&harness);
    harness.expect_events(vec![updated(2, with_delivery(&pending, Delivery::Failed))]);
}

// SEND-002
#[test]
fn only_a_failed_message_of_the_open_room_is_retried() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    let gate = Arc::new(Notify::new());
    fake.script_send(Some(Arc::clone(&gate)), Ok(7));
    harness
        .account()
        .send_message("1".to_owned().into(), "hello", None, None, false);
    let pending = expect_pending(&harness);

    // Sent already, pending, and unknown.
    harness.account().retry_message("a".to_owned().into());
    harness.account().retry_message(pending.id.clone());
    harness.account().retry_message("nope".to_owned().into());
    gate.notify_one();
    harness.expect_events(vec![updated(
        2,
        server_copy("1", pending.id.as_str(), "hello", 7),
    )]);

    assert_eq!(fake.state().send_calls.len(), 1);
}

// SEND-001
#[test]
fn a_message_confirmed_after_the_room_was_switched_is_sent_when_the_room_opens_again() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    let gate = Arc::new(Notify::new());
    fake.script_send(Some(Arc::clone(&gate)), Ok(7));
    harness
        .account()
        .send_message("1".to_owned().into(), "hello", None, None, false);
    let pending = expect_pending(&harness);
    fake.wait_for_send_calls(1);
    open_the_other_room(&harness);

    gate.notify_one();
    fake.wait_for_sends_returned(1);
    flush_after_answer(&fake, &harness);

    harness.account().open_room("1".to_owned().into());
    assert_eq!(
        reset_messages(&harness),
        [
            message("a", 1),
            message("b", 2),
            server_copy("1", pending.id.as_str(), "hello", 7)
        ]
    );
}

/// Makes sure the actor has taken in the answer of a send that returned, while room 2 is open.
fn flush_after_answer(fake: &Fake, harness: &Harness) {
    flush_pushed(fake, harness, "2");
}

// SEND-002
#[test]
fn a_message_that_fails_after_the_room_was_switched_shows_as_failed_when_it_opens_again() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    let gate = Arc::new(Notify::new());
    fake.script_send(
        Some(Arc::clone(&gate)),
        Err(ApiError::Unreachable("down".into())),
    );
    harness
        .account()
        .send_message("1".to_owned().into(), "hello", None, None, false);
    let pending = expect_pending(&harness);
    fake.wait_for_send_calls(1);
    open_the_other_room(&harness);

    gate.notify_one();
    fake.wait_for_sends_returned(1);
    flush_after_answer(&fake, &harness);

    harness.account().open_room("1".to_owned().into());
    assert_eq!(
        reset_messages(&harness),
        [
            message("a", 1),
            message("b", 2),
            with_delivery(&pending, Delivery::Failed)
        ]
    );
}

// SEND-002
#[test]
fn a_failed_message_is_retried_from_the_room_it_was_shown_in_after_switching_back() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    fake.script_send(None, Err(ApiError::Unreachable("down".into())));
    harness
        .account()
        .send_message("1".to_owned().into(), "hello", None, None, false);
    let pending = expect_pending(&harness);
    harness.expect_events(vec![updated(2, with_delivery(&pending, Delivery::Failed))]);
    open_the_other_room(&harness);
    harness.account().open_room("1".to_owned().into());
    assert_eq!(reset_messages(&harness).len(), 3);
    harness.expect_events(vec![
        status("1", true, false, false, None),
        status("1", false, false, true, None),
    ]);

    // Retrying, then switching away before the answer comes.
    let gate = Arc::new(Notify::new());
    fake.script_send(Some(Arc::clone(&gate)), Ok(7));
    harness.account().retry_message(pending.id.clone());
    harness.expect_events(vec![updated(2, pending.clone())]);
    fake.wait_for_send_calls(2);
    harness.account().open_room("2".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("2"), Vec::new()),
        status("2", true, false, true, None),
        status("2", false, false, true, None),
    ]);
    gate.notify_one();
    fake.wait_for_sends_returned(1);
    flush_after_answer(&fake, &harness);

    harness.account().open_room("1".to_owned().into());
    let shown = reset_messages(&harness);
    assert_eq!(
        shown.last(),
        Some(&server_copy("1", pending.id.as_str(), "hello", 7))
    );
    assert_eq!(shown.len(), 3);
}

// SEND-001
#[test]
fn the_echo_of_a_message_sent_from_a_room_that_is_not_open_settles_it() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    let gate = Arc::new(Notify::new());
    fake.script_send(Some(Arc::clone(&gate)), Ok(7));
    harness
        .account()
        .send_message("1".to_owned().into(), "hello", None, None, false);
    let pending = expect_pending(&harness);
    fake.wait_for_send_calls(1);
    open_the_other_room(&harness);

    let copy = server_copy("1", pending.id.as_str(), "hello", 7);
    assert!(fake.push(0, Pushed::Message(copy.clone())));
    flush_after_answer(&fake, &harness);

    harness.account().open_room("1".to_owned().into());
    assert_eq!(
        reset_messages(&harness),
        [message("a", 1), message("b", 2), copy]
    );
}

// SEND-001
#[test]
fn messages_of_others_in_rooms_that_are_not_open_are_not_kept() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    open_the_other_room(&harness);
    // Neither ours nor anyone's: only echoes of our own sends touch a closed room.
    assert!(fake.push(0, pushed_message("1", "c", 3)));
    flush_after_answer(&fake, &harness);
    harness.account().open_room("1".to_owned().into());
    assert_eq!(reset_messages(&harness), [message("a", 1), message("b", 2)]);
}

// SEND-001
#[test]
fn a_confirmed_message_moves_to_where_the_servers_time_puts_it() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    let gate = Arc::new(Notify::new());
    fake.script_send(Some(Arc::clone(&gate)), Ok(50));
    harness
        .account()
        .send_message("1".to_owned().into(), "hello", None, None, false);
    let pending = expect_pending(&harness);
    fake.wait_for_send_calls(1);

    // Someone else's message comes in meanwhile, and goes in front of the pending one.
    assert!(fake.push(0, pushed_message("1", "c", 100)));
    harness.expect_events(vec![Event::Timeline(vec![TimelineChange::Inserted {
        index: 2,
        messages: vec![message("c", 100)],
    }])]);

    // The server's clock says ours came before it.
    gate.notify_one();
    harness.expect_events(vec![Event::Timeline(vec![TimelineChange::Moved {
        from: 3,
        to: 2,
        message: server_copy("1", pending.id.as_str(), "hello", 50),
    }])]);
}

// SEND-002
#[test]
fn an_unsent_message_survives_the_catch_up_after_a_reconnect() {
    let fake = Fake::new();
    let harness = open_room_then_lose_the_connection(&fake);
    fake.script_send(None, Err(ApiError::Unreachable("down".into())));
    harness
        .account()
        .send_message("1".to_owned().into(), "hello", None, None, false);
    let pending = expect_pending(&harness);
    let failed = with_delivery(&pending, Delivery::Failed);
    harness.expect_events(vec![updated(2, failed.clone())]);

    fake.script_fetch(
        None,
        Ok(vec![
            subscription("1", "general", rc_api::RoomType::Channel),
            subscription("2", "secret", rc_api::RoomType::Private),
        ]),
    );
    fake.script_history(None, page(vec![message("c", 3), message("b", 2)], false));
    fake.end_connection(0);

    // The page goes in front of the failed message.
    harness.expect_events(vec![
        status("1", true, false, false, None),
        Event::Timeline(vec![TimelineChange::Inserted {
            index: 2,
            messages: vec![message("c", 3)],
        }]),
        status("1", false, false, false, None),
    ]);
}

// SEND-002
#[test]
fn an_unsent_message_survives_a_gap_longer_than_a_page() {
    let fake = Fake::new();
    let harness = open_room_then_lose_the_connection(&fake);
    fake.script_send(None, Err(ApiError::Unreachable("down".into())));
    harness
        .account()
        .send_message("1".to_owned().into(), "hello", None, None, false);
    let pending = expect_pending(&harness);
    let failed = with_delivery(&pending, Delivery::Failed);
    harness.expect_events(vec![updated(2, failed.clone())]);

    fake.script_fetch(
        None,
        Ok(vec![
            subscription("1", "general", rc_api::RoomType::Channel),
            subscription("2", "secret", rc_api::RoomType::Private),
        ]),
    );
    fake.script_history(None, page(vec![message("z", 26), message("y", 25)], false));
    fake.end_connection(0);

    harness.expect_events(vec![
        status("1", true, false, false, None),
        reset_event(Some("1"), vec![message("y", 25), message("z", 26), failed]),
        status("1", false, false, false, None),
    ]);
}

// SEND-001
#[test]
fn the_catch_up_settles_a_message_whose_echo_and_answer_were_lost() {
    let fake = Fake::new();
    let harness = open_room_then_lose_the_connection(&fake);
    let gate = Arc::new(Notify::new());
    fake.script_send(Some(Arc::clone(&gate)), Ok(7));
    harness
        .account()
        .send_message("1".to_owned().into(), "hello", None, None, false);
    let pending = expect_pending(&harness);
    fake.wait_for_send_calls(1);

    let copy = server_copy("1", pending.id.as_str(), "hello", 7);
    fake.script_fetch(
        None,
        Ok(vec![
            subscription("1", "general", rc_api::RoomType::Channel),
            subscription("2", "secret", rc_api::RoomType::Private),
        ]),
    );
    fake.script_history(None, page(vec![copy.clone(), message("b", 2)], false));
    fake.end_connection(0);

    harness.expect_events(vec![
        status("1", true, false, false, None),
        updated(2, copy),
        status("1", false, false, false, None),
    ]);
}

// SEND-002
#[test]
fn a_refused_token_on_a_send_expires_the_session() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    fake.script_send(None, Err(ApiError::Unauthorized));
    harness
        .account()
        .send_message("1".to_owned().into(), "hello", None, None, false);
    expect_pending(&harness);
    harness.expect_events(vec![
        session_event(logged_out(Some(LoginFailure::SessionExpired))),
        rooms_event(vec![]),
    ]);
    harness.expect_events(closed_events());
}

// SEND-001
#[test]
fn a_message_confirmed_while_the_room_loads_is_not_lost_to_the_page() {
    for by_echo in [true, false] {
        let fake = Fake::new();
        let harness = logged_in_harness(&fake);
        let history_gate = Arc::new(Notify::new());
        fake.script_history(
            Some(Arc::clone(&history_gate)),
            page(vec![message("b", 2), message("a", 1)], false),
        );
        let send_gate = Arc::new(Notify::new());
        fake.script_send(Some(Arc::clone(&send_gate)), Ok(7));
        harness.account().open_room("1".to_owned().into());
        harness.expect_events(vec![
            reset_event(Some("1"), Vec::new()),
            status("1", true, false, false, None),
        ]);
        harness
            .account()
            .send_message("1".to_owned().into(), "hello", None, None, false);
        let pending = expect_pending(&harness);
        fake.wait_for_send_calls(1);
        let copy = server_copy("1", pending.id.as_str(), "hello", 7);

        // The server confirms before the page comes, and the page was made before that.
        if by_echo {
            assert!(fake.push(0, Pushed::Message(copy.clone())));
        } else {
            send_gate.notify_one();
        }
        harness.expect_events(vec![updated(0, copy.clone())]);
        history_gate.notify_one();

        harness.expect_events(vec![
            Event::Timeline(vec![TimelineChange::Prepended(vec![
                message("a", 1),
                message("b", 2),
            ])]),
            status("1", false, false, false, None),
        ]);
    }
}

// SEND-001
#[test]
fn a_message_sent_while_the_room_loads_waits_for_the_page_behind_it() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    let gate = Arc::new(Notify::new());
    fake.script_history(
        Some(Arc::clone(&gate)),
        page(vec![message("b", 2), message("a", 1)], true),
    );
    harness.account().open_room("1".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("1"), Vec::new()),
        status("1", true, false, false, None),
    ]);
    let send_gate = Arc::new(Notify::new());
    fake.script_send(Some(Arc::clone(&send_gate)), Ok(7));
    harness
        .account()
        .send_message("1".to_owned().into(), "hello", None, None, false);
    expect_pending(&harness);

    gate.notify_one();

    harness.expect_events(vec![
        Event::Timeline(vec![TimelineChange::Inserted {
            index: 0,
            messages: vec![message("a", 1), message("b", 2)],
        }]),
        status("1", false, false, true, None),
    ]);
}
