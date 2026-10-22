// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! What the `MessageList` model shows for the reactions of a message: for each emoji, how to draw
//! it, how many users reacted, whether the viewer did, and who. Plain Rust, so it can be
//! unit-tested without Qt.

use rc_core::domain::{Message, Reaction};
use rc_core::emoji::Shown;
use rc_core::rich::Emoji;

use crate::viewer::{Viewer, is_web_url};

/// One reaction as the GUI gets it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReactionView {
    /// The shortcode as the server sends it, colons included. The GUI shows it when it has neither a
    /// glyph nor an image.
    pub emoji: String,
    /// The emoji as Unicode text; empty for a custom emoji and for a shortcode nobody knows.
    pub glyph: String,
    /// The URL of the image of a custom emoji; empty otherwise.
    pub image: String,
    pub count: i32,
    /// The viewer is among those who reacted.
    pub mine: bool,
    /// Who reacted, for a tooltip.
    pub users: String,
}

/// The reactions of `message`, in the server's order.
#[must_use]
pub fn reactions(message: &Message, viewer: &Viewer) -> Vec<ReactionView> {
    message
        .reactions
        .iter()
        .map(|reaction| view(reaction, viewer))
        .collect()
}

fn view(reaction: &Reaction, viewer: &Viewer) -> ReactionView {
    let name = reaction.emoji.trim_matches(':');
    let emoji = Emoji {
        short_code: Some(name.to_owned()),
        unicode: None,
    };
    let (glyph, image) = match emoji.shown(&viewer.emoji) {
        Shown::Unicode(text) => (text.to_owned(), String::new()),
        Shown::Image { url, .. } if is_web_url(url) => (String::new(), url.to_owned()),
        Shown::Image { .. } | Shown::Unknown(_) => (String::new(), String::new()),
    };
    ReactionView {
        emoji: reaction.emoji.clone(),
        glyph,
        image,
        count: i32::try_from(reaction.usernames.len()).unwrap_or(i32::MAX),
        mine: viewer
            .username
            .as_ref()
            .is_some_and(|me| reaction.usernames.contains(me)),
        users: reaction.usernames.join(", "),
    }
}

#[cfg(test)]
mod tests {
    use rc_core::domain::{Author, Delivery, MessageKind};
    use rc_core::emoji::CustomEmojis;

    use super::*;

    fn message(reactions: Vec<Reaction>) -> Message {
        Message {
            id: "m".to_owned().into(),
            room_id: "r".to_owned().into(),
            author: Author {
                id: "u".to_owned().into(),
                username: "bob".into(),
                display_name: None,
            },
            ts: jiff::Timestamp::UNIX_EPOCH,
            kind: MessageKind::Text("hi".into()),
            delivery: Delivery::Sent,
            attachments: Vec::new(),
            quotes: Vec::new(),
            edited: false,
            reactions,
            thread_id: None,
            thread_shown: false,
            thread_replies: 0,
            thread_last_reply: None,
        }
    }

    fn reaction(emoji: &str, usernames: &[&str]) -> Reaction {
        Reaction {
            emoji: emoji.to_owned(),
            usernames: usernames.iter().map(|&name| name.to_owned()).collect(),
        }
    }

    fn viewer(username: Option<&str>) -> Viewer {
        Viewer {
            username: username.map(str::to_owned),
            server: None,
            emoji: CustomEmojis::new(
                "https://chat.example.com",
                &[rc_api::CustomEmoji {
                    name: "party".into(),
                    aliases: Vec::new(),
                    extension: "gif".into(),
                }],
            ),
            ..Viewer::default()
        }
    }

    // RICH-007
    #[test]
    fn a_standard_emoji_has_a_glyph_a_count_and_the_users() {
        let message = message(vec![reaction(":thumbsup:", &["alice", "bob"])]);
        assert_eq!(
            reactions(&message, &viewer(Some("carol"))),
            [ReactionView {
                emoji: ":thumbsup:".into(),
                glyph: "👍".into(),
                image: String::new(),
                count: 2,
                mine: false,
                users: "alice, bob".into(),
            }]
        );
    }

    // RICH-007
    #[test]
    fn a_custom_emoji_has_the_url_of_its_image() {
        let message = message(vec![reaction(":party:", &["bob"])]);
        let shown = reactions(&message, &viewer(None));
        assert_eq!(shown[0].glyph, "");
        assert_eq!(
            shown[0].image,
            "https://chat.example.com/emoji-custom/party.gif"
        );
    }

    // RICH-007
    #[test]
    fn a_shortcode_nobody_knows_has_neither_glyph_nor_image() {
        let message = message(vec![reaction(":no-such-emoji:", &["bob"])]);
        let shown = reactions(&message, &viewer(None));
        assert_eq!((shown[0].glyph.as_str(), shown[0].image.as_str()), ("", ""));
        assert_eq!(shown[0].emoji, ":no-such-emoji:");
    }

    // RICH-007
    #[test]
    fn a_custom_emoji_that_is_not_on_the_web_is_not_drawn() {
        let mut viewer = viewer(None);
        viewer.emoji = CustomEmojis::new(
            "file:///tmp",
            &[rc_api::CustomEmoji {
                name: "party".into(),
                aliases: Vec::new(),
                extension: "gif".into(),
            }],
        );
        let shown = reactions(&message(vec![reaction(":party:", &["bob"])]), &viewer);
        assert_eq!((shown[0].glyph.as_str(), shown[0].image.as_str()), ("", ""));
    }

    // RICH-007
    #[test]
    fn the_viewers_own_reaction_is_marked() {
        let message = message(vec![
            reaction(":thumbsup:", &["alice", "bob"]),
            reaction(":tada:", &["alice"]),
        ]);
        let shown = reactions(&message, &viewer(Some("bob")));
        assert_eq!(
            shown.iter().map(|r| r.mine).collect::<Vec<_>>(),
            [true, false]
        );
        let nobody = reactions(&message, &viewer(None));
        assert!(nobody.iter().all(|r| !r.mine));
    }

    // RICH-007
    #[test]
    fn the_reactions_keep_their_order_and_a_message_without_any_shows_none() {
        let message = message(vec![
            reaction(":tada:", &["a"]),
            reaction(":thumbsup:", &["a"]),
        ]);
        let order: Vec<_> = reactions(&message, &viewer(None))
            .into_iter()
            .map(|r| r.emoji)
            .collect();
        assert_eq!(order, [":tada:", ":thumbsup:"]);
        assert_eq!(reactions(&self::message(Vec::new()), &viewer(None)), []);
    }
}
