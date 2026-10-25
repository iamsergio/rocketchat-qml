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
use rc_view::emoji_image_view::EmojiImageUrls;
use rc_view::message_list::MessageList;
use rc_view::reaction_view::custom_emoji_names;
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
    /// The rows are those of the thread pane and not of the room (see [`message_row`]).
    in_pane: bool,
    /// How many times messages were inserted at the oldest end of a list that had some.
    prepends: Cell<i32>,
    /// How many times the whole list was replaced.
    resets: Cell<i32>,
    notify: ModelNotify,
}

impl MessageModel {
    /// An empty model, for the room's messages or, with `in_pane`, the open thread's.
    pub fn new(palette: Palette, in_pane: bool) -> Self {
        Self {
            list: RefCell::new(MessageList::default()),
            palette,
            in_pane,
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
    /// Returns the custom emoji that the reactions use, for the core to download.
    pub fn show(
        &self,
        snapshot: &TimelineChange,
        viewer: Viewer,
        avatars: AvatarUrls,
        images: ImageUrls,
        emoji_images: EmojiImageUrls,
    ) -> Vec<String> {
        {
            let mut list = self.list.borrow_mut();
            list.replace(snapshot, viewer, avatars, images);
            list.use_emoji_images(emoji_images);
        }
        self.reset();
        self.custom_emoji_names()
    }

    fn reset(&self) {
        self.resets.set(self.resets.get().wrapping_add(1));
        self.notify.reset();
    }

    /// Applies the changes in order, and tells the view what changed. Returns the custom emoji
    /// that the reactions of the messages in the changes use, for the core to download; it costs
    /// no more than the changes themselves, however long the list is.
    pub fn apply(&self, changes: &[TimelineChange]) -> Vec<String> {
        for change in changes {
            self.apply_one(change);
        }
        let list = self.list.borrow();
        custom_emoji_names(
            changes.iter().flat_map(TimelineChange::messages),
            list.viewer(),
        )
    }

    /// The custom emoji that the reactions of all the messages use: it looks at every message, so
    /// it is for when the list of emoji changes and not for each update.
    pub fn custom_emoji_names(&self) -> Vec<String> {
        self.list.borrow().custom_emoji_names()
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

    /// Takes new custom emoji image files in, and tells the view which rows react with them.
    pub fn set_emoji_images(&self, changes: &[(String, String)]) {
        let runs = self.list.borrow_mut().set_emoji_images(changes);
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
        message_row(&list, row, &self.palette, self.in_pane)
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        &self.notify
    }
}

#[cfg(test)]
mod tests {
    use rc_core::domain::{Author, Delivery, Message, MessageKind, Quote, Reaction};
    use rc_core::emoji::CustomEmojis;
    use rc_core::rich::RichText;
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
        let model = MessageModel::new(palette(), false);
        model.show(
            &reset(messages),
            Viewer::default(),
            AvatarUrls::default(),
            ImageUrls::default(),
            EmojiImageUrls::default(),
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
            EmojiImageUrls::default(),
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

    // ACT-001, ACT-002, ACT-005
    #[test]
    fn a_message_that_can_be_quoted_has_a_menu_whoever_wrote_it() {
        let model = shown(days(&["a"]));
        assert!(model.row_data(0).unwrap().has_menu);
        model.set_viewer(Viewer {
            user_id: Some("u".to_owned().into()),
            ..Viewer::default()
        });
        assert!(model.row_data(0).unwrap().has_menu);
        model.set_viewer(Viewer {
            user_id: Some("someone else".to_owned().into()),
            ..Viewer::default()
        });
        assert!(model.row_data(0).unwrap().has_menu);

        // An unsent message can't be quoted, and it isn't the viewer's to change either.
        let mut unsent = message("b", "2026-10-02T10:00:00Z");
        unsent.delivery = Delivery::Pending;
        model.apply(&[TimelineChange::Appended(vec![unsent])]);
        assert!(!model.row_data(1).unwrap().has_menu);
    }

    // ACT-003
    #[test]
    fn only_a_body_with_a_link_is_marked_as_having_links() {
        let mut linked = message("a", "2026-10-01T09:00:00Z");
        linked.kind = MessageKind::Text(RichText::from_topic("see https://example.com now"));
        let model = shown(vec![linked, message("b", "2026-10-02T09:00:00Z")]);

        assert!(model.row_data(0).unwrap().has_links);
        assert!(!model.row_data(1).unwrap().has_links);
    }

    // ACT-005
    #[test]
    fn the_quotes_of_a_message_are_rows_of_their_own_and_most_messages_have_none() {
        let mut reply = message("a", "2026-10-01T09:00:00Z");
        reply.quotes = vec![Quote {
            message: Some("q".to_owned().into()),
            link: "http://s/channel/c?msg=q".to_owned(),
            author: "bob".to_owned(),
            text: "say it".into(),
            ts: Some("2026-10-01T08:00:00Z".parse().unwrap()),
        }];
        let model = shown(vec![reply, message("b", "2026-10-02T10:00:00Z")]);

        let quotes = model.row_data(0).unwrap().quotes;
        assert_eq!(quotes.row_count(), 1);
        let quote = quotes.row_data(0).unwrap();
        assert_eq!(quote.author, "bob");
        assert!(!quote.time.is_empty());
        assert_eq!(quote.body, "say it");
        assert!(quote.body_html.contains("say it"));
        assert_eq!(model.row_data(1).unwrap().quotes.row_count(), 0);
    }

    fn reaction(emoji: &str, users: &[&str]) -> Reaction {
        Reaction {
            emoji: emoji.to_owned(),
            usernames: users.iter().map(|&user| user.to_owned()).collect(),
        }
    }

    /// A viewer called "me" who knows the custom emoji "party" and "wave".
    fn viewer_with_emoji() -> Viewer {
        Viewer {
            username: Some("me".to_owned()),
            emoji: CustomEmojis::new(
                "https://chat.example.com",
                &["party", "wave"].map(|name| rc_api::CustomEmoji {
                    name: name.into(),
                    aliases: Vec::new(),
                    extension: "png".into(),
                }),
            ),
            ..Viewer::default()
        }
    }

    fn shown_to(messages: Vec<Message>, viewer: Viewer) -> (MessageModel, Vec<String>) {
        let model = MessageModel::new(palette(), false);
        let names = model.show(
            &reset(messages),
            viewer,
            AvatarUrls::default(),
            ImageUrls::default(),
            EmojiImageUrls::default(),
        );
        (model, names)
    }

    fn emoji_update(name: &str, url: &str) -> (String, String) {
        (name.to_owned(), url.to_owned())
    }

    // RICH-007, ACT-003
    #[test]
    fn the_reactions_of_a_message_are_rows_of_their_own_and_most_messages_have_none() {
        let mut reacted = message("a", "2026-10-01T09:00:00Z");
        reacted.reactions = vec![
            reaction(":thumbsup:", &["bob", "me"]),
            reaction(":party:", &["bob"]),
            reaction(":nobody_knows:", &["bob"]),
        ];
        let (model, _) = shown_to(
            vec![reacted, message("b", "2026-10-02T10:00:00Z")],
            viewer_with_emoji(),
        );

        let row = model.row_data(0).unwrap();
        assert!(row.can_react);
        let chips: Vec<_> = row.reactions.iter().collect();
        assert_eq!(chips.len(), 3);
        assert_eq!(
            (
                chips[0].label.as_str(),
                chips[0].text.as_str(),
                chips[0].count
            ),
            ("👍", "👍", 2)
        );
        assert!(chips[0].mine && !chips[1].mine);
        assert_eq!(chips[0].emoji, ":thumbsup:");
        // A custom emoji reads as the address of its image, and is drawn as its shortcode until
        // there is a file.
        assert_eq!(
            chips[1].label,
            "https://chat.example.com/emoji-custom/party.png"
        );
        assert_eq!(chips[1].text, ":party:");
        assert!(!chips[1].has_image);
        assert_eq!(chips[2].label, ":nobody_knows:");
        assert_eq!(model.row_data(1).unwrap().reactions.row_count(), 0);
    }

    // RICH-007
    #[test]
    fn a_reacted_message_follows_the_live_changes_of_its_reactions() {
        let (model, _) = shown_to(days(&["a", "b"]), viewer_with_emoji());
        assert_eq!(model.row_data(0).unwrap().reactions.row_count(), 0);

        let mut reacted = message("a", "2026-10-01T10:00:00Z");
        reacted.reactions = vec![reaction(":thumbsup:", &["me"])];
        model.apply(&[TimelineChange::Updated {
            index: 0,
            message: reacted.clone(),
        }]);
        assert_eq!(model.row_data(0).unwrap().reactions.row_count(), 1);

        reacted.reactions.clear();
        model.apply(&[TimelineChange::Updated {
            index: 0,
            message: reacted,
        }]);
        assert_eq!(model.row_data(0).unwrap().reactions.row_count(), 0);
    }

    // RICH-007
    #[test]
    fn only_the_custom_emoji_of_the_messages_that_come_are_asked_for() {
        let mut first = message("a", "2026-10-01T09:00:00Z");
        first.reactions = vec![
            reaction(":party:", &["bob"]),
            reaction(":thumbsup:", &["bob"]),
        ];
        let mut second = message("b", "2026-10-02T09:00:00Z");
        second.reactions = vec![reaction(":wave:", &["bob"]), reaction(":party:", &["me"])];
        let (model, names) = shown_to(vec![first], viewer_with_emoji());
        assert_eq!(names, ["party"]);

        // An update of a message without any, or a new one, asks for what it brings, not for what
        // the rest of the list has.
        let plain = TimelineChange::Updated {
            index: 0,
            message: message("a", "2026-10-01T09:00:00Z"),
        };
        assert!(model.apply(&[plain]).is_empty());
        let names = model.apply(&[TimelineChange::Appended(vec![second])]);
        assert_eq!(names, ["wave", "party"]);
        assert_eq!(model.custom_emoji_names(), ["wave", "party"]);
    }

    // RICH-007
    #[test]
    fn a_custom_emoji_without_a_known_url_is_not_asked_for_until_the_list_arrives() {
        let mut reacted = message("a", "2026-10-01T09:00:00Z");
        reacted.reactions = vec![reaction(":party:", &["bob"])];
        let (model, names) = shown_to(vec![reacted], Viewer::default());
        assert!(names.is_empty());

        model.set_viewer(viewer_with_emoji());
        assert_eq!(model.custom_emoji_names(), ["party"]);
    }

    // RICH-007
    #[test]
    fn a_new_emoji_file_changes_the_rows_that_react_with_it_and_no_others() {
        let dir = std::env::temp_dir().join(format!("rc_slint_emoji_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("party.svg");
        std::fs::write(
            &file,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><rect width="4" height="4"/></svg>"#,
        )
        .unwrap();
        let mut party = message("a", "2026-10-01T09:00:00Z");
        party.reactions = vec![reaction(":party:", &["bob"])];
        let mut wave = message("b", "2026-10-02T09:00:00Z");
        wave.reactions = vec![reaction(":wave:", &["bob"])];
        let (model, _) = shown_to(
            vec![party, wave, message("c", "2026-10-03T09:00:00Z")],
            viewer_with_emoji(),
        );
        let url = format!("file://{}?v=1", file.display());
        model.set_emoji_images(&[emoji_update("party", &url)]);

        let chip = |row: usize| model.row_data(row).unwrap().reactions.row_data(0).unwrap();
        assert!(chip(0).has_image);
        assert_eq!(chip(0).text, ":party:");
        assert!(!chip(1).has_image);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A model of the same messages, for the room or for the pane.
    fn shown_in(messages: Vec<Message>, in_pane: bool) -> MessageModel {
        let model = MessageModel::new(palette(), in_pane);
        model.show(
            &reset(messages),
            Viewer::default(),
            AvatarUrls::default(),
            ImageUrls::default(),
            EmojiImageUrls::default(),
        );
        model
    }

    /// A parent "p" with two replies, "r1" only in the thread and "r2" also sent to the room.
    fn thread() -> Vec<Message> {
        let mut parent = message("p", "2026-10-01T10:00:00Z");
        parent.thread_replies = 2;
        parent.thread_last_reply = Some("2026-10-01T10:05:00Z".parse().unwrap());
        let mut first = message("r1", "2026-10-01T10:01:00Z");
        first.thread_id = Some("p".to_owned().into());
        let mut second = message("r2", "2026-10-01T10:05:00Z");
        second.thread_id = Some("p".to_owned().into());
        second.thread_shown = true;
        vec![parent, first, second]
    }

    // ACT-004
    #[test]
    fn a_parent_has_its_replies_and_a_reply_leads_to_its_parent_in_the_room() {
        let model = shown_in(thread(), false);
        let rows: Vec<_> = model.iter().collect();

        assert_eq!(rows[0].thread_replies, 2);
        assert!(!rows[0].thread_last_reply.is_empty());
        assert!(rows[0].can_start_thread);
        assert!(rows[0].has_menu);
        assert_eq!(rows[1].thread_id, "p");
        assert!(!rows[1].in_thread_shown);
        assert!(rows[2].in_thread_shown);
        assert_eq!(rows[2].thread_parent_text, "p");
        // A reply starts no thread of its own.
        assert!(!rows[2].can_start_thread);
        assert_eq!(rows[2].thread_replies, 0);
        assert!(rows[2].thread_last_reply.is_empty());
    }

    // ACT-004
    #[test]
    fn in_the_pane_nothing_starts_a_thread_and_no_reply_leads_anywhere() {
        let model = shown_in(thread(), true);
        let rows: Vec<_> = model.iter().collect();

        // The count stays: the divider under the parent says it.
        assert_eq!(rows[0].thread_replies, 2);
        assert!(rows.iter().all(|row| !row.can_start_thread));
        assert!(rows.iter().all(|row| !row.in_thread_shown));
        // Quoting and reacting still give the message a menu.
        assert!(rows.iter().all(|row| row.has_menu));
    }

    // ACT-004
    #[test]
    fn an_unsent_message_starts_no_thread() {
        let mut unsent = message("u", "2026-10-01T10:00:00Z");
        unsent.delivery = Delivery::Pending;
        let model = shown_in(vec![unsent], false);

        assert!(!model.row_data(0).unwrap().can_start_thread);
    }
}
