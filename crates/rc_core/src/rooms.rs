// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The room list in display order, and the index-based changes that keep a frontend's copy of it
//! in step. Frontends mirror the list and never sort or diff it themselves.

use std::cmp::Ordering;

use crate::domain::{Room, RoomId};

/// One step for a frontend's mirror of the room list. Apply the changes in order.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RoomChange {
    /// The whole list, in order. After logging in or resuming; empty after logging out.
    Reset(Vec<Room>),
    Inserted {
        index: usize,
        room: Room,
    },
    /// Same position, new content.
    Updated {
        index: usize,
        room: Room,
    },
    /// The room moved; the content may have changed too. `to` is its index once the move is done
    /// (`Vec::remove(from)` then `Vec::insert(to, ..)`). Qt's `beginMoveRows` takes a different
    /// destination, so `rc_qt` converts.
    Moved {
        from: usize,
        to: usize,
        room: Room,
    },
    Removed {
        index: usize,
    },
}

/// The rooms in display order.
#[derive(Debug, Default)]
pub struct RoomList {
    rooms: Vec<Room>,
}

/// The display order: by name, ignoring case, and by id between equal names. ROOM-005 (Phase 3)
/// changes this to last activity.
fn display_order(a: &Room, b: &Room) -> Ordering {
    a.name
        .to_lowercase()
        .cmp(&b.name.to_lowercase())
        .then_with(|| a.id.cmp(&b.id))
}

impl RoomList {
    #[must_use]
    pub fn rooms(&self) -> &[Room] {
        &self.rooms
    }

    /// Replaces the content, sorting it.
    pub fn reset(&mut self, mut rooms: Vec<Room>) -> RoomChange {
        rooms.sort_by(display_order);
        self.rooms.clone_from(&rooms);
        RoomChange::Reset(rooms)
    }

    /// Adds the room, or replaces the one with the same id. `None` if nothing changes.
    pub fn upsert(&mut self, room: Room) -> Option<RoomChange> {
        let Some(from) = self.position(&room.id) else {
            let index = self.insertion_index(&room);
            self.rooms.insert(index, room.clone());
            return Some(RoomChange::Inserted { index, room });
        };
        if self.rooms[from] == room {
            return None;
        }
        self.rooms.remove(from);
        let to = self.insertion_index(&room);
        self.rooms.insert(to, room.clone());
        Some(if to == from {
            RoomChange::Updated { index: from, room }
        } else {
            RoomChange::Moved { from, to, room }
        })
    }

    /// `None` if there's no such room.
    pub fn remove(&mut self, id: &RoomId) -> Option<RoomChange> {
        let index = self.position(id)?;
        self.rooms.remove(index);
        Some(RoomChange::Removed { index })
    }

    fn position(&self, id: &RoomId) -> Option<usize> {
        self.rooms.iter().position(|room| &room.id == id)
    }

    /// Where `room` goes in the list as it is now.
    fn insertion_index(&self, room: &Room) -> usize {
        self.rooms
            .partition_point(|other| display_order(other, room).is_lt())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::RoomKind;

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

    fn names(list: &RoomList) -> Vec<&str> {
        list.rooms().iter().map(|r| r.name.as_str()).collect()
    }

    /// What a frontend does with the changes.
    fn apply(mirror: &mut Vec<Room>, change: &RoomChange) {
        match change {
            RoomChange::Reset(rooms) => mirror.clone_from(rooms),
            RoomChange::Inserted { index, room } => mirror.insert(*index, room.clone()),
            RoomChange::Updated { index, room } => mirror[*index] = room.clone(),
            RoomChange::Moved { from, to, room } => {
                mirror.remove(*from);
                mirror.insert(*to, room.clone());
            }
            RoomChange::Removed { index } => {
                mirror.remove(*index);
            }
        }
    }

    fn list_of(rooms: Vec<Room>) -> RoomList {
        let mut list = RoomList::default();
        list.reset(rooms);
        list
    }

    // ROOM-001
    #[test]
    fn reset_sorts_ignoring_case_and_breaks_ties_by_id() {
        let mut list = RoomList::default();
        let change = list.reset(vec![
            room("2", "bravo"),
            room("9", "alpha"),
            room("1", "Alpha"),
            room("3", "Charlie"),
        ]);
        assert_eq!(names(&list), ["Alpha", "alpha", "bravo", "Charlie"]);
        assert_eq!(change, RoomChange::Reset(list.rooms().to_vec()));
    }

    // ROOM-001
    #[test]
    fn upsert_inserts_at_the_sorted_index() {
        let mut list = list_of(vec![room("a", "a"), room("c", "c")]);
        let change = list.upsert(room("b", "b"));
        assert_eq!(
            change,
            Some(RoomChange::Inserted {
                index: 1,
                room: room("b", "b")
            })
        );
        assert_eq!(names(&list), ["a", "b", "c"]);
    }

    // ROOM-003
    #[test]
    fn upsert_with_new_counts_updates_in_place() {
        let mut list = list_of(vec![room("a", "a"), room("b", "b")]);
        let mut changed = room("b", "b");
        changed.unread = 3;
        assert_eq!(
            list.upsert(changed.clone()),
            Some(RoomChange::Updated {
                index: 1,
                room: changed.clone()
            })
        );
        assert_eq!(list.rooms()[1], changed);
    }

    // ROOM-002
    #[test]
    fn upsert_with_a_rename_moves_the_room() {
        let mut list = list_of(vec![room("a", "a"), room("b", "b"), room("c", "c")]);
        // Down.
        let down = list.upsert(room("a", "d"));
        assert_eq!(
            down,
            Some(RoomChange::Moved {
                from: 0,
                to: 2,
                room: room("a", "d")
            })
        );
        assert_eq!(names(&list), ["b", "c", "d"]);
        // Up.
        let up = list.upsert(room("a", "a"));
        assert_eq!(
            up,
            Some(RoomChange::Moved {
                from: 2,
                to: 0,
                room: room("a", "a")
            })
        );
        assert_eq!(names(&list), ["a", "b", "c"]);
    }

    // ROOM-001
    #[test]
    fn upsert_of_an_identical_room_changes_nothing() {
        let mut list = list_of(vec![room("a", "a")]);
        assert_eq!(list.upsert(room("a", "a")), None);
    }

    // ROOM-001
    #[test]
    fn remove_drops_the_room_or_ignores_an_unknown_id() {
        let mut list = list_of(vec![room("a", "a"), room("b", "b")]);
        assert_eq!(
            list.remove(&"b".to_owned().into()),
            Some(RoomChange::Removed { index: 1 })
        );
        assert_eq!(names(&list), ["a"]);
        assert_eq!(list.remove(&"zzz".to_owned().into()), None);
    }

    /// Runs `action` on the list and applies its change to the mirror, which must then equal the
    /// list.
    fn step(
        list: &mut RoomList,
        mirror: &mut Vec<Room>,
        action: impl FnOnce(&mut RoomList) -> Option<RoomChange>,
    ) {
        if let Some(change) = action(list) {
            apply(mirror, &change);
        }
        assert_eq!(mirror, list.rooms());
    }

    // ROOM-001
    #[test]
    fn a_mirror_following_the_changes_equals_the_list() {
        fn id(id: &str) -> RoomId {
            id.to_owned().into()
        }
        let (mut list, mut mirror) = (RoomList::default(), Vec::new());
        let mut read = room("a", "alpha");
        read.alert = true;

        let l = &mut list;
        let m = &mut mirror;
        step(l, m, |l| {
            Some(l.reset(vec![room("c", "Charlie"), room("a", "alpha")]))
        });
        step(l, m, |l| l.upsert(room("b", "bravo")));
        step(l, m, |l| l.upsert(room("d", "alpha")));
        step(l, m, |l| l.upsert(read));
        step(l, m, |l| l.upsert(room("c", "Aardvark")));
        step(l, m, |l| l.upsert(room("c", "zulu")));
        step(l, m, |l| l.upsert(room("c", "zulu")));
        step(l, m, |l| l.remove(&id("a")));
        step(l, m, |l| l.remove(&id("nope")));
        step(l, m, |l| {
            Some(l.reset(vec![room("x", "x"), room("y", "Y")]))
        });
        step(l, m, |l| l.remove(&id("x")));
        step(l, m, |l| Some(l.reset(Vec::new())));
    }
}
