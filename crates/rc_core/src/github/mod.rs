// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The GitHub pull requests service: connecting to GitHub, polling the user's pull requests and
//! the watched ones, keeping both lists in display order with index-based changes, and the
//! commands on a pull request (merge, update the branch, rerun, approve) and the wish to be told
//! when its CI ends. It runs apart from the Rocket.Chat accounts, with an event callback of its
//! own: see [`GitHub`].

use std::time::Duration;

use secrecy::SecretString;
use tokio::runtime::Handle;
use tokio::sync::mpsc::{self, UnboundedSender};

use crate::list_change::ListChange;

mod actor;
mod domain;
mod list;
mod ports;
#[cfg(test)]
mod tests;

pub use domain::{
    CiState, GithubError, Job, JobState, MergeState, PrKey, PullRequest, RepoRunners,
    ReviewDecision, Runner, RunnerJob, display_order,
};
pub use ports::{GhCli, GhError, GithubApi, GithubAuth, GithubConnector, GithubPorts, GithubStore};
pub use rc_github::DEFAULT_API_URL;

/// How often the lists are fetched while the page is on screen.
pub const POLL_ON_SCREEN: Duration = Duration::from_secs(60);
/// How often they are fetched otherwise, so that the tile's count and the notifications go on.
pub const POLL_BACKGROUND: Duration = Duration::from_mins(5);
/// How old the list of watched repositories may get before a refresh asks for it again. The
/// subscriptions rarely change, and fetching them costs a request per hundred.
pub const REPOS_MAX_AGE: Duration = Duration::from_mins(30);
/// How long after a command the lists are fetched: GitHub's state lags behind a rerun or a rebase.
pub const REFRESH_AFTER_COMMAND: Duration = Duration::from_secs(3);
/// How long a rate limit without a time to wait for holds the polling back.
pub const DEFAULT_RATE_LIMIT_WAIT: Duration = Duration::from_mins(1);

/// What the user has set for the service.
#[derive(Clone, Debug)]
pub struct GithubConfig {
    /// How many days back a watched pull request may have been opened.
    pub watched_days: u32,
    /// The repositories (`owner/name`) whose self-hosted runners are listed. Empty: no runners.
    pub runner_repos: Vec<String>,
}

impl Default for GithubConfig {
    fn default() -> Self {
        Self {
            watched_days: 14,
            runner_repos: Vec::new(),
        }
    }
}

/// Why connecting did not work, for the connect page to say.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectFailure {
    /// GitHub refused the token, pasted or from `gh`.
    BadToken,
    /// There is no `gh` to ask.
    GhMissing,
    /// `gh` is there but has no login.
    GhNotLoggedIn,
    RateLimited,
    Unreachable,
    /// Anything else, with the message to show.
    Other(String),
}

/// Whether GitHub is connected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Connection {
    NotConnected,
    Connecting,
    Connected {
        login: String,
    },
    /// The last try failed. A stored choice is kept: see [`GitHub::retry`].
    Failed(ConnectFailure),
}

/// How the fetching of the lists goes, for the spinner and "updated N s ago".
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RefreshState {
    Started,
    /// Everything was fetched, at that time.
    Done {
        at: jiff::Timestamp,
    },
    /// Something could not be fetched. What was held stays, and `last_good` is when everything
    /// was fetched last (`None` since connecting).
    Failed {
        error: GithubError,
        last_good: Option<jiff::Timestamp>,
    },
}

/// What a command does to a pull request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrAction {
    /// Merges with the rebase method; `force` merges although the checks did not pass.
    Merge {
        force: bool,
    },
    UpdateBranch,
    /// Reruns the failed jobs, not the cancelled ones.
    RerunFailed,
    RerunJob {
        job_id: u64,
    },
    /// Approves the runs that wait for a maintainer.
    Approve,
}

/// Names one run of a command, so that a frontend ignores the late word of an older one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AttemptId(pub(crate) u64);

impl AttemptId {
    /// An id with this number, for the tests of a frontend's cache: the service numbers its own.
    #[must_use]
    pub const fn new(number: u64) -> Self {
        Self(number)
    }
}

impl std::fmt::Display for AttemptId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandState {
    Started,
    Done,
    Failed(GithubError),
    /// Not started: another command runs on the pull request already. The event's attempt is
    /// the one that runs.
    Refused,
}

/// What the service tells the frontend.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum GithubEvent {
    /// Whether GitHub is connected. The first event, and it comes again whenever it changes.
    /// Losing the connection (the token refused, disconnecting, connecting anew) empties the
    /// lists, the count and the rest first.
    Connection(Connection),
    /// Starts and ends every fetch of the lists, the runners included. A failure comes after the
    /// changes that did get through.
    Refresh(RefreshState),
    /// Changes to the user's pull requests, to apply in order: drafts after the others, the latest
    /// change first in each. The first fill is a reset; later ones are inserts, updates (the CI
    /// moved, a job ended), moves (a push, a draft made ready) and removals (merged, closed).
    Mine(Vec<ListChange<PullRequest>>),
    /// The same for the pull requests of the watched repositories that are not the user's.
    Watched(Vec<ListChange<PullRequest>>),
    /// How many of the user's pull requests that are not drafts have failing CI: the tile's red
    /// count. Comes when it changes; losing the connection makes it 0.
    Failing(usize),
    /// The self-hosted runners of the configured repositories, whenever they change.
    Runners(Vec<RepoRunners>),
    /// How a command on a pull request goes. `Started`, then `Done` or `Failed`, with the same
    /// attempt.
    Command {
        pr: PrKey,
        action: PrAction,
        attempt: AttemptId,
        state: CommandState,
    },
    /// The pull requests to tell the user about, sorted, whenever the set changes: a request
    /// adds one, a notification or a pull request that went away drops it.
    Notifying(Vec<PrKey>),
    /// The CI of a pull request in the notify set ended; it leaves the set after this.
    CiEnded { pr: PrKey, state: CiState },
}

pub(crate) enum Command {
    ConnectGh,
    ConnectToken(SecretString),
    Disconnect,
    Retry,
    SetOnScreen(bool),
    Refresh,
    Run {
        pr: PrKey,
        action: PrAction,
    },
    NotifyWhenDone {
        pr: PrKey,
        on: bool,
    },
    /// Answers once everything before it is handled.
    #[cfg(test)]
    Barrier(std::sync::mpsc::Sender<()>),
}

/// The GitHub service, as seen from a frontend: commands go in and never block, events come out
/// through the callback given at creation (see [`Client::github`](crate::Client::github)).
/// Dropping it stops the service.
pub struct GitHub {
    commands: UnboundedSender<Command>,
}

impl GitHub {
    /// Starts the service on `runtime`. It resumes the stored choice, if there is one.
    ///
    /// `on_event` is called once per event, in order, from a runtime thread. It must return
    /// quickly and must not touch GUI objects: a Qt frontend hops to its GUI thread inside it.
    pub(crate) fn start<C, G, S, K>(
        runtime: &Handle,
        ports: GithubPorts<C, G, S, K>,
        config: GithubConfig,
        on_event: impl Fn(GithubEvent) + Send + Sync + 'static,
    ) -> Self
    where
        C: GithubConnector,
        G: GhCli,
        S: GithubStore,
        K: crate::ports::Clock,
    {
        let (commands, receiver) = mpsc::unbounded_channel();
        runtime.spawn(actor::run(
            ports,
            config,
            std::sync::Arc::new(on_event),
            receiver,
        ));
        Self { commands }
    }

    /// Connects with the token of the GitHub CLI (`gh auth token`, read now and again at every
    /// start, never kept). On success the choice is kept for the next start. Replaces a
    /// connection or an attempt under way.
    pub fn connect_with_gh(&self) {
        self.send(Command::ConnectGh);
    }

    /// Connects with a personal access token, which is checked with GitHub and then kept in the
    /// secret store. Replaces a connection or an attempt under way.
    pub fn connect_with_token(&self, token: SecretString) {
        self.send(Command::ConnectToken(token));
    }

    /// Forgets the stored choice and token, drops the lists and goes back to not connected.
    pub fn disconnect(&self) {
        self.send(Command::Disconnect);
    }

    /// Tries the stored choice again, after a failure at startup (GitHub was unreachable). Ignored
    /// unless the connection failed and something is stored.
    pub fn retry(&self) {
        self.send(Command::Retry);
    }

    /// Tells the service whether the GitHub page is showing: it fetches every minute then, and
    /// every five minutes otherwise.
    pub fn set_on_screen(&self, on_screen: bool) {
        self.send(Command::SetOnScreen(on_screen));
    }

    /// Fetches the lists now (after a rate limit's wait, if one holds it back). The list of
    /// watched repositories is fetched anew too. Ignored unless connected.
    pub fn refresh(&self) {
        self.send(Command::Refresh);
    }

    /// Merges the pull request with the rebase method. With `force` although the checks did not
    /// pass. Asking the user first is the frontend's. The answer comes as [`GithubEvent::Command`].
    pub fn merge(&self, pr: PrKey, force: bool) {
        self.run(pr, PrAction::Merge { force });
    }

    /// Rebases the pull request's branch on its base.
    pub fn update_branch(&self, pr: PrKey) {
        self.run(pr, PrAction::UpdateBranch);
    }

    /// Reruns the failed jobs of the pull request, leaving the cancelled ones.
    pub fn rerun_failed(&self, pr: PrKey) {
        self.run(pr, PrAction::RerunFailed);
    }

    /// Reruns one job (`Job::id`) of the pull request.
    pub fn rerun_job(&self, pr: PrKey, job_id: u64) {
        self.run(pr, PrAction::RerunJob { job_id });
    }

    /// Approves the runs of the pull request that wait for a maintainer (`needs_approval`).
    pub fn approve_runs(&self, pr: PrKey) {
        self.run(pr, PrAction::Approve);
    }

    /// Asks to be told when the CI of the pull request ends ([`GithubEvent::CiEnded`]), or takes
    /// that back. Only a pull request of the lists whose CI is running can be added; the others
    /// are ignored.
    pub fn notify_when_done(&self, pr: PrKey, on: bool) {
        self.send(Command::NotifyWhenDone { pr, on });
    }

    /// Runs `action` in the background. All commands refuse a pull request that has one running.
    /// Every command, whatever its outcome, brings a refresh of the lists a few seconds later.
    fn run(&self, pr: PrKey, action: PrAction) {
        self.send(Command::Run { pr, action });
    }

    fn send(&self, command: Command) {
        if self.commands.send(command).is_err() {
            tracing::error!("the GitHub service is gone, command dropped");
        }
    }

    /// Returns once the service has handled everything sent before.
    #[cfg(test)]
    pub(crate) fn barrier(&self) {
        let (done, wait) = std::sync::mpsc::channel();
        self.send(Command::Barrier(done));
        let _ = wait.recv_timeout(Duration::from_secs(5));
    }
}
