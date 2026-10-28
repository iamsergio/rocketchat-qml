// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The state a frontend keeps from the GitHub service's events, and the views built from it.

use std::collections::{BTreeSet, HashMap, HashSet};

use jiff::Timestamp;
use rc_core::github::{
    AttemptId, CiState, CommandState, Connection, GithubEvent, PrAction, PrKey, PullRequest,
    RefreshState, RepoRunners,
};
use rc_core::list_change::ListChange;

use super::chrome::{
    ConnectView, HeaderView, RunnerLine, TileView, connect_view, runner_lines, status_text,
};
use super::page::{PageContext, PrPageView, page_view};
use super::row::{PrRowView, row_view};
use super::text;

/// The two lists of the page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Mine,
    Watched,
}

/// How a list divides: the pull requests that aren't drafts come first, then the drafts, which
/// the page folds under a heading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sections {
    /// Rows `0..ready` are the pull requests that aren't drafts.
    pub ready: usize,
    /// Rows `ready..ready + drafts` are the drafts.
    pub drafts: usize,
}

impl Sections {
    /// "Drafts 4".
    #[must_use]
    pub fn drafts_title(self) -> String {
        format!("Drafts {}", self.drafts)
    }
}

/// How the last command on a pull request stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandStatus {
    Running,
    /// The reason, as a sentence.
    Failed(String),
    /// The toast text.
    Done(String),
}

/// The last command the service ran on a pull request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrCommand {
    pub action: PrAction,
    pub attempt: AttemptId,
    pub status: CommandStatus,
}

/// What an event changed, so the frontend updates only that. The list changes themselves are in
/// the event, which the frontend applies to its model (see `row_ops`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Applied {
    /// The event changed nothing the views show (a late answer of an older attempt, a command
    /// that was refused).
    Nothing,
    /// The connect page, and with it everything else when the connection was lost.
    Connection,
    /// The header: refreshing, updated N ago.
    Refresh,
    Mine,
    Watched,
    Failing,
    Runners,
    /// The command state of this pull request changed: its page, and the toast.
    Command(PrKey),
    /// The bells of the rows.
    Notifying,
    /// The CI of a pull request the user asked about ended: the frontend shows the desktop
    /// notification ([`GithubCache::ended_text`]). The tile's pip is set unless the page is on
    /// screen.
    CiEnded {
        pr: PrKey,
        state: CiState,
    },
}

/// What the GitHub screens know. Applies the service's events one to one: the lists change by the
/// service's own `ListChange`s, with no ordering of its own.
pub struct GithubCache {
    connection: Connection,
    refresh: Option<RefreshState>,
    mine: Vec<PullRequest>,
    watched: Vec<PullRequest>,
    failing: usize,
    runners: Vec<RepoRunners>,
    commands: HashMap<PrKey, PrCommand>,
    notifying: BTreeSet<PrKey>,
    unseen_finished: bool,
    on_screen: bool,
    /// The CI state each pull request had when the app first saw it or the user last opened it:
    /// a row whose state differs is bold. Kept for the life of the app, not stored.
    seen: HashMap<PrKey, CiState>,
}

impl Default for GithubCache {
    fn default() -> Self {
        Self {
            connection: Connection::NotConnected,
            refresh: None,
            mine: Vec::new(),
            watched: Vec::new(),
            failing: 0,
            runners: Vec::new(),
            commands: HashMap::new(),
            notifying: BTreeSet::new(),
            unseen_finished: false,
            on_screen: false,
            seen: HashMap::new(),
        }
    }
}

impl GithubCache {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Takes one event of the service.
    pub fn apply(&mut self, event: &GithubEvent) -> Applied {
        match event {
            GithubEvent::Connection(connection) => {
                self.apply_connection(connection);
                Applied::Connection
            }
            GithubEvent::Refresh(state) => {
                self.refresh = Some(state.clone());
                Applied::Refresh
            }
            GithubEvent::Mine(changes) => {
                self.apply_changes(Tab::Mine, changes);
                Applied::Mine
            }
            GithubEvent::Watched(changes) => {
                self.apply_changes(Tab::Watched, changes);
                Applied::Watched
            }
            GithubEvent::Failing(count) => {
                self.failing = *count;
                Applied::Failing
            }
            GithubEvent::Runners(runners) => {
                self.runners.clone_from(runners);
                Applied::Runners
            }
            GithubEvent::Command {
                pr,
                action,
                attempt,
                state,
            } => self.apply_command(pr, *action, *attempt, state),
            GithubEvent::Notifying(keys) => {
                self.notifying = keys.iter().cloned().collect();
                Applied::Notifying
            }
            GithubEvent::CiEnded { pr, state } => {
                if !self.on_screen {
                    self.unseen_finished = true;
                }
                Applied::CiEnded {
                    pr: pr.clone(),
                    state: *state,
                }
            }
            // The service's events are non-exhaustive: one we don't know shows nothing.
            _ => Applied::Nothing,
        }
    }

    fn apply_connection(&mut self, connection: &Connection) {
        self.connection = connection.clone();
        if !matches!(connection, Connection::Connected { .. }) {
            // The lists are emptied by their own events, which come first.
            self.refresh = None;
            self.commands.clear();
            self.unseen_finished = false;
        }
    }

    fn apply_changes(&mut self, tab: Tab, changes: &[ListChange<PullRequest>]) {
        for change in changes {
            let list = match tab {
                Tab::Mine => &mut self.mine,
                Tab::Watched => &mut self.watched,
            };
            match change {
                ListChange::Inserted { item, .. } => {
                    self.seen.entry(item.key.clone()).or_insert(item.ci);
                }
                ListChange::Removed { index } => {
                    if let Some(gone) = list.get(*index) {
                        self.seen.remove(&gone.key);
                    }
                }
                _ => {}
            }
            change.apply(list);
            if let ListChange::Reset(items) = change {
                for item in items {
                    self.seen.entry(item.key.clone()).or_insert(item.ci);
                }
                // Forget what neither list holds any more (a disconnect resets to nothing).
                let held: HashSet<&PrKey> = self
                    .mine
                    .iter()
                    .chain(&self.watched)
                    .map(|pr| &pr.key)
                    .collect();
                self.seen.retain(|key, _| held.contains(key));
            }
        }
    }

    fn apply_command(
        &mut self,
        pr: &PrKey,
        action: PrAction,
        attempt: AttemptId,
        state: &CommandState,
    ) -> Applied {
        let known = self.commands.get(pr).map(|command| command.attempt);
        let status = match state {
            // The attempt in a refusal is the one that runs, which has its own `Started`.
            CommandState::Refused => return Applied::Nothing,
            CommandState::Started => {
                // An answer of the same attempt came first, or a newer attempt is known.
                if known.is_some_and(|known| known >= attempt) {
                    return Applied::Nothing;
                }
                CommandStatus::Running
            }
            CommandState::Done => {
                if known.is_some_and(|known| known > attempt) {
                    return Applied::Nothing;
                }
                CommandStatus::Done(self.toast_text(pr, action))
            }
            CommandState::Failed(error) => {
                if known.is_some_and(|known| known > attempt) {
                    return Applied::Nothing;
                }
                CommandStatus::Failed(text::error_text(error))
            }
        };
        self.commands.insert(
            pr.clone(),
            PrCommand {
                action,
                attempt,
                status,
            },
        );
        Applied::Command(pr.clone())
    }

    fn toast_text(&self, key: &PrKey, action: PrAction) -> String {
        let pr = self.find(key).map(|(_, pr)| pr);
        match action {
            PrAction::Merge { .. } => format!("Merged {} #{}", key.repo, key.number),
            PrAction::UpdateBranch => {
                let branch = pr.map_or("the branch", |pr| pr.head_branch.as_str());
                format!("Updated {branch}. The checks run again.")
            }
            PrAction::RerunFailed => "Rerunning the failed jobs".to_owned(),
            PrAction::RerunJob { job_id } => {
                let name = pr
                    .and_then(|pr| pr.jobs.iter().find(|job| job.id == Some(job_id)))
                    .map_or("the job", |job| job.name.as_str());
                format!("Rerunning {name}")
            }
            PrAction::Approve => "Approved the runs".to_owned(),
        }
    }

    // --- what the frontend tells ---

    /// The page is showing (or not). Showing it clears the tile's pip. The frontend tells the
    /// service as well (`GitHub::set_on_screen`).
    pub fn set_on_screen(&mut self, on_screen: bool) {
        self.on_screen = on_screen;
        if on_screen {
            self.unseen_finished = false;
        }
    }

    /// The user opened the pull request: its row is not bold until its CI changes again.
    pub fn select(&mut self, key: &PrKey) {
        if let Some((_, pr)) = self.find(key) {
            let ci = pr.ci;
            self.seen.insert(key.clone(), ci);
        }
    }

    /// The toast or the failure of the pull request's last command was shown or closed.
    pub fn dismiss_command(&mut self, key: &PrKey) {
        if self
            .commands
            .get(key)
            .is_some_and(|command| command.status != CommandStatus::Running)
        {
            self.commands.remove(key);
        }
    }

    // --- what it holds ---

    #[must_use]
    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    #[must_use]
    pub fn login(&self) -> Option<&str> {
        match &self.connection {
            Connection::Connected { login } => Some(login),
            _ => None,
        }
    }

    /// The list in display order, as the service sent it.
    #[must_use]
    pub fn list(&self, tab: Tab) -> &[PullRequest] {
        match tab {
            Tab::Mine => &self.mine,
            Tab::Watched => &self.watched,
        }
    }

    #[must_use]
    pub fn sections(&self, tab: Tab) -> Sections {
        let list = self.list(tab);
        // The list holds the ready pull requests first.
        let ready = list.partition_point(|pr| !pr.is_draft);
        Sections {
            ready,
            drafts: list.len() - ready,
        }
    }

    #[must_use]
    pub fn find(&self, key: &PrKey) -> Option<(Tab, &PullRequest)> {
        fn in_list<'a>(list: &'a [PullRequest], key: &PrKey) -> Option<&'a PullRequest> {
            list.iter().find(|pr| &pr.key == key)
        }
        in_list(&self.mine, key)
            .map(|pr| (Tab::Mine, pr))
            .or_else(|| in_list(&self.watched, key).map(|pr| (Tab::Watched, pr)))
    }

    #[must_use]
    pub fn command(&self, key: &PrKey) -> Option<&PrCommand> {
        self.commands.get(key)
    }

    #[must_use]
    pub fn is_notifying(&self, key: &PrKey) -> bool {
        self.notifying.contains(key)
    }

    // --- views ---

    #[must_use]
    pub fn tile(&self) -> TileView {
        let connected = self.login().is_some();
        let tooltip = if connected {
            format!(
                "{} open, {} failing",
                self.sections(Tab::Mine).ready,
                self.failing
            )
        } else {
            "Connect GitHub to see them".to_owned()
        };
        TileView {
            connected,
            failing: self.failing,
            pip: self.unseen_finished,
            tooltip_title: "Pull requests",
            tooltip,
        }
    }

    #[must_use]
    pub fn header(&self, now: Timestamp) -> HeaderView {
        HeaderView {
            title: "Pull requests",
            status: status_text(&self.connection, self.refresh.as_ref(), now),
            refreshing: matches!(self.refresh, Some(RefreshState::Started)),
            mine_count: self.sections(Tab::Mine).ready,
            watched_count: self.sections(Tab::Watched).ready,
        }
    }

    #[must_use]
    pub fn connect_view(&self) -> ConnectView {
        connect_view(&self.connection)
    }

    /// The row at `index` of a list, built now; `None` past the end.
    #[must_use]
    pub fn row(&self, tab: Tab, index: usize, now: Timestamp) -> Option<PrRowView> {
        let pr = self.list(tab).get(index)?;
        let changed = self.seen.get(&pr.key).is_some_and(|seen| *seen != pr.ci);
        Some(row_view(
            pr,
            tab == Tab::Watched,
            self.notifying.contains(&pr.key),
            changed,
            now,
        ))
    }

    /// The page of a pull request of either list; `None` once it is gone (merged, closed).
    #[must_use]
    pub fn page(&self, key: &PrKey, now: Timestamp) -> Option<PrPageView> {
        let (_, pr) = self.find(key)?;
        let command = self.commands.get(key);
        let ctx = PageContext {
            login: self.login(),
            notifying: self.notifying.contains(key),
            running: command
                .filter(|command| command.status == CommandStatus::Running)
                .map(|command| command.action),
            status: command.map(|command| command.status.clone()),
            now,
        };
        Some(page_view(pr, &ctx))
    }

    #[must_use]
    pub fn runner_lines(&self) -> Vec<RunnerLine> {
        runner_lines(&self.runners)
    }

    /// The desktop notification for a [`Applied::CiEnded`]: title and body.
    #[must_use]
    pub fn ended_text(&self, key: &PrKey, state: CiState) -> (String, String) {
        let title = match state {
            CiState::Success => "Checks passed",
            CiState::Failure => "Checks failed",
            CiState::Pending | CiState::None => "Checks ended",
        };
        let name = self.find(key).map_or("", |(_, pr)| pr.title.as_str());
        (
            title.to_owned(),
            format!("{} #{} {}", key.repo, key.number, name)
                .trim_end()
                .to_owned(),
        )
    }
}
