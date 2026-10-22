// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Quoting messages.

use super::*;

/// What quoting message `id` of the first room, which `alice` wrote at `secs`, gives. `room_path`
/// is the room's part of the link.
fn quote_of(site: &str, room_path: &str, id: &str, secs: i64) -> Quote {
    Quote {
        message: Some(id.to_owned().into()),
        link: format!("{site}/{room_path}?msg={id}"),
        author: "alice".to_owned(),
        text: id.into(),
        ts: Some(jiff::Timestamp::from_second(secs).unwrap()),
    }
}

/// Logged in to a server that says its `Site_Url`, which is not the address of the login.
fn logged_in_with_site_url(fake: &Fake) -> Harness {
    fake.state().site_url = Some(Ok(Some(SITE.to_owned())));
    logged_in_harness(fake)
}

fn quote_send(harness: &Harness, room: &str, text: &str, quoted: &str) {
    harness.account().send_message(
        room.to_owned().into(),
        text,
        Some(quoted.to_owned().into()),
        None,
        false,
    );
}

// ACT-005
#[test]
fn a_quote_shows_at_once_and_goes_out_as_a_link_made_from_the_site_url() {
    let fake = Fake::new();
    let harness = logged_in_with_site_url(&fake);
    open_first_room(&fake, &harness, false);
    fake.script_send(None, Ok(7));

    quote_send(&harness, "1", "  hello\n", "b");

    let pending = expect_pending(&harness);
    assert_eq!(pending.kind, MessageKind::Text("hello".into()));
    assert_eq!(pending.quotes, [quote_of(SITE, "channel/general", "b", 2)]);
    // The host is the site's, not the login's (`h`), or the server makes no quote.
    assert_eq!(
        fake.wait_for_send_calls(1),
        [SendCall {
            room: "1".into(),
            id: pending.id.to_string(),
            text: "[ ](http://site:3000/channel/general?msg=b) hello".into(),
            thread: None,
        }]
    );
    // The server's copy replaces the local one.
    let Event::Timeline(changes) = harness.next_event() else {
        panic!("not a timeline event");
    };
    let [TimelineChange::Updated { index: 2, message }] = changes.as_slice() else {
        panic!("not the update of the pending message: {changes:?}");
    };
    assert_eq!(message.id, pending.id);
    assert_eq!(message.delivery, Delivery::Sent);
}

// ACT-005
#[test]
fn a_quote_in_a_private_group_is_linked_by_the_name_of_the_group() {
    let fake = Fake::new();
    let harness = logged_in_with_site_url(&fake);
    let held = Message {
        room_id: "2".to_owned().into(),
        ..message("g", 1)
    };
    fake.script_history(None, page(vec![held.clone()], true));
    harness.account().open_room("2".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("2"), Vec::new()),
        status("2", true, false, false, None),
        reset_event(Some("2"), vec![held]),
        status("2", false, false, true, None),
    ]);

    quote_send(&harness, "2", "hello", "g");

    let pending = expect_pending(&harness);
    assert_eq!(pending.quotes, [quote_of(SITE, "group/secret", "g", 1)]);
}

// ACT-005
#[test]
fn a_quote_in_a_direct_message_is_linked_by_the_id_of_the_room() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    fake.script_fetch(
        None,
        Ok(vec![subscription("3", "bob", rc_api::RoomType::Direct)]),
    );
    let harness = Harness::with_backoff(
        &fake,
        MemoryStore::default(),
        Backoff::new(NEVER, NEVER, NEVER),
    );
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![room("3", "bob", RoomKind::Direct)]),
    ]);
    let held = Message {
        room_id: "3".to_owned().into(),
        ..message("d", 1)
    };
    fake.script_history(None, page(vec![held.clone()], true));
    harness.account().open_room("3".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("3"), Vec::new()),
        status("3", true, false, false, None),
        reset_event(Some("3"), vec![held]),
        status("3", false, false, true, None),
    ]);

    quote_send(&harness, "3", "hello", "d");

    let pending = expect_pending(&harness);
    // The server says no site URL: the login's address does.
    assert_eq!(pending.quotes, [quote_of("http://h", "direct/3", "d", 1)]);
}

// ACT-005
#[test]
fn without_a_site_url_the_link_is_made_from_the_address_of_the_login() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);

    quote_send(&harness, "1", "hello", "a");

    let pending = expect_pending(&harness);
    assert_eq!(
        pending.quotes,
        [quote_of("http://h", "channel/general", "a", 1)]
    );
    assert_eq!(
        fake.wait_for_send_calls(1)[0].text,
        "[ ](http://h/channel/general?msg=a) hello"
    );
}

// ACT-005
#[test]
fn a_server_that_has_no_site_url_or_fails_to_say_falls_back_to_the_address_of_the_login() {
    for answer in [Ok(None), Err(ApiError::Unreachable("reset".into()))] {
        let fake = Fake::new();
        fake.state().site_url = Some(answer);
        // The login succeeds all the same: it is logged_in_harness that expects it to.
        let harness = logged_in_harness(&fake);
        open_first_room(&fake, &harness, false);

        quote_send(&harness, "1", "hello", "a");

        let pending = expect_pending(&harness);
        assert_eq!(pending.quotes[0].link, "http://h/channel/general?msg=a");
    }
}

// ACT-005
#[test]
fn each_login_asks_the_server_for_its_site_url() {
    let fake = Fake::new();
    fake.state().site_url = Some(Ok(Some(SITE.to_owned())));
    for _ in 0..2 {
        fake.script_login(None, Ok(()));
        fake.script_fetch(
            None,
            Ok(vec![subscription(
                "1",
                "general",
                rc_api::RoomType::Channel,
            )]),
        );
    }
    let harness = Harness::with_backoff(
        &fake,
        MemoryStore::default(),
        Backoff::new(NEVER, NEVER, NEVER),
    );
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![room("1", "general", RoomKind::Channel)]),
    ]);
    harness.logout();
    harness.expect_events(vec![session_event(logged_out(None)), rooms_event(vec![])]);
    // The next server has none to tell.
    fake.state().site_url = None;
    harness.login("http://h2", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h2", "alice")),
        rooms_event(vec![room("1", "general", RoomKind::Channel)]),
    ]);
    open_first_room(&fake, &harness, false);

    quote_send(&harness, "1", "hello", "a");

    let pending = expect_pending(&harness);
    assert_eq!(pending.quotes[0].link, "http://h2/channel/general?msg=a");
}

// ACT-005
#[test]
fn a_resumed_session_knows_the_site_url_too() {
    let fake = Fake::new();
    fake.state().site_url = Some(Ok(Some(SITE.to_owned())));
    fake.script_fetch(
        None,
        Ok(vec![subscription(
            "1",
            "general",
            rc_api::RoomType::Channel,
        )]),
    );
    let harness = Harness::with_store(&fake, store_with("http://h"));
    harness.expect_events(vec![
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![room("1", "general", RoomKind::Channel)]),
    ]);
    open_first_room(&fake, &harness, false);

    quote_send(&harness, "1", "hello", "a");

    let pending = expect_pending(&harness);
    assert_eq!(pending.quotes, [quote_of(SITE, "channel/general", "a", 1)]);
}

// ACT-005
#[test]
fn only_a_sent_text_message_of_the_open_room_can_be_quoted() {
    let fake = Fake::new();
    let harness = logged_in_with_site_url(&fake);
    open_first_room(&fake, &harness, false);
    // None of the sends is answered, so that the messages stay as they were shown.
    let gate = Arc::new(Notify::new());
    for _ in 0..5 {
        fake.script_send(Some(Arc::clone(&gate)), Ok(7));
    }
    harness
        .account()
        .send_message("1".to_owned().into(), "unsent", None, None, false);
    let unsent = expect_pending(&harness);
    let joined = Message {
        kind: MessageKind::System(SystemEvent::Joined),
        ..message("j", 3)
    };
    assert!(fake.push(0, Pushed::Message(joined.clone())));
    // It sorts before the unsent message.
    harness.expect_events(vec![Event::Timeline(vec![TimelineChange::Inserted {
        index: 2,
        messages: vec![joined],
    }])]);

    // An unsent message, a system message, one that isn't held, and one of another room.
    for quoted in [unsent.id.as_str(), "j", "nope"] {
        quote_send(&harness, "1", "hello", quoted);
        let pending = expect_pending(&harness);
        assert_eq!(pending.quotes, [], "{quoted}");
    }
    quote_send(&harness, "2", "hello", "b");

    // Nothing but the text went out, in each case (the last is for the room that is not open,
    // which shows nothing). The sends run as tasks of their own, so they may come in any order.
    let calls = fake.wait_for_send_calls(5);
    let mut texts: Vec<&str> = calls.iter().map(|call| call.text.as_str()).collect();
    texts.sort_unstable();
    assert_eq!(texts, ["hello", "hello", "hello", "hello", "unsent"]);
}

// ACT-005
#[test]
fn a_retry_sends_the_link_of_the_quote_again() {
    let fake = Fake::new();
    let harness = logged_in_with_site_url(&fake);
    open_first_room(&fake, &harness, false);
    fake.script_send(
        None,
        Err(ApiError::Server {
            status: 400,
            message: "no".into(),
        }),
    );
    quote_send(&harness, "1", "hello", "a");
    let pending = expect_pending(&harness);
    harness.expect_events(vec![updated(2, with_delivery(&pending, Delivery::Failed))]);

    fake.script_send(None, Ok(9));
    harness.account().retry_message(pending.id.clone());
    harness.expect_events(vec![updated(2, pending.clone())]);

    let calls = fake.wait_for_send_calls(2);
    assert_eq!(calls[0], calls[1]);
    assert_eq!(
        calls[1].text,
        "[ ](http://site:3000/channel/general?msg=a) hello"
    );
}

/// A message of the user's in the first room, from the server, that quotes `a` and says `text`.
fn reply_to_a(id: &str, text: &str, secs: i64) -> Message {
    Message {
        quotes: vec![quote_of(SITE, "channel/general", "a", 1)],
        ..server_copy("1", id, text, secs)
    }
}

/// The room's history with a reply to `a` in it, opened.
fn open_with_reply(fake: &Fake, harness: &Harness) {
    let reply = reply_to_a("r", "reply", 3);
    fake.script_history(None, page(vec![reply.clone(), message("a", 1)], true));
    harness.account().open_room("1".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("1"), Vec::new()),
        status("1", true, false, false, None),
        reset_event(Some("1"), vec![message("a", 1), reply]),
        status("1", false, false, true, None),
    ]);
}

// ACT-005
#[test]
fn an_edit_sends_the_links_of_the_quotes_with_the_new_text() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_with_reply(&fake, &harness);
    fake.script_edit(None, Ok(5));

    harness
        .account()
        .edit_message("r".to_owned().into(), "  changed\n");

    // The server keeps the quote only if the link stays in the text.
    let Event::Timeline(_) = harness.next_event() else {
        panic!("not a timeline event");
    };
    harness.expect_events(vec![action_event("r", MessageAction::Edit, Ok(()))]);
    assert_eq!(
        fake.state().edit_calls,
        [EditCall {
            room: "1".into(),
            id: "r".into(),
            text: "[ ](http://site:3000/channel/general?msg=a) changed".into()
        }]
    );
}

// ACT-005
#[test]
fn an_edit_to_the_text_that_is_shown_asks_nothing_though_the_server_text_has_links() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_with_reply(&fake, &harness);

    harness
        .account()
        .edit_message("r".to_owned().into(), "reply");

    harness.expect_events(vec![action_event("r", MessageAction::Edit, Ok(()))]);
    assert!(fake.state().edit_calls.is_empty());
}
