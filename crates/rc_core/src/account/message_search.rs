// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Searching a room's messages on the server (see `docs/design/message-search.md`). It works like
//! the member search: the same pause, the same dropping of stale answers.

use tokio::task::JoinHandle;

use crate::client::Event;
use crate::domain::{AvatarOwner, Message, RoomId};
use crate::message_search::{MESSAGES_PER_SEARCH, MessageSearch, MessageSearchState, More};
use crate::ports::{ApiError, Connector, ServerApi as _};
use crate::session::SessionState;

use super::{Actor, Done};

/// The search of the messages: what the frontend was last told, and the wait and the request that
/// lead to the next thing to tell.
pub(super) struct MessagesSearch {
    /// What the frontend was last told; none when there is no search. Its room and query are the
    /// ones being searched for.
    told: Option<MessageSearch>,
    /// Counts the searches and the cancellations, to tell the answer of the current search from a
    /// stale one.
    attempt: u64,
    /// How long the search waits for the typing to stop.
    debounce: jiff::SignedDuration,
    /// When the request is to be made, while the search waits for the typing to stop.
    due: Option<jiff::Timestamp>,
    /// The request, while it runs.
    task: Option<JoinHandle<()>>,
}

impl MessagesSearch {
    pub(super) fn new(debounce: jiff::SignedDuration) -> Self {
        Self {
            told: None,
            attempt: 0,
            debounce,
            due: None,
            task: None,
        }
    }

    /// When the request is to be made, if the search is waiting.
    pub(super) fn due(&self) -> Option<jiff::Timestamp> {
        self.due
    }
}

/// A message search request's answer.
pub(super) struct MessageSearchDone {
    /// The search the request belongs to: the answer is stale if another has begun since.
    attempt: u64,
    /// How many results the request skipped: 0 for the first page, else the page is appended.
    offset: usize,
    outcome: Result<Vec<Message>, ApiError>,
}

/// The query as it is searched: trimmed, without the `order:` words. The server would sort by
/// `order:asc` oldest first, but the results are kept, paged and grouped newest first, so the
/// order is not the user's to ask for (and the query told to the frontend is this one).
fn clean_query(query: &str) -> String {
    query
        .split_whitespace()
        .filter(|word| {
            !word
                .get(..6)
                .is_some_and(|start| start.eq_ignore_ascii_case("order:"))
        })
        .collect::<Vec<_>>()
        .join(" ")
}

impl<C: Connector> Actor<C> {
    /// Starts a search of `room`'s messages for `query`, or retries one that failed. See
    /// [`AccountHandle::search_messages`](crate::AccountHandle::search_messages).
    pub(super) fn search_messages(&mut self, room: RoomId, query: &str) {
        if !matches!(self.state, SessionState::LoggedIn(_)) {
            tracing::debug!("search_messages ignored: not logged in");
            return;
        }
        let query = clean_query(query);
        if let Some(told) = &self.message_search.told
            && told.room == room
            && told.query == query
        {
            // The same search: the answer is coming, or is here. Only a failure is tried again, at
            // once, as the user asked for it.
            if matches!(told.state, MessageSearchState::Failed(_)) {
                self.begin_message_search(room, query, true);
            }
            return;
        }
        if query.is_empty() {
            // Nothing to look for: the server refuses an empty text.
            self.abort_message_request();
            self.message_search.attempt += 1;
            let state = MessageSearchState::Found {
                messages: Vec::new(),
                more: More::No,
            };
            self.tell_message_search(room, query, state);
            return;
        }
        self.begin_message_search(room, query, false);
    }

    /// Replaces the search by one for `query` in `room`, which asks the server at once or when the
    /// typing has stopped.
    fn begin_message_search(&mut self, room: RoomId, query: String, at_once: bool) {
        self.abort_message_request();
        self.message_search.attempt += 1;
        let at_once = at_once || self.message_search.debounce <= jiff::SignedDuration::ZERO;
        // The pause counts from before the event goes out: whoever hears it may move the clock on.
        if !at_once {
            self.message_search.due = self
                .clock
                .now()
                .checked_add(self.message_search.debounce)
                .ok();
        }
        self.tell_message_search(room, query, MessageSearchState::Searching);
        if at_once {
            self.request_messages(0);
        }
    }

    /// Asks for the next page of the current search, when it has one and none is on its way. See
    /// [`AccountHandle::more_message_search`](crate::AccountHandle::more_message_search).
    pub(super) fn more_message_search(&mut self) {
        let Some(MessageSearch {
            room,
            query,
            state: MessageSearchState::Found { messages, more },
        }) = &self.message_search.told
        else {
            return;
        };
        if !matches!(more, More::Available | More::Failed(_)) {
            return;
        }
        let offset = messages.len();
        let state = MessageSearchState::Found {
            messages: messages.clone(),
            more: More::Loading,
        };
        let (room, query) = (room.clone(), query.clone());
        self.tell_message_search(room, query, state);
        self.request_messages(offset);
    }

    /// Asks the server if the wait for the typing to stop is over.
    pub(super) fn start_due_message_search(&mut self) {
        if self
            .message_search
            .due
            .is_some_and(|due| due <= self.clock.now())
        {
            self.request_messages(0);
        }
    }

    /// Asks the server for a page of results after the first `offset`, in a task that reports the
    /// answer.
    fn request_messages(&mut self, offset: usize) {
        self.message_search.due = None;
        let (Some(session), Some(told)) = (&self.session, &self.message_search.told) else {
            return;
        };
        let attempt = self.message_search.attempt;
        let (room, query) = (told.room.clone(), told.query.clone());
        let api = session.api.clone();
        let credentials = session.credentials.clone();
        let done = self.done_tx.clone();
        self.message_search.task = Some(tokio::spawn(async move {
            let outcome = api
                .search_messages(&credentials, &room, &query, MESSAGES_PER_SEARCH + 1, offset)
                .await;
            // Fails only if the actor is gone.
            let _ = done.send(Done::MessageSearch(MessageSearchDone {
                attempt,
                offset,
                outcome,
            }));
        }));
    }

    pub(super) fn handle_message_search_done(&mut self, done: MessageSearchDone) {
        let Some(told) = self.message_search.told.clone() else {
            return;
        };
        if done.attempt != self.message_search.attempt
            || !matches!(self.state, SessionState::LoggedIn(_))
        {
            tracing::debug!("answer of a stale message search dropped");
            return;
        }
        self.message_search.task = None;
        match done.outcome {
            Ok(mut page) => {
                let more = if page.len() > MESSAGES_PER_SEARCH {
                    More::Available
                } else {
                    More::No
                };
                page.truncate(MESSAGES_PER_SEARCH);
                self.want_avatars(
                    (page.iter()).map(|message| AvatarOwner::User(message.author.username.clone())),
                );
                let mut messages = match told.state {
                    MessageSearchState::Found { messages, .. } if done.offset > 0 => messages,
                    _ => Vec::new(),
                };
                messages.extend(page);
                let state = MessageSearchState::Found { messages, more };
                self.tell_message_search(told.room, told.query, state);
            }
            Err(ApiError::Unauthorized) => self.expire_session(),
            Err(error) => {
                tracing::warn!(%error, room = %told.room, query = told.query, "can't search the messages");
                let error = (&error).into();
                let state = match told.state {
                    // A page after the first: the messages the user reads stay.
                    MessageSearchState::Found { messages, .. } if done.offset > 0 => {
                        MessageSearchState::Found {
                            messages,
                            more: More::Failed(error),
                        }
                    }
                    _ => MessageSearchState::Failed(error),
                };
                self.tell_message_search(told.room, told.query, state);
            }
        }
    }

    /// Tells the frontend what the search holds now, unless it knows.
    fn tell_message_search(&mut self, room: RoomId, query: String, state: MessageSearchState) {
        let search = MessageSearch { room, query, state };
        if self.message_search.told.as_ref() != Some(&search) {
            self.message_search.told = Some(search.clone());
            (self.on_event)(Event::MessageSearch(Some(search)));
        }
    }

    /// Ends the search and the request, and tells the frontend there is none, if there was.
    pub(super) fn cancel_message_search(&mut self) {
        self.abort_message_request();
        self.message_search.attempt += 1;
        if self.message_search.told.take().is_some() {
            (self.on_event)(Event::MessageSearch(None));
        }
    }

    fn abort_message_request(&mut self) {
        self.message_search.due = None;
        if let Some(task) = self.message_search.task.take() {
            task.abort();
        }
    }

    /// Stops the task, for the actor to end.
    pub(super) fn abort_message_search(&mut self) {
        self.abort_message_request();
    }
}
