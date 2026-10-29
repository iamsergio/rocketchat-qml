// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The ports of the GitHub service, faked: GitHub itself, `gh`, and the clock.

// The port's methods are async, though the fake answers at once.
#![allow(clippy::unused_async_trait_impl)]

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use secrecy::{ExposeSecret, SecretString};
use tokio::sync::Semaphore;

use crate::github::{
    GhCli, GhError, GithubApi, GithubConnector, GithubError, PrKey, PullRequest, Runner,
};
use crate::ports::Clock;

/// What the service asked GitHub.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::github) enum Call {
    Viewer,
    Mine,
    Repos,
    Watched {
        repos: Vec<String>,
        since: jiff::civil::Date,
    },
    Runners(String),
    Merge {
        pr: PrKey,
        force: bool,
    },
    UpdateBranch(PrKey),
    RerunFailed {
        pr: PrKey,
        include_cancelled: bool,
    },
    RerunJob {
        repo: String,
        job_id: u64,
    },
    Approve(PrKey),
}

pub(super) struct State {
    pub(super) viewer: Result<String, GithubError>,
    pub(super) mine: Result<Vec<PullRequest>, GithubError>,
    pub(super) repos: Result<Vec<String>, GithubError>,
    pub(super) watched: Result<Vec<PullRequest>, GithubError>,
    pub(super) runners: HashMap<String, Result<Vec<Runner>, GithubError>>,
    /// What every command answers.
    pub(super) command: Result<(), GithubError>,
    /// The tokens the connector was given, in order.
    pub(super) tokens: Vec<String>,
    pub(super) calls: Vec<Call>,
    /// Commands wait for a permit of the gate before they answer.
    pub(super) hold_commands: bool,
}

/// GitHub, as a test sets it up. Clones share the state.
#[derive(Clone)]
pub(super) struct FakeGithub {
    state: Arc<Mutex<State>>,
    /// Held commands take a permit; the test adds one to let the longest waiting command go.
    pub(super) gate: Arc<Semaphore>,
}

impl FakeGithub {
    pub(super) fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                viewer: Ok("alice".to_owned()),
                mine: Ok(Vec::new()),
                repos: Ok(Vec::new()),
                watched: Ok(Vec::new()),
                runners: HashMap::new(),
                command: Ok(()),
                tokens: Vec::new(),
                calls: Vec::new(),
                hold_commands: false,
            })),
            gate: Arc::new(Semaphore::new(0)),
        }
    }

    pub(super) fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap()
    }

    pub(super) fn calls(&self) -> Vec<Call> {
        self.state().calls.clone()
    }

    pub(super) fn count(&self, wanted: &Call) -> usize {
        self.state().calls.iter().filter(|c| *c == wanted).count()
    }

    fn record(&self, call: Call) {
        self.state().calls.push(call);
    }

    async fn command(&self, call: Call) -> Result<(), GithubError> {
        self.record(call);
        let hold = self.state().hold_commands;
        if hold && let Ok(permit) = self.gate.acquire().await {
            permit.forget();
        }
        self.state().command.clone()
    }
}

impl GithubConnector for FakeGithub {
    type Api = FakeGithub;

    fn connect(&self, token: &SecretString) -> Result<FakeGithub, GithubError> {
        self.state().tokens.push(token.expose_secret().to_owned());
        Ok(self.clone())
    }
}

impl GithubApi for FakeGithub {
    async fn viewer(&self) -> Result<String, GithubError> {
        self.record(Call::Viewer);
        self.state().viewer.clone()
    }

    async fn my_pull_requests(&self) -> Result<Vec<PullRequest>, GithubError> {
        self.record(Call::Mine);
        self.state().mine.clone()
    }

    async fn watched_repos(&self) -> Result<Vec<String>, GithubError> {
        self.record(Call::Repos);
        self.state().repos.clone()
    }

    async fn watched_pull_requests(
        &self,
        repos: &[String],
        since: jiff::civil::Date,
    ) -> Result<Vec<PullRequest>, GithubError> {
        self.record(Call::Watched {
            repos: repos.to_vec(),
            since,
        });
        self.state().watched.clone()
    }

    async fn runners(&self, repo: &str) -> Result<Vec<Runner>, GithubError> {
        self.record(Call::Runners(repo.to_owned()));
        self.state()
            .runners
            .get(repo)
            .cloned()
            .unwrap_or(Ok(Vec::new()))
    }

    async fn merge(&self, pr: &PrKey, force: bool) -> Result<(), GithubError> {
        self.command(Call::Merge {
            pr: pr.clone(),
            force,
        })
        .await
    }

    async fn update_branch(&self, pr: &PrKey) -> Result<(), GithubError> {
        self.command(Call::UpdateBranch(pr.clone())).await
    }

    async fn rerun_failed(&self, pr: &PrKey, include_cancelled: bool) -> Result<(), GithubError> {
        self.command(Call::RerunFailed {
            pr: pr.clone(),
            include_cancelled,
        })
        .await
    }

    async fn rerun_job(&self, repo: &str, job_id: u64) -> Result<(), GithubError> {
        self.command(Call::RerunJob {
            repo: repo.to_owned(),
            job_id,
        })
        .await
    }

    async fn approve_runs(&self, pr: &PrKey) -> Result<(), GithubError> {
        self.command(Call::Approve(pr.clone())).await
    }
}

/// The GitHub CLI: gives the token the test sets, and counts how often it was asked.
#[derive(Clone)]
pub(super) struct FakeGh {
    answer: Arc<Mutex<Result<String, GhError>>>,
    asked: Arc<Mutex<usize>>,
}

impl FakeGh {
    pub(super) fn logged_in(token: &str) -> Self {
        Self {
            answer: Arc::new(Mutex::new(Ok(token.to_owned()))),
            asked: Arc::new(Mutex::new(0)),
        }
    }

    pub(super) fn set(&self, answer: Result<&str, GhError>) {
        *self.answer.lock().unwrap() = answer.map(str::to_owned);
    }

    pub(super) fn asked(&self) -> usize {
        *self.asked.lock().unwrap()
    }
}

impl GhCli for FakeGh {
    fn token(&self) -> Result<SecretString, GhError> {
        *self.asked.lock().unwrap() += 1;
        self.answer.lock().unwrap().clone().map(SecretString::from)
    }
}

/// A clock that tells the time the test sets, and stands still otherwise. Clones share the time.
/// A sleep ends when the test moves the time to its end.
#[derive(Clone)]
pub(super) struct TestClock(Arc<tokio::sync::watch::Sender<jiff::Timestamp>>);

impl TestClock {
    /// 2026-01-15 12:00 UTC.
    pub(super) fn start() -> jiff::Timestamp {
        jiff::Timestamp::from_second(1_768_478_400).unwrap()
    }

    pub(super) fn new() -> Self {
        Self(Arc::new(tokio::sync::watch::channel(Self::start()).0))
    }

    pub(super) fn advance(&self, by: Duration) {
        self.0
            .send_modify(|time| *time = time.checked_add(by).expect("in range"));
    }
}

impl Clock for TestClock {
    fn now(&self) -> jiff::Timestamp {
        *self.0.borrow()
    }

    fn sleep_until(&self, at: jiff::Timestamp) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        let mut time = self.0.subscribe();
        Box::pin(async move {
            // Fails only if the clock is gone, and then nothing is waiting any more.
            let _ = time.wait_for(|now| *now >= at).await;
        })
    }
}
