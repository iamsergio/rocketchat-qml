// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The stub's state and the answers to the requests of `rc_github`, in GitHub's JSON.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex};

use jiff::Timestamp;
use jiff::civil::Date;
use jiff::tz::TimeZone;
use rc_github::{JobState, MergeState, PrState, ReviewDecision};
use serde_json::{Value, json};
use wiremock::{Request as WireRequest, Respond, ResponseTemplate};

use crate::model::{Failure, Job, PullRequest, Request, Runner, RunnerJob};

/// The most a REST list returns per page, when the client doesn't say.
const DEFAULT_PER_PAGE: usize = 30;
const DEFAULT_SEARCH_PAGE: usize = 100;
/// The ids of the made-up runs and jobs of the runners: this plus the runner's index.
const RUNNER_JOB_BASE: u64 = 9_000_000;

pub(crate) struct State {
    /// The stub's own address, for the URLs in the answers.
    pub base: String,
    pub viewer: String,
    pub required_token: Option<String>,
    pub watched: Vec<String>,
    pub prs: Vec<PullRequest>,
    pub runners: BTreeMap<String, Vec<Runner>>,
    /// `(repo, branch)` of the branches a `DELETE` can still delete.
    pub branches: BTreeSet<(String, String)>,
    pub requests: Vec<Request>,
    /// A request's path prefix (starting with `/`) or a GraphQL operation prefix, the answer, and
    /// how many requests it still applies to.
    pub failures: Vec<(String, Failure, usize)>,
    /// How many times a merge answers `pending` before it is `merged`.
    pub merge_polls: usize,
    merges: HashMap<String, usize>,
    next_id: u64,
}

impl State {
    pub fn new() -> Self {
        Self {
            base: String::new(),
            viewer: "me".to_owned(),
            required_token: None,
            watched: Vec::new(),
            prs: Vec::new(),
            runners: BTreeMap::new(),
            branches: BTreeSet::new(),
            requests: Vec::new(),
            failures: Vec::new(),
            merge_polls: 0,
            merges: HashMap::new(),
            next_id: 1000,
        }
    }

    fn id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    fn sha(&mut self) -> String {
        format!("{:040x}", self.id())
    }

    /// Adds a pull request, filling in what the builder left open. Returns its number.
    pub fn add(&mut self, mut pr: PullRequest) -> u64 {
        if pr.number == 0 {
            let last = self
                .prs
                .iter()
                .filter(|p| p.repo == pr.repo)
                .map(|p| p.number);
            pr.number = last.max().unwrap_or(0) + 1;
        }
        assert!(
            self.find(&pr.repo, pr.number).is_none(),
            "{}#{} exists already",
            pr.repo,
            pr.number
        );
        if pr.head_branch.is_empty() {
            pr.head_branch = format!("feature/{}", pr.number);
        }
        if pr.head_sha.is_empty() {
            pr.head_sha = self.sha();
        }
        if pr.needs_approval {
            pr.approval_run_id = self.id();
        }
        for index in 0..pr.jobs.len() {
            self.number_job(&mut pr.jobs, index);
        }
        self.branches
            .insert((pr.repo.clone(), pr.head_branch.clone()));
        let number = pr.number;
        self.prs.push(pr);
        number
    }

    /// Gives the job its id, and its run: that of the earlier jobs of its workflow.
    fn number_job(&mut self, jobs: &mut [Job], index: usize) {
        if jobs[index].id == 0 {
            jobs[index].id = self.id();
        }
        if jobs[index].run_id == 0 {
            let same = jobs[..index]
                .iter()
                .find(|j| j.workflow == jobs[index].workflow && j.run_id != 0)
                .map(|j| j.run_id);
            jobs[index].run_id = same.unwrap_or_else(|| self.id());
        }
    }

    pub fn find(&self, repo: &str, number: u64) -> Option<&PullRequest> {
        self.prs
            .iter()
            .find(|p| p.repo == repo && p.number == number)
    }

    pub fn find_mut(&mut self, repo: &str, number: u64) -> Option<&mut PullRequest> {
        self.prs
            .iter_mut()
            .find(|p| p.repo == repo && p.number == number)
    }

    /// The planned failure that answers this request, if any, used up by one.
    fn failure_for(&mut self, path: &str, operation: Option<&str>) -> Option<Failure> {
        let index = self.failures.iter().position(|(target, _, left)| {
            *left > 0
                && if target.starts_with('/') {
                    path.starts_with(target.as_str())
                } else {
                    operation.is_some_and(|op| op.starts_with(target.as_str()))
                }
        })?;
        let (_, failure, left) = &mut self.failures[index];
        *left -= 1;
        Some(failure.clone())
    }
}

/// The wiremock side: every request goes through here.
pub(crate) struct Router(pub Arc<Mutex<State>>);

impl Respond for Router {
    fn respond(&self, request: &WireRequest) -> ResponseTemplate {
        let reply = match self.0.lock() {
            Ok(mut state) => handle(&mut state, request),
            // A test panicked while changing the state; the stub's answer says so.
            Err(_) => Reply::rest(500, json!({ "message": "stub state poisoned" })),
        };
        let mut template = ResponseTemplate::new(reply.status);
        for (name, value) in reply.headers {
            template = template.insert_header(name.as_str(), value.as_str());
        }
        match reply.body {
            Value::Null => template,
            body => template.set_body_json(body),
        }
    }
}

struct Reply {
    status: u16,
    headers: Vec<(String, String)>,
    body: Value,
}

impl Reply {
    fn rest(status: u16, body: Value) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body,
        }
    }

    fn ok(body: Value) -> Self {
        Self::rest(200, body)
    }

    fn not_found() -> Self {
        Self::rest(404, json!({ "message": "Not Found" }))
    }

    fn no_content() -> Self {
        Self::rest(204, Value::Null)
    }
}

impl From<Failure> for Reply {
    fn from(failure: Failure) -> Self {
        Self {
            status: failure.status,
            headers: failure.headers,
            body: failure.body,
        }
    }
}

fn handle(state: &mut State, request: &WireRequest) -> Reply {
    let method = request.method.as_str().to_owned();
    let path = request.url.path().to_owned();
    let body: Option<Value> = serde_json::from_slice(&request.body).ok();
    let is_graphql = path == "/graphql";
    let graphql = if is_graphql {
        body.as_ref().map(GraphqlCall::parse)
    } else {
        None
    };
    let operation = graphql.as_ref().map(|call| call.operation.clone());
    state.requests.push(Request {
        method: method.clone(),
        path: path.clone(),
        operation: operation.clone(),
        body: body.clone(),
    });

    if let Some(required) = &state.required_token {
        let sent = request
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok());
        if sent != Some(format!("Bearer {required}").as_str()) {
            return Failure::unauthorized().into();
        }
    }
    if let Some(failure) = state.failure_for(&path, operation.as_deref()) {
        return failure.into();
    }

    let segments: Vec<&str> = path.trim_matches('/').split('/').collect();
    let query: HashMap<String, String> = request
        .url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    match (method.as_str(), segments.as_slice()) {
        ("POST", ["graphql"]) => match graphql {
            Some(call) => Reply::ok(call.run(state)),
            None => Reply::rest(400, json!({ "message": "Problems parsing JSON" })),
        },
        ("GET", ["user", "subscriptions"]) => {
            let items = state
                .watched
                .iter()
                .map(|repo| json!({ "full_name": repo }))
                .collect();
            Reply::ok(Value::Array(page(items, &query)))
        }
        ("GET", ["repos", owner, name, "actions", "runners"]) => {
            runners(state, &format!("{owner}/{name}"), &query)
        }
        ("GET", ["repos", owner, name, "actions", "runs"]) => {
            runs(state, &format!("{owner}/{name}"), &query)
        }
        ("GET", ["repos", owner, name, "actions", "runs", run, "jobs"]) => {
            run_jobs(state, &format!("{owner}/{name}"), run, &query)
        }
        (
            "POST",
            [
                "repos",
                owner,
                name,
                "actions",
                "runs",
                run,
                "rerun-failed-jobs",
            ],
        ) => rerun_failed(state, &format!("{owner}/{name}"), run),
        ("POST", ["repos", owner, name, "actions", "runs", run, "approve"]) => {
            approve(state, &format!("{owner}/{name}"), run)
        }
        ("POST", ["repos", owner, name, "actions", "jobs", job, "rerun"]) => {
            rerun_job(state, &format!("{owner}/{name}"), job)
        }
        ("PUT", ["repos", owner, name, "pulls", number, "merge-async"]) => {
            merge(state, &format!("{owner}/{name}"), number, body.as_ref())
        }
        ("GET", ["repos", _, _, "pulls", _, "merge-async", uuid]) => poll_merge(state, uuid),
        ("DELETE", ["repos", owner, name, "git", "refs", "heads", branch @ ..]) => {
            let repo = format!("{owner}/{name}");
            if state.branches.remove(&(repo, branch.join("/"))) {
                Reply::no_content()
            } else {
                Reply::rest(422, json!({ "message": "Reference does not exist" }))
            }
        }
        _ => Reply::not_found(),
    }
}

/// `per_page` and `page` of a REST list.
fn page(items: Vec<Value>, query: &HashMap<String, String>) -> Vec<Value> {
    let number = |key: &str, default: usize| {
        query
            .get(key)
            .and_then(|v| v.parse::<usize>().ok())
            .filter(|n| *n > 0)
            .unwrap_or(default)
    };
    let per_page = number("per_page", DEFAULT_PER_PAGE);
    let page = number("page", 1);
    items
        .into_iter()
        .skip((page - 1) * per_page)
        .take(per_page)
        .collect()
}

fn runners(state: &State, repo: &str, query: &HashMap<String, String>) -> Reply {
    let all = state.runners.get(repo).map_or(&[][..], Vec::as_slice);
    let items = all
        .iter()
        .enumerate()
        .map(|(index, runner)| {
            let labels: Vec<Value> = ["self-hosted", runner.os.as_str()]
                .into_iter()
                .map(|name| (name, "read-only"))
                .chain(runner.custom_labels.iter().map(|l| (l.as_str(), "custom")))
                .enumerate()
                .map(|(i, (name, kind))| json!({ "id": i + 1, "name": name, "type": kind }))
                .collect();
            json!({
                "id": index + 1,
                "name": runner.name,
                "os": runner.os,
                "status": if runner.online { "online" } else { "offline" },
                "busy": runner.job.is_some(),
                "labels": labels,
            })
        })
        .collect();
    Reply::ok(json!({ "total_count": all.len(), "runners": page(items, query) }))
}

/// The runs and jobs a runner is busy with are made up from the runners: a run per job, its id
/// the job's.
fn runner_jobs(state: &State, repo: &str) -> Vec<(u64, String, RunnerJob)> {
    let Some(runners) = state.runners.get(repo) else {
        return Vec::new();
    };
    runners
        .iter()
        .enumerate()
        .filter_map(|(index, runner)| {
            let job = runner.job.as_ref()?;
            Some((
                RUNNER_JOB_BASE + index as u64,
                runner.name.clone(),
                job.clone(),
            ))
        })
        .collect()
}

fn runs(state: &State, repo: &str, query: &HashMap<String, String>) -> Reply {
    let items: Vec<Value> = runner_jobs(state, repo)
        .iter()
        .map(|(id, _, _)| json!({ "id": id, "status": "in_progress" }))
        .collect();
    Reply::ok(json!({ "total_count": items.len(), "workflow_runs": page(items, query) }))
}

fn run_jobs(state: &State, repo: &str, run: &str, query: &HashMap<String, String>) -> Reply {
    let items: Vec<Value> = runner_jobs(state, repo)
        .iter()
        .filter(|(id, _, _)| id.to_string() == run)
        .map(|(id, runner, job)| {
            json!({
                "id": id,
                "name": job.name,
                "workflow_name": job.workflow,
                "head_branch": job.head_branch,
                "status": "in_progress",
                "conclusion": null,
                "started_at": job.started_at.to_string(),
                "html_url": job.url,
                "runner_name": runner,
            })
        })
        .collect();
    Reply::ok(json!({ "total_count": items.len(), "jobs": page(items, query) }))
}

fn forbidden(message: &str) -> Reply {
    Reply::rest(403, json!({ "message": message }))
}

fn rerun_failed(state: &mut State, repo: &str, run: &str) -> Reply {
    let Ok(run) = run.parse::<u64>() else {
        return Reply::not_found();
    };
    let Some(pr) = state
        .prs
        .iter_mut()
        .find(|p| p.repo == repo && p.jobs.iter().any(|j| j.run_id == run))
    else {
        return Reply::not_found();
    };
    if pr
        .jobs
        .iter()
        .any(|j| j.run_id == run && j.state == JobState::Running)
    {
        return forbidden("Workflow run is still in progress");
    }
    for job in pr.jobs.iter_mut().filter(|j| j.run_id == run) {
        if matches!(job.state, JobState::Failed | JobState::Cancelled) {
            job.set_state(JobState::Running);
        }
    }
    pr.updated_at = Timestamp::now();
    Reply::rest(201, json!({}))
}

fn rerun_job(state: &mut State, repo: &str, job: &str) -> Reply {
    let Ok(id) = job.parse::<u64>() else {
        return Reply::not_found();
    };
    let Some(pr) = state
        .prs
        .iter_mut()
        .find(|p| p.repo == repo && p.jobs.iter().any(|j| j.id == id))
    else {
        return Reply::not_found();
    };
    if let Some(job) = pr.jobs.iter_mut().find(|j| j.id == id) {
        if job.state == JobState::Running {
            return forbidden("Job is still in progress");
        }
        job.set_state(JobState::Running);
    }
    pr.updated_at = Timestamp::now();
    Reply::rest(201, json!({}))
}

fn approve(state: &mut State, repo: &str, run: &str) -> Reply {
    let Ok(run) = run.parse::<u64>() else {
        return Reply::not_found();
    };
    let job_id = state.id();
    let Some(pr) = state
        .prs
        .iter_mut()
        .find(|p| p.repo == repo && p.needs_approval && p.approval_run_id == run)
    else {
        return Reply::not_found();
    };
    pr.needs_approval = false;
    if pr.jobs.is_empty() {
        // Waiting runs have no jobs yet; approving starts them.
        let mut job = Job::new("build");
        job.id = job_id;
        job.run_id = run;
        pr.jobs.push(job);
    }
    for job in &mut pr.jobs {
        job.set_state(JobState::Running);
    }
    pr.updated_at = Timestamp::now();
    Reply::rest(201, json!({}))
}

fn merge(state: &mut State, repo: &str, number: &str, body: Option<&Value>) -> Reply {
    let Some(number) = number.parse().ok() else {
        return Reply::not_found();
    };
    let polls = state.merge_polls;
    let sha = body.and_then(|b| b["sha"].as_str());
    let Some(pr) = state.find_mut(repo, number) else {
        return Reply::not_found();
    };
    if pr.state != PrState::Open {
        return Reply::rest(405, json!({ "message": "Pull Request is not mergeable" }));
    }
    if sha.is_some_and(|sha| sha != pr.head_sha) {
        return Reply::rest(
            409,
            json!({ "message": "Head branch was modified. Review and try the merge again." }),
        );
    }
    pr.state = PrState::Merged;
    pr.updated_at = Timestamp::now();
    if polls == 0 {
        return Reply::ok(json!({ "status": "merged", "details": {} }));
    }
    let uuid = format!("merge-{repo}-{number}").replace('/', "-");
    state.merges.insert(uuid.clone(), polls);
    Reply::ok(json!({ "status": "pending", "details": { "uuid": uuid } }))
}

fn poll_merge(state: &mut State, uuid: &str) -> Reply {
    match state.merges.get_mut(uuid) {
        None => Reply::not_found(),
        Some(left) if *left > 0 => {
            *left -= 1;
            Reply::ok(json!({ "status": "pending", "details": { "uuid": uuid } }))
        }
        Some(_) => Reply::ok(json!({ "status": "merged", "details": {} })),
    }
}

// --- GraphQL ---

enum Operation {
    Viewer,
    Search,
    Status,
    Checks,
    Approvals,
    Id,
    Rebase,
    Unknown,
}

struct GraphqlCall {
    kind: Operation,
    variables: Value,
    /// The short form in the log.
    operation: String,
}

/// The id of a pull request in the stub's GraphQL: the repository and the number.
fn node_id(repo: &str, number: u64) -> String {
    format!("PR_{repo}#{number}")
}

fn parse_node_id(id: &str) -> Option<(String, u64)> {
    let (repo, number) = id.strip_prefix("PR_")?.rsplit_once('#')?;
    Some((repo.to_owned(), number.parse().ok()?))
}

impl GraphqlCall {
    /// Tells the operation from its text; the queries of `rc_github` differ in what they select.
    fn parse(body: &Value) -> Self {
        let query = body["query"].as_str().unwrap_or_default();
        let variables = body["variables"].clone();
        let kind = if query.contains("updatePullRequestBranch") {
            Operation::Rebase
        } else if query.contains("search(") {
            Operation::Search
        } else if query.contains("viewer") {
            Operation::Viewer
        } else if query.contains("headRepository") {
            Operation::Status
        } else if query.contains("contexts(") {
            Operation::Checks
        } else if query.contains("checkSuites(") {
            Operation::Approvals
        } else if query.contains("{ id }") {
            Operation::Id
        } else {
            Operation::Unknown
        };
        let target = || {
            format!(
                "{}/{}#{}",
                variables["owner"].as_str().unwrap_or_default(),
                variables["name"].as_str().unwrap_or_default(),
                variables["number"]
            )
        };
        let operation = match kind {
            Operation::Viewer => "viewer".to_owned(),
            Operation::Search => format!("search {}", variables["q"].as_str().unwrap_or_default()),
            Operation::Status => format!("status {}", target()),
            Operation::Checks => format!("checks {}", target()),
            Operation::Approvals => format!("approvals {}", target()),
            Operation::Id => format!("id {}", target()),
            Operation::Rebase => {
                match parse_node_id(variables["id"].as_str().unwrap_or_default()) {
                    Some((repo, number)) => format!("rebase {repo}#{number}"),
                    None => "rebase".to_owned(),
                }
            }
            Operation::Unknown => "unknown".to_owned(),
        };
        Self {
            kind,
            variables,
            operation,
        }
    }

    fn run(&self, state: &mut State) -> Value {
        let vars = &self.variables;
        let repo = format!(
            "{}/{}",
            vars["owner"].as_str().unwrap_or_default(),
            vars["name"].as_str().unwrap_or_default()
        );
        let number = vars["number"].as_u64().unwrap_or_default();
        match self.kind {
            Operation::Viewer => json!({ "data": { "viewer": { "login": state.viewer } } }),
            Operation::Search => search(state, vars),
            Operation::Status | Operation::Checks | Operation::Approvals | Operation::Id => {
                let node = state.find(&repo, number).map(|pr| match self.kind {
                    Operation::Id => json!({ "id": node_id(&pr.repo, pr.number) }),
                    Operation::Status => status_node(&state.base, pr),
                    _ => json!({ "commits": commits(&state.base, pr) }),
                });
                json!({ "data": { "repository": { "pullRequest": node } } })
            }
            Operation::Rebase => rebase(state, vars["id"].as_str().unwrap_or_default()),
            Operation::Unknown => {
                json!({ "errors": [{ "message": "the stub does not know this query" }] })
            }
        }
    }
}

fn rebase(state: &mut State, id: &str) -> Value {
    let sha = state.sha();
    let pr = parse_node_id(id).and_then(|(repo, number)| state.find_mut(&repo, number));
    let Some(pr) = pr else {
        return json!({ "errors": [{ "message": "Could not resolve to a node" }] });
    };
    if pr.merge_state == MergeState::Dirty {
        return json!({ "errors": [{ "message": "Merge conflict between base and head" }] });
    }
    pr.merge_state = MergeState::Clean;
    pr.head_sha = sha;
    for job in &mut pr.jobs {
        job.set_state(JobState::Running);
    }
    pr.updated_at = Timestamp::now();
    json!({ "data": { "updatePullRequestBranch": { "pullRequest": { "id": id } } } })
}

fn search(state: &State, vars: &Value) -> Value {
    #[derive(PartialEq)]
    enum Author {
        Any,
        Viewer,
        Others,
    }
    let query = vars["q"].as_str().unwrap_or_default();
    let mut author = Author::Any;
    let mut repos: Vec<&str> = Vec::new();
    let mut since: Option<Date> = None;
    for word in query.split_whitespace() {
        match word {
            "author:@me" => author = Author::Viewer,
            "-author:@me" => author = Author::Others,
            _ => {
                if let Some(repo) = word.strip_prefix("repo:") {
                    repos.push(repo);
                } else if let Some(date) = word.strip_prefix("created:>=") {
                    since = date.parse().ok();
                }
            }
        }
    }
    let mut found: Vec<&PullRequest> = state
        .prs
        .iter()
        .filter(|pr| pr.state == PrState::Open)
        .filter(|pr| match author {
            Author::Any => true,
            Author::Viewer => pr.author == state.viewer,
            Author::Others => pr.author != state.viewer,
        })
        .filter(|pr| repos.is_empty() || repos.contains(&pr.repo.as_str()))
        .filter(|pr| {
            since.is_none_or(|since| pr.created_at.to_zoned(TimeZone::UTC).date() >= since)
        })
        .collect();
    found.sort_by_key(|pr| std::cmp::Reverse(pr.updated_at));

    // The cursor is the offset after the page.
    let first = vars["first"]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .unwrap_or(DEFAULT_SEARCH_PAGE);
    let start = vars["after"]
        .as_str()
        .and_then(|c| c.parse::<usize>().ok())
        .unwrap_or(0);
    let end = (start + first).min(found.len());
    let nodes: Vec<Value> = found
        .get(start..end)
        .unwrap_or_default()
        .iter()
        .map(|pr| pr_node(&state.base, pr))
        .collect();
    json!({ "data": { "search": {
        "pageInfo": { "hasNextPage": end < found.len(), "endCursor": end.to_string() },
        "nodes": nodes,
    } } })
}

fn review_word(review: Option<ReviewDecision>) -> Value {
    match review {
        Some(ReviewDecision::Approved) => json!("APPROVED"),
        Some(ReviewDecision::ChangesRequested) => json!("CHANGES_REQUESTED"),
        Some(ReviewDecision::ReviewRequired) => json!("REVIEW_REQUIRED"),
        None => Value::Null,
    }
}

fn merge_state_word(state: MergeState) -> &'static str {
    match state {
        MergeState::Behind => "BEHIND",
        MergeState::Dirty => "DIRTY",
        MergeState::Blocked => "BLOCKED",
        MergeState::Clean => "CLEAN",
        MergeState::Unstable => "UNSTABLE",
        MergeState::HasHooks => "HAS_HOOKS",
        MergeState::Draft => "DRAFT",
        MergeState::Unknown => "UNKNOWN",
    }
}

/// The check rollup: a failure counts at once, as on GitHub, even while other jobs run.
fn rollup_state(jobs: &[Job]) -> &'static str {
    if jobs
        .iter()
        .any(|j| matches!(j.state, JobState::Failed | JobState::Cancelled))
    {
        "FAILURE"
    } else if jobs.iter().any(|j| j.state == JobState::Running) {
        "PENDING"
    } else {
        "SUCCESS"
    }
}

fn job_node(base: &str, pr: &PullRequest, job: &Job) -> Value {
    let (status, conclusion) = match job.state {
        JobState::Passed => ("COMPLETED", json!("SUCCESS")),
        JobState::Failed => ("COMPLETED", json!("FAILURE")),
        JobState::Cancelled => ("COMPLETED", json!("CANCELLED")),
        JobState::Running => ("IN_PROGRESS", Value::Null),
    };
    let url = job.url.clone().unwrap_or_else(|| {
        format!(
            "{base}/{}/actions/runs/{}/job/{}",
            pr.repo, job.run_id, job.id
        )
    });
    json!({
        "__typename": "CheckRun",
        "databaseId": job.id,
        "name": job.name,
        "status": status,
        "conclusion": conclusion,
        "detailsUrl": url,
        "startedAt": job.started_at.to_string(),
        "completedAt": job.completed_at.map(|t| t.to_string()),
        "checkSuite": { "workflowRun": {
            "databaseId": job.run_id,
            "workflow": { "name": job.workflow },
        } },
    })
}

/// The `commits(last: 1)` connection, with everything the queries of `rc_github` select.
fn commits(base: &str, pr: &PullRequest) -> Value {
    let rollup = if pr.jobs.is_empty() {
        Value::Null
    } else {
        json!({
            "state": rollup_state(&pr.jobs),
            "contexts": { "nodes": pr.jobs.iter().map(|j| job_node(base, pr, j)).collect::<Vec<_>>() },
        })
    };
    let mut suites: Vec<Value> = Vec::new();
    let mut runs: Vec<u64> = Vec::new();
    for job in &pr.jobs {
        if !runs.contains(&job.run_id) {
            runs.push(job.run_id);
            let of_run = pr.jobs.iter().filter(|j| j.run_id == job.run_id);
            let running = of_run.clone().any(|j| j.state == JobState::Running);
            let conclusion = if running {
                Value::Null
            } else {
                json!("SUCCESS")
            };
            suites.push(
                json!({ "conclusion": conclusion, "workflowRun": { "databaseId": job.run_id } }),
            );
        }
    }
    if pr.needs_approval {
        suites.push(json!({
            "conclusion": "ACTION_REQUIRED",
            "workflowRun": { "databaseId": pr.approval_run_id },
        }));
    }
    json!({ "nodes": [{ "commit": {
        "statusCheckRollup": rollup,
        "checkSuites": { "nodes": suites },
    } }] })
}

fn pr_node(base: &str, pr: &PullRequest) -> Value {
    json!({
        "repository": { "nameWithOwner": pr.repo },
        "number": pr.number,
        "author": { "login": pr.author },
        "title": pr.title,
        "url": format!("{base}/{}/pull/{}", pr.repo, pr.number),
        "updatedAt": pr.updated_at.to_string(),
        "isDraft": pr.draft,
        "reviewDecision": review_word(pr.review),
        "mergeStateStatus": merge_state_word(pr.merge_state),
        "headRefName": pr.head_branch,
        "baseRefName": pr.base_branch,
        "headRefOid": pr.head_sha,
        "commits": commits(base, pr),
    })
}

fn status_node(base: &str, pr: &PullRequest) -> Value {
    let state = match pr.state {
        PrState::Open => "OPEN",
        PrState::Closed => "CLOSED",
        PrState::Merged => "MERGED",
    };
    json!({
        "number": pr.number,
        "title": pr.title,
        "state": state,
        "repository": { "nameWithOwner": pr.repo },
        "headRefName": pr.head_branch,
        "headRefOid": pr.head_sha,
        "headRepository": { "nameWithOwner": pr.repo },
        "commits": commits(base, pr),
    })
}
