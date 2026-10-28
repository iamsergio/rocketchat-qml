// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

use std::time::Duration;

use jiff::Timestamp;
use jiff::civil::Date;
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::{Method, StatusCode, Url};
use secrecy::{ExposeSecret, SecretString};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio::time::Instant;

use crate::error::Error;
use crate::types::{
    BranchDeletion, CiState, MergeOutcome, PrRef, PrState, PullRequest, PullRequestStatus, Runner,
    RunnerJob,
};
use crate::wire;

/// GitHub's own API.
pub const DEFAULT_API_URL: &str = "https://api.github.com";

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_REDIRECTS: usize = 10;
/// GitHub refuses requests without a `User-Agent`.
const USER_AGENT: &str = "rocketchat-qml";
const API_VERSION: &str = "2022-11-28";
/// The most a REST list returns per page.
const PER_PAGE: usize = 100;
/// A stop for a list that never ends, e.g. an API that ignores `page`.
const MAX_PAGES: usize = 50;
/// How often `merge` polls a merge running in the background, and how long it waits for it.
const MERGE_POLL_INTERVAL: Duration = Duration::from_secs(2);
const MERGE_TIMEOUT: Duration = Duration::from_mins(5);

/// Talks to one GitHub API with one token. Cheap to clone.
#[derive(Clone)]
pub struct GithubClient {
    http: reqwest::Client,
    base: Url,
    token: SecretString,
    merge_poll_interval: Duration,
    merge_timeout: Duration,
}

impl GithubClient {
    /// `api_url` is the API's base URL: [`DEFAULT_API_URL`], or another address such as a local
    /// stub's, possibly with a path prefix (GitHub Enterprise's `https://host/api/v3`; its GraphQL
    /// lives apart, at `/api/graphql`, which this does not follow: the GraphQL calls go to
    /// `<api_url>/graphql`). The scheme is required. The token is sent as `Authorization: Bearer`.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidUrl`] if `api_url` isn't an `http` or `https` URL with a host.
    pub fn new(api_url: &str, token: SecretString) -> Result<Self, Error> {
        let base = base_url(api_url)?;
        let mut headers = HeaderMap::new();
        headers.insert(
            reqwest::header::ACCEPT,
            HeaderValue::from_static("application/vnd.github+json"),
        );
        headers.insert(
            "x-github-api-version",
            HeaderValue::from_static(API_VERSION),
        );
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .default_headers(headers)
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            // reqwest drops the Authorization header on a redirect to another host.
            .redirect(reqwest::redirect::Policy::limited(MAX_REDIRECTS))
            .build()
            .map_err(Error::Unreachable)?;
        Ok(Self {
            http,
            base,
            token,
            merge_poll_interval: MERGE_POLL_INTERVAL,
            merge_timeout: MERGE_TIMEOUT,
        })
    }

    /// A client for GitHub itself.
    ///
    /// # Errors
    ///
    /// Only if the HTTP client can't be set up: [`Error::Unreachable`].
    pub fn github(token: SecretString) -> Result<Self, Error> {
        Self::new(DEFAULT_API_URL, token)
    }

    /// How often [`merge`](Self::merge) asks whether a merge is done, and how long it waits for
    /// that. 2 seconds and 5 minutes by default.
    #[must_use]
    pub fn with_merge_polling(mut self, interval: Duration, timeout: Duration) -> Self {
        self.merge_poll_interval = interval;
        self.merge_timeout = timeout;
        self
    }

    /// The base URL, normalized: it always ends with `/`.
    #[must_use]
    pub fn api_url(&self) -> &Url {
        &self.base
    }

    // --- lists ---

    /// The user's open pull requests, drafts included, most recently updated first (GraphQL
    /// `search` for `is:pr is:open author:@me`, all pages). Each carries the jobs of its head
    /// commit. Needs a token of a user: `@me` is nobody for an Actions token.
    ///
    /// # Errors
    ///
    /// [`Error::Unauthorized`] if the token is refused, [`Error::RateLimited`], or any of the other
    /// errors.
    pub async fn my_pull_requests(&self) -> Result<Vec<PullRequest>, Error> {
        self.search_pull_requests("is:pr is:open author:@me", usize::MAX)
            .await
    }

    /// Other authors' open pull requests in `repos` (`owner/name`) created on or after `since`,
    /// most recently updated first. [`watched_repos`](Self::watched_repos) lists the repositories
    /// of the user's subscriptions. With no repository the answer is empty without a request: a
    /// search without a `repo:` qualifier would cover all of GitHub.
    ///
    /// Search takes the repositories as qualifiers of its query, and the query has a length limit
    /// (256 characters at the time of writing), which a long list can pass: GitHub answers with
    /// an [`Error::GraphQl`] then.
    ///
    /// # Errors
    ///
    /// As [`my_pull_requests`](Self::my_pull_requests).
    pub async fn watched_pull_requests(
        &self,
        repos: &[String],
        since: Date,
    ) -> Result<Vec<PullRequest>, Error> {
        if repos.is_empty() {
            return Ok(Vec::new());
        }
        let repos = repos
            .iter()
            .fold(String::new(), |all, repo| all + " repo:" + repo);
        let search = format!("is:pr is:open -author:@me created:>={since}{repos}");
        self.search_pull_requests(&search, usize::MAX).await
    }

    /// The full names (`owner/name`) of the repositories the user watches (`GET
    /// /user/subscriptions`, all pages). REST rather than GraphQL's `viewer.watching`, which is
    /// twice as slow.
    ///
    /// # Errors
    ///
    /// As [`my_pull_requests`](Self::my_pull_requests).
    pub async fn watched_repos(&self) -> Result<Vec<String>, Error> {
        let repos: Vec<wire::SubscribedRepo> = self
            .pages(&["user", "subscriptions"], std::convert::identity)
            .await?;
        Ok(repos.into_iter().map(|repo| repo.full_name).collect())
    }

    /// Pull requests matching a GitHub search query (GraphQL `search`), most recently updated
    /// first, at most `limit` of them. The other searches of this client are this one with their
    /// query.
    ///
    /// # Errors
    ///
    /// As [`my_pull_requests`](Self::my_pull_requests).
    pub async fn search_pull_requests(
        &self,
        search: &str,
        limit: usize,
    ) -> Result<Vec<PullRequest>, Error> {
        let page_size = limit.clamp(1, PER_PAGE);
        let mut prs: Vec<PullRequest> = Vec::new();
        let mut after: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let variables = json!({ "q": search, "first": page_size, "after": after });
            let data: wire::SearchData = self.graphql(wire::SEARCH_QUERY, variables).await?;
            let page = data.search;
            prs.extend(page.nodes.into_iter().map(PullRequest::from));
            after = page.page_info.end_cursor;
            if !page.page_info.has_next_page || after.is_none() || prs.len() >= limit {
                break;
            }
        }
        prs.truncate(limit);
        // As the list of ghw: GitHub's relevance order isn't what a person looks for.
        prs.sort_by_key(|pr| std::cmp::Reverse(pr.updated_at));
        Ok(prs)
    }

    // --- one pull request ---

    /// Where a pull request stands: open or not, and its checks (GraphQL). What to poll to wait
    /// for the checks to end.
    ///
    /// # Errors
    ///
    /// [`Error::PullRequestNotFound`] if there is no such pull request or the token can't see it,
    /// or any of the other errors.
    pub async fn pull_request_status(&self, pr: &PrRef) -> Result<PullRequestStatus, Error> {
        let node: wire::StatusNode = self.pull_request_query(wire::STATUS_QUERY, pr).await?;
        Ok(node.into_status())
    }

    async fn pull_request_query<T: DeserializeOwned>(
        &self,
        query: &str,
        pr: &PrRef,
    ) -> Result<T, Error> {
        let (owner, name) = pr.owner_and_name()?;
        let data: wire::RepositoryData<T> = self
            .graphql(
                query,
                json!({ "owner": owner, "name": name, "number": pr.number }),
            )
            .await?;
        data.into_pull_request()
            .ok_or_else(|| Error::PullRequestNotFound(format!("{}#{}", pr.repo, pr.number)))
    }

    // --- rerun ---

    /// Reruns the failed jobs of every Actions run on the pull request's head commit, with the
    /// jobs that depend on them (`POST .../actions/runs/{id}/rerun-failed-jobs`).
    ///
    /// Cancelled jobs are usually siblings that fail-fast stopped after one failed. With
    /// `include_cancelled` they are rerun too (as `gh run rerun --failed` does); without, a run
    /// that has any is rerun job by job (`POST .../actions/jobs/{id}/rerun`) for its failed ones
    /// only. Keeps going when one request fails, so that a run that can't be rerun (still in
    /// progress) doesn't stop the others.
    ///
    /// # Errors
    ///
    /// [`Error::NothingToRerun`] if no job failed, [`Error::Partial`] if some requests failed, or
    /// any of the other errors.
    pub async fn rerun_failed(&self, pr: &PrRef, include_cancelled: bool) -> Result<(), Error> {
        let commits: wire::CommitsNode = self.pull_request_query(wire::RUNS_QUERY, pr).await?;
        let mut requests: Vec<(&str, u64)> = Vec::new();
        for run in commits.failed_runs() {
            if include_cancelled || !run.has_cancelled {
                requests.push(("runs", run.id));
            } else {
                requests.extend(run.failed_jobs.into_iter().map(|job| ("jobs", job)));
            }
        }
        if requests.is_empty() {
            return Err(Error::NothingToRerun);
        }
        let mut failures = Vec::new();
        let total = requests.len();
        for (kind, id) in requests {
            let id = id.to_string();
            let result = if kind == "runs" {
                self.post(&self.repo_url(&pr.repo, &["actions", "runs", &id, "rerun-failed-jobs"])?)
                    .await
            } else {
                self.post(&self.repo_url(&pr.repo, &["actions", "jobs", &id, "rerun"])?)
                    .await
            };
            if let Err(e) = result {
                tracing::warn!(error = %e, kind, id, "rerun failed");
                failures.push(e);
            }
        }
        partial(failures, total)
    }

    /// Reruns one job and the jobs that depend on it (`POST .../actions/jobs/{id}/rerun`).
    /// `job_id` is [`Job::id`](crate::Job::id).
    ///
    /// # Errors
    ///
    /// Any of the errors; GitHub refuses with a 403 when the job is still running.
    pub async fn rerun_job(&self, repo: &str, job_id: u64) -> Result<(), Error> {
        let id = job_id.to_string();
        self.post(&self.repo_url(repo, &["actions", "jobs", &id, "rerun"])?)
            .await
    }

    // --- approve ---

    /// Approves the Actions runs of the pull request's head commit that wait for a maintainer's
    /// approval (the "Approve and run" button; `POST .../actions/runs/{id}/approve`). Keeps going
    /// past runs that can't be approved. Returns how many it approved; 0 without a request when
    /// none waits.
    ///
    /// # Errors
    ///
    /// [`Error::Partial`] if some approvals failed, or any of the other errors.
    pub async fn authorize(&self, pr: &PrRef) -> Result<usize, Error> {
        let commits: wire::CommitsNode = self.pull_request_query(wire::APPROVAL_QUERY, pr).await?;
        let runs = commits.approval_runs();
        let mut failures = Vec::new();
        for run in &runs {
            let id = run.to_string();
            let result = self
                .post(&self.repo_url(&pr.repo, &["actions", "runs", &id, "approve"])?)
                .await;
            if let Err(e) = result {
                tracing::warn!(error = %e, run, "approval failed");
                failures.push(e);
            }
        }
        let approved = runs.len() - failures.len();
        partial(failures, runs.len()).map(|()| approved)
    }

    // --- update the branch ---

    /// Rebases the pull request's branch onto the latest base branch on the server (the GraphQL
    /// mutation `updatePullRequestBranch` with `updateMethod: REBASE`; REST's update-branch can
    /// only merge). No local checkout. GitHub does it in the background: the new head shows in
    /// the next [`my_pull_requests`](Self::my_pull_requests).
    ///
    /// # Errors
    ///
    /// [`Error::GraphQl`] if GitHub refuses (a conflict, the branch is up to date), or any of
    /// the other errors.
    pub async fn rebase(&self, pr: &PrRef) -> Result<(), Error> {
        let node: wire::IdNode = self.pull_request_query(wire::ID_QUERY, pr).await?;
        let _: Value = self
            .graphql(wire::REBASE_MUTATION, json!({ "id": node.id }))
            .await?;
        Ok(())
    }

    // --- merge ---

    /// Merges the pull request with the rebase method, as `ghw merge` does. The pull request must
    /// be open, and without `force` its checks must have passed; waiting for them is the caller's
    /// ([`pull_request_status`](Self::pull_request_status)).
    ///
    /// The merge is the asynchronous REST endpoint (`PUT .../pulls/{n}/merge-async`, with
    /// `merge_action=direct_merge`), because GraphQL's `mergePullRequest` refuses pull requests in
    /// a stack. It passes the head commit the checks were read at as `sha`, so that the merge is
    /// refused if someone pushed in between. It polls the merge until it is done, then deletes
    /// the head branch (GitHub isn't asked to).
    ///
    /// # Errors
    ///
    /// [`Error::NotOpen`], [`Error::ChecksNotPassed`], [`Error::MergeFailed`],
    /// [`Error::MergeTimeout`], or any of the other errors. A branch that can't be deleted is no
    /// error: it is in the [`MergeOutcome`].
    pub async fn merge(&self, pr: &PrRef, force: bool) -> Result<MergeOutcome, Error> {
        let status = self.pull_request_status(pr).await?;
        match status.state {
            PrState::Open => {}
            PrState::Closed => return Err(Error::NotOpen("closed".to_owned())),
            PrState::Merged => return Err(Error::NotOpen("merged".to_owned())),
        }
        if !force && status.ci != CiState::Success {
            return Err(Error::ChecksNotPassed(status.ci));
        }
        let number = pr.number.to_string();
        let endpoint = self.repo_url(&pr.repo, &["pulls", &number, "merge-async"])?;
        let body = json!({
            "merge_action": "direct_merge",
            "merge_method": "rebase",
            "sha": status.head_sha,
        });
        let mut merge: wire::AsyncMerge = self
            .json(Method::PUT, endpoint.clone(), Some(&body))
            .await?;
        let start = Instant::now();
        loop {
            match (merge.status.as_str(), merge.details.uuid.as_deref()) {
                ("merged", _) => break,
                ("pending", Some(uuid)) if start.elapsed() < self.merge_timeout => {
                    tokio::time::sleep(self.merge_poll_interval).await;
                    let mut poll = endpoint.clone();
                    poll.path_segments_mut()
                        .map_err(|()| Error::InvalidUrl(endpoint.to_string()))?
                        .push(uuid);
                    match self.json(Method::GET, poll, None).await {
                        Ok(new) => merge = new,
                        // A hiccup of the network or of GitHub doesn't undo the merge.
                        Err(e @ (Error::Unreachable(_) | Error::Api { status: 500.., .. })) => {
                            tracing::warn!(error = %e, "polling the merge failed, retrying");
                        }
                        Err(e) => return Err(e),
                    }
                }
                ("pending", _) => return Err(Error::MergeTimeout),
                (status, _) => {
                    return Err(Error::MergeFailed {
                        status: status.to_owned(),
                        message: merge
                            .details
                            .message
                            .unwrap_or_else(|| "no details".to_owned()),
                    });
                }
            }
        }
        let branch = match &status.head_repo {
            Some(repo) => self.delete_branch(repo, &status.head_branch).await,
            None => BranchDeletion::Skipped,
        };
        Ok(MergeOutcome { branch })
    }

    /// Deletes a branch (`DELETE .../git/refs/heads/{branch}`). It reports instead of failing:
    /// the merge it follows is done.
    async fn delete_branch(&self, repo: &str, branch: &str) -> BranchDeletion {
        let mut segments = vec!["git", "refs", "heads"];
        segments.extend(branch.split('/'));
        let url = match self.repo_url(repo, &segments) {
            Ok(url) => url,
            Err(e) => return BranchDeletion::Failed(e.to_string()),
        };
        match self.send(Method::DELETE, url, None).await {
            Ok((status, _, _)) if status.is_success() => BranchDeletion::Deleted,
            // The repository's "automatically delete head branches" may have beaten us to it.
            Ok((status, _, body))
                if status == StatusCode::UNPROCESSABLE_ENTITY
                    && message_of(&body).contains("Reference does not exist") =>
            {
                BranchDeletion::AlreadyGone
            }
            Ok((status, headers, body)) => {
                BranchDeletion::Failed(http_error(status, &headers, &body).to_string())
            }
            Err(e) => BranchDeletion::Failed(e.to_string()),
        }
    }

    // --- runners ---

    /// The self-hosted runners of a repository (`owner/name`), each busy one with the job it
    /// runs. Needs admin rights on the repository: [`Error::Forbidden`] or [`Error::Api`] with 404
    /// otherwise.
    ///
    /// # Errors
    ///
    /// Any of the errors.
    pub async fn runners(&self, repo: &str) -> Result<Vec<Runner>, Error> {
        let mut runners: Vec<Runner> = self
            .pages(
                &["repos", repo, "actions", "runners"],
                |body: wire::RunnersBody| body.runners,
            )
            .await?
            .into_iter()
            .map(Runner::from)
            .collect();
        if runners.iter().any(|runner| runner.busy) {
            let jobs = self.running_jobs(repo).await?;
            for runner in &mut runners {
                runner.job = jobs
                    .iter()
                    .find(|job| job.runner_name.as_deref() == Some(runner.name.as_str()))
                    .cloned();
            }
        }
        Ok(runners)
    }

    /// The in-progress jobs of the repository's in-progress runs. The runners API doesn't say what
    /// a busy runner runs; only the jobs name their runner.
    ///
    /// # Errors
    ///
    /// Any of the errors.
    pub async fn running_jobs(&self, repo: &str) -> Result<Vec<RunnerJob>, Error> {
        let runs: Vec<wire::RunBody> = self
            .pages_with(
                &["repos", repo, "actions", "runs"],
                &[("status", "in_progress")],
                |body: wire::RunsBody| body.workflow_runs,
            )
            .await?;
        let mut jobs = Vec::new();
        for run in runs {
            let id = run.id.to_string();
            let run_jobs: Vec<wire::JobBody> = self
                .pages_with(
                    &["repos", repo, "actions", "runs", &id, "jobs"],
                    &[("filter", "latest")],
                    |body: wire::JobsBody| body.jobs,
                )
                .await?;
            jobs.extend(
                run_jobs
                    .into_iter()
                    .filter(|job| job.status == "in_progress")
                    .map(RunnerJob::from),
            );
        }
        Ok(jobs)
    }

    // --- transport ---

    /// `repos/{owner}/{name}/{rest...}`.
    fn repo_url(&self, repo: &str, rest: &[&str]) -> Result<Url, Error> {
        let (owner, name) = repo
            .split_once('/')
            .filter(|(owner, name)| !owner.is_empty() && !name.is_empty() && !name.contains('/'))
            .ok_or_else(|| Error::InvalidUrl(format!("not a repository: {repo}")))?;
        let mut segments = vec!["repos", owner, name];
        segments.extend(rest);
        self.url(&segments, &[])
    }

    /// The base URL with these path segments (percent-encoded) and query pairs.
    fn url(&self, segments: &[&str], query: &[(&str, &str)]) -> Result<Url, Error> {
        let mut url = self.base.clone();
        url.path_segments_mut()
            .map_err(|()| Error::InvalidUrl(self.base.to_string()))?
            .pop_if_empty()
            .extend(segments.iter().flat_map(|s| s.split('/')));
        if !query.is_empty() {
            url.query_pairs_mut().extend_pairs(query);
        }
        Ok(url)
    }

    /// Every page of a REST list. `items` takes the list out of the page's body. A page with
    /// fewer than 100 items is the last.
    async fn pages<B, T>(
        &self,
        segments: &[&str],
        items: impl Fn(B) -> Vec<T>,
    ) -> Result<Vec<T>, Error>
    where
        B: DeserializeOwned,
    {
        self.pages_with(segments, &[], items).await
    }

    async fn pages_with<B, T>(
        &self,
        segments: &[&str],
        query: &[(&str, &str)],
        items: impl Fn(B) -> Vec<T>,
    ) -> Result<Vec<T>, Error>
    where
        B: DeserializeOwned,
    {
        let per_page = PER_PAGE.to_string();
        let mut all = Vec::new();
        for page in 1..=MAX_PAGES {
            let page = page.to_string();
            let mut pairs = query.to_vec();
            pairs.extend([("per_page", per_page.as_str()), ("page", page.as_str())]);
            let url = self.url(segments, &pairs)?;
            let body: B = self.json(Method::GET, url, None).await?;
            let batch = items(body);
            let last = batch.len() < PER_PAGE;
            all.extend(batch);
            if last {
                break;
            }
        }
        Ok(all)
    }

    async fn post(&self, url: &Url) -> Result<(), Error> {
        let (status, headers, body) = self.send(Method::POST, url.clone(), None).await?;
        check(status, &headers, &body)
    }

    /// A request whose successful answer is JSON.
    async fn json<T: DeserializeOwned>(
        &self,
        method: Method,
        url: Url,
        payload: Option<&Value>,
    ) -> Result<T, Error> {
        let (status, headers, body) = self.send(method, url, payload).await?;
        check(status, &headers, &body)?;
        serde_json::from_slice(&body).map_err(Error::Decode)
    }

    /// Runs a GraphQL query (`POST /graphql`). GraphQL answers 200 even when it refuses, with an
    /// `errors` array.
    async fn graphql<T: DeserializeOwned>(
        &self,
        query: &str,
        variables: Value,
    ) -> Result<T, Error> {
        let url = self.url(&["graphql"], &[])?;
        let payload = json!({ "query": query, "variables": variables });
        let response: wire::GraphQlResponse<T> =
            self.json(Method::POST, url, Some(&payload)).await?;
        if !response.errors.is_empty() {
            if response
                .errors
                .iter()
                .any(|e| e.kind.as_deref() == Some("RATE_LIMITED"))
            {
                return Err(Error::RateLimited {
                    reset: None,
                    retry_after: None,
                });
            }
            let messages: Vec<&str> = response.errors.iter().map(|e| e.message.as_str()).collect();
            return Err(Error::GraphQl(messages.join("; ")));
        }
        response
            .data
            .ok_or_else(|| Error::GraphQl("no data in the answer".to_owned()))
    }

    /// Sends one request and returns the status, headers and whole body, whatever the status is.
    async fn send(
        &self,
        method: Method,
        url: Url,
        payload: Option<&Value>,
    ) -> Result<(StatusCode, HeaderMap, Vec<u8>), Error> {
        let path = url.path().to_owned();
        let mut authorization =
            HeaderValue::from_str(&format!("Bearer {}", self.token.expose_secret()))
                .map_err(|_| Error::Unauthorized)?;
        authorization.set_sensitive(true);
        let mut request = self
            .http
            .request(method.clone(), url)
            .header(reqwest::header::AUTHORIZATION, authorization);
        if let Some(payload) = payload {
            request = request.json(payload);
        }
        let response = request.send().await.map_err(Error::Unreachable)?;
        let status = response.status();
        tracing::debug!(%method, path, status = status.as_u16(), "GitHub request");
        let headers = response.headers().clone();
        let body = response.bytes().await.map_err(Error::Unreachable)?;
        Ok((status, headers, body.to_vec()))
    }
}

/// `Ok` if nothing failed; the error itself if the only request failed; else [`Error::Partial`].
fn partial(mut failures: Vec<Error>, total: usize) -> Result<(), Error> {
    match failures.len() {
        0 => Ok(()),
        1 if total == 1 => Err(failures.remove(0)),
        failed => Err(Error::Partial {
            failed,
            total,
            first: Box::new(failures.remove(0)),
        }),
    }
}

pub(crate) fn base_url(api_url: &str) -> Result<Url, Error> {
    let mut base = Url::parse(api_url).map_err(|e| Error::InvalidUrl(format!("{api_url}: {e}")))?;
    if !matches!(base.scheme(), "http" | "https") || base.host_str().is_none() {
        return Err(Error::InvalidUrl(api_url.to_owned()));
    }
    base.set_query(None);
    base.set_fragment(None);
    if !base.path().ends_with('/') {
        let path = format!("{}/", base.path());
        base.set_path(&path);
    }
    Ok(base)
}

fn check(status: StatusCode, headers: &HeaderMap, body: &[u8]) -> Result<(), Error> {
    if status.is_success() {
        Ok(())
    } else {
        Err(http_error(status, headers, body))
    }
}

fn message_of(body: &[u8]) -> String {
    serde_json::from_slice::<wire::ErrorBody>(body)
        .ok()
        .and_then(|body| body.message)
        .unwrap_or_default()
}

/// Maps a non-2xx answer to an error. A 403 is a rate limit when the headers say the limit is
/// reached (`x-ratelimit-remaining: 0`, or `retry-after` for the secondary limit); a 429 always is.
fn http_error(status: StatusCode, headers: &HeaderMap, body: &[u8]) -> Error {
    let header = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    let message = message_of(body);
    if status == StatusCode::UNAUTHORIZED {
        return Error::Unauthorized;
    }
    if status == StatusCode::FORBIDDEN || status == StatusCode::TOO_MANY_REQUESTS {
        let retry_after = header("retry-after")
            .and_then(|v| v.parse::<u64>().ok())
            .map(Duration::from_secs);
        let exhausted = header("x-ratelimit-remaining") == Some("0");
        if status == StatusCode::TOO_MANY_REQUESTS || exhausted || retry_after.is_some() {
            let reset = header("x-ratelimit-reset")
                .and_then(|v| v.parse::<i64>().ok())
                .and_then(|secs| Timestamp::from_second(secs).ok());
            return Error::RateLimited { reset, retry_after };
        }
        return Error::Forbidden(message);
    }
    let message = if message.is_empty() {
        status.canonical_reason().unwrap_or_default().to_owned()
    } else {
        message
    };
    Error::Api {
        status: status.as_u16(),
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // GH-001
    #[test]
    fn base_urls_end_with_a_slash() {
        let url = |s: &str| base_url(s).unwrap().to_string();
        assert_eq!(url("https://api.github.com"), "https://api.github.com/");
        assert_eq!(
            url("http://127.0.0.1:8080/api/v3"),
            "http://127.0.0.1:8080/api/v3/"
        );
        assert!(matches!(
            base_url("api.github.com"),
            Err(Error::InvalidUrl(_))
        ));
        assert!(matches!(base_url("ftp://x"), Err(Error::InvalidUrl(_))));
    }

    // GH-001
    #[test]
    fn urls_keep_the_prefix_and_encode_segments() {
        let client = GithubClient::new("http://h/api/v3", SecretString::from("t")).unwrap();
        let url = client
            .url(&["repos", "o", "r", "a b"], &[("x", "y z")])
            .unwrap();
        assert_eq!(url.as_str(), "http://h/api/v3/repos/o/r/a%20b?x=y+z");
    }
}
