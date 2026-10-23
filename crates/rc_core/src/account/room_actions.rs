// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Marking rooms read or unread, leaving them, and keeping the room list.

use crate::client::Event;
use crate::domain::{Room, RoomId};
use crate::ports::{ApiError, Connector, ServerApi};
use crate::rooms::{RoomAction, RoomActionError, RoomChange};
use crate::session::SessionState;

use super::{Actor, Done, MarkReadDone, RoomActionDone};

impl<C: Connector> Actor<C> {
    /// The room as the user sees it: nothing is unread in the open room, whatever the server counts.
    /// A message that arrives while the room is open is read at once, so the server is told too.
    pub(super) fn seen_if_open(&self, room: Room) -> Room {
        let is_open = self
            .open
            .as_ref()
            .is_some_and(|open| open.timeline.room() == &room.id);
        if !is_open || !room.has_unread() {
            return room;
        }
        self.mark_read(room.id.clone());
        room.read()
    }

    /// Asks the server to mark the room read. The answer only matters if it fails.
    pub(super) fn mark_read(&self, room: RoomId) {
        let Some(session) = &self.session else {
            return;
        };
        let attempt = self.attempt;
        let api = session.api.clone();
        let credentials = session.credentials.clone();
        let done = self.done_tx.clone();
        tokio::spawn(async move {
            let outcome = api.mark_read(&credentials, &room).await;
            // Fails only if the actor is gone.
            let _ = done.send(Done::MarkRead(MarkReadDone {
                attempt,
                room,
                outcome,
            }));
        });
    }

    pub(super) fn handle_mark_read_done(&mut self, done: MarkReadDone) {
        if done.attempt != self.attempt || !matches!(self.state, SessionState::LoggedIn(_)) {
            tracing::debug!("answer of a stale mark-as-read dropped");
            return;
        }
        match done.outcome {
            Ok(()) => {}
            Err(ApiError::Unauthorized) => self.expire_session(),
            Err(error) => {
                tracing::warn!(%error, room = %done.room, "can't mark the room read");
            }
        }
    }

    /// Answers a command on a room the frontend asked for.
    fn answer_room(&self, room: RoomId, action: RoomAction, outcome: Result<(), RoomActionError>) {
        (self.on_event)(Event::RoomAction {
            room,
            action,
            outcome,
        });
    }

    /// Starts a command on a room, unless the rules refuse it: the room must be in the list, a
    /// direct message can't be left, and the open room, which stays read, can't be marked unread.
    pub(super) fn act_on_room(&mut self, room: RoomId, action: RoomAction) {
        if self.own_user().is_none() {
            tracing::debug!(?action, "room command ignored: not logged in");
            return;
        }
        let is_open = self
            .open
            .as_ref()
            .is_some_and(|open| open.timeline.room() == &room);
        let allowed = self.rooms.get(&room).is_some_and(|held| match action {
            RoomAction::Leave => held.can_leave(),
            RoomAction::MarkUnread => !is_open,
            _ => true,
        });
        if !allowed {
            tracing::debug!(%room, ?action, "refused: not a room the user can do this to");
            self.answer_room(room, action, Err(RoomActionError::NotAllowed(action)));
            return;
        }
        self.request_room_action(room, action);
    }

    /// Asks the server for the command, in a task that reports the outcome.
    fn request_room_action(&self, room: RoomId, action: RoomAction) {
        let (Some(session), Some(held)) = (&self.session, self.rooms.get(&room)) else {
            return;
        };
        let kind = held.kind;
        let api = session.api.clone();
        let credentials = session.credentials.clone();
        let attempt = self.attempt;
        let done = self.done_tx.clone();
        tokio::spawn(async move {
            let outcome = match action {
                RoomAction::Favorite => api.set_favorite(&credentials, &room, true).await,
                RoomAction::Unfavorite => api.set_favorite(&credentials, &room, false).await,
                RoomAction::MarkRead => api.mark_read(&credentials, &room).await,
                RoomAction::MarkUnread => api.mark_unread(&credentials, &room).await,
                RoomAction::Leave => api.leave_room(&credentials, &room, kind).await,
            };
            // Fails only if the actor is gone.
            let _ = done.send(Done::RoomAction(RoomActionDone {
                attempt,
                room,
                action,
                outcome,
            }));
        });
    }

    pub(super) fn handle_room_action_done(&mut self, done: RoomActionDone) {
        if done.attempt != self.attempt || !matches!(self.state, SessionState::LoggedIn(_)) {
            tracing::debug!("answer of a stale room command dropped");
            return;
        }
        match done.outcome {
            Ok(()) => {
                self.apply_room_action(&done.room, done.action);
                self.answer_room(done.room, done.action, Ok(()));
            }
            Err(ApiError::Unauthorized) => self.expire_session(),
            Err(error) => {
                tracing::warn!(%error, room = %done.room, action = ?done.action, "refused");
                self.answer_room(done.room, done.action, Err(RoomActionError::from(&error)));
            }
        }
    }

    /// Puts what the server did in the room list without waiting for its push, which then changes
    /// nothing: the room is taken as it is now, and what is set is set, not toggled. Nothing if
    /// the room has gone meanwhile.
    fn apply_room_action(&mut self, room: &RoomId, action: RoomAction) {
        match action {
            RoomAction::Favorite => self.change_room(room, |held| Room {
                favorite: true,
                ..held
            }),
            RoomAction::Unfavorite => self.change_room(room, |held| Room {
                favorite: false,
                ..held
            }),
            RoomAction::MarkRead => self.change_room(room, Room::read),
            // What the server's push says: one unread message and the alert, the mentions as
            // they are.
            RoomAction::MarkUnread => self.change_room(room, |held| Room {
                unread: held.unread.max(1),
                alert: true,
                ..held
            }),
            RoomAction::Leave => self.remove_room(room),
        }
    }

    /// Tells the frontend the room list changed, and its unread totals and held-back rooms if they
    /// did too. Every path that changes the room list ends here, with no changes if only a
    /// held-back room did: nothing is sent for the changes then, but the held-back rooms are
    /// checked.
    pub(super) fn emit_rooms(&mut self, changes: Vec<RoomChange>) {
        // A room that comes back needs its avatar like one that was just listed.
        let avatars: Vec<_> = changes
            .iter()
            .filter_map(|change| match change {
                RoomChange::Inserted { room, .. } => room.avatar_to_fetch(),
                _ => None,
            })
            .collect();
        if !changes.is_empty() {
            (self.on_event)(Event::Rooms(changes));
        }
        let unread = self.rooms.unread();
        if unread != self.unread {
            self.unread = unread;
            (self.on_event)(Event::Unread(unread));
        }
        self.emit_quiet_rooms();
        self.want_avatars(avatars);
    }

    /// Tells the frontend the held-back rooms if they are not what it was told last. The list says
    /// when they may have changed, so this costs nothing while they didn't. The rooms that joined
    /// the set get the avatar file the cache holds, and no download: nobody looks at them until
    /// the search shows one, and an account may hold hundreds.
    fn emit_quiet_rooms(&mut self) {
        let Some(quiet) = self.rooms.take_quiet_change() else {
            return;
        };
        if quiet == self.quiet {
            return;
        }
        let joined: Vec<_> = quiet
            .iter()
            .filter(|room| !self.quiet.iter().any(|known| known.id == room.id))
            .filter_map(Room::avatar_to_fetch)
            .collect();
        self.quiet.clone_from(&quiet);
        (self.on_event)(Event::QuietRooms(quiet));
        self.show_cached_avatars(joined);
    }

    /// Holds back the direct messages that went quiet, and tells the frontend.
    pub(super) fn hide_quiet_rooms(&mut self) {
        let changes = self.rooms.refresh(self.clock.now());
        if !changes.is_empty() {
            self.emit_rooms(changes);
        }
    }

    /// Records the room the user has open, `None` for none, in the room list: the open room is
    /// always shown, and the one left may go quiet. Tells the frontend.
    pub(super) fn track_open_room(&mut self, id: Option<RoomId>) {
        let changes = self.rooms.set_open(id, self.clock.now());
        if !changes.is_empty() {
            self.emit_rooms(changes);
        }
    }

    /// Changes a listed room and tells the frontend, unless nothing changes. The open room stays
    /// read, as for a push.
    fn change_room(&mut self, id: &RoomId, change: impl FnOnce(Room) -> Room) {
        let Some(held) = self.rooms.get(id).cloned() else {
            return;
        };
        let room = self.seen_if_open(change(held));
        let change = self.rooms.upsert(room, self.clock.now());
        self.emit_rooms(change.into_iter().collect());
    }

    /// Drops the room from the list, and closes it if it was open.
    pub(super) fn remove_room(&mut self, id: &RoomId) {
        let change = self.rooms.remove(id);
        self.emit_rooms(change.into_iter().collect());
        if self
            .open
            .as_ref()
            .is_some_and(|open| open.timeline.room() == id)
        {
            self.close_room();
        }
    }

    /// Empties the room list and tells the frontend, unless it was empty already.
    pub(super) fn clear_rooms(&mut self) {
        self.rooms_listed = false;
        if self.rooms.is_empty() {
            return;
        }
        let change = self.rooms.reset(Vec::new(), self.clock.now());
        self.emit_rooms(vec![change]);
    }
}
