// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The model of the open room's messages: an `rc_view` [`MessageList`] shown oldest first, which is
//! the order of the view (top to bottom). The list holds the messages newest first, so the row `r`
//! of the list is the model's index `len - 1 - r`; the core's changes are applied to the list and
//! told to the view as the changes of indices they come to.
//!
//! The view keeps its place when older messages are inserted at the top, which only it can do (it
//! knows the heights): this model counts the insertions at the oldest end ([`prepends`]) and the
//! resets ([`resets`]), which the view compares with the ones it has dealt with.
//!
//! [`prepends`]: MessageModel::prepends
//! [`resets`]: MessageModel::resets

use std::cell::{Cell, RefCell};

use rc_core::domain::AvatarOwner;
use rc_core::timeline::TimelineChange;
use rc_view::attachment_view::ImageUrls;
use rc_view::avatar_view::AvatarUrls;
use rc_view::message_list::MessageList;
use rc_view::row_ops::MessageOp;
use rc_view::viewer::Viewer;
use slint::{Model, ModelNotify, ModelTracker};

use crate::rich_markdown::Palette;
use crate::rows::message_row;
use crate::ui::MessageRow;

pub struct MessageModel {
    list: RefCell<MessageList>,
    /// The colours that the styled text of a body is written with.
    palette: Palette,
    /// How many times messages were inserted at the oldest end of a list that had some.
    prepends: Cell<i32>,
    /// How many times the whole list was replaced.
    resets: Cell<i32>,
    notify: ModelNotify,
}

impl MessageModel {
    /// An empty model.
    pub fn new(palette: Palette) -> Self {
        Self {
            list: RefCell::new(MessageList::default()),
            palette,
            prepends: Cell::new(0),
            resets: Cell::new(0),
            notify: ModelNotify::default(),
        }
    }

    /// How many insertions at the top the model has made so far (a count that wraps).
    pub fn prepends(&self) -> i32 {
        self.prepends.get()
    }

    /// How many times the model was replaced so far (a count that wraps).
    pub fn resets(&self) -> i32 {
        self.resets.get()
    }

    /// The index in the model of the list's `row`, in a list of `len` messages.
    const fn index(len: usize, row: usize) -> usize {
        len - 1 - row
    }

    /// Replaces everything with another account's (see [`MessageList::replace`]), as one reset.
    pub fn show(
        &self,
        snapshot: &TimelineChange,
        viewer: Viewer,
        avatars: AvatarUrls,
        images: ImageUrls,
    ) {
        self.list
            .borrow_mut()
            .replace(snapshot, viewer, avatars, images);
        self.reset();
    }

    fn reset(&self) {
        self.resets.set(self.resets.get().wrapping_add(1));
        self.notify.reset();
    }

    /// Applies the changes in order, and tells the view what changed.
    pub fn apply(&self, changes: &[TimelineChange]) {
        for change in changes {
            self.apply_one(change);
        }
    }

    fn apply_one(&self, change: &TimelineChange) {
        let ops = self.list.borrow().ops(change);
        if ops.is_empty() {
            // Nothing to tell, but keep the list what the core says it is.
            self.list.borrow_mut().apply(change);
        }
        for op in ops {
            match op {
                MessageOp::Reset => {
                    self.list.borrow_mut().apply(change);
                    self.reset();
                }
                MessageOp::Insert { first, last } => {
                    let before = self.list.borrow().row_count();
                    self.list.borrow_mut().apply(change);
                    let len = self.list.borrow().row_count();
                    self.notify
                        .row_added(Self::index(len, last), last - first + 1);
                    // The new rows are the first of the model, above rows that were there.
                    if before > 0 && first == before {
                        self.prepends.set(self.prepends.get().wrapping_add(1));
                    }
                }
                MessageOp::Remove { row } => {
                    let len = self.list.borrow().row_count();
                    self.list.borrow_mut().apply(change);
                    self.notify.row_removed(Self::index(len, row), 1);
                }
                MessageOp::Update { first, last } => self.rows_changed(first, last),
                MessageOp::Replace { first, last } => {
                    self.list.borrow_mut().apply(change);
                    self.rows_changed(first, last);
                }
                MessageOp::Move { from, destination } => {
                    // Qt's destination counts the list before the move.
                    let to = if destination > from {
                        destination - 1
                    } else {
                        destination
                    };
                    let len = self.list.borrow().row_count();
                    self.list.borrow_mut().apply(change);
                    // Slint has no moves: the row goes and comes back.
                    self.notify.row_removed(Self::index(len, from), 1);
                    self.notify.row_added(Self::index(len, to), 1);
                }
            }
        }
    }

    /// Tells the view that the rows `first..=last` of the list changed.
    fn rows_changed(&self, first: usize, last: usize) {
        let len = self.list.borrow().row_count();
        for row in (first..=last).filter(|&row| row < len) {
            self.notify.row_changed(Self::index(len, row));
        }
    }

    /// Shows the messages to someone else (see [`MessageList::set_viewer`]).
    pub fn set_viewer(&self, viewer: Viewer) {
        let changed = self.list.borrow_mut().set_viewer(viewer);
        if let Some((first, last)) = changed {
            self.rows_changed(first, last);
        }
    }

    /// Takes new avatar files in, and tells the view which rows show them.
    pub fn set_avatars(&self, changes: &[(AvatarOwner, String)]) {
        let runs = self.list.borrow_mut().set_avatars(changes);
        for (first, last) in runs {
            self.rows_changed(first, last);
        }
    }

    /// Takes new image files in, and tells the view which rows show them.
    pub fn set_images(&self, changes: &[(String, String)]) {
        let runs = self.list.borrow_mut().set_images(changes);
        for (first, last) in runs {
            self.rows_changed(first, last);
        }
    }
}

impl Model for MessageModel {
    type Data = MessageRow;

    fn row_count(&self) -> usize {
        self.list.borrow().row_count()
    }

    fn row_data(&self, index: usize) -> Option<MessageRow> {
        let list = self.list.borrow();
        let row = list.row_count().checked_sub(index + 1)?;
        message_row(&list, row, &self.palette)
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        &self.notify
    }
}

#[cfg(test)]
mod tests {
    use rc_core::domain::{Author, Delivery, Message, MessageKind};
    use slint::Color;

    use super::*;

    fn palette() -> Palette {
        Palette {
            mention: Color::default(),
            mention_me: Color::default(),
            channel: Color::default(),
            quote_bar: Color::default(),
        }
    }

    fn message(id: &str, ts: &str) -> Message {
        Message {
            id: id.to_owned().into(),
            room_id: "r".to_owned().into(),
            author: Author {
                id: "u".to_owned().into(),
                username: "alice".to_owned(),
                display_name: None,
            },
            ts: ts.parse().unwrap(),
            kind: MessageKind::Text(id.to_owned().into()),
            delivery: Delivery::Sent,
            attachments: Vec::new(),
            quotes: Vec::new(),
            edited: false,
            reactions: Vec::new(),
            thread_id: None,
            thread_shown: false,
            thread_replies: 0,
            thread_last_reply: None,
            thread_followers: Vec::new(),
        }
    }

    /// Messages "m0", "m1".. one day apart each, oldest first.
    fn days(ids: &[&str]) -> Vec<Message> {
        ids.iter()
            .enumerate()
            .map(|(day, id)| message(id, &format!("2026-10-{:02}T10:00:00Z", day + 1)))
            .collect()
    }

    fn reset(messages: Vec<Message>) -> TimelineChange {
        TimelineChange::Reset {
            room: None,
            messages,
        }
    }

    fn shown(messages: Vec<Message>) -> MessageModel {
        let model = MessageModel::new(palette());
        model.show(
            &reset(messages),
            Viewer::default(),
            AvatarUrls::default(),
            ImageUrls::default(),
        );
        model
    }

    /// The ids in the order the view shows them: oldest first.
    fn ids(model: &MessageModel) -> Vec<String> {
        model.iter().map(|row| row.id.to_string()).collect()
    }

    // MSG-001
    #[test]
    fn rows_run_from_the_oldest_message_to_the_newest() {
        let model = shown(days(&["a", "b", "c"]));

        assert_eq!(ids(&model), ["a", "b", "c"]);
        assert_eq!(model.row_count(), 3);
        assert_eq!(model.row_data(0).unwrap().body, "a");
        assert_eq!(model.row_data(3), None);
    }

    // MSG-001
    #[test]
    fn appended_messages_go_below_and_inserted_ones_in_time_order() {
        let model = shown(days(&["a", "c"]));

        model.apply(&[TimelineChange::Appended(vec![message(
            "d",
            "2026-10-09T10:00:00Z",
        )])]);
        assert_eq!(ids(&model), ["a", "c", "d"]);
        model.apply(&[TimelineChange::Inserted {
            index: 1,
            messages: vec![message("b", "2026-10-01T12:00:00Z")],
        }]);
        assert_eq!(ids(&model), ["a", "b", "c", "d"]);
    }

    // MSG-001
    #[test]
    fn updates_removals_and_moves_keep_the_order_of_the_rest() {
        let model = shown(days(&["a", "b", "c", "d"]));

        let mut edited = message("b", "2026-10-02T10:00:00Z");
        edited.kind = MessageKind::Text("edited".to_owned().into());
        model.apply(&[TimelineChange::Updated {
            index: 1,
            message: edited,
        }]);
        assert_eq!(model.row_data(1).unwrap().body, "edited");

        model.apply(&[TimelineChange::Removed { index: 0 }]);
        assert_eq!(ids(&model), ["b", "c", "d"]);

        model.apply(&[TimelineChange::Moved {
            from: 0,
            to: 2,
            message: message("b", "2026-10-09T10:00:00Z"),
        }]);
        assert_eq!(ids(&model), ["c", "d", "b"]);
    }

    // MSG-001
    #[test]
    fn showing_again_replaces_the_rows() {
        let model = shown(days(&["a", "b"]));

        model.show(
            &reset(days(&["x"])),
            Viewer::default(),
            AvatarUrls::default(),
            ImageUrls::default(),
        );

        assert_eq!(ids(&model), ["x"]);
    }

    // MSG-002
    #[test]
    fn older_messages_go_above_and_count_as_a_prepend_but_newer_ones_do_not() {
        let model = shown(days(&["c", "d"]));
        let resets = model.resets();
        assert_eq!(model.prepends(), 0);

        model.apply(&[TimelineChange::Prepended(days(&["a", "b"]))]);
        assert_eq!(ids(&model), ["a", "b", "c", "d"]);
        assert_eq!(model.prepends(), 1);

        model.apply(&[TimelineChange::Appended(vec![message(
            "e",
            "2026-10-09T10:00:00Z",
        )])]);
        assert_eq!(ids(&model), ["a", "b", "c", "d", "e"]);
        assert_eq!(model.prepends(), 1);
        assert_eq!(model.resets(), resets);
    }

    // MSG-002
    #[test]
    fn messages_into_an_empty_list_or_in_the_middle_are_not_prepends() {
        let model = shown(Vec::new());
        model.apply(&[TimelineChange::Prepended(days(&["b"]))]);
        assert_eq!(model.prepends(), 0);

        model.apply(&[TimelineChange::Inserted {
            index: 1,
            messages: vec![message("c", "2026-10-09T10:00:00Z")],
        }]);
        assert_eq!(ids(&model), ["b", "c"]);
        assert_eq!(model.prepends(), 0);

        // At the oldest end of a list that has messages, an insert is one.
        model.apply(&[TimelineChange::Inserted {
            index: 0,
            messages: vec![message("a", "2026-09-01T10:00:00Z")],
        }]);
        assert_eq!(ids(&model), ["a", "b", "c"]);
        assert_eq!(model.prepends(), 1);
    }

    // MSG-002
    #[test]
    fn a_reset_is_counted_and_a_prepend_of_nothing_changes_nothing() {
        let model = shown(days(&["a"]));
        let resets = model.resets();

        model.apply(&[TimelineChange::Prepended(Vec::new())]);
        assert_eq!((model.prepends(), model.resets()), (0, resets));
        model.apply(&[reset(days(&["b"]))]);
        assert_eq!((model.prepends(), model.resets()), (0, resets + 1));
        assert_eq!(ids(&model), ["b"]);
    }

    // MSG-004
    #[test]
    fn the_oldest_message_of_each_day_has_the_separator() {
        let model = shown(vec![
            message("a", "2026-10-01T09:00:00Z"),
            message("b", "2026-10-01T10:00:00Z"),
            message("c", "2026-10-03T10:00:00Z"),
        ]);
        let first_of_day = |model: &MessageModel| -> Vec<bool> {
            model.iter().map(|row| row.first_of_day).collect()
        };
        assert_eq!(first_of_day(&model), [true, false, true]);

        // An older message of the first day takes the separator from "a".
        model.apply(&[TimelineChange::Prepended(vec![message(
            "z",
            "2026-10-01T08:00:00Z",
        )])]);
        assert_eq!(ids(&model), ["z", "a", "b", "c"]);
        assert_eq!(first_of_day(&model), [true, false, false, true]);
    }

    // SEND-002
    #[test]
    fn a_message_shows_its_delivery_and_the_row_follows_it() {
        let mut unsent = message("a", "2026-10-01T09:00:00Z");
        unsent.delivery = Delivery::Pending;
        let model = shown(vec![unsent.clone()]);
        let delivery = |model: &MessageModel| model.row_data(0).unwrap().delivery.to_string();
        assert_eq!(delivery(&model), "pending");

        unsent.delivery = Delivery::Failed;
        model.apply(&[TimelineChange::Updated {
            index: 0,
            message: unsent.clone(),
        }]);
        assert_eq!(delivery(&model), "failed");

        unsent.delivery = Delivery::Sent;
        model.apply(&[TimelineChange::Updated {
            index: 0,
            message: unsent,
        }]);
        assert_eq!(delivery(&model), "sent");
    }

    // RICH-006
    #[test]
    fn an_edited_message_is_marked_and_the_row_follows_it() {
        let mut edited = message("a", "2026-10-01T09:00:00Z");
        let model = shown(vec![edited.clone(), message("b", "2026-10-01T10:00:00Z")]);
        assert!(!model.row_data(0).unwrap().edited);

        edited.edited = true;
        model.apply(&[TimelineChange::Updated {
            index: 0,
            message: edited,
        }]);
        assert!(model.row_data(0).unwrap().edited);
        assert!(!model.row_data(1).unwrap().edited);
    }
}
