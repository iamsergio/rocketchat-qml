// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! `rc_github::GithubClient` against the stub, for every call and every change of state the stub
//! makes: the stub and the client agree, and a change to either shows here.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use jiff::{Timestamp, ToSpan};
use rc_github::{
    BranchDeletion, CiState, Error, GithubClient, JobState, MergeState, PrRef, PrState,
    ReviewDecision,
};
use rc_test_github::{Failure, GithubStub, Job, PullRequest, Runner, RunnerJob};
use secrecy::SecretString;

const REPO: &str = "iamsergio/rocketchat-qml";

fn client_with(stub: &GithubStub, token: &str) -> GithubClient {
    GithubClient::new(stub.url(), SecretString::from(token))
        .unwrap()
        .with_merge_polling(Duration::from_millis(10), Duration::from_secs(5))
}

fn client(stub: &GithubStub) -> GithubClient {
    client_with(stub, "token")
}

fn pr_ref(number: u64) -> PrRef {
    PrRef {
        repo: REPO.to_owned(),
        number,
    }
}

/// A pull request of the viewer whose CI has passed.
fn green(title: &str) -> PullRequest {
    PullRequest::new(REPO, title)
        .author("me")
        .job(Job::new("build").workflow("Build").state(JobState::Passed))
        .job(Job::new("lint").workflow("Build").state(JobState::Passed))
}

// INFRA-009
#[tokio::test]
async fn viewer_and_the_required_token() {
    let stub = GithubStub::start();
    stub.set_viewer("sergio");
    assert_eq!(client(&stub).viewer().await.unwrap(), "sergio");

    stub.require_token(Some("secret"));
    assert!(matches!(
        client_with(&stub, "other").viewer().await,
        Err(Error::Unauthorized)
    ));
    assert_eq!(
        client_with(&stub, "secret").viewer().await.unwrap(),
        "sergio"
    );

    stub.require_token(None);
    assert_eq!(client(&stub).viewer().await.unwrap(), "sergio");
}

// INFRA-009
#[tokio::test]
async fn my_pull_requests_carry_everything_the_client_reads() {
    let stub = GithubStub::start();
    let number = stub.add_pull_request(
        green("Add the room menu")
            .head("feature/room-menu")
            .base("develop")
            .head_sha("9f1c")
            .review(ReviewDecision::Approved)
            .merge_state(MergeState::Behind)
            .job(Job::new("test").workflow("Test").state(JobState::Failed))
            .job(Job::new("deploy").workflow("Test").url("http://ci/deploy")),
    );
    stub.add_pull_request(green("Someone else's").author("other"));
    stub.add_pull_request(green("A draft").draft());
    stub.add_pull_request(green("Merged").number(30));
    stub.edit_pull_request(REPO, 30, |pr| pr.state = PrState::Merged);

    let prs = client(&stub).my_pull_requests().await.unwrap();

    let titles: Vec<&str> = prs.iter().map(|p| p.title.as_str()).collect();
    assert_eq!(titles.len(), 2, "{titles:?}");
    let pr = prs.iter().find(|p| p.number == number).unwrap();
    assert_eq!(pr.repo, REPO);
    assert_eq!(pr.author, "me");
    assert!(!pr.is_draft);
    assert_eq!(pr.head_branch, "feature/room-menu");
    assert_eq!(pr.base_branch, "develop");
    assert_eq!(pr.head_sha, "9f1c");
    assert_eq!(pr.review, Some(ReviewDecision::Approved));
    assert_eq!(pr.merge_state, MergeState::Behind);
    assert_eq!(pr.ci, CiState::Failure);
    assert!(!pr.needs_approval);
    assert!(
        pr.url.ends_with(&format!("/{REPO}/pull/{number}")),
        "{}",
        pr.url
    );
    let jobs: Vec<_> = pr
        .jobs
        .iter()
        .map(|j| (j.name.as_str(), j.workflow_name.as_deref(), j.state))
        .collect();
    assert_eq!(
        jobs,
        [
            ("build", Some("Build"), JobState::Passed),
            ("lint", Some("Build"), JobState::Passed),
            ("test", Some("Test"), JobState::Failed),
            ("deploy", Some("Test"), JobState::Running),
        ]
    );
    assert!(pr.jobs.iter().all(|j| j.id.is_some()));
    assert_eq!(pr.jobs[3].url.as_deref(), Some("http://ci/deploy"));
    assert!(
        pr.jobs[0]
            .url
            .as_deref()
            .unwrap()
            .contains("/actions/runs/")
    );
    assert!(pr.jobs[0].completed_at.is_some());
    assert!(pr.jobs[3].completed_at.is_none());
    assert!(prs.iter().any(|p| p.is_draft));
}

// INFRA-009
#[tokio::test]
async fn pull_requests_without_jobs_have_no_checks_and_the_rollup_follows_the_jobs() {
    let stub = GithubStub::start();
    stub.add_pull_request(PullRequest::new(REPO, "bare").author("me"));
    let number = stub.add_pull_request(green("green"));
    let client = client(&stub);

    let prs = client.my_pull_requests().await.unwrap();
    assert_eq!(
        prs.iter().find(|p| p.title == "bare").unwrap().ci,
        CiState::None
    );
    assert_eq!(
        client
            .pull_request_status(&pr_ref(number))
            .await
            .unwrap()
            .ci,
        CiState::Success
    );

    stub.set_job_state(REPO, number, "lint", JobState::Running);
    assert_eq!(
        client
            .pull_request_status(&pr_ref(number))
            .await
            .unwrap()
            .ci,
        CiState::Pending
    );
    stub.set_job_state(REPO, number, "build", JobState::Cancelled);
    assert_eq!(
        client
            .pull_request_status(&pr_ref(number))
            .await
            .unwrap()
            .ci,
        CiState::Failure
    );
    stub.set_all_jobs(REPO, number, JobState::Passed);
    assert_eq!(
        client
            .pull_request_status(&pr_ref(number))
            .await
            .unwrap()
            .ci,
        CiState::Success
    );
}

// INFRA-009
#[tokio::test]
async fn editing_moves_the_update_time_and_the_order() {
    let stub = GithubStub::start();
    let old = Timestamp::now() - 48.hours();
    let first = stub.add_pull_request(green("first").updated_at(old));
    let second = stub.add_pull_request(green("second").updated_at(old + 1.hour()));
    let client = client(&stub);

    let numbers =
        |prs: Vec<rc_github::PullRequest>| prs.iter().map(|p| p.number).collect::<Vec<_>>();
    assert_eq!(
        numbers(client.my_pull_requests().await.unwrap()),
        [second, first]
    );

    stub.set_merge_state(REPO, first, MergeState::Blocked);
    stub.set_review(REPO, first, Some(ReviewDecision::ChangesRequested));
    let prs = client.my_pull_requests().await.unwrap();
    assert_eq!(numbers(prs.clone()), [first, second]);
    assert_eq!(prs[0].merge_state, MergeState::Blocked);
    assert_eq!(prs[0].review, Some(ReviewDecision::ChangesRequested));

    stub.remove_pull_request(REPO, first);
    assert_eq!(numbers(client.my_pull_requests().await.unwrap()), [second]);
}

// INFRA-009
#[tokio::test]
async fn watched_pull_requests_are_others_in_the_repos_since_the_date() {
    let stub = GithubStub::start();
    stub.set_watched(&["a/one", "b/two"]);
    let since = Timestamp::now() - 14 * 24.hours();
    let recent = since + 24.hours();
    stub.add_pull_request(
        PullRequest::new("a/one", "recent")
            .author("x")
            .created_at(recent),
    );
    stub.add_pull_request(
        PullRequest::new("a/one", "old")
            .author("x")
            .created_at(since - 48.hours()),
    );
    stub.add_pull_request(
        PullRequest::new("b/two", "other repo")
            .author("y")
            .created_at(recent),
    );
    stub.add_pull_request(
        PullRequest::new("c/three", "unwatched")
            .author("y")
            .created_at(recent),
    );
    stub.add_pull_request(
        PullRequest::new("a/one", "mine")
            .author("me")
            .created_at(recent),
    );
    let client = client(&stub);

    let repos = client.watched_repos().await.unwrap();
    assert_eq!(repos, ["a/one", "b/two"]);
    let date = since.to_zoned(jiff::tz::TimeZone::UTC).date();
    let mut titles: Vec<String> = client
        .watched_pull_requests(&repos, date)
        .await
        .unwrap()
        .into_iter()
        .map(|p| p.title)
        .collect();
    titles.sort();
    assert_eq!(titles, ["other repo", "recent"]);

    let ops = stub.operations("search");
    assert_eq!(ops.len(), 1);
    assert!(
        ops[0].contains("-author:@me") && ops[0].contains("repo:a/one"),
        "{ops:?}"
    );
}

// INFRA-009
#[tokio::test]
async fn lists_are_paged() {
    let stub = GithubStub::start();
    let many: Vec<String> = (0..230).map(|i| format!("o/r{i}")).collect();
    stub.set_watched(&many.iter().map(String::as_str).collect::<Vec<_>>());
    for i in 0..250 {
        stub.add_pull_request(PullRequest::new(REPO, &format!("pr {i}")).author("me"));
    }
    let client = client(&stub);

    assert_eq!(client.my_pull_requests().await.unwrap().len(), 250);
    assert_eq!(stub.operations("search").len(), 3);
    assert_eq!(client.watched_repos().await.unwrap(), many);
    assert_eq!(stub.requests_to("GET", "/user/subscriptions").len(), 3);
}

// INFRA-009
#[tokio::test]
async fn merge_ends_the_pull_request_and_deletes_the_branch() {
    let stub = GithubStub::start();
    let number = stub.add_pull_request(green("ship it").head("feature/ship"));
    let other = stub.add_pull_request(green("stays"));
    assert!(stub.branch_exists(REPO, "feature/ship"));
    let client = client(&stub);

    let outcome = client.merge(&pr_ref(number), false).await.unwrap();

    assert_eq!(outcome.branch, BranchDeletion::Deleted);
    assert!(!stub.branch_exists(REPO, "feature/ship"));
    let left: Vec<u64> = client
        .my_pull_requests()
        .await
        .unwrap()
        .iter()
        .map(|p| p.number)
        .collect();
    assert_eq!(left, [other]);
    assert_eq!(
        stub.pull_request(REPO, number).unwrap().state,
        PrState::Merged
    );
    assert_eq!(
        client
            .pull_request_status(&pr_ref(number))
            .await
            .unwrap()
            .state,
        PrState::Merged
    );
    assert!(matches!(
        client.merge(&pr_ref(number), false).await,
        Err(Error::NotOpen(_))
    ));
    let puts = stub.requests_to("PUT", &format!("/repos/{REPO}/pulls/{number}/merge-async"));
    assert_eq!(puts.len(), 1);
    let body = puts[0].body.as_ref().unwrap();
    assert_eq!(body["merge_method"], "rebase");
    assert_eq!(
        body["sha"],
        stub.pull_request(REPO, number).unwrap().head_sha.as_str()
    );
}

// INFRA-009
#[tokio::test]
async fn merge_needs_green_checks_unless_forced() {
    let stub = GithubStub::start();
    let number = stub.add_pull_request(green("red").head("feature/red"));
    stub.set_job_state(REPO, number, "lint", JobState::Failed);
    let client = client(&stub);

    assert!(matches!(
        client.merge(&pr_ref(number), false).await,
        Err(Error::ChecksNotPassed(CiState::Failure))
    ));
    assert!(stub.requests_to("PUT", "/repos").is_empty());

    client.merge(&pr_ref(number), true).await.unwrap();
    assert_eq!(stub.requests_to("PUT", "/repos").len(), 1);
}

// INFRA-009
#[tokio::test]
async fn a_stale_sha_is_refused_with_409() {
    let stub = GithubStub::start();
    let number = stub.add_pull_request(green("pushed").head_sha("abc"));
    let url = format!("{}/repos/{REPO}/pulls/{number}/merge-async", stub.url());

    let http = reqwest::Client::new();
    let stale = http
        .put(&url)
        .json(&serde_json::json!({ "merge_method": "rebase", "sha": "old" }))
        .send()
        .await
        .unwrap();
    assert_eq!(stale.status(), 409);
    assert_eq!(
        stub.pull_request(REPO, number).unwrap().state,
        PrState::Open
    );

    let fresh = http
        .put(&url)
        .json(&serde_json::json!({ "merge_method": "rebase", "sha": "abc" }))
        .send()
        .await
        .unwrap();
    assert_eq!(fresh.status(), 200);
    let again = http
        .put(&url)
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(again.status(), 405);
}

// INFRA-009
#[tokio::test]
async fn a_merge_can_take_a_few_polls() {
    let stub = GithubStub::start();
    let number = stub.add_pull_request(green("slow"));
    stub.set_merge_polls(2);

    client(&stub).merge(&pr_ref(number), false).await.unwrap();

    let polls = stub.requests_to("GET", &format!("/repos/{REPO}/pulls/{number}/merge-async/"));
    assert_eq!(polls.len(), 3);
}

// INFRA-009
#[tokio::test]
async fn a_branch_that_is_gone_is_already_gone() {
    let stub = GithubStub::start();
    let number = stub.add_pull_request(green("auto deleted").head("feature/gone"));
    let http = reqwest::Client::new();
    let url = format!("{}/repos/{REPO}/git/refs/heads/feature/gone", stub.url());
    assert_eq!(http.delete(&url).send().await.unwrap().status(), 204);

    let outcome = client(&stub).merge(&pr_ref(number), false).await.unwrap();

    assert_eq!(outcome.branch, BranchDeletion::AlreadyGone);
}

// INFRA-009
#[tokio::test]
async fn a_missing_pull_request_is_not_found() {
    let stub = GithubStub::start();
    assert!(matches!(
        client(&stub).pull_request_status(&pr_ref(99)).await,
        Err(Error::PullRequestNotFound(_))
    ));
}

// INFRA-009
#[tokio::test]
async fn rerun_failed_starts_the_failed_jobs_of_finished_runs() {
    let stub = GithubStub::start();
    let number = stub.add_pull_request(
        green("flaky")
            .job(Job::new("test").workflow("Test").state(JobState::Failed))
            .job(Job::new("e2e").workflow("Test").state(JobState::Cancelled)),
    );
    let client = client(&stub);

    client.rerun_failed(&pr_ref(number), true).await.unwrap();

    let jobs = stub.pull_request(REPO, number).unwrap().jobs;
    let states: Vec<_> = jobs.iter().map(|j| (j.name.as_str(), j.state)).collect();
    assert_eq!(
        states,
        [
            ("build", JobState::Passed),
            ("lint", JobState::Passed),
            ("test", JobState::Running),
            ("e2e", JobState::Running),
        ]
    );
    assert_eq!(stub.requests_to("POST", "/repos").len(), 1);
    assert!(matches!(
        client.rerun_failed(&pr_ref(number), true).await,
        Err(Error::NothingToRerun)
    ));
}

// INFRA-009
#[tokio::test]
async fn rerun_failed_without_cancelled_reruns_job_by_job() {
    let stub = GithubStub::start();
    let number = stub.add_pull_request(
        PullRequest::new(REPO, "fail fast")
            .author("me")
            .job(Job::new("test").workflow("Test").state(JobState::Failed))
            .job(Job::new("e2e").workflow("Test").state(JobState::Cancelled)),
    );

    client(&stub)
        .rerun_failed(&pr_ref(number), false)
        .await
        .unwrap();

    let jobs = stub.pull_request(REPO, number).unwrap().jobs;
    assert_eq!(jobs[0].state, JobState::Running);
    assert_eq!(jobs[1].state, JobState::Cancelled);
    assert_eq!(
        stub.requests_to("POST", "/repos/iamsergio/rocketchat-qml/actions/jobs")
            .len(),
        1
    );
}

// INFRA-009
#[tokio::test]
async fn a_run_in_progress_cannot_be_rerun() {
    let stub = GithubStub::start();
    let number = stub.add_pull_request(
        PullRequest::new(REPO, "busy")
            .author("me")
            .job(Job::new("test").workflow("Test").state(JobState::Failed))
            .job(Job::new("slow").workflow("Test")),
    );
    let client = client(&stub);

    let error = client
        .rerun_failed(&pr_ref(number), true)
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Forbidden(_)), "{error}");
    let job = stub.pull_request(REPO, number).unwrap().jobs[1].id;
    assert!(matches!(
        client.rerun_job(REPO, job).await,
        Err(Error::Forbidden(_))
    ));
}

// INFRA-009
#[tokio::test]
async fn rerun_job_starts_that_job() {
    let stub = GithubStub::start();
    let number = stub.add_pull_request(green("one job"));
    stub.set_job_state(REPO, number, "lint", JobState::Failed);
    let jobs = client(&stub)
        .my_pull_requests()
        .await
        .unwrap()
        .remove(0)
        .jobs;
    let lint = jobs.iter().find(|j| j.name == "lint").unwrap();

    client(&stub)
        .rerun_job(REPO, lint.id.unwrap())
        .await
        .unwrap();

    let jobs = stub.pull_request(REPO, number).unwrap().jobs;
    assert_eq!(jobs[0].state, JobState::Passed);
    assert_eq!(jobs[1].state, JobState::Running);
    assert!(matches!(
        client(&stub).rerun_job(REPO, 1).await,
        Err(Error::Api { status: 404, .. })
    ));
}

// INFRA-009
#[tokio::test]
async fn approve_clears_the_wait_and_starts_the_jobs() {
    let stub = GithubStub::start();
    let waiting = stub.add_pull_request(
        PullRequest::new(REPO, "first time")
            .author("me")
            .needs_approval(),
    );
    let ready = stub.add_pull_request(green("approved already"));
    let client = client(&stub);

    let prs = client.my_pull_requests().await.unwrap();
    assert!(prs.iter().any(|p| p.number == waiting && p.needs_approval));
    assert_eq!(client.authorize(&pr_ref(ready)).await.unwrap(), 0);
    assert!(stub.requests_to("POST", "/repos").is_empty());

    assert_eq!(client.authorize(&pr_ref(waiting)).await.unwrap(), 1);

    let pr = stub.pull_request(REPO, waiting).unwrap();
    assert!(!pr.needs_approval);
    assert!(!pr.jobs.is_empty() && pr.jobs.iter().all(|j| j.state == JobState::Running));
    let prs = client.my_pull_requests().await.unwrap();
    let pr = prs.iter().find(|p| p.number == waiting).unwrap();
    assert!(!pr.needs_approval);
    assert_eq!(pr.ci, CiState::Pending);
    assert_eq!(client.authorize(&pr_ref(waiting)).await.unwrap(), 0);
}

// INFRA-009
#[tokio::test]
async fn rebase_makes_the_branch_clean_with_a_new_head_and_new_jobs() {
    let stub = GithubStub::start();
    let number = stub.add_pull_request(
        green("behind")
            .merge_state(MergeState::Behind)
            .head_sha("old"),
    );
    let client = client(&stub);

    client.rebase(&pr_ref(number)).await.unwrap();

    let pr = client.my_pull_requests().await.unwrap().remove(0);
    assert_eq!(pr.merge_state, MergeState::Clean);
    assert_ne!(pr.head_sha, "old");
    assert!(pr.jobs.iter().all(|j| j.state == JobState::Running));
    assert_eq!(
        stub.operations("rebase"),
        [format!("rebase {REPO}#{number}")]
    );
}

// INFRA-009
#[tokio::test]
async fn rebase_with_conflicts_is_refused() {
    let stub = GithubStub::start();
    let number = stub.add_pull_request(
        green("conflict")
            .merge_state(MergeState::Dirty)
            .head_sha("old"),
    );

    let error = client(&stub).rebase(&pr_ref(number)).await.unwrap_err();

    assert!(
        matches!(error, Error::GraphQl(ref m) if m.contains("conflict")),
        "{error}"
    );
    assert_eq!(stub.pull_request(REPO, number).unwrap().head_sha, "old");
}

// INFRA-009
#[tokio::test]
async fn runners_show_their_jobs() {
    let stub = GithubStub::start();
    let job = RunnerJob::new("build", "Build", "room-menu");
    stub.set_runners(
        REPO,
        vec![
            Runner::new("builder-1").label("gpu").running(job),
            Runner::new("builder-2").offline(),
            Runner::new("builder-3"),
        ],
    );

    let runners = client(&stub).runners(REPO).await.unwrap();

    assert_eq!(runners.len(), 3);
    assert!(runners[0].busy && runners[0].online);
    assert_eq!(runners[0].custom_labels, ["gpu"]);
    let job = runners[0].job.as_ref().unwrap();
    assert_eq!(
        (job.name.as_str(), job.workflow_name.as_str()),
        ("build", "Build")
    );
    assert_eq!(job.head_branch, "room-menu");
    assert_eq!(job.runner_name.as_deref(), Some("builder-1"));
    assert!(!runners[1].online && !runners[1].busy);
    assert!(runners[2].online && !runners[2].busy && runners[2].job.is_none());
    assert!(
        client(&stub)
            .runners("no/runners")
            .await
            .unwrap()
            .is_empty()
    );
}

// INFRA-009
#[tokio::test]
async fn planned_failures_apply_to_the_next_requests_only() {
    let stub = GithubStub::start();
    stub.add_pull_request(green("x"));
    let client = client(&stub);

    stub.fail_next("/graphql", 1, Failure::unauthorized());
    assert!(matches!(
        client.my_pull_requests().await,
        Err(Error::Unauthorized)
    ));
    assert_eq!(client.my_pull_requests().await.unwrap().len(), 1);

    stub.fail_next("/graphql", 2, Failure::server_error());
    for _ in 0..2 {
        assert!(matches!(
            client.viewer().await,
            Err(Error::Api { status: 500, .. })
        ));
    }
    client.viewer().await.unwrap();

    stub.fail_next("/graphql", 1, Failure::rate_limited());
    let error = client.viewer().await.unwrap_err();
    assert!(
        matches!(error, Error::RateLimited { reset: Some(_), .. }),
        "{error}"
    );

    stub.fail_next("/graphql", 1, Failure::new(403, "Resource not accessible"));
    assert!(matches!(client.viewer().await, Err(Error::Forbidden(_))));

    stub.fail_next(
        "/user/subscriptions",
        1,
        Failure::new(403, "x").header("retry-after", "30"),
    );
    assert!(matches!(
        client.watched_repos().await,
        Err(Error::RateLimited {
            retry_after: Some(_),
            ..
        })
    ));

    stub.fail_next("/graphql", 1, Failure::graphql("Something broke"));
    assert!(matches!(client.viewer().await, Err(Error::GraphQl(ref m)) if m == "Something broke"));
    stub.fail_next("/graphql", 1, Failure::graphql_rate_limited());
    assert!(matches!(
        client.viewer().await,
        Err(Error::RateLimited { .. })
    ));
    client.viewer().await.unwrap();
}

// INFRA-009
#[tokio::test]
async fn failures_can_target_a_graphql_operation() {
    let stub = GithubStub::start();
    let number = stub.add_pull_request(green("x").merge_state(MergeState::Behind));
    let client = client(&stub);
    stub.fail_next("rebase", 1, Failure::graphql("nope"));

    client.viewer().await.unwrap();
    assert!(matches!(
        client.rebase(&pr_ref(number)).await,
        Err(Error::GraphQl(_))
    ));
    // The id lookup before it went through; the mutation was the failure.
    assert_eq!(stub.operations("id").len(), 1);
    stub.clear_failures();
    client.rebase(&pr_ref(number)).await.unwrap();
}

// INFRA-009
#[tokio::test]
async fn the_log_records_what_was_asked_for() {
    let stub = GithubStub::start();
    let number = stub.add_pull_request(green("log me"));
    let client = client(&stub);

    client.viewer().await.unwrap();
    client.merge(&pr_ref(number), false).await.unwrap();

    let log: Vec<String> = stub
        .requests()
        .iter()
        .map(|r| {
            format!(
                "{} {} {}",
                r.method,
                r.path,
                r.operation.as_deref().unwrap_or("")
            )
        })
        .collect();
    assert_eq!(
        log,
        [
            "POST /graphql viewer".to_owned(),
            format!("POST /graphql status {REPO}#{number}"),
            format!("PUT /repos/{REPO}/pulls/{number}/merge-async "),
            format!("DELETE /repos/{REPO}/git/refs/heads/feature/{number} "),
        ]
    );
    stub.clear_requests();
    assert!(stub.requests().is_empty());
}
