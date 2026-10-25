// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The model of the open room's thread list: the parents of its threads, the one with the most
//! recent reply first, held as the core's changes say and told to the view as the changes of the
//! rows they come to. A row is made when the view asks for it, so a long list costs only the rows
//! in sight.
//!
//! The unread marks of a row (ACT-011) will come from the room's subscription and not from the
//! list's changes, as in the Qt model: a `set_unread` that announces only the rows that look
//! different, next to `set_avatars`.

use std::cell::RefCell;

use jiff::Timestamp;
use jiff::tz::TimeZone;
use rc_core::domain::{AvatarOwner, Message};
use rc_core::threads::ThreadListChange;
use rc_view::avatar_view::{AvatarUrls, message_owner, owners, rows_using};
use rc_view::row_ops::{RowOp, row_ops};
use slint::{Model, ModelNotify, ModelTracker};

use crate::rows::thread_row;
use crate::ui::ThreadRow;

pub struct ThreadListModel {
    threads: RefCell<Vec<Message>>,
    avatars: RefCell<AvatarUrls>,
    /// Where the time of the last reply is read: the user's.
    zone: TimeZone,
    notify: ModelNotify,
}

impl Default for ThreadListModel {
    fn default() -> Self {
        Self {
            threads: RefCell::new(Vec::new()),
            avatars: RefCell::new(AvatarUrls::default()),
            zone: TimeZone::system(),
            notify: ModelNotify::default(),
        }
    }
}

impl ThreadListModel {
    /// Replaces the threads and the avatars, as one reset: another account is shown.
    pub fn show(&self, snapshot: &ThreadListChange, avatars: AvatarUrls) {
        *self.avatars.borrow_mut() = avatars;
        snapshot.apply(&mut self.threads.borrow_mut());
        self.notify.reset();
    }

    /// Applies the changes in order, and tells the view what changed.
    pub fn apply(&self, changes: &[ThreadListChange]) {
        for change in changes {
            let op = row_ops(change);
            change.apply(&mut self.threads.borrow_mut());
            match op {
                RowOp::Reset => self.notify.reset(),
                RowOp::Insert { row } => self.notify.row_added(row, 1),
                RowOp::Update { row } => self.notify.row_changed(row),
                RowOp::Remove { row } => self.notify.row_removed(row, 1),
                // Slint has no moves: the row goes and comes back.
                RowOp::Move { from, to, .. } => {
                    self.notify.row_removed(from, 1);
                    self.notify.row_added(to, 1);
                }
            }
        }
    }

    /// Takes new avatar files in, and tells the view which rows show them.
    pub fn set_avatars(&self, changes: &[(AvatarOwner, String)]) {
        self.avatars.borrow_mut().set(changes);
        let runs = {
            let threads = self.threads.borrow();
            rows_using(threads.len(), &owners(changes), |at| {
                threads.get(at).and_then(message_owner)
            })
        };
        for (first, last) in runs {
            for at in first..=last {
                self.notify.row_changed(at);
            }
        }
    }
}

impl Model for ThreadListModel {
    type Data = ThreadRow;

    fn row_count(&self) -> usize {
        self.threads.borrow().len()
    }

    fn row_data(&self, row: usize) -> Option<ThreadRow> {
        let threads = self.threads.borrow();
        let parent = threads.get(row)?;
        Some(thread_row(
            parent,
            &self.avatars.borrow(),
            &self.zone,
            Timestamp::now(),
        ))
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        &self.notify
    }
}

#[cfg(test)]
mod tests {
    use rc_core::domain::{Author, Delivery, MessageKind};

    use super::*;

    fn parent(id: &str, replies: u32) -> Message {
        Message {
            id: id.to_owned().into(),
            room_id: "r".to_owned().into(),
            author: Author {
                id: "u".to_owned().into(),
                username: "alice".to_owned(),
                display_name: None,
            },
            ts: "2026-10-20T10:00:00Z".parse().unwrap(),
            kind: MessageKind::Text(format!("text of {id}").into()),
            delivery: Delivery::Sent,
            attachments: Vec::new(),
            quotes: Vec::new(),
            edited: false,
            reactions: Vec::new(),
            thread_id: None,
            thread_shown: false,
            thread_replies: replies,
            thread_last_reply: "2026-10-20T11:00:00Z".parse().ok(),
            thread_followers: Vec::new(),
        }
    }

    fn shown(ids: &[&str]) -> ThreadListModel {
        let model = ThreadListModel::default();
        let parents = ids.iter().map(|id| parent(id, 1)).collect();
        model.show(&ThreadListChange::Reset(parents), AvatarUrls::default());
        model
    }

    fn ids(model: &ThreadListModel) -> Vec<String> {
        model.iter().map(|row| row.id.to_string()).collect()
    }

    // ACT-010
    #[test]
    fn a_row_says_who_started_the_thread_and_how_many_replies_it_has() {
        let model = ThreadListModel::default();
        model.show(
            &ThreadListChange::Reset(vec![parent("a", 1), parent("b", 3)]),
            AvatarUrls::default(),
        );

        let row = model.row_data(1).unwrap();

        assert_eq!(row.id, "b");
        assert_eq!(row.author, "alice");
        assert_eq!(row.text, "text of b");
        assert_eq!(row.summary, "3 replies");
        assert_eq!(model.row_data(0).unwrap().summary, "1 reply");
        assert_eq!(model.row_count(), 2);
        assert_eq!(model.row_data(2), None);
    }

    // ACT-010
    #[test]
    fn inserts_updates_removals_and_moves_keep_the_order_of_the_rest() {
        let model = shown(&["a", "b", "c"]);

        model.apply(&[ThreadListChange::Inserted {
            index: 0,
            item: parent("n", 1),
        }]);
        assert_eq!(ids(&model), ["n", "a", "b", "c"]);

        model.apply(&[ThreadListChange::Updated {
            index: 1,
            item: parent("a", 5),
        }]);
        assert_eq!(model.row_data(1).unwrap().summary, "5 replies");

        model.apply(&[ThreadListChange::Moved {
            from: 3,
            to: 0,
            item: parent("c", 2),
        }]);
        assert_eq!(ids(&model), ["c", "n", "a", "b"]);

        model.apply(&[ThreadListChange::Removed { index: 1 }]);
        assert_eq!(ids(&model), ["c", "a", "b"]);
    }

    // ACT-010
    #[test]
    fn showing_again_replaces_the_rows() {
        let model = shown(&["a", "b"]);

        model.show(
            &ThreadListChange::Reset(vec![parent("x", 1)]),
            AvatarUrls::default(),
        );

        assert_eq!(ids(&model), ["x"]);
    }

    // ACT-010
    #[test]
    fn a_new_avatar_shows_in_the_rows_of_its_author_only() {
        let model = ThreadListModel::default();
        let mut other = parent("b", 1);
        other.author.username = "bob".to_owned();
        model.show(
            &ThreadListChange::Reset(vec![parent("a", 1), other]),
            AvatarUrls::default(),
        );
        assert!(model.row_data(0).unwrap().avatar_url.is_empty());

        model.set_avatars(&[(
            AvatarOwner::User("alice".to_owned()),
            "file:///nowhere/alice.png".to_owned(),
        )]);

        assert_eq!(
            model.row_data(0).unwrap().avatar_url,
            "file:///nowhere/alice.png"
        );
        assert!(model.row_data(1).unwrap().avatar_url.is_empty());
    }
}
