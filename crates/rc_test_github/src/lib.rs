// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Test support: a local GitHub for the GUI tests. There is none to run, and no test may talk to
//! the real one (not even read-only), so [`GithubStub`] answers the requests `rc_github` makes
//! from state the test controls: pull requests with their jobs, the watched repositories and the
//! runners. Actions change the state the way GitHub would (a merge ends the pull request, a rerun
//! starts its jobs), so the next poll shows it, and the requests received are logged for the test
//! to assert on.
//!
//! The server is wiremock's `MockServer` on a thread of its own, with one catch-all mock whose
//! `Respond` reads and changes the shared state. Calls block and may panic: they only stand in for
//! what happens on GitHub while a test runs.

#![forbid(unsafe_code)]
// Test support: a failed call should just fail the test.
#![allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]

mod model;
mod router;

use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};

use jiff::Timestamp;
use rc_github::{JobState, MergeState, ReviewDecision};
use tokio::runtime::Builder;
use wiremock::matchers::any;
use wiremock::{Mock, MockServer};

pub use model::{Failure, Job, PullRequest, Request, Runner, RunnerJob};
use router::{Router, State};

/// A running stub. Dropping it stops the server.
pub struct GithubStub {
    url: String,
    state: Arc<Mutex<State>>,
    stop: Option<Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl GithubStub {
    /// Starts a stub on a free localhost port. The viewer is `me`, no repository is watched, and
    /// every token is accepted.
    ///
    /// # Panics
    ///
    /// If the server can't start.
    #[must_use]
    pub fn start() -> Self {
        let state = Arc::new(Mutex::new(State::new()));
        let (url_tx, url_rx) = mpsc::channel();
        let (stop, stop_rx) = mpsc::channel::<()>();
        let router = Router(Arc::clone(&state));
        let thread = thread::spawn(move || {
            let runtime = Builder::new_current_thread().build().unwrap();
            runtime.block_on(async {
                let server = MockServer::start().await;
                Mock::given(any()).respond_with(router).mount(&server).await;
                url_tx.send(server.uri()).unwrap();
                // The server runs on a thread of its own: this one only keeps it alive.
                let _ = stop_rx.recv();
            });
        });
        let url = url_rx.recv().unwrap();
        state.lock().unwrap().base.clone_from(&url);
        Self {
            url,
            state,
            stop: Some(stop),
            thread: Some(thread),
        }
    }

    /// The address to give `GithubClient::new` (or the app): `http://127.0.0.1:<port>`.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    fn with<R>(&self, f: impl FnOnce(&mut State) -> R) -> R {
        f(&mut self.lock())
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap()
    }

    // --- the account ---

    /// The login of the token's user, whom `author:@me` means. `me` at first.
    pub fn set_viewer(&self, login: &str) {
        self.with(|s| login.clone_into(&mut s.viewer));
    }

    /// Refuses every request that doesn't carry `Bearer <token>` with a 401; `None` accepts any
    /// token again.
    pub fn require_token(&self, token: Option<&str>) {
        self.with(|s| s.required_token = token.map(str::to_owned));
    }

    /// The repositories (`owner/name`) of the viewer's subscriptions.
    pub fn set_watched(&self, repos: &[&str]) {
        self.with(|s| s.watched = repos.iter().map(|r| (*r).to_owned()).collect());
    }

    // --- pull requests ---

    /// Adds a pull request and returns its number. Gives its jobs ids, and the jobs of one
    /// workflow one run.
    ///
    /// # Panics
    ///
    /// If the repository has a pull request with that number already.
    #[allow(clippy::must_use_candidate)] // A test often has no use for the number.
    pub fn add_pull_request(&self, pr: PullRequest) -> u64 {
        self.with(|s| s.add(pr))
    }

    /// Changes a pull request. Its update time moves to now unless `change` sets it.
    ///
    /// # Panics
    ///
    /// If there is no such pull request.
    pub fn edit_pull_request(
        &self,
        repo: &str,
        number: u64,
        change: impl FnOnce(&mut PullRequest),
    ) {
        self.with(|s| {
            let pr = s
                .find_mut(repo, number)
                .unwrap_or_else(|| panic!("no pull request {repo}#{number}"));
            let before = pr.updated_at;
            change(pr);
            if pr.updated_at == before {
                pr.updated_at = Timestamp::now();
            }
        });
    }

    /// Takes a pull request away as if it had never been (a merged one stays, see
    /// [`pull_request`](Self::pull_request)).
    pub fn remove_pull_request(&self, repo: &str, number: u64) {
        self.with(|s| s.prs.retain(|p| !(p.repo == repo && p.number == number)));
    }

    /// A copy of the pull request, whatever its state: a merged one is still here, with `state`
    /// `Merged`.
    #[must_use]
    pub fn pull_request(&self, repo: &str, number: u64) -> Option<PullRequest> {
        self.with(|s| s.find(repo, number).cloned())
    }

    /// Whether the branch exists in the repository: added with a pull request, gone after a
    /// `DELETE`.
    #[must_use]
    pub fn branch_exists(&self, repo: &str, branch: &str) -> bool {
        self.with(|s| s.branches.contains(&(repo.to_owned(), branch.to_owned())))
    }

    /// Sets the state of the jobs named `job` (a test "finishes" the CI with `Passed`).
    ///
    /// # Panics
    ///
    /// If there is no such pull request or job.
    pub fn set_job_state(&self, repo: &str, number: u64, job: &str, state: JobState) {
        self.edit_pull_request(repo, number, |pr| {
            let mut found = false;
            for j in pr.jobs.iter_mut().filter(|j| j.name == job) {
                j.set_state(state);
                found = true;
            }
            assert!(found, "no job {job} in {}#{}", pr.repo, pr.number);
        });
    }

    /// Sets the state of every job of the pull request.
    pub fn set_all_jobs(&self, repo: &str, number: u64, state: JobState) {
        self.edit_pull_request(repo, number, |pr| {
            for job in &mut pr.jobs {
                job.set_state(state);
            }
        });
    }

    pub fn set_merge_state(&self, repo: &str, number: u64, state: MergeState) {
        self.edit_pull_request(repo, number, |pr| pr.merge_state = state);
    }

    pub fn set_review(&self, repo: &str, number: u64, review: Option<ReviewDecision>) {
        self.edit_pull_request(repo, number, |pr| pr.review = review);
    }

    /// Makes a merge answer `pending` this many times when polled before it says `merged`.
    /// 0 (the default) merges at once.
    pub fn set_merge_polls(&self, polls: usize) {
        self.with(|s| s.merge_polls = polls);
    }

    // --- runners ---

    /// The self-hosted runners of a repository. A runner with a job is busy.
    pub fn set_runners(&self, repo: &str, runners: Vec<Runner>) {
        self.with(|s| {
            s.runners.insert(repo.to_owned(), runners);
        });
    }

    // --- failures ---

    /// Answers the next `count` requests that match `target` with `failure` instead. `target` is
    /// a path prefix when it starts with `/` (`/graphql`, `/repos/o/r/pulls/12/merge-async`), or a
    /// GraphQL operation prefix otherwise (`search`, `rebase`: see [`Request::operation`]).
    pub fn fail_next(&self, target: &str, count: usize, failure: Failure) {
        self.with(|s| s.failures.push((target.to_owned(), failure, count)));
    }

    /// Forgets the planned failures.
    pub fn clear_failures(&self) {
        self.with(|s| s.failures.clear());
    }

    // --- the log ---

    /// Every request received so far, in order, those that failed or were refused included.
    #[must_use]
    pub fn requests(&self) -> Vec<Request> {
        self.with(|s| s.requests.clone())
    }

    /// The requests with this method whose path starts with `path_prefix`.
    #[must_use]
    pub fn requests_to(&self, method: &str, path_prefix: &str) -> Vec<Request> {
        self.with(|s| {
            s.requests
                .iter()
                .filter(|r| r.method == method && r.path.starts_with(path_prefix))
                .cloned()
                .collect()
        })
    }

    /// The GraphQL operations received whose short form starts with `prefix`.
    #[must_use]
    pub fn operations(&self, prefix: &str) -> Vec<String> {
        self.with(|s| {
            s.requests
                .iter()
                .filter_map(|r| r.operation.clone())
                .filter(|op| op.starts_with(prefix))
                .collect()
        })
    }

    pub fn clear_requests(&self) {
        self.with(|s| s.requests.clear());
    }
}

impl Drop for GithubStub {
    fn drop(&mut self) {
        drop(self.stop.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
