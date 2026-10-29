// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

use secrecy::ExposeSecret;

use super::*;
use crate::github::{ConnectFailure, GhError, GithubError, GithubStore};

fn connected(login: &str) -> GithubEvent {
    GithubEvent::Connection(Connection::Connected {
        login: login.to_owned(),
    })
}

fn failed(failure: ConnectFailure) -> GithubEvent {
    GithubEvent::Connection(Connection::Failed(failure))
}

// GH-009
#[test]
fn nothing_stored_starts_not_connected_and_asks_nobody() {
    let h = Harness::new();
    assert_eq!(h.next(), GithubEvent::Connection(Connection::NotConnected));
    h.no_more_events();
    assert!(h.github_api.calls().is_empty());
    assert_eq!(h.gh.asked(), 0);
}

// GH-009
#[test]
fn connecting_with_gh_uses_its_token_and_keeps_only_the_choice() {
    let h = Harness::new();
    h.next();
    h.github().connect_with_gh();
    assert_eq!(h.next(), GithubEvent::Connection(Connection::Connecting));
    assert_eq!(h.next(), connected("alice"));
    assert_eq!(h.github_api.state().tokens, ["gh-token"]);
    // The token stays in gh's hands.
    wait_until(|| (h.stored() == Some(GithubAuth::GhCli)).then_some(()));
}

// GH-009
#[test]
fn connecting_with_a_pasted_token_trims_it_checks_it_and_keeps_it() {
    let h = Harness::new();
    h.next();
    h.github().connect_with_token(token(" ghp_pasted\n"));
    assert_eq!(h.next(), GithubEvent::Connection(Connection::Connecting));
    assert_eq!(h.next(), connected("alice"));
    assert_eq!(h.github_api.state().tokens, ["ghp_pasted"]);
    assert_eq!(h.github_api.count(&Call::Viewer), 1);
    wait_until(|| match h.stored() {
        Some(GithubAuth::Token(t)) => (t.expose_secret() == "ghp_pasted").then_some(()),
        _ => None,
    });
}

// GH-009
#[test]
fn a_refused_token_fails_and_keeps_nothing() {
    let h = Harness::new();
    h.next();
    h.github_api.state().viewer = Err(GithubError::Unauthorized);
    h.github().connect_with_token(token("bad"));
    assert_eq!(h.next(), GithubEvent::Connection(Connection::Connecting));
    assert_eq!(h.next(), failed(ConnectFailure::BadToken));
    h.no_more_events();
    assert_eq!(h.stored(), None);
}

// GH-009
#[test]
fn a_blank_token_is_refused_without_asking_github() {
    let h = Harness::new();
    h.next();
    h.github().connect_with_token(token("  \n"));
    h.next();
    assert_eq!(h.next(), failed(ConnectFailure::BadToken));
    assert!(h.github_api.calls().is_empty());
}

// GH-009
#[test]
fn gh_that_is_missing_or_not_logged_in_is_a_state_not_a_crash() {
    let h = Harness::new();
    h.next();
    h.gh.set(Err(GhError::NotInstalled));
    h.github().connect_with_gh();
    h.next();
    assert_eq!(h.next(), failed(ConnectFailure::GhMissing));

    h.gh.set(Err(GhError::NotLoggedIn));
    h.github().connect_with_gh();
    h.next();
    assert_eq!(h.next(), failed(ConnectFailure::GhNotLoggedIn));

    h.gh.set(Err(GhError::Failed("boom".to_owned())));
    h.github().connect_with_gh();
    h.next();
    assert_eq!(h.next(), failed(ConnectFailure::Other("boom".to_owned())));
    assert!(h.github_api.calls().is_empty());
    assert_eq!(h.stored(), None);
}

// GH-009
#[test]
fn unreachable_and_rate_limited_github_are_told_apart() {
    let h = Harness::new();
    h.next();
    h.github_api.state().viewer = Err(GithubError::Unreachable);
    h.github().connect_with_token(token("t"));
    h.next();
    assert_eq!(h.next(), failed(ConnectFailure::Unreachable));

    h.github_api.state().viewer = Err(GithubError::RateLimited {
        reset: None,
        retry_after: None,
    });
    h.github().connect_with_token(token("t"));
    h.next();
    assert_eq!(h.next(), failed(ConnectFailure::RateLimited));

    h.github_api.state().viewer = Err(GithubError::Other("odd".to_owned()));
    h.github().connect_with_token(token("t"));
    h.next();
    assert_eq!(h.next(), failed(ConnectFailure::Other("odd".to_owned())));
}

// GH-009
#[test]
fn a_stored_gh_choice_is_resumed_at_startup_by_reading_gh_again() {
    let store = MemoryGithubStore::default();
    store.save(&GithubAuth::GhCli).unwrap();
    let h = Harness::with(store, GithubConfig::default());
    assert_eq!(h.next(), GithubEvent::Connection(Connection::Connecting));
    assert_eq!(h.next(), connected("alice"));
    assert_eq!(h.gh.asked(), 1);
    assert_eq!(h.github_api.state().tokens, ["gh-token"]);
}

// GH-009
#[test]
fn a_stored_token_is_resumed_at_startup() {
    let store = MemoryGithubStore::default();
    store.save(&GithubAuth::Token(token("ghp_stored"))).unwrap();
    let h = Harness::with(store, GithubConfig::default());
    assert_eq!(h.next(), GithubEvent::Connection(Connection::Connecting));
    assert_eq!(h.next(), connected("alice"));
    assert_eq!(h.gh.asked(), 0);
    assert_eq!(h.github_api.state().tokens, ["ghp_stored"]);
}

// GH-009
#[test]
fn a_failed_resume_keeps_the_choice_and_retry_tries_again() {
    let store = MemoryGithubStore::default();
    store.save(&GithubAuth::GhCli).unwrap();
    let gh = FakeGh::logged_in("gh-token");
    gh.set(Err(GhError::NotLoggedIn));
    let h = Harness::with_gh(store, GithubConfig::default(), gh);
    assert_eq!(h.next(), GithubEvent::Connection(Connection::Connecting));
    assert_eq!(h.next(), failed(ConnectFailure::GhNotLoggedIn));
    assert_eq!(h.stored(), Some(GithubAuth::GhCli));

    h.gh.set(Ok("gh-token"));
    h.github().retry();
    assert_eq!(h.next(), GithubEvent::Connection(Connection::Connecting));
    assert_eq!(h.next(), connected("alice"));
}

// GH-009
#[test]
fn retry_does_nothing_unless_a_connection_failed() {
    let h = Harness::new();
    h.next();
    h.github().retry();
    h.no_more_events();
}

// GH-009
#[test]
fn disconnecting_forgets_the_login_and_empties_what_was_shown() {
    let h = Harness::new();
    h.next();
    h.github_api.state().mine = Ok(vec![pr(1, 1, CiState::Failure), pr(2, 2, CiState::Pending)]);
    h.github_api.state().watched = Ok(vec![pr(3, 3, CiState::Success)]);
    h.github_api.state().repos = Ok(vec!["o/r".to_owned()]);
    h.connect();
    h.github().notify_when_done(key(2), true);
    h.until(|e| matches!(e, GithubEvent::Notifying(_)));
    wait_until(|| h.stored().map(|_| ()));

    h.github().disconnect();
    assert_eq!(
        h.next(),
        GithubEvent::Mine(vec![ListChange::Reset(Vec::new())])
    );
    assert_eq!(
        h.next(),
        GithubEvent::Watched(vec![ListChange::Reset(Vec::new())])
    );
    assert_eq!(h.next(), GithubEvent::Failing(0));
    assert_eq!(h.next(), GithubEvent::Notifying(Vec::new()));
    assert_eq!(h.next(), GithubEvent::Connection(Connection::NotConnected));
    wait_until(|| h.stored().is_none().then_some(()));

    // Nothing is fetched while disconnected, however long it takes.
    let calls = h.github_api.calls().len();
    h.advance_quietly(std::time::Duration::from_secs(3600));
    assert_eq!(h.github_api.calls().len(), calls);
}

// GH-009
#[test]
fn a_restart_after_disconnecting_finds_nothing() {
    let h = Harness::new();
    h.next();
    h.connect();
    wait_until(|| h.stored().map(|_| ()));
    h.github().disconnect();
    h.until(|e| matches!(e, GithubEvent::Connection(Connection::NotConnected)));
    wait_until(|| h.stored().is_none().then_some(()));
    let again = Harness::with(h.store.clone(), GithubConfig::default());
    assert_eq!(
        again.next(),
        GithubEvent::Connection(Connection::NotConnected)
    );
}

// GH-009
#[test]
fn a_token_that_stops_working_ends_the_connection_but_keeps_the_choice() {
    let h = Harness::new();
    h.next();
    h.github_api.state().mine = Ok(vec![pr(1, 1, CiState::Failure)]);
    h.connect();
    wait_until(|| h.stored().map(|_| ()));

    h.github_api.state().mine = Err(GithubError::Unauthorized);
    let events = h.advance_to_refresh(std::time::Duration::from_secs(300));
    assert!(matches!(
        events.last(),
        Some(GithubEvent::Refresh(RefreshState::Failed {
            error: GithubError::Unauthorized,
            ..
        }))
    ));
    assert_eq!(
        h.next(),
        GithubEvent::Mine(vec![ListChange::Reset(Vec::new())])
    );
    assert_eq!(h.next(), GithubEvent::Failing(0));
    assert_eq!(h.next(), failed(ConnectFailure::BadToken));
    assert!(h.stored().is_some());
}

// GH-009
#[test]
fn connecting_again_leaves_one_connection() {
    let h = Harness::new();
    h.next();
    h.github().connect_with_token(token("first"));
    h.github().connect_with_token(token("second"));
    wait_until(|| (h.github_api.state().tokens.len() == 2).then_some(()));
    let events: Vec<_> = std::iter::from_fn(|| {
        h.events
            .recv_timeout(std::time::Duration::from_millis(300))
            .ok()
    })
    .collect();
    // However far the first try got, after the last start there is exactly one end: the answer
    // of the first, if it comes late, is dropped.
    let last_start = events
        .iter()
        .rposition(|e| matches!(e, GithubEvent::Connection(Connection::Connecting)))
        .unwrap();
    let ends = events[last_start..]
        .iter()
        .filter(|e| matches!(e, GithubEvent::Connection(Connection::Connected { .. })))
        .count();
    assert_eq!(ends, 1, "{events:?}");
}
