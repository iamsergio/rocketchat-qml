// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The model of the room search results: the rooms that match the query, which the view lays out
//! from the field upwards. It keeps no copy of the account's rooms, only what it found, and each
//! search replaces it as a whole: a new query is a new result set, as the Qt model's reset.
//!
//! The rows are in the order the view lays them out top to bottom, which is the reverse of the
//! ranking: the best match is the last row, right above the field. The view can't bottom-align a
//! lazy list (SLINT.md), so it lays out every row with a plain `for`.

use std::cell::RefCell;

use rc_core::domain::{AvatarOwner, Room};
use rc_view::avatar_view::{AvatarUrls, owners, rows_using};
use slint::{Model, ModelNotify, ModelTracker};

use crate::rows::room_row;
use crate::ui::RoomRow;

#[derive(Default)]
pub struct RoomSearchModel {
    /// Worst match first.
    rooms: RefCell<Vec<Room>>,
    avatars: RefCell<AvatarUrls>,
    notify: ModelNotify,
}

impl RoomSearchModel {
    /// Replaces the results with `found`, best match first, as one reset.
    pub fn show(&self, mut found: Vec<Room>, avatars: AvatarUrls) {
        found.reverse();
        *self.rooms.borrow_mut() = found;
        *self.avatars.borrow_mut() = avatars;
        self.notify.reset();
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
            for row in first..=last {
                self.notify.row_changed(row);
            }
        }
    }
}

impl Model for RoomSearchModel {
    type Data = RoomRow;

    fn row_count(&self) -> usize {
        self.rooms.borrow().len()
    }

    fn row_data(&self, row: usize) -> Option<RoomRow> {
        self.rooms
            .borrow()
            .get(row)
            .map(|room| room_row(room, &self.avatars.borrow()))
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        &self.notify
    }
}

#[cfg(test)]
mod tests {
    use rc_core::domain::{Room, RoomKind};
    use rc_view::avatar_view::AvatarUrls;
    use slint::Model;

    use super::RoomSearchModel;

    fn room(id: &str) -> Room {
        Room {
            id: id.to_owned().into(),
            name: id.to_owned(),
            kind: RoomKind::Channel,
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

    // ROOM-010
    #[test]
    fn the_best_match_is_the_last_row() {
        let model = RoomSearchModel::default();

        model.show(vec![room("best"), room("next")], AvatarUrls::default());

        assert_eq!(model.row_count(), 2);
        assert_eq!(model.row_data(1).unwrap().id, "best");
        assert_eq!(model.row_data(0).unwrap().id, "next");
    }
}
