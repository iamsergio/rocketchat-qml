// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The GitHub part of the hub: the service beside the accounts, the cache of what it says, and the
//! view model and models that show it. Like the accounts' views, the models are given the changes
//! of each event as rows built here, right after the cache took the event in, so they never read
//! a cache that is ahead of what they have been told.
//!
//! Only the list on screen (Mine or Watched) has a model. Its rows are numbered by a generation
//! that grows whenever the model is given a whole list again, and a change made for an older
//! generation is dropped by the model: the list it was given later has it already.

use std::collections::HashSet;

use cxx_qt::CxxQtThread;
use jiff::Timestamp;
use rc_core::github::{GitHub, GithubEvent, PrKey};
use rc_view::github::{Applied, GithubCache, RunnerLine, Tab};
use secrecy::SecretString;

use super::{Registered, lock};
use crate::github_view::{VmState, entries, row_changes};
use crate::github_vm::qobject::GithubVm;
use crate::pull_request_list::{Snapshot, qobject::PullRequestList};
use crate::runner_list_model::qobject::RunnerList;

pub(super) struct GithubHub {
    /// The running service, once the client has started it.
    pub(super) service: Option<GitHub>,
    cache: GithubCache,
    /// The GitHub page is on screen.
    shown: bool,
    /// The list the model shows.
    tab: Tab,
    generation: u64,
    /// The pull request whose page is open.
    selected: Option<PrKey>,
    vm: Registered<GithubVm>,
    list: Registered<PullRequestList>,
    runners: Registered<RunnerList>,
}

impl GithubHub {
    pub(super) fn new() -> Self {
        Self {
            service: None,
            cache: GithubCache::new(),
            shown: false,
            tab: Tab::Mine,
            generation: 0,
            selected: None,
            vm: Registered::new("Github"),
            list: Registered::new("PullRequestList"),
            runners: Registered::new("RunnerList"),
        }
    }

    /// Takes one event of the service in, on the GUI thread's behalf: the cache first, then what it
    /// changed to the models.
    fn dispatch(&mut self, event: &GithubEvent) {
        let now = Timestamp::now();
        let applied = self.cache.apply(event);
        match (&applied, event) {
            (Applied::Mine, GithubEvent::Mine(changes))
            | (Applied::Watched, GithubEvent::Watched(changes)) => {
                let tab = if applied == Applied::Mine {
                    Tab::Mine
                } else {
                    Tab::Watched
                };
                if tab == self.tab {
                    let rows = row_changes(&self.cache, tab, changes, now);
                    let generation = self.generation;
                    self.list.queue(move |model| model.apply(generation, &rows));
                }
            }
            (Applied::Runners, _) => {
                let lines = self.cache.runner_lines();
                self.runners.queue(move |model| model.show(lines));
            }
            (Applied::Notifying, GithubEvent::Notifying(keys)) => {
                let notifying: HashSet<PrKey> = keys.iter().cloned().collect();
                self.list.queue(move |model| model.set_bells(&notifying));
            }
            // The desktop notification for the end of a CI comes with the notifications
            // themselves.
            _ => {}
        }
        self.push_state(now);
    }

    /// Tells the view model what the cache says.
    fn push_state(&mut self, now: Timestamp) {
        // A pull request that left the lists (merged, closed, or the login ended) has no page.
        if self
            .selected
            .as_ref()
            .is_some_and(|key| self.cache.find(key).is_none())
        {
            self.selected = None;
        }
        let state = VmState::from_cache(&self.cache, self.shown, self.selected.as_ref(), now);
        self.vm.queue(move |vm| vm.show_state(&state));
    }

    /// The GitHub page came on screen, or went off it.
    pub(super) fn set_shown(&mut self, shown: bool) {
        self.shown = shown;
        self.cache.set_on_screen(shown);
        if let Some(service) = &self.service {
            service.set_on_screen(shown);
        }
        self.push_state(Timestamp::now());
    }

    fn snapshot(&self, now: Timestamp) -> Snapshot {
        Snapshot {
            generation: self.generation,
            tab: self.tab,
            entries: entries(&self.cache, self.tab, now),
        }
    }
}

/// The core's GitHub event callback, on a runtime thread.
#[allow(clippy::needless_pass_by_value)] // The callback's signature.
pub(super) fn dispatch(event: GithubEvent) {
    if let Some(github) = lock().github.as_mut() {
        github.dispatch(&event);
    }
}

/// Runs `command` on the GitHub part of the hub, or logs that it was ignored.
fn with_github<R>(what: &str, command: impl FnOnce(&mut GithubHub) -> R) -> Option<R> {
    let mut hub = lock();
    let result = hub.github.as_mut().map(command);
    if result.is_none() {
        tracing::warn!("{what} ignored: the core isn't running");
    }
    result
}

/// Runs `command` on the service, or logs that it was ignored.
fn with_service(what: &str, command: impl FnOnce(&GitHub)) {
    with_github(what, |github| {
        if let Some(service) = &github.service {
            command(service);
        } else {
            tracing::warn!("{what} ignored: GitHub isn't running");
        }
    });
}

/// Registers the `Github` view model and returns what it shows now. Call on the GUI thread; the
/// changes that follow are queued to `thread`.
pub fn register_github(thread: CxxQtThread<GithubVm>) -> VmState {
    with_github("register_github", |github| {
        github.vm.register(thread);
        VmState::from_cache(
            &github.cache,
            github.shown,
            github.selected.as_ref(),
            Timestamp::now(),
        )
    })
    .unwrap_or_default()
}

/// Registers the `PullRequestList` model and returns the rows it starts with.
pub fn register_pull_requests(thread: CxxQtThread<PullRequestList>) -> Snapshot {
    with_github("register_pull_requests", |github| {
        github.list.register(thread);
        github.snapshot(Timestamp::now())
    })
    .unwrap_or(Snapshot {
        generation: 0,
        tab: Tab::Mine,
        entries: Vec::new(),
    })
}

/// Registers the `RunnerList` model and returns its rows.
pub fn register_runners(thread: CxxQtThread<RunnerList>) -> Vec<RunnerLine> {
    with_github("register_runners", |github| {
        github.runners.register(thread);
        github.cache.runner_lines()
    })
    .unwrap_or_default()
}

/// The model shows the other list from now on: this is its rows. What was queued to it for the
/// list it showed is stale then (it has another generation).
pub fn github_show_tab(tab: Tab) -> Snapshot {
    with_github("github_show_tab", |github| {
        github.tab = tab;
        github.generation += 1;
        github.snapshot(Timestamp::now())
    })
    .unwrap_or(Snapshot {
        generation: 0,
        tab,
        entries: Vec::new(),
    })
}

/// Opens the pull request: its page shows, and its row is not bold until its CI changes again.
pub fn github_select(key: &PrKey) {
    with_github("github_select", |github| {
        github.cache.select(key);
        github.selected = Some(key.clone());
        github.push_state(Timestamp::now());
    });
}

/// The time passed: the ages of the rows and the header's "updated N s ago" show again.
pub fn github_tick() {
    with_github("github_tick", |github| {
        let now = Timestamp::now();
        github.push_state(now);
        github.list.queue(move |model| model.refresh_ages(now));
    });
}

/// Shows the GitHub page in place of the current account's.
pub fn github_show() {
    let mut guard = lock();
    let hub = &mut *guard;
    let shown = hub.switchboard.show_github();
    hub.carry_out(shown);
}

pub fn github_connect_with_gh() {
    with_service("connect_with_gh", GitHub::connect_with_gh);
}

pub fn github_connect_with_token(token: SecretString) {
    with_service("connect_with_token", |service| {
        service.connect_with_token(token);
    });
}

pub fn github_disconnect() {
    with_service("disconnect", GitHub::disconnect);
}

pub fn github_refresh() {
    with_service("refresh", GitHub::refresh);
}
