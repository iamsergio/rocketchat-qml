// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! How a [`RoomChange`] maps to the calls a `QAbstractListModel` makes around it. Plain Rust, so it
//! can be unit-tested without Qt.

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
/// `Insert` and `Reset` are where the change is applied to the model's list; `Update` comes after
/// it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageOp {
    /// `beginResetModel` / `endResetModel` around the change.
    Reset,
    /// `beginInsertRows(first, last)` / `endInsertRows` around the change.
    Insert { first: usize, last: usize },
    /// `dataChanged` for the rows `first..=last`, all roles, after the change.
    Update { first: usize, last: usize },
}

/// The notifications to make for `change`, applied to a list of `held` messages. A prepend also
/// updates the message that was first: its day separator shows only if it starts a day, which the
/// new message before it decides.
#[must_use]
pub fn message_ops(change: &TimelineChange, held: usize) -> Vec<MessageOp> {
    match change {
        TimelineChange::Prepended(older) if !older.is_empty() => {
            let last = older.len() - 1;
            let mut ops = vec![MessageOp::Insert { first: 0, last }];
            if held > 0 {
                ops.push(MessageOp::Update {
                    first: older.len(),
                    last: older.len(),
                });
            }
            ops
        }
        TimelineChange::Appended(newer) if !newer.is_empty() => vec![MessageOp::Insert {
            first: held,
            last: held + newer.len() - 1,
        }],
        TimelineChange::Prepended(_) | TimelineChange::Appended(_) => Vec::new(),
        // `Reset`, and whatever the core adds later.
        _ => vec![MessageOp::Reset],
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
    fn a_prepend_inserts_at_the_top_and_updates_the_old_first_row() {
        let older = TimelineChange::Prepended(vec![message("a"), message("b")]);
        assert_eq!(
            message_ops(&older, 5),
            [
                MessageOp::Insert { first: 0, last: 1 },
                MessageOp::Update { first: 2, last: 2 }
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
    fn an_append_inserts_after_the_last_row() {
        let newer = TimelineChange::Appended(vec![message("a"), message("b"), message("c")]);
        assert_eq!(
            message_ops(&newer, 4),
            [MessageOp::Insert { first: 4, last: 6 }]
        );
        assert_eq!(
            message_ops(&newer, 0),
            [MessageOp::Insert { first: 0, last: 2 }]
        );
    }

    // MSG-002
    #[test]
    fn empty_pages_notify_nothing() {
        assert!(message_ops(&TimelineChange::Prepended(Vec::new()), 3).is_empty());
        assert!(message_ops(&TimelineChange::Appended(Vec::new()), 3).is_empty());
    }

    /// Follows the ops on a list the way the model does and checks the rows seen by the views.
    #[test]
    fn the_ops_cover_exactly_the_rows_that_changed() {
        let before = vec![message("c"), message("d")];
        let change = TimelineChange::Prepended(vec![message("a"), message("b")]);
        let mut after = before.clone();
        change.apply(&mut after);
        let ops = message_ops(&change, before.len());
        let Some(MessageOp::Update { first, last }) = ops.last().copied() else {
            panic!("no update");
        };
        // The updated row is the old first message, now at its new index.
        assert_eq!((first, last), (2, 2));
        assert_eq!(after[first].id, before[0].id);
    }
}
