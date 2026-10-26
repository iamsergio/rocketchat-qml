// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The server's directory: the people and the public channels a search of it finds, and what the
//! user can do with them. The room list's own search stays local; the directory adds to it.

use crate::domain::RoomId;
use crate::ports::ApiError;
use crate::rooms::RoomActionError;

/// How many people, and how many channels, one search asks for. A frontend shows at most 5 of
/// each, and the user's own rooms among the answers are left out of those, so there are enough
/// left.
pub const RESULTS_PER_KIND: usize = 10;

/// How long the search waits after the last change of the text before it asks the server, so that
/// typing a name doesn't ask once per letter.
pub(crate) const DEBOUNCE: jiff::SignedDuration = jiff::SignedDuration::from_millis(300);

/// A user the directory found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Person {
    pub username: String,
    /// The display name, if the user has one.
    pub name: Option<String>,
}

/// A public channel the directory found. The directory does not say whether the user is in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirectoryChannel {
    pub id: RoomId,
    /// What the room list would show: the display name if the channel has one, else its name.
    pub name: String,
    /// What the channel is about; empty for a channel without one.
    pub topic: String,
    /// How many users are in it.
    pub members: u32,
}

/// What a request of the directory found: the first entries, and how many there are in all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found<T> {
    pub items: Vec<T>,
    pub total: u32,
}

/// Why the directory couldn't be searched. The texts are meant to be shown as they are.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DirectoryError {
    #[error("Can't reach the server's directory")]
    Unreachable,
    /// Any other error, with its message.
    #[error("The server's directory reported an error: {0}")]
    Server(String),
}

impl From<&ApiError> for DirectoryError {
    fn from(error: &ApiError) -> Self {
        match error {
            ApiError::Unreachable(_) => Self::Unreachable,
            other => Self::Server(other.to_string()),
        }
    }
}

/// How far a search is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DirectoryState {
    /// The query was typed and the answer is not here yet: the search waits for the typing to stop
    /// (`Tuning::directory_debounce`), then asks the server. Carries no results.
    Searching,
    /// The server answered. Also what the empty query is.
    Ready,
    /// The server couldn't be asked, or refused. Searching for the same text again retries.
    Failed(DirectoryError),
}

/// What the directory search holds, as the frontend is told: the results are the server's, whole,
/// before the user's own rooms are left out (see `rc_view::directory`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Directory {
    /// The text searched for, trimmed. Empty when there is no search.
    pub query: String,
    pub state: DirectoryState,
    /// At most [`RESULTS_PER_KIND`] users matching the query, in the server's order.
    pub people: Vec<Person>,
    /// How many users match in all.
    pub people_total: u32,
    /// At most [`RESULTS_PER_KIND`] public channels matching the query, the one with the most
    /// members first.
    pub channels: Vec<DirectoryChannel>,
    /// How many channels match in all.
    pub channels_total: u32,
}

impl Directory {
    /// No search: no query, no results.
    #[must_use]
    pub const fn cleared() -> Self {
        Self {
            query: String::new(),
            state: DirectoryState::Ready,
            people: Vec::new(),
            people_total: 0,
            channels: Vec::new(),
            channels_total: 0,
        }
    }

    /// A search for `query` that has not been answered.
    #[must_use]
    pub fn searching(query: &str) -> Self {
        Self {
            query: query.to_owned(),
            state: DirectoryState::Searching,
            ..Self::cleared()
        }
    }
}

impl Default for Directory {
    fn default() -> Self {
        Self::cleared()
    }
}

/// Something the user asks of a person or a channel the directory found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DirectoryAction {
    /// Opens the direct message with this username, which the server creates the first time.
    OpenDirect(String),
    /// Shows a public channel the user is not in, as a preview.
    Preview(RoomId),
    /// Joins a public channel.
    Join(RoomId),
}

/// Where a [`DirectoryAction`] is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActionState {
    /// The server is at it.
    Working,
    /// Over, and there is nothing to say: it worked, or it was overtaken (the user opened another
    /// room before a preview came).
    Done,
    /// The server refused or couldn't be reached. The text is meant to be shown as it is, in the
    /// room list's error strip.
    Failed(RoomActionError),
}
