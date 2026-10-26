// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Searching the server's directory, opening a person's direct message, previewing a channel and
//! joining it. The fake clock moves the pause before a search.

use super::*;

/// How long the search waits for the typing to stop.
const PAUSE: Duration = Duration::from_millis(300);
/// Long enough to say that something did not happen.
const QUIET: Duration = Duration::from_millis(150);

fn person(username: &str) -> Person {
    Person {
        username: username.to_owned(),
        name: None,
    }
}

fn people(usernames: &[&str]) -> Found<Person> {
    Found {
        total: u32::try_from(usernames.len()).unwrap(),
        items: usernames.iter().map(|name| person(name)).collect(),
    }
}

fn found_channel(id: &str, name: &str, members: u32) -> DirectoryChannel {
    DirectoryChannel {
        id: id.to_owned().into(),
        name: name.to_owned(),
        topic: String::new(),
        members,
    }
}

fn channels(entries: &[(&str, &str)]) -> Found<DirectoryChannel> {
    Found {
        total: u32::try_from(entries.len()).unwrap(),
        items: entries
            .iter()
            .map(|(id, name)| found_channel(id, name, 7))
            .collect(),
    }
}

fn ready(query: &str, people: Found<Person>, channels: Found<DirectoryChannel>) -> Directory {
    Directory {
        query: query.to_owned(),
        state: DirectoryState::Ready,
        people: people.items,
        people_total: people.total,
        channels: channels.items,
        channels_total: channels.total,
    }
}

/// The next `Directory` event, skipping the other events.
fn next_directory(harness: &Harness) -> Directory {
    loop {
        if let Event::Directory(directory) = harness.next_event() {
            return directory;
        }
    }
}

/// Checks that no `Directory` event comes for a little while, and drops the other events.
fn assert_no_directory(harness: &Harness) {
    while let Ok(event) = harness.recv_timeout(QUIET) {
        assert!(!matches!(event, Event::Directory(_)), "{event:?}");
    }
}

/// Lets the typing stop, and waits for the search to reach the server.
fn pause_and_wait_for_calls(fake: &Fake, harness: &Harness, calls: usize) {
    harness.clock.advance(PAUSE);
    fake.wait_for_directory_calls(calls);
}

fn action(action: DirectoryAction, state: ActionState) -> Event {
    Event::DirectoryAction { action, state }
}

fn open_direct_of(username: &str) -> DirectoryAction {
    DirectoryAction::OpenDirect(username.to_owned())
}

fn preview_of(id: &str) -> DirectoryAction {
    DirectoryAction::Preview(id.to_owned().into())
}

fn join_of(id: &str) -> DirectoryAction {
    DirectoryAction::Join(id.to_owned().into())
}

/// The next `DirectoryAction` event, skipping the other events.
fn next_action(harness: &Harness) -> (DirectoryAction, ActionState) {
    loop {
        if let Event::DirectoryAction { action, state } = harness.next_event() {
            return (action, state);
        }
    }
}

/// Checks that no `DirectoryAction` event comes for a little while, and drops the other events.
fn assert_no_action(harness: &Harness) {
    while let Ok(event) = harness.recv_timeout(QUIET) {
        assert!(!matches!(event, Event::DirectoryAction { .. }), "{event:?}");
    }
}

fn is_done(event: &Event) -> bool {
    matches!(
        event,
        Event::DirectoryAction {
            state: ActionState::Done | ActionState::Failed(_),
            ..
        }
    )
}

/// Checks that no `Rooms` or `Preview` event comes for a little while.
fn assert_no_rooms_or_preview(harness: &Harness) {
    while let Ok(event) = harness.recv_timeout(QUIET) {
        assert!(
            !matches!(event, Event::Rooms(_) | Event::Preview(_)),
            "{event:?}"
        );
    }
}

fn dm_subscription(id: &str, peer: &str) -> rc_api::Subscription {
    subscription(id, peer, rc_api::RoomType::Direct)
}

fn dm_room(id: &str, peer: &str) -> Room {
    room(id, peer, RoomKind::Direct)
}

fn channel_info(id: &str, name: &str) -> rc_api::RoomInfo {
    rc_api::RoomInfo {
        room_id: id.to_owned(),
        room_type: rc_api::RoomType::Channel,
        name: name.to_owned(),
        display_name: None,
        topic: "About".to_owned(),
        avatar_etag: None,
        last_message: Some(at(100)),
        created: None,
    }
}

/// The room a preview of `channel_info(id, name)` shows.
fn previewed(id: &str, name: &str) -> Room {
    Room {
        topic: "About".to_owned(),
        last_activity: at(100),
        ..room(id, name, RoomKind::Channel)
    }
}

/// The subscription the server makes when the user joins the channel `chan` (`visit`), with the
/// topic and the time of the last message `rooms.info` gives.
fn joined_subscription() -> rc_api::Subscription {
    rc_api::Subscription {
        topic: "About".to_owned(),
        last_message: Some(at(100)),
        ..subscription("chan", "visit", rc_api::RoomType::Channel)
    }
}

/// Previews the channel `chan` (`visit`) and takes the events up to its first page, which has `a`.
fn previewing(fake: &Fake, harness: &Harness) {
    fake.script_info(None, Ok(channel_info("chan", "visit")));
    fake.script_history(None, page(vec![message("a", 1)], true));
    harness.account().preview_channel("chan".to_owned().into());
    let events = harness.events_until(|event| {
        matches!(event, Event::TimelineStatus(status) if status.room.is_some() && !status.loading)
    });
    assert!(events.contains(&Event::Preview(Some(previewed("chan", "visit")))));
}

// ACT-008
#[test]
fn a_query_shows_searching_at_once_and_the_server_is_asked_after_the_pause() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    fake.script_directory(Ok(people(&["bob"])), Ok(channels(&[("c1", "bobs-chat")])));

    harness.account().search_directory("  bo ");

    assert_eq!(next_directory(&harness), Directory::searching("bo"));
    harness.clock.advance(Duration::from_millis(299));
    std::thread::sleep(QUIET);
    assert!(fake.state().directory_calls.is_empty());
    harness.clock.advance(Duration::from_millis(1));
    let calls = fake.wait_for_directory_calls(2);
    for kind in ["users", "channels"] {
        let call = DirectoryCall {
            kind,
            text: "bo".to_owned(),
            count: 10,
        };
        assert!(calls.contains(&call), "{calls:?}");
    }
    assert_eq!(
        next_directory(&harness),
        ready("bo", people(&["bob"]), channels(&[("c1", "bobs-chat")]))
    );
}

// ACT-008
#[test]
fn a_zero_pause_asks_the_server_at_once() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    let (client, events) = raw_client_with_debounce(&fake, jiff::SignedDuration::ZERO);
    let id = events.recv_timeout(TIMEOUT).unwrap().0;
    client
        .account(&id)
        .login("http://h", "alice", SecretString::from("secret"));
    loop {
        if matches!(events.recv_timeout(TIMEOUT).unwrap().1, Event::Rooms(_)) {
            break;
        }
    }

    client.account(&id).search_directory("bob");

    fake.wait_for_directory_calls(2);
}

/// A client whose directory search waits for `debounce`, logged out.
fn raw_client_with_debounce(
    fake: &Fake,
    debounce: jiff::SignedDuration,
) -> (Client, Receiver<(AccountId, Event)>) {
    let (tx, events) = mpsc::channel();
    let client = Client::with_tuning(
        fake.clone(),
        MemoryStore::default(),
        FakeCache::default(),
        FakeClock::default(),
        move |id, event| {
            let _ = tx.send((id, event));
        },
        Tuning {
            directory_debounce: debounce,
            ..Tuning::default()
        },
    )
    .unwrap();
    (client, events)
}

// ACT-008
#[test]
fn typing_on_starts_the_pause_again() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    fake.script_directory(Ok(people(&["bob"])), Ok(channels(&[])));

    harness.account().search_directory("b");
    assert_eq!(next_directory(&harness), Directory::searching("b"));
    harness.clock.advance(Duration::from_millis(200));
    harness.account().search_directory("bo");
    assert_eq!(next_directory(&harness), Directory::searching("bo"));
    harness.clock.advance(Duration::from_millis(200));
    std::thread::sleep(QUIET);
    assert!(fake.state().directory_calls.is_empty());
    harness.clock.advance(Duration::from_millis(100));

    let calls = fake.wait_for_directory_calls(2);
    assert!(calls.iter().all(|call| call.text == "bo"), "{calls:?}");
    assert_eq!(
        next_directory(&harness),
        ready("bo", people(&["bob"]), channels(&[]))
    );
}

// ACT-008
#[test]
fn a_newer_text_drops_the_answer_of_the_request_in_flight() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    let gate = Arc::new(Notify::new());
    fake.script_directory_gated(
        Some(gate.clone()),
        Ok(people(&["stale"])),
        Ok(channels(&[])),
    );
    fake.script_directory(Ok(people(&["fresh"])), Ok(channels(&[])));
    harness.account().search_directory("a");
    assert_eq!(next_directory(&harness), Directory::searching("a"));
    pause_and_wait_for_calls(&fake, &harness, 2);

    harness.account().search_directory("ab");
    assert_eq!(next_directory(&harness), Directory::searching("ab"));
    gate.notify_one();
    pause_and_wait_for_calls(&fake, &harness, 4);

    // The request of the first text was ended, so nothing of it is told.
    assert_eq!(
        next_directory(&harness),
        ready("ab", people(&["fresh"]), channels(&[]))
    );
}

// ACT-008
#[test]
fn a_blank_text_clears_the_search_and_cancels_the_pause() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    harness.account().search_directory("bob");
    assert_eq!(next_directory(&harness), Directory::searching("bob"));

    harness.account().search_directory("   ");

    assert_eq!(next_directory(&harness), Directory::cleared());
    harness.clock.advance(PAUSE);
    std::thread::sleep(QUIET);
    assert!(fake.state().directory_calls.is_empty());
    assert_no_directory(&harness);
}

// ACT-008
#[test]
fn a_blank_text_drops_the_answer_of_the_request_in_flight() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    let gate = Arc::new(Notify::new());
    fake.script_directory_gated(Some(gate.clone()), Ok(people(&["bob"])), Ok(channels(&[])));
    harness.account().search_directory("bob");
    assert_eq!(next_directory(&harness), Directory::searching("bob"));
    pause_and_wait_for_calls(&fake, &harness, 2);

    harness.account().search_directory("");
    assert_eq!(next_directory(&harness), Directory::cleared());
    gate.notify_one();

    assert_no_directory(&harness);
}

// ACT-008
#[test]
fn a_blank_text_with_nothing_searched_tells_nothing() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);

    harness.account().search_directory("");
    harness.account().search_directory("  ");

    assert_no_directory(&harness);
}

// ACT-008
#[test]
fn the_same_text_again_does_nothing() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    fake.script_directory(Ok(people(&["bob"])), Ok(channels(&[])));
    harness.account().search_directory("bob");
    assert_eq!(next_directory(&harness), Directory::searching("bob"));

    // While it waits, while it runs and once it is answered.
    harness.account().search_directory(" bob");
    pause_and_wait_for_calls(&fake, &harness, 2);
    harness.account().search_directory("bob ");
    next_directory(&harness);
    harness.account().search_directory("bob");

    assert_no_directory(&harness);
    harness.clock.advance(PAUSE);
    std::thread::sleep(QUIET);
    assert_eq!(fake.state().directory_calls.len(), 2);
}

// ACT-008
#[test]
fn a_failed_search_is_reported_and_the_same_text_again_retries_at_once() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    fake.script_directory(
        Err(ApiError::Unreachable("down".to_owned())),
        Ok(channels(&[])),
    );
    fake.script_directory(Ok(people(&["bob"])), Ok(channels(&[])));
    harness.account().search_directory("bob");
    assert_eq!(next_directory(&harness), Directory::searching("bob"));
    pause_and_wait_for_calls(&fake, &harness, 2);
    assert_eq!(
        next_directory(&harness),
        Directory {
            state: DirectoryState::Failed(DirectoryError::Unreachable),
            ..Directory::searching("bob")
        }
    );
    assert_eq!(
        DirectoryError::Unreachable.to_string(),
        "Can't reach the server's directory"
    );

    // No pause: the clock stays where it is.
    harness.account().search_directory("bob");

    assert_eq!(next_directory(&harness), Directory::searching("bob"));
    assert_eq!(
        next_directory(&harness),
        ready("bob", people(&["bob"]), channels(&[]))
    );
}

// ACT-008
#[test]
fn a_server_error_in_the_search_is_reported_with_its_text() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    fake.script_directory(Ok(people(&[])), Err(refused()));
    harness.account().search_directory("bob");
    next_directory(&harness);
    harness.clock.advance(PAUSE);

    let failed = next_directory(&harness);

    let DirectoryState::Failed(error) = failed.state else {
        panic!("not a failure");
    };
    assert_eq!(
        error.to_string(),
        "The server's directory reported an error: the server answered 400: error-action-not-allowed"
    );
}

// ACT-008
#[test]
fn an_expired_token_in_the_search_expires_the_session() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    fake.script_directory(Err(ApiError::Unauthorized), Ok(channels(&[])));
    harness.account().search_directory("bob");
    next_directory(&harness);

    harness.clock.advance(PAUSE);

    let events = harness.events_until(|event| {
        matches!(
            event,
            Event::Session(SessionState::LoggedOut { failure: Some(_) })
        )
    });
    assert!(!events.iter().any(|event| matches!(
        event,
        Event::Directory(Directory {
            state: DirectoryState::Failed(_),
            ..
        })
    )));
    // What the session showed is cleared, the search too.
    assert_eq!(next_directory(&harness), Directory::cleared());
}

// ACT-008
#[test]
fn the_avatars_of_the_people_and_the_channels_found_are_wanted() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    fake.script_directory(Ok(people(&["bob"])), Ok(channels(&[("c1", "chat")])));
    harness.account().search_directory("b");
    next_directory(&harness);
    harness.clock.advance(PAUSE);

    let owners = fake.wait_for_avatar_calls(2);

    assert!(owners.contains(&AvatarOwner::User("bob".to_owned())));
    assert!(owners.contains(&AvatarOwner::Room("c1".to_owned().into())));
}

// ACT-008
#[test]
fn a_logout_clears_the_search_and_drops_the_answer_of_the_request_in_flight() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    let gate = Arc::new(Notify::new());
    fake.script_directory_gated(Some(gate.clone()), Ok(people(&["bob"])), Ok(channels(&[])));
    harness.account().search_directory("bob");
    next_directory(&harness);
    pause_and_wait_for_calls(&fake, &harness, 2);

    harness.logout();
    let events = harness.events_until(|event| matches!(event, Event::Directory(_)));
    assert_eq!(events.last(), Some(&Event::Directory(Directory::cleared())));
    gate.notify_one();

    assert_no_directory(&harness);
}

// ACT-008
#[test]
fn a_search_while_logged_out_is_ignored() {
    let fake = Fake::new();
    let harness = Harness::new(&fake);

    harness.account().search_directory("bob");
    harness.clock.advance(PAUSE);

    assert_no_directory(&harness);
    assert!(fake.state().directory_calls.is_empty());
}

// ACT-007
#[test]
fn opening_a_direct_message_lists_it_and_opens_it() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    fake.script_direct(None, Ok("dm".to_owned().into()));
    fake.script_own_subscription("dm", Ok(Some(dm_subscription("dm", "bob"))));
    fake.script_history(None, page(Vec::new(), true));

    harness.account().open_direct("bob");

    let events = harness.events_until(is_done);
    assert_eq!(
        events,
        vec![
            action(open_direct_of("bob"), ActionState::Working),
            rooms_change(RoomChange::Inserted {
                index: 0,
                item: dm_room("dm", "bob"),
            }),
            reset_event(Some("dm"), Vec::new()),
            status("dm", true, false, false, None),
            action(open_direct_of("bob"), ActionState::Done),
        ]
    );
    assert_eq!(fake.state().direct_calls, ["bob"]);
}

// ACT-007
#[test]
fn the_push_of_the_new_direct_message_before_the_answer_changes_nothing() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    let gate = Arc::new(Notify::new());
    fake.script_direct(Some(gate.clone()), Ok("dm".to_owned().into()));
    fake.script_own_subscription("dm", Ok(Some(dm_subscription("dm", "bob"))));
    harness.account().open_direct("bob");
    assert_eq!(
        next_action(&harness),
        (open_direct_of("bob"), ActionState::Working)
    );

    assert!(fake.push(
        0,
        pushed_subscription("dm", "bob", rc_api::RoomType::Direct)
    ));
    harness.expect_events(vec![rooms_change(RoomChange::Inserted {
        index: 0,
        item: dm_room("dm", "bob"),
    })]);
    gate.notify_one();

    // Listed once, opened, done.
    let events = harness.events_until(is_done);
    assert!(
        !events.iter().any(|event| matches!(event, Event::Rooms(_))),
        "{events:?}"
    );
    assert!(events.contains(&reset_event(Some("dm"), Vec::new())));
}

// ACT-007
#[test]
fn the_push_of_the_new_direct_message_after_the_answer_changes_nothing() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    fake.script_direct(None, Ok("dm".to_owned().into()));
    fake.script_own_subscription("dm", Ok(Some(dm_subscription("dm", "bob"))));
    harness.account().open_direct("bob");
    harness.events_until(is_done);
    harness.expect_events(vec![status("dm", false, false, true, None)]);

    assert!(fake.push(
        0,
        pushed_subscription("dm", "bob", rc_api::RoomType::Direct)
    ));
    flush_pushed(&fake, &harness, "dm");

    // Nothing but the flush came: the room was listed already.
    assert_no_rooms_or_preview(&harness);
}

// ACT-007
#[test]
fn a_direct_message_that_is_listed_already_is_just_opened() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    fake.script_fetch(None, Ok(vec![dm_subscription("dm", "bob")]));
    let harness = Harness::new(&fake);
    harness.login("http://h", "alice");
    harness.events_until(|event| matches!(event, Event::Rooms(_)));
    fake.script_direct(None, Ok("dm".to_owned().into()));
    fake.script_own_subscription("dm", Ok(Some(dm_subscription("dm", "bob"))));

    harness.account().open_direct("bob");

    let events = harness.events_until(is_done);
    assert!(
        !events.iter().any(|event| matches!(event, Event::Rooms(_))),
        "{events:?}"
    );
    assert!(events.contains(&reset_event(Some("dm"), Vec::new())));
}

// ACT-007
#[test]
fn a_direct_message_that_went_quiet_comes_back_when_it_is_opened() {
    let day: i64 = 24 * 3600;
    let fake = Fake::new();
    let harness = Harness::new(&fake);
    harness.clock.set(at(30 * day));
    let old = rc_api::Subscription {
        last_message: Some(at(day)),
        ..dm_subscription("dm", "bob")
    };
    fake.script_login(None, Ok(()));
    fake.script_fetch(None, Ok(vec![old.clone()]));
    harness.login("http://h", "alice");
    let events = harness.events_until(|event| matches!(event, Event::QuietRooms(_)));
    assert!(events.contains(&rooms_event(Vec::new())), "{events:?}");
    fake.script_direct(None, Ok("dm".to_owned().into()));
    fake.script_own_subscription("dm", Ok(Some(old)));

    harness.account().open_direct("bob");

    let events = harness.events_until(is_done);
    let inserted = events.iter().any(|event| {
        matches!(event, Event::Rooms(changes)
            if matches!(&changes[..], [RoomChange::Inserted { item, .. }] if item.id == "dm".to_owned().into()))
    });
    assert!(inserted, "{events:?}");
    assert!(events.contains(&Event::QuietRooms(Vec::new())));
    assert!(events.contains(&reset_event(Some("dm"), Vec::new())));
}

// ACT-007
#[test]
fn a_failed_direct_message_is_reported_and_nothing_opens() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    fake.script_direct(None, Err(ApiError::Unreachable("down".to_owned())));
    fake.script_direct(None, Err(refused()));

    harness.account().open_direct("bob");
    let events = harness.events_until(is_done);

    assert_eq!(
        events,
        vec![
            action(open_direct_of("bob"), ActionState::Working),
            action(
                open_direct_of("bob"),
                ActionState::Failed(RoomActionError::Unreachable)
            ),
        ]
    );
    harness.account().open_direct("bob");
    let (_, state) = loop {
        let (which, state) = next_action(&harness);
        if state != ActionState::Working {
            break (which, state);
        }
    };
    let ActionState::Failed(error) = state else {
        panic!("not a failure");
    };
    assert_eq!(
        error.to_string(),
        "The server reported an error: the server answered 400: error-action-not-allowed"
    );
}

// ACT-007
#[test]
fn a_second_action_while_one_is_under_way_is_ignored() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    let gate = Arc::new(Notify::new());
    fake.script_direct(Some(gate.clone()), Ok("dm".to_owned().into()));
    fake.script_own_subscription("dm", Ok(Some(dm_subscription("dm", "bob"))));
    harness.account().open_direct("bob");
    next_action(&harness);

    harness.account().open_direct("carol");
    harness.account().join_channel("chan".to_owned().into());
    harness.account().preview_channel("chan".to_owned().into());
    std::thread::sleep(QUIET);
    gate.notify_one();

    assert_eq!(
        next_action(&harness),
        (open_direct_of("bob"), ActionState::Done)
    );
    assert_eq!(fake.state().direct_calls, ["bob"]);
    assert!(fake.state().join_calls.is_empty());
    assert!(fake.state().info_calls.is_empty());
}

// ACT-007
#[test]
fn an_expired_token_on_a_direct_message_expires_the_session() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    fake.script_direct(None, Err(ApiError::Unauthorized));

    harness.account().open_direct("bob");

    let events = harness.events_until(|event| {
        matches!(
            event,
            Event::Session(SessionState::LoggedOut { failure: Some(_) })
        )
    });
    assert!(!events.iter().any(is_done), "{events:?}");
}

// ACT-007
#[test]
fn a_logout_drops_the_answer_of_a_direct_message_in_flight() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    let gate = Arc::new(Notify::new());
    fake.script_direct(Some(gate.clone()), Ok("dm".to_owned().into()));
    fake.script_own_subscription("dm", Ok(Some(dm_subscription("dm", "bob"))));
    harness.account().open_direct("bob");
    next_action(&harness);
    harness.logout();
    // The rooms go with the session, after the action is ended.
    harness.events_until(|event| matches!(event, Event::Rooms(_)));

    gate.notify_one();

    assert_no_action(&harness);
    assert!(fake.state().subscription_calls.is_empty());
}

// ACT-007
#[test]
fn a_direct_message_the_server_does_not_list_is_an_error() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    fake.script_direct(None, Ok("dm".to_owned().into()));

    harness.account().open_direct("bob");

    let (_, state) = loop {
        let (which, state) = next_action(&harness);
        if state != ActionState::Working {
            break (which, state);
        }
    };
    assert!(
        matches!(state, ActionState::Failed(RoomActionError::Server(_))),
        "{state:?}"
    );
}

// ACT-008
#[test]
fn previewing_a_channel_opens_it_without_listing_it() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    fake.script_info(None, Ok(channel_info("chan", "visit")));
    fake.script_history(None, page(vec![message("a", 1)], true));

    harness.account().preview_channel("chan".to_owned().into());

    let events = harness.events_until(is_done);
    assert_eq!(
        events,
        vec![
            action(preview_of("chan"), ActionState::Working),
            Event::Preview(Some(previewed("chan", "visit"))),
            reset_event(Some("chan"), Vec::new()),
            status("chan", true, false, false, None),
            action(preview_of("chan"), ActionState::Done),
        ]
    );
    harness.expect_events(vec![
        reset_event(Some("chan"), vec![message("a", 1)]),
        status("chan", false, false, true, None),
    ]);
    // The channel is followed for deletions and typing, and not marked read: the user has no
    // subscription to it.
    assert_eq!(
        fake.wait_for_watched(0, 1),
        [Some("chan".to_owned().into())]
    );
    assert!(fake.state().mark_read_calls.is_empty());
    assert_eq!(fake.state().history_calls[0].kind, RoomKind::Channel);
}

// ACT-008
#[test]
fn a_previewed_channel_gets_live_messages() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    previewing(&fake, &harness);

    assert!(fake.push(0, pushed_message("chan", "b", 2)));

    let live = Message {
        room_id: "chan".to_owned().into(),
        ..message("b", 2)
    };
    harness.expect_events(vec![Event::Timeline(vec![TimelineChange::Appended(vec![
        live,
    ])])]);
}

// ACT-008
#[test]
fn a_channel_of_the_room_list_is_just_opened() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);

    harness.account().preview_channel("1".to_owned().into());

    harness.expect_events(vec![
        reset_event(Some("1"), Vec::new()),
        status("1", true, false, false, None),
    ]);
    assert!(fake.state().info_calls.is_empty());
    assert_no_action(&harness);
}

// ACT-008
#[test]
fn previewing_the_channel_that_is_previewed_opens_it_again_without_asking() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    previewing(&fake, &harness);

    harness.account().preview_channel("chan".to_owned().into());

    // It is open and loaded: nothing happens.
    assert_no_action(&harness);
    assert_eq!(fake.state().info_calls.len(), 1);
}

// ACT-008
#[test]
fn opening_another_room_ends_the_preview_and_the_user_stays_out() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    previewing(&fake, &harness);
    fake.script_history(None, page(Vec::new(), true));

    harness.account().open_room("1".to_owned().into());

    let events = harness.events_until(|event| {
        matches!(event, Event::TimelineStatus(status) if status.room == Some("1".to_owned().into()))
    });
    assert_eq!(events[0], Event::Preview(None));
    assert!(events.contains(&reset_event(Some("1"), Vec::new())));
    assert!(fake.state().join_calls.is_empty());
    assert_no_rooms_or_preview(&harness);
}

// ACT-008
#[test]
fn opening_a_room_cancels_a_preview_that_is_still_on_its_way() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    let gate = Arc::new(Notify::new());
    fake.script_info(Some(gate.clone()), Ok(channel_info("chan", "visit")));
    harness.account().preview_channel("chan".to_owned().into());
    assert_eq!(
        next_action(&harness),
        (preview_of("chan"), ActionState::Working)
    );

    harness.account().open_room("1".to_owned().into());
    assert_eq!(
        next_action(&harness),
        (preview_of("chan"), ActionState::Done)
    );
    gate.notify_one();

    // The room that was opened stays.
    assert_no_rooms_or_preview(&harness);
}

// ACT-008
#[test]
fn a_preview_that_fails_is_reported_and_nothing_opens() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    fake.script_info(None, Err(refused()));

    harness.account().preview_channel("chan".to_owned().into());

    let events = harness.events_until(is_done);
    assert_eq!(events.len(), 2, "{events:?}");
    assert!(matches!(
        &events[1],
        Event::DirectoryAction {
            state: ActionState::Failed(RoomActionError::Server(_)),
            ..
        }
    ));
}

// ACT-008
#[test]
fn a_channel_that_is_a_private_group_is_not_previewed() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    let mut info = channel_info("chan", "visit");
    info.room_type = rc_api::RoomType::Private;
    fake.script_info(None, Ok(info));

    harness.account().preview_channel("chan".to_owned().into());

    let events = harness.events_until(is_done);
    assert!(
        matches!(
            &events[1],
            Event::DirectoryAction {
                state: ActionState::Failed(_),
                ..
            }
        ),
        "{events:?}"
    );
}

// ACT-008
#[test]
fn an_expired_token_on_a_preview_expires_the_session() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    fake.script_info(None, Err(ApiError::Unauthorized));

    harness.account().preview_channel("chan".to_owned().into());

    harness.events_until(|event| {
        matches!(
            event,
            Event::Session(SessionState::LoggedOut { failure: Some(_) })
        )
    });
}

// ACT-008
#[test]
fn joining_a_previewed_channel_lists_it_keeps_it_open_and_ends_the_preview() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    previewing(&fake, &harness);
    fake.script_own_subscription("chan", Ok(Some(joined_subscription())));

    harness.account().join_channel("chan".to_owned().into());

    let events = harness.events_until(is_done);
    let listed = Room {
        topic: "About".to_owned(),
        last_activity: at(100),
        ..room("chan", "visit", RoomKind::Channel)
    };
    // The timeline is not reset: no `Timeline` event at all.
    assert_eq!(
        events,
        vec![
            action(join_of("chan"), ActionState::Working),
            rooms_change(RoomChange::Inserted {
                index: 0,
                item: listed,
            }),
            Event::Preview(None),
            action(join_of("chan"), ActionState::Done),
        ]
    );
    assert_eq!(fake.state().join_calls, ["chan"]);
    // It is a room of the user now: the new messages are read at once.
    assert!(fake.push(0, pushed_message("chan", "b", 2)));
    let live = Message {
        room_id: "chan".to_owned().into(),
        ..message("b", 2)
    };
    harness.expect_events(vec![Event::Timeline(vec![TimelineChange::Appended(vec![
        live,
    ])])]);
}

// ACT-008
#[test]
fn the_push_of_the_joined_channel_before_the_answer_ends_the_preview_once() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    previewing(&fake, &harness);
    let gate = Arc::new(Notify::new());
    fake.script_join(Some(gate.clone()), Ok(()));
    fake.script_own_subscription("chan", Ok(Some(joined_subscription())));
    harness.account().join_channel("chan".to_owned().into());
    next_action(&harness);

    assert!(fake.push(0, Pushed::Subscription(joined_subscription())));
    let events = harness.events_until(|event| matches!(event, Event::Preview(None)));
    assert!(matches!(&events[0], Event::Rooms(_)), "{events:?}");
    gate.notify_one();

    assert_eq!(next_action(&harness), (join_of("chan"), ActionState::Done));
    assert_no_rooms_or_preview(&harness);
}

// ACT-008
#[test]
fn a_join_that_fails_is_reported_and_the_preview_stays() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    previewing(&fake, &harness);
    fake.script_join(None, Err(ApiError::Unreachable("down".to_owned())));

    harness.account().join_channel("chan".to_owned().into());

    assert_eq!(
        next_action(&harness),
        (join_of("chan"), ActionState::Working)
    );
    assert_eq!(
        next_action(&harness),
        (
            join_of("chan"),
            ActionState::Failed(RoomActionError::Unreachable)
        )
    );
    assert_no_rooms_or_preview(&harness);
}

// ACT-008
#[test]
fn joining_a_channel_that_is_listed_already_is_ignored() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);

    harness.account().join_channel("1".to_owned().into());

    assert_no_action(&harness);
    assert!(fake.state().join_calls.is_empty());
}

// ACT-008
#[test]
fn an_expired_token_on_a_join_expires_the_session() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    fake.script_join(None, Err(ApiError::Unauthorized));

    harness.account().join_channel("chan".to_owned().into());

    harness.events_until(|event| {
        matches!(
            event,
            Event::Session(SessionState::LoggedOut { failure: Some(_) })
        )
    });
}

// ACT-008
#[test]
fn a_logout_ends_the_preview_and_drops_the_answer_of_a_join_in_flight() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    previewing(&fake, &harness);
    let gate = Arc::new(Notify::new());
    fake.script_join(Some(gate.clone()), Ok(()));
    harness.account().join_channel("chan".to_owned().into());
    next_action(&harness);

    harness.logout();
    let events = harness.events_until(|event| *event == Event::Preview(None));
    assert!(
        events
            .iter()
            .any(|event| matches!(event, Event::Session(_)))
    );
    gate.notify_one();

    assert_no_action(&harness);
    assert!(fake.state().subscription_calls.is_empty());
}

// ACT-008
#[test]
fn a_room_of_the_list_going_away_does_not_touch_a_preview() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    previewing(&fake, &harness);

    assert!(fake.push(0, removed("2")));
    harness.expect_events(vec![Event::Rooms(vec![RoomChange::Removed { index: 1 }])]);
    flush_pushed(&fake, &harness, "chan");

    assert_no_rooms_or_preview(&harness);
}

// ACT-008
#[test]
fn a_previewed_channel_stays_open_through_a_reconnect() {
    let fake = Fake::new();
    let harness = logged_in_harness_with(&fake, fast());
    previewing(&fake, &harness);
    fake.script_history(None, page(vec![message("a", 1), message("b", 2)], true));

    fake.end_connection(0);
    fake.wait_for_connections(2);

    // The newest messages are fetched again, and the room is not closed for lacking a subscription.
    let calls = fake.wait_for_history_calls(2);
    assert_eq!(calls[1].room, "chan");
    assert_eq!(
        fake.wait_for_watched(1, 1),
        [Some("chan".to_owned().into())]
    );
    let events = harness.events_until(
        |event| matches!(event, Event::TimelineStatus(s) if !s.loading && s.room.is_some()),
    );
    assert!(!events.contains(&Event::Preview(None)), "{events:?}");
}
