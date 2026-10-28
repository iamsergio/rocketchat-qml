// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The views around the list: the tile of the rail, the header of the list, the connect page and
//! the runner footer.

use jiff::Timestamp;
use rc_core::github::{Connection, RefreshState, RepoRunners};

use super::text;

/// The GitHub tile at the bottom of the rail.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TileView {
    /// Greyed out while false.
    pub connected: bool,
    /// The red count: the user's pull requests that aren't drafts and fail. 0 shows nothing.
    pub failing: usize,
    /// A pull request the user asked about finished its CI and the page hasn't been looked at
    /// since.
    pub pip: bool,
    /// "Pull requests".
    pub tooltip_title: &'static str,
    /// "N open, M failing" or "Connect GitHub to see them".
    pub tooltip: String,
}

/// The header of the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeaderView {
    /// "Pull requests".
    pub title: &'static str,
    /// "<login>, updated 40 s ago", "updating…", or "<error>, last updated 5 min ago". Empty while
    /// not connected.
    pub status: String,
    /// The refresh button spins.
    pub refreshing: bool,
    /// The "Mine" tab's count: pull requests that aren't drafts.
    pub mine_count: usize,
    /// The "Watched" tab's count: pull requests that aren't drafts.
    pub watched_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectState {
    NotConnected,
    Connecting,
    Failed,
    Connected,
}

/// What the connect page shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectView {
    pub state: ConnectState,
    /// Why connecting failed; empty unless `state` is `Failed`.
    pub reason: String,
}

/// What a runner is doing, which the lamp of its line shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunnerState {
    Busy,
    Idle,
    Offline,
}

/// One line of the runner footer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunnerLine {
    pub repo: String,
    pub name: String,
    pub state: RunnerState,
    /// "Linux runner, busy with <repo>", "idle" or "offline".
    pub status: String,
}

pub(super) fn connect_view(connection: &Connection) -> ConnectView {
    let (state, reason) = match connection {
        Connection::NotConnected => (ConnectState::NotConnected, String::new()),
        Connection::Connecting => (ConnectState::Connecting, String::new()),
        Connection::Connected { .. } => (ConnectState::Connected, String::new()),
        Connection::Failed(failure) => (ConnectState::Failed, text::connect_failure_text(failure)),
    };
    ConnectView { state, reason }
}

pub(super) fn status_text(
    connection: &Connection,
    refresh: Option<&RefreshState>,
    now: Timestamp,
) -> String {
    let Connection::Connected { login } = connection else {
        return String::new();
    };
    match refresh {
        None => login.clone(),
        Some(RefreshState::Done { at }) => {
            format!("{login}, updated {} ago", text::age(now, *at))
        }
        Some(RefreshState::Started) => "updating…".to_owned(),
        Some(RefreshState::Failed { error, last_good }) => {
            let reason = text::error_text(error);
            match last_good {
                Some(at) => format!("{reason}, last updated {} ago", text::age(now, *at)),
                None => reason,
            }
        }
    }
}

pub(super) fn runner_lines(runners: &[RepoRunners]) -> Vec<RunnerLine> {
    runners
        .iter()
        .flat_map(|repo| {
            repo.runners.iter().map(|runner| {
                let (state, status) = if !runner.online {
                    (RunnerState::Offline, "offline".to_owned())
                } else if runner.busy {
                    (
                        RunnerState::Busy,
                        format!(
                            "{} runner, busy with {}",
                            capitalized(&runner.os),
                            repo.repo
                        ),
                    )
                } else {
                    (RunnerState::Idle, "idle".to_owned())
                };
                RunnerLine {
                    repo: repo.repo.clone(),
                    name: runner.name.clone(),
                    state,
                    status,
                }
            })
        })
        .collect()
}

fn capitalized(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}
