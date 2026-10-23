// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The realtime cycle of a session, and the fetches that go with it: rooms, emoji, policy.

use tokio::time::Instant;

use crate::client::{Connection, Event};
use crate::domain::{AvatarOwner, Room, RoomId, room_from};
use crate::emoji::CustomEmojis;
use crate::policy::MessagePolicy;
use crate::ports::{ApiError, Connector, Pushed, Realtime, ServerApi};
use crate::session::SessionState;

use super::{
    Actor, Done, EmojiDone, PolicyDone, RealtimeDone, RealtimeLinks, RealtimeReport, RoomsDone,
};

impl<C: Connector> Actor<C> {
    /// Starts a sync cycle: opens the realtime connection of the session, then (when it is open)
    /// fetches the rooms and, if a room is open, its newest messages. Logging in runs one, and so
    /// does every reconnect. The connection comes before the rooms: what happens between the
    /// fetch and the subscription would be lost otherwise. What the server pushes meanwhile waits
    /// in the backlog.
    ///
    /// A cycle ends when the connection does, or when a fetch fails: the actor waits, with
    /// [`Backoff`], and starts another.
    pub(super) fn open_realtime(&mut self) {
        let Some(session) = &self.session else {
            return;
        };
        self.cycle += 1;
        let cycle = self.cycle;
        self.realtime_open = false;
        self.realtime_watch = None;
        self.realtime_read = None;
        self.succeeded_at = None;
        self.backlog = Some(Vec::new());
        let api = session.api.clone();
        let credentials = session.credentials.clone();
        let done = self.done_tx.clone();
        self.realtime_task = Some(tokio::spawn(async move {
            let report = |done| Done::Realtime(RealtimeReport { cycle, done });
            let Realtime {
                mut events,
                watch,
                read_threads,
            } = match api.realtime(&credentials).await {
                Ok(realtime) => realtime,
                Err(error) => {
                    // Fails only if the actor is gone.
                    let _ = done.send(report(RealtimeDone::Opened(Err(error))));
                    return;
                }
            };
            if done
                .send(report(RealtimeDone::Opened(Ok(RealtimeLinks {
                    watch,
                    read_threads,
                }))))
                .is_err()
            {
                return;
            }
            while let Some(event) = events.recv().await {
                if done
                    .send(report(RealtimeDone::Pushed(Box::new(event))))
                    .is_err()
                {
                    return;
                }
            }
            let _ = done.send(report(RealtimeDone::Ended));
        }));
    }

    pub(super) fn handle_realtime_done(&mut self, report: RealtimeReport) {
        // A cycle that ended is waiting for the next one: its queued reports are moot.
        if report.cycle != self.cycle
            || self.reconnect_task.is_some()
            || !matches!(self.state, SessionState::LoggedIn(_))
        {
            tracing::debug!("report of a stale realtime connection dropped");
            return;
        }
        match report.done {
            RealtimeDone::Opened(Ok(links)) => {
                self.realtime_open = true;
                self.realtime_watch = Some(links.watch);
                self.realtime_read = Some(links.read_threads);
                // A connection that is new hasn't been asked to read anything.
                self.read_requested = None;
                if let Some(open) = &self.open {
                    self.watch_room(Some(open.timeline.room().clone()));
                }
                self.fetch_rooms();
            }
            RealtimeDone::Opened(Err(ApiError::Unauthorized)) => self.expire_session(),
            RealtimeDone::Opened(Err(error)) => {
                tracing::warn!(%error, "can't open the realtime connection");
                self.end_cycle();
                // The first rooms are worth having without live updates.
                if !self.rooms_listed {
                    self.fetch_rooms();
                }
            }
            RealtimeDone::Pushed(event) => match &mut self.backlog {
                Some(backlog) => backlog.push(*event),
                None => self.handle_pushed(*event),
            },
            RealtimeDone::Ended => {
                tracing::warn!("the realtime connection ended");
                self.end_cycle();
            }
        }
    }

    /// Gives up the current cycle: drops the connection and whatever else it was doing, and starts
    /// the wait for the next one. The held rooms and messages stay on show meanwhile. What the
    /// cycle's tasks still had queued is dropped as stale while the wait lasts.
    fn end_cycle(&mut self) {
        self.abort_rooms_task();
        self.abort_realtime_task();
        self.realtime_open = false;
        // Their stops can't reach us any more.
        self.end_typing();
        // A connection that dropped soon after it worked is no better than one that never did.
        if let Some(since) = self.succeeded_at.take()
            && self.backoff.lasted(since.elapsed())
        {
            self.backoff.reset();
        }
        let delay = self.backoff.next_delay();
        tracing::info!(?delay, "reconnecting after a wait");
        self.set_connection(Connection::Waiting { retry_in: delay });
        let done = self.done_tx.clone();
        let cycle = self.cycle;
        self.reconnect_task = Some(tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            // Fails only if the actor is gone.
            let _ = done.send(Done::Reconnect { cycle });
        }));
    }

    pub(super) fn handle_reconnect(&mut self, cycle: u64) {
        if cycle != self.cycle || !matches!(self.state, SessionState::LoggedIn(_)) {
            tracing::debug!("end of a stale wait dropped");
            return;
        }
        self.reconnect_task = None;
        self.set_connection(Connection::Reconnecting);
        self.open_realtime();
    }

    fn set_connection(&mut self, connection: Connection) {
        if self.connection != connection {
            self.connection = connection;
            (self.on_event)(Event::Connection(connection));
        }
    }

    /// Puts what the server pushed into the state and tells the frontend.
    fn handle_pushed(&mut self, event: Pushed) {
        match event {
            Pushed::Subscription(subscription) => {
                let id = RoomId::from(subscription.room_id.clone());
                // The server doesn't push the time of the last message with the subscription.
                let known = self.rooms.get(&id).cloned();
                let known_unread_threads = known.as_ref().map(|room| room.unread_threads.clone());
                match room_from(subscription) {
                    Some(mut room) => {
                        // Nor does it push the avatar or the topic of the room.
                        if let Some(known) = known {
                            room.last_activity = known.last_activity;
                            room.avatar_etag = known.avatar_etag;
                            room.topic = known.topic;
                        }
                        let room = self.seen_if_open(room);
                        let owner = room.avatar_to_fetch();
                        let unread_threads = room.unread_threads.clone();
                        let change = self.rooms.upsert(room, self.clock.now());
                        self.emit_rooms(change.into_iter().collect());
                        let before = known_unread_threads.unwrap_or_default();
                        self.threads_unread_changed(&id, &before, &unread_threads);
                        // A hidden room gets its avatar when it comes back.
                        if self.rooms.is_shown(&id) {
                            self.want_avatars(owner);
                        }
                    }
                    // Hidden now, or not a kind we list.
                    None => self.remove_room(&id),
                }
            }
            Pushed::SubscriptionRemoved(id) => self.remove_room(&id),
            Pushed::RoomChanged {
                room,
                last_message,
                topic,
            } => {
                let now = self.clock.now();
                // A room without a last message (just created) has no activity.
                let moved = last_message.and_then(|at| self.rooms.bump(&room, at, now));
                let retitled = topic.and_then(|topic| self.rooms.set_topic(&room, topic, now));
                self.emit_rooms(moved.into_iter().chain(retitled).collect());
            }
            Pushed::Message(message) => self.add_live_message(message),
            Pushed::MessageDeleted { room, id } => self.delete_message(&room, &id),
            Pushed::Typing {
                room,
                username,
                typing,
                thread,
            } => self.handle_typing(&room, &username, typing, thread),
            // The server announces everybody's: only what was looked at is looked at again, and
            // the rooms of the list, as a room that had no avatar may have one now.
            Pushed::AvatarChanged(owner) => {
                let seen = self.avatars.invalidate(&owner);
                let listed = matches!(&owner, AvatarOwner::Room(id) if self.rooms.is_shown(id));
                if seen || listed {
                    self.want_avatars([owner]);
                }
            }
        }
    }

    /// Fetches the rooms of the current cycle.
    fn fetch_rooms(&mut self) {
        let Some(session) = &self.session else {
            return;
        };
        let cycle = self.cycle;
        let api = session.api.clone();
        let credentials = session.credentials.clone();
        let done = self.done_tx.clone();
        self.rooms_task = Some(tokio::spawn(async move {
            let outcome = api.subscriptions(&credentials).await;
            // Fails only if the actor is gone.
            let _ = done.send(Done::Rooms(RoomsDone { cycle, outcome }));
        }));
    }

    pub(super) fn handle_rooms_done(&mut self, done: RoomsDone) {
        if done.cycle != self.cycle || !matches!(self.state, SessionState::LoggedIn(_)) {
            tracing::debug!("answer of a stale room fetch dropped");
            return;
        }
        self.rooms_task = None;
        let subscriptions = match done.outcome {
            Ok(subscriptions) => subscriptions,
            // The token died between the login and the fetch: same as an expired resume. It's
            // dead already, so there's nothing to revoke.
            Err(ApiError::Unauthorized) => {
                self.expire_session();
                return;
            }
            Err(error) => {
                tracing::warn!(%error, "can't list the rooms");
                // A connection that opened but can't be followed by the rooms is no use: the
                // next cycle starts over. If the cycle ended already (the connection never
                // opened), its wait is under way.
                if self.realtime_open {
                    self.end_cycle();
                }
                return;
            }
        };
        let rooms = subscriptions
            .into_iter()
            .filter_map(room_from)
            .map(|room| self.seen_if_open(room))
            .collect();
        let changes = if self.rooms_listed {
            // Reconnecting: the frontend keeps its rows, and gets what changed meanwhile.
            self.rooms.sync(rooms, self.clock.now())
        } else {
            vec![self.rooms.reset(rooms, self.clock.now())]
        };
        self.rooms_listed = true;
        self.emit_rooms(changes);
        self.want_avatars(
            self.rooms
                .rooms()
                .iter()
                .filter_map(Room::avatar_to_fetch)
                .collect::<Vec<_>>(),
        );
        self.fetch_emoji();
        self.fetch_policy();
        if self.realtime_open {
            self.succeeded_at = Some(Instant::now());
            self.set_connection(Connection::Connected);
            self.catch_up_open_room();
            self.read_open_thread_if_unread();
        }
        for event in self.backlog.take().unwrap_or_default() {
            self.handle_pushed(event);
        }
    }

    /// Fetches the custom emoji of the current cycle. Messages show without them until they come.
    fn fetch_emoji(&mut self) {
        let Some(session) = &self.session else {
            return;
        };
        if let Some(task) = self.emoji_task.take() {
            task.abort();
        }
        let cycle = self.cycle;
        let api = session.api.clone();
        let credentials = session.credentials.clone();
        let done = self.done_tx.clone();
        self.emoji_task = Some(tokio::spawn(async move {
            let outcome = api.custom_emoji(&credentials).await;
            // Fails only if the actor is gone.
            let _ = done.send(Done::Emoji(EmojiDone { cycle, outcome }));
        }));
    }

    pub(super) fn handle_emoji_done(&mut self, done: EmojiDone) {
        let SessionState::LoggedIn(account) = &self.state else {
            return;
        };
        if done.cycle != self.cycle {
            tracing::debug!("answer of a stale custom emoji fetch dropped");
            return;
        }
        self.emoji_task = None;
        let list = match done.outcome {
            Ok(list) => list,
            // The rooms fetch of the cycle meets the same refusal and ends the session.
            Err(error) => {
                tracing::warn!(%error, "can't list the custom emoji");
                return;
            }
        };
        let emoji = CustomEmojis::new(&account.server, &list);
        if emoji != self.emoji {
            self.emoji = emoji.clone();
            (self.on_event)(Event::CustomEmoji(emoji));
        }
    }

    /// Fetches what users may do with their messages in the current cycle. Until it comes, and if
    /// it fails, the policy stays as it was: the server refuses what isn't allowed anyway.
    fn fetch_policy(&mut self) {
        let Some(session) = &self.session else {
            return;
        };
        if let Some(task) = self.policy_task.take() {
            task.abort();
        }
        let cycle = self.cycle;
        let api = session.api.clone();
        let credentials = session.credentials.clone();
        let done = self.done_tx.clone();
        self.policy_task = Some(tokio::spawn(async move {
            let outcome = api.message_settings(&credentials).await;
            // Fails only if the actor is gone.
            let _ = done.send(Done::Policy(PolicyDone { cycle, outcome }));
        }));
    }

    pub(super) fn handle_policy_done(&mut self, done: PolicyDone) {
        if done.cycle != self.cycle || !matches!(self.state, SessionState::LoggedIn(_)) {
            tracing::debug!("answer of a stale message settings fetch dropped");
            return;
        }
        self.policy_task = None;
        match done.outcome {
            Ok(settings) => {
                let policy = MessagePolicy::from(settings);
                if policy != self.policy {
                    self.policy = policy;
                    (self.on_event)(Event::MessagePolicy(policy));
                }
            }
            // The rooms fetch of the cycle meets the same refusal and ends the session.
            Err(error) => tracing::warn!(%error, "can't read the message settings"),
        }
    }

    /// Forgets the custom emoji and tells the frontend, unless there were none.
    pub(super) fn clear_emoji(&mut self) {
        if let Some(task) = self.emoji_task.take() {
            task.abort();
        }
        if self.emoji != CustomEmojis::default() {
            self.emoji = CustomEmojis::default();
            (self.on_event)(Event::CustomEmoji(CustomEmojis::default()));
        }
    }

    /// Resets the message policy to the default and tells the frontend, unless it was that.
    pub(super) fn clear_policy(&mut self) {
        if let Some(task) = self.policy_task.take() {
            task.abort();
        }
        if self.policy != MessagePolicy::default() {
            self.policy = MessagePolicy::default();
            (self.on_event)(Event::MessagePolicy(MessagePolicy::default()));
        }
    }

    /// Fetches the newest messages of the open room again, after the connection was lost: what was
    /// said meanwhile was not pushed. The page joins what is held (see
    /// [`Timeline::merge_latest`]), and the messages pushed from now on wait for it. The edits and
    /// deletions of the messages held are fetched too (see [`fetch_changes`](Self::fetch_changes)).
    /// A room that has left the list is closed instead.
    fn catch_up_open_room(&mut self) {
        let Some(open) = &self.open else {
            return;
        };
        let id = open.timeline.room().clone();
        if self.rooms.get(&id).is_some() {
            self.abort_history_task();
            self.fetch_history(false);
            if self.open_thread.is_some() {
                self.fetch_thread(false);
            }
            self.refresh_thread_list();
        } else {
            self.close_room();
        }
    }

    pub(super) fn abort_rooms_task(&mut self) {
        if let Some(task) = self.rooms_task.take() {
            task.abort();
        }
    }

    /// Ends the realtime connection, if any, and a wait for the next one, and forgets what the
    /// connection pushed that wasn't applied yet.
    pub(super) fn abort_realtime_task(&mut self) {
        self.backlog = None;
        self.realtime_open = false;
        self.realtime_watch = None;
        self.realtime_read = None;
        if let Some(task) = self.realtime_task.take() {
            task.abort();
        }
        if let Some(task) = self.reconnect_task.take() {
            task.abort();
        }
    }
}
