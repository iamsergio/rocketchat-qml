// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The model of the open room's thread list: the parents of its threads, the one with the most
//! recent reply first, held as the core's changes say and told to the view as the changes of the
//! rows they come to. A row is made when the view asks for it, so a long list costs only the rows
//! in sight.
//!
//! The unread marks of a row (ACT-011) come from the room's subscription and not from the list's
//! changes, as in the Qt model: `set_unread` announces only the rows that look different, next to
//! `set_avatars`.

use std::cell::RefCell;

use jiff::Timestamp;
use jiff::tz::TimeZone;
use rc_core::domain::{AvatarOwner, Message};
use rc_core::threads::ThreadListChange;
use rc_view::avatar_view::{AvatarUrls, message_owner, owners, rows_using};
use rc_view::row_ops::{RowOp, row_ops};
use rc_view::thread_list_view::{ThreadUnread, rows_changed};
use slint::{Model, ModelNotify, ModelTracker};

use crate::rows::thread_row;
use crate::ui::ThreadRow;

pub struct ThreadListModel {
    threads: RefCell<Vec<Message>>,
    avatars: RefCell<AvatarUrls>,
    /// Which threads look unread, and mentioned: the open room's subscription says.
    unread: RefCell<ThreadUnread>,
    /// Where the time of the last reply is read: the user's.
    zone: TimeZone,
    notify: ModelNotify,
}

impl Default for ThreadListModel {
    fn default() -> Self {
        Self {
            threads: RefCell::new(Vec::new()),
            avatars: RefCell::new(AvatarUrls::default()),
            unread: RefCell::new(ThreadUnread::default()),
            zone: TimeZone::system(),
            notify: ModelNotify::default(),
        }
    }
}

impl ThreadListModel {
    /// Replaces the threads, the avatars and the unread marks, as one reset: another account is
    /// shown.
    pub fn show(&self, snapshot: &ThreadListChange, avatars: AvatarUrls, unread: ThreadUnread) {
        *self.avatars.borrow_mut() = avatars;
        *self.unread.borrow_mut() = unread;
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

    /// Takes in which threads are unread, and announces the rows that look different, and only
    /// those. Nothing is announced when the sets are the same, as when the event is about other
    /// rooms. The sets are compared row by row with a short scan of each, so it costs the rows
    /// times the unread threads, which are few.
    pub fn set_unread(&self, unread: ThreadUnread) {
        if *self.unread.borrow() == unread {
            return;
        }
        let runs = rows_changed(&self.threads.borrow(), &self.unread.borrow(), &unread);
        *self.unread.borrow_mut() = unread;
        for (first, last) in runs {
            for at in first..=last {
                self.notify.row_changed(at);
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
        let unread = self.unread.borrow();
        let mut row = thread_row(parent, &self.avatars.borrow(), &self.zone, Timestamp::now());
        row.unread = unread.is_unread(&parent.id);
        row.mentioned = unread.is_mentioned(&parent.id);
        Some(row)
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        &self.notify
    }
}

#[cfg(test)]
mod tests {
    use rc_core::domain::{Author, Delivery, MessageKind};

    use std::rc::Rc;

    use slint::ModelExt;

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
        model.show(
            &ThreadListChange::Reset(parents),
            AvatarUrls::default(),
            ThreadUnread::default(),
        );
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
            ThreadUnread::default(),
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
            ThreadUnread::default(),
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
            ThreadUnread::default(),
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

    fn ids_of(ids: &[&str]) -> Vec<rc_core::domain::MessageId> {
        ids.iter().map(|id| (*id).to_owned().into()).collect()
    }

    /// The ids of the rows the model announces as changed from now on, in order. A `FilterModel`
    /// asks its filter about a row again when the source says that row changed, which is all a
    /// test can see of the announcements.
    fn announced(model: &Rc<ThreadListModel>) -> Rc<RefCell<Vec<String>>> {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let log = Rc::clone(&seen);
        let view = Rc::clone(model).filter(move |row| {
            log.borrow_mut().push(row.id.to_string());
            true
        });
        // The view reads every row once as it is made.
        seen.borrow_mut().clear();
        // Kept alive by the model's tracker for as long as the model lives.
        std::mem::forget(view);
        seen
    }

    // ACT-011
    #[test]
    fn a_row_says_whether_it_is_unread_and_mentions_the_user() {
        let model = shown(&["a", "b", "c"]);
        model.set_unread(ThreadUnread {
            unread: ids_of(&["b", "c"]),
            mentioned: ids_of(&["c", "a"]),
        });

        let marks = |at| {
            let row = model.row_data(at).unwrap();
            (row.unread, row.mentioned)
        };

        assert_eq!(marks(0), (false, false));
        assert_eq!(marks(1), (true, false));
        // A mention counts only while the thread is unread.
        assert_eq!(marks(2), (true, true));
    }

    // ACT-011
    #[test]
    fn only_the_rows_that_look_different_are_announced_and_none_when_nothing_changed() {
        let model = Rc::new(shown(&["a", "b", "c", "d"]));
        let unread = ThreadUnread {
            unread: ids_of(&["b"]),
            mentioned: Vec::new(),
        };
        model.set_unread(unread.clone());
        let seen = announced(&model);

        // The same sets again, as when other rooms get messages.
        model.set_unread(unread.clone());
        assert!(seen.borrow().is_empty());

        // `b` is read, `c` is unread and mentions, `d` is not in the list's sets at all.
        model.set_unread(ThreadUnread {
            unread: ids_of(&["c", "x"]),
            mentioned: ids_of(&["c"]),
        });
        assert_eq!(*seen.borrow(), ["b", "c"]);

        // A thread that is mentioned and read, and one the list does not hold, change nothing.
        seen.borrow_mut().clear();
        model.set_unread(ThreadUnread {
            unread: ids_of(&["c"]),
            mentioned: ids_of(&["c", "d"]),
        });
        assert!(seen.borrow().is_empty());
    }
}
