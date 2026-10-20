// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! What the hub remembers of the core's events, so that a view model created late can catch up.
//! Plain Rust, so it can be unit-tested without Qt or a [`Client`](rc_core::Client).

use rc_core::Event;
use rc_core::domain::Room;
use rc_core::rooms::RoomChange;
use rc_core::session::SessionState;

/// The latest session state and a mirror of the room list.
#[derive(Debug, Default)]
pub struct Cache {
    /// `None` before the first event.
    state: Option<SessionState>,
    rooms: Vec<Room>,
}

impl Cache {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: None,
            rooms: Vec::new(),
        }
    }

    /// Takes the event in, as the core emits them: in order.
    pub fn apply(&mut self, event: &Event) {
        match event {
            Event::Session(state) => self.state = Some(state.clone()),
            Event::Rooms(changes) => {
                for change in changes {
                    change.apply(&mut self.rooms);
                }
            }
            _ => {}
        }
    }

    #[must_use]
    pub fn state(&self) -> Option<&SessionState> {
        self.state.as_ref()
    }

    /// The change that brings an empty model to the current room list.
    #[must_use]
    pub fn rooms_snapshot(&self) -> RoomChange {
        RoomChange::Reset(self.rooms.clone())
    }
}

#[cfg(test)]
mod tests {
    use rc_core::domain::RoomKind;
    use rc_core::rooms::RoomList;

    use super::*;

    fn room(id: &str, name: &str) -> Room {
        Room {
            id: id.to_owned().into(),
            kind: RoomKind::Channel,
            name: name.to_owned(),
            unread: 0,
            user_mentions: 0,
            group_mentions: 0,
            alert: false,
        }
    }

    /// Events as the core would emit them for a session that lists, adds, renames and removes
    /// rooms, and logs out.
    fn script() -> Vec<Event> {
        let mut list = RoomList::default();
        let rooms = |change: Option<RoomChange>| Event::Rooms(change.into_iter().collect());
        vec![
            Event::Session(SessionState::LoggedOut { failure: None }),
            Event::Session(SessionState::Connecting),
            Event::Rooms(vec![
                list.reset(vec![room("a", "alpha"), room("c", "charlie")]),
            ]),
            rooms(list.upsert(room("b", "bravo"))),
            rooms(list.upsert(room("a", "zulu"))),
            rooms(list.remove(&"c".to_owned().into())),
            Event::Session(SessionState::LoggedOut { failure: None }),
            Event::Rooms(vec![list.reset(Vec::new())]),
        ]
    }

    // AUTH-001
    #[test]
    fn remembers_the_latest_session_state() {
        let mut cache = Cache::new();
        assert_eq!(cache.state(), None);
        cache.apply(&Event::Session(SessionState::Connecting));
        assert_eq!(cache.state(), Some(&SessionState::Connecting));
        cache.apply(&Event::Session(SessionState::LoggedOut { failure: None }));
        assert_eq!(
            cache.state(),
            Some(&SessionState::LoggedOut { failure: None })
        );
    }

    // ROOM-001
    #[test]
    fn a_snapshot_taken_at_any_point_followed_by_the_rest_equals_the_replay() {
        let events = script();
        let mut replay = Vec::new();
        let mut full = Cache::new();
        for event in &events {
            full.apply(event);
            if let Event::Rooms(changes) = event {
                for change in changes {
                    change.apply(&mut replay);
                }
            }
        }

        // A view model registering after `taken` events starts from the snapshot and then gets
        // the later changes one by one.
        for taken in 0..=events.len() {
            let mut cache = Cache::new();
            for event in &events[..taken] {
                cache.apply(event);
            }
            let mut mirror = Vec::new();
            cache.rooms_snapshot().apply(&mut mirror);
            for event in &events[taken..] {
                if let Event::Rooms(changes) = event {
                    for change in changes {
                        change.apply(&mut mirror);
                    }
                }
            }
            assert_eq!(mirror, replay, "registered after {taken} events");
        }
        assert_eq!(full.rooms_snapshot(), RoomChange::Reset(replay));
    }
}
