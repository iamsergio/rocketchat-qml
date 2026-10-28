// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

use std::time::Duration;

use jiff::civil::date;

use super::*;
use crate::github::GithubError;

const SECS: fn(u64) -> Duration = Duration::from_secs;

fn refreshes(h: &Harness) -> usize {
    h.github_api.count(&Call::Mine)
}

fn wait_for_refreshes(h: &Harness, count: usize) {
    wait_until(|| (refreshes(h) == count).then_some(()));
}

// GH-001
#[test]
fn connecting_fetches_at_once_and_reports_in_order() {
    let h = Harness::new();
    h.next();
    h.github_api.state().mine = Ok(vec![pr(1, 1, CiState::Failure)]);
    h.github_api.state().watched = Ok(vec![pr(2, 2, CiState::Success)]);
    h.github_api.state().repos = Ok(vec!["o/r".to_owned()]);
    let events = h.connect();
    let kinds: Vec<&str> = events
        .iter()
        .map(|e| match e {
            GithubEvent::Connection(Connection::Connecting) => "connecting",
            GithubEvent::Connection(Connection::Connected { .. }) => "connected",
            GithubEvent::Refresh(RefreshState::Started) => "started",
            GithubEvent::Mine(_) => "mine",
            GithubEvent::Watched(_) => "watched",
            GithubEvent::Failing(1) => "failing",
            GithubEvent::Refresh(RefreshState::Done { .. }) => "done",
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "connecting",
            "connected",
            "started",
            "mine",
            "watched",
            "failing",
            "done"
        ]
    );
    assert_eq!(
        events.last(),
        Some(&GithubEvent::Refresh(RefreshState::Done {
            at: TestClock::start()
        }))
    );
}

// GH-001
#[test]
fn off_screen_it_fetches_every_five_minutes() {
    let h = Harness::new();
    h.next();
    h.connect();
    wait_for_refreshes(&h, 1);
    h.advance_quietly(SECS(299));
    h.advance_to_refresh(SECS(1));
    wait_for_refreshes(&h, 2);
    h.advance_quietly(SECS(299));
    h.advance_to_refresh(SECS(1));
    wait_for_refreshes(&h, 3);
}

// GH-001
#[test]
fn on_screen_it_fetches_every_minute() {
    let h = Harness::new();
    h.next();
    h.github().set_on_screen(true);
    h.connect();
    h.advance_quietly(SECS(59));
    h.advance_to_refresh(SECS(1));
    wait_for_refreshes(&h, 2);
    h.advance_quietly(SECS(59));
    h.advance_to_refresh(SECS(1));
    wait_for_refreshes(&h, 3);
}

// GH-001
#[test]
fn coming_on_screen_after_a_while_fetches_at_once_and_leaving_slows_down() {
    let h = Harness::new();
    h.next();
    h.connect();
    // Two minutes off screen: the next fetch is three minutes away.
    h.advance_quietly(SECS(120));
    h.github().set_on_screen(true);
    h.until_refreshed();
    wait_for_refreshes(&h, 2);

    // Off screen again: the minute that is left no longer counts.
    h.github().set_on_screen(false);
    h.advance_quietly(SECS(60));
    h.advance_to_refresh(SECS(240));
    wait_for_refreshes(&h, 3);
}

// GH-001
#[test]
fn refresh_fetches_at_once_whatever_the_interval() {
    let h = Harness::new();
    h.next();
    h.connect();
    h.github().refresh();
    let events = h.until_refreshed();
    assert_eq!(
        events.first(),
        Some(&GithubEvent::Refresh(RefreshState::Started))
    );
    wait_for_refreshes(&h, 2);
}

// GH-001
#[test]
fn refresh_while_disconnected_is_ignored() {
    let h = Harness::new();
    h.next();
    h.github().refresh();
    h.no_more_events();
    assert!(h.github_api.calls().is_empty());
}

// GH-002
#[test]
fn the_watched_window_is_fourteen_days_back_from_today() {
    let h = Harness::new();
    h.next();
    h.github_api.state().repos = Ok(vec!["a/b".to_owned(), "c/d".to_owned()]);
    h.connect();
    assert_eq!(
        h.github_api.count(&Call::Watched {
            repos: vec!["a/b".to_owned(), "c/d".to_owned()],
            // The clock reads 2026-01-15.
            since: date(2026, 1, 1),
        }),
        1
    );
}

// GH-002
#[test]
fn the_window_follows_the_setting() {
    let h = Harness::with(
        MemoryGithubStore::default(),
        GithubConfig {
            watched_days: 3,
            ..GithubConfig::default()
        },
    );
    h.next();
    h.connect();
    assert!(h.github_api.calls().contains(&Call::Watched {
        repos: Vec::new(),
        since: date(2026, 1, 12)
    }));
}

// GH-002
#[test]
fn the_repositories_are_fetched_again_only_when_old_or_asked_for() {
    let h = Harness::new();
    h.next();
    h.connect();
    assert_eq!(h.github_api.count(&Call::Repos), 1);
    h.advance_to_refresh(SECS(300));
    h.advance_to_refresh(SECS(300));
    assert_eq!(h.github_api.count(&Call::Repos), 1);
    assert_eq!(refreshes(&h), 3);

    // The user asks for a refresh.
    h.github().refresh();
    h.until_refreshed();
    assert_eq!(h.github_api.count(&Call::Repos), 2);

    // Half an hour later the list is old.
    for _ in 0..5 {
        h.advance_to_refresh(SECS(300));
    }
    assert_eq!(h.github_api.count(&Call::Repos), 2);
    h.advance_to_refresh(SECS(300));
    assert_eq!(h.github_api.count(&Call::Repos), 3);
}

// GH-001
#[test]
fn a_failed_refresh_keeps_the_lists_and_says_when_the_last_good_one_was() {
    let h = Harness::new();
    h.next();
    h.github_api.state().mine = Ok(vec![pr(1, 1, CiState::Success)]);
    h.connect();

    h.github_api.state().mine = Err(GithubError::Unreachable);
    let events = h.advance_to_refresh(SECS(300));
    assert_eq!(
        events,
        [
            GithubEvent::Refresh(RefreshState::Started),
            GithubEvent::Refresh(RefreshState::Failed {
                error: GithubError::Unreachable,
                last_good: Some(TestClock::start()),
            }),
        ]
    );

    // It tries again at the next interval, and recovers.
    h.github_api.state().mine = Ok(vec![pr(1, 1, CiState::Success)]);
    let events = h.advance_to_refresh(SECS(300));
    assert_eq!(
        events.last(),
        Some(&GithubEvent::Refresh(RefreshState::Done {
            at: TestClock::start() + SECS(600)
        }))
    );
    // The row never changed.
    assert!(mine_changes(&events).is_empty());
}

// GH-001
#[test]
fn what_could_be_fetched_is_shown_when_the_other_list_fails() {
    let h = Harness::new();
    h.next();
    h.github_api.state().mine = Ok(vec![pr(1, 1, CiState::Success)]);
    h.github_api.state().watched = Err(GithubError::Other("search failed".to_owned()));
    let events = h.connect();
    assert!(matches!(
        mine_changes(&events).as_slice(),
        [ListChange::Reset(all)] if all.len() == 1
    ));
    assert_eq!(
        events.last(),
        Some(&GithubEvent::Refresh(RefreshState::Failed {
            error: GithubError::Other("search failed".to_owned()),
            last_good: None,
        }))
    );
}

// GH-001
#[test]
fn a_rate_limit_holds_every_request_back_until_the_reset() {
    let h = Harness::new();
    h.next();
    h.github().set_on_screen(true);
    h.connect();

    // Limited for two minutes by the primary limit.
    let reset = TestClock::start() + SECS(60) + SECS(120);
    h.github_api.state().mine = Err(GithubError::RateLimited {
        reset: Some(reset),
        retry_after: None,
    });
    let events = h.advance_to_refresh(SECS(60));
    assert!(matches!(
        events.last(),
        Some(GithubEvent::Refresh(RefreshState::Failed {
            error: GithubError::RateLimited { .. },
            ..
        }))
    ));
    wait_for_refreshes(&h, 2);

    // The minute interval passes twice without a request.
    h.github_api.state().mine = Ok(Vec::new());
    h.advance_quietly(SECS(60));
    h.advance_quietly(SECS(59));
    // And a manual refresh waits too.
    h.github().refresh();
    h.advance_to_refresh(SECS(1));
    wait_for_refreshes(&h, 3);
}

// GH-001
#[test]
fn a_secondary_limit_waits_for_retry_after() {
    let h = Harness::new();
    h.next();
    h.github().set_on_screen(true);
    h.connect();
    h.github_api.state().mine = Err(GithubError::RateLimited {
        reset: None,
        retry_after: Some(SECS(180)),
    });
    h.advance_to_refresh(SECS(60));
    h.github_api.state().mine = Ok(Vec::new());
    h.advance_quietly(SECS(179));
    h.advance_to_refresh(SECS(1));
    wait_for_refreshes(&h, 3);
}

// GH-001
#[test]
fn a_rate_limit_without_a_time_waits_a_minute() {
    let h = Harness::new();
    h.next();
    h.connect();
    h.github_api.state().mine = Err(GithubError::RateLimited {
        reset: None,
        retry_after: None,
    });
    h.advance_to_refresh(SECS(300));
    // A manual refresh would be due at once, but the limit holds it back for a minute.
    h.github_api.state().mine = Ok(Vec::new());
    h.github().refresh();
    h.advance_quietly(SECS(59));
    h.advance_to_refresh(SECS(1));
    wait_for_refreshes(&h, 3);
}

// GH-001
#[test]
fn a_reset_that_has_passed_still_waits_a_few_seconds() {
    let h = Harness::new();
    h.next();
    h.connect();
    h.github_api.state().mine = Err(GithubError::RateLimited {
        reset: Some(TestClock::start()),
        retry_after: None,
    });
    h.advance_to_refresh(SECS(300));
    h.github_api.state().mine = Ok(Vec::new());
    h.github().refresh();
    h.advance_quietly(SECS(4));
    h.advance_to_refresh(SECS(1));
    wait_for_refreshes(&h, 3);
}
