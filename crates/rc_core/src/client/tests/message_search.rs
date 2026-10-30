// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Searching a room's messages on the server. The fake clock moves the pause before a search.

use super::*;
use crate::message_search::{MessageSearch, MessageSearchError, MessageSearchState, More};

/// How long the search waits for the typing to stop.
const PAUSE: Duration = Duration::from_millis(300);
/// Long enough to say that something did not happen.
const QUIET: Duration = Duration::from_millis(150);

/// `count` messages of room `1`, newest first, called `m<from>`, `m<from - 1>`...
fn results(from: i64, count: i64) -> Vec<Message> {
    (0..count)
        .map(|n| message(&format!("m{}", from - n), from - n))
        .collect()
}

fn event(room: &str, query: &str, state: MessageSearchState) -> Event {
    Event::MessageSearch(Some(MessageSearch {
        room: room.to_owned().into(),
        query: query.to_owned(),
        state,
    }))
}

fn searching(room: &str, query: &str) -> Event {
    event(room, query, MessageSearchState::Searching)
}

fn found(room: &str, query: &str, messages: Vec<Message>, more: bool) -> Event {
    let more = if more { More::Available } else { More::No };
    found_with(room, query, messages, more)
}

fn found_with(room: &str, query: &str, messages: Vec<Message>, more: More) -> Event {
    event(room, query, MessageSearchState::Found { messages, more })
}

/// A search for "alpha" in room `1` whose first page, of 20, is shown and has more.
fn search_with_a_page(fake: &Fake, harness: &Harness) {
    fake.script_message_search(None, Ok(results(40, 21)));
    search(harness, "1", "alpha");
    next_search(harness);
    pause_and_wait_for_calls(fake, harness, 1);
    next_search(harness);
}

/// The next `MessageSearch` event, skipping the other events.
fn next_search(harness: &Harness) -> Event {
    loop {
        let event = harness.next_event();
        if matches!(event, Event::MessageSearch(_)) {
            return event;
        }
    }
}

/// Checks that no `MessageSearch` event comes for a little while, and drops the other events.
fn assert_no_search(harness: &Harness) {
    while let Ok(event) = harness.recv_timeout(QUIET) {
        assert!(!matches!(event, Event::MessageSearch(_)), "{event:?}");
    }
}

fn search(harness: &Harness, room: &str, query: &str) {
    harness
        .account()
        .search_messages(&room.to_owned().into(), query);
}

/// Lets the typing stop, and waits for the search to reach the server.
fn pause_and_wait_for_calls(fake: &Fake, harness: &Harness, calls: usize) {
    harness.clock.advance(PAUSE);
    fake.wait_for_message_search_calls(calls);
}

// MSG-008
#[test]
fn a_query_shows_searching_at_once_and_the_server_is_asked_after_the_pause_for_one_more() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    fake.script_message_search(None, Ok(results(5, 2)));

    search(&harness, "1", "alpha");

    assert_eq!(next_search(&harness), searching("1", "alpha"));
    harness.clock.advance(Duration::from_millis(299));
    std::thread::sleep(QUIET);
    assert!(fake.state().message_search_calls.is_empty());
    harness.clock.advance(Duration::from_millis(1));
    let calls = fake.wait_for_message_search_calls(1);
    assert_eq!(
        calls,
        [MessageSearchCall {
            room: "1".to_owned(),
            text: "alpha".to_owned(),
            count: 21,
            offset: 0,
        }]
    );
    assert_eq!(
        next_search(&harness),
        found("1", "alpha", results(5, 2), false)
    );
}

// MSG-008
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
            message_search_debounce: jiff::SignedDuration::ZERO,
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
        .search_messages(&"1".to_owned().into(), "alpha");

    fake.wait_for_message_search_calls(1);
}

// MSG-008
#[test]
fn the_query_is_trimmed() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    fake.script_message_search(None, Ok(results(5, 1)));

    search(&harness, "1", "  alpha \n");

    assert_eq!(next_search(&harness), searching("1", "alpha"));
    harness.clock.advance(PAUSE);
    assert_eq!(fake.wait_for_message_search_calls(1)[0].text, "alpha");
    next_search(&harness);
    // The same search with other blanks is the same search.
    search(&harness, "1", "alpha ");
    assert_no_search(&harness);
    assert_eq!(fake.state().message_search_calls.len(), 1);
}

// MSG-008
#[test]
fn an_empty_query_asks_nothing_and_finds_nothing() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);

    search(&harness, "1", "  ");

    assert_eq!(next_search(&harness), found("1", "", Vec::new(), false));
    harness.clock.advance(PAUSE);
    std::thread::sleep(QUIET);
    assert!(fake.state().message_search_calls.is_empty());
}

// MSG-008
#[test]
fn an_empty_query_ends_the_search_in_flight_and_drops_its_answer() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    let gate = Arc::new(Notify::new());
    fake.script_message_search(Some(gate.clone()), Ok(results(5, 1)));
    search(&harness, "1", "alpha");
    next_search(&harness);
    pause_and_wait_for_calls(&fake, &harness, 1);

    search(&harness, "1", "");

    assert_eq!(next_search(&harness), found("1", "", Vec::new(), false));
    gate.notify_one();
    assert_no_search(&harness);
}

// MSG-008
#[test]
fn a_full_page_says_there_is_more_and_holds_twenty() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    fake.script_message_search(None, Ok(results(40, 21)));

    search(&harness, "1", "alpha");
    next_search(&harness);
    harness.clock.advance(PAUSE);

    assert_eq!(
        next_search(&harness),
        found("1", "alpha", results(40, 20), true)
    );
}

// MSG-008
#[test]
fn more_tells_loading_at_once_then_appends_the_next_page_and_asks_after_the_ones_held() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    let gate = Arc::new(Notify::new());
    fake.script_message_search(None, Ok(results(40, 21)));
    fake.script_message_search(Some(gate.clone()), Ok(results(20, 5)));
    search(&harness, "1", "alpha");
    next_search(&harness);
    pause_and_wait_for_calls(&fake, &harness, 1);
    next_search(&harness);

    harness.account().more_message_search();

    assert_eq!(
        next_search(&harness),
        found_with("1", "alpha", results(40, 20), More::Loading)
    );
    let calls = fake.wait_for_message_search_calls(2);
    assert_eq!(
        calls[1],
        MessageSearchCall {
            room: "1".to_owned(),
            text: "alpha".to_owned(),
            count: 21,
            offset: 20,
        }
    );
    gate.notify_one();
    let mut all = results(40, 20);
    all.extend(results(20, 5));
    assert_eq!(next_search(&harness), found("1", "alpha", all, false));
}

// MSG-008
#[test]
fn more_is_ignored_with_no_search_while_waiting_after_no_more_and_while_loading() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    let gate = Arc::new(Notify::new());
    fake.script_message_search(None, Ok(results(5, 2)));
    fake.script_message_search(None, Ok(results(40, 21)));
    fake.script_message_search(Some(gate.clone()), Ok(results(20, 5)));

    // Nothing is searched.
    harness.account().more_message_search();
    assert_no_search(&harness);
    // The search is waiting for the typing to stop.
    search(&harness, "1", "alpha");
    next_search(&harness);
    harness.account().more_message_search();
    pause_and_wait_for_calls(&fake, &harness, 1);
    next_search(&harness);
    // It found all there is.
    harness.account().more_message_search();
    assert_no_search(&harness);
    assert_eq!(fake.state().message_search_calls.len(), 1);

    // A page is on its way: asking again says and asks nothing.
    search(&harness, "1", "beta");
    next_search(&harness);
    pause_and_wait_for_calls(&fake, &harness, 2);
    next_search(&harness);
    harness.account().more_message_search();
    assert!(matches!(
        next_search(&harness),
        Event::MessageSearch(Some(MessageSearch {
            state: MessageSearchState::Found {
                more: More::Loading,
                ..
            },
            ..
        }))
    ));
    fake.wait_for_message_search_calls(3);
    harness.account().more_message_search();
    assert_no_search(&harness);
    assert_eq!(fake.state().message_search_calls.len(), 3);
    gate.notify_one();
    let Event::MessageSearch(Some(MessageSearch {
        state: MessageSearchState::Found { messages, more },
        ..
    })) = next_search(&harness)
    else {
        panic!("not found");
    };
    assert_eq!((messages.len(), more), (25, More::No));
}

// MSG-008
#[test]
fn a_failed_page_keeps_the_messages_and_asking_again_retries_and_appends() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    search_with_a_page(&fake, &harness);
    fake.script_message_search(None, Err(ApiError::Unreachable("down".to_owned())));
    fake.script_message_search(None, Ok(results(20, 5)));

    harness.account().more_message_search();

    assert_eq!(
        next_search(&harness),
        found_with("1", "alpha", results(40, 20), More::Loading)
    );
    assert_eq!(
        next_search(&harness),
        found_with(
            "1",
            "alpha",
            results(40, 20),
            More::Failed(MessageSearchError::Unreachable)
        )
    );
    harness.account().more_message_search();
    assert_eq!(
        next_search(&harness),
        found_with("1", "alpha", results(40, 20), More::Loading)
    );
    let calls = fake.wait_for_message_search_calls(3);
    assert_eq!(calls[2].offset, 20);
    let mut all = results(40, 20);
    all.extend(results(20, 5));
    assert_eq!(next_search(&harness), found("1", "alpha", all, false));
}

// MSG-008
#[test]
fn an_expired_token_in_a_page_expires_the_session() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    search_with_a_page(&fake, &harness);
    fake.script_message_search(None, Err(ApiError::Unauthorized));

    harness.account().more_message_search();

    let events = harness.events_until(|event| {
        matches!(
            event,
            Event::Session(SessionState::LoggedOut { failure: Some(_) })
        )
    });
    assert!(!events.iter().any(|event| matches!(
        event,
        Event::MessageSearch(Some(MessageSearch {
            state: MessageSearchState::Found {
                more: More::Failed(_),
                ..
            },
            ..
        }))
    )));
}

// MSG-008
#[test]
fn a_new_search_drops_the_page_of_the_old_one_on_its_way() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    let gate = Arc::new(Notify::new());
    fake.script_message_search(None, Ok(results(40, 21)));
    fake.script_message_search(Some(gate.clone()), Ok(results(20, 5)));
    fake.script_message_search(None, Ok(results(9, 1)));
    search(&harness, "1", "alpha");
    next_search(&harness);
    pause_and_wait_for_calls(&fake, &harness, 1);
    next_search(&harness);
    harness.account().more_message_search();
    assert!(matches!(
        next_search(&harness),
        Event::MessageSearch(Some(_))
    ));
    fake.wait_for_message_search_calls(2);

    search(&harness, "1", "beta");
    assert_eq!(next_search(&harness), searching("1", "beta"));
    gate.notify_one();
    pause_and_wait_for_calls(&fake, &harness, 3);

    assert_eq!(
        next_search(&harness),
        found("1", "beta", results(9, 1), false)
    );
}

// MSG-008
#[test]
fn typing_on_starts_the_pause_again() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    fake.script_message_search(None, Ok(results(5, 1)));

    search(&harness, "1", "a");
    assert_eq!(next_search(&harness), searching("1", "a"));
    harness.clock.advance(Duration::from_millis(200));
    search(&harness, "1", "al");
    assert_eq!(next_search(&harness), searching("1", "al"));
    harness.clock.advance(Duration::from_millis(200));
    std::thread::sleep(QUIET);
    assert!(fake.state().message_search_calls.is_empty());
    harness.clock.advance(Duration::from_millis(100));

    let calls = fake.wait_for_message_search_calls(1);
    assert_eq!(calls[0].text, "al");
    assert_eq!(
        next_search(&harness),
        found("1", "al", results(5, 1), false)
    );
}

// MSG-008
#[test]
fn a_newer_query_drops_the_answer_of_the_request_in_flight() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    let gate = Arc::new(Notify::new());
    fake.script_message_search(Some(gate.clone()), Ok(results(5, 1)));
    fake.script_message_search(None, Ok(results(9, 1)));
    search(&harness, "1", "a");
    assert_eq!(next_search(&harness), searching("1", "a"));
    pause_and_wait_for_calls(&fake, &harness, 1);

    search(&harness, "1", "al");
    assert_eq!(next_search(&harness), searching("1", "al"));
    gate.notify_one();
    pause_and_wait_for_calls(&fake, &harness, 2);

    assert_eq!(
        next_search(&harness),
        found("1", "al", results(9, 1), false)
    );
}

// MSG-008
#[test]
fn another_room_drops_the_answer_of_the_request_in_flight() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    let gate = Arc::new(Notify::new());
    fake.script_message_search(Some(gate.clone()), Ok(results(5, 1)));
    fake.script_message_search(None, Ok(results(9, 1)));
    search(&harness, "1", "a");
    next_search(&harness);
    pause_and_wait_for_calls(&fake, &harness, 1);

    // The same query, in the other room.
    search(&harness, "2", "a");
    assert_eq!(next_search(&harness), searching("2", "a"));
    gate.notify_one();
    pause_and_wait_for_calls(&fake, &harness, 2);

    assert_eq!(next_search(&harness), found("2", "a", results(9, 1), false));
}

// MSG-008
#[test]
fn the_same_search_again_does_nothing() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    fake.script_message_search(None, Ok(results(5, 1)));
    search(&harness, "1", "alpha");
    next_search(&harness);
    pause_and_wait_for_calls(&fake, &harness, 1);
    next_search(&harness);

    search(&harness, "1", "alpha");
    harness.clock.advance(PAUSE);

    assert_no_search(&harness);
    assert_eq!(fake.state().message_search_calls.len(), 1);
}

// MSG-008
#[test]
fn a_failure_is_told_and_the_same_search_again_retries_at_once() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    fake.script_message_search(None, Err(ApiError::Unreachable("down".to_owned())));
    fake.script_message_search(None, Ok(results(5, 1)));
    search(&harness, "1", "alpha");
    next_search(&harness);
    pause_and_wait_for_calls(&fake, &harness, 1);
    assert_eq!(
        next_search(&harness),
        event(
            "1",
            "alpha",
            MessageSearchState::Failed(MessageSearchError::Unreachable)
        )
    );

    search(&harness, "1", "alpha");

    assert_eq!(next_search(&harness), searching("1", "alpha"));
    fake.wait_for_message_search_calls(2);
    assert_eq!(
        next_search(&harness),
        found("1", "alpha", results(5, 1), false)
    );
}

// MSG-008
#[test]
fn another_error_carries_the_servers_message() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    fake.script_message_search(None, Err(refused()));
    search(&harness, "1", "/(/i");
    next_search(&harness);

    harness.clock.advance(PAUSE);

    let Event::MessageSearch(Some(MessageSearch {
        state: MessageSearchState::Failed(error),
        ..
    })) = next_search(&harness)
    else {
        panic!("not a failure");
    };
    assert!(matches!(error, MessageSearchError::Server(_)), "{error:?}");
}

// MSG-008
#[test]
fn cancelling_ends_the_search_and_drops_the_answer_of_the_request_in_flight() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    let gate = Arc::new(Notify::new());
    fake.script_message_search(Some(gate.clone()), Ok(results(5, 1)));
    search(&harness, "1", "alpha");
    next_search(&harness);
    pause_and_wait_for_calls(&fake, &harness, 1);

    harness.account().cancel_message_search();

    assert_eq!(next_search(&harness), Event::MessageSearch(None));
    gate.notify_one();
    assert_no_search(&harness);
}

// MSG-008
#[test]
fn cancelling_during_the_pause_asks_nothing() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    search(&harness, "1", "alpha");
    next_search(&harness);

    harness.account().cancel_message_search();
    assert_eq!(next_search(&harness), Event::MessageSearch(None));
    harness.clock.advance(PAUSE);

    std::thread::sleep(QUIET);
    assert!(fake.state().message_search_calls.is_empty());
    // Cancelling again says nothing.
    harness.account().cancel_message_search();
    assert_no_search(&harness);
}

// MSG-008
#[test]
fn an_expired_token_in_the_search_expires_the_session() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    fake.script_message_search(None, Err(ApiError::Unauthorized));
    search(&harness, "1", "alpha");
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
        Event::MessageSearch(Some(MessageSearch {
            state: MessageSearchState::Failed(_),
            ..
        }))
    )));
    // What the session showed is cleared, the search too.
    assert_eq!(next_search(&harness), Event::MessageSearch(None));
}

// MSG-008
#[test]
fn a_logout_clears_the_search_and_drops_the_answer_of_the_request_in_flight() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    let gate = Arc::new(Notify::new());
    fake.script_message_search(Some(gate.clone()), Ok(results(5, 1)));
    search(&harness, "1", "alpha");
    next_search(&harness);
    pause_and_wait_for_calls(&fake, &harness, 1);

    harness.logout();
    let events = harness.events_until(|event| matches!(event, Event::MessageSearch(_)));
    assert_eq!(events.last(), Some(&Event::MessageSearch(None)));
    gate.notify_one();

    assert_no_search(&harness);
}

// MSG-008
#[test]
fn a_search_while_logged_out_is_ignored() {
    let fake = Fake::new();
    let harness = Harness::new(&fake);

    search(&harness, "1", "alpha");
    harness.clock.advance(PAUSE);

    assert_no_search(&harness);
    assert!(fake.state().message_search_calls.is_empty());
}

// MSG-008
#[test]
fn the_avatars_of_the_authors_found_are_wanted() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    let mut by_bob = message("m2", 2);
    by_bob.author.username = "bob".to_owned();
    fake.script_message_search(None, Ok(vec![by_bob, message("m1", 1)]));
    search(&harness, "1", "alpha");
    next_search(&harness);
    harness.clock.advance(PAUSE);

    let owners = fake.wait_for_avatar_calls(2);

    assert!(owners.contains(&AvatarOwner::User("bob".to_owned())));
    assert!(owners.contains(&AvatarOwner::User("alice".to_owned())));
}
