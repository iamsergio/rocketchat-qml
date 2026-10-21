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

use super::{Actor, Done, Fetch, ThreadDone};

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

    /// Shows the replies of a message of the open room in a thread, and fetches the newest.
    pub(super) fn open_thread(&mut self, parent: &MessageId) {
        if !matches!(self.state, SessionState::LoggedIn(_)) {
            tracing::debug!("open_thread ignored: not logged in");
            return;
        }
        if self
            .open_thread
            .as_ref()
            .is_some_and(|thread| &thread.parent.id == parent && thread.error.is_none())
        {
            return;
        }
        let held = self
            .open
            .as_ref()
            .and_then(|open| open.timeline.message(parent))
            .filter(|message| {
                !message.is_unsent()
                    && message.thread_id.is_none()
                    && matches!(message.kind, MessageKind::Text(_))
            })
            .cloned();
        let Some(message) = held else {
            tracing::debug!(%parent, "open_thread ignored: not a sent text message of the open room");
            return;
        };
        // The reset below tells the frontend; the thread that was open has nothing more to say.
        if let Some(task) = self.thread_task.take() {
            task.abort();
        }
        let timeline = Timeline::new(message.room_id.clone(), RoomHistory::default());
        (self.on_event)(Event::ThreadTimeline(vec![timeline.reset()]));
        self.open_thread = Some(OpenThread {
            parent: message,
            timeline,
            fetch: None,
            error: None,
            live: Vec::new(),
            deleted: HashSet::new(),
        });
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
            started: jiff::Timestamp::now(),
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
        if let Some(task) = self.thread_task.take() {
            task.abort();
        }
        if self.open_thread.take().is_some() {
            (self.on_event)(Event::ThreadTimeline(vec![TimelineChange::Reset {
                room: None,
                messages: Vec::new(),
            }]));
            self.emit_thread_status();
        }
    }
}
