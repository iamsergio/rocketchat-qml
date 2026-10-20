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
/// `Reset`, `Insert`, `Replace` and `Move` are where the change is applied to the model's list (see
/// [`apply_newest_first`]), the first one in the list of ops; `Update` only notifies.
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
    /// Applies the change, which replaces a message without moving it, then `dataChanged` for the
    /// rows `first..=last`, all roles: the neighbours of the message show a day separator that
    /// depends on its time.
    Replace { first: usize, last: usize },
    /// `beginMoveRows(from, from, destination)` / `endMoveRows` around the change.
    Move {
        from: usize,
        /// Qt's destination, see [`qt_move_destination`].
        destination: usize,
    },
}

/// The rows `first..=last` and their neighbours, within a list of `len` rows.
fn with_neighbours(first: usize, last: usize, len: usize) -> (usize, usize) {
    (
        first.saturating_sub(1),
        (last + 1).min(len.saturating_sub(1)),
    )
}

/// The notifications to make for `change`, applied to a list of `held` messages. A prepend of
/// older messages also updates the oldest message held (the last row): its day separator shows
/// only if it starts a day, which the new message before it decides. An insert in front of newer
/// messages does the same for the one right after it in time, and a move for the neighbours of
/// both places.
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
        TimelineChange::Inserted { index, messages } if !messages.is_empty() => {
            // The messages are oldest first, the rows newest first.
            let first = held - index;
            let mut ops = vec![MessageOp::Insert {
                first,
                last: first + messages.len() - 1,
            }];
            if first > 0 {
                ops.push(MessageOp::Update {
                    first: first - 1,
                    last: first - 1,
                });
            }
            ops
        }
        TimelineChange::Updated { index, .. } => {
            let row = held - 1 - index;
            let (first, last) = with_neighbours(row, row, held);
            vec![MessageOp::Replace { first, last }]
        }
        TimelineChange::Moved { from, to, .. } => {
            let (from_row, to_row) = (held - 1 - from, held - 1 - to);
            let (first, last) = with_neighbours(from_row.min(to_row), from_row.max(to_row), held);
            vec![
                MessageOp::Move {
                    from: from_row,
                    destination: qt_move_destination(from_row, to_row),
                },
                MessageOp::Update { first, last },
            ]
        }
        TimelineChange::Prepended(_)
        | TimelineChange::Appended(_)
        | TimelineChange::Inserted { .. } => Vec::new(),
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
        TimelineChange::Inserted {
            index,
            messages: new,
        } => {
            let row = messages.len() - index;
            messages.splice(row..row, new.iter().rev().cloned());
        }
        TimelineChange::Updated { index, message } => {
            let row = messages.len() - 1 - index;
            messages[row] = message.clone();
        }
        TimelineChange::Moved { from, to, message } => {
            let len = messages.len();
            messages.remove(len - 1 - from);
            messages.insert(len - 1 - to, message.clone());
        }
        // A change the core adds later: `message_ops` resets the model for it, but there is
        // nothing to rebuild the list from.
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use rc_core::domain::{Author, Delivery, Message, MessageKind, Room, RoomKind};

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
            last_activity: jiff::Timestamp::UNIX_EPOCH,
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
            delivery: Delivery::Sent,
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

    // SEND-001
    #[test]
    fn an_insert_in_front_of_the_unsent_messages_counts_rows_from_the_newest() {
        // Oldest first: a, b, p, q. `c` goes in at 2, and the pending ones stay newer than it.
        let change = TimelineChange::Inserted {
            index: 2,
            messages: vec![message("c")],
        };
        assert_eq!(
            message_ops(&change, 4),
            [
                MessageOp::Insert { first: 2, last: 2 },
                // The message after it in time starts or doesn't start a day because of it.
                MessageOp::Update { first: 1, last: 1 }
            ]
        );
        // At the very end it is an append.
        let at_end = TimelineChange::Inserted {
            index: 4,
            messages: vec![message("c"), message("d")],
        };
        assert_eq!(
            message_ops(&at_end, 4),
            [MessageOp::Insert { first: 0, last: 1 }]
        );
    }

    // SEND-001
    #[test]
    fn an_update_replaces_the_row_and_notifies_its_neighbours() {
        let change = TimelineChange::Updated {
            index: 1,
            message: message("p"),
        };
        // Oldest first: a, p, q, r: `p` is row 2 of 4.
        assert_eq!(
            message_ops(&change, 4),
            [MessageOp::Replace { first: 1, last: 3 }]
        );
        // The ends have one neighbour only.
        let newest = TimelineChange::Updated {
            index: 3,
            message: message("r"),
        };
        assert_eq!(
            message_ops(&newest, 4),
            [MessageOp::Replace { first: 0, last: 1 }]
        );
        let oldest = TimelineChange::Updated {
            index: 0,
            message: message("a"),
        };
        assert_eq!(
            message_ops(&oldest, 4),
            [MessageOp::Replace { first: 2, last: 3 }]
        );
        let only = TimelineChange::Updated {
            index: 0,
            message: message("a"),
        };
        assert_eq!(
            message_ops(&only, 1),
            [MessageOp::Replace { first: 0, last: 0 }]
        );
    }

    // SEND-001
    #[test]
    fn a_move_goes_by_rows_from_the_newest() {
        // Oldest first: a, b, c, p; `p` moves from 3 to 1, which is row 0 to row 2 of 4.
        let change = TimelineChange::Moved {
            from: 3,
            to: 1,
            message: message("p"),
        };
        assert_eq!(
            message_ops(&change, 4),
            [
                MessageOp::Move {
                    from: 0,
                    destination: 3
                },
                MessageOp::Update { first: 0, last: 3 }
            ]
        );
        // And up: from 1 to 2 is row 2 to row 1 of 4.
        let up = TimelineChange::Moved {
            from: 1,
            to: 2,
            message: message("p"),
        };
        assert_eq!(
            message_ops(&up, 4),
            [
                MessageOp::Move {
                    from: 2,
                    destination: 1
                },
                MessageOp::Update { first: 0, last: 3 }
            ]
        );
    }

    // SEND-001, SEND-002
    #[test]
    fn a_reversed_mirror_following_the_send_changes_equals_the_timeline_reversed() {
        use rc_core::domain::Delivery;
        use rc_core::ports::RoomHistory;
        use rc_core::timeline::Timeline;

        let unsent = |id: &str, secs: i64| Message {
            delivery: Delivery::Pending,
            ts: jiff::Timestamp::from_second(secs).unwrap(),
            ..message(id)
        };
        let sent = |id: &str, secs: i64| Message {
            ts: jiff::Timestamp::from_second(secs).unwrap(),
            ..message(id)
        };
        let mut timeline = Timeline::new("r".to_owned().into(), RoomHistory::default());
        let mut mirror = Vec::new();
        let check = |timeline: &Timeline, mirror: &mut Vec<Message>, changes: &[TimelineChange]| {
            for change in changes {
                // The ops are made against the list as it is before the change.
                let ops = message_ops(change, mirror.len());
                apply_newest_first(change, mirror);
                assert!(!ops.is_empty(), "{change:?}");
            }
            let reversed: Vec<_> = timeline.messages().iter().rev().cloned().collect();
            assert_eq!(*mirror, reversed, "{changes:?}");
        };
        let changes = timeline.merge_latest(vec![sent("a", 1), sent("b", 2)], false);
        check(&timeline, &mut mirror, &changes);
        let changes = vec![timeline.add_unsent(unsent("p", 10))];
        check(&timeline, &mut mirror, &changes);
        let changes = vec![timeline.add_unsent(unsent("q", 11))];
        check(&timeline, &mut mirror, &changes);
        let changes = timeline.merge_live(sent("c", 3));
        check(&timeline, &mut mirror, &changes);
        let changes: Vec<_> = timeline.settle(sent("q", 2)).into_iter().collect();
        check(&timeline, &mut mirror, &changes);
        let changes: Vec<_> = timeline.settle(sent("p", 12)).into_iter().collect();
        check(&timeline, &mut mirror, &changes);
        let ids: Vec<_> = mirror.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["p", "c", "q", "b", "a"]);
    }
}
