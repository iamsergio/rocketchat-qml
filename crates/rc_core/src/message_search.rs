// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The search of a room's messages on the server (see `docs/design/message-search.md`).

use crate::domain::{Message, RoomId};
use crate::ports::ApiError;

/// How many messages a page of the results holds. The request asks for one more, to know whether
/// there are more.
pub const MESSAGES_PER_SEARCH: usize = 20;

/// How long the search waits after the last change of the query before it asks the server, so that
/// typing a word doesn't ask once per letter.
pub(crate) const DEBOUNCE: jiff::SignedDuration = jiff::SignedDuration::from_millis(300);

/// Why the messages couldn't be searched. The texts are meant to be shown as they are.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum MessageSearchError {
    #[error("Can't reach the server")]
    Unreachable,
    /// Any other error, with its message.
    #[error("The server reported an error: {0}")]
    Server(String),
}

impl From<&ApiError> for MessageSearchError {
    fn from(error: &ApiError) -> Self {
        match error {
            ApiError::Unreachable(_) => Self::Unreachable,
            other => Self::Server(other.to_string()),
        }
    }
}

/// What there is after the messages a search found: the footer of the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum More {
    /// The messages held are all there are.
    No,
    /// The server has more: `AccountHandle::more_message_search` asks for the next page.
    Available,
    /// The next page was asked for and is not here yet.
    Loading,
    /// The next page couldn't be fetched. The messages held stay; asking again retries.
    Failed(MessageSearchError),
}

/// How far a message search is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MessageSearchState {
    /// The query was typed and the answer is not here yet: the search waits for the typing to stop
    /// (`Tuning::message_search_debounce`), then asks the server.
    Searching,
    /// The messages that match, newest first, as the server sends them: all the pages asked for so
    /// far. A snapshot: an edit or a deletion afterwards doesn't change it.
    Found {
        messages: Vec<Message>,
        /// Whether the server has more messages that match than these, and how asking for them
        /// goes.
        more: More,
    },
    /// The server couldn't be asked for the first page, or refused. Searching for the same text
    /// again retries. A page after the first that fails is [`More::Failed`] instead.
    Failed(MessageSearchError),
}

/// What the search of a room's messages holds, as the frontend is told.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageSearch {
    pub room: RoomId,
    /// The text searched for, trimmed.
    pub query: String,
    pub state: MessageSearchState,
}
