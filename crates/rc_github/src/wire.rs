// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The queries and the JSON as GitHub sends it. Private to the crate: everything is converted into
//! the public types. Unknown fields are ignored and optional ones default.
//!
//! The queries are those of the `ghw` command line tool, except that they take variables instead
//! of pasting values into the text, and that a pull request is named by repository and number
//! (`repository(owner:, name:) { pullRequest(number:) }`) rather than by URL (`resource(url:)`),
//! which keeps them independent of the host.

use jiff::Timestamp;
use serde::Deserialize;

use crate::types::{
    CiState, Job, JobState, PrState, PullRequest, PullRequestStatus, Runner, RunnerJob,
};

/// What a check run is, as a GraphQL selection: for an Actions job its id (the job id), its
/// workflow, and the times; for a commit status of an external CI its context.
macro_rules! contexts {
    () => {
        "contexts(first: 100) {
      nodes {
        __typename
        ... on CheckRun {
          databaseId name status conclusion detailsUrl startedAt completedAt
          checkSuite { workflowRun { databaseId workflow { name } } }
        }
        ... on StatusContext { context state targetUrl }
      }
    }"
    };
}

/// The suites of the head commit; those concluded `ACTION_REQUIRED` wait for approval.
macro_rules! check_suites {
    () => {
        "checkSuites(first: 50) { nodes { conclusion workflowRun { databaseId } } }"
    };
}

/// Who the token belongs to.
pub(crate) const VIEWER_QUERY: &str = "query { viewer { login } }";

/// A search for pull requests, one page. The caller pages on `pageInfo`.
pub(crate) const SEARCH_QUERY: &str = concat!(
    "query($q: String!, $first: Int!, $after: String) {
  search(query: $q, type: ISSUE, first: $first, after: $after) {
    pageInfo { hasNextPage endCursor }
    nodes { ...Pr }
  }
}
fragment Pr on PullRequest {
  repository { nameWithOwner }
  number
  author { login }
  title
  url
  updatedAt
  isDraft
  headRefName
  baseRefName
  headRefOid
  commits(last: 1) {
    nodes { commit { statusCheckRollup { state ",
    contexts!(),
    " } ",
    check_suites!(),
    " } }
  }
}"
);

pub(crate) const STATUS_QUERY: &str = "query($owner: String!, $name: String!, $number: Int!) {
  repository(owner: $owner, name: $name) {
    pullRequest(number: $number) {
      number
      title
      state
      repository { nameWithOwner }
      headRefName
      headRefOid
      headRepository { nameWithOwner }
      commits(last: 1) { nodes { commit { statusCheckRollup { state } } } }
    }
  }
}";

pub(crate) const RUNS_QUERY: &str = concat!(
    "query($owner: String!, $name: String!, $number: Int!) {
  repository(owner: $owner, name: $name) {
    pullRequest(number: $number) {
      commits(last: 1) { nodes { commit { statusCheckRollup { state ",
    contexts!(),
    " } } } }
    }
  }
}"
);

pub(crate) const APPROVAL_QUERY: &str = concat!(
    "query($owner: String!, $name: String!, $number: Int!) {
  repository(owner: $owner, name: $name) {
    pullRequest(number: $number) {
      commits(last: 1) { nodes { commit { ",
    check_suites!(),
    " } } }
    }
  }
}"
);

pub(crate) const ID_QUERY: &str = "query($owner: String!, $name: String!, $number: Int!) {
  repository(owner: $owner, name: $name) {
    pullRequest(number: $number) { id }
  }
}";

/// REST's update-branch can only merge; the mutation can rebase, as `gh pr update-branch
/// --rebase` does.
pub(crate) const REBASE_MUTATION: &str = "mutation($id: ID!) {
  updatePullRequestBranch(input: { pullRequestId: $id, updateMethod: REBASE }) {
    pullRequest { id }
  }
}";

// --- GraphQL envelope ---

#[derive(Deserialize)]
pub(crate) struct GraphQlResponse<T> {
    pub data: Option<T>,
    #[serde(default)]
    pub errors: Vec<GraphQlError>,
}

#[derive(Deserialize)]
pub(crate) struct GraphQlError {
    pub message: String,
    #[serde(rename = "type")]
    pub kind: Option<String>,
}

// --- search ---

#[derive(Deserialize)]
pub(crate) struct ViewerData {
    pub viewer: Viewer,
}

#[derive(Deserialize)]
pub(crate) struct Viewer {
    pub login: String,
}

#[derive(Deserialize)]
pub(crate) struct SearchData {
    pub search: Search,
}

#[derive(Deserialize)]
pub(crate) struct Search {
    #[serde(rename = "pageInfo")]
    pub page_info: PageInfo,
    pub nodes: Vec<PrNode>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PageInfo {
    pub has_next_page: bool,
    pub end_cursor: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PrNode {
    repository: Repository,
    number: u64,
    /// Null for deleted accounts.
    author: Option<Author>,
    title: String,
    url: String,
    updated_at: Timestamp,
    is_draft: bool,
    head_ref_name: String,
    base_ref_name: String,
    head_ref_oid: String,
    commits: Commits,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Repository {
    name_with_owner: String,
}

#[derive(Deserialize)]
struct Author {
    login: String,
}

#[derive(Deserialize)]
struct Commits {
    nodes: Vec<CommitNode>,
}

#[derive(Deserialize)]
struct CommitNode {
    commit: Commit,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Commit {
    status_check_rollup: Option<Rollup>,
    check_suites: Option<CheckSuites>,
}

#[derive(Deserialize)]
struct Rollup {
    state: String,
    contexts: Option<Contexts>,
}

#[derive(Deserialize)]
struct Contexts {
    nodes: Vec<CheckContext>,
}

#[derive(Deserialize)]
struct CheckSuites {
    nodes: Vec<SuiteNode>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SuiteNode {
    conclusion: Option<String>,
    workflow_run: Option<RunId>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunId {
    database_id: u64,
}

/// GitHub Actions jobs are check runs; status contexts come from external CI through the statuses
/// API.
#[derive(Deserialize)]
#[serde(tag = "__typename", rename_all_fields = "camelCase")]
enum CheckContext {
    CheckRun {
        database_id: u64,
        name: String,
        status: String,
        conclusion: Option<String>,
        details_url: Option<String>,
        started_at: Option<Timestamp>,
        completed_at: Option<Timestamp>,
        check_suite: Option<CheckSuite>,
    },
    StatusContext {
        context: String,
        state: String,
        target_url: Option<String>,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CheckSuite {
    workflow_run: Option<WorkflowRun>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkflowRun {
    database_id: u64,
    workflow: Workflow,
}

#[derive(Deserialize)]
struct Workflow {
    name: String,
}

fn is_passing(conclusion: Option<&str>) -> bool {
    matches!(
        conclusion,
        Some("SUCCESS" | "NEUTRAL" | "SKIPPED" | "STALE")
    )
}

impl From<CheckContext> for Job {
    fn from(context: CheckContext) -> Self {
        match context {
            CheckContext::CheckRun {
                database_id,
                name,
                status,
                conclusion,
                details_url,
                started_at,
                completed_at,
                check_suite,
            } => {
                let state = match (status.as_str(), conclusion.as_deref()) {
                    ("COMPLETED", c) if is_passing(c) => JobState::Passed,
                    ("COMPLETED", Some("CANCELLED")) => JobState::Cancelled,
                    ("COMPLETED", _) => JobState::Failed,
                    _ => JobState::Running,
                };
                Self {
                    id: Some(database_id),
                    name,
                    workflow_name: check_suite
                        .and_then(|s| s.workflow_run)
                        .map(|run| run.workflow.name),
                    state,
                    conclusion,
                    url: details_url,
                    started_at,
                    completed_at,
                }
            }
            CheckContext::StatusContext {
                context,
                state,
                target_url,
            } => Self {
                id: None,
                name: context,
                workflow_name: None,
                state: match CiState::from_rollup(Some(&state)) {
                    CiState::Success => JobState::Passed,
                    CiState::Failure => JobState::Failed,
                    CiState::Pending | CiState::None => JobState::Running,
                },
                conclusion: None,
                url: target_url,
                started_at: None,
                completed_at: None,
            },
        }
    }
}

/// An Actions run with failed or cancelled jobs.
#[derive(Debug, PartialEq)]
pub(crate) struct FailedRun {
    pub id: u64,
    /// Job ids, excluding cancelled jobs.
    pub failed_jobs: Vec<u64>,
    pub has_cancelled: bool,
}

impl Commits {
    fn head(&self) -> Option<&Commit> {
        self.nodes.first().map(|node| &node.commit)
    }

    fn ci(&self) -> CiState {
        let state = self
            .head()
            .and_then(|commit| commit.status_check_rollup.as_ref())
            .map(|rollup| rollup.state.as_str());
        CiState::from_rollup(state)
    }

    fn into_contexts(self) -> Vec<CheckContext> {
        self.nodes
            .into_iter()
            .next()
            .and_then(|node| node.commit.status_check_rollup)
            .and_then(|rollup| rollup.contexts)
            .map_or_else(Vec::new, |contexts| contexts.nodes)
    }

    /// Ids of the head commit's Actions runs that wait for approval. Such runs have no jobs yet,
    /// so the rollup is null and they show up only as suites concluded `ACTION_REQUIRED`.
    fn approval_runs(&self) -> Vec<u64> {
        let suites = self.head().and_then(|c| c.check_suites.as_ref());
        suites
            .into_iter()
            .flat_map(|suites| &suites.nodes)
            .filter(|suite| suite.conclusion.as_deref() == Some("ACTION_REQUIRED"))
            .filter_map(|suite| suite.workflow_run.as_ref().map(|run| run.database_id))
            .collect()
    }

    /// Actions runs with a failed or cancelled job, in GitHub's order.
    fn failed_runs(self) -> Vec<FailedRun> {
        let mut runs: Vec<FailedRun> = Vec::new();
        for context in self.into_contexts() {
            let CheckContext::CheckRun {
                database_id,
                status,
                conclusion,
                check_suite:
                    Some(CheckSuite {
                        workflow_run: Some(run),
                    }),
                ..
            } = context
            else {
                continue;
            };
            if status != "COMPLETED" || is_passing(conclusion.as_deref()) {
                continue;
            }
            let index = runs
                .iter()
                .position(|r| r.id == run.database_id)
                .unwrap_or_else(|| {
                    runs.push(FailedRun {
                        id: run.database_id,
                        failed_jobs: Vec::new(),
                        has_cancelled: false,
                    });
                    runs.len() - 1
                });
            let failed_run = &mut runs[index];
            if conclusion.as_deref() == Some("CANCELLED") {
                failed_run.has_cancelled = true;
            } else {
                failed_run.failed_jobs.push(database_id);
            }
        }
        runs
    }
}

impl From<PrNode> for PullRequest {
    fn from(node: PrNode) -> Self {
        let ci = node.commits.ci();
        let needs_approval = !node.commits.approval_runs().is_empty();
        let jobs = node
            .commits
            .into_contexts()
            .into_iter()
            .map(Job::from)
            .collect();
        Self {
            repo: node.repository.name_with_owner,
            number: node.number,
            title: node.title,
            author: node.author.map_or_else(|| "ghost".to_owned(), |a| a.login),
            url: node.url,
            updated_at: node.updated_at,
            is_draft: node.is_draft,
            head_branch: node.head_ref_name,
            base_branch: node.base_ref_name,
            head_sha: node.head_ref_oid,
            ci,
            jobs,
            needs_approval,
        }
    }
}

// --- one pull request ---

/// `{ repository { pullRequest } }`, either of them null when it doesn't exist or can't be seen.
#[derive(Deserialize)]
pub(crate) struct RepositoryData<T> {
    repository: Option<PullRequestSlot<T>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PullRequestSlot<T> {
    pull_request: Option<T>,
}

impl<T> RepositoryData<T> {
    pub fn into_pull_request(self) -> Option<T> {
        self.repository?.pull_request
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StatusNode {
    number: u64,
    title: String,
    state: String,
    repository: Repository,
    head_ref_name: String,
    head_ref_oid: String,
    head_repository: Option<Repository>,
    commits: Commits,
}

impl StatusNode {
    pub fn into_status(self) -> PullRequestStatus {
        let state = match self.state.as_str() {
            "MERGED" => PrState::Merged,
            "CLOSED" => PrState::Closed,
            _ => PrState::Open,
        };
        PullRequestStatus {
            ci: self.commits.ci(),
            repo: self.repository.name_with_owner,
            number: self.number,
            title: self.title,
            state,
            head_branch: self.head_ref_name,
            head_sha: self.head_ref_oid,
            head_repo: self.head_repository.map(|r| r.name_with_owner),
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct CommitsNode {
    commits: Commits,
}

impl CommitsNode {
    pub fn failed_runs(self) -> Vec<FailedRun> {
        self.commits.failed_runs()
    }

    pub fn approval_runs(&self) -> Vec<u64> {
        self.commits.approval_runs()
    }
}

#[derive(Deserialize)]
pub(crate) struct IdNode {
    pub id: String,
}

// --- REST ---

#[derive(Deserialize)]
pub(crate) struct ErrorBody {
    pub message: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct SubscribedRepo {
    pub full_name: String,
}

#[derive(Deserialize)]
pub(crate) struct RunnersBody {
    pub runners: Vec<RunnerBody>,
}

#[derive(Deserialize)]
pub(crate) struct RunnerBody {
    name: String,
    os: String,
    status: String,
    busy: bool,
    #[serde(default)]
    labels: Vec<LabelBody>,
}

#[derive(Deserialize)]
struct LabelBody {
    name: String,
    #[serde(rename = "type")]
    kind: String,
}

impl From<RunnerBody> for Runner {
    fn from(body: RunnerBody) -> Self {
        Self {
            online: body.status == "online",
            custom_labels: body
                .labels
                .into_iter()
                .filter(|label| label.kind == "custom")
                .map(|label| label.name)
                .collect(),
            name: body.name,
            os: body.os,
            busy: body.busy,
            job: None,
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct RunsBody {
    pub workflow_runs: Vec<RunBody>,
}

#[derive(Deserialize)]
pub(crate) struct RunBody {
    pub id: u64,
}

#[derive(Deserialize)]
pub(crate) struct JobsBody {
    pub jobs: Vec<JobBody>,
}

#[derive(Deserialize)]
pub(crate) struct JobBody {
    name: String,
    workflow_name: Option<String>,
    head_branch: Option<String>,
    started_at: Timestamp,
    html_url: String,
    pub status: String,
    runner_name: Option<String>,
}

impl From<JobBody> for RunnerJob {
    fn from(body: JobBody) -> Self {
        Self {
            name: body.name,
            workflow_name: body.workflow_name.unwrap_or_default(),
            head_branch: body.head_branch.unwrap_or_default(),
            started_at: body.started_at,
            url: body.html_url,
            runner_name: body.runner_name,
        }
    }
}

/// Response of the asynchronous merge endpoint, both when requesting and when polling.
#[derive(Deserialize)]
pub(crate) struct AsyncMerge {
    /// `pending`, `merged`, `enqueued` or `failed`.
    pub status: String,
    #[serde(default)]
    pub details: AsyncMergeDetails,
}

#[derive(Default, Deserialize)]
pub(crate) struct AsyncMergeDetails {
    pub message: Option<String>,
    /// Only while pending.
    pub uuid: Option<String>,
}
