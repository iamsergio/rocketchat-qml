// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! What a message list holds, whatever toolkit shows it: the core's messages, held newest first
//! (row 0 is the newest), the zone they are read in, who they are shown to, and the avatar and
//! image URLs. It applies the core's changes and says which rows each update touches; the
//! notifications the toolkit wants around them are its own.

use jiff::tz::TimeZone;
use rc_core::domain::{AvatarOwner, Message};
use rc_core::timeline::TimelineChange;

use crate::attachment_view::{ImageUrls, paths, shows_image};
use crate::avatar_view::{AvatarUrls, message_owner, owners, rows_using, runs};
use crate::message_view::{first_of_day, thread_parent_text};
use crate::row_ops::{MessageOp, apply_newest_first, message_ops};
use crate::viewer::Viewer;

/// The messages of a list, mirrored from the core's changes and held newest first.
pub struct MessageList {
    messages: Vec<Message>,
    /// Where times and days are read: the user's.
    zone: TimeZone,
    /// Who the messages are shown to: it decides the mentions that stand out.
    viewer: Viewer,
    avatars: AvatarUrls,
    images: ImageUrls,
}

impl Default for MessageList {
    fn default() -> Self {
        Self::new(
            Viewer::default(),
            AvatarUrls::default(),
            ImageUrls::default(),
        )
    }
}

impl MessageList {
    /// An empty list, showing the messages to `viewer`.
    #[must_use]
    pub fn new(viewer: Viewer, avatars: AvatarUrls, images: ImageUrls) -> Self {
        Self {
            messages: Vec::new(),
            zone: TimeZone::system(),
            viewer,
            avatars,
            images,
        }
    }

    #[must_use]
    pub fn row_count(&self) -> usize {
        self.messages.len()
    }

    /// The notifications to make for `change`, which [`apply`](Self::apply) then applies.
    #[must_use]
    pub fn ops(&self, change: &TimelineChange) -> Vec<MessageOp> {
        message_ops(change, self.messages.len())
    }

    /// Applies `change` to the held messages.
    pub fn apply(&mut self, change: &TimelineChange) {
        apply_newest_first(change, &mut self.messages);
    }

    /// Replaces everything with another account's: the messages (a `Reset` snapshot), who they are
    /// shown to, and the avatars and images they show. The caller announces a model reset around
    /// it.
    pub fn replace(
        &mut self,
        snapshot: &TimelineChange,
        viewer: Viewer,
        avatars: AvatarUrls,
        images: ImageUrls,
    ) {
        (self.viewer, self.avatars, self.images) = (viewer, avatars, images);
        self.apply(snapshot);
    }

    /// Shows the messages to someone else, with other custom emoji or under another policy: what
    /// they show, and what the viewer can do with them, changes. Returns the rows to announce as
    /// changed (first and last), if there are any.
    pub fn set_viewer(&mut self, viewer: Viewer) -> Option<(usize, usize)> {
        if self.viewer == viewer {
            return None;
        }
        self.viewer = viewer;
        self.messages.len().checked_sub(1).map(|last| (0, last))
    }

    /// Takes new avatar files in. Returns the runs of rows (first and last) that show them.
    pub fn set_avatars(&mut self, changes: &[(AvatarOwner, String)]) -> Vec<(usize, usize)> {
        self.avatars.set(changes);
        rows_using(self.messages.len(), &owners(changes), |at| {
            self.messages.get(at).and_then(message_owner)
        })
    }

    /// Takes new image files in. Returns the runs of rows (first and last) whose message shows
    /// them.
    pub fn set_images(&mut self, changes: &[(String, String)]) -> Vec<(usize, usize)> {
        self.images.set(changes);
        let wanted = paths(changes);
        runs(self.messages.len(), |at| {
            self.messages
                .get(at)
                .is_some_and(|message| shows_image(message, &wanted))
        })
    }

    /// The message at `row`.
    #[must_use]
    pub fn message(&self, row: usize) -> Option<&Message> {
        self.messages.get(row)
    }

    /// Where times and days are read.
    #[must_use]
    pub fn zone(&self) -> &TimeZone {
        &self.zone
    }

    /// Who the messages are shown to.
    #[must_use]
    pub fn viewer(&self) -> &Viewer {
        &self.viewer
    }

    #[must_use]
    pub fn avatars(&self) -> &AvatarUrls {
        &self.avatars
    }

    #[must_use]
    pub fn images(&self) -> &ImageUrls {
        &self.images
    }

    /// Whether the message at `row` is the first of its day in the zone. The message before it is
    /// the next row: rows run from newest to oldest. `false` for a row that isn't there.
    #[must_use]
    pub fn first_of_day(&self, row: usize) -> bool {
        self.messages
            .get(row)
            .is_some_and(|message| first_of_day(self.messages.get(row + 1), message, &self.zone))
    }

    /// The text of the parent of the message at `row` if it is a thread reply (see
    /// [`thread_parent_text`]), among the held messages; empty otherwise.
    #[must_use]
    pub fn thread_parent_text(&self, row: usize) -> String {
        self.messages
            .get(row)
            .map(|message| thread_parent_text(&self.messages, message))
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use rc_core::domain::{Attachment, Author, Delivery, ImageAttachment, MessageKind};

    use super::*;

    fn message(id: &str, ts: &str, user: &str) -> Message {
        Message {
            id: id.to_owned().into(),
            room_id: "r".to_owned().into(),
            author: Author {
                id: user.to_owned().into(),
                username: user.to_owned(),
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
        }
    }

    fn reset(messages: Vec<Message>) -> TimelineChange {
        TimelineChange::Reset {
            room: None,
            messages,
        }
    }

    fn ids(list: &MessageList) -> Vec<String> {
        (0..list.row_count())
            .filter_map(|row| list.message(row))
            .map(|message| message.id.as_str().to_owned())
            .collect()
    }

    fn list_in_utc(messages: Vec<Message>) -> MessageList {
        let mut list = MessageList {
            zone: TimeZone::UTC,
            ..MessageList::default()
        };
        list.apply(&reset(messages));
        list
    }

    // MSG-001
    #[test]
    fn a_reset_and_an_insert_keep_newest_first() {
        let mut list = MessageList::default();
        list.apply(&reset(vec![
            message("a", "2026-10-20T10:00:00Z", "alice"),
            message("b", "2026-10-20T11:00:00Z", "alice"),
        ]));
        assert_eq!(ids(&list), ["b", "a"]);

        let change = TimelineChange::Appended(vec![message("c", "2026-10-20T12:00:00Z", "bob")]);
        assert_eq!(list.ops(&change), [MessageOp::Insert { first: 0, last: 0 }]);
        list.apply(&change);
        assert_eq!(ids(&list), ["c", "b", "a"]);
        assert_eq!(list.row_count(), 3);
    }

    // MSG-004
    #[test]
    fn the_first_of_a_day_is_the_oldest_of_it_in_the_zone() {
        let list = list_in_utc(vec![
            message("a", "2026-10-20T23:00:00Z", "alice"),
            message("b", "2026-10-20T23:30:00Z", "alice"),
            message("c", "2026-10-21T00:30:00Z", "alice"),
        ]);
        // Rows: c, b, a.
        assert!(!list.first_of_day(1));
        assert!(list.first_of_day(0));
        assert!(list.first_of_day(2));
        assert!(!list.first_of_day(3));
    }

    // RICH-004
    #[test]
    fn new_avatars_touch_the_rows_that_show_them() {
        let mut list = list_in_utc(vec![
            message("a", "2026-10-20T10:00:00Z", "alice"),
            message("b", "2026-10-20T11:00:00Z", "bob"),
            message("c", "2026-10-20T12:00:00Z", "alice"),
            message("d", "2026-10-20T13:00:00Z", "alice"),
        ]);
        // Rows: d, c, b, a.
        let runs = list.set_avatars(&[(AvatarOwner::User("alice".to_owned()), "u".to_owned())]);
        assert_eq!(runs, [(0, 1), (3, 3)]);
        assert_eq!(
            list.avatars().url(&AvatarOwner::User("alice".to_owned())),
            "u"
        );
    }

    // RICH-005
    #[test]
    fn new_images_touch_the_rows_that_show_them() {
        let mut with_image = message("a", "2026-10-20T10:00:00Z", "alice");
        with_image.attachments.push(Attachment {
            title: "t".to_owned(),
            link: "/file-upload/x".to_owned(),
            description: None,
            size: None,
            image: Some(ImageAttachment {
                path: "/file-upload/x".to_owned(),
                width: None,
                height: None,
            }),
        });
        let mut list = list_in_utc(vec![
            with_image,
            message("b", "2026-10-20T11:00:00Z", "alice"),
        ]);
        let runs = list.set_images(&[("/file-upload/x".to_owned(), "file:///x".to_owned())]);
        assert_eq!(runs, [(1, 1)]);
        assert_eq!(list.images().url("/file-upload/x"), "file:///x");
    }

    // MSG-001
    #[test]
    fn a_new_viewer_touches_every_row_and_the_same_one_none() {
        let mut list = list_in_utc(vec![
            message("a", "2026-10-20T10:00:00Z", "alice"),
            message("b", "2026-10-20T11:00:00Z", "alice"),
        ]);
        assert_eq!(list.set_viewer(Viewer::default()), None);
        let viewer = Viewer {
            username: Some("bob".to_owned()),
            ..Viewer::default()
        };
        assert_eq!(list.set_viewer(viewer), Some((0, 1)));
    }
}
