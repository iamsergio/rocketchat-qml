// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The list of the open room's threads: its pages and live updates.

use crate::client::Event;
use crate::domain::{AvatarOwner, Message, MessageId, MessageKind, RoomId};
use crate::ports::{ApiError, Connector, ServerApi};
use crate::session::SessionState;
use crate::threads::{ThreadFilter, ThreadList, ThreadListChange, ThreadListStatus};
use crate::timeline::TimelineError;

use super::{Actor, Done, ThreadsListDone, ThreadsListParentDone};

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
        let unread = self.unread_threads_of(&message.room_id);
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
            open.list.pushed(message, me.as_ref(), &unread)
        };
        self.emit_thread_list_changes(changes);
    }

    /// The threads of the room with replies the user hasn't read, as its subscription says.
    fn unread_threads_of(&self, room: &RoomId) -> Vec<MessageId> {
        (self.rooms.get(room))
            .map(|room| room.unread_threads.clone())
            .unwrap_or_default()
    }

    /// The subscription of a room changed, and with it the threads that are unread: the list of
    /// unread threads loses the ones that were read, and takes in the parents of the ones that
    /// weren't unread before, which it fetches since the push doesn't carry them. The thread that
    /// is open is read, if it isn't.
    pub(super) fn threads_unread_changed(
        &mut self,
        room: &RoomId,
        before: &[MessageId],
        after: &[MessageId],
    ) {
        let unread_list = self.thread_list.as_ref().is_some_and(|open| {
            open.list.room() == room && open.list.filter() == ThreadFilter::Unread
        });
        if unread_list {
            for id in before.iter().filter(|id| !after.contains(id)) {
                self.remove_from_thread_list(id);
            }
            // One the list holds is there already: its row follows the pushes of the parent.
            let held = |id: &MessageId| {
                (self.thread_list.as_ref()).is_some_and(|open| open.list.message(id).is_some())
            };
            let added: Vec<_> = (after.iter())
                .filter(|id| !before.contains(id) && !held(id))
                .collect();
            for id in added {
                self.fetch_listed_parent(id);
            }
        }
        self.read_open_thread_if_unread();
    }

    /// Fetches the parent of a thread that became unread, to put it in the list.
    fn fetch_listed_parent(&mut self, id: &MessageId) {
        let Some(session) = &self.session else {
            return;
        };
        let id = id.clone();
        let api = session.api.clone();
        let credentials = session.credentials.clone();
        let done = self.done_tx.clone();
        self.thread_list_parents.spawn(async move {
            let outcome = api.message(&credentials, &id).await;
            // Fails only if the actor is gone.
            let _ = done.send(Done::ThreadsListParent(ThreadsListParentDone {
                id,
                outcome,
            }));
        });
    }

    pub(super) fn handle_threads_list_parent_done(&mut self, done: ThreadsListParentDone) {
        let message = match done.outcome {
            Ok(message) => message,
            Err(ApiError::Unauthorized) => {
                self.expire_session();
                return;
            }
            Err(error) => {
                tracing::warn!(%error, thread = %done.id, "can't fetch the unread thread's parent");
                return;
            }
        };
        // Still wanted: the list is the unread one of the parent's room, and the thread is unread.
        let unread = self.unread_threads_of(&message.room_id);
        let me = self.own_user().map(|user| user.id.clone());
        let Some(open) = self.thread_list.as_mut().filter(|open| {
            open.list.room() == &message.room_id && open.list.filter() == ThreadFilter::Unread
        }) else {
            tracing::debug!("answer of a fetch for a closed thread list dropped");
            return;
        };
        let changes = open.list.pushed(&message, me.as_ref(), &unread);
        self.emit_thread_list_changes(changes);
    }

    /// Tells the server the open thread is read, if the room's subscription lists it as unread:
    /// opening a thread reads it, and so does a reply that comes while it is open. Once for each
    /// time it is unread; the server answers with the subscription, without the thread.
    pub(super) fn read_open_thread_if_unread(&mut self) {
        let Some(thread) = &self.open_thread else {
            self.read_requested = None;
            return;
        };
        let id = thread.parent.id.clone();
        if !self.unread_threads_of(thread.timeline.room()).contains(&id) {
            if self.read_requested.as_ref() == Some(&id) {
                self.read_requested = None;
            }
            return;
        }
        if self.read_requested.as_ref() == Some(&id) {
            return;
        }
        let Some(read) = &self.realtime_read else {
            tracing::debug!(thread = %id, "the thread is not marked read: no connection");
            return;
        };
        if read.send(id.clone()).is_ok() {
            self.read_requested = Some(id);
        }
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
        self.thread_list_parents.abort_all();
        if self.thread_list.take().is_some() {
            (self.on_event)(Event::ThreadList(vec![ThreadListChange::Reset(Vec::new())]));
            self.emit_thread_list_status();
        }
    }
}
