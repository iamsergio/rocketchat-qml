// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Pushed events, and the realtime connection and its reconnects.

use super::*;

// RT-002
#[test]
fn what_is_pushed_before_the_rooms_are_listed_is_replayed_on_the_list() {
    let fake = Fake::new();
    let gate = Arc::new(Notify::new());
    fake.script_login(None, Ok(()));
    fake.script_fetch(
        Some(Arc::clone(&gate)),
        Ok(vec![
            subscription("1", "general", rc_api::RoomType::Channel),
            subscription("2", "secret", rc_api::RoomType::Private),
        ]),
    );
    let mut unread = subscription("2", "secret", rc_api::RoomType::Private);
    unread.unread = 3;
    fake.script_realtime(
        None,
        vec![
            Pushed::Subscription(unread),
            pushed_subscription("3", "alpha", rc_api::RoomType::Channel),
        ],
        Ok(()),
    );
    let harness = Harness::new(&fake);
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
    ]);
    // The connection was opened before the rooms were asked for.
    fake.wait_for_subscriptions_calls(1);
    assert_eq!(fake.state().realtime_calls.len(), 1);
    gate.notify_one();

    let mut secret = room("2", "secret", RoomKind::Private);
    secret.unread = 3;
    harness.expect_events(vec![
        rooms_event(vec![
            room("1", "general", RoomKind::Channel),
            room("2", "secret", RoomKind::Private),
        ]),
        rooms_change(RoomChange::Updated {
            index: 1,
            item: secret,
        }),
        rooms_change(RoomChange::Inserted {
            index: 0,
            item: room("3", "alpha", RoomKind::Channel),
        }),
    ]);
    assert_eq!(
        fake.state().realtime_calls,
        [("uid".to_owned(), "tok".to_owned())]
    );
}

// RT-002
#[test]
fn a_subscription_change_inserts_updates_or_moves_the_room() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    // New.
    assert!(fake.push(
        0,
        pushed_subscription("3", "zulu", rc_api::RoomType::Direct)
    ));
    harness.expect_events(vec![rooms_change(RoomChange::Inserted {
        index: 2,
        item: room("3", "zulu", RoomKind::Direct),
    })]);
    // Same place, new counts.
    let mut mentioned = subscription("1", "general", rc_api::RoomType::Channel);
    mentioned.unread = 2;
    mentioned.user_mentions = 1;
    mentioned.alert = true;
    assert!(fake.push(0, Pushed::Subscription(mentioned)));
    let mut general = room("1", "general", RoomKind::Channel);
    general.unread = 2;
    general.user_mentions = 1;
    general.alert = true;
    harness.expect_events(vec![rooms_change(RoomChange::Updated {
        index: 0,
        item: general,
    })]);
    // Renamed.
    assert!(fake.push(
        0,
        pushed_subscription("2", "alpha", rc_api::RoomType::Private)
    ));
    harness.expect_events(vec![rooms_change(RoomChange::Moved {
        from: 1,
        to: 0,
        item: room("2", "alpha", RoomKind::Private),
    })]);
}

// RT-002
#[test]
fn a_removed_or_hidden_subscription_leaves_the_list() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    assert!(fake.push(0, removed("2")));
    harness.expect_events(vec![rooms_change(RoomChange::Removed { index: 1 })]);
    // The next push shows nothing was emitted for an unknown room, or one that's gone.
    assert!(fake.push(0, removed("2")));
    assert!(fake.push(0, removed("nope")));
    let mut hidden = subscription("1", "general", rc_api::RoomType::Channel);
    hidden.open = false;
    assert!(fake.push(0, Pushed::Subscription(hidden)));
    harness.expect_events(vec![rooms_change(RoomChange::Removed { index: 0 })]);
}

// RT-002
#[test]
fn removing_the_open_room_closes_it() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    assert!(fake.push(0, removed("1")));
    harness.expect_events(vec![rooms_change(RoomChange::Removed { index: 0 })]);
    harness.expect_events(closed_events());
    // The room is gone: it can't be opened.
    harness.account().open_room("1".to_owned().into());
    assert!(fake.push(0, removed("2")));
    harness.expect_events(vec![rooms_change(RoomChange::Removed { index: 0 })]);
}

// RT-001
#[test]
fn a_live_message_is_appended_to_the_open_room_and_kept() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    assert!(fake.push(0, pushed_message("1", "c", 3)));
    harness.expect_events(vec![appended("c", 3)]);

    // Kept: it's there when the room opens again.
    harness.account().open_room("2".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("2"), Vec::new()),
        status("2", true, false, false, None),
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
}

// RT-001
#[test]
fn a_live_message_already_held_or_of_another_room_is_ignored() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    open_first_room(&fake, &harness, false);
    assert!(fake.push(0, pushed_message("1", "b", 2)));
    assert!(fake.push(0, pushed_message("2", "x", 9)));
    assert!(fake.push(0, pushed_message("nowhere", "y", 9)));
    assert!(fake.push(0, pushed_message("1", "c", 3)));
    // Nothing came out for the first three.
    harness.expect_events(vec![appended("c", 3)]);
}

// RT-001
#[test]
fn a_message_pushed_while_the_room_loads_joins_the_page() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    let gate = Arc::new(Notify::new());
    fake.script_history(
        Some(Arc::clone(&gate)),
        page(vec![message("b", 2), message("a", 1)], false),
    );
    harness.account().open_room("1".to_owned().into());
    harness.expect_events(vec![
        reset_event(Some("1"), Vec::new()),
        status("1", true, false, false, None),
    ]);
    fake.wait_for_history_calls(1);
    assert!(fake.push(0, pushed_message("1", "c", 3)));
    // Pushes come out in order: once this one has, the message before it was taken in.
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
        reset_event(
            Some("1"),
            vec![message("a", 1), message("b", 2), message("c", 3)],
        ),
        status("1", false, false, false, None),
    ]);
}

// AUTH-004, ACCT-006
#[test]
fn an_unauthorized_realtime_connection_expires_the_session_and_keeps_the_account() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    fake.script_realtime(None, Vec::new(), Err(ApiError::Unauthorized));
    let store = MemoryStore::default();
    let harness = Harness::with_store(&fake, store.clone());
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        session_event(logged_out(Some(LoginFailure::SessionExpired))),
    ]);
    wait_for_expired(&store);
    // Dropping flushes the actor's work: a fetch or a revoke would show by now.
    drop(harness);
    assert!(fake.state().subscriptions_calls.is_empty());
    assert!(fake.state().logout_calls.is_empty());
}

// RT-003
#[test]
fn another_realtime_failure_still_lists_the_rooms() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    fake.script_realtime(
        None,
        Vec::new(),
        Err(ApiError::Unreachable("refused".into())),
    );
    fake.script_fetch(
        None,
        Ok(vec![subscription(
            "1",
            "general",
            rc_api::RoomType::Channel,
        )]),
    );
    let harness = Harness::new(&fake);
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![room("1", "general", RoomKind::Channel)]),
    ]);
}

// RT-003
#[test]
fn a_connection_that_ends_leaves_the_session_working() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    fake.end_connection(0);
    open_first_room(&fake, &harness, false);
}

/// How long a connection must stay up in the tests that need it to count as working.
const STABLE: Duration = Duration::from_millis(300);

// RT-003
#[test]
fn a_lost_connection_is_reopened_and_the_rooms_synced_without_a_reset() {
    let fake = Fake::new();
    let harness = logged_in_harness_with(&fake, fast());
    // While away: "secret" was left, "alpha" joined, "general" got unread messages.
    fake.script_fetch(
        None,
        Ok(vec![
            general_with(2),
            subscription("3", "alpha", rc_api::RoomType::Channel),
        ]),
    );
    // Pushed as soon as the new connection opens: replayed on top of the synced list.
    fake.script_realtime(None, vec![Pushed::Subscription(general_with(5))], Ok(()));
    fake.end_connection(0);

    harness.expect_events(vec![
        Event::Rooms(vec![
            RoomChange::Removed { index: 1 },
            RoomChange::Inserted {
                index: 0,
                item: room("3", "alpha", RoomKind::Channel),
            },
            RoomChange::Updated {
                index: 1,
                item: general_room(2),
            },
        ]),
        rooms_change(RoomChange::Updated {
            index: 1,
            item: general_room(5),
        }),
    ]);
    assert_eq!(fake.state().realtime_calls.len(), 2);
    // The new connection is the live one.
    assert!(fake.push(
        1,
        pushed_subscription("4", "zulu", rc_api::RoomType::Channel)
    ));
    harness.expect_events(vec![rooms_change(RoomChange::Inserted {
        index: 2,
        item: room("4", "zulu", RoomKind::Channel),
    })]);
}

/// The time between `from` and the realtime call `index`.
fn wait_before_call(fake: &Fake, index: usize, from: Instant) -> Duration {
    wait_until(|| fake.state().realtime_times.get(index).copied()).duration_since(from)
}

/// A fake and a harness whose first three connections fail to open and whose fourth works; the
/// rooms are listed on the first and the fourth. Returns once the rooms are listed again.
fn after_three_failed_opens(backoff: Backoff) -> (Fake, Harness) {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    for _ in 0..3 {
        fake.script_realtime(
            None,
            Vec::new(),
            Err(ApiError::Unreachable("refused".into())),
        );
    }
    fake.script_fetch(None, Ok(vec![general_with(0)]));
    fake.script_fetch(None, Ok(vec![general_with(0)]));
    let harness = Harness::with_backoff(&fake, MemoryStore::default(), backoff);
    harness.login("http://h", "alice");
    // The rooms are listed once, although the connection isn't there.
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![general_room(0)]),
    ]);
    // The fourth connection opened, and the rooms were fetched once more: only that.
    fake.wait_for_connections(4);
    wait_until(|| (fake.state().subscriptions_calls.len() == 2).then_some(()));
    (fake, harness)
}

// RT-003
#[test]
fn a_failed_open_is_retried_with_growing_waits() {
    let (fake, harness) = after_three_failed_opens(fast());
    // Each wait is at least half of its step: 100, 200, 400 ms.
    let times = fake.state().realtime_times.clone();
    assert_eq!(times.len(), 4);
    for (index, step) in [BASE, BASE * 2, BASE * 4].into_iter().enumerate() {
        assert!(
            times[index + 1].duration_since(times[index]) >= step / 2,
            "wait {index} was shorter than {:?}",
            step / 2
        );
    }
    drop(harness);
}

// RT-003
#[test]
fn a_connection_that_drops_soon_after_it_worked_keeps_the_growing_wait() {
    let (fake, harness) = after_three_failed_opens(fast());
    // The fifth step would be 800 ms, of which 400 at least; the first step's is 100 at most.
    let lost = Instant::now();
    fake.end_connection(3);
    assert!(wait_before_call(&fake, 4, lost) >= CAP / 2);
    // And the one after it grows again, to the cap.
    let lost = Instant::now();
    fake.end_connection(4);
    assert!(wait_before_call(&fake, 5, lost) >= CAP / 2);
    drop(harness);
}

// RT-003
#[test]
fn a_connection_that_stayed_up_starts_the_waits_over() {
    let (fake, harness) = after_three_failed_opens(Backoff::new(BASE, CAP, STABLE));
    std::thread::sleep(STABLE * 2);
    // It worked for long enough, so the next loss waits for the first step (100 ms at most), not
    // the fifth (400 at least).
    let lost = Instant::now();
    fake.end_connection(3);
    assert!(wait_before_call(&fake, 4, lost) < CAP / 2);
    drop(harness);
}

// RT-003
#[test]
fn the_open_room_gets_the_messages_it_missed() {
    let fake = Fake::new();
    let harness = open_room_then_lose_the_connection(&fake);
    fake.script_fetch(
        None,
        Ok(vec![
            subscription("1", "general", rc_api::RoomType::Channel),
            subscription("2", "secret", rc_api::RoomType::Private),
        ]),
    );
    // Missed: c. Pushed right after reconnecting, while the page loads: d.
    fake.script_history(None, page(vec![message("c", 3), message("b", 2)], false));
    fake.script_realtime(None, vec![pushed_message("1", "d", 4)], Ok(()));
    fake.end_connection(0);

    // No room changes, no reset: the page touches what is held, so it is appended.
    harness.expect_events(vec![
        status("1", true, false, false, None),
        Event::Timeline(vec![TimelineChange::Appended(vec![
            message("c", 3),
            message("d", 4),
        ])]),
        status("1", false, false, false, None),
    ]);
    let calls = fake.state().history_calls.clone();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[1].before, None);
    assert_eq!(calls[1].room, "1");
}

// RT-003
#[test]
fn a_gap_longer_than_a_page_drops_the_held_messages() {
    let fake = Fake::new();
    let harness = open_room_then_lose_the_connection(&fake);
    fake.script_fetch(
        None,
        Ok(vec![
            subscription("1", "general", rc_api::RoomType::Channel),
            subscription("2", "secret", rc_api::RoomType::Private),
        ]),
    );
    // A page that doesn't reach a, b: keeping them would leave a hole nothing fills.
    fake.script_history(None, page(vec![message("z", 26), message("y", 25)], false));
    fake.end_connection(0);

    harness.expect_events(vec![
        status("1", true, false, false, None),
        reset_event(Some("1"), vec![message("y", 25), message("z", 26)]),
        status("1", false, false, false, None),
    ]);
}

// RT-003
#[test]
fn an_open_room_that_is_gone_after_the_reconnect_is_closed() {
    let fake = Fake::new();
    let harness = open_room_then_lose_the_connection(&fake);
    fake.script_fetch(
        None,
        Ok(vec![subscription("2", "secret", rc_api::RoomType::Private)]),
    );
    fake.end_connection(0);

    harness.expect_events(vec![rooms_change(RoomChange::Removed { index: 0 })]);
    harness.expect_events(closed_events());
    assert_eq!(fake.state().history_calls.len(), 1);
}

// RT-003
#[test]
fn logout_cancels_a_pending_reconnect() {
    let fake = Fake::new();
    let harness = logged_in_harness_with(&fake, Backoff::new(BASE * 2, BASE * 2, NEVER));
    fake.end_connection(0);
    harness.logout();
    harness.expect_events(vec![session_event(logged_out(None)), rooms_event(vec![])]);
    // Past the wait, even with the jitter at its least.
    std::thread::sleep(BASE * 4);
    assert_eq!(fake.state().realtime_calls.len(), 1);
}

// RT-003, ACCT-006
#[test]
fn an_expired_token_on_a_reconnect_expires_the_session_and_keeps_the_account() {
    let fake = Fake::new();
    let store = MemoryStore::default();
    fake.script_login(None, Ok(()));
    let harness = Harness::with_backoff(&fake, store.clone(), fast());
    harness.login("http://h", "alice");
    harness.expect(&[SessionState::Connecting, logged_in("http://h", "alice")]);
    fake.wait_for_connections(1);
    fake.script_realtime(None, Vec::new(), Err(ApiError::Unauthorized));
    fake.end_connection(0);
    harness.expect(&[logged_out(Some(LoginFailure::SessionExpired))]);
    wait_for_expired(&store);
    // No more tries, and nothing revoked.
    std::thread::sleep(BASE * 2);
    drop(harness);
    assert_eq!(fake.state().realtime_calls.len(), 2);
    assert!(fake.state().logout_calls.is_empty());
}

// RT-003
#[test]
fn a_failed_room_fetch_ends_the_cycle_and_is_retried_after_a_wait() {
    let fake = Fake::new();
    let harness = logged_in_harness_with(&fake, fast());
    fake.script_fetch(None, Err(ApiError::Unreachable("reset".into())));
    fake.script_fetch(
        None,
        Ok(vec![
            general_with(1),
            subscription("2", "secret", rc_api::RoomType::Private),
        ]),
    );
    let lost = Instant::now();
    fake.end_connection(0);

    harness.expect_events(vec![rooms_change(RoomChange::Updated {
        index: 0,
        item: general_room(1),
    })]);
    // Connection 1 was dropped when its fetch failed, and connection 2 came after a wait.
    assert!(
        fake.state().connections[1]
            .as_ref()
            .is_some_and(UnboundedSender::is_closed)
    );
    assert_eq!(fake.state().realtime_calls.len(), 3);
    // Two waits: the first step's half at least, and the second's.
    assert!(lost.elapsed() >= BASE / 2 + BASE);
}

// RT-001
#[test]
fn logout_ends_the_connection_and_the_next_session_gets_its_own() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    harness.logout();
    harness.expect_events(vec![session_event(logged_out(None)), rooms_event(vec![])]);
    // The client dropped its end: the connection is closed.
    fake.wait_for_connection_closed(0);
    assert!(!fake.push(0, removed("1")));

    fake.script_login(None, Ok(()));
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![]),
    ]);
    assert!(fake.push(
        1,
        pushed_subscription("5", "new", rc_api::RoomType::Channel)
    ));
    harness.expect_events(vec![rooms_change(RoomChange::Inserted {
        index: 0,
        item: room("5", "new", RoomKind::Channel),
    })]);
}

// RT-001
#[test]
fn a_connection_that_opens_after_logout_is_dropped() {
    let fake = Fake::new();
    let gate = Arc::new(Notify::new());
    fake.script_login(None, Ok(()));
    fake.script_login(None, Ok(()));
    fake.script_realtime(
        Some(Arc::clone(&gate)),
        vec![pushed_subscription("old", "old", rc_api::RoomType::Channel)],
        Ok(()),
    );
    let harness = Harness::new(&fake);
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
    ]);
    fake.wait_for_connections(1);

    harness.logout();
    harness.expect_events(vec![session_event(logged_out(None))]);
    fake.wait_for_connection_closed(0);
    gate.notify_one();

    fake.script_fetch(
        None,
        Ok(vec![subscription("new", "new", rc_api::RoomType::Channel)]),
    );
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![room("new", "new", RoomKind::Channel)]),
    ]);
    // Only the new connection's pushes arrive.
    assert!(fake.push(
        1,
        pushed_subscription("5", "other", rc_api::RoomType::Channel)
    ));
    harness.expect_events(vec![rooms_change(RoomChange::Inserted {
        index: 1,
        item: room("5", "other", RoomKind::Channel),
    })]);
}

// RT-004
#[test]
fn a_lost_connection_is_reported_with_the_wait_and_then_the_attempt_and_the_recovery() {
    let fake = Fake::new();
    let harness = logged_in_harness_with(&fake, fast());
    fake.end_connection(0);
    let Connection::Waiting { retry_in } = harness.next_connection() else {
        panic!("not waiting");
    };
    assert!((BASE / 2..=BASE).contains(&retry_in), "{retry_in:?}");
    assert_eq!(harness.next_connection(), Connection::Reconnecting);
    assert_eq!(harness.next_connection(), Connection::Connected);
}

// RT-004
#[test]
fn a_failed_attempt_is_reported_as_waiting_again_with_a_longer_wait() {
    let fake = Fake::new();
    let harness = logged_in_harness_with(&fake, fast());
    fake.script_realtime(
        None,
        Vec::new(),
        Err(ApiError::Unreachable("refused".into())),
    );
    fake.end_connection(0);
    let Connection::Waiting { retry_in: first } = harness.next_connection() else {
        panic!("not waiting");
    };
    assert_eq!(harness.next_connection(), Connection::Reconnecting);
    let Connection::Waiting { retry_in: second } = harness.next_connection() else {
        panic!("not waiting");
    };
    assert!(second >= BASE, "{first:?} then {second:?}");
    assert_eq!(harness.next_connection(), Connection::Reconnecting);
    assert_eq!(harness.next_connection(), Connection::Connected);
}

// RT-004
#[test]
fn a_connection_that_is_never_lost_reports_nothing() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    fake.wait_for_connections(1);
    harness.logout();
    loop {
        match harness.recv_timeout(TIMEOUT).expect("no event") {
            Event::Connection(connection) => panic!("{connection:?}"),
            Event::Session(SessionState::LoggedOut { .. }) => break,
            _ => {}
        }
    }
}
