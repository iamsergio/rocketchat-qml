// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Who is typing in the open room and in its open thread: what the server pushes, what expires,
//! and what the frontend is told.

use crate::client::Event;
use crate::domain::{Message, MessageId, RoomId};
use crate::ports::Connector;
use crate::typing::Place;

use super::Actor;

impl<C: Connector> Actor<C> {
    /// Takes in that someone started or stopped typing, in the room or in one of its threads
    /// (`thread`). Only the open room counts, and the user's own username doesn't: it is their
    /// other clients that type.
    pub(super) fn handle_typing(
        &mut self,
        room: &RoomId,
        username: &str,
        typing: bool,
        thread: Option<MessageId>,
    ) {
        if self.open.as_ref().map(|open| open.timeline.room()) != Some(room)
            || self.own_user().is_some_and(|me| me.username == username)
        {
            return;
        }
        let place = thread.map_or(Place::Room, Place::Thread);
        if typing {
            self.typists.start(username, &place, self.clock.now());
        } else {
            self.typists.stop(username, &place);
        }
        self.tell_typing();
    }

    /// A message ends its author's typing in the place it was sent to: the room, or its thread.
    pub(super) fn message_ends_typing(&mut self, message: &Message) {
        if self.open.as_ref().map(|open| open.timeline.room()) != Some(&message.room_id) {
            return;
        }
        let place = message.thread_id.clone().map_or(Place::Room, Place::Thread);
        self.typists.stop(&message.author.username, &place);
        self.tell_typing();
    }

    /// Forgets the typists whose time is up.
    pub(super) fn expire_typists(&mut self) {
        self.typists.expire(self.clock.now());
        self.tell_typing();
    }

    /// Forgets everyone, and tells the frontend unless nobody was shown: another room, a lost
    /// connection, a logout.
    pub(super) fn end_typing(&mut self) {
        self.typists.clear();
        self.tell_typing();
    }

    /// Tells the frontend the names of the open room's typists, and of the open thread's, for each
    /// one that differs from what it was last told. A list that empties comes as an empty list for
    /// the room or the thread it was for, also when that one is closed or replaced meanwhile.
    pub(super) fn tell_typing(&mut self) {
        let room = self.open.as_ref().map(|open| {
            (
                open.timeline.room().clone(),
                self.names_typing(&Place::Room),
            )
        });
        let thread = self.open_thread.as_ref().map(|thread| {
            let parent = thread.parent.id.clone();
            let names = self.names_typing(&Place::Thread(parent.clone()));
            (parent, names)
        });
        let on_event = &self.on_event;
        tell(&mut self.typing_told, room, |room, names| {
            on_event(Event::Typing { room, names });
        });
        tell(&mut self.thread_typing_told, thread, |thread, names| {
            on_event(Event::ThreadTyping { thread, names });
        });
    }

    /// The names to show for the typists of `place`: the display name if a held message of the open
    /// room has one for the username, else the username.
    fn names_typing(&self, place: &Place) -> Vec<String> {
        let held = self.open.iter().flat_map(|open| open.timeline.messages());
        self.typists
            .usernames(place)
            .into_iter()
            .map(|username| {
                held.clone()
                    .filter(|m| m.author.username == username)
                    .find_map(|m| m.author.display_name.clone())
                    .unwrap_or_else(|| username.to_owned())
            })
            .collect()
    }
}

/// Tells, through `emit`, what changed between the list `told` last (`None` if it was empty or
/// nothing was open) and the list `shown` now, for a key (a room or a thread): nothing if they are
/// the same; otherwise an empty list for the key that was told, if it is not the one shown now, and
/// then the list of the one shown. Empty lists are not told as new, and `told` becomes what is
/// shown.
fn tell<K: PartialEq + Clone>(
    told: &mut Option<(K, Vec<String>)>,
    shown: Option<(K, Vec<String>)>,
    mut emit: impl FnMut(K, Vec<String>),
) {
    let shown = shown.filter(|(_, names)| !names.is_empty());
    if *told == shown {
        return;
    }
    if let Some((old, _)) = told.as_ref()
        && shown.as_ref().is_none_or(|(key, _)| key != old)
    {
        emit(old.clone(), Vec::new());
    }
    if let Some((key, names)) = &shown {
        emit(key.clone(), names.clone());
    }
    *told = shown;
}
