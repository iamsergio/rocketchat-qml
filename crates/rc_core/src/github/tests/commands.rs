// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

use std::time::Duration;

use super::*;
use crate::github::{AttemptId, CommandState, GithubError, PrAction};

fn command(pr: u64, action: PrAction, attempt: AttemptId, state: CommandState) -> GithubEvent {
    GithubEvent::Command {
        pr: key(pr),
        action,
        attempt,
        state,
    }
}

fn is_command(event: &GithubEvent) -> bool {
    matches!(event, GithubEvent::Command { .. })
}

fn connected() -> Harness {
    let h = Harness::new();
    h.next();
    h.github_api.state().mine = Ok(vec![pr(1, 1, CiState::Failure), pr(2, 2, CiState::Success)]);
    h.connect();
    h
}

// GH-003
#[test]
fn merge_starts_and_is_done_and_asks_github_with_the_force() {
    let h = connected();
    h.github().merge(key(2), false);
    let a = AttemptId(1);
    let action = PrAction::Merge { force: false };
    assert_eq!(h.next(), command(2, action, a, CommandState::Started));
    assert_eq!(h.next(), command(2, action, a, CommandState::Done));

    h.github().merge(key(1), true);
    let action = PrAction::Merge { force: true };
    assert_eq!(
        h.next(),
        command(1, action, AttemptId(2), CommandState::Started)
    );
    assert_eq!(
        h.next(),
        command(1, action, AttemptId(2), CommandState::Done)
    );
    assert_eq!(
        h.github_api
            .calls()
            .iter()
            .filter(|c| matches!(c, Call::Merge { .. }))
            .cloned()
            .collect::<Vec<_>>(),
        [
            Call::Merge {
                pr: key(2),
                force: false
            },
            Call::Merge {
                pr: key(1),
                force: true
            },
        ]
    );
}

// GH-003
#[test]
fn a_refused_merge_says_why() {
    let h = connected();
    h.github_api.state().command = Err(GithubError::ChecksNotPassed(CiState::Failure));
    h.github().merge(key(1), false);
    let action = PrAction::Merge { force: false };
    h.next();
    assert_eq!(
        h.next(),
        command(
            1,
            action,
            AttemptId(1),
            CommandState::Failed(GithubError::ChecksNotPassed(CiState::Failure))
        )
    );
}

// GH-004
#[test]
fn update_branch_runs_and_reports() {
    let h = connected();
    h.github().update_branch(key(1));
    assert_eq!(
        h.next(),
        command(
            1,
            PrAction::UpdateBranch,
            AttemptId(1),
            CommandState::Started
        )
    );
    assert_eq!(
        h.next(),
        command(1, PrAction::UpdateBranch, AttemptId(1), CommandState::Done)
    );
    assert!(h.github_api.calls().contains(&Call::UpdateBranch(key(1))));
}

// GH-005
#[test]
fn rerun_failed_leaves_the_cancelled_jobs_and_rerun_job_takes_the_job() {
    let h = connected();
    h.github().rerun_failed(key(1));
    h.until(is_command);
    h.until(is_command);
    h.github().rerun_job(key(1), 77);
    assert_eq!(
        h.next(),
        command(
            1,
            PrAction::RerunJob { job_id: 77 },
            AttemptId(2),
            CommandState::Started
        )
    );
    h.next();
    let calls = h.github_api.calls();
    assert!(calls.contains(&Call::RerunFailed {
        pr: key(1),
        include_cancelled: false
    }));
    assert!(calls.contains(&Call::RerunJob {
        repo: "o/r".to_owned(),
        job_id: 77
    }));
}

// GH-005
#[test]
fn a_failed_rerun_is_reported_with_its_reason() {
    let h = connected();
    h.github_api.state().command = Err(GithubError::NothingToRerun);
    h.github().rerun_failed(key(2));
    h.next();
    assert_eq!(
        h.next(),
        command(
            2,
            PrAction::RerunFailed,
            AttemptId(1),
            CommandState::Failed(GithubError::NothingToRerun)
        )
    );
}

// GH-006
#[test]
fn approve_runs_asks_github_to_approve() {
    let h = connected();
    h.github().approve_runs(key(1));
    h.next();
    assert_eq!(
        h.next(),
        command(1, PrAction::Approve, AttemptId(1), CommandState::Done)
    );
    assert!(h.github_api.calls().contains(&Call::Approve(key(1))));
}

// GH-003
#[test]
fn a_second_command_on_the_same_pull_request_is_refused_not_queued() {
    let h = connected();
    h.github_api.state().hold_commands = true;
    h.github().merge(key(1), false);
    assert_eq!(
        h.next(),
        command(
            1,
            PrAction::Merge { force: false },
            AttemptId(1),
            CommandState::Started
        )
    );
    h.github().rerun_failed(key(1));
    // Refused, with the attempt that runs.
    assert_eq!(
        h.next(),
        command(
            1,
            PrAction::RerunFailed,
            AttemptId(1),
            CommandState::Refused
        )
    );
    // Another pull request is free.
    h.github().update_branch(key(2));
    assert_eq!(
        h.next(),
        command(
            2,
            PrAction::UpdateBranch,
            AttemptId(2),
            CommandState::Started
        )
    );

    h.github_api.gate.add_permits(2);
    let done = h.until(|e| {
        matches!(
            e,
            GithubEvent::Command {
                state: CommandState::Done,
                ..
            }
        )
    });
    let done2 = h.until(|e| {
        matches!(
            e,
            GithubEvent::Command {
                state: CommandState::Done,
                ..
            }
        )
    });
    assert_eq!(done.len() + done2.len(), 2);
    // Only the merge and the update were asked for.
    let asked = h
        .github_api
        .calls()
        .into_iter()
        .filter(|c| {
            !matches!(
                c,
                Call::Viewer | Call::Mine | Call::Repos | Call::Watched { .. }
            )
        })
        .count();
    assert_eq!(asked, 2);

    // The pull request is free again, with a new attempt.
    h.github_api.state().hold_commands = false;
    h.github().rerun_failed(key(1));
    assert_eq!(
        h.next(),
        command(
            1,
            PrAction::RerunFailed,
            AttemptId(3),
            CommandState::Started
        )
    );
}

// GH-003
#[test]
fn the_late_answer_of_an_older_attempt_is_dropped() {
    let h = connected();
    h.github_api.state().hold_commands = true;
    h.github().merge(key(1), false);
    assert_eq!(
        h.next(),
        command(
            1,
            PrAction::Merge { force: false },
            AttemptId(1),
            CommandState::Started
        )
    );
    // The connection is replaced while the merge runs, and the user merges again.
    h.github().connect_with_token(token("ghp_other"));
    h.until_connected();
    h.until_refreshed();
    h.github().merge(key(1), false);
    assert_eq!(
        h.next(),
        command(
            1,
            PrAction::Merge { force: false },
            AttemptId(2),
            CommandState::Started
        )
    );

    // The first merge ends first (the gate is fair), and says nothing; the second one does.
    h.github_api.gate.add_permits(1);
    h.github_api.gate.add_permits(1);
    let events = h.until(|e| {
        matches!(
            e,
            GithubEvent::Command {
                state: CommandState::Done,
                ..
            }
        )
    });
    assert_eq!(
        events.last(),
        Some(&command(
            1,
            PrAction::Merge { force: false },
            AttemptId(2),
            CommandState::Done
        ))
    );
    assert!(!events.iter().any(|e| matches!(
        e,
        GithubEvent::Command {
            attempt: AttemptId(1),
            ..
        }
    )));
}

// GH-003
#[test]
fn a_command_refreshes_the_lists_a_few_seconds_later_whatever_its_outcome() {
    let h = connected();
    h.github().update_branch(key(1));
    h.until(is_command);
    h.until(is_command);
    h.advance_quietly(Duration::from_secs(2));
    h.advance_to_refresh(Duration::from_secs(1));
    assert_eq!(h.github_api.count(&Call::Mine), 2);

    h.github_api.state().command = Err(GithubError::Other("nope".to_owned()));
    h.github().merge(key(2), false);
    h.until(is_command);
    h.until(is_command);
    h.advance_quietly(Duration::from_secs(2));
    h.advance_to_refresh(Duration::from_secs(1));
    assert_eq!(h.github_api.count(&Call::Mine), 3);
}

// GH-003
#[test]
fn a_merged_pull_request_leaves_the_list_with_that_refresh() {
    let h = connected();
    h.github().merge(key(2), false);
    h.until(is_command);
    h.until(is_command);
    h.github_api.state().mine = Ok(vec![pr(1, 1, CiState::Failure)]);
    let events = h.advance_to_refresh(Duration::from_secs(3));
    assert_eq!(mine_changes(&events), [ListChange::Removed { index: 0 }]);
}

// GH-003
#[test]
fn commands_while_not_connected_are_dropped() {
    let h = Harness::new();
    h.next();
    h.github().merge(key(1), false);
    h.no_more_events();
    assert!(h.github_api.calls().is_empty());
}

// GH-003
#[test]
fn a_command_that_gets_the_token_refused_ends_the_connection() {
    let h = connected();
    h.github_api.state().command = Err(GithubError::Unauthorized);
    h.github().merge(key(1), false);
    h.next();
    let events = h.until(|e| matches!(e, GithubEvent::Connection(Connection::Failed(_))));
    assert!(events.contains(&command(
        1,
        PrAction::Merge { force: false },
        AttemptId(1),
        CommandState::Failed(GithubError::Unauthorized)
    )));
}
