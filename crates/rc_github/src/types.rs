// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The types the crate hands out. Plain data; the JSON is in `wire`.

use jiff::Timestamp;

use crate::error::Error;

/// How the checks of a commit stand, from the state of its check rollup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CiState {
    Success,
    Failure,
    /// Running or expected.
    Pending,
    /// No checks (no rollup) on the commit.
    None,
}

impl CiState {
    pub(crate) fn from_rollup(state: Option<&str>) -> Self {
        match state {
            Some("SUCCESS") => Self::Success,
            Some("FAILURE" | "ERROR") => Self::Failure,
            Some("PENDING" | "EXPECTED") => Self::Pending,
            _ => Self::None,
        }
    }
}

/// How a job stands. A cancelled job is apart from a failed one: it is usually a sibling that
/// fail-fast stopped after another failed, so it is noise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobState {
    /// Success, neutral, skipped or stale.
    Passed,
    Failed,
    Cancelled,
    /// Queued, waiting, pending or in progress.
    Running,
}

/// A job of a pull request's head commit: a check run (an Actions job) or a commit status of
/// an external CI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    /// The check run's id, which for Actions is the job id [`GithubClient::rerun_job`] takes.
    /// `None` for a commit status.
    ///
    /// [`GithubClient::rerun_job`]: crate::GithubClient::rerun_job
    pub id: Option<u64>,
    pub name: String,
    /// The workflow of an Actions job.
    pub workflow_name: Option<String>,
    pub state: JobState,
    /// GitHub's own word for how a completed job ended (`FAILURE`, `TIMED_OUT`, `SKIPPED`...);
    /// `None` while it runs, and for a commit status.
    pub conclusion: Option<String>,
    /// The job's page.
    pub url: Option<String>,
    pub started_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
}

/// An open pull request of the search, with its CI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequest {
    /// `owner/name`.
    pub repo: String,
    pub number: u64,
    pub title: String,
    /// The author's login; `ghost` for a deleted account.
    pub author: String,
    pub url: String,
    pub updated_at: Timestamp,
    pub is_draft: bool,
    pub head_branch: String,
    pub base_branch: String,
    pub head_sha: String,
    pub ci: CiState,
    /// All the jobs of the head commit, in GitHub's order (the first hundred).
    pub jobs: Vec<Job>,
    /// Actions runs of the head commit wait for a maintainer's approval (a first-time
    /// contributor's pull request): [`GithubClient::authorize`] approves them.
    ///
    /// [`GithubClient::authorize`]: crate::GithubClient::authorize
    pub needs_approval: bool,
}

impl PullRequest {
    #[must_use]
    pub fn pr_ref(&self) -> PrRef {
        PrRef {
            repo: self.repo.clone(),
            number: self.number,
        }
    }
}

/// Names a pull request without its host: the client talks to one API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrRef {
    /// `owner/name`.
    pub repo: String,
    pub number: u64,
}

impl PrRef {
    /// Reads `https://<host>/<owner>/<repo>/pull/<number>`, with a tab such as `/checks`, a query
    /// or a fragment after it ignored. The host is not checked.
    ///
    /// # Errors
    ///
    /// [`Error::NotAPullRequestUrl`] for any other URL.
    pub fn from_url(url: &str) -> Result<Self, Error> {
        let err = || Error::NotAPullRequestUrl(url.to_owned());
        let bare = url.split(['?', '#']).next().unwrap_or_default();
        let path = bare
            .strip_prefix("https://")
            .or_else(|| bare.strip_prefix("http://"))
            .ok_or_else(err)?;
        let segments: Vec<&str> = path.trim_end_matches('/').split('/').collect();
        let [_host, owner, repo, "pull", number, ..] = segments.as_slice() else {
            return Err(err());
        };
        if owner.is_empty() || repo.is_empty() {
            return Err(err());
        }
        if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
            return Err(err());
        }
        Ok(Self {
            repo: format!("{owner}/{repo}"),
            number: number.parse().map_err(|_| err())?,
        })
    }

    pub(crate) fn owner_and_name(&self) -> Result<(&str, &str), Error> {
        self.repo
            .split_once('/')
            .filter(|(owner, name)| !owner.is_empty() && !name.is_empty() && !name.contains('/'))
            .ok_or_else(|| Error::NotAPullRequestUrl(self.repo.clone()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrState {
    Open,
    Closed,
    Merged,
}

/// What a merge, or a wait for CI, looks at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequestStatus {
    pub repo: String,
    pub number: u64,
    pub title: String,
    pub state: PrState,
    pub ci: CiState,
    pub head_branch: String,
    pub head_sha: String,
    /// `None` if the head repository (a fork) was deleted.
    pub head_repo: Option<String>,
}

/// What became of the head branch after a merge. GitHub is not asked to delete it with the merge:
/// the client does, so that it can say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BranchDeletion {
    Deleted,
    /// It was gone already, e.g. the repository deletes head branches by itself.
    AlreadyGone,
    /// No head repository to delete from.
    Skipped,
    /// The merge is done; only the branch is left. Carries GitHub's message.
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeOutcome {
    pub branch: BranchDeletion,
}

/// A self-hosted runner of a repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Runner {
    pub name: String,
    pub os: String,
    pub online: bool,
    pub busy: bool,
    /// The labels set by hand; the read-only ones repeat `self-hosted` and the OS and architecture.
    pub custom_labels: Vec<String>,
    /// What a busy runner is running. The runners API doesn't say, so it is found among
    /// [`GithubClient::running_jobs`].
    ///
    /// [`GithubClient::running_jobs`]: crate::GithubClient::running_jobs
    pub job: Option<RunnerJob>,
}

/// An in-progress Actions job, and the runner that has it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerJob {
    pub name: String,
    pub workflow_name: String,
    pub head_branch: String,
    pub started_at: Timestamp,
    pub url: String,
    pub runner_name: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // GH-003
    #[test]
    fn pr_ref_from_url() {
        let expected = PrRef {
            repo: "iamsergio/rocketchat-qml".to_owned(),
            number: 42,
        };
        for url in [
            "https://github.com/iamsergio/rocketchat-qml/pull/42",
            "https://github.com/iamsergio/rocketchat-qml/pull/42/",
            "https://github.com/iamsergio/rocketchat-qml/pull/42/checks?x=1#top",
            "http://127.0.0.1:8080/iamsergio/rocketchat-qml/pull/42",
        ] {
            assert_eq!(PrRef::from_url(url).unwrap(), expected, "{url}");
        }
    }

    // GH-003
    #[test]
    fn pr_ref_rejects_other_urls() {
        for url in [
            "github.com/o/r/pull/1",
            "https://github.com/o/r/issues/1",
            "https://github.com/o/r/pull/x",
            "https://github.com/o/r/pull/",
            "https://github.com/o/r/actions/runs/1",
            "https://github.com//r/pull/1",
        ] {
            assert!(
                matches!(PrRef::from_url(url), Err(Error::NotAPullRequestUrl(_))),
                "{url}"
            );
        }
    }

    // GH-003
    #[test]
    fn rollup_states() {
        assert_eq!(CiState::from_rollup(Some("SUCCESS")), CiState::Success);
        assert_eq!(CiState::from_rollup(Some("ERROR")), CiState::Failure);
        assert_eq!(CiState::from_rollup(Some("EXPECTED")), CiState::Pending);
        assert_eq!(CiState::from_rollup(None), CiState::None);
    }
}
