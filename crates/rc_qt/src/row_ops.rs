// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! How a [`RoomChange`] maps to the calls a `QAbstractListModel` makes around it. Plain Rust, so it
//! can be unit-tested without Qt.

use rc_core::domain::Message;
use rc_core::rooms::RoomChange;
use rc_core::timeline::TimelineChange;

/// The notifications for one change. Rows are indices before the change is applied, except for
/// `Moved::to`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowOp {
    /// `beginResetModel` / `endResetModel` around the change.
    Reset,
    /// `beginInsertRows(row, row)` / `endInsertRows`.
    Insert { row: usize },
    /// `dataChanged` for the row, all roles, after the change.
    Update { row: usize },
    /// `beginRemoveRows(row, row)` / `endRemoveRows`.
    Remove { row: usize },
    /// `beginMoveRows(from, from, destination)` / `endMoveRows`, then `dataChanged` at `to` (the
    /// content may have changed too).
    Move {
        from: usize,
        /// Qt's destination, see [`qt_move_destination`].
        destination: usize,
        /// The row's index once the move is done.
        to: usize,
    },
}

/// The notifications to make for `change`.
#[must_use]
pub fn row_ops(change: &RoomChange) -> RowOp {
    match *change {
        RoomChange::Inserted { index, .. } => RowOp::Insert { row: index },
        RoomChange::Updated { index, .. } => RowOp::Update { row: index },
        RoomChange::Removed { index } => RowOp::Remove { row: index },
        RoomChange::Moved { from, to, .. } => RowOp::Move {
            from,
            destination: qt_move_destination(from, to),
            to,
        },
        // `Reset`, and whatever the core adds later: a reset is always right, as the change is
        // applied to the model's list in between.
        _ => RowOp::Reset,
    }
}

/// The `destinationChild` argument of `beginMoveRows` for moving the row at `from` so that it
/// ends up at `to`. Qt counts the destination in the list as it is before the move, so moving down
/// needs one more: the row goes in front of the one at `destination`.
#[must_use]
pub fn qt_move_destination(from: usize, to: usize) -> usize {
    if to > from { to + 1 } else { to }
}

/// The notifications for one [`TimelineChange`] of the message list, in the order to make them.
/// `Insert` and `Reset` are where the change is applied to the model's list (see
/// [`apply_newest_first`]); `Update` comes after it.
///
/// The model holds the messages newest first, so that the view can run bottom to top and keep its
/// place when older messages arrive: they go in after the last row, which is above everything on
/// screen. Rows are therefore counted from the newest message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageOp {
    /// `beginResetModel` / `endResetModel` around the change.
    Reset,
    /// `beginInsertRows(first, last)` / `endInsertRows` around the change.
    Insert { first: usize, last: usize },
    /// `dataChanged` for the rows `first..=last`, all roles, after the change.
    Update { first: usize, last: usize },
}

/// The notifications to make for `change`, applied to a list of `held` messages. A prepend of
/// older messages also updates the oldest message held (the last row): its day separator shows
/// only if it starts a day, which the new message before it decides.
#[must_use]
pub fn message_ops(change: &TimelineChange, held: usize) -> Vec<MessageOp> {
    match change {
        TimelineChange::Prepended(older) if !older.is_empty() => {
            let mut ops = vec![MessageOp::Insert {
                first: held,
                last: held + older.len() - 1,
            }];
            if held > 0 {
                ops.push(MessageOp::Update {
                    first: held - 1,
                    last: held - 1,
                });
            }
            ops
        }
        TimelineChange::Appended(newer) if !newer.is_empty() => vec![MessageOp::Insert {
            first: 0,
            last: newer.len() - 1,
        }],
        TimelineChange::Prepended(_) | TimelineChange::Appended(_) => Vec::new(),
        // `Reset`, and whatever the core adds later.
        _ => vec![MessageOp::Reset],
    }
}

/// Applies `change` to a newest-first copy of the message list, which is how the model holds it.
/// The core's changes are oldest first, so each one is reversed on the way in.
pub fn apply_newest_first(change: &TimelineChange, messages: &mut Vec<Message>) {
    match change {
        TimelineChange::Reset { messages: all, .. } => {
            messages.clear();
            messages.extend(all.iter().rev().cloned());
        }
        TimelineChange::Prepended(older) => messages.extend(older.iter().rev().cloned()),
        TimelineChange::Appended(newer) => {
            messages.splice(0..0, newer.iter().rev().cloned());
        }
        // A change the core adds later: `message_ops` resets the model for it, but there is
        // nothing to rebuild the list from.
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use rc_core::domain::{Author, Message, MessageKind, Room, RoomKind};

    use super::*;

    fn room() -> Room {
        Room {
            id: "r".to_owned().into(),
            kind: RoomKind::Channel,
            name: "r".to_owned(),
            unread: 0,
            user_mentions: 0,
            group_mentions: 0,
            alert: false,
        }
    }

    // ROOM-001
    #[test]
    fn maps_every_kind_of_change() {
        assert_eq!(row_ops(&RoomChange::Reset(vec![room()])), RowOp::Reset);
        assert_eq!(
            row_ops(&RoomChange::Inserted {
                index: 4,
                room: room()
            }),
            RowOp::Insert { row: 4 }
        );
        assert_eq!(
            row_ops(&RoomChange::Updated {
                index: 2,
                room: room()
            }),
            RowOp::Update { row: 2 }
        );
        assert_eq!(
            row_ops(&RoomChange::Removed { index: 1 }),
            RowOp::Remove { row: 1 }
        );
        assert_eq!(
            row_ops(&RoomChange::Moved {
                from: 3,
                to: 0,
                room: room()
            }),
            RowOp::Move {
                from: 3,
                destination: 0,
                to: 0
            }
        );
    }

    // ROOM-002
    #[test]
    fn qt_move_destination_counts_before_the_move() {
        // Up: the row goes in front of the one now at `to`.
        assert_eq!(qt_move_destination(3, 1), 1);
        assert_eq!(qt_move_destination(1, 0), 0);
        // Down: past the row at `to`, which is one more.
        assert_eq!(qt_move_destination(0, 2), 3);
        assert_eq!(qt_move_destination(1, 2), 3);
    }

    // ROOM-002
    #[test]
    fn a_move_down_lands_where_the_core_put_it() {
        // Qt's semantics, on a plain list: take the row out, insert it in front of `destination`
        // (as counted before the move).
        let mut list = vec!['a', 'b', 'c', 'd'];
        let (from, to) = (0, 2);
        let destination = qt_move_destination(from, to);
        let row = list[from];
        list.insert(destination, row);
        list.remove(from);
        assert_eq!(list, ['b', 'c', 'a', 'd']);
        assert_eq!(list[to], 'a');
    }

    // ROOM-002
    #[test]
    fn a_move_up_lands_where_the_core_put_it() {
        let mut list = vec!['a', 'b', 'c', 'd'];
        let (from, to) = (3, 1);
        let destination = qt_move_destination(from, to);
        let row = list[from];
        list.insert(destination, row);
        list.remove(from + 1);
        assert_eq!(list, ['a', 'd', 'b', 'c']);
        assert_eq!(list[to], 'd');
    }

    fn message(id: &str) -> Message {
        Message {
            id: id.to_owned().into(),
            room_id: "r".to_owned().into(),
            author: Author {
                id: "u".to_owned().into(),
                username: "u".into(),
                display_name: None,
            },
            ts: jiff::Timestamp::UNIX_EPOCH,
            kind: MessageKind::Text(id.to_owned()),
        }
    }

    // MSG-001
    #[test]
    fn a_reset_resets_the_model() {
        let reset = TimelineChange::Reset {
            room: None,
            messages: vec![message("a")],
        };
        assert_eq!(message_ops(&reset, 3), [MessageOp::Reset]);
    }

    // MSG-002
    #[test]
    fn a_prepend_inserts_after_the_last_row_and_updates_it() {
        let older = TimelineChange::Prepended(vec![message("a"), message("b")]);
        assert_eq!(
            message_ops(&older, 5),
            [
                MessageOp::Insert { first: 5, last: 6 },
                MessageOp::Update { first: 4, last: 4 }
            ]
        );
        // Nothing was there before: no row to update.
        assert_eq!(
            message_ops(&older, 0),
            [MessageOp::Insert { first: 0, last: 1 }]
        );
    }

    // MSG-001
    #[test]
    fn an_append_inserts_at_row_zero() {
        let newer = TimelineChange::Appended(vec![message("a"), message("b"), message("c")]);
        assert_eq!(
            message_ops(&newer, 4),
            [MessageOp::Insert { first: 0, last: 2 }]
        );
    }

    // MSG-002
    #[test]
    fn empty_pages_notify_nothing() {
        assert!(message_ops(&TimelineChange::Prepended(Vec::new()), 3).is_empty());
        assert!(message_ops(&TimelineChange::Appended(Vec::new()), 3).is_empty());
    }

    fn ids(messages: &[Message]) -> Vec<&str> {
        messages.iter().map(|m| m.id.as_str()).collect()
    }

    // MSG-002
    #[test]
    fn the_updated_row_is_the_old_oldest_message() {
        let mut held = Vec::new();
        let first = TimelineChange::Reset {
            room: None,
            messages: vec![message("c"), message("d")],
        };
        apply_newest_first(&first, &mut held);
        let change = TimelineChange::Prepended(vec![message("a"), message("b")]);
        let ops = message_ops(&change, held.len());
        apply_newest_first(&change, &mut held);
        let Some(MessageOp::Update { first, .. }) = ops.last().copied() else {
            panic!("no update");
        };
        assert_eq!(ids(&held), ["d", "c", "b", "a"]);
        // `c`, the oldest of the messages that were held, keeps its row.
        assert_eq!(held[first].id.as_str(), "c");
    }

    // MSG-001, MSG-002
    #[test]
    fn a_reversed_mirror_equals_the_timeline_reversed() {
        let mut timeline = Vec::new();
        let mut mirror = Vec::new();
        let changes = [
            TimelineChange::Reset {
                room: None,
                messages: vec![message("e"), message("f")],
            },
            TimelineChange::Prepended(vec![message("c"), message("d")]),
            TimelineChange::Appended(vec![message("g")]),
            TimelineChange::Prepended(vec![message("a"), message("b")]),
            TimelineChange::Appended(vec![message("h"), message("i")]),
            TimelineChange::Reset {
                room: None,
                messages: vec![message("x")],
            },
            TimelineChange::Prepended(Vec::new()),
            TimelineChange::Reset {
                room: None,
                messages: Vec::new(),
            },
        ];
        for change in &changes {
            change.apply(&mut timeline);
            apply_newest_first(change, &mut mirror);
            let reversed: Vec<_> = timeline.iter().rev().cloned().collect();
            assert_eq!(mirror, reversed, "{change:?}");
        }
    }
}
