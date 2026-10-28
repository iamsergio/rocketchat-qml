// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The GitHub service driven through a fake GitHub, a fake `gh`, a store in memory and a clock the
//! test moves. Plain `#[test]`s: the service runs on a runtime of the harness, which can't be
//! dropped inside an async test. Nothing here touches the network, `gh` or the OS keyring.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use jiff::Timestamp;
use secrecy::SecretString;
use tokio::runtime::{Builder, Runtime};

use super::{
    CiState, Connection, GitHub, GithubAuth, GithubConfig, GithubEvent, GithubPorts, MergeState,
    PrKey, PullRequest, RefreshState,
};
use crate::adapters::MemoryGithubStore;
use crate::list_change::ListChange;
use fake::{Call, FakeGh, FakeGithub, TestClock};

mod commands;
mod connection;
mod fake;
mod lists;
mod notify;
mod polling;

const TIMEOUT: Duration = Duration::from_secs(5);

/// Polls `check` until it returns a value.
fn wait_until<T>(check: impl Fn() -> Option<T>) -> T {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if let Some(value) = check() {
            return value;
        }
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// A pull request of `o/r`, updated `minute` minutes after a fixed time.
fn pr(number: u64, minute: i64, ci: CiState) -> PullRequest {
    PullRequest {
        key: PrKey::new("o/r", number),
        title: format!("Pull request {number}"),
        author: "alice".to_owned(),
        url: format!("https://github.com/o/r/pull/{number}"),
        updated_at: Timestamp::from_second(1_768_000_000 + minute * 60).unwrap(),
        is_draft: false,
        review: None,
        merge_state: MergeState::Clean,
        head_branch: format!("branch-{number}"),
        base_branch: "main".to_owned(),
        head_sha: format!("sha{number}"),
        ci,
        jobs: Vec::new(),
        needs_approval: false,
    }
}

fn draft(number: u64, minute: i64) -> PullRequest {
    PullRequest {
        is_draft: true,
        ..pr(number, minute, CiState::None)
    }
}

fn key(number: u64) -> PrKey {
    PrKey::new("o/r", number)
}

fn token(text: &str) -> SecretString {
    SecretString::from(text)
}

struct Harness {
    github: Option<GitHub>,
    runtime: Option<Runtime>,
    github_api: FakeGithub,
    gh: FakeGh,
    store: MemoryGithubStore,
    clock: TestClock,
    events: Receiver<GithubEvent>,
}

impl Harness {
    fn new() -> Self {
        Self::with(MemoryGithubStore::default(), GithubConfig::default())
    }

    /// A service that starts with `store` as it is, e.g. after an earlier run.
    fn with(store: MemoryGithubStore, config: GithubConfig) -> Self {
        Self::with_gh(store, config, FakeGh::logged_in("gh-token"))
    }

    fn with_gh(store: MemoryGithubStore, config: GithubConfig, gh: FakeGh) -> Self {
        let runtime = Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let github_api = FakeGithub::new();
        let clock = TestClock::new();
        let (tx, events) = mpsc::channel();
        let github = GitHub::start(
            runtime.handle(),
            GithubPorts {
                connector: github_api.clone(),
                gh: gh.clone(),
                store: store.clone(),
                clock: clock.clone(),
            },
            config,
            move |event| {
                let _ = tx.send(event);
            },
        );
        Self {
            github: Some(github),
            runtime: Some(runtime),
            github_api,
            gh,
            store,
            clock,
            events,
        }
    }

    fn github(&self) -> &GitHub {
        self.github.as_ref().unwrap()
    }

    /// The next event.
    fn next(&self) -> GithubEvent {
        self.events.recv_timeout(TIMEOUT).expect("no event")
    }

    /// The events up to and including the first that `done` accepts.
    fn until(&self, done: impl Fn(&GithubEvent) -> bool) -> Vec<GithubEvent> {
        let mut events = Vec::new();
        loop {
            let event = self.next();
            let last = done(&event);
            events.push(event);
            if last {
                return events;
            }
        }
    }

    /// The events up to the end of the next refresh, done or failed.
    fn until_refreshed(&self) -> Vec<GithubEvent> {
        self.until(|e| {
            matches!(
                e,
                GithubEvent::Refresh(RefreshState::Done { .. } | RefreshState::Failed { .. })
            )
        })
    }

    fn until_connected(&self) -> Vec<GithubEvent> {
        self.until(|e| matches!(e, GithubEvent::Connection(Connection::Connected { .. })))
    }

    /// Connects with a pasted token and waits for the first refresh to end.
    fn connect(&self) -> Vec<GithubEvent> {
        self.github().connect_with_token(token("ghp_pasted"));
        let mut events = self.until_connected();
        events.extend(self.until_refreshed());
        events
    }

    /// Moves the clock and waits for the refresh that is due.
    fn advance_to_refresh(&self, by: Duration) -> Vec<GithubEvent> {
        self.clock.advance(by);
        self.until_refreshed()
    }

    /// Moves the clock and checks that nothing was fetched: the service has seen all commands, and
    /// asked GitHub nothing more.
    fn advance_quietly(&self, by: Duration) {
        let before = self.github_api.calls().len();
        self.clock.advance(by);
        self.github().barrier();
        assert_eq!(self.github_api.calls().len(), before, "it fetched too soon");
    }

    fn no_more_events(&self) {
        self.github().barrier();
        match self.events.recv_timeout(Duration::from_millis(100)) {
            Err(RecvTimeoutError::Timeout) => {}
            other => panic!("unexpected event {other:?}"),
        }
    }

    fn stored(&self) -> Option<GithubAuth> {
        use crate::github::GithubStore;
        self.store.load().unwrap()
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        self.github.take();
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_timeout(Duration::from_secs(1));
        }
    }
}

fn mine_changes(events: &[GithubEvent]) -> Vec<ListChange<PullRequest>> {
    events
        .iter()
        .filter_map(|e| match e {
            GithubEvent::Mine(changes) => Some(changes.clone()),
            _ => None,
        })
        .flatten()
        .collect()
}

fn watched_changes(events: &[GithubEvent]) -> Vec<ListChange<PullRequest>> {
    events
        .iter()
        .filter_map(|e| match e {
            GithubEvent::Watched(changes) => Some(changes.clone()),
            _ => None,
        })
        .flatten()
        .collect()
}
