// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

use std::time::Duration;

use super::*;

const FIVE_MINUTES: Duration = Duration::from_secs(300);

fn ended(events: &[GithubEvent]) -> Vec<(PrKey, CiState)> {
    events
        .iter()
        .filter_map(|e| match e {
            GithubEvent::CiEnded { pr, state } => Some((pr.clone(), *state)),
            _ => None,
        })
        .collect()
}

fn notifying(events: &[GithubEvent]) -> Vec<Vec<PrKey>> {
    events
        .iter()
        .filter_map(|e| match e {
            GithubEvent::Notifying(all) => Some(all.clone()),
            _ => None,
        })
        .collect()
}

fn running() -> Harness {
    let h = Harness::new();
    h.next();
    h.github_api.state().mine = Ok(vec![pr(1, 1, CiState::Pending), pr(2, 2, CiState::Success)]);
    h.github_api.state().watched = Ok(vec![pr(5, 5, CiState::Pending)]);
    h.connect();
    h
}

// GH-008
#[test]
fn a_pull_request_in_the_set_reports_once_when_its_ci_passes() {
    let h = running();
    h.github().notify_when_done(key(1), true);
    assert_eq!(h.next(), GithubEvent::Notifying(vec![key(1)]));

    // Still running: nothing.
    let events = h.advance_to_refresh(FIVE_MINUTES);
    assert!(ended(&events).is_empty());

    h.github_api.state().mine = Ok(vec![pr(1, 1, CiState::Success), pr(2, 2, CiState::Success)]);
    let events = h.advance_to_refresh(FIVE_MINUTES);
    assert_eq!(ended(&events), [(key(1), CiState::Success)]);
    assert_eq!(notifying(&events), [Vec::<PrKey>::new()]);
    // The event comes before the set shrinks.
    let at = |f: fn(&GithubEvent) -> bool| events.iter().position(f).unwrap();
    assert!(
        at(|e| matches!(e, GithubEvent::CiEnded { .. }))
            < at(|e| matches!(e, GithubEvent::Notifying(_)))
    );

    // Once only, even if it runs again and ends again.
    h.github_api.state().mine = Ok(vec![pr(1, 1, CiState::Pending), pr(2, 2, CiState::Success)]);
    h.advance_to_refresh(FIVE_MINUTES);
    h.github_api.state().mine = Ok(vec![pr(1, 1, CiState::Failure), pr(2, 2, CiState::Success)]);
    let events = h.advance_to_refresh(FIVE_MINUTES);
    assert!(ended(&events).is_empty());
}

// GH-008
#[test]
fn a_failure_ends_the_wait_too() {
    let h = running();
    h.github().notify_when_done(key(1), true);
    h.next();
    h.github_api.state().mine = Ok(vec![pr(1, 1, CiState::Failure), pr(2, 2, CiState::Success)]);
    let events = h.advance_to_refresh(FIVE_MINUTES);
    assert_eq!(ended(&events), [(key(1), CiState::Failure)]);
}

// GH-008
#[test]
fn a_watched_pull_request_can_be_waited_for() {
    let h = running();
    h.github().notify_when_done(key(5), true);
    assert_eq!(h.next(), GithubEvent::Notifying(vec![key(5)]));
    h.github_api.state().watched = Ok(vec![pr(5, 5, CiState::Success)]);
    let events = h.advance_to_refresh(FIVE_MINUTES);
    assert_eq!(ended(&events), [(key(5), CiState::Success)]);
}

// GH-008
#[test]
fn a_pull_request_that_goes_away_is_dropped_without_a_notification() {
    let h = running();
    h.github().notify_when_done(key(1), true);
    h.github().notify_when_done(key(5), true);
    h.next();
    h.next();
    // 1 is merged; 5 stays.
    h.github_api.state().mine = Ok(vec![pr(2, 2, CiState::Success)]);
    let events = h.advance_to_refresh(FIVE_MINUTES);
    assert!(ended(&events).is_empty());
    assert_eq!(notifying(&events), [vec![key(5)]]);
}

// GH-008
#[test]
fn a_failed_refresh_does_not_make_a_pull_request_look_gone() {
    let h = running();
    h.github().notify_when_done(key(1), true);
    h.next();
    h.github_api.state().mine = Err(crate::github::GithubError::Unreachable);
    let events = h.advance_to_refresh(FIVE_MINUTES);
    assert!(notifying(&events).is_empty());
    assert!(ended(&events).is_empty());
}

// GH-008
#[test]
fn only_a_known_pull_request_with_running_ci_can_be_added_and_it_can_be_taken_back() {
    let h = running();
    // Unknown, and not running.
    h.github().notify_when_done(key(99), true);
    h.github().notify_when_done(key(2), true);
    h.no_more_events();

    h.github().notify_when_done(key(1), true);
    assert_eq!(h.next(), GithubEvent::Notifying(vec![key(1)]));
    // Adding again changes nothing.
    h.github().notify_when_done(key(1), true);
    h.github().notify_when_done(key(5), true);
    assert_eq!(h.next(), GithubEvent::Notifying(vec![key(1), key(5)]));
    h.github().notify_when_done(key(1), false);
    assert_eq!(h.next(), GithubEvent::Notifying(vec![key(5)]));
    h.github().notify_when_done(key(1), false);
    h.no_more_events();

    h.github_api.state().mine = Ok(vec![pr(1, 1, CiState::Success), pr(2, 2, CiState::Success)]);
    let events = h.advance_to_refresh(FIVE_MINUTES);
    assert!(ended(&events).is_empty());
}
