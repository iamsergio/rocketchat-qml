// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! What a message list holds, whatever toolkit shows it: the core's messages, held newest first
//! (row 0 is the newest), the zone they are read in, who they are shown to, and the avatar and
//! image URLs. It applies the core's changes and says which rows each update touches; the
//! notifications the toolkit wants around them are its own.

use std::collections::HashSet;

use jiff::tz::TimeZone;
use rc_core::domain::{AvatarOwner, Message, MessageId};
use rc_core::timeline::TimelineChange;

use crate::attachment_view::{ImageUrls, paths, shows_image};
use crate::avatar_view::{AvatarUrls, message_owner, owners, rows_using, runs};
use crate::emoji_image_view::EmojiImageUrls;
use crate::message_view::{first_of_day, thread_parent_text};
use crate::reaction_view::{custom_emoji_names, shows_emoji};
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
    emoji_images: EmojiImageUrls,
    /// The message the room was asked to show (a search result), which is marked: the core's
    /// `TimelineStatus.found`. Whether it is among the held messages is for the rows to tell.
    found: Option<MessageId>,
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
            emoji_images: EmojiImageUrls::default(),
            found: None,
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

    /// Marks the message `found` (none for no mark). Returns the runs of rows (first and last) that
    /// change: the one that was marked and the one that is now, if the list holds them.
    pub fn set_found(&mut self, found: Option<MessageId>) -> Vec<(usize, usize)> {
        if self.found == found {
            return Vec::new();
        }
        let before = std::mem::replace(&mut self.found, found);
        let rows = [before, self.found.clone()]
            .iter()
            .flatten()
            .filter_map(|id| self.row_of(id.as_str()))
            .collect::<Vec<_>>();
        runs(self.messages.len(), |at| rows.contains(&at))
    }

    /// Whether the message at `row` is the marked one.
    #[must_use]
    pub fn is_found(&self, row: usize) -> bool {
        match (&self.found, self.messages.get(row)) {
            (Some(found), Some(message)) => *found == message.id,
            _ => false,
        }
    }

    /// The row of the message with this id, if the list holds it.
    #[must_use]
    pub fn row_of(&self, id: &str) -> Option<usize> {
        self.messages
            .iter()
            .position(|message| message.id.as_str() == id)
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

    /// Replaces the custom emoji image files the reactions show, with the ones known now. The
    /// caller announces a model reset around it, like for [`replace`](Self::replace).
    pub fn use_emoji_images(&mut self, emoji_images: EmojiImageUrls) {
        self.emoji_images = emoji_images;
    }

    /// Takes new custom emoji image files in (by name, without colons). Returns the runs of rows
    /// (first and last) whose message reacts with one of them.
    pub fn set_emoji_images(&mut self, changes: &[(String, String)]) -> Vec<(usize, usize)> {
        self.emoji_images.set(changes);
        let names: HashSet<String> = changes.iter().map(|(name, _)| name.clone()).collect();
        runs(self.messages.len(), |at| {
            self.messages
                .get(at)
                .is_some_and(|message| shows_emoji(message, &self.viewer, &names))
        })
    }

    /// The names of the custom emoji that the reactions of the held messages use.
    #[must_use]
    pub fn custom_emoji_names(&self) -> Vec<String> {
        custom_emoji_names(&self.messages, &self.viewer)
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

    #[must_use]
    pub fn emoji_images(&self) -> &EmojiImageUrls {
        &self.emoji_images
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
            thread_followers: Vec::new(),
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

    // RICH-007
    #[test]
    fn new_emoji_files_touch_the_rows_that_react_with_them() {
        let mut party = message("a", "2026-10-20T10:00:00Z", "alice");
        party.reactions = vec![rc_core::domain::Reaction {
            emoji: ":party:".to_owned(),
            usernames: vec!["bob".to_owned()],
        }];
        let mut list = MessageList::default();
        list.apply(&reset(vec![
            party.clone(),
            message("b", "2026-10-20T11:00:00Z", "alice"),
            party,
        ]));
        let viewer = Viewer {
            emoji: rc_core::emoji::CustomEmojis::new(
                "https://chat.example.com",
                &[rc_api::CustomEmoji {
                    name: "party".into(),
                    aliases: Vec::new(),
                    extension: "gif".into(),
                }],
            ),
            ..Viewer::default()
        };
        list.set_viewer(viewer);

        // Rows: a, b, a.
        let runs = list.set_emoji_images(&[("party".to_owned(), "file:///p".to_owned())]);
        assert_eq!(runs, [(0, 0), (2, 2)]);
        assert_eq!(list.emoji_images().url("party"), "file:///p");
        assert!(
            list.set_emoji_images(&[("other".to_owned(), "file:///o".to_owned())])
                .is_empty()
        );
    }

    // MSG-008
    #[test]
    fn marking_a_message_touches_its_row_and_the_one_that_was_marked() {
        let mut list = list_in_utc(vec![
            message("a", "2026-10-20T10:00:00Z", "alice"),
            message("b", "2026-10-20T11:00:00Z", "alice"),
            message("c", "2026-10-20T12:00:00Z", "alice"),
        ]);
        // Rows: c, b, a.
        assert_eq!(list.set_found(Some("b".to_owned().into())), [(1, 1)]);
        assert!(list.is_found(1) && !list.is_found(0) && !list.is_found(2));
        assert_eq!(list.row_of("a"), Some(2));

        // Another one: both rows change, as one run when they are next to each other and else as
        // two.
        assert_eq!(list.set_found(Some("a".to_owned().into())), [(1, 2)]);
        assert_eq!(
            list.set_found(Some("c".to_owned().into())),
            [(0, 0), (2, 2)]
        );
        // The same one again changes nothing; none clears the mark.
        assert!(list.set_found(Some("c".to_owned().into())).is_empty());
        assert_eq!(list.set_found(None), [(0, 0)]);
        assert!(!list.is_found(0));
    }

    // MSG-008
    #[test]
    fn a_mark_on_a_message_the_list_does_not_hold_touches_nothing_until_it_arrives() {
        let mut list = list_in_utc(vec![message("a", "2026-10-20T10:00:00Z", "alice")]);
        assert!(list.set_found(Some("old".to_owned().into())).is_empty());
        assert!(!list.is_found(0));

        // The window brings it in with a reset: the mark is on it at once.
        list.apply(&reset(vec![
            message("old", "2026-10-01T10:00:00Z", "alice"),
            message("a", "2026-10-20T10:00:00Z", "alice"),
        ]));
        assert!(list.is_found(1));
        assert!(!list.is_found(7));
    }
}
