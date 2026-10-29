// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! What the stub knows: pull requests with their jobs, runners, and the log of requests.

use jiff::Timestamp;
use rc_github::{JobState, MergeState, PrState, ReviewDecision};
use serde_json::Value;

/// A job (a check run of Actions) of a pull request's head commit.
#[derive(Debug, Clone)]
pub struct Job {
    /// Assigned when the pull request is added.
    pub id: u64,
    pub name: String,
    pub workflow: String,
    /// The Actions run the job belongs to: the jobs of one workflow share it.
    pub run_id: u64,
    pub state: JobState,
    /// Where the job's page is; a made-up one on the stub's own address when `None`.
    pub url: Option<String>,
    pub started_at: Timestamp,
    pub completed_at: Option<Timestamp>,
}

impl Job {
    /// A running job of the workflow `CI`.
    #[must_use]
    pub fn new(name: &str) -> Self {
        Self {
            id: 0,
            name: name.to_owned(),
            workflow: "CI".to_owned(),
            run_id: 0,
            state: JobState::Running,
            url: None,
            started_at: Timestamp::now(),
            completed_at: None,
        }
    }

    #[must_use]
    pub fn workflow(mut self, workflow: &str) -> Self {
        workflow.clone_into(&mut self.workflow);
        self
    }

    #[must_use]
    pub fn state(mut self, state: JobState) -> Self {
        self.set_state(state);
        self
    }

    #[must_use]
    pub fn url(mut self, url: &str) -> Self {
        self.url = Some(url.to_owned());
        self
    }

    /// Moves the job to `state` the way a run would: a finished job has an end time, a started
    /// one a new start time.
    pub fn set_state(&mut self, state: JobState) {
        self.state = state;
        if state == JobState::Running {
            self.started_at = Timestamp::now();
            self.completed_at = None;
        } else {
            self.completed_at = Some(Timestamp::now());
        }
    }
}

/// An open (or merged) pull request. Build one with [`PullRequest::new`] and the chained setters,
/// add it with [`GithubStub::add_pull_request`](crate::GithubStub::add_pull_request); the fields
/// are public for [`GithubStub::edit_pull_request`](crate::GithubStub::edit_pull_request).
#[derive(Debug, Clone)]
pub struct PullRequest {
    /// `owner/name`.
    pub repo: String,
    /// 0 picks the next free number of the repository when it is added.
    pub number: u64,
    pub title: String,
    pub author: String,
    pub draft: bool,
    /// Empty picks `feature/<number>`.
    pub head_branch: String,
    pub base_branch: String,
    /// Empty picks a new one.
    pub head_sha: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub review: Option<ReviewDecision>,
    pub merge_state: MergeState,
    pub jobs: Vec<Job>,
    /// The Actions run of the head commit waits for a maintainer's approval (its suite concluded
    /// `ACTION_REQUIRED`).
    pub needs_approval: bool,
    pub(crate) approval_run_id: u64,
    /// Merged by a request to the stub; a merged pull request leaves every list.
    pub state: PrState,
}

impl PullRequest {
    /// An open pull request by `someone` into `main`, clean, with no jobs and no review.
    #[must_use]
    pub fn new(repo: &str, title: &str) -> Self {
        let now = Timestamp::now();
        Self {
            repo: repo.to_owned(),
            number: 0,
            title: title.to_owned(),
            author: "someone".to_owned(),
            draft: false,
            head_branch: String::new(),
            base_branch: "main".to_owned(),
            head_sha: String::new(),
            created_at: now,
            updated_at: now,
            review: None,
            merge_state: MergeState::Clean,
            jobs: Vec::new(),
            needs_approval: false,
            approval_run_id: 0,
            state: PrState::Open,
        }
    }

    #[must_use]
    pub fn number(mut self, number: u64) -> Self {
        self.number = number;
        self
    }

    #[must_use]
    pub fn author(mut self, author: &str) -> Self {
        author.clone_into(&mut self.author);
        self
    }

    /// A draft; its merge state is `DRAFT`, as GitHub's.
    #[must_use]
    pub fn draft(mut self) -> Self {
        self.draft = true;
        self.merge_state = MergeState::Draft;
        self
    }

    #[must_use]
    pub fn head(mut self, branch: &str) -> Self {
        branch.clone_into(&mut self.head_branch);
        self
    }

    #[must_use]
    pub fn base(mut self, branch: &str) -> Self {
        branch.clone_into(&mut self.base_branch);
        self
    }

    #[must_use]
    pub fn head_sha(mut self, sha: &str) -> Self {
        sha.clone_into(&mut self.head_sha);
        self
    }

    #[must_use]
    pub fn created_at(mut self, at: Timestamp) -> Self {
        self.created_at = at;
        self
    }

    #[must_use]
    pub fn updated_at(mut self, at: Timestamp) -> Self {
        self.updated_at = at;
        self
    }

    #[must_use]
    pub fn review(mut self, review: ReviewDecision) -> Self {
        self.review = Some(review);
        self
    }

    #[must_use]
    pub fn merge_state(mut self, state: MergeState) -> Self {
        self.merge_state = state;
        self
    }

    #[must_use]
    pub fn job(mut self, job: Job) -> Self {
        self.jobs.push(job);
        self
    }

    #[must_use]
    pub fn needs_approval(mut self) -> Self {
        self.needs_approval = true;
        self
    }
}

/// What a busy runner is running.
#[derive(Debug, Clone)]
pub struct RunnerJob {
    pub name: String,
    pub workflow: String,
    pub head_branch: String,
    pub url: String,
    pub started_at: Timestamp,
}

impl RunnerJob {
    #[must_use]
    pub fn new(name: &str, workflow: &str, head_branch: &str) -> Self {
        Self {
            name: name.to_owned(),
            workflow: workflow.to_owned(),
            head_branch: head_branch.to_owned(),
            url: String::new(),
            started_at: Timestamp::now(),
        }
    }
}

/// A self-hosted runner of a repository; busy when it has a job.
#[derive(Debug, Clone)]
pub struct Runner {
    pub name: String,
    pub os: String,
    pub online: bool,
    pub custom_labels: Vec<String>,
    pub job: Option<RunnerJob>,
}

impl Runner {
    /// An online, idle Linux runner.
    #[must_use]
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            os: "linux".to_owned(),
            online: true,
            custom_labels: Vec::new(),
            job: None,
        }
    }

    #[must_use]
    pub fn offline(mut self) -> Self {
        self.online = false;
        self
    }

    #[must_use]
    pub fn label(mut self, label: &str) -> Self {
        self.custom_labels.push(label.to_owned());
        self
    }

    #[must_use]
    pub fn running(mut self, job: RunnerJob) -> Self {
        self.job = Some(job);
        self
    }
}

/// An answer to give instead of the real one.
#[derive(Debug, Clone)]
pub struct Failure {
    pub(crate) status: u16,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: Value,
}

impl Failure {
    /// A REST error: `status` with GitHub's `{"message": ...}`.
    #[must_use]
    pub fn new(status: u16, message: &str) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: serde_json::json!({ "message": message }),
        }
    }

    /// 401: the token is refused.
    #[must_use]
    pub fn unauthorized() -> Self {
        Self::new(401, "Bad credentials")
    }

    /// 403 with `x-ratelimit-remaining: 0` and a reset a minute from now.
    #[must_use]
    pub fn rate_limited() -> Self {
        let reset = Timestamp::now().as_second() + 60;
        Self::new(403, "API rate limit exceeded")
            .header("x-ratelimit-remaining", "0")
            .header("x-ratelimit-reset", &reset.to_string())
    }

    /// 500.
    #[must_use]
    pub fn server_error() -> Self {
        Self::new(500, "Server Error")
    }

    /// A 200 whose GraphQL `errors` carry `message`.
    #[must_use]
    pub fn graphql(message: &str) -> Self {
        Self {
            status: 200,
            headers: Vec::new(),
            body: serde_json::json!({ "errors": [{ "message": message }] }),
        }
    }

    /// A 200 whose GraphQL `errors` say the rate limit is spent, as GraphQL does.
    #[must_use]
    pub fn graphql_rate_limited() -> Self {
        Self {
            status: 200,
            headers: Vec::new(),
            body: serde_json::json!({
                "errors": [{ "type": "RATE_LIMITED", "message": "API rate limit exceeded" }]
            }),
        }
    }

    #[must_use]
    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_owned(), value.to_owned()));
        self
    }
}

/// A request the stub received.
#[derive(Debug, Clone)]
pub struct Request {
    pub method: String,
    pub path: String,
    /// For GraphQL a short form of the operation: `viewer`, `search <query>`, `status o/r#12`,
    /// `checks o/r#12`, `approvals o/r#12`, `rebase o/r#12`. `None` for REST.
    pub operation: Option<String>,
    /// The JSON body, if any.
    pub body: Option<Value>,
}
