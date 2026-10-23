// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The list of the open room's threads: its pages and live updates.

use crate::client::Event;
use crate::domain::{AvatarOwner, Message, MessageId, MessageKind};
use crate::ports::{ApiError, Connector, ServerApi};
use crate::session::SessionState;
use crate::threads::{ThreadFilter, ThreadList, ThreadListChange, ThreadListStatus};
use crate::timeline::TimelineError;

use super::{Actor, Done, ThreadsListDone};

/// The thread list of the open room and the request for its pages.
pub(super) struct OpenThreadList {
    pub(super) list: ThreadList,
    pub(super) fetch: Option<ListFetch>,
    /// Why the last request failed.
    pub(super) error: Option<TimelineError>,
}

/// A request for a page of the thread list in flight.
pub(super) struct ListFetch {
    /// Identifies the request among the history and thread fetches.
    attempt: u64,
    kind: ListFetchKind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ListFetchKind {
    /// The first page, for a list that has nothing.
    First,
    /// The page after the threads held.
    More,
    /// The first page again, for a list that is shown, to bring it up to date (a reconnect).
    Refresh,
}

/// The owners of the avatars of the authors of the threads.
fn authors_of(threads: &[Message]) -> Vec<AvatarOwner> {
    threads
        .iter()
        .filter(|message| matches!(message.kind, MessageKind::Text(_)))
        .map(|message| AvatarOwner::User(message.author.username.clone()))
        .collect()
}

impl<C: Connector> Actor<C> {
    /// Lists the threads of the open room, from the first page.
    pub(super) fn open_thread_list(&mut self, filter: ThreadFilter) {
        if !matches!(self.state, SessionState::LoggedIn(_)) {
            tracing::debug!("open_thread_list ignored: not logged in");
            return;
        }
        let Some(open) = &self.open else {
            tracing::debug!("open_thread_list ignored: no room is open");
            return;
        };
        let room = open.timeline.room().clone();
        if self.thread_list.as_ref().is_some_and(|open| {
            open.list.room() == &room && open.list.filter() == filter && open.error.is_none()
        }) {
            return;
        }
        if let Some(task) = self.thread_list_task.take() {
            task.abort();
        }
        self.thread_list = Some(OpenThreadList {
            list: ThreadList::new(room, filter),
            fetch: None,
            error: None,
        });
        // The reset tells the frontend that what it shows is gone, whichever filter it was.
        (self.on_event)(Event::ThreadList(vec![ThreadListChange::Reset(Vec::new())]));
        self.fetch_threads_page(ListFetchKind::First);
    }

    /// Fetches the next page of the list.
    pub(super) fn load_more_threads(&mut self) {
        let Some(open) = &self.thread_list else {
            tracing::debug!("load_more_threads ignored: the thread list is not open");
            return;
        };
        // Nothing is held when the first page failed: opening the list again retries that.
        if open.fetch.is_some() || open.list.reached_end() || open.list.rows().is_empty() {
            return;
        }
        self.fetch_threads_page(ListFetchKind::More);
    }

    /// Asks for a page of the open list, and tells the frontend that it is loading.
    fn fetch_threads_page(&mut self, kind: ListFetchKind) {
        let (Some(session), Some(open)) = (&self.session, &mut self.thread_list) else {
            return;
        };
        self.history_attempt += 1;
        let attempt = self.history_attempt;
        open.fetch = Some(ListFetch { attempt, kind });
        open.error = None;
        let offset = if kind == ListFetchKind::More {
            open.list.rows().len()
        } else {
            0
        };
        let room = open.list.room().clone();
        let filter = open.list.filter();
        let count = self.threads_page_size;
        let api = session.api.clone();
        let credentials = session.credentials.clone();
        let done = self.done_tx.clone();
        if let Some(task) = self.thread_list_task.take() {
            task.abort();
        }
        self.thread_list_task = Some(tokio::spawn(async move {
            let outcome = api
                .threads_list(&credentials, &room, filter, offset, count)
                .await;
            // Fails only if the actor is gone.
            let _ = done.send(Done::ThreadsList(ThreadsListDone { attempt, outcome }));
        }));
        // A refresh says nothing about itself: it is not loading, as far as the frontend knows.
        if kind != ListFetchKind::Refresh {
            self.emit_thread_list_status();
        }
    }

    pub(super) fn handle_threads_list_done(&mut self, done: ThreadsListDone) {
        let Some(open) = &mut self.thread_list else {
            tracing::debug!("answer of a thread list request for a closed list dropped");
            return;
        };
        let Some(fetch) = open.fetch.take_if(|fetch| fetch.attempt == done.attempt) else {
            tracing::debug!("answer of a stale thread list request dropped");
            return;
        };
        self.thread_list_task = None;
        let changes = match done.outcome {
            Ok(page) => match fetch.kind {
                ListFetchKind::First => open.list.reset_to(page.threads, page.reached_end),
                ListFetchKind::More => open.list.append(page.threads, page.reached_end),
                // What the list holds is right if it starts with the page: a reset would only
                // take the frontend's place in the list from it.
                ListFetchKind::Refresh => {
                    let rows = open.list.rows();
                    let same = rows.len() >= page.threads.len()
                        && rows[..page.threads.len()] == page.threads[..]
                        && (rows.len() > page.threads.len()
                            || open.list.reached_end() == page.reached_end);
                    if same {
                        Vec::new()
                    } else {
                        open.list.reset_to(page.threads, page.reached_end)
                    }
                }
            },
            Err(ApiError::Unauthorized) => {
                self.expire_session();
                return;
            }
            Err(error) => {
                tracing::warn!(%error, "can't load the thread list");
                // A refresh that fails leaves the list as it is: it was fine a moment ago.
                if fetch.kind != ListFetchKind::Refresh {
                    open.error = Some(TimelineError::from(&error));
                }
                Vec::new()
            }
        };
        if fetch.kind == ListFetchKind::Refresh && changes.is_empty() {
            return;
        }
        self.emit_thread_list_changes(changes);
        self.emit_thread_list_status();
    }

    /// Fetches the first page of the open list again, after the connection was lost: what was
    /// said meanwhile was not pushed. The list changes only if the page differs from what it
    /// starts with, and then it is a reset to the page.
    pub(super) fn refresh_thread_list(&mut self) {
        let Some(open) = &self.thread_list else {
            return;
        };
        // A list that has nothing yet is loading its first page, not refreshing.
        let kind = if open.list.rows().is_empty() {
            ListFetchKind::First
        } else {
            ListFetchKind::Refresh
        };
        self.fetch_threads_page(kind);
    }

    /// Takes in a pushed message: a parent that changed, or that was removed.
    pub(super) fn push_to_thread_list(&mut self, message: &Message) {
        let me = self.own_user().map(|user| user.id.clone());
        let Some(open) = self
            .thread_list
            .as_mut()
            .filter(|open| open.list.room() == &message.room_id)
        else {
            return;
        };
        if message.thread_id.is_some() {
            return;
        }
        // A parent that was deleted while it had replies is pushed as a removed message.
        let changes = if message.is_deleted_parent() {
            open.list.removed(&message.id).into_iter().collect()
        } else {
            open.list.pushed(message, me.as_ref())
        };
        self.emit_thread_list_changes(changes);
    }

    /// The parent with this id was deleted or changed otherwise: the copy the list holds.
    pub(super) fn remove_from_thread_list(&mut self, id: &MessageId) {
        let Some(open) = &mut self.thread_list else {
            return;
        };
        let changes = open.list.removed(id).into_iter().collect();
        self.emit_thread_list_changes(changes);
    }

    /// Puts the server's newer copy of a message into the list, if it holds it.
    pub(super) fn replace_in_thread_list(&mut self, message: &Message) {
        let Some(open) = &mut self.thread_list else {
            return;
        };
        let changes = open.list.replaced(message).into_iter().collect();
        self.emit_thread_list_changes(changes);
    }

    fn emit_thread_list_changes(&mut self, changes: Vec<ThreadListChange>) {
        if changes.is_empty() {
            return;
        }
        let authors = (changes.iter())
            .flat_map(|change| match change {
                ThreadListChange::Reset(all) => all.clone(),
                ThreadListChange::Inserted { item, .. }
                | ThreadListChange::Updated { item, .. }
                | ThreadListChange::Moved { item, .. } => vec![item.clone()],
                _ => Vec::new(),
            })
            .collect::<Vec<_>>();
        (self.on_event)(Event::ThreadList(changes));
        self.want_avatars(authors_of(&authors));
    }

    fn emit_thread_list_status(&self) {
        let status = match &self.thread_list {
            Some(open) => ThreadListStatus {
                room: Some(open.list.room().clone()),
                filter: open.list.filter(),
                loading: open
                    .fetch
                    .as_ref()
                    .is_some_and(|fetch| fetch.kind == ListFetchKind::First),
                loading_more: open
                    .fetch
                    .as_ref()
                    .is_some_and(|fetch| fetch.kind == ListFetchKind::More),
                reached_end: open.list.reached_end(),
                error: open.error.clone(),
            },
            None => ThreadListStatus::default(),
        };
        (self.on_event)(Event::ThreadListStatus(status));
    }

    /// Closes the thread list and tells the frontend, unless it was closed.
    pub(super) fn close_thread_list(&mut self) {
        if let Some(task) = self.thread_list_task.take() {
            task.abort();
        }
        if self.thread_list.take().is_some() {
            (self.on_event)(Event::ThreadList(vec![ThreadListChange::Reset(Vec::new())]));
            self.emit_thread_list_status();
        }
    }
}
