// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The open room: its timeline, history, catch-up and live messages.

use std::collections::HashSet;
use std::time::Duration;

use crate::avatars::authors_of;
use crate::client::Event;
use crate::domain::{Message, MessageId, RoomId};
use crate::images::images_of;
use crate::ports::{ApiError, Connector, ServerApi};
use crate::session::SessionState;
use crate::timeline::{PAGE_SIZE, Timeline, TimelineChange, TimelineError, TimelineStatus};

use super::{Actor, ChangesDone, Done, Fetch, HistoryDone, OpenRoom};

/// How much earlier than the time a room was last up to date its changes are asked for: the
/// server's clock may differ from ours.
const CATCH_UP_MARGIN: Duration = Duration::from_secs(60);

impl<C: Connector> Actor<C> {
    /// Tells the realtime connection, if there is one, which room is open.
    pub(super) fn watch_room(&self, room: Option<RoomId>) {
        if let Some(watch) = &self.realtime_watch {
            // Fails only if the connection is gone, which the actor hears of separately.
            let _ = watch.send(room);
        }
    }

    /// Takes a deleted message out of the room's messages, open or not, and out of the open
    /// thread. A reply that was held counts one less in its parent, as the server's does: it
    /// doesn't push the parent for a deletion. Deleting the parent closes the thread.
    pub(super) fn delete_message(&mut self, room: &RoomId, id: &MessageId) {
        let reply_of = self
            .find_message(id)
            .filter(|held| &held.room_id == room)
            .and_then(|held| held.thread_id.clone());
        if let Some(open) = self
            .open
            .as_mut()
            .filter(|open| open.timeline.room() == room)
        {
            open.live.retain(|m| &m.id != id);
            open.deleted.insert(id.clone());
        }
        self.change_timeline(room, |timeline| timeline.remove(id).into_iter().collect());
        if (self.thread_list.as_ref()).is_some_and(|open| open.list.room() == room) {
            self.remove_from_thread_list(id);
        }
        if self
            .pending_thread
            .as_ref()
            .is_some_and(|pending| &pending.parent == id)
        {
            self.cancel_pending_thread();
        }
        if self
            .open_thread
            .as_ref()
            .is_some_and(|thread| &thread.parent.id == id)
        {
            self.close_thread();
        } else if let Some(thread) = self
            .open_thread
            .as_mut()
            .filter(|thread| thread.timeline.room() == room)
        {
            thread.live.retain(|m| &m.id != id);
            thread.deleted.insert(id.clone());
            self.change_thread_timeline(|timeline| timeline.remove(id).into_iter().collect());
        }
        if let Some(parent) = reply_of {
            self.reply_removed(room, &parent);
        }
    }

    /// Counts one reply less in the held copies of the parent. The time of the last reply stays,
    /// as the server's does.
    fn reply_removed(&mut self, room: &RoomId, parent: &MessageId) {
        // The open thread's parent may be one the room doesn't hold.
        let held = match self.open_thread.as_ref().filter(|t| &t.parent.id == parent) {
            Some(thread) => Some(&thread.parent),
            None => self
                .find_message(parent)
                .or_else(|| (self.thread_list.as_ref()).and_then(|open| open.list.message(parent))),
        };
        let Some(mut copy) = held.cloned() else {
            return;
        };
        copy.thread_replies = copy.thread_replies.saturating_sub(1);
        self.replace_everywhere(room, &copy);
    }

    /// The held message with this id, in the open room or in the open thread.
    pub(super) fn find_message(&self, id: &MessageId) -> Option<&Message> {
        self.open
            .as_ref()
            .and_then(|open| open.timeline.message(id))
            .or_else(|| (self.open_thread.as_ref()).and_then(|thread| thread.timeline.message(id)))
    }

    /// Takes in the server's newer copy of a message that is held as confirmed, wherever it is
    /// held: in the room, in the thread, and as the thread's parent.
    pub(super) fn replace_everywhere(&mut self, room: &RoomId, message: &Message) {
        if (self.thread_list.as_ref()).is_some_and(|open| open.list.room() == room) {
            self.replace_in_thread_list(message);
        }
        if let Some(thread) = self
            .open_thread
            .as_mut()
            .filter(|thread| thread.parent.id == message.id)
        {
            thread.parent.clone_from(message);
        }
        self.change_timelines(room, |timeline| {
            timeline.replace(message.clone()).into_iter().collect()
        });
    }

    /// Adds a pushed message to the open room, or to the open thread, or both. Other rooms aren't
    /// held: they load their history when opened, and their unread counts arrive as subscription
    /// changes.
    pub(super) fn add_live_message(&mut self, message: Message) {
        self.message_ends_typing(&message);
        self.push_to_thread_list(&message);
        self.add_live_thread_message(&message);
        if message.shows_in_room() {
            self.add_live_room_message(message);
        }
    }

    /// Adds a pushed message to the open room.
    fn add_live_room_message(&mut self, message: Message) {
        let room = message.room_id.clone();
        let echo = self.own_user().is_some_and(|me| me.id == message.author.id);
        let Some(open) = self
            .open
            .as_mut()
            .filter(|open| open.timeline.room() == &room)
        else {
            // The echo of a message sent from a room that isn't open any more.
            if echo {
                self.change_timeline(&room, |timeline| {
                    timeline.settle(message).into_iter().collect()
                });
            }
            return;
        };
        let confirms = open
            .timeline
            .message(&message.id)
            .is_some_and(Message::is_unsent);
        let newest_to_come = open.fetch.as_ref().is_some_and(|fetch| !fetch.older)
            || (!open.timeline.has_sent() && !open.timeline.reached_start());
        if newest_to_come {
            // A later copy of a message replaces the one waiting.
            open.live.retain(|m| m.id != message.id);
            open.live.push(message.clone());
            if !confirms {
                return;
            }
        }
        self.change_timeline(&room, |timeline| timeline.merge_live(message));
    }

    /// Shows the room, from what is held of it, and fetches its newest messages.
    pub(super) fn open_room(&mut self, id: RoomId) {
        if !matches!(self.state, SessionState::LoggedIn(_)) {
            tracing::debug!("open_room ignored: not logged in");
            return;
        }
        if self
            .open
            .as_ref()
            .is_some_and(|open| open.timeline.room() == &id && open.error.is_none())
        {
            return;
        }
        let Some(kind) = self.rooms.get(&id).map(|room| room.kind) else {
            tracing::debug!(room = %id, "open_room ignored: not in the room list");
            return;
        };
        self.close_thread();
        self.close_thread_list();
        self.abort_history_task();
        let history = self.messages.load(&id);
        let timeline = Timeline::new(id.clone(), history);
        let reset = timeline.reset();
        let authors = authors_of(std::slice::from_ref(&reset));
        let images = images_of(std::slice::from_ref(&reset));
        (self.on_event)(Event::Timeline(vec![reset]));
        self.want_avatars(authors);
        self.want_images(images);
        self.open = Some(OpenRoom {
            timeline,
            kind,
            fetch: None,
            error: None,
            live: Vec::new(),
            changes: None,
            deleted: HashSet::new(),
        });
        self.end_typing();
        self.track_open_room(Some(id.clone()));
        self.watch_room(Some(id));
        self.mark_open_room_read();
        self.fetch_history(false);
    }

    /// The user is looking at the open room: nothing in it is unread. The server is told even if
    /// the list shows it read already, as the counts may lag behind.
    fn mark_open_room_read(&mut self) {
        let Some(open) = &self.open else {
            return;
        };
        let id = open.timeline.room().clone();
        self.mark_read(id.clone());
        let Some(room) = self.rooms.get(&id).filter(|room| room.has_unread()) else {
            return;
        };
        if let Some(change) = self.rooms.upsert(room.clone().read(), self.clock.now()) {
            self.emit_rooms(vec![change]);
        }
    }

    pub(super) fn load_older(&mut self) {
        let Some(open) = &self.open else {
            tracing::debug!("load_older ignored: no room is open");
            return;
        };
        // Nothing is held when the first fetch failed: opening the room again retries that.
        if open.fetch.is_some() || open.timeline.reached_start() || open.timeline.oldest().is_none()
        {
            return;
        }
        self.fetch_history(true);
    }

    /// Fetches a page of the open room: the newest, or the one before the oldest message held.
    pub(super) fn fetch_history(&mut self, older: bool) {
        let (Some(session), Some(open)) = (&self.session, &mut self.open) else {
            return;
        };
        self.history_attempt += 1;
        let attempt = self.history_attempt;
        let started = self.clock.now();
        // Only the newest page of a room that was up to date once needs the catch-up.
        let with_changes = !older && open.timeline.history().synced_at.is_some();
        open.fetch = Some(Fetch {
            attempt,
            older,
            started,
            with_changes,
        });
        open.error = None;
        let before = if older { open.timeline.oldest() } else { None };
        let room = open.timeline.room().clone();
        let kind = open.kind;
        let api = session.api.clone();
        let credentials = session.credentials.clone();
        let done = self.done_tx.clone();
        self.history_task = Some(tokio::spawn(async move {
            let outcome = api
                .history(&credentials, &room, kind, before, PAGE_SIZE)
                .await;
            // Fails only if the actor is gone.
            let _ = done.send(Done::History(HistoryDone {
                attempt,
                room,
                outcome,
            }));
        }));
        if with_changes {
            self.fetch_changes();
        }
        self.emit_timeline_status();
    }

    /// Asks for what happened to the open room's messages since it was last up to date: the
    /// newest page can't tell of an edit or a deletion further back, and the connection didn't
    /// push those that happened while it was closed or lost. The request starts a minute earlier
    /// than that, as the server's clock may differ from ours; applying a change twice does
    /// nothing.
    pub(super) fn fetch_changes(&mut self) {
        let (Some(session), Some(open)) = (&self.session, &mut self.open) else {
            return;
        };
        let Some(synced_at) = open.timeline.history().synced_at else {
            return;
        };
        if let Some(task) = self.changes_task.take() {
            task.abort();
        }
        self.history_attempt += 1;
        let attempt = self.history_attempt;
        open.changes = Some(attempt);
        let started = self.clock.now();
        let since = synced_at.checked_sub(CATCH_UP_MARGIN).unwrap_or(synced_at);
        let room = open.timeline.room().clone();
        let api = session.api.clone();
        let credentials = session.credentials.clone();
        let done = self.done_tx.clone();
        self.changes_task = Some(tokio::spawn(async move {
            let outcome = api.changes_since(&credentials, &room, since).await;
            // Fails only if the actor is gone.
            let _ = done.send(Done::Changes(ChangesDone {
                attempt,
                room,
                started,
                outcome,
            }));
        }));
    }

    pub(super) fn handle_changes_done(&mut self, done: ChangesDone) {
        let Some(open) = &mut self.open else {
            tracing::debug!("answer of a catch-up for a closed room dropped");
            return;
        };
        if open.timeline.room() != &done.room
            || open.changes.take_if(|a| *a == done.attempt).is_none()
        {
            tracing::debug!("answer of a stale catch-up dropped");
            return;
        }
        self.changes_task = None;
        let changes = match done.outcome {
            Ok(changes) => changes,
            Err(ApiError::Unauthorized) => {
                self.expire_session();
                return;
            }
            Err(error) => {
                tracing::warn!(%error, room = %done.room, "can't catch up with the room's changes");
                return;
            }
        };
        open.deleted.extend(changes.deleted.iter().cloned());
        let timeline = &mut open.timeline;
        let mut applied = Vec::new();
        let for_thread = self.open_thread.is_some().then(|| changes.clone());
        // Messages that aren't held come with the newest page.
        for message in changes.updated {
            if message.shows_in_room() && !open.deleted.contains(&message.id) {
                applied.extend(timeline.replace(message));
            }
        }
        for id in &changes.deleted {
            applied.extend(timeline.remove(id));
        }
        timeline.set_synced_at(done.started);
        self.messages.save(&done.room, timeline.history());
        if !applied.is_empty() {
            let authors = authors_of(&applied);
            let images = images_of(&applied);
            (self.on_event)(Event::Timeline(applied));
            self.want_avatars(authors);
            self.want_images(images);
        }
        if let Some(changes) = for_thread {
            self.apply_changes_to_thread(&done.room, changes);
        }
    }

    pub(super) fn handle_history_done(&mut self, done: HistoryDone) {
        let Some(open) = &mut self.open else {
            tracing::debug!("answer of a history fetch for a closed room dropped");
            return;
        };
        let Some(fetch) = open.fetch.take_if(|fetch| fetch.attempt == done.attempt) else {
            tracing::debug!("answer of a stale history fetch dropped");
            return;
        };
        let older = fetch.older;
        self.history_task = None;
        let mut authors = Vec::new();
        let mut images = Vec::new();
        match done.outcome {
            Ok(page) => {
                let mut messages = page.messages;
                messages.retain(|m| !open.deleted.contains(&m.id));
                let changes = if older {
                    open.timeline.merge_older(messages, page.reached_start)
                } else {
                    // What was pushed meanwhile is newer than the page's copy of a message.
                    messages.retain(|m| open.live.iter().all(|live| live.id != m.id));
                    messages.append(&mut open.live);
                    let changes = open.timeline.merge_latest(messages, page.reached_start);
                    // Without a catch-up, the page is what brought the room up to date.
                    if !fetch.with_changes {
                        open.timeline.set_synced_at(fetch.started);
                    }
                    changes
                };
                self.messages.save(&done.room, open.timeline.history());
                if !changes.is_empty() {
                    authors = authors_of(&changes);
                    images = images_of(&changes);
                    (self.on_event)(Event::Timeline(changes));
                }
            }
            Err(ApiError::Unauthorized) => {
                self.expire_session();
                return;
            }
            Err(error) => {
                tracing::warn!(%error, room = %done.room, "can't load the messages");
                open.error = Some(TimelineError::from(&error));
            }
        }
        self.want_avatars(authors);
        self.want_images(images);
        self.emit_timeline_status();
    }

    /// Changes the held copies of messages of `room`, in the open room (or what the store holds
    /// of it) and in the open thread, if it is that room's. For changes to messages that are held
    /// already: a message that isn't is left alone.
    pub(super) fn change_timelines(
        &mut self,
        room: &RoomId,
        change: impl Fn(&mut Timeline) -> Vec<TimelineChange>,
    ) {
        self.change_timeline(room, &change);
        if self
            .open_thread
            .as_ref()
            .is_some_and(|thread| thread.timeline.room() == room)
        {
            self.change_thread_timeline(&change);
        }
    }

    /// A page of the newest messages may be on its way and predate `message`, which would make it
    /// drop the message: the open room's and the open thread's pages take it in with them.
    pub(super) fn hold_for_page(&mut self, message: &Message) {
        if message.shows_in_room()
            && let Some(open) = self.open.as_mut().filter(|open| {
                open.timeline.room() == &message.room_id
                    && open.fetch.as_ref().is_some_and(|fetch| !fetch.older)
            })
        {
            open.live.retain(|m| m.id != message.id);
            open.live.push(message.clone());
        }
        if let Some(thread) = self.open_thread.as_mut().filter(|thread| {
            thread.concerns(message)
                && thread.parent.id != message.id
                && thread.fetch.as_ref().is_some_and(|fetch| !fetch.older)
        }) {
            thread.live.retain(|m| m.id != message.id);
            thread.live.push(message.clone());
        }
    }

    /// Changes the messages of `room` and keeps the result: in the open timeline if it is the
    /// room's, which the frontend hears about, else in what the store holds of the room, which
    /// nobody shows.
    pub(super) fn change_timeline(
        &mut self,
        room: &RoomId,
        change: impl FnOnce(&mut Timeline) -> Vec<TimelineChange>,
    ) {
        if let Some(open) = self
            .open
            .as_mut()
            .filter(|open| open.timeline.room() == room)
        {
            let changes = change(&mut open.timeline);
            if !changes.is_empty() {
                self.messages.save(room, open.timeline.history());
                let authors = authors_of(&changes);
                let images = images_of(&changes);
                (self.on_event)(Event::Timeline(changes));
                self.want_avatars(authors);
                self.want_images(images);
            }
            return;
        }
        let mut timeline = Timeline::new(room.clone(), self.messages.load(room));
        if !change(&mut timeline).is_empty() {
            self.messages.save(room, timeline.history());
        }
    }

    pub(super) fn abort_history_task(&mut self) {
        if let Some(task) = self.history_task.take() {
            task.abort();
        }
        if let Some(task) = self.changes_task.take() {
            task.abort();
        }
        if let Some(open) = &mut self.open {
            open.changes = None;
        }
    }

    fn emit_timeline_status(&self) {
        let status = match &self.open {
            Some(open) => TimelineStatus {
                room: Some(open.timeline.room().clone()),
                loading: open.fetch.as_ref().is_some_and(|f| !f.older),
                loading_older: open.fetch.as_ref().is_some_and(|f| f.older),
                reached_start: open.timeline.reached_start(),
                error: open.error.clone(),
            },
            None => TimelineStatus::default(),
        };
        (self.on_event)(Event::TimelineStatus(status));
    }

    /// Closes the open room and forgets every held message, and tells the frontend, unless no room
    /// was open.
    pub(super) fn clear_timeline(&mut self) {
        self.messages.clear();
        self.close_room();
    }

    /// Closes the open room, keeping what is held of it, and tells the frontend, unless no room was
    /// open.
    pub(super) fn close_room(&mut self) {
        self.close_thread();
        self.close_thread_list();
        self.abort_history_task();
        self.end_typing();
        if self.open.take().is_some() {
            self.watch_room(None);
            (self.on_event)(Event::Timeline(vec![TimelineChange::Reset {
                room: None,
                messages: Vec::new(),
            }]));
            self.emit_timeline_status();
            self.track_open_room(None);
        }
    }
}
