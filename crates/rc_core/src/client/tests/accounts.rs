// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Several accounts.

use super::*;

const OTHER: &str = "http://other";

/// A fake that answers for the server `http://other`, apart from `fake`.
fn other_server(fake: &Fake) -> Fake {
    let other = Fake::new();
    fake.serve(OTHER, &other);
    other
}

/// The ids of the accounts in `store`, in order.
fn stored_ids(store: &MemoryStore) -> Vec<String> {
    (store.load().unwrap().iter())
        .map(|account| account.id.to_string())
        .collect()
}

/// A store holding a signed-in account `one` on `http://h` and `two` on `http://other`.
fn store_with_two() -> MemoryStore {
    let store = MemoryStore::default();
    store.save(&stored_as("one", "http://h")).unwrap();
    store.save(&stored_as("two", OTHER)).unwrap();
    store
}

/// A harness with accounts `one` and `two` resumed, past their events.
fn two_resumed(fake: &Fake, other: &Fake, store: &MemoryStore) -> (Harness, AccountId) {
    fake.script_fetch(
        None,
        Ok(vec![subscription(
            "1",
            "general",
            rc_api::RoomType::Channel,
        )]),
    );
    other.script_fetch(
        None,
        Ok(vec![subscription(
            "9",
            "elsewhere",
            rc_api::RoomType::Channel,
        )]),
    );
    let harness = Harness::with_accounts(fake, store.clone());
    let two = AccountId::from("two".to_owned());
    harness.expect(&[logged_in("http://h", "alice")]);
    harness.expect_of(&two, &[SessionState::Resuming, logged_in(OTHER, "alice")]);
    harness.expect_events(vec![rooms_event(vec![room(
        "1",
        "general",
        RoomKind::Channel,
    )])]);
    harness.expect_events_of(
        &two,
        vec![rooms_event(vec![room("9", "elsewhere", RoomKind::Channel)])],
    );
    (harness, two)
}

// ACCT-001
#[test]
fn a_new_account_is_logged_out_and_logs_in_beside_the_first() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    harness.allow_other_accounts();
    let other = other_server(&fake);
    other.script_login(None, Ok(()));
    other.script_fetch(
        None,
        Ok(vec![subscription(
            "9",
            "elsewhere",
            rc_api::RoomType::Channel,
        )]),
    );

    let b = harness.client.add_account();
    assert_ne!(&b, harness.id());
    harness.expect_events_of(&b, vec![Event::Added, session_event(logged_out(None))]);
    harness
        .client
        .account(&b)
        .login(OTHER, "bob", SecretString::from("secret"));
    harness.expect_events_of(
        &b,
        vec![
            session_event(SessionState::Connecting),
            session_event(logged_in(OTHER, "bob")),
            rooms_event(vec![room("9", "elsewhere", RoomKind::Channel)]),
        ],
    );

    // The first account is still logged in, with its own rooms, and nothing of it was logged
    // into the other server.
    assert_eq!(
        other.login_calls(),
        [("bob".to_owned(), "secret".to_owned())]
    );
    assert_eq!(
        fake.login_calls(),
        [("alice".to_owned(), "secret".to_owned())]
    );
    assert!(
        harness
            .recv_of(harness.id(), Duration::from_millis(100))
            .is_err()
    );
}

// ACCT-001
#[test]
fn commands_go_only_to_their_account() {
    let fake = Fake::new();
    let harness = logged_in_harness(&fake);
    harness.allow_other_accounts();
    let other = other_server(&fake);
    other.script_login(None, Ok(()));
    other.script_fetch(
        None,
        Ok(vec![subscription(
            "9",
            "elsewhere",
            rc_api::RoomType::Channel,
        )]),
    );
    let b = harness.client.add_account();
    harness
        .client
        .account(&b)
        .login(OTHER, "bob", SecretString::from("secret"));
    harness.expect_of(
        &b,
        &[
            logged_out(None),
            SessionState::Connecting,
            logged_in(OTHER, "bob"),
        ],
    );

    open_first_room(&fake, &harness, false);
    other.script_history(None, page(Vec::new(), true));
    harness.client.account(&b).open_room("9".to_owned().into());
    harness.expect_events_of(
        &b,
        vec![
            rooms_event(vec![room("9", "elsewhere", RoomKind::Channel)]),
            reset_event(Some("9"), Vec::new()),
            status("9", true, false, false, None),
            status("9", false, false, true, None),
        ],
    );

    let asked = |fake: &Fake| -> Vec<String> {
        (fake.state().history_calls.iter())
            .map(|call| call.room.clone())
            .collect()
    };
    assert_eq!(asked(&fake), ["1"]);
    assert_eq!(asked(&other), ["9"]);
}

// ACCT-002
#[test]
fn with_two_stored_accounts_both_are_added_in_order_and_resume() {
    let fake = Fake::new();
    let other = other_server(&fake);
    let store = store_with_two();
    let harness = Harness::with_accounts(&fake, store.clone());
    // The stored order: `one` came first.
    assert_eq!(harness.id().as_str(), "one");
    assert_eq!(harness.first, SessionState::Resuming);
    let two = AccountId::from("two".to_owned());
    harness.expect(&[logged_in("http://h", "alice")]);
    harness.expect_events_of(&two, vec![Event::Added]);
    harness.expect_of(&two, &[SessionState::Resuming, logged_in(OTHER, "alice")]);
    assert_eq!(fake.state().me_calls.len(), 1);
    assert_eq!(other.state().me_calls.len(), 1);
    drop(harness);
    assert_eq!(stored_ids(&store), ["one", "two"]);
}

// ACCT-002
#[test]
fn an_unreachable_server_does_not_stop_the_other_account_from_resuming() {
    let fake = Fake::new();
    let other = other_server(&fake);
    other.state().version = Some(Err(ApiError::Unreachable("refused".into())));
    let store = store_with_two();
    let harness = Harness::with_accounts(&fake, store.clone());
    let two = AccountId::from("two".to_owned());
    harness.expect(&[logged_in("http://h", "alice")]);
    harness.expect_of(
        &two,
        &[
            SessionState::Resuming,
            logged_out(Some(LoginFailure::Unreachable)),
        ],
    );
    drop(harness);
    // The account that couldn't be reached keeps its session.
    assert_eq!(stored_ids(&store), ["one", "two"]);
}

// ACCT-005
#[test]
fn removing_a_logged_in_account_revokes_its_token_and_leaves_the_other_working() {
    let fake = Fake::new();
    let other = other_server(&fake);
    let store = store_with_two();
    let (harness, two) = two_resumed(&fake, &other, &store);

    harness.client.remove_account(&two);
    // `Removed` is the last event of the account, after those of its logout.
    harness.expect_events_of(
        &two,
        vec![
            session_event(logged_out(None)),
            rooms_event(vec![]),
            Event::Removed,
        ],
    );
    assert_eq!(
        other.wait_for_logout_call(),
        ("uid".to_owned(), "stored-tok".to_owned())
    );
    wait_until(|| (stored_ids(&store) == ["one"]).then_some(()));
    assert!(fake.state().logout_calls.is_empty());

    // Commands for it are dropped, and the other account works.
    harness
        .client
        .account(&two)
        .login(OTHER, "bob", SecretString::from("secret"));
    open_first_room_of(&fake, &harness);
    assert!(harness.recv_of(&two, Duration::from_millis(100)).is_err());
    assert!(other.login_calls().is_empty());
}

// ACCT-005
#[test]
fn removing_a_logged_out_account_forgets_its_stored_entry() {
    let fake = Fake::new();
    let store = MemoryStore::default();
    let mut signed_out = stored_as("one", "http://h");
    signed_out.token = None;
    store.save(&signed_out).unwrap();
    store.save(&stored_as("two", "http://h")).unwrap();
    let harness = Harness::with_accounts(&fake, store.clone());
    assert_eq!(harness.first, logged_out(None));

    harness.client.remove_account(harness.id());
    // Logged out already: nothing but the end.
    harness.expect_events(vec![Event::Removed]);
    wait_until(|| (stored_ids(&store) == ["two"]).then_some(()));
    assert!(fake.state().logout_calls.is_empty());
}

// ACCT-005
#[test]
fn removing_the_last_account_adds_a_new_logged_out_one_after_removed() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    let store = MemoryStore::default();
    let harness = Harness::with_store(&fake, store.clone());
    harness.allow_other_accounts();
    harness.login("http://h", "alice");
    harness.expect(&[SessionState::Connecting, logged_in("http://h", "alice")]);
    // The rooms are listed before it goes, so that their event doesn't come in the middle.
    harness.expect_events(vec![rooms_event(vec![])]);

    harness.client.remove_account(harness.id());
    harness.expect_events(vec![session_event(logged_out(None)), Event::Removed]);
    wait_for_empty(&store);
    fake.wait_for_logout_call();

    // The new account is the first thing after `Removed`, and it works.
    let new = harness.next_added();
    assert_ne!(&new, harness.id());
    harness.expect_of(&new, &[logged_out(None)]);
    fake.script_login(None, Ok(()));
    harness
        .client
        .account(&new)
        .login("http://h", "alice", SecretString::from("secret"));
    harness.expect_of(
        &new,
        &[SessionState::Connecting, logged_in("http://h", "alice")],
    );
}

// ACCT-005
#[test]
fn dropping_the_client_waits_for_the_logouts_of_all_accounts() {
    let fake = Fake::new();
    let other = other_server(&fake);
    let store = store_with_two();
    let (harness, two) = two_resumed(&fake, &other, &store);
    fake.state().logout_delay = Duration::from_millis(500);
    other.state().logout_delay = Duration::from_millis(500);

    harness.logout();
    harness.logout_account(&two);
    harness.expect(&[logged_out(None)]);
    harness.expect_of(&two, &[logged_out(None)]);
    drop(harness);

    // Both revocations finished, and both store writes were applied.
    assert_eq!(fake.state().logouts_done, 1);
    assert_eq!(other.state().logouts_done, 1);
    assert!(store.load().unwrap().is_empty());
}

fn identity(server: &str, username: &str) -> Identity {
    Identity {
        server: server.to_owned(),
        username: username.to_owned(),
    }
}

/// The state of a user whose id on the server is not the fake's usual one.
fn logged_in_as(server: &str, username: &str, id: &str) -> SessionState {
    SessionState::LoggedIn(Account {
        server: server.to_owned(),
        user: User {
            id: id.to_owned().into(),
            ..user(username)
        },
    })
}

/// A harness for a store, with alice logged in on `http://h` and the events of that read.
fn alice_logged_in(fake: &Fake, store: &MemoryStore) -> Harness {
    fake.script_login(None, Ok(()));
    let harness = Harness::with_store(fake, store.clone());
    harness.allow_other_accounts();
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![]),
    ]);
    harness
}

/// The events of an account that logs in, up to the end of the attempt.
fn attempt_events(harness: &Harness, id: &AccountId) -> Vec<Event> {
    harness.events_until_of(id, |event| {
        matches!(
            event,
            Event::Session(
                SessionState::LoggedIn(_) | SessionState::LoggedOut { failure: Some(_) }
            )
        )
    })
}

// ACCT-002
#[test]
fn a_stored_account_tells_who_it_is_right_after_added() {
    let fake = Fake::new();
    let (client, events) = raw_client(
        &fake,
        store_with("http://h"),
        FakeCache::default(),
        Backoff::new(NEVER, NEVER, NEVER),
    );
    assert_eq!(
        raw_events(&events, 4),
        [
            Event::Added,
            Event::Identity(identity("http://h", "alice")),
            session_event(SessionState::Resuming),
            session_event(logged_in("http://h", "alice")),
        ]
    );
    drop(client);
}

// ACCT-002
#[test]
fn a_stored_account_without_a_token_is_added_logged_out_with_its_identity() {
    let fake = Fake::new();
    let store = MemoryStore::default();
    let mut signed_out = stored("http://h");
    signed_out.token = None;
    store.save(&signed_out).unwrap();
    let (client, events) = raw_client(
        &fake,
        store.clone(),
        FakeCache::default(),
        Backoff::new(NEVER, NEVER, NEVER),
    );
    assert_eq!(
        raw_events(&events, 3),
        [
            Event::Added,
            Event::Identity(identity("http://h", "alice")),
            session_event(logged_out(None)),
        ]
    );
    drop(client);
    // Nothing was resumed, and the account is still stored.
    assert!(fake.state().me_calls.is_empty());
    assert_eq!(stored_ids(&store), [STORED_ID]);
}

// ACCT-002
#[test]
fn a_first_login_tells_who_the_account_is_before_logging_in() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    let harness = Harness::new(&fake);
    harness.login("http://h", "alice");
    assert_eq!(
        attempt_events(&harness, harness.id()),
        [
            session_event(SessionState::Connecting),
            Event::Identity(identity("http://h", "alice")),
            session_event(logged_in("http://h", "alice")),
        ]
    );
}

// ACCT-002
#[test]
fn a_login_as_someone_else_on_an_expired_account_tells_the_new_identity() {
    let fake = Fake::new();
    fake.state().me_result = Some(Err(ApiError::Unauthorized));
    fake.state().user_ids.insert("bob".into(), "bob-id".into());
    fake.script_login(None, Ok(()));
    let store = store_with("http://h");
    let harness = Harness::with_store(&fake, store.clone());
    harness.expect(&[logged_out(Some(LoginFailure::SessionExpired))]);

    harness.login("http://h", "bob");
    assert_eq!(
        attempt_events(&harness, harness.id()),
        [
            session_event(SessionState::Connecting),
            Event::Identity(identity("http://h", "bob")),
            session_event(logged_in_as("http://h", "bob", "bob-id")),
        ]
    );
}

// ACCT-002
#[test]
fn a_login_as_the_same_user_tells_nothing_new() {
    let fake = Fake::new();
    fake.state().me_result = Some(Err(ApiError::Unauthorized));
    fake.script_login(None, Ok(()));
    let harness = Harness::with_store(&fake, store_with("http://h"));
    harness.expect(&[logged_out(Some(LoginFailure::SessionExpired))]);

    harness.login("http://h", "alice");
    assert_eq!(
        attempt_events(&harness, harness.id()),
        [
            session_event(SessionState::Connecting),
            session_event(logged_in("http://h", "alice")),
        ]
    );
}

// ACCT-006
#[test]
fn logging_in_again_on_an_expired_account_saves_the_token_under_the_same_id() {
    let fake = Fake::new();
    fake.state().me_result = Some(Err(ApiError::Unauthorized));
    fake.script_login(None, Ok(()));
    let store = store_with("http://h");
    let harness = Harness::with_store(&fake, store.clone());
    harness.expect(&[logged_out(Some(LoginFailure::SessionExpired))]);
    wait_for_expired(&store);

    harness.login("http://h", "alice");
    harness.expect(&[SessionState::Connecting, logged_in("http://h", "alice")]);
    let saved = wait_until(|| {
        let accounts = store.load().unwrap();
        let [account] = accounts.as_slice() else {
            return None;
        };
        account.token.is_some().then(|| account.clone())
    });
    assert_eq!(saved.id.as_str(), STORED_ID);
    assert_eq!(saved.server, "http://h");
    assert_eq!(saved.token.unwrap().expose_secret(), "tok");
}

// ACCT-006
#[test]
fn a_restart_lists_an_expired_account_logged_out_with_its_identity() {
    let fake = Fake::new();
    fake.state().me_result = Some(Err(ApiError::Unauthorized));
    let store = store_with("http://h");
    let harness = Harness::with_store(&fake, store.clone());
    harness.expect(&[logged_out(Some(LoginFailure::SessionExpired))]);
    wait_for_expired(&store);
    drop(harness);

    let (client, events) = raw_client(
        &fake,
        store.clone(),
        FakeCache::default(),
        Backoff::new(NEVER, NEVER, NEVER),
    );
    assert_eq!(
        raw_events(&events, 3),
        [
            Event::Added,
            Event::Identity(identity("http://h", "alice")),
            session_event(logged_out(None)),
        ]
    );
    drop(client);
    // It wasn't resumed again.
    assert_eq!(fake.state().me_calls.len(), 1);
}

// ACCT-006
#[test]
fn a_live_expiry_keeps_the_account_stored_without_a_token() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    let store = MemoryStore::default();
    let harness = Harness::with_backoff(&fake, store.clone(), fast());
    harness.login("http://h", "alice");
    harness.expect(&[SessionState::Connecting, logged_in("http://h", "alice")]);
    fake.wait_for_connections(1);
    fake.script_realtime(None, Vec::new(), Err(ApiError::Unauthorized));
    fake.end_connection(0);
    harness.expect(&[logged_out(Some(LoginFailure::SessionExpired))]);

    let kept = wait_for_expired(&store);
    assert_eq!(&kept.id, harness.id());
    assert_eq!(kept.server, "http://h");
    assert_eq!(kept.username, "alice");
    assert_eq!(kept.user_id.as_str(), "uid");
}

// ACCT-007
#[test]
fn a_user_another_account_has_cannot_be_added_and_the_new_token_is_revoked() {
    let fake = Fake::new();
    let store = MemoryStore::default();
    let harness = alice_logged_in(&fake, &store);
    let first = harness.id().to_string();

    let b = harness.client.add_account();
    fake.script_login(None, Ok(()));
    harness
        .client
        .account(&b)
        .login("http://h", "alice", SecretString::from("secret"));
    assert_eq!(
        attempt_events(&harness, &b),
        [
            Event::Added,
            session_event(logged_out(None)),
            session_event(SessionState::Connecting),
            session_event(logged_out(Some(LoginFailure::AlreadyAdded))),
        ]
    );
    assert_eq!(
        fake.wait_for_logout_call(),
        ("uid".to_owned(), "tok".to_owned())
    );
    // Nothing of it was saved or opened.
    drop(harness);
    assert_eq!(stored_ids(&store), [first]);
    assert_eq!(fake.state().realtime_calls.len(), 1);
}

// ACCT-007
#[test]
fn the_failure_says_the_account_was_added_already() {
    assert_eq!(
        LoginFailure::AlreadyAdded.to_string(),
        "You have added that account already"
    );
}

// ACCT-007
#[test]
fn a_user_listed_without_a_token_cannot_be_added_either() {
    let fake = Fake::new();
    let store = MemoryStore::default();
    let mut signed_out = stored_as("one", "http://h");
    signed_out.token = None;
    store.save(&signed_out).unwrap();
    let harness = Harness::with_accounts(&fake, store.clone());
    assert_eq!(harness.first, logged_out(None));

    let b = harness.client.add_account();
    fake.script_login(None, Ok(()));
    harness
        .client
        .account(&b)
        .login("http://h", "alice", SecretString::from("secret"));
    let events = attempt_events(&harness, &b);
    assert_eq!(
        events.last(),
        Some(&session_event(logged_out(Some(LoginFailure::AlreadyAdded))))
    );
    assert!(!events.iter().any(|e| matches!(e, Event::Identity(_))));
    fake.wait_for_logout_call();
    drop(harness);
    assert_eq!(stored_ids(&store), ["one"]);
}

// ACCT-007
#[test]
fn the_same_username_on_another_server_and_another_user_on_the_same_server_are_fine() {
    let fake = Fake::new();
    let store = MemoryStore::default();
    let harness = alice_logged_in(&fake, &store);
    let other = other_server(&fake);
    other.script_login(None, Ok(()));
    fake.state().user_ids.insert("bob".into(), "bob-id".into());

    let on_other = harness.client.add_account();
    harness
        .client
        .account(&on_other)
        .login(OTHER, "alice", SecretString::from("secret"));
    let events = attempt_events(&harness, &on_other);
    assert_eq!(
        events.last(),
        Some(&session_event(logged_in(OTHER, "alice")))
    );

    fake.script_login(None, Ok(()));
    let bob = harness.client.add_account();
    harness
        .client
        .account(&bob)
        .login("http://h", "bob", SecretString::from("secret"));
    let events = attempt_events(&harness, &bob);
    assert_eq!(
        events.last(),
        Some(&session_event(logged_in_as("http://h", "bob", "bob-id")))
    );
    drop(harness);
    assert_eq!(store.load().unwrap().len(), 3);
}

// ACCT-007
#[test]
fn the_user_can_be_added_again_once_the_other_account_is_removed() {
    let fake = Fake::new();
    let store = MemoryStore::default();
    let harness = alice_logged_in(&fake, &store);
    let first = harness.id().clone();
    let b = harness.client.add_account();
    fake.script_login(None, Ok(()));
    harness
        .client
        .account(&b)
        .login("http://h", "alice", SecretString::from("secret"));
    assert_eq!(
        attempt_events(&harness, &b).last(),
        Some(&session_event(logged_out(Some(LoginFailure::AlreadyAdded))))
    );

    harness.client.remove_account(&first);
    harness.events_until_of(&first, |event| *event == Event::Removed);
    fake.script_login(None, Ok(()));
    harness
        .client
        .account(&b)
        .login("http://h", "alice", SecretString::from("secret"));
    assert_eq!(
        attempt_events(&harness, &b).last(),
        Some(&session_event(logged_in("http://h", "alice")))
    );
}

/// The fake's subscription of room `id` with unread messages and mentions of the user.
fn unread_in(id: &str, unread: u32, mentions: u32) -> rc_api::Subscription {
    let mut sub = subscription(id, id, rc_api::RoomType::Channel);
    sub.unread = unread;
    sub.user_mentions = mentions;
    sub
}

fn totals(rooms: u32, mentions: u32) -> Unread {
    Unread { rooms, mentions }
}

// ACCT-004
#[test]
fn the_unread_totals_follow_the_room_list() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    fake.script_fetch(
        None,
        Ok(vec![
            unread_in("a", 3, 1),
            unread_in("b", 2, 0),
            unread_in("c", 0, 0),
        ]),
    );
    let harness = Harness::new(&fake);
    harness.login("http://h", "alice");

    // Listed: the totals come right after the rooms.
    let events = harness.events_until(|event| matches!(event, Event::Unread(_)));
    assert!(matches!(
        &events[events.len() - 2..],
        [Event::Rooms(_), Event::Unread(unread)] if *unread == totals(2, 1)
    ));

    // A pushed change.
    assert!(fake.push(0, Pushed::Subscription(unread_in("a", 0, 0))));
    assert_eq!(harness.next_unread(), totals(1, 0));

    // A change that leaves the totals alone sends nothing: the next totals are the next change's.
    let mut favorite = unread_in("b", 2, 0);
    favorite.favorite = true;
    assert!(fake.push(0, Pushed::Subscription(favorite)));
    assert!(fake.push(0, Pushed::Subscription(unread_in("c", 4, 7))));
    assert_eq!(harness.next_unread(), totals(2, 7));

    // Marking a room read.
    harness.account().mark_read("c".to_owned().into());
    assert_eq!(harness.next_unread(), totals(1, 0));

    // A logout empties the list.
    harness.logout();
    assert_eq!(harness.next_unread(), totals(0, 0));
}

// ACCT-004
#[test]
fn nothing_is_sent_while_the_totals_are_zero() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    fake.script_fetch(None, Ok(vec![unread_in("a", 0, 0)]));
    let harness = Harness::new(&fake);
    harness.login("http://h", "alice");
    harness.expect_events(vec![
        session_event(SessionState::Connecting),
        session_event(logged_in("http://h", "alice")),
        rooms_event(vec![room_from(unread_in("a", 0, 0)).unwrap()]),
    ]);
    assert!(fake.push(0, Pushed::Subscription(unread_in("b", 0, 0))));
    harness.expect_events(vec![rooms_change(RoomChange::Inserted {
        index: 1,
        room: room_from(unread_in("b", 0, 0)).unwrap(),
    })]);
    harness.logout();
    harness.expect_events(vec![session_event(logged_out(None)), rooms_event(vec![])]);

    // Logging in again with unread rooms: the first totals ever sent come now.
    fake.script_login(None, Ok(()));
    fake.script_fetch(None, Ok(vec![unread_in("a", 1, 0)]));
    harness.login("http://h", "alice");
    let events = harness.events_until(|event| matches!(event, Event::Unread(_)));
    assert_eq!(events.last(), Some(&Event::Unread(totals(1, 0))));
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::Unread(_)))
            .count(),
        1
    );
}

// ACCT-004
#[test]
fn two_accounts_have_separate_totals() {
    let fake = Fake::new();
    let other = other_server(&fake);
    let store = store_with_two();
    fake.script_fetch(None, Ok(vec![unread_in("a", 1, 0)]));
    other.script_fetch(None, Ok(vec![unread_in("x", 5, 2), unread_in("y", 1, 1)]));
    let harness = Harness::with_accounts(&fake, store);
    let two = AccountId::from("two".to_owned());

    assert_eq!(harness.next_unread(), totals(1, 0));
    assert_eq!(harness.next_unread_of(&two), totals(2, 3));
    assert!(other.push(0, Pushed::Subscription(unread_in("x", 0, 0))));
    assert_eq!(harness.next_unread_of(&two), totals(1, 1));
    // The first account heard nothing of it.
    assert!(harness.recv_timeout(Duration::from_millis(100)).is_err());
}
