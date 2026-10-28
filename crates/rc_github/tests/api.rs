// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The client against a mock HTTP server serving GitHub-shaped responses.

// Helpers outside #[test] fns may unwrap: a failure should just fail the test.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use jiff::civil::date;
use rc_github::{BranchDeletion, CiState, Error, GithubClient, JobState, PrRef, PrState};
use secrecy::SecretString;
use serde_json::{Value, json};
use wiremock::matchers::{
    body_json, body_partial_json, body_string_contains, header, method, path, query_param,
};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

const SEARCH_PAGE1: &str = include_str!("fixtures/search_page1.json");
const SEARCH_PAGE2: &str = include_str!("fixtures/search_page2.json");
const STATUS_OPEN_SUCCESS: &str = include_str!("fixtures/status_open_success.json");
const RUNS_FAILED: &str = include_str!("fixtures/runs_failed.json");
const RUNNERS: &str = include_str!("fixtures/runners.json");
const RUNS_IN_PROGRESS: &str = include_str!("fixtures/runs_in_progress.json");
const RUN_JOBS: &str = include_str!("fixtures/run_jobs.json");

const TOKEN: &str = "ghp_faketoken";

fn client(server: &MockServer) -> GithubClient {
    GithubClient::new(&server.uri(), SecretString::from(TOKEN))
        .unwrap()
        .with_merge_polling(Duration::from_millis(10), Duration::from_secs(5))
}

fn json_response(status: u16, body: &str) -> ResponseTemplate {
    ResponseTemplate::new(status)
        .insert_header("content-type", "application/json")
        .set_body_string(body)
}

fn pr() -> PrRef {
    PrRef {
        repo: "iamsergio/rocketchat-qml".to_owned(),
        number: 41,
    }
}

/// A GraphQL mock: the query text must contain `needle`.
async fn mount_graphql(server: &MockServer, needle: &str, response: ResponseTemplate) {
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(header("Authorization", format!("Bearer {TOKEN}").as_str()))
        .and(body_string_contains(needle))
        .respond_with(response)
        .mount(server)
        .await;
}

async fn mount(server: &MockServer, verb: &str, route: &str, response: ResponseTemplate) {
    Mock::given(method(verb))
        .and(path(route))
        .and(header("Authorization", format!("Bearer {TOKEN}").as_str()))
        .respond_with(response)
        .mount(server)
        .await;
}

/// The bodies of the requests the server got for `route`.
async fn bodies(server: &MockServer, route: &str) -> Vec<Value> {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path() == route)
        .map(|r| serde_json::from_slice(&r.body).unwrap_or(Value::Null))
        .collect()
}

// --- the search ---

async fn mount_search_pages(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_partial_json(json!({ "variables": { "after": null } })))
        .respond_with(json_response(200, SEARCH_PAGE1))
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/graphql"))
        .and(body_partial_json(
            json!({ "variables": { "after": "Y3Vyc29yOjE=" } }),
        ))
        .respond_with(json_response(200, SEARCH_PAGE2))
        .mount(server)
        .await;
}

// GH-001
#[tokio::test]
async fn my_pull_requests_pages_and_parses() {
    let server = MockServer::start().await;
    mount_search_pages(&server).await;

    let prs = client(&server).my_pull_requests().await.unwrap();

    let requests = bodies(&server, "/graphql").await;
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0]["variables"]["q"], "is:pr is:open author:@me");
    assert_eq!(requests[1]["variables"]["q"], "is:pr is:open author:@me");
    let query = requests[0]["query"].as_str().unwrap();
    for wanted in [
        "statusCheckRollup",
        "headRefOid",
        "baseRefName",
        "startedAt",
        "checkSuites",
    ] {
        assert!(query.contains(wanted), "{wanted}");
    }

    // Most recently updated first, across the pages.
    let numbers: Vec<u64> = prs.iter().map(|p| p.number).collect();
    assert_eq!(numbers, [7, 41, 39]);

    let draft = &prs[0];
    assert_eq!(draft.repo, "kdab/other");
    assert_eq!(draft.author, "ghost");
    assert!(draft.is_draft);
    assert_eq!(draft.base_branch, "develop");
    assert_eq!(draft.head_branch, "fix/typo");
    assert_eq!(draft.ci, CiState::None);
    assert!(draft.jobs.is_empty());
    assert!(draft.needs_approval);
    assert_eq!(draft.pr_ref().repo, "kdab/other");

    let first = &prs[1];
    assert_eq!(first.title, "Add the room menu");
    assert_eq!(first.author, "iamsergio");
    assert_eq!(
        first.url,
        "https://github.com/iamsergio/rocketchat-qml/pull/41"
    );
    assert_eq!(first.updated_at.to_string(), "2026-10-20T09:00:00Z");
    assert!(!first.is_draft);
    assert_eq!(first.head_branch, "room-menu");
    assert_eq!(first.base_branch, "main");
    assert_eq!(first.head_sha, "9f1c2d3e4a5b6c7d8e9f00112233445566778899");
    assert_eq!(first.ci, CiState::Failure);
    assert!(!first.needs_approval);

    assert_eq!(prs[2].ci, CiState::None);
}

// GH-001
#[tokio::test]
async fn jobs_carry_workflow_state_url_and_times() {
    let server = MockServer::start().await;
    mount_search_pages(&server).await;

    let prs = client(&server).my_pull_requests().await.unwrap();
    let jobs = &prs[1].jobs;

    let states: Vec<JobState> = jobs.iter().map(|j| j.state).collect();
    assert_eq!(
        states,
        [
            JobState::Passed,
            JobState::Failed,
            JobState::Cancelled,
            JobState::Running,
            JobState::Running
        ]
    );
    let build = &jobs[0];
    assert_eq!(build.id, Some(7001));
    assert_eq!(build.name, "build");
    assert_eq!(build.workflow_name.as_deref(), Some("Build"));
    assert_eq!(build.conclusion.as_deref(), Some("SUCCESS"));
    assert_eq!(
        build.url.as_deref(),
        Some("https://github.com/iamsergio/rocketchat-qml/actions/runs/500/job/7001")
    );
    assert_eq!(
        build.started_at.unwrap().to_string(),
        "2026-10-20T09:01:00Z"
    );
    assert_eq!(
        build.completed_at.unwrap().to_string(),
        "2026-10-20T09:05:30Z"
    );

    assert_eq!(jobs[1].conclusion.as_deref(), Some("TIMED_OUT"));
    assert_eq!(jobs[3].conclusion, None);
    assert_eq!(jobs[3].completed_at, None);
    assert_eq!(jobs[3].workflow_name.as_deref(), Some("Coverage"));

    // An external CI's commit status: no id, no workflow.
    let external = &jobs[4];
    assert_eq!(external.id, None);
    assert_eq!(external.name, "ci/external");
    assert_eq!(external.workflow_name, None);
    assert_eq!(external.url.as_deref(), Some("https://ci.example.com/42"));
}

// GH-002
#[tokio::test]
async fn watched_search_names_the_repos_and_the_date() {
    let server = MockServer::start().await;
    mount_graphql(&server, "search(query", json_response(200, SEARCH_PAGE2)).await;
    let repos = vec!["kdab/other".to_owned(), "o/r".to_owned()];

    let prs = client(&server)
        .watched_pull_requests(&repos, date(2026, 10, 7))
        .await
        .unwrap();

    assert_eq!(prs.len(), 1);
    let requests = bodies(&server, "/graphql").await;
    assert_eq!(
        requests[0]["variables"]["q"],
        "is:pr is:open -author:@me created:>=2026-10-07 repo:kdab/other repo:o/r"
    );
}

// GH-002
#[tokio::test]
async fn watched_search_without_repos_asks_nothing() {
    let server = MockServer::start().await;

    let prs = client(&server)
        .watched_pull_requests(&[], date(2026, 10, 7))
        .await
        .unwrap();

    assert!(prs.is_empty());
    assert!(server.received_requests().await.unwrap().is_empty());
}

// GH-002
#[tokio::test]
async fn watched_repos_follow_the_pages() {
    let server = MockServer::start().await;
    let full: Vec<Value> = (0..100)
        .map(|i| json!({ "full_name": format!("o/r{i}") }))
        .collect();
    Mock::given(method("GET"))
        .and(path("/user/subscriptions"))
        .and(query_param("per_page", "100"))
        .and(query_param("page", "1"))
        .and(header("Authorization", format!("Bearer {TOKEN}").as_str()))
        .respond_with(json_response(200, &Value::Array(full).to_string()))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/user/subscriptions"))
        .and(query_param("page", "2"))
        .respond_with(json_response(200, r#"[{"full_name":"last/one","id":1}]"#))
        .mount(&server)
        .await;

    let repos = client(&server).watched_repos().await.unwrap();

    assert_eq!(repos.len(), 101);
    assert_eq!(repos[0], "o/r0");
    assert_eq!(repos[100], "last/one");
}

// GH-001
#[tokio::test]
async fn requests_say_who_they_are() {
    let server = MockServer::start().await;
    mount(
        &server,
        "GET",
        "/user/subscriptions",
        json_response(200, "[]"),
    )
    .await;

    client(&server).watched_repos().await.unwrap();

    let requests = server.received_requests().await.unwrap();
    let request: &Request = &requests[0];
    let value = |name: &str| {
        request
            .headers
            .get(name)
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned()
    };
    assert_eq!(value("accept"), "application/vnd.github+json");
    assert_eq!(value("x-github-api-version"), "2022-11-28");
    assert!(!value("user-agent").is_empty());
}

// --- one pull request ---

// GH-003
#[tokio::test]
async fn status_of_a_pull_request() {
    let server = MockServer::start().await;
    mount_graphql(
        &server,
        "headRepository",
        json_response(200, STATUS_OPEN_SUCCESS),
    )
    .await;

    let status = client(&server).pull_request_status(&pr()).await.unwrap();

    assert_eq!(status.state, PrState::Open);
    assert_eq!(status.ci, CiState::Success);
    assert_eq!(status.head_branch, "feature/room-menu");
    assert_eq!(
        status.head_repo.as_deref(),
        Some("iamsergio/rocketchat-qml")
    );
    assert_eq!(
        bodies(&server, "/graphql").await[0]["variables"],
        json!({ "owner": "iamsergio", "name": "rocketchat-qml", "number": 41 })
    );
}

// GH-003
#[tokio::test]
async fn status_of_a_missing_pull_request() {
    let server = MockServer::start().await;
    mount_graphql(
        &server,
        "headRepository",
        json_response(200, r#"{"data":{"repository":{"pullRequest":null}}}"#),
    )
    .await;

    let err = client(&server)
        .pull_request_status(&pr())
        .await
        .unwrap_err();

    assert!(
        matches!(err, Error::PullRequestNotFound(ref what) if what.ends_with("#41")),
        "{err}"
    );
}

// --- merge ---

const HEAD: &str = "9f1c2d3e4a5b6c7d8e9f00112233445566778899";
const MERGE_ASYNC: &str = "/repos/iamsergio/rocketchat-qml/pulls/41/merge-async";
const BRANCH_REF: &str = "/repos/iamsergio/rocketchat-qml/git/refs/heads/feature/room-menu";

async fn mount_status(server: &MockServer, state: &str, rollup: Option<&str>) {
    let mut body: Value = serde_json::from_str(STATUS_OPEN_SUCCESS).unwrap();
    let pull = &mut body["data"]["repository"]["pullRequest"];
    pull["state"] = json!(state);
    pull["commits"]["nodes"][0]["commit"]["statusCheckRollup"] =
        rollup.map_or(Value::Null, |s| json!({ "state": s }));
    mount_graphql(
        server,
        "headRepository",
        json_response(200, &body.to_string()),
    )
    .await;
}

fn merged() -> ResponseTemplate {
    json_response(200, r#"{"status":"merged","details":{}}"#)
}

// GH-003
#[tokio::test]
async fn merge_rebases_polls_and_deletes_the_branch() {
    let server = MockServer::start().await;
    mount_status(&server, "OPEN", Some("SUCCESS")).await;
    Mock::given(method("PUT"))
        .and(path(MERGE_ASYNC))
        .and(header("Authorization", format!("Bearer {TOKEN}").as_str()))
        .and(body_json(json!({
            "merge_action": "direct_merge",
            "merge_method": "rebase",
            "sha": HEAD,
        })))
        .respond_with(json_response(
            200,
            r#"{"status":"pending","details":{"uuid":"abc-123"}}"#,
        ))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("{MERGE_ASYNC}/abc-123")))
        .respond_with(merged())
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path(BRANCH_REF))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    let outcome = client(&server).merge(&pr(), false).await.unwrap();

    assert_eq!(outcome.branch, BranchDeletion::Deleted);
}

// GH-003
#[tokio::test]
async fn merge_that_is_done_at_once_needs_no_polling() {
    let server = MockServer::start().await;
    mount_status(&server, "OPEN", Some("SUCCESS")).await;
    mount(&server, "PUT", MERGE_ASYNC, merged()).await;
    mount(&server, "DELETE", BRANCH_REF, ResponseTemplate::new(204)).await;

    client(&server).merge(&pr(), false).await.unwrap();

    let gets = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.method.as_str() == "GET")
        .count();
    assert_eq!(gets, 0);
}

// GH-003
#[tokio::test]
async fn merge_refuses_what_has_not_passed() {
    for (rollup, expected) in [
        (Some("PENDING"), CiState::Pending),
        (Some("FAILURE"), CiState::Failure),
        (None, CiState::None),
    ] {
        let server = MockServer::start().await;
        mount_status(&server, "OPEN", rollup).await;

        let err = client(&server).merge(&pr(), false).await.unwrap_err();

        assert!(
            matches!(err, Error::ChecksNotPassed(state) if state == expected),
            "{err}"
        );
        // Nothing but the status was asked.
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }
}

// GH-003
#[tokio::test]
async fn force_merges_without_looking_at_the_checks() {
    let server = MockServer::start().await;
    mount_status(&server, "OPEN", Some("FAILURE")).await;
    mount(&server, "PUT", MERGE_ASYNC, merged()).await;
    mount(&server, "DELETE", BRANCH_REF, ResponseTemplate::new(204)).await;

    client(&server).merge(&pr(), true).await.unwrap();
}

// GH-003
#[tokio::test]
async fn merge_needs_an_open_pull_request_even_when_forced() {
    for (state, word) in [("CLOSED", "closed"), ("MERGED", "merged")] {
        let server = MockServer::start().await;
        mount_status(&server, state, Some("SUCCESS")).await;

        let err = client(&server).merge(&pr(), true).await.unwrap_err();

        assert!(matches!(err, Error::NotOpen(ref s) if s == word), "{err}");
    }
}

// GH-003
#[tokio::test]
async fn merge_that_fails_says_why() {
    let server = MockServer::start().await;
    mount_status(&server, "OPEN", Some("SUCCESS")).await;
    mount(
        &server,
        "PUT",
        MERGE_ASYNC,
        json_response(
            200,
            r#"{"status":"failed","details":{"message":"Merge conflict"}}"#,
        ),
    )
    .await;

    let err = client(&server).merge(&pr(), false).await.unwrap_err();

    assert!(
        matches!(err, Error::MergeFailed { ref status, ref message }
            if status == "failed" && message == "Merge conflict"),
        "{err}"
    );
}

// GH-003
#[tokio::test]
async fn merge_refused_because_the_head_moved() {
    let server = MockServer::start().await;
    mount_status(&server, "OPEN", Some("SUCCESS")).await;
    mount(
        &server,
        "PUT",
        MERGE_ASYNC,
        json_response(
            409,
            r#"{"message":"Head branch was modified. Review and try the merge again."}"#,
        ),
    )
    .await;

    let err = client(&server).merge(&pr(), false).await.unwrap_err();

    assert!(
        matches!(err, Error::Api { status: 409, ref message } if message.contains("modified")),
        "{err}"
    );
}

// GH-003
#[tokio::test]
async fn merge_that_stays_pending_times_out() {
    let server = MockServer::start().await;
    mount_status(&server, "OPEN", Some("SUCCESS")).await;
    let pending = json_response(200, r#"{"status":"pending","details":{"uuid":"u1"}}"#);
    mount(&server, "PUT", MERGE_ASYNC, pending.clone()).await;
    mount(&server, "GET", &format!("{MERGE_ASYNC}/u1"), pending).await;
    let client =
        client(&server).with_merge_polling(Duration::from_millis(5), Duration::from_millis(60));

    let err = client.merge(&pr(), false).await.unwrap_err();

    assert!(matches!(err, Error::MergeTimeout), "{err}");
}

// GH-003
#[tokio::test]
async fn merge_polling_survives_a_server_error() {
    let server = MockServer::start().await;
    mount_status(&server, "OPEN", Some("SUCCESS")).await;
    mount(
        &server,
        "PUT",
        MERGE_ASYNC,
        json_response(200, r#"{"status":"pending","details":{"uuid":"u1"}}"#),
    )
    .await;
    Mock::given(method("GET"))
        .and(path(format!("{MERGE_ASYNC}/u1")))
        .respond_with(json_response(502, "{}"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    mount(&server, "GET", &format!("{MERGE_ASYNC}/u1"), merged()).await;
    mount(&server, "DELETE", BRANCH_REF, ResponseTemplate::new(204)).await;

    client(&server).merge(&pr(), false).await.unwrap();
}

// GH-003
#[tokio::test]
async fn branch_deleted_by_the_repository_already() {
    let server = MockServer::start().await;
    mount_status(&server, "OPEN", Some("SUCCESS")).await;
    mount(&server, "PUT", MERGE_ASYNC, merged()).await;
    mount(
        &server,
        "DELETE",
        BRANCH_REF,
        json_response(422, r#"{"message":"Reference does not exist"}"#),
    )
    .await;

    let outcome = client(&server).merge(&pr(), false).await.unwrap();

    assert_eq!(outcome.branch, BranchDeletion::AlreadyGone);
}

// GH-003
#[tokio::test]
async fn a_branch_that_cannot_be_deleted_does_not_fail_the_merge() {
    let server = MockServer::start().await;
    mount_status(&server, "OPEN", Some("SUCCESS")).await;
    mount(&server, "PUT", MERGE_ASYNC, merged()).await;
    mount(
        &server,
        "DELETE",
        BRANCH_REF,
        json_response(
            403,
            r#"{"message":"Resource not accessible by personal access token"}"#,
        ),
    )
    .await;

    let outcome = client(&server).merge(&pr(), false).await.unwrap();

    assert!(
        matches!(outcome.branch, BranchDeletion::Failed(ref m) if m.contains("not allowed")),
        "{outcome:?}"
    );
}

// GH-003
#[tokio::test]
async fn no_head_repository_means_no_branch_to_delete() {
    let server = MockServer::start().await;
    let mut body: Value = serde_json::from_str(STATUS_OPEN_SUCCESS).unwrap();
    body["data"]["repository"]["pullRequest"]["headRepository"] = Value::Null;
    mount_graphql(
        &server,
        "headRepository",
        json_response(200, &body.to_string()),
    )
    .await;
    mount(&server, "PUT", MERGE_ASYNC, merged()).await;

    let outcome = client(&server).merge(&pr(), false).await.unwrap();

    assert_eq!(outcome.branch, BranchDeletion::Skipped);
}

// --- update the branch ---

// GH-004
#[tokio::test]
async fn rebase_sends_the_mutation_with_the_rebase_method() {
    let server = MockServer::start().await;
    mount_graphql(
        &server,
        "updatePullRequestBranch",
        json_response(
            200,
            r#"{"data":{"updatePullRequestBranch":{"pullRequest":{"id":"PR_kwDOabc"}}}}"#,
        ),
    )
    .await;
    mount_graphql(
        &server,
        "pullRequest(number: $number) { id }",
        json_response(
            200,
            r#"{"data":{"repository":{"pullRequest":{"id":"PR_kwDOabc"}}}}"#,
        ),
    )
    .await;

    client(&server).rebase(&pr()).await.unwrap();

    let requests = bodies(&server, "/graphql").await;
    assert_eq!(requests.len(), 2);
    let mutation = requests[1]["query"].as_str().unwrap();
    assert!(mutation.contains("updateMethod: REBASE"));
    assert_eq!(requests[1]["variables"], json!({ "id": "PR_kwDOabc" }));
}

// GH-004
#[tokio::test]
async fn rebase_refused_by_github() {
    let server = MockServer::start().await;
    mount_graphql(
        &server,
        "pullRequest(number: $number) { id }",
        json_response(
            200,
            r#"{"data":{"repository":{"pullRequest":{"id":"PR_1"}}}}"#,
        ),
    )
    .await;
    mount_graphql(
        &server,
        "updatePullRequestBranch",
        json_response(
            200,
            r#"{"data":null,"errors":[{"type":"UNPROCESSABLE","message":"Cannot update PR branch due to conflicts"}]}"#,
        ),
    )
    .await;

    let err = client(&server).rebase(&pr()).await.unwrap_err();

    assert!(
        matches!(err, Error::GraphQl(ref m) if m.contains("conflicts")),
        "{err}"
    );
}

// --- rerun ---

const RUNS: &str = "/repos/iamsergio/rocketchat-qml/actions/runs";
const JOBS: &str = "/repos/iamsergio/rocketchat-qml/actions/jobs";

async fn mount_runs_failed(server: &MockServer) {
    mount_graphql(server, "contexts(first", json_response(200, RUNS_FAILED)).await;
}

async fn expect_post(server: &MockServer, route: &str) {
    Mock::given(method("POST"))
        .and(path(route))
        .and(header("Authorization", format!("Bearer {TOKEN}").as_str()))
        .respond_with(json_response(201, "{}"))
        .expect(1)
        .mount(server)
        .await;
}

// GH-005
#[tokio::test]
async fn rerun_failed_leaves_cancelled_jobs_alone() {
    let server = MockServer::start().await;
    mount_runs_failed(&server).await;
    // Run 100 has only a failed job; run 200 also a cancelled one, so its failed job is rerun
    // alone; run 300 has only a cancelled one.
    expect_post(&server, &format!("{RUNS}/100/rerun-failed-jobs")).await;
    expect_post(&server, &format!("{JOBS}/21/rerun")).await;

    client(&server).rerun_failed(&pr(), false).await.unwrap();

    // The mocks check their counts when the server drops; count here too for a clear failure.
    let posts = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.method.as_str() == "POST" && r.url.path() != "/graphql")
        .count();
    assert_eq!(posts, 2);
}

// GH-005
#[tokio::test]
async fn rerun_failed_with_cancelled_reruns_every_run() {
    let server = MockServer::start().await;
    mount_runs_failed(&server).await;
    for run in [100, 200, 300] {
        expect_post(&server, &format!("{RUNS}/{run}/rerun-failed-jobs")).await;
    }

    client(&server).rerun_failed(&pr(), true).await.unwrap();
}

// GH-005
#[tokio::test]
async fn rerun_failed_with_nothing_failed() {
    let server = MockServer::start().await;
    let body = json!({"data":{"repository":{"pullRequest":{"commits":{"nodes":[
        {"commit":{"statusCheckRollup":null}}]}}}}});
    mount_graphql(
        &server,
        "contexts(first",
        json_response(200, &body.to_string()),
    )
    .await;

    let err = client(&server)
        .rerun_failed(&pr(), false)
        .await
        .unwrap_err();

    assert!(matches!(err, Error::NothingToRerun), "{err}");
}

// GH-005
#[tokio::test]
async fn rerun_failed_keeps_going_when_one_request_fails() {
    let server = MockServer::start().await;
    mount_runs_failed(&server).await;
    mount(
        &server,
        "POST",
        &format!("{RUNS}/100/rerun-failed-jobs"),
        json_response(403, r#"{"message":"Workflow run is still in progress"}"#),
    )
    .await;
    expect_post(&server, &format!("{JOBS}/21/rerun")).await;

    let err = client(&server)
        .rerun_failed(&pr(), false)
        .await
        .unwrap_err();

    match err {
        Error::Partial {
            failed: 1,
            total: 2,
            first,
        } => {
            assert!(matches!(*first, Error::Forbidden(ref m) if m.contains("in progress")));
        }
        other => panic!("{other}"),
    }
}

// GH-005
#[tokio::test]
async fn rerun_failed_with_one_request_failing_reports_that_error() {
    let server = MockServer::start().await;
    let body = json!({"data":{"repository":{"pullRequest":{"commits":{"nodes":[{"commit":{
        "statusCheckRollup":{"state":"FAILURE","contexts":{"nodes":[
        {"__typename":"CheckRun","databaseId":5,"name":"a","status":"COMPLETED","conclusion":"FAILURE",
         "detailsUrl":null,"startedAt":null,"completedAt":null,
         "checkSuite":{"workflowRun":{"databaseId":50,"workflow":{"name":"W"}}}}]}}}}]}}}}});
    mount_graphql(
        &server,
        "contexts(first",
        json_response(200, &body.to_string()),
    )
    .await;
    mount(
        &server,
        "POST",
        &format!("{RUNS}/50/rerun-failed-jobs"),
        json_response(404, r#"{"message":"Not Found"}"#),
    )
    .await;

    let err = client(&server)
        .rerun_failed(&pr(), false)
        .await
        .unwrap_err();

    assert!(matches!(err, Error::Api { status: 404, .. }), "{err}");
}

// GH-005
#[tokio::test]
async fn rerun_one_job() {
    let server = MockServer::start().await;
    expect_post(&server, &format!("{JOBS}/7002/rerun")).await;

    client(&server)
        .rerun_job("iamsergio/rocketchat-qml", 7002)
        .await
        .unwrap();
}

// GH-005
#[tokio::test]
async fn rerun_job_of_a_malformed_repo() {
    let server = MockServer::start().await;

    let err = client(&server).rerun_job("nonsense", 1).await.unwrap_err();

    assert!(matches!(err, Error::InvalidUrl(_)), "{err}");
}

// --- approve ---

fn approval_body(conclusions: &[(&str, u64)]) -> String {
    let suites: Vec<Value> = conclusions
        .iter()
        .map(|(c, id)| json!({ "conclusion": c, "workflowRun": { "databaseId": id } }))
        .collect();
    json!({"data":{"repository":{"pullRequest":{"commits":{"nodes":[
        {"commit":{"checkSuites":{"nodes":suites}}}]}}}}})
    .to_string()
}

// GH-006
#[tokio::test]
async fn authorize_approves_the_runs_waiting() {
    let server = MockServer::start().await;
    mount_graphql(
        &server,
        "checkSuites(first",
        json_response(
            200,
            &approval_body(&[
                ("ACTION_REQUIRED", 900),
                ("SUCCESS", 901),
                ("ACTION_REQUIRED", 902),
            ]),
        ),
    )
    .await;
    expect_post(&server, &format!("{RUNS}/900/approve")).await;
    expect_post(&server, &format!("{RUNS}/902/approve")).await;

    let approved = client(&server).authorize(&pr()).await.unwrap();

    assert_eq!(approved, 2);
}

// GH-006
#[tokio::test]
async fn authorize_with_nothing_waiting_posts_nothing() {
    let server = MockServer::start().await;
    mount_graphql(
        &server,
        "checkSuites(first",
        json_response(200, &approval_body(&[("SUCCESS", 901)])),
    )
    .await;

    let approved = client(&server).authorize(&pr()).await.unwrap();

    assert_eq!(approved, 0);
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

// GH-006
#[tokio::test]
async fn authorize_keeps_going_past_a_run_it_cannot_approve() {
    let server = MockServer::start().await;
    mount_graphql(
        &server,
        "checkSuites(first",
        json_response(
            200,
            &approval_body(&[("ACTION_REQUIRED", 900), ("ACTION_REQUIRED", 902)]),
        ),
    )
    .await;
    mount(
        &server,
        "POST",
        &format!("{RUNS}/900/approve"),
        json_response(404, r#"{"message":"Not Found"}"#),
    )
    .await;
    expect_post(&server, &format!("{RUNS}/902/approve")).await;

    let err = client(&server).authorize(&pr()).await.unwrap_err();

    assert!(
        matches!(
            err,
            Error::Partial {
                failed: 1,
                total: 2,
                ..
            }
        ),
        "{err}"
    );
}

// --- runners ---

const RUNNERS_PATH: &str = "/repos/iamsergio/rocketchat-qml/actions/runners";

// GH-007
#[tokio::test]
async fn runners_with_the_job_of_a_busy_one() {
    let server = MockServer::start().await;
    mount(&server, "GET", RUNNERS_PATH, json_response(200, RUNNERS)).await;
    Mock::given(method("GET"))
        .and(path(RUNS))
        .and(query_param("status", "in_progress"))
        .and(query_param("per_page", "100"))
        .respond_with(json_response(200, RUNS_IN_PROGRESS))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("{RUNS}/600/jobs")))
        .and(query_param("filter", "latest"))
        .respond_with(json_response(200, RUN_JOBS))
        .mount(&server)
        .await;

    let runners = client(&server)
        .runners("iamsergio/rocketchat-qml")
        .await
        .unwrap();

    assert_eq!(runners.len(), 2);
    let busy = &runners[0];
    assert_eq!(busy.name, "builder-1");
    assert_eq!(busy.os, "linux");
    assert!(busy.online && busy.busy);
    assert_eq!(busy.custom_labels, ["gpu"]);
    let job = busy.job.as_ref().unwrap();
    assert_eq!(job.name, "build");
    assert_eq!(job.workflow_name, "Build");
    assert_eq!(job.head_branch, "room-menu");
    assert_eq!(job.started_at.to_string(), "2026-10-28T08:00:00Z");
    assert_eq!(job.url, "https://github.com/o/r/actions/runs/600/job/8001");

    let idle = &runners[1];
    assert!(!idle.online && !idle.busy);
    assert!(idle.custom_labels.is_empty());
    assert!(idle.job.is_none());
}

// GH-007
#[tokio::test]
async fn idle_runners_ask_for_no_jobs() {
    let server = MockServer::start().await;
    let body = json!({"total_count":1,"runners":[
        {"id":1,"name":"r","os":"linux","status":"online","busy":false,"labels":[]}]});
    mount(
        &server,
        "GET",
        RUNNERS_PATH,
        json_response(200, &body.to_string()),
    )
    .await;

    let runners = client(&server)
        .runners("iamsergio/rocketchat-qml")
        .await
        .unwrap();

    assert_eq!(runners.len(), 1);
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

// GH-007
#[tokio::test]
async fn running_jobs_are_only_the_in_progress_ones() {
    let server = MockServer::start().await;
    mount(&server, "GET", RUNS, json_response(200, RUNS_IN_PROGRESS)).await;
    mount(
        &server,
        "GET",
        &format!("{RUNS}/600/jobs"),
        json_response(200, RUN_JOBS),
    )
    .await;

    let jobs = client(&server)
        .running_jobs("iamsergio/rocketchat-qml")
        .await
        .unwrap();

    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].runner_name.as_deref(), Some("builder-1"));
}

// GH-007
#[tokio::test]
async fn runners_need_admin_rights() {
    let server = MockServer::start().await;
    mount(
        &server,
        "GET",
        RUNNERS_PATH,
        json_response(
            403,
            r#"{"message":"Must have admin rights to Repository."}"#,
        ),
    )
    .await;

    let err = client(&server)
        .runners("iamsergio/rocketchat-qml")
        .await
        .unwrap_err();

    assert!(
        matches!(err, Error::Forbidden(ref m) if m.contains("admin")),
        "{err}"
    );
}

// --- errors ---

fn rate_limited(headers: &[(&str, &str)]) -> ResponseTemplate {
    headers.iter().fold(
        json_response(403, r#"{"message":"API rate limit exceeded"}"#),
        |response, (name, value)| response.insert_header(*name, *value),
    )
}

// GH-001
#[tokio::test]
async fn a_refused_token() {
    let server = MockServer::start().await;
    mount(
        &server,
        "GET",
        "/user/subscriptions",
        json_response(401, r#"{"message":"Bad credentials"}"#),
    )
    .await;

    let err = client(&server).watched_repos().await.unwrap_err();

    assert!(matches!(err, Error::Unauthorized), "{err}");
}

// GH-001
#[tokio::test]
async fn a_refused_token_on_graphql() {
    let server = MockServer::start().await;
    mount(
        &server,
        "POST",
        "/graphql",
        json_response(401, r#"{"message":"Bad credentials"}"#),
    )
    .await;

    let err = client(&server).my_pull_requests().await.unwrap_err();

    assert!(matches!(err, Error::Unauthorized), "{err}");
}

// GH-001
#[tokio::test]
async fn the_primary_rate_limit() {
    let server = MockServer::start().await;
    mount(
        &server,
        "GET",
        "/user/subscriptions",
        rate_limited(&[
            ("x-ratelimit-remaining", "0"),
            ("x-ratelimit-reset", "1793174400"),
        ]),
    )
    .await;

    let err = client(&server).watched_repos().await.unwrap_err();

    match err {
        Error::RateLimited {
            reset: Some(reset),
            retry_after: None,
        } => {
            assert_eq!(reset.as_second(), 1_793_174_400);
        }
        other => panic!("{other}"),
    }
}

// GH-001
#[tokio::test]
async fn the_secondary_rate_limit() {
    let server = MockServer::start().await;
    mount(
        &server,
        "GET",
        "/user/subscriptions",
        rate_limited(&[("retry-after", "60")]),
    )
    .await;

    let err = client(&server).watched_repos().await.unwrap_err();

    assert!(
        matches!(err, Error::RateLimited { reset: None, retry_after: Some(d) } if d == Duration::from_secs(60)),
        "{err}"
    );
}

// GH-001
#[tokio::test]
async fn too_many_requests() {
    let server = MockServer::start().await;
    mount(
        &server,
        "GET",
        "/user/subscriptions",
        ResponseTemplate::new(429),
    )
    .await;

    let err = client(&server).watched_repos().await.unwrap_err();

    assert!(matches!(err, Error::RateLimited { .. }), "{err}");
}

// GH-001
#[tokio::test]
async fn a_forbidden_that_is_no_rate_limit() {
    let server = MockServer::start().await;
    mount(
        &server,
        "GET",
        "/user/subscriptions",
        json_response(
            403,
            r#"{"message":"Resource not accessible by integration"}"#,
        )
        .insert_header("x-ratelimit-remaining", "4999"),
    )
    .await;

    let err = client(&server).watched_repos().await.unwrap_err();

    assert!(
        matches!(err, Error::Forbidden(ref m) if m.contains("not accessible")),
        "{err}"
    );
}

// GH-001
#[tokio::test]
async fn graphql_errors_become_one_error() {
    let server = MockServer::start().await;
    mount(
        &server,
        "POST",
        "/graphql",
        json_response(
            200,
            r#"{"errors":[{"type":"INVALID","message":"Parse error"},{"message":"Second"}]}"#,
        ),
    )
    .await;

    let err = client(&server).my_pull_requests().await.unwrap_err();

    assert!(
        matches!(err, Error::GraphQl(ref m) if m == "Parse error; Second"),
        "{err}"
    );
}

// GH-001
#[tokio::test]
async fn graphql_rate_limit() {
    let server = MockServer::start().await;
    mount(
        &server,
        "POST",
        "/graphql",
        json_response(
            200,
            r#"{"errors":[{"type":"RATE_LIMITED","message":"API rate limit exceeded for user"}]}"#,
        ),
    )
    .await;

    let err = client(&server).my_pull_requests().await.unwrap_err();

    assert!(matches!(err, Error::RateLimited { .. }), "{err}");
}

// GH-001
#[tokio::test]
async fn a_server_error() {
    let server = MockServer::start().await;
    mount(&server, "POST", "/graphql", ResponseTemplate::new(502)).await;

    let err = client(&server).my_pull_requests().await.unwrap_err();

    assert!(
        matches!(err, Error::Api { status: 502, ref message } if message == "Bad Gateway"),
        "{err}"
    );
}

// GH-001
#[tokio::test]
async fn an_answer_that_is_not_what_was_asked() {
    let server = MockServer::start().await;
    mount(
        &server,
        "POST",
        "/graphql",
        json_response(200, r#"{"data":{"nothing":1}}"#),
    )
    .await;

    let err = client(&server).my_pull_requests().await.unwrap_err();

    assert!(matches!(err, Error::Decode(_)), "{err}");
}

// GH-001
#[tokio::test]
async fn an_unreachable_server() {
    // A port nothing listens on: one that was just free.
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let url = format!("http://127.0.0.1:{port}");
    let client = GithubClient::new(&url, SecretString::from(TOKEN)).unwrap();

    let err = client.my_pull_requests().await.unwrap_err();

    assert!(matches!(err, Error::Unreachable(_)), "{err}");
}

// GH-001
#[test]
fn an_invalid_api_url() {
    let err = GithubClient::new("not a url", SecretString::from(TOKEN))
        .err()
        .unwrap();
    assert!(matches!(err, Error::InvalidUrl(_)), "{err}");
}

// GH-001
#[tokio::test]
async fn the_api_url_may_have_a_prefix() {
    let server = MockServer::start().await;
    mount(
        &server,
        "GET",
        "/api/v3/user/subscriptions",
        json_response(200, "[]"),
    )
    .await;
    let client = GithubClient::new(
        &format!("{}/api/v3", server.uri()),
        SecretString::from(TOKEN),
    )
    .unwrap();

    assert!(client.watched_repos().await.unwrap().is_empty());
}
