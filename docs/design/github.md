# GitHub pull requests

The GitHub page shows the user's open pull requests and their CI, and merges, updates, reruns and approves them: what the `ghw` command line tool does in a terminal. This document is the protocol part, `rc_github`, the GitHub analogue of `rc_api`. The actor (`rc_core::github`), the views and the pages come in later steps and extend it. Feature IDs: GH-001 to GH-008 in [TRACEABILITY.md](../../TRACEABILITY.md).

## The crate

`rc_github` is a typed async client over `reqwest` (rustls). No `gh` process and no GitHub crate. It keeps no state; the serde wire types (`wire.rs`) are private and everything it returns is a plain type from `types.rs`, which `rc_core` maps later.

`GithubClient::new(api_url, token)` takes the API's base URL (`https://api.github.com` by default; the GUI tests point it at a local stub) and the token as a `SecretString`, sent as `Authorization: Bearer`. REST paths are relative to the base URL and GraphQL is `POST <base>/graphql`. That is right for github.com and for a stub, not for GitHub Enterprise, whose GraphQL is at `/api/graphql` beside its `/api/v3` REST (not needed yet). Every request carries `Accept: application/vnd.github+json`, `X-GitHub-Api-Version` and a `User-Agent` (GitHub refuses requests without one).

A pull request is named by `PrRef` (`owner/name` and number), not by URL: the queries take repository and number variables, so they don't depend on the host. `PrRef::from_url` reads the URLs the GUI has.

## Calls

| Call | What it asks |
|---|---|
| `my_pull_requests` | GraphQL `search(query: "is:pr is:open author:@me", type: ISSUE)` |
| `watched_repos` | `GET /user/subscriptions` |
| `watched_pull_requests(repos, since)` | the same search for `is:pr is:open -author:@me created:>=<since> repo:a repo:b ...`; no repository, no request, because a search without `repo:` covers all of GitHub |
| `search_pull_requests(query, limit)` | what both searches are |
| `pull_request_status` | GraphQL: state, head branch and sha, head repository, rollup state |
| `rerun_failed(pr, include_cancelled)` | GraphQL for the failed jobs, then `POST /repos/{r}/actions/runs/{id}/rerun-failed-jobs` or `.../actions/jobs/{id}/rerun` |
| `rerun_job(repo, job_id)` | `POST /repos/{r}/actions/jobs/{id}/rerun` |
| `authorize(pr)` | GraphQL for the check suites concluded `ACTION_REQUIRED`, then `POST /repos/{r}/actions/runs/{id}/approve` for each |
| `rebase(pr)` | GraphQL: the pull request's node id, then the `updatePullRequestBranch` mutation |
| `merge(pr, force)` | status, then `PUT /repos/{r}/pulls/{n}/merge-async`, polling, `DELETE /repos/{r}/git/refs/heads/{branch}` |
| `runners(repo)`, `running_jobs(repo)` | `GET /repos/{r}/actions/runners`, `.../actions/runs?status=in_progress`, `.../actions/runs/{id}/jobs?filter=latest` |

### Why GraphQL

The check rollup (the state of a commit's checks, and each job with its workflow, state, URL and times) is only in GraphQL: `gh search prs --json` can't return it, and REST needs a request per pull request and per check suite. The search works across all repositories, unlike `gh pr list`. The queries are ghw's, with the variables passed as variables instead of pasted into the text, plus `number`, `headRefName`, `baseRefName`, `headRefOid` and the jobs' `startedAt` and `completedAt` for the page. `statusCheckRollup.contexts` lists the jobs of the head commit: check runs (Actions jobs, whose `databaseId` is the job id) and status contexts (an external CI, without an id). `checkSuites` is asked as well because a run waiting for a maintainer's approval has no jobs yet, so the rollup is null and it only shows as a suite concluded `ACTION_REQUIRED` (`PullRequest::needs_approval`). The watched repositories are listed through REST because `viewer.watching` is twice as slow, and fetching each repository's pull requests through GraphQL times out (504).

A job is `Passed` (success, neutral, skipped, stale), `Failed`, `Cancelled` or `Running` (anything not completed). Cancelled is apart because it is usually a sibling that fail-fast stopped after another job failed. Only the first 100 jobs of a commit and 50 of its suites are read.

The search is paged with `pageInfo` and a cursor (100 a page; GitHub returns at most 1000 results). The result is sorted most recently updated first. REST lists are paged with `per_page=100&page=N`; a page of fewer than 100 items is the last.

### Update the branch

`gh pr update-branch --rebase` is the GraphQL `updatePullRequestBranch` mutation with `updateMethod: REBASE`. REST's `PUT /pulls/{n}/update-branch` can only merge, so the mutation is used. It takes the pull request's node id, which a query gives first. GitHub does the rebase in the background: the new head shows in the next search.

### Merge

`merge` is ghw's `merge_pr` without its waiting: the pull request must be open, and unless `force` its rollup must be `SUCCESS` (no checks and pending are refusals too, with the state in `Error::ChecksNotPassed`). Waiting for the checks, and telling the user when they end (GH-008), is the core's, which polls `pull_request_status`.

The merge is the asynchronous REST endpoint, because GraphQL's `mergePullRequest` refuses pull requests in a stack. The body is `merge_action=direct_merge`, `merge_method=rebase` and `sha`, the head commit the checks were read at, so that GitHub refuses the merge (409) if someone pushed in between. The answer is `{status, details: {message, uuid}}`: `merged` is done; `pending` with a `uuid` is polled with `GET .../merge-async/{uuid}` every 2 seconds for up to 5 minutes (`with_merge_polling` changes both), ignoring failures of the network and 5xx answers in between; any other status is `Error::MergeFailed` with the message. After the merge the client deletes the head branch (`DELETE .../git/refs/heads/{branch}` in the head repository, the segments of the branch encoded one by one), as GitHub isn't asked to. That cannot undo the merge, so its result is data in `MergeOutcome`: `Deleted`, `AlreadyGone` (422 "Reference does not exist": the repository deletes head branches by itself), `Skipped` (the head repository is gone) or `Failed(message)`.

### Rerun and approve

`rerun_failed` reads the failed jobs of the head commit and groups them by Actions run. With `include_cancelled` each such run gets `rerun-failed-jobs` (what `gh run rerun --failed` does, cancelled jobs included). Without, a run that has a cancelled job is rerun job by job for its failed ones, so the fail-fast siblings stay as they are. Both keep going when a request fails, so that a run still in progress doesn't stop the others, and report `Error::Partial` (or the error itself when it was the only request). Nothing failed is `Error::NothingToRerun`. `authorize` approves every run waiting, the same way, and returns how many.

### Runners

A repository's self-hosted runners (name, OS, online, busy, the custom labels; the read-only ones repeat `self-hosted` and the OS) need admin rights on the repository. The runners API doesn't say what a busy runner is running, only the jobs name their runner, so when a runner is busy the in-progress runs and their in-progress jobs are listed and matched by name.

## Errors

`Error` is a `thiserror` enum the core can show:

- `Unauthorized`: 401, the token is wrong or revoked.
- `RateLimited { reset, retry_after }`: 429; or 403 when `x-ratelimit-remaining` is `0` (the primary limit, `x-ratelimit-reset` is when it renews) or `retry-after` is present (the secondary one); or a GraphQL error of type `RATE_LIMITED`, which comes with a 200 and neither.
- `Forbidden(message)`: any other 403 (no admin rights on a repository, a token without the scope).
- `Api { status, message }`: any other non-2xx, with GitHub's message.
- `GraphQl(messages)`: the `errors` array of a 200 answer, joined. Any error fails the call, even with partial data.
- `Decode`, `Unreachable`, `InvalidUrl`, and the ones of the actions above: `NotAPullRequestUrl`, `PullRequestNotFound`, `NotOpen`, `ChecksNotPassed`, `MergeFailed`, `MergeTimeout`, `NothingToRerun`, `Partial`.

## Tests

There is no local GitHub, so there is no fake one to test against in this crate. `tests/api.rs` runs every call against wiremock with GitHub-shaped JSON (request method, path, query and body, the authorization header, parsing, pagination, every error). No test talks to the real GitHub, not even read-only, and none uses the user's `gh` or token: the JSON shapes come from GitHub's documentation and ghw's own tests. The GUI tests will later use a stub, `rc_test_github`.
