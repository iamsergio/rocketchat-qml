// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The core's own types for GitHub pull requests: what the adapter makes of `rc_github`'s types,
//! so that the frontends and the use cases don't depend on the protocol crate.

use std::cmp::Reverse;
use std::fmt;
use std::time::Duration;

use jiff::Timestamp;

/// Names a pull request for good: repository and number. The list rows, the commands and the
/// notify set all use it.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PrKey {
    /// `owner/name`.
    pub repo: String,
    pub number: u64,
}

impl PrKey {
    #[must_use]
    pub fn new(repo: &str, number: u64) -> Self {
        Self {
            repo: repo.to_owned(),
            number,
        }
    }
}

impl fmt::Display for PrKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}#{}", self.repo, self.number)
    }
}

/// How the checks of a pull request's head commit stand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CiState {
    Success,
    Failure,
    /// Running or expected.
    Pending,
    /// No checks on the commit.
    None,
}

/// What the reviews of a pull request add up to, when its branch asks for reviews.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReviewDecision {
    Approved,
    ChangesRequested,
    ReviewRequired,
}

/// Whether GitHub would let the pull request be merged (`mergeStateStatus`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MergeState {
    /// The head is behind the base.
    Behind,
    /// Conflicts with the base.
    Dirty,
    /// A rule stops the merge: reviews, required checks.
    Blocked,
    Clean,
    /// Mergeable, but a check that isn't required fails.
    Unstable,
    HasHooks,
    Draft,
    /// Also while GitHub is still working it out.
    Unknown,
}

/// How one job stands. A cancelled job is apart from a failed one: it is usually a sibling that
/// fail-fast stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobState {
    Passed,
    Failed,
    Cancelled,
    Running,
}

/// A job of the head commit: an Actions job, or the commit status of an external CI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Job {
    /// The Actions job id that rerunning a job takes; `None` for a commit status.
    pub id: Option<u64>,
    pub name: String,
    pub workflow: Option<String>,
    pub state: JobState,
    pub url: Option<String>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
}

/// An open pull request with its CI, as the lists hold it. Equality is what decides whether a
/// refresh changes a row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PullRequest {
    pub key: PrKey,
    pub title: String,
    pub author: String,
    pub url: String,
    pub updated_at: Timestamp,
    pub is_draft: bool,
    pub review: Option<ReviewDecision>,
    pub merge_state: MergeState,
    pub head_branch: String,
    pub base_branch: String,
    pub head_sha: String,
    pub ci: CiState,
    pub jobs: Vec<Job>,
    /// Actions runs wait for a maintainer's approval.
    pub needs_approval: bool,
}

/// The order of the lists: pull requests that are ready first, then the drafts, each group with
/// the latest change first. The key breaks ties, so that the order is the same on every refresh.
#[must_use]
pub fn display_order(a: &PullRequest, b: &PullRequest) -> std::cmp::Ordering {
    (a.is_draft, Reverse(a.updated_at), &a.key).cmp(&(b.is_draft, Reverse(b.updated_at), &b.key))
}

/// A self-hosted runner of a repository.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Runner {
    pub name: String,
    pub os: String,
    pub online: bool,
    pub busy: bool,
    pub labels: Vec<String>,
    /// What a busy runner runs, when it could be found.
    pub job: Option<RunnerJob>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunnerJob {
    pub name: String,
    pub workflow: String,
    pub branch: String,
    pub started_at: Timestamp,
    pub url: String,
}

/// The runners of one repository.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepoRunners {
    pub repo: String,
    pub runners: Vec<Runner>,
}

/// Why a call to GitHub failed. Owned and comparable, so that fakes can build it and events can
/// carry it.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum GithubError {
    #[error("GitHub refused the token")]
    Unauthorized,
    /// `reset` is when the primary limit renews, `retry_after` how long a secondary one asks to
    /// wait; GitHub sends one of them or neither.
    #[error("GitHub's rate limit is reached")]
    RateLimited {
        reset: Option<Timestamp>,
        retry_after: Option<Duration>,
    },
    #[error("can't reach GitHub")]
    Unreachable,
    #[error("not allowed: {0}")]
    Forbidden(String),
    /// The pull request is not open any more (`closed` or `merged`).
    #[error("the pull request is already {0}")]
    NotOpen(String),
    #[error("the checks of the pull request are not passing")]
    ChecksNotPassed(CiState),
    #[error("the merge failed: {0}")]
    MergeFailed(String),
    #[error("the merge is still pending")]
    MergeTimeout,
    #[error("no failed jobs to rerun")]
    NothingToRerun,
    /// Some of several requests failed; the others were done.
    #[error("{failed} of {total} requests failed")]
    Partial { failed: usize, total: usize },
    #[error("no such pull request")]
    NotFound,
    /// Anything else, with the message to show.
    #[error("{0}")]
    Other(String),
}
