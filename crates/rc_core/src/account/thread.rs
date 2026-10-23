// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The open thread: its timeline, pages and live replies.

use std::collections::HashSet;

use crate::avatars::authors_of;
use crate::client::Event;
use crate::domain::{Message, MessageId, MessageKind, RoomId};
use crate::images::images_of;
use crate::ports::{ApiError, Changes, Connector, RoomHistory, ServerApi};
use crate::session::SessionState;
use crate::timeline::{PAGE_SIZE, ThreadStatus, Timeline, TimelineChange, TimelineError};

use super::{Actor, Done, Fetch, ThreadDone, ThreadParentDone};

/// The thread whose messages the frontend shows beside the open room.
pub(super) struct OpenThread {
    /// The latest copy of the parent, which joins the timeline with the page that reaches the
    /// start, and follows edits, reactions and reply counts meanwhile.
    pub(super) parent: Message,
    /// The parent first, then the replies. Never kept: a thread is fetched on every open.
    pub(super) timeline: Timeline,
    pub(super) fetch: Option<Fetch>,
    /// Why the last fetch failed.
    pub(super) error: Option<TimelineError>,
    /// Replies pushed while the newest ones were still to come, as for the room's.
    pub(super) live: Vec<Message>,
    /// The messages deleted while the thread was open.
    pub(super) deleted: HashSet<MessageId>,
}

/// A thread asked for whose parent the open room doesn't hold, which is on its way.
pub(super) struct PendingThread {
    /// Identifies the fetch of the parent.
    pub(super) attempt: u64,
    pub(super) parent: MessageId,
    /// The parent as pushed while it was on its way, which is newer than the answer.
    pub(super) newer: Option<Message>,
}

/// Whether a thread can be opened from the message: a sent text message that is not a reply.
fn can_start_thread(message: &Message) -> bool {
    !message.is_unsent()
        && message.thread_id.is_none()
        && matches!(message.kind, MessageKind::Text(_))
}

impl OpenThread {
    /// Whether the message is the parent or one of the replies.
    pub(super) fn concerns(&self, message: &Message) -> bool {
        message.id == self.parent.id || message.thread_id.as_ref() == Some(&self.parent.id)
    }

    /// The replies the server confirmed that are held: where the next page starts.
    pub(super) fn replies_held(&self) -> usize {
        self.timeline
            .messages()
            .iter()
            .filter(|m| !m.is_unsent() && m.id != self.parent.id)
            .count()
    }
}

impl<C: Connector> Actor<C> {
    /// Adds a pushed message to the open thread: a reply, or a new copy of the parent.
    pub(super) fn add_live_thread_message(&mut self, message: &Message) {
        if let Some(pending) = (self.pending_thread.as_mut()).filter(|p| p.parent == message.id) {
            pending.newer = Some(message.clone());
        }
        let Some(thread) = self
            .open_thread
            .as_mut()
            .filter(|thread| thread.concerns(message))
        else {
            return;
        };
        if message.id == thread.parent.id {
            // A parent with replies isn't deleted but left as a placeholder, and nothing is
            // announced as deleted: the thread has no parent any more.
            if message.is_deleted_parent() {
                self.close_thread();
                return;
            }
            // The room takes its copy as it takes any message's.
            thread.parent.clone_from(message);
            self.change_thread_timeline(|timeline| {
                timeline.replace(message.clone()).into_iter().collect()
            });
            return;
        }
        let confirms = thread
            .timeline
            .message(&message.id)
            .is_some_and(Message::is_unsent);
        let newest_to_come = thread.fetch.as_ref().is_some_and(|fetch| !fetch.older)
            || (!thread.timeline.has_sent() && !thread.timeline.reached_start());
        if newest_to_come {
            thread.live.retain(|m| m.id != message.id);
            thread.live.push(message.clone());
            if !confirms {
                return;
            }
        }
        self.change_thread_timeline(|timeline| timeline.merge_live(message.clone()));
    }

    /// Puts what the catch-up found into the open thread: the new copies of the parent and of the
    /// replies it holds, and the replies deleted. The parent's copy has the count the server has
    /// now, so a deleted reply is not counted again.
    pub(super) fn apply_changes_to_thread(&mut self, room: &RoomId, changes: Changes) {
        let Some(thread) = self
            .open_thread
            .as_mut()
            .filter(|thread| thread.timeline.room() == room)
        else {
            return;
        };
        thread.deleted.extend(changes.deleted.iter().cloned());
        let mut applied = Vec::new();
        for message in changes.updated {
            if !thread.concerns(&message) || thread.deleted.contains(&message.id) {
                continue;
            }
            if message.id == thread.parent.id {
                thread.parent = message.clone();
            }
            applied.extend(thread.timeline.replace(message));
        }
        for id in &changes.deleted {
            applied.extend(thread.timeline.remove(id));
        }
        self.emit_thread_changes(applied);
    }

    /// Shows the replies of a message of the open room in a thread, and fetches the newest. A
    /// parent that the room doesn't hold (it has left the window, or the frontend knows it from a
    /// list) is fetched first, and the thread opens when it arrives.
    pub(super) fn open_thread(&mut self, parent: &MessageId) {
        if !matches!(self.state, SessionState::LoggedIn(_)) {
            tracing::debug!("open_thread ignored: not logged in");
            return;
        }
        // The last thing asked for wins, also over a parent still on its way.
        self.cancel_pending_thread();
        if self
            .open_thread
            .as_ref()
            .is_some_and(|thread| &thread.parent.id == parent && thread.error.is_none())
        {
            return;
        }
        let Some(open) = &self.open else {
            tracing::debug!(%parent, "open_thread ignored: no room is open");
            return;
        };
        match open.timeline.message(parent) {
            Some(held) if can_start_thread(held) => self.start_thread(held.clone()),
            Some(_) => {
                tracing::debug!(%parent, "open_thread ignored: not a sent text message");
            }
            None => self.fetch_thread_parent(parent),
        }
    }

    /// Fetches a message that the open room doesn't hold, to open its thread.
    fn fetch_thread_parent(&mut self, parent: &MessageId) {
        let Some(session) = &self.session else {
            return;
        };
        self.history_attempt += 1;
        let attempt = self.history_attempt;
        self.pending_thread = Some(PendingThread {
            attempt,
            parent: parent.clone(),
            newer: None,
        });
        let id = parent.clone();
        let api = session.api.clone();
        let credentials = session.credentials.clone();
        let done = self.done_tx.clone();
        self.parent_task = Some(tokio::spawn(async move {
            let outcome = api.message(&credentials, &id).await;
            // Fails only if the actor is gone.
            let _ = done.send(Done::ThreadParent(ThreadParentDone { attempt, outcome }));
        }));
    }

    /// Opens the thread whose parent was fetched, if it still can be: the answer of a thread that
    /// was asked for since, or of one from a room that is closed, is dropped. The ones that
    /// can't be opened (a message of another room or a reply, one that isn't there) leave
    /// whatever is shown as it is.
    pub(super) fn handle_thread_parent_done(&mut self, done: ThreadParentDone) {
        let Some(pending) = self
            .pending_thread
            .take_if(|pending| pending.attempt == done.attempt)
        else {
            tracing::debug!("answer of a stale fetch of a thread's parent dropped");
            return;
        };
        self.parent_task = None;
        let message = match done.outcome {
            // A parent pushed meanwhile is newer than the answer, as for a page.
            Ok(message) => pending.newer.unwrap_or(message),
            Err(ApiError::Unauthorized) => {
                self.expire_session();
                return;
            }
            Err(error @ ApiError::Server { .. }) => {
                tracing::debug!(%error, parent = %pending.parent, "open_thread ignored: no such message");
                return;
            }
            Err(error) => {
                tracing::warn!(%error, parent = %pending.parent, "can't fetch the thread's parent");
                return;
            }
        };
        let in_open_room = self
            .open
            .as_ref()
            .is_some_and(|open| open.timeline.room() == &message.room_id);
        if message.id != pending.parent || !in_open_room || !can_start_thread(&message) {
            tracing::debug!(
                parent = %pending.parent,
                "open_thread ignored: not a sent text message of the open room"
            );
            return;
        }
        self.start_thread(message);
    }

    /// Forgets the parent being fetched, if any.
    pub(super) fn cancel_pending_thread(&mut self) {
        self.pending_thread = None;
        if let Some(task) = self.parent_task.take() {
            task.abort();
        }
    }

    /// Opens the thread of `parent`, a message of the open room that `can_start_thread`.
    fn start_thread(&mut self, parent: Message) {
        // The reset below tells the frontend; the thread that was open has nothing more to say.
        if let Some(task) = self.thread_task.take() {
            task.abort();
        }
        let timeline = Timeline::new(parent.room_id.clone(), RoomHistory::default());
        (self.on_event)(Event::ThreadTimeline(vec![timeline.reset()]));
        self.open_thread = Some(OpenThread {
            parent,
            timeline,
            fetch: None,
            error: None,
            live: Vec::new(),
            deleted: HashSet::new(),
        });
        // Typists in this thread show at once, and the old thread's are told to be gone.
        self.tell_typing();
        self.fetch_thread(false);
    }

    pub(super) fn load_older_thread(&mut self) {
        let Some(thread) = &self.open_thread else {
            tracing::debug!("load_older_thread ignored: no thread is open");
            return;
        };
        // Nothing is held when the first fetch failed: opening the thread again retries that.
        if thread.fetch.is_some() || thread.timeline.reached_start() || thread.replies_held() == 0 {
            return;
        }
        self.fetch_thread(true);
    }

    /// Fetches a page of the open thread's replies: the newest, or the one after the replies held.
    /// The server counts from the newest, and the replies that arrived since shift it, which the
    /// timeline absorbs by de-duplicating.
    pub(super) fn fetch_thread(&mut self, older: bool) {
        let (Some(session), Some(thread)) = (&self.session, &mut self.open_thread) else {
            return;
        };
        self.history_attempt += 1;
        let attempt = self.history_attempt;
        thread.fetch = Some(Fetch {
            attempt,
            older,
            started: self.clock.now(),
            with_changes: false,
        });
        thread.error = None;
        let offset = if older { thread.replies_held() } else { 0 };
        let parent = thread.parent.id.clone();
        let api = session.api.clone();
        let credentials = session.credentials.clone();
        let done = self.done_tx.clone();
        if let Some(task) = self.thread_task.take() {
            task.abort();
        }
        self.thread_task = Some(tokio::spawn(async move {
            let outcome = api
                .thread_messages(&credentials, &parent, offset, PAGE_SIZE)
                .await;
            // Fails only if the actor is gone.
            let _ = done.send(Done::Thread(ThreadDone {
                attempt,
                parent,
                outcome,
            }));
        }));
        self.emit_thread_status();
    }

    pub(super) fn handle_thread_done(&mut self, done: ThreadDone) {
        let Some(thread) = &mut self.open_thread else {
            tracing::debug!("answer of a thread fetch for a closed thread dropped");
            return;
        };
        let Some(fetch) = thread.fetch.take_if(|fetch| fetch.attempt == done.attempt) else {
            tracing::debug!("answer of a stale thread fetch dropped");
            return;
        };
        self.thread_task = None;
        let changes = match done.outcome {
            Ok(page) => {
                let mut messages = page.messages;
                messages.retain(|m| !thread.deleted.contains(&m.id));
                // The parent is the first row, and comes with the page that reaches the start.
                if page.reached_start {
                    messages.push(thread.parent.clone());
                }
                if fetch.older {
                    thread.timeline.merge_older(messages, page.reached_start)
                } else {
                    // What was pushed meanwhile is newer than the page's copy of a message.
                    messages.retain(|m| thread.live.iter().all(|live| live.id != m.id));
                    messages.append(&mut thread.live);
                    thread.timeline.merge_latest(messages, page.reached_start)
                }
            }
            Err(ApiError::Unauthorized) => {
                self.expire_session();
                return;
            }
            Err(error) => {
                tracing::warn!(%error, thread = %done.parent, "can't load the thread");
                thread.error = Some(TimelineError::from(&error));
                Vec::new()
            }
        };
        self.emit_thread_changes(changes);
        self.emit_thread_status();
    }

    /// Changes the open thread's messages, which the frontend hears about. Nothing is kept: a
    /// thread is fetched on every open.
    pub(super) fn change_thread_timeline(
        &mut self,
        change: impl FnOnce(&mut Timeline) -> Vec<TimelineChange>,
    ) {
        let Some(thread) = &mut self.open_thread else {
            return;
        };
        let changes = change(&mut thread.timeline);
        self.emit_thread_changes(changes);
    }

    fn emit_thread_changes(&mut self, changes: Vec<TimelineChange>) {
        if changes.is_empty() {
            return;
        }
        let authors = authors_of(&changes);
        let images = images_of(&changes);
        (self.on_event)(Event::ThreadTimeline(changes));
        self.want_avatars(authors);
        self.want_images(images);
    }

    fn emit_thread_status(&self) {
        let status = match &self.open_thread {
            Some(thread) => ThreadStatus {
                parent: Some(thread.parent.id.clone()),
                loading: thread.fetch.as_ref().is_some_and(|f| !f.older),
                loading_older: thread.fetch.as_ref().is_some_and(|f| f.older),
                reached_start: thread.timeline.reached_start(),
                error: thread.error.clone(),
            },
            None => ThreadStatus::default(),
        };
        (self.on_event)(Event::ThreadStatus(status));
    }

    /// Closes the open thread and tells the frontend, unless no thread was open.
    pub(super) fn close_thread(&mut self) {
        self.cancel_pending_thread();
        if let Some(task) = self.thread_task.take() {
            task.abort();
        }
        if self.open_thread.take().is_some() {
            (self.on_event)(Event::ThreadTimeline(vec![TimelineChange::Reset {
                room: None,
                messages: Vec::new(),
            }]));
            self.emit_thread_status();
            self.tell_typing();
        }
    }
}
