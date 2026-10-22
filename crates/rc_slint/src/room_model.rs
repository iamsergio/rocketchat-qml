// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The model of the room list: the rooms of the current account, as the core's changes say, with
//! a heading row before each section while there are favorites. One list, so that one view scrolls
//! it all, and the view creates only the rows in sight: a row is made when it is asked for, so the
//! avatar of a room nobody looks at isn't loaded.
//!
//! The rows are, while some room is a favorite: the heading "Favorites", the favorites, the
//! heading "Rooms", the others; else only the rooms. The core keeps the favorites first, so a room
//! at index `i` is at row `i + 1` if it is a favorite, `i + 2` if not, and `i` with no headings.
//! A change of the room list is told to the view as the changes of rows it comes to, headings
//! included: an insertion, a removal, an update or a move never resets the model.

use std::cell::{Cell, RefCell};

use rc_core::domain::{AvatarOwner, Room};
use rc_core::rooms::RoomChange;
use rc_view::avatar_view::{AvatarUrls, owners, rows_using};
use rc_view::row_ops::{RowOp, row_ops};
use slint::{Model, ModelNotify, ModelTracker};

use crate::rows::room_row;
use crate::ui::{RoomRow, RoomSection};

#[derive(Default)]
pub struct RoomModel {
    rooms: RefCell<Vec<Room>>,
    avatars: RefCell<AvatarUrls>,
    /// How many of the rooms are favorites.
    favorites: Cell<usize>,
    /// Whether the headings are in the rows.
    headed: Cell<bool>,
    /// While headed, how many rows are between the two headings: the favorites. The heading
    /// "Rooms" is at row `1 + favorites_shown`. It differs from `favorites` between the
    /// notifications of one change.
    favorites_shown: Cell<usize>,
    notify: ModelNotify,
}

impl RoomModel {
    /// Replaces the rooms and their avatars, as one reset: another account is shown.
    pub fn show(&self, rooms: Vec<Room>, avatars: AvatarUrls) {
        *self.rooms.borrow_mut() = rooms;
        *self.avatars.borrow_mut() = avatars;
        self.reset();
    }

    /// Starts over from the rooms held.
    fn reset(&self) {
        let favorites = self
            .rooms
            .borrow()
            .iter()
            .filter(|room| room.favorite)
            .count();
        self.favorites.set(favorites);
        self.headed.set(favorites > 0);
        self.favorites_shown.set(favorites);
        self.notify.reset();
    }

    /// The row of the room at `at` in the list, if it is a favorite or not.
    fn row_of(&self, at: usize, favorite: bool) -> usize {
        match (self.headed.get(), favorite) {
            (false, _) => at,
            (true, true) => at + 1,
            (true, false) => at + 2,
        }
    }

    /// Applies the changes in order, and tells the view what changed.
    pub fn apply(&self, changes: &[RoomChange]) {
        for change in changes {
            let op = row_ops(change);
            // What the room at the index was, before the change: the rows it is at depend on it.
            let was = |at: usize| {
                self.rooms
                    .borrow()
                    .get(at)
                    .is_some_and(|room| room.favorite)
            };
            let before = match op {
                RowOp::Update { row: at }
                | RowOp::Remove { row: at }
                | RowOp::Move { from: at, .. } => was(at),
                RowOp::Reset | RowOp::Insert { .. } => false,
            };
            change.apply(&mut self.rooms.borrow_mut());
            let now = |at: usize| {
                self.rooms
                    .borrow()
                    .get(at)
                    .is_some_and(|room| room.favorite)
            };
            match op {
                RowOp::Reset => self.reset(),
                RowOp::Insert { row: at } => self.inserted(at, now(at)),
                RowOp::Remove { row: at } => self.removed(at, before),
                RowOp::Update { row: at } => {
                    self.notify.row_changed(self.row_of(at, before));
                    self.favorites.set(
                        (self.favorites.get() + usize::from(now(at)))
                            .saturating_sub(usize::from(before)),
                    );
                    self.fix_headings();
                }
                RowOp::Move { from, to, .. } => {
                    self.removed(from, before);
                    self.inserted(to, now(to));
                }
            }
        }
    }

    /// A room, favorite or not, was inserted at `at`.
    fn inserted(&self, at: usize, favorite: bool) {
        let row = self.row_of(at, favorite);
        if self.headed.get() && favorite {
            self.favorites_shown.set(self.favorites_shown.get() + 1);
        }
        self.favorites
            .set(self.favorites.get() + usize::from(favorite));
        self.notify.row_added(row, 1);
        self.fix_headings();
    }

    /// The room that was at `at`, a favorite or not, is gone from the rooms already.
    fn removed(&self, at: usize, favorite: bool) {
        let row = self.row_of(at, favorite);
        if self.headed.get() && favorite {
            self.favorites_shown
                .set(self.favorites_shown.get().saturating_sub(1));
        }
        self.favorites
            .set(self.favorites.get().saturating_sub(usize::from(favorite)));
        self.notify.row_removed(row, 1);
        self.fix_headings();
    }

    /// Makes the headings what the favorites say: there, or gone, and the second one in its place.
    fn fix_headings(&self) {
        let favorites = self.favorites.get();
        match (self.headed.get(), favorites > 0) {
            (false, true) => {
                self.headed.set(true);
                self.favorites_shown.set(favorites);
                self.notify.row_added(0, 1);
                self.notify.row_added(1 + favorites, 1);
            }
            (true, false) => {
                self.notify.row_removed(1 + self.favorites_shown.get(), 1);
                self.notify.row_removed(0, 1);
                self.headed.set(false);
                self.favorites_shown.set(0);
            }
            (true, true) if self.favorites_shown.get() != favorites => {
                self.notify.row_removed(1 + self.favorites_shown.get(), 1);
                self.favorites_shown.set(favorites);
                self.notify.row_added(1 + favorites, 1);
            }
            _ => {}
        }
    }

    /// Takes new avatar files in, and tells the view which rows show them.
    pub fn set_avatars(&self, changes: &[(AvatarOwner, String)]) {
        self.avatars.borrow_mut().set(changes);
        let runs = {
            let rooms = self.rooms.borrow();
            rows_using(rooms.len(), &owners(changes), |at| {
                rooms.get(at).map(Room::avatar_owner)
            })
        };
        for (first, last) in runs {
            for at in first..=last {
                let favorite = self
                    .rooms
                    .borrow()
                    .get(at)
                    .is_some_and(|room| room.favorite);
                self.notify.row_changed(self.row_of(at, favorite));
            }
        }
    }
}

impl Model for RoomModel {
    type Data = RoomRow;

    fn row_count(&self) -> usize {
        self.rooms.borrow().len() + if self.headed.get() { 2 } else { 0 }
    }

    fn row_data(&self, row: usize) -> Option<RoomRow> {
        let heading = |section| RoomRow {
            section,
            ..RoomRow::default()
        };
        let room = |at: usize| {
            self.rooms
                .borrow()
                .get(at)
                .map(|room| room_row(room, &self.avatars.borrow()))
        };
        if !self.headed.get() {
            return room(row);
        }
        let shown = self.favorites_shown.get();
        match row {
            0 => Some(heading(RoomSection::Favorites)),
            heading_row if heading_row == 1 + shown => Some(heading(RoomSection::Rooms)),
            before if before <= shown => room(before - 1),
            after => room(after - 2),
        }
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        &self.notify
    }
}
