// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! What a row of the list shows for a pull request, and the pieces the page shares with it: how
//! the CI stands and the segments of the check bar.

use jiff::Timestamp;
use rc_core::github::{CiState, Job, JobState, PrKey, PullRequest};

use super::text;

/// How the checks of a pull request stand, as the user reads it. A failure comes first, then runs
/// waiting for a maintainer, which hide that the rest still runs or passed: the pull request
/// can't be merged until they are approved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CiKind {
    Failed,
    WaitingApproval,
    Running,
    Passed,
    NoChecks,
}

#[must_use]
pub fn ci_kind(pr: &PullRequest) -> CiKind {
    if pr.ci == CiState::Failure {
        CiKind::Failed
    } else if pr.needs_approval {
        CiKind::WaitingApproval
    } else {
        match pr.ci {
            CiState::Pending => CiKind::Running,
            CiState::Success => CiKind::Passed,
            CiState::None | CiState::Failure => CiKind::NoChecks,
        }
    }
}

/// The state glyph of a row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Glyph {
    Draft,
    Failed,
    WaitingApproval,
    Running,
    Passed,
    NoChecks,
}

#[must_use]
pub fn glyph(pr: &PullRequest) -> Glyph {
    if pr.is_draft {
        return Glyph::Draft;
    }
    match ci_kind(pr) {
        CiKind::Failed => Glyph::Failed,
        CiKind::WaitingApproval => Glyph::WaitingApproval,
        CiKind::Running => Glyph::Running,
        CiKind::Passed => Glyph::Passed,
        CiKind::NoChecks => Glyph::NoChecks,
    }
}

/// One segment of a check bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Segment {
    Failed,
    Running,
    Cancelled,
    Passed,
    /// Runs wait for approval and have no jobs yet.
    Waiting,
}

/// Where a job state sorts: failed, running, cancelled, passed.
fn rank(state: JobState) -> u8 {
    match state {
        JobState::Failed => 0,
        JobState::Running => 1,
        JobState::Cancelled => 2,
        JobState::Passed => 3,
    }
}

fn segment(state: JobState) -> Segment {
    match state {
        JobState::Failed => Segment::Failed,
        JobState::Running => Segment::Running,
        JobState::Cancelled => Segment::Cancelled,
        JobState::Passed => Segment::Passed,
    }
}

/// The jobs in the order of the check bar and of the page's job rows: failed, running, cancelled,
/// passed, each group in GitHub's order. Segment `i` of the bar is job `i` of this list.
#[must_use]
pub fn ordered_jobs(pr: &PullRequest) -> Vec<&Job> {
    let mut jobs: Vec<&Job> = pr.jobs.iter().collect();
    jobs.sort_by_key(|job| rank(job.state));
    jobs
}

/// One segment per job in [`ordered_jobs`] order; a single waiting one for a pull request whose
/// runs wait for approval and that has no jobs; nothing for one without checks.
#[must_use]
pub fn check_bar(pr: &PullRequest) -> Vec<Segment> {
    if pr.jobs.is_empty() {
        return if pr.needs_approval {
            vec![Segment::Waiting]
        } else {
            Vec::new()
        };
    }
    ordered_jobs(pr)
        .into_iter()
        .map(|job| segment(job.state))
        .collect()
}

/// A row of the Mine or Watched list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrRowView {
    pub key: PrKey,
    pub glyph: Glyph,
    pub repo: String,
    /// The author, on rows of the Watched list; empty on Mine.
    pub author: String,
    /// "11 min", "1 h", "5 d", "3 w", "8 mo": see [`text::age`].
    pub age: String,
    /// The user asked to be told when its CI ends.
    pub bell: bool,
    pub title: String,
    pub number: u64,
    /// "#41".
    pub number_text: String,
    pub bar: Vec<Segment>,
    /// The CI state changed since the user last opened the pull request (or since the app first
    /// saw it): the row is bold.
    pub changed: bool,
}

pub(super) fn row_view(
    pr: &PullRequest,
    show_author: bool,
    bell: bool,
    changed: bool,
    now: Timestamp,
) -> PrRowView {
    PrRowView {
        key: pr.key.clone(),
        glyph: glyph(pr),
        repo: pr.key.repo.clone(),
        author: if show_author {
            pr.author.clone()
        } else {
            String::new()
        },
        age: text::age(now, pr.updated_at),
        bell,
        title: pr.title.clone(),
        number: pr.key.number,
        number_text: format!("#{}", pr.key.number),
        bar: check_bar(pr),
        changed,
    }
}
