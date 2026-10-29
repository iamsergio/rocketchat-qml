// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The GitHub ports over `rc_github`, and over the `gh` command line tool.

use std::ffi::OsString;
use std::io::ErrorKind;
use std::process::{Command, Stdio};

use rc_github::GithubClient;
use secrecy::SecretString;

use crate::github::{
    CiState, GhCli, GhError, GithubApi, GithubConnector, GithubError, Job, JobState, MergeState,
    PrKey, PullRequest, ReviewDecision, Runner, RunnerJob,
};

/// Opens the clients for one API address: GitHub's, or a stub's in the GUI tests.
pub struct RestGithubConnector {
    api_url: String,
}

impl RestGithubConnector {
    #[must_use]
    pub fn new(api_url: &str) -> Self {
        Self {
            api_url: api_url.to_owned(),
        }
    }
}

impl GithubConnector for RestGithubConnector {
    type Api = RestGithub;

    fn connect(&self, token: &SecretString) -> Result<RestGithub, GithubError> {
        let client = GithubClient::new(&self.api_url, token.clone()).map_err(error_from)?;
        Ok(RestGithub { client })
    }
}

#[derive(Clone)]
pub struct RestGithub {
    client: GithubClient,
}

impl GithubApi for RestGithub {
    async fn viewer(&self) -> Result<String, GithubError> {
        self.client.viewer().await.map_err(error_from)
    }

    async fn my_pull_requests(&self) -> Result<Vec<PullRequest>, GithubError> {
        let prs = self.client.my_pull_requests().await.map_err(error_from)?;
        Ok(prs.into_iter().map(pull_request_from).collect())
    }

    async fn watched_repos(&self) -> Result<Vec<String>, GithubError> {
        self.client.watched_repos().await.map_err(error_from)
    }

    async fn watched_pull_requests(
        &self,
        repos: &[String],
        since: jiff::civil::Date,
    ) -> Result<Vec<PullRequest>, GithubError> {
        let prs = self
            .client
            .watched_pull_requests(repos, since)
            .await
            .map_err(error_from)?;
        Ok(prs.into_iter().map(pull_request_from).collect())
    }

    async fn runners(&self, repo: &str) -> Result<Vec<Runner>, GithubError> {
        let runners = self.client.runners(repo).await.map_err(error_from)?;
        Ok(runners.into_iter().map(runner_from).collect())
    }

    async fn merge(&self, pr: &PrKey, force: bool) -> Result<(), GithubError> {
        let outcome = self
            .client
            .merge(&pr_ref(pr), force)
            .await
            .map_err(error_from)?;
        // The merge is done; what became of the branch is not worth failing the command for.
        tracing::debug!(%pr, branch = ?outcome.branch, "merged");
        Ok(())
    }

    async fn update_branch(&self, pr: &PrKey) -> Result<(), GithubError> {
        self.client.rebase(&pr_ref(pr)).await.map_err(error_from)
    }

    async fn rerun_failed(&self, pr: &PrKey, include_cancelled: bool) -> Result<(), GithubError> {
        self.client
            .rerun_failed(&pr_ref(pr), include_cancelled)
            .await
            .map_err(error_from)
    }

    async fn rerun_job(&self, repo: &str, job_id: u64) -> Result<(), GithubError> {
        self.client
            .rerun_job(repo, job_id)
            .await
            .map_err(error_from)
    }

    async fn approve_runs(&self, pr: &PrKey) -> Result<(), GithubError> {
        let approved = self
            .client
            .authorize(&pr_ref(pr))
            .await
            .map_err(error_from)?;
        tracing::debug!(%pr, approved, "approved runs");
        Ok(())
    }
}

fn pr_ref(pr: &PrKey) -> rc_github::PrRef {
    rc_github::PrRef {
        repo: pr.repo.clone(),
        number: pr.number,
    }
}

fn ci_from(state: rc_github::CiState) -> CiState {
    match state {
        rc_github::CiState::Success => CiState::Success,
        rc_github::CiState::Failure => CiState::Failure,
        rc_github::CiState::Pending => CiState::Pending,
        rc_github::CiState::None => CiState::None,
    }
}

fn job_from(job: rc_github::Job) -> Job {
    Job {
        id: job.id,
        name: job.name,
        workflow: job.workflow_name,
        state: match job.state {
            rc_github::JobState::Passed => JobState::Passed,
            rc_github::JobState::Failed => JobState::Failed,
            rc_github::JobState::Cancelled => JobState::Cancelled,
            rc_github::JobState::Running => JobState::Running,
        },
        url: job.url,
        started_at: job.started_at,
        completed_at: job.completed_at,
    }
}

fn pull_request_from(pr: rc_github::PullRequest) -> PullRequest {
    PullRequest {
        key: PrKey {
            repo: pr.repo,
            number: pr.number,
        },
        title: pr.title,
        author: pr.author,
        url: pr.url,
        updated_at: pr.updated_at,
        is_draft: pr.is_draft,
        review: pr.review.map(review_from),
        merge_state: merge_state_from(pr.merge_state),
        head_branch: pr.head_branch,
        base_branch: pr.base_branch,
        head_sha: pr.head_sha,
        ci: ci_from(pr.ci),
        jobs: pr.jobs.into_iter().map(job_from).collect(),
        needs_approval: pr.needs_approval,
    }
}

fn review_from(review: rc_github::ReviewDecision) -> ReviewDecision {
    match review {
        rc_github::ReviewDecision::Approved => ReviewDecision::Approved,
        rc_github::ReviewDecision::ChangesRequested => ReviewDecision::ChangesRequested,
        rc_github::ReviewDecision::ReviewRequired => ReviewDecision::ReviewRequired,
    }
}

fn merge_state_from(state: rc_github::MergeState) -> MergeState {
    match state {
        rc_github::MergeState::Behind => MergeState::Behind,
        rc_github::MergeState::Dirty => MergeState::Dirty,
        rc_github::MergeState::Blocked => MergeState::Blocked,
        rc_github::MergeState::Clean => MergeState::Clean,
        rc_github::MergeState::Unstable => MergeState::Unstable,
        rc_github::MergeState::HasHooks => MergeState::HasHooks,
        rc_github::MergeState::Draft => MergeState::Draft,
        rc_github::MergeState::Unknown => MergeState::Unknown,
    }
}

fn runner_from(runner: rc_github::Runner) -> Runner {
    Runner {
        name: runner.name,
        os: runner.os,
        online: runner.online,
        busy: runner.busy,
        labels: runner.custom_labels,
        job: runner.job.map(|job: rc_github::RunnerJob| RunnerJob {
            name: job.name,
            workflow: job.workflow_name,
            branch: job.head_branch,
            started_at: job.started_at,
            url: job.url,
        }),
    }
}

fn error_from(error: rc_github::Error) -> GithubError {
    use rc_github::Error as E;
    match error {
        E::Unauthorized => GithubError::Unauthorized,
        E::RateLimited { reset, retry_after } => GithubError::RateLimited { reset, retry_after },
        E::Unreachable(_) => GithubError::Unreachable,
        E::Forbidden(message) => GithubError::Forbidden(message),
        E::NotOpen(state) => GithubError::NotOpen(state),
        E::ChecksNotPassed(state) => GithubError::ChecksNotPassed(ci_from(state)),
        E::MergeFailed { message, .. } => GithubError::MergeFailed(message),
        E::MergeTimeout => GithubError::MergeTimeout,
        E::NothingToRerun => GithubError::NothingToRerun,
        E::Partial { failed, total, .. } => GithubError::Partial { failed, total },
        E::PullRequestNotFound(_) => GithubError::NotFound,
        other => GithubError::Other(other.to_string()),
    }
}

/// The GitHub CLI, run as a process.
pub struct GhProcess {
    program: OsString,
    /// Arguments before `auth token`: none for `gh` itself.
    leading: Vec<OsString>,
}

impl Default for GhProcess {
    fn default() -> Self {
        Self::with_program("gh", Vec::new())
    }
}

impl GhProcess {
    /// Runs another program than the `gh` found on the `PATH`, with `leading` before the
    /// arguments `auth token`: a shell with a script, in tests.
    #[must_use]
    pub fn with_program(program: impl Into<OsString>, leading: Vec<OsString>) -> Self {
        Self {
            program: program.into(),
            leading,
        }
    }
}

impl GhCli for GhProcess {
    fn token(&self) -> Result<SecretString, GhError> {
        let output = Command::new(&self.program)
            .args(&self.leading)
            .args(["auth", "token"])
            .stdin(Stdio::null())
            .output()
            .map_err(|error| match error.kind() {
                ErrorKind::NotFound => GhError::NotInstalled,
                _ => GhError::Failed(error.to_string()),
            })?;
        let text = |bytes: &[u8]| String::from_utf8_lossy(bytes).trim().to_owned();
        if output.status.success() {
            let token = text(&output.stdout);
            return if token.is_empty() {
                Err(GhError::NotLoggedIn)
            } else {
                Ok(SecretString::from(token))
            };
        }
        let message = text(&output.stderr);
        let lower = message.to_lowercase();
        if lower.contains("not logged in") || lower.contains("no oauth token") {
            Err(GhError::NotLoggedIn)
        } else {
            Err(GhError::Failed(
                message
                    .lines()
                    .next()
                    .unwrap_or("it exited with an error")
                    .to_owned(),
            ))
        }
    }
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use std::path::{Path, PathBuf};

    use secrecy::ExposeSecret;

    use super::*;

    /// A directory of our own that goes away with the test.
    struct Scripts(PathBuf);

    impl Scripts {
        fn new() -> Self {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos());
            let dir =
                std::env::temp_dir().join(format!("rc-fake-gh-{}-{nanos}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        /// A fake `gh` that runs `body`.
        fn gh(&self, name: &str, body: &str) -> GhProcess {
            let path = self.0.join(name);
            std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
            // Run by a shell rather than executed: a file that was just written can be busy for
            // an exec while another test's fork holds it open.
            GhProcess::with_program("/bin/sh", vec![path.into_os_string()])
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Scripts {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    // GH-009
    #[test]
    fn gh_gives_its_token_without_the_newline() {
        let scripts = Scripts::new();
        // It must be asked for `auth token`, and nothing else.
        let gh = scripts.gh(
            "ok",
            r#"[ "$1 $2" = "auth token" ] && echo "ghp_fromgh" || exit 2"#,
        );
        assert_eq!(gh.token().unwrap().expose_secret(), "ghp_fromgh");
    }

    // GH-009
    #[test]
    fn gh_that_is_not_logged_in_says_so() {
        let scripts = Scripts::new();
        let message = scripts.gh(
            "message",
            "echo 'no oauth token found for github.com' >&2; exit 1",
        );
        assert_eq!(message.token().unwrap_err(), GhError::NotLoggedIn);
        let empty = scripts.gh("empty", "exit 0");
        assert_eq!(empty.token().unwrap_err(), GhError::NotLoggedIn);
    }

    // GH-009
    #[test]
    fn gh_that_fails_otherwise_gives_its_message() {
        let scripts = Scripts::new();
        let gh = scripts.gh("broken", "echo 'boom' >&2; echo 'more' >&2; exit 3");
        assert_eq!(gh.token().unwrap_err(), GhError::Failed("boom".to_owned()));
    }

    // GH-009
    #[test]
    fn a_missing_gh_is_not_installed() {
        let scripts = Scripts::new();
        let gh = GhProcess::with_program(scripts.path().join("absent"), Vec::new());
        assert_eq!(gh.token().unwrap_err(), GhError::NotInstalled);
    }
}
