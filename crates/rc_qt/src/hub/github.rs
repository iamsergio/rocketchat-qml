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
use rc_view::github::{Applied, GithubCache, JobGroupView, PrPageView, RunnerLine, Tab};
use secrecy::SecretString;

use super::{Registered, lock};
use crate::github_view::{Confirm, VmState, entries, pr_id, row_changes};
use crate::github_vm::qobject::GithubVm;
use crate::job_list_model::qobject::JobList;
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
    /// Merge or Merge anyway asked the user to confirm, for the selected pull request.
    confirming: Option<Confirm>,
    vm: Registered<GithubVm>,
    jobs: Registered<JobList>,
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
            confirming: None,
            vm: Registered::new("Github"),
            jobs: Registered::new("JobList"),
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

    /// The page of the selected pull request, if it is in a list.
    fn page(&self, now: Timestamp) -> Option<PrPageView> {
        self.selected
            .as_ref()
            .and_then(|key| self.cache.page(key, now))
    }

    /// Tells the view model and the job list what the cache says.
    fn push_state(&mut self, now: Timestamp) {
        let page = self.page(now);
        // A pull request that left the lists (merged, closed, or the login ended) has no page. It
        // stays selected while its last command has something to say, as the toast of a merge
        // outlives the pull request.
        if let Some(key) = &self.selected
            && page.is_none()
            && self.cache.command(key).is_none()
        {
            self.selected = None;
        }
        if page.is_none() {
            self.confirming = None;
        }
        let state = VmState::from_cache(
            &self.cache,
            self.shown,
            self.selected.as_ref(),
            page.as_ref(),
            self.confirming,
            now,
        );
        self.vm.queue(move |vm| vm.show_state(&state));
        let jobs = page.map(|page| (pr_id(&page.key), page.checks.groups));
        self.jobs.queue(move |model| model.show(jobs));
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
        let now = Timestamp::now();
        let page = github.page(now);
        VmState::from_cache(
            &github.cache,
            github.shown,
            github.selected.as_ref(),
            page.as_ref(),
            github.confirming,
            now,
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

/// Registers the `JobList` model and returns what it starts with: the open pull request's id and
/// its job groups.
pub fn register_jobs(thread: CxxQtThread<JobList>) -> Option<(String, Vec<JobGroupView>)> {
    with_github("register_jobs", |github| {
        github.jobs.register(thread);
        github
            .page(Timestamp::now())
            .map(|page| (pr_id(&page.key), page.checks.groups))
    })
    .flatten()
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
        github.confirming = None;
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

/// A command on the pull request that is open: `command` gets the service and its key. Nothing
/// happens when none is open.
fn with_selected(what: &str, command: impl FnOnce(&GitHub, PrKey)) {
    with_github(what, |github| {
        let (Some(service), Some(key)) = (&github.service, github.selected.clone()) else {
            tracing::warn!("{what} ignored: no pull request is open");
            return;
        };
        // A pull request that is gone (merged) has nothing to act on.
        if github.cache.find(&key).is_some() {
            command(service, key);
        }
    });
}

/// Asks the user to confirm Merge (or Merge anyway if `force`): the buttons are replaced by the
/// question until `github_cancel_confirm` or `github_merge`.
pub fn github_ask_merge(force: bool) {
    with_github("github_ask_merge", |github| {
        if github.selected.is_some() {
            github.confirming = Some(Confirm { force });
            github.push_state(Timestamp::now());
        }
    });
}

/// The user said no to the question of Merge.
pub fn github_cancel_confirm() {
    with_github("github_cancel_confirm", |github| {
        github.confirming = None;
        github.push_state(Timestamp::now());
    });
}

/// Merges the open pull request; `force` merges though the checks failed.
pub fn github_merge(force: bool) {
    with_github("github_merge", |github| {
        github.confirming = None;
        github.push_state(Timestamp::now());
    });
    with_selected("merge", |service, key| service.merge(key, force));
}

pub fn github_update_branch() {
    with_selected("update_branch", GitHub::update_branch);
}

pub fn github_rerun_failed() {
    with_selected("rerun_failed", GitHub::rerun_failed);
}

/// Reruns the job at this position of the check bar, if it has an id.
pub fn github_rerun_job(index: usize) {
    with_github("github_rerun_job", |github| {
        let Some(page) = github.page(Timestamp::now()) else {
            return;
        };
        let job_id = page
            .checks
            .groups
            .iter()
            .flat_map(|group| &group.jobs)
            .find(|job| job.index == index)
            .and_then(|job| job.job_id);
        if let (Some(service), Some(job_id)) = (&github.service, job_id) {
            service.rerun_job(page.key, job_id);
        }
    });
}

pub fn github_approve_runs() {
    with_selected("approve_runs", GitHub::approve_runs);
}

/// Asks to be told (or not any more) when the CI of the open pull request ends.
pub fn github_set_notify(on: bool) {
    with_selected("set_notify", |service, key| {
        service.notify_when_done(key, on);
    });
}

/// The toast or the failure of the last command was shown or closed.
pub fn github_dismiss_command() {
    with_github("github_dismiss_command", |github| {
        if let Some(key) = github.selected.clone() {
            github.cache.dismiss_command(&key);
            github.push_state(Timestamp::now());
        }
    });
}
