# GitHub pull requests

The GitHub page shows the user's open pull requests and their CI, and merges, updates, reruns and approves them: what the `ghw` command line tool does in a terminal. It has four parts so far: the protocol crate `rc_github`, the GitHub analogue of `rc_api`, the service in `rc_core::github`, the views in `rc_view::github`, and the Qt page. Feature IDs: GH-001 to GH-009 in [TRACEABILITY.md](../../TRACEABILITY.md).

## The crate

`rc_github` is a typed async client over `reqwest` (rustls). No `gh` process and no GitHub crate. It keeps no state; the serde wire types (`wire.rs`) are private and everything it returns is a plain type from `types.rs`, which `rc_core` maps later.

`GithubClient::new(api_url, token)` takes the API's base URL (`https://api.github.com` by default; the GUI tests point it at a local stub) and the token as a `SecretString`, sent as `Authorization: Bearer`. REST paths are relative to the base URL and GraphQL is `POST <base>/graphql`. That is right for github.com and for a stub, not for GitHub Enterprise, whose GraphQL is at `/api/graphql` beside its `/api/v3` REST (not needed yet). Every request carries `Accept: application/vnd.github+json`, `X-GitHub-Api-Version` and a `User-Agent` (GitHub refuses requests without one).

A pull request is named by `PrRef` (`owner/name` and number), not by URL: the queries take repository and number variables, so they don't depend on the host. `PrRef::from_url` reads the URLs the GUI has.

## Calls

| Call | What it asks |
|---|---|
| `viewer` | GraphQL `viewer { login }`: who the token belongs to, the cheapest check of a token |
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

The check rollup (the state of a commit's checks, and each job with its workflow, state, URL and times) is only in GraphQL: `gh search prs --json` can't return it, and REST needs a request per pull request and per check suite. The search works across all repositories, unlike `gh pr list`. The queries are ghw's, with the variables passed as variables instead of pasted into the text, plus `number`, `headRefName`, `baseRefName`, `headRefOid`, `reviewDecision` (`PullRequest::review`: approved, changes requested, review required, none when the branch asks for no review) and `mergeStateStatus` (`PullRequest::merge_state`: behind, dirty, blocked, clean, unstable, has hooks, draft; anything else unknown) and the jobs' `startedAt` and `completedAt` for the page. `statusCheckRollup.contexts` lists the jobs of the head commit: check runs (Actions jobs, whose `databaseId` is the job id) and status contexts (an external CI, without an id). `checkSuites` is asked as well because a run waiting for a maintainer's approval has no jobs yet, so the rollup is null and it only shows as a suite concluded `ACTION_REQUIRED` (`PullRequest::needs_approval`). The watched repositories are listed through REST because `viewer.watching` is twice as slow, and fetching each repository's pull requests through GraphQL times out (504).

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

## The service (`rc_core::github`)

`GitHub` is the handle a frontend drives, the analogue of `Client` for the accounts, and it is a service apart from them: GitHub is not an account, has no room list and needs no Rocket.Chat login. `Client::github(api_url, store, config, on_event)` starts it on the client's own tokio runtime (the client keeps a handle to it), so there is one runtime and one set of threads; `GitHub::start` takes any runtime handle and ports, which is how the tests run it. `Client::start`'s callback is unchanged. `on_event: Fn(GithubEvent)` is called from a runtime thread, in order, and must return quickly, like `Client`'s. Dropping the `GitHub` stops the service; drop it before the client.

Inside there is one actor task that owns the state. Everything slow (the token from `gh`, the keyring, GitHub's requests) runs in tasks of its own that report back through an inbox, so the actor never waits on I/O. It handles commands before the inbox and the inbox before the timer, like the account actors. Every answer carries the number of the connection (the session) it was made for, and the answer of an older one is dropped.

### Ports

In `github/ports.rs`, with the adapters in `adapters/github.rs`, `memory.rs` and `keyring.rs`:

- `GithubConnector::connect(token)` makes a `GithubApi` (cheap to clone, `impl Future` methods like `ServerApi`): `viewer`, `my_pull_requests`, `watched_repos`, `watched_pull_requests(repos, since)`, `runners(repo)`, and the commands `merge`, `update_branch`, `rerun_failed`, `rerun_job`, `approve_runs`. It returns the core's own types and `GithubError`; `RestGithubConnector::new(api_url)` maps `rc_github`'s types and errors, with the API's address from the caller so that a GUI test can point it at a stub. `rerun_failed` is always called with `include_cancelled = false`, as `ghw` does.
- `GhCli::token()` is `gh auth token` (`GhProcess`, a process run with no input). A missing `gh` (`NotInstalled`), one without a login (`NotLoggedIn`: it exits non-zero saying so, or prints nothing) and any other failure are `GhError`s, a state the connect page shows, not a crash. The tests run a script under `/bin/sh` in place of `gh`; nothing runs the real `gh` or reaches the real GitHub.
- `GithubStore` keeps the user's choice: `GithubAuth::GhCli` (no token: it is read from `gh` at each start and never copied) or `GithubAuth::Token`, a pasted one. `KeyringGithubStore` keeps it in one entry of the accounts' keyring (service `RocketChatQml`, user `github`; the text `gh`, or `token:` and the token), `MemoryGithubStore` in memory, and `adapters::default_github_store()` is the keyring with memory as the fallback, as `default_store()` is for the accounts. The store calls block, so they run off the async threads, in a worker that keeps their order (a disconnect followed by a connect must not end with the old token). `RC_SECRET_STORE` is read by the frontends' startup (`rc_view::startup::github_store`): memory for `memory`, the keyring otherwise, and memory with a log line for `file:` (there is no file-backed version). `Client::github` takes the `GhProcess` too; `rc_view::startup::github_setup` makes it, and the API's address, from `RC_GH_PROGRAM` and `RC_GITHUB_API` in developer-mode builds, and from the defaults (`gh`, `api.github.com`) otherwise.
- The `Clock` port of the accounts tells the time and sleeps, so tests move time.

### Connection

`GithubEvent::Connection` is `NotConnected`, `Connecting`, `Connected { login }` or `Failed(ConnectFailure)` (`BadToken`, `GhMissing`, `GhNotLoggedIn`, `RateLimited`, `Unreachable`, `Other(message)`). It is the first event: at startup the stored choice is read and resumed (`Connecting`, then the outcome); with nothing stored it is `NotConnected`. `connect_with_gh()` reads the token from `gh`; `connect_with_token(token)` trims it; both ask `viewer { login }`, and only on success keep the choice. A blank token is `BadToken` without a request. A new connect replaces one that is under way or running. `disconnect()` forgets the stored choice and token and empties everything. `retry()` tries the stored choice again after a failed resume (GitHub was unreachable at startup; a stored pasted token can't be asked of the frontend). A token GitHub refuses later (a refresh or a command gets 401) ends the connection as `Failed(BadToken)`, with the stored choice kept.

Losing the connection, for whatever reason, emits first what it empties: `Mine` and `Watched` reset to nothing, `Failing(0)`, `Runners([])` and `Notifying([])`, each only if it held something.

### Polling

A refresh fetches the user's pull requests, the watched ones, and the runners of the configured repositories (none until the settings step; `GithubConfig::runner_repos`), concurrently. It runs at once after connecting; then every 60 s while the frontend says the page is on screen (`set_on_screen`), every 5 minutes otherwise; at once on `refresh()`; and 3 s after any command, because GitHub's state lags (a rerun does not show as running yet). The next time is derived from when the last refresh started, the interval of the moment and the earlier of these requests, so coming on screen after two minutes off it fetches at once. Only one refresh runs: a request meanwhile is kept for after it. The watched repositories (`user/subscriptions`) are fetched again when more than 30 minutes old and on `refresh()`, not at each poll; the watched window is `GithubConfig::watched_days` (14) days back from today in UTC.

`GithubEvent::Refresh` is `Started`, then `Done { at }` or `Failed { error, last_good }`. A list that failed keeps what it held and the other list is still applied, so `Failed` comes after the changes that got through; `last_good` is when everything was last fetched (the "updated N s ago"). A failed runners fetch is not a failure: it keeps the previous runners of that repository, and one without admin rights has none.

A rate limit (`RateLimited`) holds every request back, the manual ones too, until its `reset`, or `retry_after` from now, or 60 s if neither came, and never less than 5 s. A 401 ends the connection (above).

### Lists

`Mine` and `Watched` hold `PullRequest`s (the core's type, with `PrKey` = repository and number, the jobs, the CI state, `needs_approval`) in display order: those that are not drafts first, then the drafts, each with the latest change first (the key breaks ties). The events are `Vec<ListChange<PullRequest>>`, the generic changes of `list_change` that the room list's `RoomChange` mirrors: the first fill (or the first after the list was emptied) is a `Reset`; later refreshes are diffed against what was emitted. Removals come first, highest index first, then moves, inserts and updates. The kept pull requests that are in the right order among themselves (the longest increasing run) stay put and the others are moved, so a push is one `Moved` (to the top) and a CI change one `Updated`; a refresh that changes nothing emits nothing, which keeps hundreds of watched pull requests cheap (the cost of a refresh is the diff, O(n log n), and the changes). A pull request that a query returns twice (paging can overlap) counts once. A tie in the run can be resolved either way (a draft made ready can be a move of it or of its neighbour), always with the same result.

`Failing(n)` is the tile's red count: the user's pull requests that are not drafts and whose CI failed. It is recomputed after each refresh but emitted only when it changes. The "something finished" pip is `CiEnded`.

### Commands

`merge(pr, force)`, `update_branch`, `rerun_failed`, `rerun_job(pr, job_id)` and `approve_runs` each run in the background and answer with `GithubEvent::Command { pr, action, attempt, state }`: `Started`, then `Done` or `Failed(GithubError)` with the same `AttemptId` (counted for the life of the service). While one runs on a pull request, another on the same one is not started or queued: it answers `Refused`, with the attempt that runs. Different pull requests run side by side. An answer is dropped if the connection changed meanwhile, so an old attempt cannot report over a newer one. The state that the refresh timer and the rate limit need is set before the event, so a frontend that reacts to `Done` sees a consistent service. Commands while not connected are dropped. Asking the user before a merge is the frontend's.

### Notify when done

`notify_when_done(pr, on)` adds a pull request of the lists whose CI is `Pending` to the set (anything else is ignored), or takes it out. After each refresh the set is checked against the lists as they are: a pull request whose CI is `Success` or `Failure` emits `CiEnded { pr, state }` and leaves the set, so it fires once; one that is in neither list any more (merged, closed) leaves silently; a failed refresh changes no list, so it cannot make a pull request look gone. `Notifying(keys)` carries the set, sorted, whenever it changes (for the "notify me" toggle). The desktop notification is the frontend's.

### Tests

`github/tests` runs the service on a runtime of its own with a fake `GithubApi` (every call recorded, answers set by the test, commands that can be held), a fake `gh`, `MemoryGithubStore` and a clock the test moves. A test that expects nothing to happen moves the clock, waits until the service has handled all commands (a test-only barrier) and checks the calls; one that expects something waits for it. `github/list.rs` also checks the diff against random lists by applying the changes to a copy.

## Tests

There is no local GitHub, so there is no fake one to test against in this crate. `tests/api.rs` runs every call against wiremock with GitHub-shaped JSON (request method, path, query and body, the authorization header, parsing, pagination, every error). No test talks to the real GitHub, not even read-only, and none uses the user's `gh` or token: the JSON shapes come from GitHub's documentation and ghw's own tests. The GUI tests use a stub, `rc_test_github`.

### The stub (`rc_test_github`)

`GithubStub::start()` runs a wiremock `MockServer` on a thread of its own, on a free localhost port (`url()`), with one catch-all `Respond` that reads and changes an `Arc<Mutex<State>>` shared with the test's handle. Wiremock was chosen because it is already a dependency, serves real HTTP, and its `Respond` is all a stateful server needs; a hand-written HTTP server would only add code. The test sets the state (viewer, watched repositories, `PullRequest`s built with defaults and `Job`s, `Runner`s) and reads it back; the stub answers by the same queries `rc_github` sends (told apart by their text): `author:@me` gives the viewer's open pull requests, `-author:@me created:>=<date> repo:..` the others' in those repositories created since the date, paged by a cursor; REST lists are paged by `per_page` and `page`.

Actions change the state as GitHub would, so that the next poll shows it: a merge (`merge-async`, 409 for a `sha` that is not the head, 405 when not open, optionally `pending` for a few polls) marks the pull request merged, which removes it from the searches, and the following `DELETE` of its head branch removes the branch; rerun failed and rerun a job turn those jobs to running (403 while a run is still in progress, as GitHub's); approve clears `needs_approval` and starts the jobs (a waiting run has none yet, so one is made); update branch sets the merge state to clean, a new head and the jobs to running (a conflict is a GraphQL error). Editing a pull request moves its update time to now, which reorders the lists. A test can plan failures for the next N requests to a path prefix or a GraphQL operation (401, a rate limit with its headers, 500, GraphQL `errors`), require a token (any is accepted otherwise), and read the log of requests (method, path, a short form of the GraphQL operation, the JSON body) to assert on.

`rc_test_github/tests/client.rs` drives `GithubClient` against the stub for every call and state change, so that the stub and the client agree.

The Qt app finds the stub through `RC_GITHUB_API` in developer-mode builds, and the GUI tests point `RC_GH_PROGRAM` at a script of their own that prints a token, so that neither the real `gh` nor the real GitHub is reached.

## The views (`rc_view::github`)

Plain Rust for both frontends, no GUI code and no clock: whatever shows an age or a duration takes `now` (`jiff::Timestamp`), so the frontend chooses when to redraw (a timer while the page is on screen).

### The cache

`GithubCache::apply(&GithubEvent) -> Applied` keeps what the views need and nothing the service holds twice over: the connection, the refresh state, the two lists, the failing count, the runners, the commands, the notify set and two flags. The lists are the service's own, changed by its `ListChange`s one to one (`ListChange::apply`), with no ordering of the cache's own; `Applied` says which part changed (`Connection`, `Refresh`, `Mine`, `Watched`, `Failing`, `Runners`, `Command(pr)`, `Notifying`, `CiEnded`), and the frontend applies the same event's changes to its model (`row_ops`). `sections(tab)` says where the non-drafts end (`ready`) and how many drafts follow: the page shows them under a folded "Drafts n" heading.

Views are built on demand, per item: `row(tab, index, now)` for one list row, `page(key, now)` for the page of a pull request of either list (`None` once it is gone from both, e.g. merged), `tile()`, `header(now)`, `connect_view()` and `runner_lines()`. A refresh of hundreds of watched pull requests costs the changes, not a view per row.

- **Commands**: per pull request the last `PrCommand` (action, attempt, `Running`, `Failed(reason)` or `Done(toast)`). An event of an older attempt is ignored, so is a `Started` after its own end, and `Refused` changes nothing (the attempt it names has its own `Started`). The toast or failure stays until `dismiss_command`; a running command isn't dismissed. Losing the connection drops them.
- **The pip**: `CiEnded` sets it unless the page is on screen (`set_on_screen(true)` clears it and the frontend tells the service as well). The event comes out as `Applied::CiEnded` for the desktop notification, whose words `ended_text` gives.
- **Bold rows**: the cache remembers the CI state of each pull request when it first saw it and when the user last opened it (`select`); a row whose state differs is `changed`. It is a map in memory, a few bytes per pull request, kept with the diffs of the lists (insert adds, remove drops, a reset forgets what neither list holds); it isn't stored, so after a restart nothing is bold.

### The rows and the page

A row has the glyph (`Draft`, `Failed`, `WaitingApproval`, `Running`, `Passed`, and `NoChecks`, which the mockup didn't have), the repository, the author (Watched rows only), the age (`text::age`: "40 s", "11 min", "1 h", "5 d", "3 w", "8 mo", "1 y", the unit changing at 1 min, 1 h, 1 day, 7 days, 30 days and 365 days), the bell, the title, "#number", the check bar and `changed`. The CI reads Failed first, then runs waiting for approval, then running, passed, none: a failure shows although other runs wait.

The check bar has a segment per job ordered failed, running, cancelled, passed (a job's place in its group is GitHub's), one waiting segment for a pull request with runs to approve and no jobs, nothing without checks. The page's job rows come in the same order and carry their `index`, so segment `i` is job row `i`.

The page's buttons are `Button { label, visible, enabled, busy }`. While a command runs on the pull request its button is busy and every command button, the job rows' Rerun among them, is disabled; Open on GitHub (the `url`) and the notify toggle aren't commands. Merge is always shown, and its reason (`merge_reason`) is the first rule that holds: a draft ("Drafts can't be merged"), runs to approve ("The runs need approval"), checks running ("Waiting for the checks"), checks failed ("2 checks failed"), no checks ("There are no checks"), then by merge state: behind ("main is ahead"), dirty ("Conflicts with main"), blocked ("Needs an approving review" when the review state asks for one, else "Blocked by a branch rule"). Clean, unstable, has hooks and unknown can be merged: for unknown the server has the last word and its refusal comes back as a failure. A pull request without checks can't be merged, as the service refuses it unless forced. Merge anyway shows for a failing, non-draft pull request. Merge and Merge anyway carry the question the frontend asks first (`merge_confirm`, `merge_anyway_confirm`). Update branch shows when behind, Rerun failed jobs when a job failed, Notify while the checks run.

The checks section has the heading, the tally ("1 failed, 2 running, 26 passed", nothing for zero), the bar and the non-empty groups "Failed n", "Running n", "Cancelled n" and "Passed n"; cancelled start folded, and passed ones unless everything passed. A job row shows "<workflow> / ", the name, the duration ("48 s", "9 m 12 s", "1 h 5 m"; "6 m so far" while it runs, counted from `now`; empty without times), Rerun for failed jobs with an id, and its URL.

### The tile, header, connect page and runners

The tile is `connected` (grey otherwise), the failing count the service sent, the pip and the tooltip: "N open, M failing" (N the user's pull requests that aren't drafts) or "Connect GitHub to see them". The header says "<login>, updated 40 s ago", "updating…" or "<error>, last updated 5 min ago" (just the error when nothing was ever fetched), with the two tabs' counts (non-drafts). The connect page gets its state and the reason of a failure as a sentence (`text::connect_failure_text`). The runner footer has a line per runner: "Linux runner, busy with <repo>", "idle" or "offline", the repository being the one whose runners were asked.

## The Qt page

The tile, the connect page, the list and the page of the open pull request (header, approval banner, actions, checks, toast) are in the Qt app (`rc_qt` and `src/gui`). The desktop notification when a CI ends is a later step. The Slint app has none of it yet.

It is off unless the app is started with `--gh`: `main.cpp` calls `rc_qt::enable_github()` before the core starts, so the hub doesn't start the service (its commands are then logged and ignored), and `AppInfo.github` is false, which hides the tile and lets the servers reach down to the settings button.

### Screens

`rc_view::switchboard` keeps what fills the main area (`Screen`: `Account` or `GitHub`) apart from the current account, which stays current under the GitHub page: its events still reach the views, so going back needs no snapshot. While GitHub is shown no server tile looks current (the rows' `current` flag means "current and on screen"; their unread dot shows). `Shown::Screen` tells the frontend: Main.qml shows `GithubPage.qml` while `Github.shown`, and the hub tells the cache and the service (`set_on_screen`), which polls every minute then.

### The hub's part (`rc_qt::hub::github`)

The service starts with the client (before it, so no first event is lost), on its runtime, and its events hop to the hub like the accounts' (`dispatch` locks the hub, applies the event to the `GithubCache` and queues what changed). The models are never given the cache to read: after each event the hub builds the rows of its changes (`github_view::row_changes`) and the state of the view model (`VmState::from_cache`) and queues them, so a model never sees a cache that is ahead of the changes still queued to it. `tick` (the page calls it every 30 s while on screen, and when it opens) builds the header again and asks the list to refresh the ages, which emits `dataChanged` for the age role of the rows where the text changed, and for nothing else.

- **`Github`** (singleton `GithubVm`): `shown`, the tile (`tileConnected`, `tileFailing`, `tilePip`, `tipTitle`, `tipLine`), the connect page (`connectState`, `connectReason`), the header (`headerTitle`, `headerStatus`, `refreshing`, `mineCount`, `watchedCount`) and the open pull request's header (`pageId` empty when none, `pageRepo`, `pageNumber`, `pageUpdated`, `pageTitle`, `pageHead`, `pageBase`, `pageReview`, `pageBaseNote`, `pageDraft`, `pageUrl`) and the rest of its page: `approvalText` (the banner; empty when no run waits) with `approveEnabled`, `approveBusy`; `mergeEnabled`, `mergeBusy`, `mergeReason`; `mergeAnywayVisible`, `mergeAnywayEnabled`, `mergeAnywayBusy`; the same three for `updateBranch` and `rerunFailed`; `notifyVisible`, `notifyEnabled`, `notifyLabel`, `notifying`; the question of Merge, `confirmKind` (empty, `merge` or `merge-anyway`) and `confirmText`; `commandError` and `toastText`; the top of the checks, `checksHeading`, `checksTally` with `tallyStates` and `tallyTexts` (a swatch before each word) and `checksBar`. Invokables: `show`, `connectWithGh`, `connectWithToken`, `disconnect`, `refresh`, `tick`, and for the pull request that is open `askMerge(force)`, `cancelConfirm`, `merge(force)`, `updateBranch`, `rerunFailed`, `rerunJob(index)` (the job's position in the bar; the hub looks its id up), `approveRuns`, `setNotify(on)` and `dismissCommand`. A pull request that leaves the lists (merged, or the login ended) closes its page.
- **`PullRequestList`**: one model for the tab on screen (`tab`, 0 Mine and 1 Watched; `showTab` swaps the rows, as one reset), because only one list shows at a time and hundreds of watched pull requests would otherwise be kept in two models. It mirrors the service's list (the rows are `Entry`s: the `PrRowView`, the update time for the age and the draft flag) and applies its `ListChange`s one row at a time. A change carries the generation of the list it was made for; `showTab` makes a new one, so a change queued for the tab that was on screen is dropped by the model. The rows are the pull requests that are not drafts, a heading "Drafts n" (`rowType` `drafts`, with `draftsTitle` and `draftsOpen`) and, while it is open (`toggleDrafts`, closed on each tab change), the drafts. `rc_view::github::fold` plans the notifications for each change (`Fold::plan`, tested against a plain list with random changes): inserts and removes at the right display row, the heading coming and going with the first and last draft, a move inside a section as one `beginMoveRows`, one across the heading as a remove and an insert, and nothing at all for a draft while it is folded. `select(row)` opens the pull request (`GithubCache::select`, which makes its row not bold).
- **`JobList`** (`job_list_model.rs`, rows in `job_rows.rs`): the jobs of the open pull request's checks, flat: a heading per group (`rowType` `group`: `groupName` "Failed", `groupTitle` "Failed 2", `groupOpen`) followed by its jobs while it is open (`rowType` `job`: `jobIndex`, `jobState`, `workflow`, `jobName`, `duration`, `rerunVisible`, `rerunEnabled`, `rerunBusy`, `url`). The hub gives it the groups of the page whenever it builds the page's state; the same rows with other words (a poll) are one `dataChanged`, a group coming or going or another pull request a reset. Folds the user made (`toggleGroup(state)`, `reveal(jobIndex)`, which opens the job's group and returns its row) are the model's and are forgotten with the pull request.
- **`RunnerList`**: the runner lines of the foot, reset as a whole when they change (a few rows). Nothing asks for runners until the settings step lets the user choose the repositories, so it stays empty.

### The Qt page

- **The tile** (`GithubTile.qml`) sits at the bottom of the rail, above the settings button and below the list of servers, with a server tile's size, radius, badge and pips and the GitHub mark (`icons/mark-github.svg`, the Octicons path). Greyed out while not connected (the base colour, a faint mark, an inset border), with the red count of failing pull requests, the short pip when a CI ended while the page wasn't looked at (not yet set, for the notifications are a later step), and the tall pip and the own wash while the page is on screen. The tool tip is the rail's (`RailTip`): "Pull requests" over "N open, M failing" or "Connect GitHub to see them". A click shows the page, a right-click offers "Disconnect GitHub" when connected.
- **The connect page** (`GithubConnectPage.qml`, no sidebar) offers the login of `gh` ("Use it") or a pasted token (a monospace field, `echoMode` password, and "Connect with the token"), disabled with a spinner while connecting, and the reason of a failure in the error colour.
- **The sidebar** (`GithubSidebar.qml`) has the header (title, "<login>, updated 40 s ago", the refresh button that spins while it fetches), the tabs with their counts, the list (`PullRequestRow` and `DraftsHeading` chosen by `rowType`) with its empty text, and the runner lines.
- **A row** has the state glyph, the repository (and the author on Watched), the bell, the age, the title as styled text followed by the number in the faint colour (`rc_view::rich_html::escaped`, so a title with markup shows as written), bold when its CI changed since the user looked, and the check bar (`CheckBar.qml`), one segment per job in the colour of its state, dimmed for passed ones unless the row is selected.
- **The main area** (`GithubMain.qml`) shows the open pull request's header (repository and number, "updated 3 min ago", title, the branches, the review and base notes, "Draft", an "Open on GitHub" button) or "Pick a pull request to see its checks."

The element names are in [GUI testing](gui-testing.md). The tests (`crates/rc_gui_tests/tests/github.rs`: GH-001, GH-002 and GH-009) start the app with `RC_GITHUB_API` at an `rc_test_github` stub, `RC_SECRET_STORE=memory` and `RC_GH_PROGRAM` at a script that prints a token, with no Rocket.Chat server.

### The page of a pull request

`GithubMain.qml` stacks, under the header, `GithubApproval` (the warning strip with "Approve runs", only while `approvalText` isn't empty), `GithubActionBar` and `GithubChecks`, and puts `GithubToast` over the bottom of the area. All buttons are `GithubButton` in a look (`primary`, `warning`, `signal`, `ghost`, `quiet`; `on` for a toggle that is on; `busy` shows a spinner and keeps the colour though the button is disabled), so the view's `visible`, `enabled` and `busy` of each button are bound as they are.

- **The action bar** shows Merge (primary, with the reason after it in faint text while disabled), Merge anyway (quiet, only when the checks failed), then Update branch, Rerun failed jobs and the notify toggle (bell, filled bell while on) at the right. Merge and Merge anyway only ask: `askMerge` sets the confirmation in the hub (`GithubHub::confirming`, which `VmState` turns into `confirmKind` and `confirmText` from the view's texts), and the question with Merge (or Merge anyway) and Cancel takes the bar's place. Cancel, Escape (a `Shortcut`), selecting another pull request and the pull request leaving the lists drop it; Merge in the question calls `merge(force)`, which drops it and starts the command. The reason of a failed command is a line under the bar, in the error colour, with a dismiss button.
- **The toast** shows `toastText` (the text of a command that is done) for `Theme.githubToastDuration`, then calls `dismissCommand`. The hub keeps the pull request selected while its last command has something to say, so the toast of a merge, which drops the pull request from the lists a moment later, still shows (over the hint to pick one).
- **The checks** are one `ListView` (so all of it scrolls): its header has the heading, the tally with a swatch of each state's colour (`Theme.checkColor`, which the bars use too) and the bar (`CheckBar` made `interactive`), and its rows are the `JobList`. A segment and its job row light each other (`litJob`, one property of the list that the bar's `MouseArea`s and the rows' `HoverHandler`s set; the segment grows by `Theme.checkBarHoverScale`); a click on a segment calls `JobList.reveal` and scrolls to the row. Rerun on a failed job calls `rerunJob`, the button beside it opens the job's URL.
