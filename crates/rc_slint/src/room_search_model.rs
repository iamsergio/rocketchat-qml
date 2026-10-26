// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The model of the room search results: the rooms that match the query, and the people and
//! channels the server's directory found, which the view lays out from the field upwards. It keeps
//! no copy of the account's rooms, only what it found, and each search replaces it as a whole: a
//! new query is a new result set, as the Qt model's reset. It also holds the keyboard cursor, which
//! only ever rests on a row that can be acted on.
//!
//! The rows are in the order the view lays them out top to bottom, which is the reverse of the
//! ranking: the best match is the last row, right above the field. The view can't bottom-align a
//! lazy list (SLINT.md), so it lays out every row with a plain `for`. Inside, as in the Qt model,
//! the rows are kept bottom to top, which is what the cursor helpers of `rc_view` count in.

use std::cell::{Cell, RefCell};

use rc_core::directory::DirectoryAction;
use rc_view::avatar_view::{AvatarUrls, owners, rows_using};
use rc_view::directory::{SearchRow as Found, first_actionable, next_actionable};
use slint::{Model, ModelNotify, ModelTracker};

use crate::rows::search_row;
use crate::ui::SearchRow;

#[derive(Default)]
pub struct RoomSearchModel {
    /// Bottom to top.
    rows: RefCell<Vec<Found>>,
    avatars: RefCell<AvatarUrls>,
    /// The directory action under way, if any.
    busy: RefCell<Option<DirectoryAction>>,
    /// The row of the cursor, counted as `rows` is.
    cursor: Cell<Option<usize>>,
    notify: ModelNotify,
}

impl RoomSearchModel {
    /// Replaces the results with `found`, top to bottom as `rc_view::directory::search_rows` makes
    /// them, as one reset. The cursor stays on its row if `keep_cursor` and the row is still
    /// there, else it goes to the best match.
    pub fn show(
        &self,
        mut found: Vec<Found>,
        avatars: AvatarUrls,
        busy: Option<DirectoryAction>,
        keep_cursor: bool,
    ) {
        found.reverse();
        let kept = self
            .cursor
            .get()
            .and_then(|at| self.rows.borrow().get(at).map(Found::key))
            .filter(|_| keep_cursor);
        let at = kept
            .and_then(|key| found.iter().position(|row| row.key() == key))
            .or_else(|| first_actionable(&found));
        *self.rows.borrow_mut() = found;
        *self.avatars.borrow_mut() = avatars;
        *self.busy.borrow_mut() = busy;
        self.cursor.set(at);
        self.notify.reset();
    }

    /// Takes the directory action under way in, or none, and tells the view which rows it changes.
    pub fn set_busy(&self, busy: Option<DirectoryAction>) {
        let changed: Vec<usize> = {
            let rows = self.rows.borrow();
            let is = |busy: &Option<DirectoryAction>, row: &Found| {
                busy.as_ref().is_some_and(|action| row.is_busy_with(action))
            };
            let old = self.busy.borrow();
            (0..rows.len())
                .filter(|at| is(&old, &rows[*at]) != is(&busy, &rows[*at]))
                .collect()
        };
        *self.busy.borrow_mut() = busy;
        for at in changed {
            self.notify.row_changed(self.screen_row(at));
        }
    }

    /// Whether a directory action is under way: the core ignores a second one.
    pub fn is_busy(&self) -> bool {
        self.busy.borrow().is_some()
    }

    /// Takes new avatar files in, and tells the view which rows show them.
    pub fn set_avatars(&self, changes: &[(rc_core::domain::AvatarOwner, String)]) {
        self.avatars.borrow_mut().set(changes);
        let runs = {
            let rows = self.rows.borrow();
            rows_using(rows.len(), &owners(changes), |at| match rows.get(at) {
                Some(Found::Room(room)) => Some(room.avatar_owner()),
                Some(Found::Person(person)) => {
                    Some(rc_core::domain::AvatarOwner::User(person.username.clone()))
                }
                Some(Found::Channel(channel)) => {
                    Some(rc_core::domain::AvatarOwner::Room(channel.id.clone()))
                }
                _ => None,
            })
        };
        for (first, last) in runs {
            for row in first..=last {
                self.notify.row_changed(self.screen_row(row));
            }
        }
    }

    /// Moves the cursor to the next row up the screen that can be acted on (or back down, if
    /// `up` is false), if there is one.
    pub fn move_cursor(&self, up: bool) {
        let Some(from) = self.cursor.get() else {
            return;
        };
        if let Some(to) = next_actionable(&self.rows.borrow(), from, up) {
            self.cursor.set(Some(to));
        }
    }

    /// The row of the cursor as the view counts them, top to bottom; -1 for none.
    pub fn cursor(&self) -> i32 {
        self.cursor
            .get()
            .map_or(-1, |at| i32::try_from(self.screen_row(at)).unwrap_or(-1))
    }

    /// The row at `screen` (counted top to bottom), if there is one.
    pub fn found_at(&self, screen: usize) -> Option<Found> {
        let rows = self.rows.borrow();
        rows.len()
            .checked_sub(screen + 1)
            .and_then(|at| rows.get(at))
            .cloned()
    }

    /// Counted top to bottom, which is how the view counts, from counted bottom to top.
    fn screen_row(&self, at: usize) -> usize {
        self.rows.borrow().len().saturating_sub(at + 1)
    }
}

impl Model for RoomSearchModel {
    type Data = SearchRow;

    fn row_count(&self) -> usize {
        self.rows.borrow().len()
    }

    fn row_data(&self, row: usize) -> Option<SearchRow> {
        self.found_at(row)
            .map(|found| search_row(&found, &self.avatars.borrow(), self.busy.borrow().as_ref()))
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        &self.notify
    }
}

#[cfg(test)]
mod tests {
    use rc_core::directory::{DirectoryAction, Person};
    use rc_core::domain::{Room, RoomKind};
    use rc_view::avatar_view::AvatarUrls;
    use rc_view::directory::{Group, SearchRow as Found};
    use slint::Model;

    use super::RoomSearchModel;
    use crate::ui::SearchKind;

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

    fn show(model: &RoomSearchModel, found: Vec<Found>, keep_cursor: bool) {
        model.show(found, AvatarUrls::default(), None, keep_cursor);
    }

    // ROOM-010
    #[test]
    fn the_best_match_is_the_last_row_and_the_cursor_starts_on_it() {
        let model = RoomSearchModel::default();

        // Top to bottom, as the rows of `rc_view` come: the best match is the last.
        show(
            &model,
            vec![Found::Room(room("next")), Found::Room(room("best"))],
            false,
        );

        assert_eq!(model.row_count(), 2);
        assert_eq!(model.row_data(1).unwrap().room.id, "best");
        assert_eq!(model.row_data(0).unwrap().room.id, "next");
        assert_eq!(model.cursor(), 1);
    }

    // ACT-007
    #[test]
    fn the_cursor_skips_the_labels_and_stays_on_its_row_through_a_refresh() {
        let model = RoomSearchModel::default();
        let rows = || {
            vec![
                Found::Label(Group::People),
                Found::Searching,
                Found::Label(Group::Rooms),
                Found::Room(room("a")),
                Found::Room(room("b")),
            ]
        };
        show(&model, rows(), false);
        assert_eq!(model.cursor(), 4);

        model.move_cursor(true);
        model.move_cursor(true);

        // The label and the searching row are not stops: the second Up has nowhere to go.
        assert_eq!(model.cursor(), 3);
        show(&model, rows(), true);
        assert_eq!(model.cursor(), 3);
        show(&model, rows(), false);
        assert_eq!(model.cursor(), 4);
        model.move_cursor(false);
        assert_eq!(model.cursor(), 4);
    }

    // ACT-007
    #[test]
    fn the_busy_row_is_the_one_the_action_is_about() {
        let model = RoomSearchModel::default();
        let person = |username: &str| {
            Found::Person(Person {
                username: username.to_owned(),
                name: None,
            })
        };
        show(&model, vec![person("alice"), person("bob")], false);
        assert!(!model.is_busy());

        model.set_busy(Some(DirectoryAction::OpenDirect("bob".to_owned())));

        assert!(model.is_busy());
        assert!(!model.row_data(0).unwrap().busy);
        let bob = model.row_data(1).unwrap();
        assert!(bob.busy);
        assert_eq!(bob.kind, SearchKind::Person);
        assert_eq!(bob.room.name, "bob");
    }
}
