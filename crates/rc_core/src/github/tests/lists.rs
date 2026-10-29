// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

use std::time::Duration;

use super::*;
use crate::github::{GithubError, RepoRunners, Runner};

const FIVE_MINUTES: Duration = Duration::from_secs(300);

fn titles(changes: &[ListChange<PullRequest>]) -> Vec<u64> {
    changes
        .iter()
        .flat_map(|c| match c {
            ListChange::Reset(all) => all.iter().map(|p| p.key.number).collect(),
            _ => Vec::new(),
        })
        .collect()
}

// GH-001
#[test]
fn mine_is_ordered_by_latest_change_with_the_drafts_last() {
    let h = Harness::new();
    h.next();
    h.github_api.state().mine = Ok(vec![
        draft(1, 50),
        pr(2, 10, CiState::Success),
        draft(3, 40),
        pr(4, 30, CiState::Pending),
    ]);
    let events = h.connect();
    assert_eq!(titles(&mine_changes(&events)), [4, 2, 1, 3]);
}

// GH-002
#[test]
fn watched_has_its_own_list_and_changes() {
    let h = Harness::new();
    h.next();
    h.github_api.state().mine = Ok(vec![pr(1, 1, CiState::Success)]);
    h.github_api.state().watched = Ok(vec![pr(7, 3, CiState::Pending), draft(8, 9)]);
    let events = h.connect();
    assert_eq!(titles(&mine_changes(&events)), [1]);
    assert_eq!(titles(&watched_changes(&events)), [7, 8]);

    h.github_api.state().watched = Ok(vec![pr(7, 3, CiState::Success), draft(8, 9)]);
    let events = h.advance_to_refresh(FIVE_MINUTES);
    assert!(mine_changes(&events).is_empty());
    assert_eq!(
        watched_changes(&events),
        [ListChange::Updated {
            index: 0,
            item: pr(7, 3, CiState::Success)
        }]
    );
}

// GH-001
#[test]
fn a_refresh_costs_only_what_changed() {
    let h = Harness::new();
    h.next();
    let many: Vec<PullRequest> = (1..=300)
        .map(|n| pr(n, i64::try_from(n).unwrap(), CiState::Success))
        .collect();
    h.github_api.state().mine = Ok(many.clone());
    let events = h.connect();
    assert_eq!(titles(&mine_changes(&events)).len(), 300);

    // Nothing changed: no list event at all.
    let events = h.advance_to_refresh(FIVE_MINUTES);
    assert_eq!(
        events,
        [
            GithubEvent::Refresh(RefreshState::Started),
            GithubEvent::Refresh(RefreshState::Done {
                at: TestClock::start() + FIVE_MINUTES
            })
        ]
    );

    // One CI turned red, one got a push, one was merged, a new one opened.
    let mut next = many;
    next[99].ci = CiState::Failure;
    next[9] = pr(10, 1000, CiState::Success);
    next.remove(199);
    next.push(pr(301, 2, CiState::Pending));
    h.github_api.state().mine = Ok(next);
    let events = h.advance_to_refresh(FIVE_MINUTES);
    let changes = mine_changes(&events);
    // Newest first, so number 200 sits at index 100 and goes; 10, which was at 290 (289 by then),
    // moves to the top; 301 lands behind 2; and 100 is at 200 in the end.
    assert_eq!(
        changes,
        [
            ListChange::Removed { index: 100 },
            ListChange::Moved {
                from: 289,
                to: 0,
                item: pr(10, 1000, CiState::Success)
            },
            ListChange::Inserted {
                index: 298,
                item: pr(301, 2, CiState::Pending)
            },
            ListChange::Updated {
                index: 200,
                item: {
                    let mut p = pr(100, 100, CiState::Success);
                    p.ci = CiState::Failure;
                    p
                }
            },
        ]
    );
}

// GH-001
#[test]
fn a_ci_update_a_push_a_merge_and_a_new_pull_request_are_told_exactly() {
    let h = Harness::new();
    h.next();
    h.github_api.state().mine = Ok(vec![
        pr(1, 30, CiState::Pending),
        pr(2, 20, CiState::Success),
        pr(3, 10, CiState::Success),
    ]);
    h.connect();

    // 1 finishes red.
    h.github_api.state().mine = Ok(vec![
        pr(1, 30, CiState::Failure),
        pr(2, 20, CiState::Success),
        pr(3, 10, CiState::Success),
    ]);
    let events = h.advance_to_refresh(FIVE_MINUTES);
    assert_eq!(
        mine_changes(&events),
        [ListChange::Updated {
            index: 0,
            item: pr(1, 30, CiState::Failure)
        }]
    );

    // 3 gets a push: it moves to the top.
    h.github_api.state().mine = Ok(vec![
        pr(1, 30, CiState::Failure),
        pr(2, 20, CiState::Success),
        pr(3, 40, CiState::Pending),
    ]);
    let events = h.advance_to_refresh(FIVE_MINUTES);
    assert_eq!(
        mine_changes(&events),
        [ListChange::Moved {
            from: 2,
            to: 0,
            item: pr(3, 40, CiState::Pending)
        }]
    );

    // 2 is merged, 5 opens, as a draft.
    h.github_api.state().mine = Ok(vec![
        pr(1, 30, CiState::Failure),
        pr(3, 40, CiState::Pending),
        draft(5, 50),
    ]);
    let events = h.advance_to_refresh(FIVE_MINUTES);
    assert_eq!(
        mine_changes(&events),
        [
            ListChange::Removed { index: 2 },
            ListChange::Inserted {
                index: 2,
                item: draft(5, 50)
            }
        ]
    );
}

// GH-001
#[test]
fn a_draft_made_ready_moves_up_and_one_made_a_draft_moves_down() {
    let h = Harness::new();
    h.next();
    h.github_api.state().mine = Ok(vec![pr(1, 30, CiState::Success), draft(2, 20)]);
    h.connect();
    let ready = pr(2, 20, CiState::None);
    h.github_api.state().mine = Ok(vec![pr(1, 30, CiState::Success), ready.clone()]);
    let events = h.advance_to_refresh(FIVE_MINUTES);
    // Still after 1 by time, so it updates in place.
    assert_eq!(
        mine_changes(&events),
        [ListChange::Updated {
            index: 1,
            item: ready.clone()
        }]
    );

    let back = PullRequest {
        is_draft: true,
        ..pr(1, 30, CiState::Success)
    };
    let ready_again = ready.clone();
    h.github_api.state().mine = Ok(vec![back.clone(), ready]);
    let events = h.advance_to_refresh(FIVE_MINUTES);
    assert_eq!(
        mine_changes(&events),
        [
            // Which of the two moves is up to the list; the other one changes in place.
            ListChange::Moved {
                from: 1,
                to: 0,
                item: ready_again
            },
            ListChange::Updated {
                index: 1,
                item: back
            }
        ]
    );
}

// GH-001
#[test]
fn the_tile_counts_failing_pull_requests_that_are_not_drafts_when_it_changes() {
    let h = Harness::new();
    h.next();
    let failing_draft = PullRequest {
        ci: CiState::Failure,
        ..draft(3, 3)
    };
    h.github_api.state().mine = Ok(vec![
        pr(1, 1, CiState::Failure),
        pr(2, 2, CiState::Failure),
        failing_draft.clone(),
    ]);
    let events = h.connect();
    let counts: Vec<usize> = events
        .iter()
        .filter_map(|e| match e {
            GithubEvent::Failing(n) => Some(*n),
            _ => None,
        })
        .collect();
    assert_eq!(counts, [2]);

    // Unchanged: nothing. One fixed: 1. Both fixed: 0.
    let events = h.advance_to_refresh(FIVE_MINUTES);
    assert!(!events.iter().any(|e| matches!(e, GithubEvent::Failing(_))));
    h.github_api.state().mine = Ok(vec![
        pr(1, 1, CiState::Failure),
        pr(2, 2, CiState::Success),
        failing_draft.clone(),
    ]);
    let events = h.advance_to_refresh(FIVE_MINUTES);
    assert!(events.contains(&GithubEvent::Failing(1)));
    h.github_api.state().mine = Ok(vec![failing_draft]);
    let events = h.advance_to_refresh(FIVE_MINUTES);
    assert!(events.contains(&GithubEvent::Failing(0)));
}

// GH-007
#[test]
fn no_runners_are_asked_for_until_repositories_are_configured() {
    let h = Harness::new();
    h.next();
    let events = h.connect();
    assert!(!events.iter().any(|e| matches!(e, GithubEvent::Runners(_))));
    assert!(
        !h.github_api
            .calls()
            .iter()
            .any(|c| matches!(c, Call::Runners(_)))
    );
}

fn runner(name: &str, busy: bool) -> Runner {
    Runner {
        name: name.to_owned(),
        os: "Linux".to_owned(),
        online: true,
        busy,
        labels: vec!["gpu".to_owned()],
        job: None,
    }
}

// GH-007
#[test]
fn the_runners_of_the_configured_repositories_come_when_they_change() {
    let h = Harness::with(
        MemoryGithubStore::default(),
        GithubConfig {
            runner_repos: vec!["o/r".to_owned(), "o/private".to_owned()],
            ..GithubConfig::default()
        },
    );
    h.next();
    h.github_api
        .state()
        .runners
        .insert("o/r".to_owned(), Ok(vec![runner("one", false)]));
    // No admin rights on the second one: it has none to show.
    h.github_api.state().runners.insert(
        "o/private".to_owned(),
        Err(GithubError::Forbidden("no".to_owned())),
    );
    let events = h.connect();
    let expected = vec![RepoRunners {
        repo: "o/r".to_owned(),
        runners: vec![runner("one", false)],
    }];
    assert!(events.contains(&GithubEvent::Runners(expected.clone())));

    // Unchanged: nothing.
    let events = h.advance_to_refresh(FIVE_MINUTES);
    assert!(!events.iter().any(|e| matches!(e, GithubEvent::Runners(_))));

    // Busy now.
    h.github_api
        .state()
        .runners
        .insert("o/r".to_owned(), Ok(vec![runner("one", true)]));
    let events = h.advance_to_refresh(FIVE_MINUTES);
    assert!(events.contains(&GithubEvent::Runners(vec![RepoRunners {
        repo: "o/r".to_owned(),
        runners: vec![runner("one", true)],
    }])));

    // A failed fetch keeps what was shown.
    h.github_api
        .state()
        .runners
        .insert("o/r".to_owned(), Err(GithubError::Unreachable));
    let events = h.advance_to_refresh(FIVE_MINUTES);
    assert!(!events.iter().any(|e| matches!(e, GithubEvent::Runners(_))));
}
