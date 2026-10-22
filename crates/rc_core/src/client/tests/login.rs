// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Logging in and out, and resuming the stored session.

use super::*;

fn failing_login(failure: ApiError, expected: LoginFailure) {
    let fake = Fake::new();
    fake.script_login(None, Err(failure));
    let harness = Harness::new(&fake);
    harness.login("http://h", "u");
    harness.expect(&[SessionState::Connecting, logged_out(Some(expected))]);
}

// AUTH-001
#[test]
fn login_succeeds() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    let harness = Harness::new(&fake);
    harness.login(" chat.example.com ", "alice");
    harness.expect(&[
        SessionState::Connecting,
        logged_in("https://chat.example.com", "alice"),
    ]);
    assert_eq!(
        fake.login_calls(),
        [("alice".to_owned(), "secret".to_owned())]
    );
}

// AUTH-002
#[test]
fn invalid_credentials_are_reported() {
    failing_login(
        ApiError::InvalidCredentials,
        LoginFailure::InvalidCredentials,
    );
}

// AUTH-003
#[test]
fn invalid_url_is_reported() {
    let fake = Fake::new();
    fake.state().connect_error = Some(ApiError::InvalidUrl("x".into()));
    let harness = Harness::new(&fake);
    harness.login("x", "u");
    harness.expect(&[
        SessionState::Connecting,
        logged_out(Some(LoginFailure::InvalidUrl)),
    ]);
}

// AUTH-003
#[test]
fn unreachable_server_is_reported() {
    let fake = Fake::new();
    fake.state().version = Some(Err(ApiError::Unreachable("refused".into())));
    let harness = Harness::new(&fake);
    harness.login("http://h", "u");
    harness.expect(&[
        SessionState::Connecting,
        logged_out(Some(LoginFailure::Unreachable)),
    ]);
}

// AUTH-003
#[test]
fn non_rocketchat_server_is_reported() {
    let fake = Fake::new();
    fake.state().version = Some(Err(ApiError::NotRocketChat));
    let harness = Harness::new(&fake);
    harness.login("http://h", "u");
    harness.expect(&[
        SessionState::Connecting,
        logged_out(Some(LoginFailure::NotRocketChat)),
    ]);
}

// AUTH-003
#[test]
fn old_server_is_refused_without_logging_in() {
    let fake = Fake::new();
    fake.state().version = Some(Ok(ServerVersion { major: 7, minor: 9 }));
    let harness = Harness::new(&fake);
    harness.login("http://h", "u");
    let failure = LoginFailure::UnsupportedVersion {
        found: "7.9".into(),
        minimum: "8.0".into(),
    };
    harness.expect(&[SessionState::Connecting, logged_out(Some(failure))]);
    assert!(fake.login_calls().is_empty());
}

// AUTH-003
#[test]
fn server_errors_are_reported_with_their_message() {
    let error = ApiError::Server {
        status: 500,
        message: "boom".into(),
    };
    let expected = LoginFailure::Server(error.to_string());
    assert!(expected.to_string().contains("boom"));
    failing_login(error, expected);
}

// AUTH-005
#[test]
fn logout_revokes_the_token() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    let harness = Harness::new(&fake);
    harness.login("http://h", "alice");
    harness.expect(&[SessionState::Connecting, logged_in("http://h", "alice")]);
    harness.logout();
    harness.expect(&[logged_out(None)]);
    assert_eq!(
        fake.wait_for_logout_call(),
        ("uid".to_owned(), "tok".to_owned())
    );
}

// AUTH-005
#[test]
fn logout_ends_logged_out_even_if_the_server_call_fails() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    fake.state().logout_result = Some(Err(ApiError::Unauthorized));
    let harness = Harness::new(&fake);
    harness.login("http://h", "alice");
    harness.expect(&[SessionState::Connecting, logged_in("http://h", "alice")]);
    harness.logout();
    harness.expect(&[logged_out(None)]);
    fake.wait_for_logout_call();
}

// AUTH-005
#[test]
fn logout_while_connecting_cancels_the_login() {
    let fake = Fake::new();
    let gate = Arc::new(Notify::new());
    fake.script_login(Some(Arc::clone(&gate)), Ok(()));
    fake.script_login(None, Err(ApiError::InvalidCredentials));
    let harness = Harness::new(&fake);
    harness.login("http://h", "alice");
    harness.expect(&[SessionState::Connecting]);
    // Cancel only once the attempt is really waiting at the gate.
    fake.wait_for_login_calls(1);

    harness.logout();
    harness.expect(&[logged_out(None)]);

    // The cancelled attempt must not surface, whatever it does now.
    gate.notify_one();
    harness.login("http://h", "bob");
    harness.expect(&[
        SessionState::Connecting,
        logged_out(Some(LoginFailure::InvalidCredentials)),
    ]);
}

// AUTH-001
#[test]
fn login_while_connecting_is_ignored() {
    let fake = Fake::new();
    let gate = Arc::new(Notify::new());
    fake.script_login(Some(Arc::clone(&gate)), Ok(()));
    let harness = Harness::new(&fake);
    harness.login("http://h", "alice");
    harness.login("http://h", "bob");
    harness.expect(&[SessionState::Connecting]);

    gate.notify_one();
    harness.expect(&[logged_in("http://h", "alice")]);
    // Nothing extra came in between: the next event is the logout's.
    harness.logout();
    harness.expect(&[logged_out(None)]);
    assert_eq!(fake.login_calls().len(), 1);
}

// AUTH-002
#[test]
fn a_new_login_clears_the_old_failure() {
    let fake = Fake::new();
    fake.script_login(None, Err(ApiError::InvalidCredentials));
    fake.script_login(None, Ok(()));
    let harness = Harness::new(&fake);
    harness.login("http://h", "alice");
    harness.expect(&[
        SessionState::Connecting,
        logged_out(Some(LoginFailure::InvalidCredentials)),
    ]);
    harness.login("http://h", "alice");
    harness.expect(&[SessionState::Connecting, logged_in("http://h", "alice")]);
}

// AUTH-004
#[test]
fn an_empty_store_starts_logged_out() {
    let harness = Harness::new(&Fake::new());
    assert_eq!(harness.first, logged_out(None));
}

// AUTH-004
#[test]
fn a_stored_session_is_resumed_without_logging_in() {
    let fake = Fake::new();
    let harness = Harness::with_store(&fake, store_with("http://h"));
    assert_eq!(harness.first, SessionState::Resuming);
    harness.expect(&[logged_in("http://h", "alice")]);
    assert_eq!(
        fake.state().me_calls,
        [("uid".to_owned(), "stored-tok".to_owned())]
    );
    assert!(fake.login_calls().is_empty());
}

// AUTH-004
#[test]
fn a_rejected_stored_session_expires_and_is_forgotten() {
    let fake = Fake::new();
    fake.state().me_result = Some(Err(ApiError::Unauthorized));
    let store = store_with("http://h");
    let harness = Harness::with_store(&fake, store.clone());
    harness.expect(&[logged_out(Some(LoginFailure::SessionExpired))]);
    wait_for_empty(&store);
}

// AUTH-004
#[test]
fn a_login_after_a_failed_resume_is_connecting_not_resuming() {
    let fake = Fake::new();
    fake.state().me_result = Some(Err(ApiError::Unauthorized));
    fake.script_login(None, Ok(()));
    let harness = Harness::with_store(&fake, store_with("http://h"));
    assert_eq!(harness.first, SessionState::Resuming);
    harness.expect(&[logged_out(Some(LoginFailure::SessionExpired))]);
    harness.login("http://h", "alice");
    harness.expect(&[SessionState::Connecting, logged_in("http://h", "alice")]);
}

// AUTH-004
#[test]
fn an_unreachable_server_keeps_the_stored_session() {
    let fake = Fake::new();
    fake.state().version = Some(Err(ApiError::Unreachable("refused".into())));
    let store = store_with("http://h");
    let harness = Harness::with_store(&fake, store.clone());
    harness.expect(&[logged_out(Some(LoginFailure::Unreachable))]);
    // The state is final before any store write would have been queued; flush the queue by
    // dropping the client.
    drop(harness);
    assert_eq!(store.load().unwrap().len(), 1);
}

// AUTH-004
#[test]
fn a_server_that_is_now_too_old_keeps_the_stored_session() {
    let fake = Fake::new();
    fake.state().version = Some(Ok(ServerVersion { major: 7, minor: 9 }));
    let store = store_with("http://h");
    let harness = Harness::with_store(&fake, store.clone());
    let failure = LoginFailure::UnsupportedVersion {
        found: "7.9".into(),
        minimum: "8.0".into(),
    };
    harness.expect(&[logged_out(Some(failure))]);
    drop(harness);
    assert_eq!(store.load().unwrap().len(), 1);
    assert!(fake.state().me_calls.is_empty());
}

// AUTH-004
#[test]
fn a_store_that_cant_be_read_starts_logged_out_and_logins_still_work() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    let store = FakeStore {
        fail_load: true,
        ..FakeStore::default()
    };
    let harness = Harness::with_store(&fake, store);
    assert_eq!(harness.first, logged_out(None));
    harness.login("http://h", "alice");
    harness.expect(&[SessionState::Connecting, logged_in("http://h", "alice")]);
}

// AUTH-004
#[test]
fn a_session_survives_a_restart() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    let store = MemoryStore::default();

    let first = Harness::with_store(&fake, store.clone());
    first.login("http://h", "alice");
    first.expect(&[SessionState::Connecting, logged_in("http://h", "alice")]);
    // Dropping flushes the queued save.
    drop(first);

    let second = Harness::with_store(&fake, store);
    assert_eq!(second.first, SessionState::Resuming);
    second.expect(&[logged_in("http://h", "alice")]);
    assert_eq!(fake.login_calls().len(), 1);
}

// AUTH-001
// AUTH-004
#[test]
fn login_saves_the_account_under_the_actors_id() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    let store = MemoryStore::default();
    let harness = Harness::with_store(&fake, store.clone());
    harness.login("chat.example.com", "alice");
    harness.expect(&[
        SessionState::Connecting,
        logged_in("https://chat.example.com", "alice"),
    ]);
    let saved = wait_until(|| store.load().unwrap().into_iter().next());
    assert_eq!(saved.server, "https://chat.example.com");
    assert_eq!(saved.username, "alice");
    assert_eq!(saved.user_id.as_str(), "uid");
    assert_eq!(saved.token.as_ref().unwrap().expose_secret(), "tok");
    // A fresh actor has an id of its own, and the next login of the same actor reuses it.
    harness.logout();
    harness.expect(&[logged_out(None)]);
    fake.script_login(None, Ok(()));
    harness.login("chat.example.com", "alice");
    harness.expect(&[
        SessionState::Connecting,
        logged_in("https://chat.example.com", "alice"),
    ]);
    drop(harness);
    let accounts = store.load().unwrap();
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].id, saved.id);
}

// AUTH-001
#[test]
fn a_store_that_cant_be_written_does_not_stop_the_login() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    let store = FakeStore {
        fail_save: true,
        ..FakeStore::default()
    };
    let harness = Harness::with_store(&fake, store);
    harness.login("http://h", "alice");
    harness.expect(&[SessionState::Connecting, logged_in("http://h", "alice")]);
}

// AUTH-005
#[test]
fn logout_forgets_the_stored_session() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    let store = MemoryStore::default();
    let harness = Harness::with_store(&fake, store.clone());
    harness.login("http://h", "alice");
    harness.expect(&[SessionState::Connecting, logged_in("http://h", "alice")]);
    harness.logout();
    harness.expect(&[logged_out(None)]);
    // The save and the clear are applied in order, so the store ends up empty.
    wait_for_empty(&store);
    fake.wait_for_logout_call();
}

// AUTH-005
#[test]
fn dropping_the_client_after_logout_finishes_the_logout() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    fake.state().logout_delay = Duration::from_millis(200);
    let store = MemoryStore::default();
    let harness = Harness::with_store(&fake, store.clone());
    harness.login("http://h", "alice");
    harness.expect(&[SessionState::Connecting, logged_in("http://h", "alice")]);
    harness.logout();
    harness.expect(&[logged_out(None)]);

    drop(harness);

    assert_eq!(fake.state().logouts_done, 1);
    assert!(store.load().unwrap().is_empty());
}

// AUTH-005
#[test]
fn logout_while_resuming_cancels_and_forgets() {
    let fake = Fake::new();
    let gate = Arc::new(Notify::new());
    fake.state().me_gate = Some(Arc::clone(&gate));
    let store = store_with("http://h");
    let harness = Harness::with_store(&fake, store.clone());
    assert_eq!(harness.first, SessionState::Resuming);
    fake.wait_for_me_call();

    harness.logout();

    harness.expect(&[logged_out(None)]);
    wait_for_empty(&store);
    assert_eq!(
        fake.wait_for_logout_call(),
        ("uid".to_owned(), "stored-tok".to_owned())
    );
}

// AUTH-005
// ACCT-002
#[test]
fn logout_removes_the_account_of_the_actor_and_leaves_the_others() {
    let fake = Fake::new();
    let store = store_with("http://h");
    store.save(&stored_as("other-id", "http://other")).unwrap();
    // The other account runs too.
    let harness = Harness::with_accounts(&fake, store.clone());
    harness.expect(&[logged_in("http://h", "alice")]);
    harness.logout();
    harness.expect(&[logged_out(None)]);
    drop(harness);
    let ids: Vec<String> = store
        .load()
        .unwrap()
        .iter()
        .map(|account| account.id.to_string())
        .collect();
    assert_eq!(ids, ["other-id"]);
}

// AUTH-004
// ACCT-002
#[test]
fn a_resume_uses_the_stored_id() {
    let fake = Fake::new();
    let store = MemoryStore::default();
    store.save(&stored_as("first-id", "http://h")).unwrap();
    let harness = Harness::with_store(&fake, store.clone());
    harness.expect(&[logged_in("http://h", "alice")]);
    // Nothing is saved by a resume, and logging out removes the account that was resumed.
    harness.logout();
    harness.expect(&[logged_out(None)]);
    wait_for_empty(&store);
}

// AUTH-004
// ACCT-002
#[test]
fn with_three_stored_accounts_the_ones_with_a_token_resume_and_a_logout_removes_only_its_own() {
    let fake = Fake::new();
    let store = MemoryStore::default();
    let mut signed_out = stored_as("signed-out-id", "http://signed-out");
    signed_out.token = None;
    store.save(&signed_out).unwrap();
    store
        .save(&stored_as("second-id", "http://second"))
        .unwrap();
    store.save(&stored_as("third-id", "http://third")).unwrap();
    let harness = Harness::with_accounts(&fake, store.clone());
    // The signed-out account is the first.
    assert_eq!(harness.first, logged_out(None));
    let second = AccountId::from("second-id".to_owned());
    harness.expect_of(
        &second,
        &[SessionState::Resuming, logged_in("http://second", "alice")],
    );
    harness.logout_account(&second);
    harness.expect_of(&second, &[logged_out(None)]);
    drop(harness);
    // Only the account that logged out went; the signed-out one and the other are untouched.
    let ids: Vec<String> = store
        .load()
        .unwrap()
        .iter()
        .map(|account| account.id.to_string())
        .collect();
    assert_eq!(ids, ["signed-out-id", "third-id"]);
}

// AUTH-004
// ACCT-002
#[test]
fn an_account_without_a_token_is_not_resumed_and_stays_stored() {
    let fake = Fake::new();
    let store = MemoryStore::default();
    let mut signed_out = stored("http://h");
    signed_out.token = None;
    store.save(&signed_out).unwrap();
    let harness = Harness::with_store(&fake, store.clone());
    assert_eq!(harness.first, logged_out(None));
    assert!(fake.state().me_calls.is_empty());
    drop(harness);
    let accounts = store.load().unwrap();
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].id.as_str(), STORED_ID);
    assert!(accounts[0].token.is_none());
}
