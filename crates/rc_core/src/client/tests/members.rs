// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Searching a room's members for the mention popup. The fake clock moves the pause before a
//! search.

use super::*;

/// How long the search waits for the typing to stop.
const PAUSE: Duration = Duration::from_millis(300);
/// Long enough to say that something did not happen.
const QUIET: Duration = Duration::from_millis(150);

fn member(username: &str) -> Member {
    Member {
        id: format!("id-{username}"),
        username: username.to_owned(),
        name: None,
    }
}

fn members(usernames: &[&str], total: u32) -> Found<Member> {
    Found {
        total,
        items: usernames.iter().map(|name| member(name)).collect(),
    }
}

fn searching(room: &str, query: &str) -> Event {
    event(room, query, MemberSearchState::Searching)
}

fn found(room: &str, query: &str, usernames: &[&str], total: u32) -> Event {
    let members = members(usernames, total);
    let state = MemberSearchState::Found {
        members: members.items,
        total,
    };
    event(room, query, state)
}

fn event(room: &str, query: &str, state: MemberSearchState) -> Event {
    Event::MemberSearch(Some(MemberSearch {
        room: room.to_owned().into(),
        query: query.to_owned(),
        state,
    }))
}

/// The next `MemberSearch` event, skipping the other events.
fn next_search(harness: &Harness) -> Event {
    loop {
        let event = harness.next_event();
        if matches!(event, Event::MemberSearch(_)) {
            return event;
        }
    }
}

/// Checks that no `MemberSearch` event comes for a little while, and drops the other events.
fn assert_no_search(harness: &Harness) {
    while let Ok(event) = harness.recv_timeout(QUIET) {
        assert!(!matches!(event, Event::MemberSearch(_)), "{event:?}");
    }
}

fn search(harness: &Harness, room: &str, query: &str) {
    harness
        .account()
        .search_members(&room.to_owned().into(), query);
}

/// Lets the typing stop, and waits for the search to reach the server.
fn pause_and_wait_for_calls(fake: &Fake, harness: &Harness, calls: usize) {
    harness.clock.advance(PAUSE);
    fake.wait_for_member_calls(calls);
}

// ACT-013
#[test]
fn a_query_shows_searching_at_once_and_the_server_is_asked_after_the_pause() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    fake.script_members(None, Ok(members(&["bob", "bobby"], 9)));

    search(&harness, "1", "bo");

    assert_eq!(next_search(&harness), searching("1", "bo"));
    harness.clock.advance(Duration::from_millis(299));
    std::thread::sleep(QUIET);
    assert!(fake.state().member_calls.is_empty());
    harness.clock.advance(Duration::from_millis(1));
    let calls = fake.wait_for_member_calls(1);
    assert_eq!(
        calls,
        [MemberCall {
            room: "1".to_owned(),
            filter: "bo".to_owned(),
            count: 7,
        }]
    );
    assert_eq!(
        next_search(&harness),
        found("1", "bo", &["bob", "bobby"], 9)
    );
}

// ACT-013
#[test]
fn the_empty_query_lists_the_first_members_of_a_private_group() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    // The user is a member too, and so is counted out.
    fake.script_members(None, Ok(members(&["owner"], 2)));

    search(&harness, "2", "");

    assert_eq!(next_search(&harness), searching("2", ""));
    pause_and_wait_for_calls(&fake, &harness, 1);
    assert_eq!(next_search(&harness), found("2", "", &["owner"], 1));
}

// ACT-013
#[test]
fn a_zero_pause_asks_the_server_at_once() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
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
            members_debounce: jiff::SignedDuration::ZERO,
            ..Tuning::default()
        },
    )
    .unwrap();
    let id = events.recv_timeout(TIMEOUT).unwrap().0;
    client
        .account(&id)
        .login("http://h", "bob", SecretString::from("secret"));
    loop {
        if matches!(events.recv_timeout(TIMEOUT).unwrap().1, Event::Rooms(_)) {
            break;
        }
    }

    client
        .account(&id)
        .search_members(&"1".to_owned().into(), "bo");

    fake.wait_for_member_calls(1);
}

// ACT-013
#[test]
fn typing_on_starts_the_pause_again() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    fake.script_members(None, Ok(members(&["bob"], 1)));

    search(&harness, "1", "b");
    assert_eq!(next_search(&harness), searching("1", "b"));
    harness.clock.advance(Duration::from_millis(200));
    search(&harness, "1", "bo");
    assert_eq!(next_search(&harness), searching("1", "bo"));
    harness.clock.advance(Duration::from_millis(200));
    std::thread::sleep(QUIET);
    assert!(fake.state().member_calls.is_empty());
    harness.clock.advance(Duration::from_millis(100));

    let calls = fake.wait_for_member_calls(1);
    assert_eq!(calls[0].filter, "bo");
    assert_eq!(next_search(&harness), found("1", "bo", &["bob"], 1));
}

// ACT-013
#[test]
fn a_newer_query_drops_the_answer_of_the_request_in_flight() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    let gate = Arc::new(Notify::new());
    fake.script_members(Some(gate.clone()), Ok(members(&["stale"], 1)));
    fake.script_members(None, Ok(members(&["fresh"], 1)));
    search(&harness, "1", "b");
    assert_eq!(next_search(&harness), searching("1", "b"));
    pause_and_wait_for_calls(&fake, &harness, 1);

    search(&harness, "1", "bo");
    assert_eq!(next_search(&harness), searching("1", "bo"));
    gate.notify_one();
    pause_and_wait_for_calls(&fake, &harness, 2);

    assert_eq!(next_search(&harness), found("1", "bo", &["fresh"], 1));
}

// ACT-013
#[test]
fn another_room_drops_the_answer_of_the_request_in_flight() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    let gate = Arc::new(Notify::new());
    fake.script_members(Some(gate.clone()), Ok(members(&["stale"], 1)));
    fake.script_members(None, Ok(members(&["fresh"], 1)));
    search(&harness, "1", "b");
    assert_eq!(next_search(&harness), searching("1", "b"));
    pause_and_wait_for_calls(&fake, &harness, 1);

    // The same query, in the other room.
    search(&harness, "2", "b");
    assert_eq!(next_search(&harness), searching("2", "b"));
    gate.notify_one();
    pause_and_wait_for_calls(&fake, &harness, 2);

    assert_eq!(next_search(&harness), found("2", "b", &["fresh"], 1));
}

// ACT-013
#[test]
fn the_same_search_again_does_nothing() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    fake.script_members(None, Ok(members(&["bob"], 1)));
    search(&harness, "1", "bo");
    assert_eq!(next_search(&harness), searching("1", "bo"));
    pause_and_wait_for_calls(&fake, &harness, 1);
    assert_eq!(next_search(&harness), found("1", "bo", &["bob"], 1));

    search(&harness, "1", "bo");
    harness.clock.advance(PAUSE);

    assert_no_search(&harness);
    assert_eq!(fake.state().member_calls.len(), 1);
}

// ACT-013
#[test]
fn a_failure_is_told_and_the_same_search_again_retries_at_once() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    fake.script_members(None, Err(ApiError::Unreachable("down".to_owned())));
    fake.script_members(None, Ok(members(&["bob"], 1)));
    search(&harness, "1", "bo");
    assert_eq!(next_search(&harness), searching("1", "bo"));
    pause_and_wait_for_calls(&fake, &harness, 1);
    assert_eq!(
        next_search(&harness),
        event(
            "1",
            "bo",
            MemberSearchState::Failed(MemberError::Unreachable)
        )
    );

    search(&harness, "1", "bo");

    assert_eq!(next_search(&harness), searching("1", "bo"));
    fake.wait_for_member_calls(2);
    assert_eq!(next_search(&harness), found("1", "bo", &["bob"], 1));
}

// ACT-013
#[test]
fn another_error_carries_the_servers_message() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    fake.script_members(None, Err(refused()));
    search(&harness, "1", "bo");
    next_search(&harness);

    harness.clock.advance(PAUSE);

    let Event::MemberSearch(Some(MemberSearch {
        state: MemberSearchState::Failed(error),
        ..
    })) = next_search(&harness)
    else {
        panic!("not a failure");
    };
    assert!(matches!(error, MemberError::Server(_)), "{error:?}");
}

// ACT-013
#[test]
fn cancelling_ends_the_search_and_drops_the_answer_of_the_request_in_flight() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    let gate = Arc::new(Notify::new());
    fake.script_members(Some(gate.clone()), Ok(members(&["bob"], 1)));
    search(&harness, "1", "bo");
    next_search(&harness);
    pause_and_wait_for_calls(&fake, &harness, 1);

    harness.account().cancel_member_search();

    assert_eq!(next_search(&harness), Event::MemberSearch(None));
    gate.notify_one();
    assert_no_search(&harness);
}

// ACT-013
#[test]
fn cancelling_during_the_pause_asks_nothing() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    search(&harness, "1", "bo");
    next_search(&harness);

    harness.account().cancel_member_search();
    assert_eq!(next_search(&harness), Event::MemberSearch(None));
    harness.clock.advance(PAUSE);

    std::thread::sleep(QUIET);
    assert!(fake.state().member_calls.is_empty());
    // Cancelling again says nothing.
    harness.account().cancel_member_search();
    assert_no_search(&harness);
}

// ACT-013
#[test]
fn an_expired_token_in_the_search_expires_the_session() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    fake.script_members(None, Err(ApiError::Unauthorized));
    search(&harness, "1", "bo");
    next_search(&harness);

    harness.clock.advance(PAUSE);

    let events = harness.events_until(|event| {
        matches!(
            event,
            Event::Session(SessionState::LoggedOut { failure: Some(_) })
        )
    });
    assert!(!events.iter().any(|event| matches!(
        event,
        Event::MemberSearch(Some(MemberSearch {
            state: MemberSearchState::Failed(_),
            ..
        }))
    )));
    // What the session showed is cleared, the search too.
    assert_eq!(next_search(&harness), Event::MemberSearch(None));
}

// ACT-013
#[test]
fn a_logout_clears_the_search_and_drops_the_answer_of_the_request_in_flight() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    let gate = Arc::new(Notify::new());
    fake.script_members(Some(gate.clone()), Ok(members(&["bob"], 1)));
    search(&harness, "1", "bo");
    next_search(&harness);
    pause_and_wait_for_calls(&fake, &harness, 1);

    harness.logout();
    let events = harness.events_until(|event| matches!(event, Event::MemberSearch(_)));
    assert_eq!(events.last(), Some(&Event::MemberSearch(None)));
    gate.notify_one();

    assert_no_search(&harness);
}

// ACT-013
#[test]
fn a_search_while_logged_out_is_ignored() {
    let fake = Fake::new();
    let harness = Harness::new(&fake);

    search(&harness, "1", "bo");
    harness.clock.advance(PAUSE);

    assert_no_search(&harness);
    assert!(fake.state().member_calls.is_empty());
}

// ACT-013
#[test]
fn the_avatars_of_the_members_found_are_wanted() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    fake.script_members(None, Ok(members(&["bob", "bobby"], 2)));
    search(&harness, "1", "bo");
    next_search(&harness);
    harness.clock.advance(PAUSE);

    let owners = fake.wait_for_avatar_calls(2);

    assert!(owners.contains(&AvatarOwner::User("bob".to_owned())));
    assert!(owners.contains(&AvatarOwner::User("bobby".to_owned())));
}

/// A client logged in with a channel and a direct message with `bob`, called "Bob Roe".
fn harness_with_direct(fake: &Fake) -> Harness {
    fake.script_login(None, Ok(()));
    let direct = rc_api::Subscription {
        display_name: Some("Bob Roe".to_owned()),
        ..subscription("3", "bob", rc_api::RoomType::Direct)
    };
    fake.script_fetch(
        None,
        Ok(vec![
            subscription("1", "general", rc_api::RoomType::Channel),
            direct,
        ]),
    );
    let harness = Harness::new(fake);
    harness.login("http://h", "bob");
    harness.events_until(|event| matches!(event, Event::Rooms(_)));
    harness
}

// ACT-013
#[test]
fn a_direct_message_is_answered_with_the_other_person_and_no_request() {
    let fake = Fake::new();
    let harness = harness_with_direct(&fake);

    search(&harness, "3", "ro");

    let expected = Member {
        id: String::new(),
        username: "bob".to_owned(),
        name: Some("Bob Roe".to_owned()),
    };
    assert_eq!(
        next_search(&harness),
        event(
            "3",
            "ro",
            MemberSearchState::Found {
                members: vec![expected],
                total: 1,
            }
        )
    );
    search(&harness, "3", "zz");
    assert_eq!(next_search(&harness), found("3", "zz", &[], 0));
    harness.clock.advance(PAUSE);
    std::thread::sleep(QUIET);
    assert!(fake.state().member_calls.is_empty());
}

// ACT-013
#[test]
fn a_direct_message_answer_replaces_a_request_in_flight() {
    let fake = Fake::new();
    let harness = harness_with_direct(&fake);
    let gate = Arc::new(Notify::new());
    fake.script_members(Some(gate.clone()), Ok(members(&["stale"], 1)));
    search(&harness, "1", "b");
    next_search(&harness);
    pause_and_wait_for_calls(&fake, &harness, 1);

    search(&harness, "3", "b");

    let Event::MemberSearch(Some(answer)) = next_search(&harness) else {
        panic!("no search");
    };
    assert_eq!(answer.room.as_str(), "3");
    assert!(matches!(
        answer.state,
        MemberSearchState::Found { total: 1, .. }
    ));
    gate.notify_one();
    assert_no_search(&harness);
}

// ACT-013
#[test]
fn the_user_is_left_out_of_the_page_and_the_total_loses_one_and_the_page_still_fills() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    // The user, "alice", is among the seven asked for.
    let page = ["u0", "alice", "u2", "u3", "u4", "u5", "u6"];
    fake.script_members(None, Ok(members(&page, 12)));
    search(&harness, "1", "");
    next_search(&harness);

    harness.clock.advance(PAUSE);

    assert_eq!(
        next_search(&harness),
        found("1", "", &["u0", "u2", "u3", "u4", "u5", "u6"], 11)
    );
}

// ACT-013
#[test]
fn a_user_beyond_the_page_who_matches_still_comes_off_the_total() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    // Seven without the user: the sixth row is the last, the user is further down.
    let page = ["a0", "a1", "a2", "a3", "a4", "a5", "a6"];
    fake.script_members(None, Ok(members(&page, 20)));
    search(&harness, "1", "AL");
    next_search(&harness);

    harness.clock.advance(PAUSE);

    assert_eq!(
        next_search(&harness),
        found("1", "AL", &["a0", "a1", "a2", "a3", "a4", "a5"], 19)
    );
}

// ACT-013
#[test]
fn a_user_who_does_not_match_the_query_leaves_the_total_alone() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    fake.script_members(None, Ok(members(&["bob", "bobby"], 2)));
    search(&harness, "1", "bob");
    next_search(&harness);

    harness.clock.advance(PAUSE);

    assert_eq!(
        next_search(&harness),
        found("1", "bob", &["bob", "bobby"], 2)
    );
}
