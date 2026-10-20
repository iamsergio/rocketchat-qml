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

impl RoomChange {
    /// Applies the change to a frontend's mirror of the room list. The mirror must be in the state
    /// the change was made against, i.e. all earlier changes were applied in order, and the indices
    /// must be in range: a change that doesn't fit is a bug in the caller, and panics like the
    /// `Vec` operation it stands for.
    pub fn apply(&self, rooms: &mut Vec<Room>) {
        match self {
            Self::Reset(all) => rooms.clone_from(all),
            Self::Inserted { index, room } => rooms.insert(*index, room.clone()),
            Self::Updated { index, room } => rooms[*index] = room.clone(),
            Self::Moved { from, to, room } => {
                rooms.remove(*from);
                rooms.insert(*to, room.clone());
            }
            Self::Removed { index } => {
                rooms.remove(*index);
            }
        }
    }
}

/// The rooms in display order.
#[derive(Debug, Default)]
pub struct RoomList {
    rooms: Vec<Room>,
}

/// The display order: the most recently active room first, then by name, ignoring case, and by id
/// between equal names.
fn display_order(a: &Room, b: &Room) -> Ordering {
    b.last_activity
        .cmp(&a.last_activity)
        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
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

    /// Makes the list equal to `rooms`, a fresh snapshot, with the smallest changes: the removals
    /// first, then the new and changed rooms. Unlike [`reset`](Self::reset), a frontend keeps its
    /// rows (and what they show, such as a selection) for the rooms that didn't change.
    pub fn sync(&mut self, rooms: Vec<Room>) -> Vec<RoomChange> {
        let gone: Vec<RoomId> = self
            .rooms
            .iter()
            .filter(|old| !rooms.iter().any(|room| room.id == old.id))
            .map(|old| old.id.clone())
            .collect();
        let mut changes: Vec<RoomChange> = gone.iter().filter_map(|id| self.remove(id)).collect();
        let mut rooms = rooms;
        rooms.sort_by(display_order);
        changes.extend(rooms.into_iter().filter_map(|room| self.upsert(room)));
        changes
    }

    /// `None` if there's no such room.
    pub fn remove(&mut self, id: &RoomId) -> Option<RoomChange> {
        let index = self.position(id)?;
        self.rooms.remove(index);
        Some(RoomChange::Removed { index })
    }

    /// Moves the room up if it had a message after what is known of it. `None` if the room isn't
    /// listed or already knew that time.
    pub fn bump(&mut self, id: &RoomId, at: jiff::Timestamp) -> Option<RoomChange> {
        let room = self.get(id).filter(|room| room.last_activity < at)?;
        self.upsert(Room {
            last_activity: at,
            ..room.clone()
        })
    }

    #[must_use]
    pub fn get(&self, id: &RoomId) -> Option<&Room> {
        self.rooms.iter().find(|room| &room.id == id)
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
            peer: None,
            avatar_etag: None,
            unread: 0,
            user_mentions: 0,
            group_mentions: 0,
            alert: false,
            last_activity: jiff::Timestamp::UNIX_EPOCH,
        }
    }

    fn at(secs: i64) -> jiff::Timestamp {
        jiff::Timestamp::from_second(secs).unwrap()
    }

    fn active(id: &str, name: &str, secs: i64) -> Room {
        Room {
            last_activity: at(secs),
            ..room(id, name)
        }
    }

    fn names(list: &RoomList) -> Vec<&str> {
        list.rooms().iter().map(|r| r.name.as_str()).collect()
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

    // ROOM-001
    #[test]
    fn sync_turns_a_snapshot_into_fine_grained_changes() {
        let mut list = list_of(vec![
            room("a", "a"),
            room("b", "b"),
            room("c", "c"),
            room("d", "d"),
        ]);
        let mut busy = room("d", "d");
        busy.unread = 4;
        let changes = list.sync(vec![
            busy.clone(),
            room("c", "c"),
            room("a", "e"),
            room("n", "bb"),
        ]);
        assert_eq!(
            changes,
            [
                RoomChange::Removed { index: 1 },
                RoomChange::Inserted {
                    index: 1,
                    room: room("n", "bb")
                },
                RoomChange::Updated {
                    index: 3,
                    room: busy
                },
                RoomChange::Moved {
                    from: 0,
                    to: 3,
                    room: room("a", "e")
                },
            ]
        );
        assert_eq!(names(&list), ["bb", "c", "d", "e"]);
    }

    // ROOM-001
    #[test]
    fn sync_of_the_same_rooms_changes_nothing() {
        let mut list = list_of(vec![room("a", "a"), room("b", "b")]);
        assert_eq!(list.sync(vec![room("b", "b"), room("a", "a")]), []);
        assert_eq!(names(&list), ["a", "b"]);
    }

    // ROOM-001
    #[test]
    fn sync_of_an_empty_snapshot_removes_every_room() {
        let mut list = list_of(vec![room("a", "a"), room("b", "b")]);
        assert_eq!(
            list.sync(Vec::new()),
            [
                RoomChange::Removed { index: 0 },
                RoomChange::Removed { index: 0 }
            ]
        );
        assert_eq!(list.sync(Vec::new()), []);
    }

    // ROOM-001
    #[test]
    fn a_mirror_following_the_sync_changes_equals_the_list() {
        let (mut list, mut mirror) = (RoomList::default(), Vec::new());
        let mut flagged = room("q", "queue");
        flagged.alert = true;
        let snapshots = [
            vec![room("c", "Charlie"), room("a", "alpha"), room("q", "queue")],
            vec![room("q", "Alpha"), room("z", "zulu"), room("c", "Charlie")],
            vec![flagged.clone(), room("y", "yankee"), room("c", "zzz")],
            vec![],
            vec![flagged, room("a", "alpha")],
        ];
        for snapshot in snapshots {
            for change in list.sync(snapshot.clone()) {
                change.apply(&mut mirror);
            }
            let mut expected = snapshot;
            expected.sort_by(display_order);
            assert_eq!(mirror, expected);
            assert_eq!(mirror, list.rooms());
        }
    }

    /// Runs `action` on the list and applies its change to the mirror, which must then equal the
    /// list.
    fn step(
        list: &mut RoomList,
        mirror: &mut Vec<Room>,
        action: impl FnOnce(&mut RoomList) -> Option<RoomChange>,
    ) {
        if let Some(change) = action(list) {
            change.apply(mirror);
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

    // ROOM-005
    #[test]
    fn the_most_recently_active_room_comes_first_then_name_then_id() {
        let mut list = RoomList::default();
        list.reset(vec![
            active("1", "old", 10),
            active("2", "beta", 20),
            active("3", "alpha", 20),
            active("0", "alpha", 20),
            active("4", "new", 30),
        ]);
        assert_eq!(names(&list), ["new", "alpha", "alpha", "beta", "old"]);
        let ids: Vec<_> = list.rooms().iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["4", "0", "3", "2", "1"]);
    }

    // ROOM-005
    #[test]
    fn bump_moves_the_room_to_the_front() {
        let mut list = list_of(vec![
            active("a", "a", 30),
            active("b", "b", 20),
            active("c", "c", 10),
        ]);
        let change = list.bump(&"c".to_owned().into(), at(40));
        assert_eq!(
            change,
            Some(RoomChange::Moved {
                from: 2,
                to: 0,
                room: active("c", "c", 40)
            })
        );
        assert_eq!(names(&list), ["c", "a", "b"]);
    }

    // ROOM-005
    #[test]
    fn bump_within_the_same_place_updates_in_place() {
        let mut list = list_of(vec![active("a", "a", 30), active("b", "b", 20)]);
        assert_eq!(
            list.bump(&"a".to_owned().into(), at(35)),
            Some(RoomChange::Updated {
                index: 0,
                room: active("a", "a", 35)
            })
        );
    }

    // ROOM-005
    #[test]
    fn bump_ignores_old_times_and_unknown_rooms() {
        let mut list = list_of(vec![active("a", "a", 30), active("b", "b", 20)]);
        assert_eq!(list.bump(&"b".to_owned().into(), at(20)), None);
        assert_eq!(list.bump(&"b".to_owned().into(), at(5)), None);
        assert_eq!(list.bump(&"zzz".to_owned().into(), at(99)), None);
        assert_eq!(names(&list), ["a", "b"]);
    }

    // ROOM-005
    #[test]
    fn sync_moves_rooms_whose_activity_changed() {
        let mut list = list_of(vec![active("a", "a", 30), active("b", "b", 20)]);
        let changes = list.sync(vec![active("a", "a", 30), active("b", "b", 40)]);
        assert_eq!(
            changes,
            [RoomChange::Moved {
                from: 1,
                to: 0,
                room: active("b", "b", 40)
            }]
        );
    }

    // ROOM-005
    #[test]
    fn a_mirror_following_the_bumps_equals_the_list() {
        let (mut list, mut mirror) = (RoomList::default(), Vec::new());
        let id = |id: &str| -> RoomId { id.to_owned().into() };
        let l = &mut list;
        let m = &mut mirror;
        step(l, m, |l| {
            Some(l.reset(vec![
                active("a", "a", 10),
                active("b", "b", 20),
                active("c", "c", 30),
            ]))
        });
        step(l, m, |l| l.bump(&id("a"), at(40)));
        step(l, m, |l| l.bump(&id("c"), at(50)));
        step(l, m, |l| l.bump(&id("c"), at(50)));
        step(l, m, |l| l.bump(&id("b"), at(45)));
        step(l, m, |l| l.upsert(active("d", "d", 47)));
        step(l, m, |l| l.remove(&id("c")));
        assert_eq!(names(&list), ["d", "b", "a"]);
    }
}
