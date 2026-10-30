// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The open room: its timeline, history, catch-up and live messages.

use std::collections::HashSet;
use std::time::Duration;

use crate::avatars::authors_of;
use crate::client::Event;
use crate::domain::{Message, MessageId, Room, RoomId};
use crate::images::images_of;
use crate::ports::{ApiError, Connector, MessageWindow, NewerPage, Page, ServerApi};
use crate::session::SessionState;
use crate::timeline::{Found, FoundState, Timeline, TimelineChange, TimelineError, TimelineStatus};

use super::{Actor, ChangesDone, Done, Fetch, FetchKind, Fetched, HistoryDone, OpenRoom};

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
        if (self.open.as_ref())
            .and_then(|open| open.found.as_ref())
            .is_some_and(|found| &found.message == id)
        {
            self.clear_found();
        }
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
    pub(super) fn add_live_message(&mut self, message: &Message) {
        self.message_ends_typing(message);
        self.push_to_thread_list(message);
        self.add_live_thread_message(message);
        if message.shows_in_room() {
            self.add_live_room_message(message);
        }
    }

    /// Adds a pushed message to the open room.
    fn add_live_room_message(&mut self, message: &Message) {
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
                    timeline.settle(message.clone()).into_iter().collect()
                });
            }
            return;
        };
        let confirms = open
            .timeline
            .message(&message.id)
            .is_some_and(Message::is_unsent);
        let newest_to_come = open
            .fetch
            .as_ref()
            .is_some_and(|fetch| fetch.kind == FetchKind::Newest)
            || (!open.timeline.has_sent() && !open.timeline.reached_start());
        if newest_to_come {
            // A later copy of a message replaces the one waiting.
            open.live.retain(|m| m.id != message.id);
            open.live.push(message.clone());
            if !confirms {
                return;
            }
        }
        self.change_timeline(&room, |timeline| timeline.merge_live(message.clone()));
    }

    /// The room the user may open: one of the room list, or the channel being previewed.
    fn openable(&self, id: &RoomId) -> Option<&Room> {
        self.rooms
            .get(id)
            .or_else(|| self.preview.as_ref().filter(|preview| &preview.id == id))
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
        let Some(kind) = self.openable(&id).map(|room| room.kind) else {
            tracing::debug!(room = %id, "open_room ignored: not in the room list");
            return;
        };
        if self
            .preview
            .as_ref()
            .is_some_and(|preview| preview.id != id)
        {
            self.end_preview();
        }
        self.cancel_preview_request();
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
            found: None,
            newer_to: None,
        });
        self.end_typing();
        self.track_open_room(Some(id.clone()));
        self.watch_room(Some(id));
        self.mark_open_room_read();
        self.fetch_history(FetchKind::Newest);
    }

    /// The user is looking at the open room: nothing in it is unread. The server is told even if
    /// the list shows it read already, as the counts may lag behind.
    fn mark_open_room_read(&mut self) {
        let Some(open) = &self.open else {
            return;
        };
        let id = open.timeline.room().clone();
        // A previewed channel has no subscription to mark read.
        let Some(room) = self.rooms.get(&id) else {
            return;
        };
        self.mark_read(id.clone());
        if !room.has_unread() {
            return;
        }
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
        self.fetch_history(FetchKind::Older);
    }

    /// Fetches a page of the open room: the newest, the one before the oldest message held, the
    /// one after the newest message held, or the window around a message.
    pub(super) fn fetch_history(&mut self, kind: FetchKind) {
        let (Some(session), Some(open)) = (&self.session, &mut self.open) else {
            return;
        };
        self.history_attempt += 1;
        let attempt = self.history_attempt;
        let started = self.clock.now();
        // Only the newest page of a room that was up to date once needs the catch-up.
        let with_changes = kind == FetchKind::Newest && open.timeline.history().synced_at.is_some();
        open.fetch = Some(Fetch {
            attempt,
            kind: kind.clone(),
            started,
            with_changes,
            resume_newest: false,
        });
        open.error = None;
        let before = if kind == FetchKind::Older {
            open.timeline.oldest()
        } else {
            None
        };
        // The newest message is asked for again, and de-duplicated: messages sent in the same
        // millisecond as it may come after it.
        let after = (open.timeline.newest().max(open.newer_to))
            .and_then(|newest| newest.checked_sub(Duration::from_millis(1)).ok());
        let room = open.timeline.room().clone();
        let room_kind = open.kind;
        let page_size = self.page_size;
        let api = session.api.clone();
        let credentials = session.credentials.clone();
        let done = self.done_tx.clone();
        self.history_task = Some(tokio::spawn(async move {
            let outcome = match kind {
                FetchKind::Newest | FetchKind::Older => api
                    .history(&credentials, &room, room_kind, before, page_size)
                    .await
                    .map(Fetched::Page),
                FetchKind::Newer => api
                    .newer(&credentials, &room, after, page_size)
                    .await
                    .map(Fetched::Newer),
                FetchKind::Around(message) => api
                    .around(&credentials, &room, &message, page_size)
                    .await
                    .map(Fetched::Window),
            };
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
        let for_deleted = changes.deleted.clone();
        // Messages that aren't held come with the newest page.
        let updated: Vec<_> = changes
            .updated
            .iter()
            .filter(|m| m.shows_in_room() && !open.deleted.contains(&m.id))
            .collect();
        for message in &updated {
            applied.extend(timeline.replace((*message).clone()));
        }
        for id in &changes.deleted {
            applied.extend(timeline.remove(id));
        }
        timeline.set_synced_at(done.started);
        if timeline.is_detached() {
            // The window isn't saved, so the copy that is gets the changes too.
            let mut stored = Timeline::new(done.room.clone(), self.messages.load(&done.room));
            for message in updated {
                stored.replace(message.clone());
            }
            for id in &changes.deleted {
                stored.remove(id);
            }
            stored.set_synced_at(done.started);
            self.messages.save(&done.room, stored.history());
        }
        open.persist(&mut *self.messages);
        if !applied.is_empty() {
            let authors = authors_of(&applied);
            let images = images_of(&applied);
            (self.on_event)(Event::Timeline(applied));
            self.want_avatars(authors);
            self.want_images(images);
        }
        if (self.open.as_ref())
            .and_then(|open| open.found.as_ref())
            .is_some_and(|found| for_deleted.contains(&found.message))
        {
            self.clear_found();
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
        self.history_task = None;
        let fetched = match done.outcome {
            Ok(fetched) => fetched,
            Err(ApiError::Unauthorized) => {
                self.expire_session();
                return;
            }
            Err(error) => {
                tracing::warn!(%error, room = %done.room, "can't load the messages");
                open.error = Some(TimelineError::from(&error));
                // Nothing was found, and "missing" would be a claim about the message.
                if matches!(fetch.kind, FetchKind::Around(_)) {
                    open.found = None;
                }
                self.emit_timeline_status();
                return;
            }
        };
        match (fetch.kind.clone(), fetched) {
            (FetchKind::Newest | FetchKind::Older, Fetched::Page(page)) => {
                self.page_arrived(&fetch, page);
            }
            (FetchKind::Around(target), Fetched::Window(window)) => {
                self.window_arrived(&fetch, target, window);
            }
            (FetchKind::Newer, Fetched::Newer(page)) => self.newer_arrived(page),
            _ => tracing::warn!(room = %done.room, "a fetch got another kind of answer"),
        }
    }

    /// Takes in the newest page, or the one before the oldest message held.
    fn page_arrived(&mut self, fetch: &Fetch, page: Page) {
        let Some(open) = &mut self.open else {
            return;
        };
        let mut messages = page.messages;
        messages.retain(|m| !open.deleted.contains(&m.id));
        let changes = if fetch.kind == FetchKind::Older {
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
        open.persist(&mut *self.messages);
        let (mut authors, mut images) = (Vec::new(), Vec::new());
        if !changes.is_empty() {
            authors = authors_of(&changes);
            images = images_of(&changes);
            (self.on_event)(Event::Timeline(changes));
        }
        self.want_avatars(authors);
        self.want_images(images);
        self.emit_timeline_status();
    }

    /// Takes in the window around the message that `show_message` asked for.
    fn window_arrived(&mut self, fetch: &Fetch, target: MessageId, window: Option<MessageWindow>) {
        let Some(open) = &mut self.open else {
            return;
        };
        let wanted = open
            .found
            .as_ref()
            .is_some_and(|found| found.message == target && found.state == FoundState::Loading);
        let window = window.map(|mut window| {
            window.messages.retain(|m| !open.deleted.contains(&m.id));
            window
        });
        // The target is deleted, or something else was asked for since, or sending cleared it.
        let usable = wanted
            && window
                .as_ref()
                .is_some_and(|window| window.messages.iter().any(|m| m.id == target));
        if !usable {
            if wanted {
                // A message the server doesn't have, or a reply in a thread that the room doesn't
                // show, which isn't in a window of the room (MSG-008, step 3e).
                open.found = Some(Found {
                    message: target,
                    state: FoundState::Missing,
                });
            }
            self.emit_timeline_status();
            self.resume_newest(fetch);
            return;
        }
        let Some(window) = window else {
            return;
        };
        let room = open.timeline.room().clone();
        open.newer_to = None;
        let changes =
            open.timeline
                .merge_window(window.messages, window.more_before, window.more_after);
        open.found = Some(Found {
            message: target.clone(),
            state: FoundState::Shown,
        });
        // Saves nothing while the window is detached.
        open.persist(&mut *self.messages);
        let authors = authors_of(&changes);
        let images = images_of(&changes);
        // The status first: a frontend that sticks to the bottom must know not to.
        self.emit_timeline_status();
        (self.on_event)(Event::Timeline(changes));
        self.want_avatars(authors);
        self.want_images(images);
        (self.on_event)(Event::ShowMessage {
            room,
            message: target,
        });
        self.resume_newest(fetch);
    }

    /// Fetches the newest messages that a fetch cut short, if the timeline is attached: a
    /// detached one is brought up to date by showing the latest.
    fn resume_newest(&mut self, fetch: &Fetch) {
        if fetch.resume_newest
            && self
                .open
                .as_ref()
                .is_some_and(|o| !o.timeline.is_detached())
        {
            self.fetch_history(FetchKind::Newest);
        }
    }

    /// Takes in the page after the newest message of a detached window. A page that reaches the
    /// end of the room, or a message the stored copy holds, joins the window to the newest
    /// messages: the stored copy's messages after it come along, and the newest page is fetched,
    /// with the catch-up, as opening the room does.
    fn newer_arrived(&mut self, page: NewerPage) {
        let Some(open) = &mut self.open else {
            return;
        };
        let mut messages = page.messages;
        messages.retain(|m| !open.deleted.contains(&m.id));
        let mut joins = page.reached_end;
        if !joins {
            let stored = self.messages.load(open.timeline.room());
            let held: HashSet<_> = stored.messages.iter().map(|m| &m.id).collect();
            joins = messages.iter().any(|m| held.contains(&m.id));
            if joins {
                // Not the end of the room: what the stored copy holds after the page is the
                // rest of what the room had when it was left.
                let reached = messages
                    .last()
                    .map_or(open.timeline.newest(), |m| Some(m.ts));
                messages.extend(
                    stored
                        .messages
                        .iter()
                        .filter(|m| m.is_unsent() || reached.is_none_or(|ts| m.ts > ts))
                        .filter(|m| !open.deleted.contains(&m.id))
                        .cloned(),
                );
            }
        }
        let changes = open.timeline.merge_newer(messages);
        open.newer_to = open.newer_to.max(page.last);
        if joins {
            open.timeline.attach();
            open.newer_to = None;
        }
        open.persist(&mut *self.messages);
        let authors = authors_of(&changes);
        let images = images_of(&changes);
        if !changes.is_empty() {
            (self.on_event)(Event::Timeline(changes));
        }
        self.want_avatars(authors);
        self.want_images(images);
        if joins {
            self.fetch_history(FetchKind::Newest);
        } else {
            self.emit_timeline_status();
        }
    }

    /// Shows a message of the open room (see [`AccountHandle::show_message`]).
    ///
    /// [`AccountHandle::show_message`]: crate::client::AccountHandle::show_message
    pub(super) fn show_message(&mut self, id: MessageId) {
        let Some(open) = &mut self.open else {
            tracing::debug!("show_message ignored: no room is open");
            return;
        };
        let room = open.timeline.room().clone();
        let held = open.timeline.message(&id).is_some();
        // The last request wins: an answer to an earlier jump is not wanted any more. The newest
        // page and the catch-up are not given up: they are for the room, not for the jump.
        let aborted = open.fetch.take_if(|fetch| match fetch.kind {
            FetchKind::Around(_) => true,
            FetchKind::Newest => !held,
            FetchKind::Older | FetchKind::Newer => false,
        });
        if aborted.is_some()
            && let Some(task) = self.history_task.take()
        {
            task.abort();
        }
        let resume_newest = aborted
            .as_ref()
            .is_some_and(|fetch| fetch.kind == FetchKind::Newest || fetch.resume_newest);
        if held {
            open.found = Some(Found {
                message: id.clone(),
                state: FoundState::Shown,
            });
            self.emit_timeline_status();
            (self.on_event)(Event::ShowMessage { room, message: id });
            if resume_newest {
                self.fetch_history(FetchKind::Newest);
            }
            return;
        }
        open.found = Some(Found {
            message: id.clone(),
            state: FoundState::Loading,
        });
        self.fetch_history(FetchKind::Around(id));
        if let Some(fetch) = self.open.as_mut().and_then(|open| open.fetch.as_mut()) {
            fetch.resume_newest = resume_newest;
        }
    }

    /// Fetches the page after a detached window (see [`AccountHandle::load_newer`]).
    ///
    /// [`AccountHandle::load_newer`]: crate::client::AccountHandle::load_newer
    pub(super) fn load_newer(&mut self) {
        let Some(open) = &self.open else {
            tracing::debug!("load_newer ignored: no room is open");
            return;
        };
        if open.fetch.is_some() || !open.timeline.is_detached() || open.timeline.newest().is_none()
        {
            return;
        }
        self.fetch_history(FetchKind::Newer);
    }

    /// Replaces a detached window with what is held of the room, and fetches the newest messages.
    pub(super) fn show_latest(&mut self) {
        let Some(open) = &mut self.open else {
            tracing::debug!("show_latest ignored: no room is open");
            return;
        };
        if !open.timeline.is_detached() {
            self.clear_found();
            return;
        }
        if let Some(task) = self.history_task.take() {
            task.abort();
        }
        open.fetch = None;
        open.found = None;
        open.newer_to = None;
        let room = open.timeline.room().clone();
        open.timeline = Timeline::new(room, self.messages.load(open.timeline.room()));
        let reset = open.timeline.reset();
        let authors = authors_of(std::slice::from_ref(&reset));
        let images = images_of(std::slice::from_ref(&reset));
        self.emit_timeline_status();
        (self.on_event)(Event::Timeline(vec![reset]));
        self.want_avatars(authors);
        self.want_images(images);
        self.fetch_history(FetchKind::Newest);
    }

    /// Forgets the message that was marked, and tells the frontend, unless there was none.
    pub(super) fn clear_found(&mut self) {
        if self
            .open
            .as_mut()
            .is_some_and(|open| open.found.take().is_some())
        {
            self.emit_timeline_status();
        }
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
                    && open
                        .fetch
                        .as_ref()
                        .is_some_and(|fetch| fetch.kind == FetchKind::Newest)
            })
        {
            open.live.retain(|m| m.id != message.id);
            open.live.push(message.clone());
        }
        if let Some(thread) = self.open_thread.as_mut().filter(|thread| {
            thread.concerns(message)
                && thread.parent.id != message.id
                && thread
                    .fetch
                    .as_ref()
                    .is_some_and(|fetch| fetch.kind == FetchKind::Newest)
        }) {
            thread.live.retain(|m| m.id != message.id);
            thread.live.push(message.clone());
        }
    }

    /// Changes the messages of `room` and keeps the result: in the open timeline if it is the
    /// room's, which the frontend hears about, else in what the store holds of the room, which
    /// nobody shows. A detached window gets the change and so does the store, which holds the
    /// attached copy.
    pub(super) fn change_timeline(
        &mut self,
        room: &RoomId,
        change: impl Fn(&mut Timeline) -> Vec<TimelineChange>,
    ) {
        if let Some(open) = self
            .open
            .as_mut()
            .filter(|open| open.timeline.room() == room)
        {
            let changes = change(&mut open.timeline);
            if open.timeline.is_detached() {
                // The window isn't saved: the copy that is gets the change on its own.
                let mut stored = Timeline::new(room.clone(), self.messages.load(room));
                if !change(&mut stored).is_empty() {
                    self.messages.save(room, stored.history());
                }
            }
            if !changes.is_empty() {
                open.persist(&mut *self.messages);
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
                loading: open
                    .fetch
                    .as_ref()
                    .is_some_and(|f| f.kind == FetchKind::Newest),
                loading_older: open
                    .fetch
                    .as_ref()
                    .is_some_and(|f| f.kind == FetchKind::Older),
                loading_newer: open
                    .fetch
                    .as_ref()
                    .is_some_and(|f| f.kind == FetchKind::Newer),
                detached: open.timeline.is_detached(),
                found: open.found.clone(),
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
        self.end_preview();
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
