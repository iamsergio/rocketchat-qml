// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The index-based changes that keep a frontend's copy of an ordered list in step, whatever the
//! list holds (the room list holds rooms, see [`RoomChange`](crate::rooms::RoomChange)). Frontends
//! mirror the list and never sort or diff it themselves.

/// One step for a frontend's mirror of a list of `T`. Apply the changes in order.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ListChange<T> {
    /// The whole list, in order.
    Reset(Vec<T>),
    Inserted {
        index: usize,
        item: T,
    },
    /// Same position, new content.
    Updated {
        index: usize,
        item: T,
    },
    /// The item moved; the content may have changed too. `to` is its index once the move is done
    /// (`Vec::remove(from)` then `Vec::insert(to, ..)`). Qt's `beginMoveRows` takes a different
    /// destination, so `rc_qt` converts.
    Moved {
        from: usize,
        to: usize,
        item: T,
    },
    Removed {
        index: usize,
    },
}

impl<T: Clone> ListChange<T> {
    /// Applies the change to a frontend's mirror of the list. The mirror must be in the state the
    /// change was made against, i.e. all earlier changes were applied in order, and the indices
    /// must be in range: a change that doesn't fit is a bug in the caller, and panics like the
    /// `Vec` operation it stands for.
    pub fn apply(&self, items: &mut Vec<T>) {
        match self {
            Self::Reset(all) => items.clone_from(all),
            Self::Inserted { index, item } => items.insert(*index, item.clone()),
            Self::Updated { index, item } => items[*index] = item.clone(),
            Self::Moved { from, to, item } => {
                items.remove(*from);
                items.insert(*to, item.clone());
            }
            Self::Removed { index } => {
                items.remove(*index);
            }
        }
    }
}
