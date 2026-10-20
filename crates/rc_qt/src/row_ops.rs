// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! How a [`RoomChange`] maps to the calls a `QAbstractListModel` makes around it. Plain Rust, so it
//! can be unit-tested without Qt.

use rc_core::rooms::RoomChange;

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

#[cfg(test)]
mod tests {
    use rc_core::domain::{Room, RoomKind};

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
}
