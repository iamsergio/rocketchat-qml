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
