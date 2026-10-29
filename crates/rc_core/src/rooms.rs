// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The room list in display order, and the index-based changes that keep a frontend's copy of it
//! in step. Frontends mirror the list and never sort or diff it themselves.

use std::cmp::Ordering;
use std::collections::HashMap;

use crate::domain::{Room, RoomId, RoomKind};
use crate::list_change::ListChange;
use crate::ports::ApiError;

/// One step for a frontend's mirror of the room list. Apply the changes in order. After logging in
/// or resuming the first one is a `Reset` to the rooms; after logging out, a `Reset` to nothing.
pub type RoomChange = ListChange<Room>;

/// How long a direct message may go without activity before the list stops showing it.
pub const QUIET_AFTER: jiff::SignedDuration = jiff::SignedDuration::from_hours(10 * 24);

/// How often the list shows the news of channels: at most once in this long.
// TODO: make this configurable from the settings dialog (PLAN.md, Phase 6).
pub const CHANNEL_BATCH: jiff::SignedDuration = jiff::SignedDuration::from_mins(25);

/// The account's rooms: the ones the frontend shows, in display order, and the ones the list holds
/// back. Positions exist only among the shown rooms: a held-back room is found by its id.
///
/// A direct message is held back once it has been quiet for longer than `quiet_after`, unless it is
/// a favorite, shows as unread or is the open room. The room changes never mention a held-back
/// room; the frontend hears of them only as a set, [`quiet`](RoomList::quiet). It comes back, with
/// an `Inserted`, as soon as the rule lets it, e.g. with a new message.
///
/// The news of a channel or a private group (more unread, an alert, newer activity, new unread
/// threads) is held back too, but the room stays in the list: it keeps showing what it showed
/// until the batch is released, all channels at once, `batch_every` after the first held change.
/// Direct messages, rooms with a mention, the open room and the rooms the user acts on are never
/// held. See [`upsert`](RoomList::upsert) and [`release`](RoomList::release).
#[derive(Debug)]
pub struct RoomList {
    /// The rooms the frontend shows, in display order. `RoomChange` indices point here.
    shown: Vec<Room>,
    /// The rooms the list doesn't show. They have no position, only an id.
    hidden: HashMap<RoomId, Room>,
    /// Whether `hidden` may have changed since [`take_quiet_change`](Self::take_quiet_change). It
    /// is set whenever a room goes in or out of `hidden`, or is replaced in it, without checking
    /// whether the content is different.
    hidden_touched: bool,
    /// How long a direct message may be quiet and still be shown.
    quiet_after: jiff::SignedDuration,
    /// The room the user has open, which is always shown.
    open: Option<RoomId>,
    /// The channels whose news is held: the latest real state of each. `shown` has the same room
    /// with the news it showed before.
    pending: HashMap<RoomId, Room>,
    /// When the current batch started: the time of the first change held since the last release.
    /// `Some` exactly when `pending` isn't empty.
    held_since: Option<jiff::Timestamp>,
    /// How often the news of channels is shown. Zero turns holding it off.
    batch_every: jiff::SignedDuration,
}

impl Default for RoomList {
    /// A list that holds direct messages back after [`QUIET_AFTER`] and shows the news of channels
    /// at once.
    fn default() -> Self {
        Self::new(QUIET_AFTER, jiff::SignedDuration::ZERO)
    }
}

/// Something the user does to a room from the room list's menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomAction {
    Favorite,
    Unfavorite,
    MarkRead,
    MarkUnread,
    Leave,
}

/// Why a [`RoomAction`] didn't happen. The texts are meant to be shown as they are.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RoomActionError {
    /// Refused without asking the server: the room isn't in the list, is a direct message and
    /// can't be left, or is the open room and can't be marked unread.
    #[error("This room can't be {}", match .0 {
        RoomAction::Favorite => "added to the favorites",
        RoomAction::Unfavorite => "removed from the favorites",
        RoomAction::MarkRead => "marked as read",
        RoomAction::MarkUnread => "marked as unread",
        RoomAction::Leave => "left",
    })]
    NotAllowed(RoomAction),
    #[error("Can't reach the server")]
    Unreachable,
    /// Any other error, with its message.
    #[error("The server reported an error: {0}")]
    Server(String),
}

impl From<&ApiError> for RoomActionError {
    fn from(error: &ApiError) -> Self {
        match error {
            ApiError::Unreachable(_) => Self::Unreachable,
            other => Self::Server(other.to_string()),
        }
    }
}

/// The display order: the favorites first; in each group the most recently active room first, then
/// by name, ignoring case, and by id between equal names.
fn display_order(a: &Room, b: &Room) -> Ordering {
    b.favorite
        .cmp(&a.favorite)
        .then_with(|| b.last_activity.cmp(&a.last_activity))
        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        .then_with(|| a.id.cmp(&b.id))
}

/// What a frontend shows as an account's badge: the totals over its room list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Unread {
    /// How many rooms show as unread.
    pub rooms: u32,
    /// The sum of the rooms' mention badges.
    pub mentions: u32,
}

impl RoomList {
    /// An empty list that holds direct messages back after `quiet_after`, and the news of channels
    /// for `batch_every` (zero for no holding).
    #[must_use]
    pub fn new(quiet_after: jiff::SignedDuration, batch_every: jiff::SignedDuration) -> Self {
        Self {
            shown: Vec::new(),
            hidden: HashMap::new(),
            hidden_touched: false,
            quiet_after,
            open: None,
            pending: HashMap::new(),
            held_since: None,
            batch_every,
        }
    }

    /// The unread totals of all the rooms, the hidden ones too, as they are shown: the news of a
    /// held channel isn't in them.
    #[must_use]
    pub fn unread(&self) -> Unread {
        self.shown
            .iter()
            .chain(self.hidden.values())
            .fold(Unread::default(), |total, room| Unread {
                rooms: total.rooms + u32::from(room.has_unread()),
                mentions: total.mentions + room.mention_badge(),
            })
    }

    /// The rooms the frontend shows, in display order: what `RoomChange` indices refer to.
    #[must_use]
    pub fn rooms(&self) -> &[Room] {
        &self.shown
    }

    /// The rooms the list holds back, the most recently active first, then by id: a frontend that
    /// wants them, to search among them, takes them in this order and never sorts.
    #[must_use]
    pub fn quiet(&self) -> Vec<Room> {
        let mut quiet: Vec<Room> = self.hidden.values().cloned().collect();
        quiet.sort_by(|a, b| {
            b.last_activity
                .cmp(&a.last_activity)
                .then_with(|| a.id.cmp(&b.id))
        });
        quiet
    }

    /// The held-back rooms as [`quiet`](Self::quiet) has them, if one may have gone in, out or
    /// changed since the last call; `None` if none did, which is the usual case and costs nothing.
    /// "May have": a room replaced by an identical one counts, so the caller compares.
    pub fn take_quiet_change(&mut self) -> Option<Vec<Room>> {
        std::mem::take(&mut self.hidden_touched).then(|| self.quiet())
    }

    /// Whether the list holds no room at all, shown or hidden.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.shown.is_empty() && self.hidden.is_empty()
    }

    /// Replaces the content, sorting what is shown. The hidden rooms and the held news go too: the
    /// rooms are the truth.
    pub fn reset(&mut self, rooms: Vec<Room>, now: jiff::Timestamp) -> RoomChange {
        self.pending.clear();
        self.held_since = None;
        let (mut shown, hidden): (Vec<Room>, Vec<Room>) = rooms
            .into_iter()
            .partition(|room| self.is_shown_at(room, now));
        shown.sort_by(display_order);
        self.shown.clone_from(&shown);
        self.hidden_touched |= !self.hidden.is_empty() || !hidden.is_empty();
        self.hidden = hidden
            .into_iter()
            .map(|room| (room.id.clone(), room))
            .collect();
        RoomChange::Reset(shown)
    }

    /// Adds the room, or replaces the one with the same id, wherever it is, and puts it where the
    /// rule says: a shown room that is held back from now on is `Removed`, and a hidden one that
    /// is shown from now on is `Inserted`. `None` if the frontend's list doesn't change.
    ///
    /// If the room is a channel and the change is news the user may do without, the shown room
    /// gets everything but the news, which waits for [`release`](Self::release) (the first one
    /// starts the batch at `now`). A channel with a mention, the open room, a room that became
    /// read and a room the list didn't show are never held, and drop the news held for them.
    pub fn upsert(&mut self, room: Room, now: jiff::Timestamp) -> Option<RoomChange> {
        if let Some(shown) = self.holdable(&room) {
            let held = Room {
                unread: shown.unread,
                alert: shown.alert,
                last_activity: shown.last_activity,
                unread_threads: shown.unread_threads.clone(),
                mentioned_threads: shown.mentioned_threads.clone(),
                ..room.clone()
            };
            self.pending.insert(room.id.clone(), room);
            self.held_since.get_or_insert(now);
            return self.place(held, now);
        }
        self.drop_pending(&room.id);
        self.place(room, now)
    }

    /// Like [`upsert`](Self::upsert), but the news is shown at once: for what the user did.
    pub fn upsert_now(&mut self, room: Room, now: jiff::Timestamp) -> Option<RoomChange> {
        self.drop_pending(&room.id);
        self.place(room, now)
    }

    /// Shows the news of all the held channels, moving them into place, and ends the batch.
    pub fn release(&mut self, now: jiff::Timestamp) -> Vec<RoomChange> {
        self.held_since = None;
        let mut rooms: Vec<Room> = self.pending.drain().map(|(_, room)| room).collect();
        rooms.sort_by(display_order);
        rooms
            .into_iter()
            .filter_map(|room| self.place(room, now))
            .collect()
    }

    /// How many rooms have news held back.
    #[must_use]
    pub fn held_rooms(&self) -> usize {
        self.pending.len()
    }

    /// Whether news is held back, waiting for [`release`](Self::release).
    #[must_use]
    pub fn holds_news(&self) -> bool {
        self.held_since.is_some()
    }

    /// When the current batch is to be released. `None` if no news is held.
    #[must_use]
    pub fn next_release(&self) -> Option<jiff::Timestamp> {
        self.held_since?.checked_add(self.batch_every).ok()
    }

    /// The shown copy of `room` if the change to `room` is news to hold back: it is a channel, it
    /// differs from the shown copy in what the news are made of, and the user would be spared.
    fn holdable(&self, room: &Room) -> Option<&Room> {
        if self.batch_every <= jiff::SignedDuration::ZERO
            || !matches!(room.kind, RoomKind::Channel | RoomKind::Private)
            || room.user_mentions > 0
            || room.group_mentions > 0
            || self.open.as_ref() == Some(&room.id)
        {
            return None;
        }
        let shown = self.shown.iter().find(|shown| shown.id == room.id)?;
        // Read elsewhere: nothing to spare the user from. Read against the real state, as the
        // shown row of a held room may never have shown the news that was read.
        let before = self.pending.get(&room.id).unwrap_or(shown);
        if before.has_unread() && !room.has_unread() {
            return None;
        }
        let same_news = shown.unread == room.unread
            && shown.alert == room.alert
            && shown.last_activity == room.last_activity
            && shown.unread_threads == room.unread_threads
            && shown.mentioned_threads == room.mentioned_threads;
        (!same_news).then_some(shown)
    }

    /// Forgets the news held for the room, and ends the batch if that was the last.
    fn drop_pending(&mut self, id: &RoomId) {
        self.pending.remove(id);
        if self.pending.is_empty() {
            self.held_since = None;
        }
    }

    /// Puts the room, as it is to be shown, where the quiet rule says, in place of the one with
    /// the same id: a shown room that is held back from now on is `Removed`, and a hidden one that
    /// is shown from now on is `Inserted`. `None` if the frontend's list doesn't change.
    fn place(&mut self, room: Room, now: jiff::Timestamp) -> Option<RoomChange> {
        let from = self.position(&room.id);
        if !self.is_shown_at(&room, now) {
            let id = room.id.clone();
            self.hidden.insert(id, room);
            self.hidden_touched = true;
            let index = from?;
            self.shown.remove(index);
            return Some(RoomChange::Removed { index });
        }
        let Some(from) = from else {
            // A room is in one place only: a hidden one is shown from now on.
            self.hidden_touched |= self.hidden.remove(&room.id).is_some();
            let index = self.insertion_index(&room);
            self.shown.insert(index, room.clone());
            return Some(RoomChange::Inserted { index, item: room });
        };
        if self.shown[from] == room {
            return None;
        }
        self.shown.remove(from);
        let to = self.insertion_index(&room);
        self.shown.insert(to, room.clone());
        Some(if to == from {
            RoomChange::Updated {
                index: from,
                item: room,
            }
        } else {
            RoomChange::Moved {
                from,
                to,
                item: room,
            }
        })
    }

    /// Makes the list equal to `rooms`, a fresh snapshot, with the smallest changes: the removals
    /// first, then the new and changed rooms. Unlike [`reset`](Self::reset), a frontend keeps its
    /// rows (and what they show, such as a selection) for the rooms that didn't change.
    pub fn sync(&mut self, rooms: Vec<Room>, now: jiff::Timestamp) -> Vec<RoomChange> {
        let gone: Vec<RoomId> = self
            .shown
            .iter()
            .chain(self.hidden.values())
            .filter(|old| !rooms.iter().any(|room| room.id == old.id))
            .map(|old| old.id.clone())
            .collect();
        let mut changes: Vec<RoomChange> = gone.iter().filter_map(|id| self.remove(id)).collect();
        let mut rooms = rooms;
        rooms.sort_by(display_order);
        changes.extend(rooms.into_iter().filter_map(|room| self.upsert(room, now)));
        changes
    }

    /// `None` if there's no such room, or it was hidden: the frontend has no row to remove.
    pub fn remove(&mut self, id: &RoomId) -> Option<RoomChange> {
        self.drop_pending(id);
        let Some(index) = self.position(id) else {
            self.hidden_touched |= self.hidden.remove(id).is_some();
            return None;
        };
        self.shown.remove(index);
        Some(RoomChange::Removed { index })
    }

    /// Moves the room up if it had a message after what is known of it, and shows it if that
    /// brings it back. `None` if the room isn't known or already knew that time, or stays hidden.
    pub fn bump(
        &mut self,
        id: &RoomId,
        at: jiff::Timestamp,
        now: jiff::Timestamp,
    ) -> Option<RoomChange> {
        let room = self.get(id).filter(|room| room.last_activity < at)?;
        self.upsert(
            Room {
                last_activity: at,
                ..room.clone()
            },
            now,
        )
    }

    /// Gives the room its new topic, empty for none. `None` if the room isn't known, already has
    /// that topic, or stays hidden.
    pub fn set_topic(
        &mut self,
        id: &RoomId,
        topic: String,
        now: jiff::Timestamp,
    ) -> Option<RoomChange> {
        let room = self.get(id).filter(|room| room.topic != topic)?;
        self.upsert(
            Room {
                topic,
                ..room.clone()
            },
            now,
        )
    }

    /// Records which room the user has open, `None` for none. The open room is always shown, so
    /// one that was hidden comes back; the room that was open before is checked again and may go.
    pub fn set_open(&mut self, id: Option<RoomId>, now: jiff::Timestamp) -> Vec<RoomChange> {
        let before = std::mem::replace(&mut self.open, id.clone());
        let rooms: Vec<Room> = before
            .into_iter()
            .chain(id)
            .filter_map(|id| self.get(&id).cloned())
            .collect();
        rooms
            .into_iter()
            .filter_map(|room| self.upsert(room, now))
            .collect()
    }

    /// Checks the shown rooms again, as time has passed, and holds back the ones that went quiet.
    pub fn refresh(&mut self, now: jiff::Timestamp) -> Vec<RoomChange> {
        let quiet: Vec<Room> = self
            .shown
            .iter()
            .filter(|room| !self.is_shown_at(room, now))
            .cloned()
            .collect();
        quiet
            .into_iter()
            .filter_map(|room| self.upsert(room, now))
            .collect()
    }

    /// The first time a shown room will have to go, if time alone does it: the instant after the
    /// room has been quiet for `quiet_after`. `None` if no shown room can.
    #[must_use]
    pub fn next_quiet(&self) -> Option<jiff::Timestamp> {
        let limit = self
            .quiet_after
            .checked_add(jiff::SignedDuration::from_nanos(1))?;
        self.shown
            .iter()
            .filter(|room| !self.is_exempt(room))
            .filter_map(|room| room.last_activity.checked_add(limit).ok())
            .min()
    }

    /// The room with that id, shown or hidden, in its real state: the news held for it included,
    /// so callers merge what the server pushes onto what the server last said.
    #[must_use]
    pub fn get(&self, id: &RoomId) -> Option<&Room> {
        self.pending.get(id).or_else(|| {
            self.shown
                .iter()
                .find(|room| &room.id == id)
                .or_else(|| self.hidden.get(id))
        })
    }

    /// Whether the frontend shows the room.
    #[must_use]
    pub fn is_shown(&self, id: &RoomId) -> bool {
        self.position(id).is_some()
    }

    /// The index of a shown room. Hidden rooms have none.
    fn position(&self, id: &RoomId) -> Option<usize> {
        self.shown.iter().position(|room| &room.id == id)
    }

    /// Whether the room is shown whatever its age: anything but a direct message, or one the user
    /// cares about.
    fn is_exempt(&self, room: &Room) -> bool {
        room.kind != RoomKind::Direct
            || room.favorite
            || room.has_unread()
            || self.open.as_ref() == Some(&room.id)
    }

    /// Whether the room is shown at `now`: the exempt ones are, and so are the others unless they
    /// have been quiet for longer than `quiet_after`.
    fn is_shown_at(&self, room: &Room, now: jiff::Timestamp) -> bool {
        self.is_exempt(room) || now.duration_since(room.last_activity) <= self.quiet_after
    }

    /// Where `room` goes among the shown rooms as they are now.
    fn insertion_index(&self, room: &Room) -> usize {
        self.shown
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
            topic: String::new(),
            unread: 0,
            user_mentions: 0,
            group_mentions: 0,
            alert: false,
            favorite: false,
            unread_threads: Vec::new(),
            mentioned_threads: Vec::new(),
            last_activity: jiff::Timestamp::UNIX_EPOCH,
        }
    }

    fn at(secs: i64) -> jiff::Timestamp {
        jiff::Timestamp::from_second(secs).unwrap()
    }

    /// The time the tests run at, long after the rooms' epoch.
    fn now() -> jiff::Timestamp {
        at(1_000_000)
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
        list.reset(rooms, now());
        list
    }

    // ROOM-001
    #[test]
    fn reset_sorts_ignoring_case_and_breaks_ties_by_id() {
        let mut list = RoomList::default();
        let change = list.reset(
            vec![
                room("2", "bravo"),
                room("9", "alpha"),
                room("1", "Alpha"),
                room("3", "Charlie"),
            ],
            now(),
        );
        assert_eq!(names(&list), ["Alpha", "alpha", "bravo", "Charlie"]);
        assert_eq!(change, RoomChange::Reset(list.rooms().to_vec()));
    }

    // ROOM-001
    #[test]
    fn upsert_inserts_at_the_sorted_index() {
        let mut list = list_of(vec![room("a", "a"), room("c", "c")]);
        let change = list.upsert(room("b", "b"), now());
        assert_eq!(
            change,
            Some(RoomChange::Inserted {
                index: 1,
                item: room("b", "b")
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
            list.upsert(changed.clone(), now()),
            Some(RoomChange::Updated {
                index: 1,
                item: changed.clone()
            })
        );
        assert_eq!(list.rooms()[1], changed);
    }

    // ROOM-002
    #[test]
    fn upsert_with_a_rename_moves_the_room() {
        let mut list = list_of(vec![room("a", "a"), room("b", "b"), room("c", "c")]);
        // Down.
        let down = list.upsert(room("a", "d"), now());
        assert_eq!(
            down,
            Some(RoomChange::Moved {
                from: 0,
                to: 2,
                item: room("a", "d")
            })
        );
        assert_eq!(names(&list), ["b", "c", "d"]);
        // Up.
        let up = list.upsert(room("a", "a"), now());
        assert_eq!(
            up,
            Some(RoomChange::Moved {
                from: 2,
                to: 0,
                item: room("a", "a")
            })
        );
        assert_eq!(names(&list), ["a", "b", "c"]);
    }

    // ROOM-001
    #[test]
    fn upsert_of_an_identical_room_changes_nothing() {
        let mut list = list_of(vec![room("a", "a")]);
        assert_eq!(list.upsert(room("a", "a"), now()), None);
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
        let changes = list.sync(
            vec![
                busy.clone(),
                room("c", "c"),
                room("a", "e"),
                room("n", "bb"),
            ],
            now(),
        );
        assert_eq!(
            changes,
            [
                RoomChange::Removed { index: 1 },
                RoomChange::Inserted {
                    index: 1,
                    item: room("n", "bb")
                },
                RoomChange::Updated {
                    index: 3,
                    item: busy
                },
                RoomChange::Moved {
                    from: 0,
                    to: 3,
                    item: room("a", "e")
                },
            ]
        );
        assert_eq!(names(&list), ["bb", "c", "d", "e"]);
    }

    // ROOM-001
    #[test]
    fn sync_of_the_same_rooms_changes_nothing() {
        let mut list = list_of(vec![room("a", "a"), room("b", "b")]);
        assert_eq!(list.sync(vec![room("b", "b"), room("a", "a")], now()), []);
        assert_eq!(names(&list), ["a", "b"]);
    }

    // ROOM-001
    #[test]
    fn sync_of_an_empty_snapshot_removes_every_room() {
        let mut list = list_of(vec![room("a", "a"), room("b", "b")]);
        assert_eq!(
            list.sync(Vec::new(), now()),
            [
                RoomChange::Removed { index: 0 },
                RoomChange::Removed { index: 0 }
            ]
        );
        assert_eq!(list.sync(Vec::new(), now()), []);
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
            for change in list.sync(snapshot.clone(), now()) {
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
            Some(l.reset(vec![room("c", "Charlie"), room("a", "alpha")], now()))
        });
        step(l, m, |l| l.upsert(room("b", "bravo"), now()));
        step(l, m, |l| l.upsert(room("d", "alpha"), now()));
        step(l, m, |l| l.upsert(read, now()));
        step(l, m, |l| l.upsert(room("c", "Aardvark"), now()));
        step(l, m, |l| l.upsert(room("c", "zulu"), now()));
        step(l, m, |l| l.upsert(room("c", "zulu"), now()));
        step(l, m, |l| l.remove(&id("a")));
        step(l, m, |l| l.remove(&id("nope")));
        step(l, m, |l| {
            Some(l.reset(vec![room("x", "x"), room("y", "Y")], now()))
        });
        step(l, m, |l| l.remove(&id("x")));
        step(l, m, |l| Some(l.reset(Vec::new(), now())));
    }

    // ROOM-005
    #[test]
    fn the_most_recently_active_room_comes_first_then_name_then_id() {
        let mut list = RoomList::default();
        list.reset(
            vec![
                active("1", "old", 10),
                active("2", "beta", 20),
                active("3", "alpha", 20),
                active("0", "alpha", 20),
                active("4", "new", 30),
            ],
            now(),
        );
        assert_eq!(names(&list), ["new", "alpha", "alpha", "beta", "old"]);
        let ids: Vec<_> = list.rooms().iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["4", "0", "3", "2", "1"]);
    }

    fn favorite(room: Room) -> Room {
        Room {
            favorite: true,
            ..room
        }
    }

    // ROOM-006
    #[test]
    fn favorites_come_first_each_group_ordered_by_activity_then_name_then_id() {
        let mut list = RoomList::default();
        list.reset(
            vec![
                active("1", "recent", 50),
                favorite(active("2", "old favorite", 10)),
                active("3", "older", 5),
                favorite(active("4", "new favorite", 30)),
                favorite(active("5", "Alpha", 10)),
            ],
            now(),
        );
        assert_eq!(
            names(&list),
            ["new favorite", "Alpha", "old favorite", "recent", "older"]
        );
    }

    // ROOM-006
    #[test]
    fn a_new_favorite_moves_up_above_more_recent_rooms() {
        let mut list = list_of(vec![
            active("a", "a", 30),
            active("b", "b", 20),
            active("c", "c", 10),
        ]);
        let change = list.upsert(favorite(active("c", "c", 10)), now());
        assert_eq!(
            change,
            Some(RoomChange::Moved {
                from: 2,
                to: 0,
                item: favorite(active("c", "c", 10))
            })
        );
        assert_eq!(names(&list), ["c", "a", "b"]);
    }

    // ROOM-006
    #[test]
    fn a_favorite_that_stays_first_updates_in_place_and_one_taken_back_moves_down() {
        let mut list = list_of(vec![favorite(active("a", "a", 10)), active("b", "b", 20)]);
        assert_eq!(names(&list), ["a", "b"]);
        let mut busy = favorite(active("a", "a", 10));
        busy.unread = 2;
        assert_eq!(
            list.upsert(busy.clone(), now()),
            Some(RoomChange::Updated {
                index: 0,
                item: busy
            })
        );
        assert_eq!(
            list.upsert(active("a", "a", 10), now()),
            Some(RoomChange::Moved {
                from: 0,
                to: 1,
                item: active("a", "a", 10)
            })
        );
        assert_eq!(names(&list), ["b", "a"]);
    }

    // ROOM-006
    #[test]
    fn a_bump_does_not_lift_a_room_above_the_favorites() {
        let mut list = list_of(vec![
            favorite(active("f", "f", 10)),
            active("a", "a", 30),
            active("b", "b", 20),
        ]);
        let change = list.bump(&"b".to_owned().into(), at(40), now());
        assert_eq!(
            change,
            Some(RoomChange::Moved {
                from: 2,
                to: 1,
                item: active("b", "b", 40)
            })
        );
        assert_eq!(names(&list), ["f", "b", "a"]);
        // A favorite bumped stays among the favorites, above everything else.
        list.bump(&"f".to_owned().into(), at(60), now());
        assert_eq!(names(&list), ["f", "b", "a"]);
    }

    // ROOM-006
    #[test]
    fn sync_moves_the_rooms_whose_favorite_flag_changed() {
        let mut list = list_of(vec![active("a", "a", 30), active("b", "b", 20)]);
        let changes = list.sync(
            vec![active("a", "a", 30), favorite(active("b", "b", 20))],
            now(),
        );
        assert_eq!(
            changes,
            [RoomChange::Moved {
                from: 1,
                to: 0,
                item: favorite(active("b", "b", 20))
            }]
        );
    }

    // ROOM-006
    #[test]
    fn a_mirror_following_the_favorite_changes_equals_the_list() {
        let (mut list, mut mirror) = (RoomList::default(), Vec::new());
        let l = &mut list;
        let m = &mut mirror;
        step(l, m, |l| {
            Some(l.reset(
                vec![
                    active("a", "a", 10),
                    favorite(active("b", "b", 20)),
                    active("c", "c", 30),
                ],
                now(),
            ))
        });
        step(l, m, |l| l.upsert(favorite(active("a", "a", 10)), now()));
        step(l, m, |l| l.upsert(favorite(active("a", "a", 10)), now()));
        step(l, m, |l| l.upsert(active("b", "b", 20), now()));
        step(l, m, |l| l.upsert(favorite(active("d", "d", 5)), now()));
        step(l, m, |l| l.bump(&"c".to_owned().into(), at(99), now()));
        step(l, m, |l| l.remove(&"a".to_owned().into()));
        assert_eq!(names(&list), ["d", "c", "b"]);
    }

    // ROOM-007
    #[test]
    fn room_actions_say_why_they_were_refused() {
        let text = |action| RoomActionError::NotAllowed(action).to_string();
        assert_eq!(
            text(RoomAction::Favorite),
            "This room can't be added to the favorites"
        );
        assert_eq!(
            text(RoomAction::Unfavorite),
            "This room can't be removed from the favorites"
        );
        assert_eq!(
            text(RoomAction::MarkRead),
            "This room can't be marked as read"
        );
        assert_eq!(
            text(RoomAction::MarkUnread),
            "This room can't be marked as unread"
        );
        assert_eq!(text(RoomAction::Leave), "This room can't be left");
        assert_eq!(
            RoomActionError::from(&ApiError::Unreachable("refused".into())).to_string(),
            "Can't reach the server"
        );
        let server = ApiError::Server {
            status: 400,
            message: "You are the last owner".into(),
        };
        assert_eq!(
            RoomActionError::from(&server).to_string(),
            "The server reported an error: the server answered 400: You are the last owner"
        );
    }

    // ROOM-005
    #[test]
    fn bump_moves_the_room_to_the_front() {
        let mut list = list_of(vec![
            active("a", "a", 30),
            active("b", "b", 20),
            active("c", "c", 10),
        ]);
        let change = list.bump(&"c".to_owned().into(), at(40), now());
        assert_eq!(
            change,
            Some(RoomChange::Moved {
                from: 2,
                to: 0,
                item: active("c", "c", 40)
            })
        );
        assert_eq!(names(&list), ["c", "a", "b"]);
    }

    // ROOM-005
    #[test]
    fn bump_within_the_same_place_updates_in_place() {
        let mut list = list_of(vec![active("a", "a", 30), active("b", "b", 20)]);
        assert_eq!(
            list.bump(&"a".to_owned().into(), at(35), now()),
            Some(RoomChange::Updated {
                index: 0,
                item: active("a", "a", 35)
            })
        );
    }

    // ROOM-005
    #[test]
    fn bump_ignores_old_times_and_unknown_rooms() {
        let mut list = list_of(vec![active("a", "a", 30), active("b", "b", 20)]);
        assert_eq!(list.bump(&"b".to_owned().into(), at(20), now()), None);
        assert_eq!(list.bump(&"b".to_owned().into(), at(5), now()), None);
        assert_eq!(list.bump(&"zzz".to_owned().into(), at(99), now()), None);
        assert_eq!(names(&list), ["a", "b"]);
    }

    // ROOM-009
    #[test]
    fn set_topic_updates_the_room_in_place_and_ignores_the_same_one_and_unknown_rooms() {
        let mut list = list_of(vec![active("a", "a", 30), active("b", "b", 20)]);
        let with_topic = Room {
            topic: "Plans".to_owned(),
            ..active("b", "b", 20)
        };
        assert_eq!(
            list.set_topic(&"b".to_owned().into(), "Plans".to_owned(), now()),
            Some(RoomChange::Updated {
                index: 1,
                item: with_topic
            })
        );
        assert_eq!(
            list.set_topic(&"b".to_owned().into(), "Plans".to_owned(), now()),
            None
        );
        assert_eq!(
            list.set_topic(&"zzz".to_owned().into(), "Plans".to_owned(), now()),
            None
        );
        // Taking it away is a change too.
        assert_eq!(
            list.set_topic(&"b".to_owned().into(), String::new(), now()),
            Some(RoomChange::Updated {
                index: 1,
                item: active("b", "b", 20)
            })
        );
    }

    // ROOM-005
    #[test]
    fn sync_moves_rooms_whose_activity_changed() {
        let mut list = list_of(vec![active("a", "a", 30), active("b", "b", 20)]);
        let changes = list.sync(vec![active("a", "a", 30), active("b", "b", 40)], now());
        assert_eq!(
            changes,
            [RoomChange::Moved {
                from: 1,
                to: 0,
                item: active("b", "b", 40)
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
            Some(l.reset(
                vec![
                    active("a", "a", 10),
                    active("b", "b", 20),
                    active("c", "c", 30),
                ],
                now(),
            ))
        });
        step(l, m, |l| l.bump(&id("a"), at(40), now()));
        step(l, m, |l| l.bump(&id("c"), at(50), now()));
        step(l, m, |l| l.bump(&id("c"), at(50), now()));
        step(l, m, |l| l.bump(&id("b"), at(45), now()));
        step(l, m, |l| l.upsert(active("d", "d", 47), now()));
        step(l, m, |l| l.remove(&id("c")));
        assert_eq!(names(&list), ["d", "b", "a"]);
    }

    // ACCT-004
    #[test]
    fn the_unread_totals_count_the_unread_rooms_and_sum_the_mention_badges() {
        assert_eq!(RoomList::default().unread(), Unread::default());
        let list = list_of(vec![
            room("a", "a"),
            Room {
                unread: 5,
                user_mentions: 2,
                ..room("b", "b")
            },
            Room {
                user_mentions: 1,
                alert: true,
                ..room("c", "c")
            },
            Room {
                group_mentions: 4,
                ..room("d", "d")
            },
        ]);
        assert_eq!(
            list.unread(),
            Unread {
                rooms: 3,
                mentions: 3
            }
        );
    }

    /// How long the lists of the quiet tests let a direct message be quiet.
    const QUIET: i64 = 100;

    /// A direct message whose last activity was `age` seconds before [`now`].
    fn dm(id: &str, name: &str, age: i64) -> Room {
        Room {
            kind: RoomKind::Direct,
            last_activity: now() - jiff::SignedDuration::from_secs(age),
            ..room(id, name)
        }
    }

    /// A list that holds direct messages back after [`QUIET`] seconds, holding `rooms`.
    fn quiet_list(rooms: Vec<Room>) -> RoomList {
        let mut list = RoomList::new(
            jiff::SignedDuration::from_secs(QUIET),
            jiff::SignedDuration::ZERO,
        );
        list.reset(rooms, now());
        list
    }

    fn id(id: &str) -> RoomId {
        id.to_owned().into()
    }

    // ROOM-008
    #[test]
    fn a_direct_message_quiet_for_longer_than_the_limit_is_left_out_but_still_known() {
        let list = quiet_list(vec![dm("q", "quiet", QUIET + 1), dm("n", "new", 5)]);
        assert_eq!(names(&list), ["new"]);
        assert_eq!(list.get(&id("q")).map(|r| r.name.as_str()), Some("quiet"));
        assert!(!list.is_shown(&id("q")));
        assert!(list.is_shown(&id("n")));
        assert!(!list.is_empty());
    }

    // ROOM-008
    #[test]
    fn a_direct_message_exactly_as_old_as_the_limit_is_still_shown() {
        let list = quiet_list(vec![dm("a", "exact", QUIET), dm("b", "over", QUIET + 1)]);
        assert_eq!(names(&list), ["exact"]);
    }

    // ROOM-008
    #[test]
    fn favorites_unread_rooms_and_other_kinds_are_shown_however_quiet() {
        let old = QUIET * 1000;
        let counted = Room {
            unread: 1,
            ..dm("u", "unread", old)
        };
        let mentioned = Room {
            user_mentions: 1,
            ..dm("m", "mentioned", old)
        };
        let alerted = Room {
            alert: true,
            ..dm("l", "alerted", old)
        };
        let channel = Room {
            last_activity: at(0),
            ..room("c", "channel")
        };
        let list = quiet_list(vec![
            favorite(dm("f", "favorite", old)),
            counted,
            mentioned,
            alerted,
            channel,
            dm("q", "quiet", old),
        ]);
        let mut shown = names(&list);
        shown.sort_unstable();
        assert_eq!(
            shown,
            ["alerted", "channel", "favorite", "mentioned", "unread"]
        );
    }

    // ROOM-008
    #[test]
    fn the_open_room_is_shown_however_quiet_and_is_hidden_again_when_left() {
        let mut list = quiet_list(vec![dm("a", "a", QUIET + 1), dm("b", "b", QUIET + 1)]);
        assert_eq!(names(&list), Vec::<&str>::new());
        // Opened while hidden: it comes back.
        assert_eq!(
            list.set_open(Some(id("a")), now()),
            [RoomChange::Inserted {
                index: 0,
                item: dm("a", "a", QUIET + 1)
            }]
        );
        // Another one opens: the first one goes, the second one comes.
        assert_eq!(
            list.set_open(Some(id("b")), now()),
            [
                RoomChange::Removed { index: 0 },
                RoomChange::Inserted {
                    index: 0,
                    item: dm("b", "b", QUIET + 1)
                }
            ]
        );
        assert_eq!(
            list.set_open(None, now()),
            [RoomChange::Removed { index: 0 }]
        );
        assert_eq!(list.set_open(None, now()), []);
        assert!(list.rooms().is_empty());
    }

    // ROOM-008
    #[test]
    fn leaving_a_room_that_is_not_quiet_hides_nothing() {
        let mut list = quiet_list(vec![dm("a", "a", 5), dm("b", "b", 6)]);
        assert_eq!(list.set_open(Some(id("a")), now()), []);
        assert_eq!(list.set_open(Some(id("b")), now()), []);
        assert_eq!(names(&list), ["a", "b"]);
    }

    // ROOM-008
    #[test]
    fn a_bump_brings_a_hidden_direct_message_back_at_the_place_of_its_time() {
        let mut list = quiet_list(vec![
            active("c1", "c1", 1_000_000 - 10),
            active("c2", "c2", 1_000_000 - 300),
            dm("h", "hidden", 500),
        ]);
        assert_eq!(names(&list), ["c1", "c2"]);
        // A time that is still quiet changes nothing.
        assert_eq!(list.bump(&id("h"), at(1_000_000 - 400), now()), None);
        assert_eq!(names(&list), ["c1", "c2"]);
        // A time between the two is the second place.
        let back = list.bump(&id("h"), at(1_000_000 - 50), now());
        assert_eq!(
            back,
            Some(RoomChange::Inserted {
                index: 1,
                item: Room {
                    last_activity: at(1_000_000 - 50),
                    ..dm("h", "hidden", 0)
                }
            })
        );
        assert_eq!(names(&list), ["c1", "hidden", "c2"]);
    }

    // ROOM-008
    #[test]
    fn a_hidden_direct_message_that_shows_unread_comes_back_with_an_insert() {
        let mut list = quiet_list(vec![dm("h", "hidden", QUIET + 1)]);
        let counted = Room {
            unread: 2,
            ..dm("h", "hidden", QUIET + 1)
        };
        assert_eq!(
            list.upsert(counted.clone(), now()),
            Some(RoomChange::Inserted {
                index: 0,
                item: counted.clone()
            })
        );
        // Read again, it is quiet again.
        assert_eq!(
            list.upsert(dm("h", "hidden", QUIET + 1), now()),
            Some(RoomChange::Removed { index: 0 })
        );
        // A hidden room that stays hidden changes nothing.
        assert_eq!(list.upsert(dm("h", "other name", QUIET + 5), now()), None);
        assert_eq!(
            list.get(&id("h")).map(|r| r.name.as_str()),
            Some("other name")
        );
    }

    // ROOM-008
    #[test]
    fn refresh_hides_the_rooms_that_went_quiet_with_removals() {
        let mut list = quiet_list(vec![
            dm("a", "a", 10),
            dm("b", "b", 60),
            favorite(dm("f", "f", 60)),
            active("c", "c", 0),
        ]);
        assert_eq!(list.refresh(now()), []);
        // 41 seconds on, "b" is 101 seconds old, "a" 51.
        let later = now() + jiff::SignedDuration::from_secs(41);
        let changes = list.refresh(later);
        assert_eq!(changes.len(), 1);
        assert!(matches!(changes[0], RoomChange::Removed { index: 2 }));
        assert_eq!(names(&list), ["f", "a", "c"]);
        assert_eq!(list.refresh(later), []);
        assert!(list.get(&id("b")).is_some());
    }

    // ROOM-008
    #[test]
    fn reset_and_sync_put_each_room_where_the_rule_says() {
        let mut list = quiet_list(vec![dm("a", "a", 10), dm("h", "h", QUIET + 1)]);
        assert_eq!(names(&list), ["a"]);
        let change = list.reset(vec![dm("x", "x", 1), dm("y", "y", QUIET + 9)], now());
        assert_eq!(change, RoomChange::Reset(vec![dm("x", "x", 1)]));
        assert!(list.get(&id("h")).is_none());
        assert!(list.get(&id("y")).is_some());

        // The snapshot drops "x" and the hidden "y", brings "h" with a message and "z" quiet, and
        // makes "w", which is shown, quiet.
        let mut list = quiet_list(vec![
            dm("x", "x", 1),
            dm("y", "y", QUIET + 9),
            dm("w", "w", 50),
            dm("v", "v", 2),
        ]);
        let changes = list.sync(
            vec![
                dm("w", "w", QUIET + 1),
                dm("v", "v", 2),
                dm("h", "h", 3),
                dm("z", "z", QUIET + 2),
            ],
            now(),
        );
        assert_eq!(
            changes,
            [
                RoomChange::Removed { index: 0 },
                RoomChange::Inserted {
                    index: 1,
                    item: dm("h", "h", 3)
                },
                RoomChange::Removed { index: 2 },
            ]
        );
        assert_eq!(names(&list), ["v", "h"]);
        assert!(list.get(&id("x")).is_none());
        assert!(list.get(&id("y")).is_none());
        assert!(list.get(&id("z")).is_some());
        // Gone from a snapshot, a hidden room is gone from the list too.
        assert_eq!(list.sync(vec![dm("v", "v", 2), dm("h", "h", 3)], now()), []);
        assert!(list.get(&id("z")).is_none());
    }

    // ROOM-008
    #[test]
    fn removing_a_hidden_room_tells_the_frontend_nothing_and_forgets_it() {
        let mut list = quiet_list(vec![dm("h", "h", QUIET + 1)]);
        assert_eq!(list.remove(&id("h")), None);
        assert!(list.get(&id("h")).is_none());
        assert!(list.is_empty());
    }

    // ROOM-010
    #[test]
    fn the_quiet_change_flag_is_set_by_what_touches_the_hidden_rooms_only() {
        let mut list = quiet_list(vec![dm("h", "h", QUIET + 1), dm("n", "n", 5)]);
        // A reset that held rooms back.
        assert_eq!(list.take_quiet_change().map(|rooms| rooms.len()), Some(1));
        // Taken, it is clear.
        assert_eq!(list.take_quiet_change(), None);
        // A bump of a shown room, a topic, a mark as unread: the hidden rooms are not touched.
        list.bump(&id("n"), now(), now());
        list.set_topic(&id("n"), "t".to_owned(), now());
        list.upsert(dm("n", "renamed", 4), now());
        list.set_open(Some(id("n")), now());
        list.set_open(None, now());
        assert_eq!(list.take_quiet_change(), None);
        // A hidden room replaced, staying hidden.
        list.set_topic(&id("h"), "t".to_owned(), now());
        assert_eq!(
            list.take_quiet_change().map(|rooms| rooms[0].topic.clone()),
            Some("t".to_owned())
        );
        // One that comes back, one that goes, one removed from the list, a snapshot.
        list.upsert(dm("h", "h", 1), now());
        assert_eq!(list.take_quiet_change(), Some(vec![]));
        list.upsert(dm("n", "n", QUIET + 1), now());
        assert_eq!(list.take_quiet_change().map(|rooms| rooms.len()), Some(1));
        list.remove(&id("n"));
        assert_eq!(list.take_quiet_change(), Some(vec![]));
        list.upsert(dm("x", "x", QUIET + 1), now());
        list.sync(vec![dm("h", "h", 1)], now());
        assert_eq!(list.take_quiet_change(), Some(vec![]));
        list.reset(vec![], now());
        assert_eq!(list.take_quiet_change(), None);
    }

    // ROOM-010
    #[test]
    fn the_quiet_rooms_come_most_recent_first_then_by_id() {
        let mut list = quiet_list(vec![
            dm("c", "c", QUIET + 30),
            dm("b", "b", QUIET + 10),
            dm("a", "a", QUIET + 10),
            dm("d", "d", QUIET + 20),
            dm("n", "new", 5),
        ]);
        let ids = |list: &RoomList| -> Vec<String> {
            list.quiet()
                .iter()
                .map(|room| room.id.to_string())
                .collect()
        };
        // The shown room is not among them.
        assert_eq!(ids(&list), ["a", "b", "d", "c"]);
        // A room that comes back leaves the set.
        list.upsert(dm("d", "d", 1), now());
        assert_eq!(ids(&list), ["a", "b", "c"]);
        assert_eq!(quiet_list(vec![dm("n", "new", 5)]).quiet(), []);
    }

    // ROOM-008
    #[test]
    fn the_next_quiet_time_is_the_first_instant_a_shown_room_may_go() {
        let list = quiet_list(vec![
            dm("a", "a", 10),
            dm("b", "b", 30),
            favorite(dm("f", "f", 90)),
            dm("h", "h", QUIET + 50),
            active("c", "c", 0),
        ]);
        // "b" is the oldest of the ones time can hide. It is quiet for longer than the limit one
        // nanosecond after it has been exactly as old as that.
        let expected = now() - jiff::SignedDuration::from_secs(30)
            + jiff::SignedDuration::from_secs(QUIET)
            + jiff::SignedDuration::from_nanos(1);
        assert_eq!(list.next_quiet(), Some(expected));
        assert!(list.is_shown(&id("b")));
        // That instant is when the room goes.
        let mut list = list;
        assert_eq!(
            list.refresh(expected - jiff::SignedDuration::from_nanos(1)),
            []
        );
        assert_eq!(list.refresh(expected).len(), 1);
        assert!(!list.is_shown(&id("b")));
        assert_eq!(
            quiet_list(vec![favorite(dm("f", "f", 1))]).next_quiet(),
            None
        );
        assert_eq!(RoomList::default().next_quiet(), None);
    }

    /// Like [`step`], for an action that makes several changes.
    fn steps(
        list: &mut RoomList,
        mirror: &mut Vec<Room>,
        action: impl FnOnce(&mut RoomList) -> Vec<RoomChange>,
    ) {
        for change in action(list) {
            change.apply(mirror);
        }
        assert_eq!(mirror, list.rooms());
    }

    // ROOM-008
    #[test]
    fn a_mirror_following_the_changes_equals_the_list_while_rooms_come_and_go() {
        let (mut list, mut mirror) = (
            RoomList::new(
                jiff::SignedDuration::from_secs(QUIET),
                jiff::SignedDuration::ZERO,
            ),
            Vec::new(),
        );
        let l = &mut list;
        let m = &mut mirror;
        let later = now() + jiff::SignedDuration::from_secs(60);
        step(l, m, |l| {
            Some(l.reset(
                vec![
                    dm("a", "a", 10),
                    dm("b", "b", QUIET + 1),
                    active("c", "c", 1_000_000 - 20),
                    dm("d", "d", 50),
                ],
                now(),
            ))
        });
        step(l, m, |l| l.bump(&id("b"), now(), now()));
        step(l, m, |l| l.upsert(dm("e", "e", QUIET + 5), now()));
        steps(l, m, |l| l.set_open(Some(id("e")), now()));
        steps(l, m, |l| l.set_open(Some(id("a")), now()));
        steps(l, m, |l| l.set_open(None, later));
        steps(l, m, |l| l.refresh(later));
        steps(l, m, |l| l.refresh(later));
        step(l, m, |l| l.remove(&id("c")));
        step(l, m, |l| l.remove(&id("e")));
        steps(l, m, |l| l.sync(vec![dm("a", "a", 1)], later));
    }

    /// How long the lists of the batch tests hold the news of channels.
    const BATCH: i64 = 100;

    fn batch_list(rooms: Vec<Room>) -> RoomList {
        let mut list = RoomList::new(QUIET_AFTER, jiff::SignedDuration::from_secs(BATCH));
        list.reset(rooms, now());
        list
    }

    fn rid(id: &str) -> RoomId {
        id.to_owned().into()
    }

    /// The room with `unread` messages and its last activity at `secs`.
    fn with_news(room: Room, unread: u32, secs: i64) -> Room {
        Room {
            unread,
            last_activity: at(secs),
            ..room
        }
    }

    fn bold(list: &RoomList, name: &str) -> bool {
        list.rooms()
            .iter()
            .find(|room| room.name == name)
            .is_some_and(Room::has_unread)
    }

    // ROOM-011
    #[test]
    fn the_news_of_a_channel_is_held_but_a_rename_shows() {
        let mut list = batch_list(vec![active("a", "a", 10), active("b", "b", 5)]);
        let change = list.upsert(with_news(active("b", "b", 5), 3, 20), now());
        // Nothing the user would notice: no bold, no move.
        assert_eq!(change, None);
        assert_eq!(names(&list), ["a", "b"]);
        assert!(!bold(&list, "b"));
        assert_eq!(list.unread(), Unread::default());
        // The real state is what `get` gives.
        assert_eq!(list.get(&rid("b")).map(|r| r.unread), Some(3));
        // A rename shows at once, with the news still held.
        let renamed = Room {
            name: "z".to_owned(),
            ..with_news(active("b", "b", 5), 3, 20)
        };
        let change = list.upsert(renamed, now());
        assert_eq!(
            change,
            Some(RoomChange::Updated {
                index: 1,
                item: Room {
                    name: "z".to_owned(),
                    ..active("b", "b", 5)
                }
            })
        );
        assert_eq!(list.get(&rid("b")).map(|r| r.unread), Some(3));
    }

    // ROOM-011
    #[test]
    fn private_groups_and_favorites_are_held_too() {
        let mut list = batch_list(vec![
            Room {
                kind: RoomKind::Private,
                ..active("p", "p", 10)
            },
            Room {
                favorite: true,
                ..active("f", "f", 5)
            },
        ]);
        for (key, secs) in [("p", 10), ("f", 5)] {
            let mut news = list.get(&rid(key)).unwrap().clone();
            news.unread = 1;
            news.last_activity = at(secs + 100);
            assert_eq!(list.upsert(news, now()), None);
        }
        assert!(!bold(&list, "p") && !bold(&list, "f"));
    }

    // ROOM-011
    #[test]
    fn release_shows_all_the_held_channels_together_and_moves_them() {
        let mut list = batch_list(vec![
            active("a", "a", 30),
            active("b", "b", 20),
            active("c", "c", 10),
        ]);
        list.upsert(with_news(active("c", "c", 10), 1, 50), now());
        list.upsert(with_news(active("b", "b", 20), 2, 40), now());
        assert_eq!(names(&list), ["a", "b", "c"]);
        let changes = list.release(now());
        assert_eq!(
            changes,
            vec![
                RoomChange::Moved {
                    from: 2,
                    to: 0,
                    item: with_news(active("c", "c", 10), 1, 50)
                },
                RoomChange::Moved {
                    from: 2,
                    to: 1,
                    item: with_news(active("b", "b", 20), 2, 40)
                },
            ]
        );
        assert_eq!(names(&list), ["c", "b", "a"]);
        assert_eq!(list.unread().rooms, 2);
        assert_eq!(list.next_release(), None);
        assert_eq!(list.release(now()), []);
    }

    // ROOM-011
    #[test]
    fn a_mention_shows_the_room_at_once_with_its_held_news() {
        for user in [true, false] {
            let mut list = batch_list(vec![active("a", "a", 30), active("b", "b", 20)]);
            list.upsert(with_news(active("b", "b", 20), 1, 40), now());
            let mut mentioned = with_news(active("b", "b", 20), 2, 45);
            if user {
                mentioned.user_mentions = 1;
            } else {
                mentioned.group_mentions = 1;
            }
            let change = list.upsert(mentioned.clone(), now());
            assert_eq!(
                change,
                Some(RoomChange::Moved {
                    from: 1,
                    to: 0,
                    item: mentioned
                })
            );
            assert_eq!(list.next_release(), None);
            assert_eq!(list.unread().rooms, 1);
        }
    }

    // ROOM-011
    #[test]
    fn a_direct_message_is_never_held() {
        let mut list = batch_list(vec![Room {
            kind: RoomKind::Direct,
            ..active("d", "d", 10)
        }]);
        let news = Room {
            kind: RoomKind::Direct,
            ..with_news(active("d", "d", 10), 1, 20)
        };
        assert!(list.upsert(news, now()).is_some());
        assert!(bold(&list, "d"));
        assert_eq!(list.next_release(), None);
    }

    // ROOM-011
    #[test]
    fn the_open_room_is_never_held_and_opening_a_held_room_shows_it_whole() {
        let mut list = batch_list(vec![active("a", "a", 30), active("b", "b", 20)]);
        list.set_open(Some(rid("a")), now());
        assert!(
            list.upsert(with_news(active("a", "a", 30), 1, 35), now())
                .is_some()
        );
        assert!(bold(&list, "a"));
        list.upsert(with_news(active("b", "b", 20), 4, 40), now());
        assert!(!bold(&list, "b"));
        let changes = list.set_open(Some(rid("b")), now());
        assert!(!changes.is_empty());
        assert_eq!(list.rooms()[0].name, "b");
        assert_eq!(list.rooms()[0].unread, 4);
        assert_eq!(list.next_release(), None);
    }

    // ROOM-011
    #[test]
    fn becoming_read_drops_the_held_news() {
        let mut list = batch_list(vec![with_news(active("a", "a", 30), 1, 30)]);
        list.upsert(with_news(active("a", "a", 30), 5, 40), now());
        assert!(list.next_release().is_some());
        let change = list.upsert(active("a", "a", 30), now());
        assert!(change.is_some());
        assert!(!bold(&list, "a"));
        assert_eq!(list.next_release(), None);
        assert_eq!(list.get(&rid("a")).map(|r| r.unread), Some(0));
        assert_eq!(list.release(now()), []);

        // Held while the row showed it read, then read elsewhere: shown at once, moved up by its
        // newer activity, and nothing is left to release.
        let mut list = batch_list(vec![active("a", "a", 30), active("b", "b", 20)]);
        list.upsert(with_news(active("b", "b", 20), 2, 40), now());
        let change = list.upsert(active("b", "b", 40), now());
        assert_eq!(
            change,
            Some(RoomChange::Moved {
                from: 1,
                to: 0,
                item: active("b", "b", 40)
            })
        );
        assert_eq!(list.next_release(), None);
        assert_eq!(list.release(now()), []);
    }

    // ROOM-011
    #[test]
    fn a_new_room_is_shown_at_once() {
        let mut list = batch_list(vec![]);
        let change = list.upsert(with_news(active("a", "a", 30), 2, 30), now());
        assert!(matches!(change, Some(RoomChange::Inserted { .. })));
        assert!(bold(&list, "a"));
        assert_eq!(list.next_release(), None);
    }

    // ROOM-011
    #[test]
    fn upsert_now_shows_the_news_and_drops_the_held() {
        let mut list = batch_list(vec![active("a", "a", 30), active("b", "b", 20)]);
        list.upsert(with_news(active("b", "b", 20), 1, 40), now());
        let unread = with_news(active("b", "b", 20), 1, 40);
        assert!(list.upsert_now(unread, now()).is_some());
        assert!(bold(&list, "b"));
        assert_eq!(list.next_release(), None);
        // Held again, then an action of the user on the other room leaves it held.
        list.upsert(with_news(active("a", "a", 30), 1, 50), now());
        assert!(
            list.upsert_now(with_news(active("b", "b", 20), 1, 40), now())
                .is_none()
        );
        assert!(!bold(&list, "a"));
        assert!(list.next_release().is_some());
    }

    // ROOM-011
    #[test]
    fn the_unread_totals_leave_out_held_news() {
        let mut list = batch_list(vec![active("a", "a", 30), active("b", "b", 20)]);
        list.upsert(with_news(active("a", "a", 30), 3, 40), now());
        assert_eq!(list.unread(), Unread::default());
        list.release(now());
        assert_eq!(list.unread().rooms, 1);
    }

    // ROOM-011
    #[test]
    fn the_next_release_follows_the_first_held_change() {
        let mut list = batch_list(vec![active("a", "a", 30), active("b", "b", 20)]);
        assert_eq!(list.next_release(), None);
        list.upsert(with_news(active("a", "a", 30), 1, 40), at(1_000));
        list.upsert(with_news(active("b", "b", 20), 1, 41), at(1_050));
        assert_eq!(list.next_release(), Some(at(1_000 + BATCH)));
        list.release(at(1_100));
        assert_eq!(list.next_release(), None);
        list.upsert(with_news(active("a", "a", 30), 2, 60), at(2_000));
        assert_eq!(list.next_release(), Some(at(2_000 + BATCH)));
    }

    // ROOM-012
    #[test]
    fn a_release_before_the_deadline_restarts_the_batch_at_the_next_held_change() {
        let mut list = batch_list(vec![active("a", "a", 30), active("b", "b", 20)]);
        assert!(!list.holds_news());
        list.upsert(with_news(active("a", "a", 30), 1, 40), at(1_000));
        assert!(list.holds_news());
        // By command, long before the deadline at 1_000 + BATCH.
        assert_eq!(list.release(at(1_010)).len(), 1);
        assert!(!list.holds_news());
        assert_eq!(list.next_release(), None);
        list.upsert(with_news(active("b", "b", 20), 1, 41), at(1_040));
        assert_eq!(list.next_release(), Some(at(1_040 + BATCH)));
    }

    // ROOM-011
    #[test]
    fn a_bump_and_a_subscription_while_held_keep_the_newest_activity() {
        let mut list = batch_list(vec![active("a", "a", 30), active("b", "b", 20)]);
        // A bump first, then the subscription push built on what `get` says.
        assert_eq!(list.bump(&rid("b"), at(50), now()), None);
        let mut pushed = list.get(&rid("b")).unwrap().clone();
        pushed.unread = 1;
        assert_eq!(list.upsert(pushed, now()), None);
        assert_eq!(list.get(&rid("b")).map(|r| r.last_activity), Some(at(50)));
        // The other way round.
        let mut pushed = list.get(&rid("a")).unwrap().clone();
        pushed.unread = 1;
        assert_eq!(list.upsert(pushed, now()), None);
        assert_eq!(list.bump(&rid("a"), at(60), now()), None);
        assert_eq!(names(&list), ["a", "b"]);
        assert_eq!(list.release(now()).len(), 2);
        assert_eq!(list.get(&rid("a")).map(|r| r.last_activity), Some(at(60)));
        assert_eq!(list.get(&rid("b")).map(|r| r.last_activity), Some(at(50)));
        assert_eq!(names(&list), ["a", "b"]);
    }

    // ROOM-011
    #[test]
    fn reset_and_remove_clear_the_held_news() {
        let mut list = batch_list(vec![active("a", "a", 30), active("b", "b", 20)]);
        list.upsert(with_news(active("a", "a", 30), 1, 40), now());
        list.upsert(with_news(active("b", "b", 20), 1, 41), now());
        list.remove(&rid("a"));
        assert!(list.get(&rid("a")).is_none());
        assert!(list.next_release().is_some());
        list.reset(vec![active("b", "b", 20)], now());
        assert_eq!(list.next_release(), None);
        assert_eq!(list.get(&rid("b")).map(|r| r.unread), Some(0));
        assert_eq!(list.release(now()), []);
    }

    // ROOM-011
    #[test]
    fn a_zero_interval_holds_nothing() {
        let mut list = RoomList::new(QUIET_AFTER, jiff::SignedDuration::ZERO);
        list.reset(vec![active("a", "a", 30)], now());
        assert!(
            list.upsert(with_news(active("a", "a", 30), 1, 40), now())
                .is_some()
        );
        assert!(bold(&list, "a"));
        assert_eq!(list.next_release(), None);
    }
}
