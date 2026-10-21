// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! What the application knows about, independent of how the server spells it.

use std::fmt;

use secrecy::SecretString;

use crate::quote::quoted_message;
use crate::random;
use crate::rich::RichText;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct UserId(String);

impl UserId {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for UserId {
    fn from(id: String) -> Self {
        Self(id)
    }
}

impl fmt::Display for UserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RoomId(String);

impl RoomId {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for RoomId {
    fn from(id: String) -> Self {
        Self(id)
    }
}

impl fmt::Display for RoomId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MessageId(String);

impl MessageId {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// An id for a message the user is about to send, shaped like the ones the server makes.
    #[must_use]
    pub(crate) fn generate() -> Self {
        const ALPHABET: &[u8] = b"23456789ABCDEFGHJKLMNPQRSTWXYZabcdefghijkmnopqrstuvwxyz";
        let id = (0..17)
            .map(|_| {
                let at = usize::try_from(random::bits() % ALPHABET.len() as u64).unwrap_or(0);
                char::from(ALPHABET[at])
            })
            .collect();
        Self(id)
    }
}

impl From<String> for MessageId {
    fn from(id: String) -> Self {
        Self(id)
    }
}

impl fmt::Display for MessageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct User {
    pub id: UserId,
    pub username: String,
    pub display_name: Option<String>,
}

/// Enough to make authenticated calls and to resume a session later. Debug output doesn't reveal
/// the token.
#[derive(Clone, Debug)]
pub struct Credentials {
    pub user_id: UserId,
    pub token: SecretString,
}

/// Who is logged in, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {
    /// The normalized URL that was used to log in.
    pub server: String,
    pub user: User,
}

/// What is kept between runs to resume a session.
#[derive(Clone, Debug)]
pub struct StoredSession {
    /// The normalized URL that was used to log in.
    pub server: String,
    pub credentials: Credentials,
}

/// Whose avatar it is.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum AvatarOwner {
    /// By username.
    User(String),
    /// A channel or group.
    Room(RoomId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomKind {
    Channel,
    /// A private group.
    Private,
    Direct,
}

/// A room the user has joined, as the room list shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Room {
    pub id: RoomId,
    pub kind: RoomKind,
    /// What the room list shows.
    pub name: String,
    /// The other user's username, for a direct message with one other user.
    pub peer: Option<String>,
    /// Identifies the room's own avatar, if it has one; the server draws the others. A change
    /// that only a push brings isn't seen here, as pushes don't carry it.
    pub avatar_etag: Option<String>,
    pub unread: u32,
    /// Mentions of the user (`@name`).
    pub user_mentions: u32,
    /// `@all` and `@here`.
    pub group_mentions: u32,
    /// Something new that the counts don't cover (e.g. thread replies): show the room as unread.
    pub alert: bool,
    /// The user marked the room as a favorite: the room list shows favorites first, in a section
    /// of their own.
    pub favorite: bool,
    /// When the room last had a message; when it joined it, for a room without messages. The room
    /// list shows the most recent first.
    pub last_activity: jiff::Timestamp,
}

impl Room {
    /// The avatar the room shows: the other user's for a direct message, else the room's own. The
    /// server draws a direct message's own avatar from its id, which says nothing about anyone.
    #[must_use]
    pub fn avatar_owner(&self) -> AvatarOwner {
        match &self.peer {
            Some(username) => AvatarOwner::User(username.clone()),
            None => AvatarOwner::Room(self.id.clone()),
        }
    }

    /// The avatar worth fetching for the room, if any: the other user's for a direct message (the
    /// server says whether they have one when asked), the room's own if it has one. A room
    /// without gets the initials of its name instead.
    #[must_use]
    pub fn avatar_to_fetch(&self) -> Option<AvatarOwner> {
        (self.peer.is_some() || self.avatar_etag.is_some()).then(|| self.avatar_owner())
    }

    /// Whether the room shows as unread.
    #[must_use]
    pub fn has_unread(&self) -> bool {
        self.unread > 0 || self.user_mentions > 0 || self.group_mentions > 0 || self.alert
    }

    /// Whether the user may leave the room: channels and private groups, not direct messages.
    #[must_use]
    pub fn can_leave(&self) -> bool {
        self.kind != RoomKind::Direct
    }

    /// The room with nothing unread.
    #[must_use]
    pub fn read(self) -> Self {
        Self {
            unread: 0,
            user_mentions: 0,
            group_mentions: 0,
            alert: false,
            ..self
        }
    }
}

/// The room a subscription stands for, or `None` if the room list doesn't show it: rooms the user
/// hid (Rocket.Chat's own sidebar hides them too), and kinds an end-user client has no use for
/// (livechat, voip...).
#[must_use]
pub(crate) fn room_from(sub: rc_api::Subscription) -> Option<Room> {
    let kind = match sub.room_type {
        rc_api::RoomType::Channel => RoomKind::Channel,
        rc_api::RoomType::Private => RoomKind::Private,
        rc_api::RoomType::Direct => RoomKind::Direct,
        rc_api::RoomType::Other(code) => {
            tracing::debug!(room = %sub.room_id, code, "room kind not listed");
            return None;
        }
    };
    if !sub.open {
        return None;
    }
    // The display name is empty for rooms such as `general`; a DM's `name` is the other user's
    // username while its display name is their real name. A group DM's is all of them, joined.
    let peer = Some(&sub.name)
        .filter(|name| kind == RoomKind::Direct && !name.is_empty() && !name.contains(','))
        .cloned();
    let name = sub
        .display_name
        .filter(|name| !name.is_empty())
        .or_else(|| Some(sub.name).filter(|name| !name.is_empty()))
        .unwrap_or_else(|| sub.room_id.clone());
    Some(Room {
        id: sub.room_id.into(),
        kind,
        name,
        peer,
        avatar_etag: sub.avatar_etag,
        unread: sub.unread,
        user_mentions: sub.user_mentions,
        group_mentions: sub.group_mentions,
        alert: sub.alert,
        favorite: sub.favorite,
        last_activity: sub
            .last_message
            .or(sub.created)
            .unwrap_or(jiff::Timestamp::UNIX_EPOCH),
    })
}

/// Who wrote a message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Author {
    pub id: UserId,
    pub username: String,
    pub display_name: Option<String>,
}

/// What happened, for a message that reports an event instead of saying something. For
/// `Joined`, `Left` and `TopicChanged` the author is who joined, left or changed the topic; for
/// `Added` and `Removed` the author is who did it, and `username` is who it was done to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SystemEvent {
    Joined,
    Left,
    Added {
        username: String,
    },
    Removed {
        username: String,
    },
    TopicChanged {
        topic: String,
    },
    /// An event this client has no wording for yet, with the server's type code (`r` for a rename,
    /// `room_changed_description`...).
    Other(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MessageKind {
    /// What the author wrote, as text and as formatted blocks.
    Text(RichText),
    System(SystemEvent),
}

/// Where a message stands on its way to the server. Everything the server gave us is `Sent`;
/// the others are messages the user wrote that it hasn't confirmed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Delivery {
    #[default]
    Sent,
    /// Being sent, or waiting for the connection.
    Pending,
    /// The server refused it or couldn't be reached. Sending it again is up to the user.
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub id: MessageId,
    pub room_id: RoomId,
    pub author: Author,
    /// The server's time; for an unsent message, when the user wrote it.
    pub ts: jiff::Timestamp,
    pub kind: MessageKind,
    pub delivery: Delivery,
    /// The files attached to it. An unsent message has none.
    pub attachments: Vec<Attachment>,
    /// The messages it quotes, in the order the author quoted them. Its text doesn't show the
    /// links that make them.
    pub quotes: Vec<Quote>,
    /// The author changed the text after sending it. An unsent message isn't.
    pub edited: bool,
    /// What users reacted with, in the order the server lists it. An unsent message has none.
    pub reactions: Vec<Reaction>,
}

/// A message that another one quotes, as a copy taken when the quote was made: it doesn't follow
/// edits of the quoted message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Quote {
    /// The quoted message, if the link says which.
    pub message: Option<MessageId>,
    /// The permalink of the quoted message.
    pub link: String,
    /// The username of the quoted message's author.
    pub author: String,
    /// What the quoted message said, without the links of the quotes it has itself.
    pub text: RichText,
    pub ts: Option<jiff::Timestamp>,
}

/// One emoji that users reacted to a message with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reaction {
    /// The shortcode with its colons (`:thumbsup:`). The server's custom emoji are named the same
    /// way.
    pub emoji: String,
    /// Who reacted, by username. Never empty.
    pub usernames: Vec<String>,
}

/// A file uploaded with a message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attachment {
    pub title: String,
    /// Where the original file is, a path on the server (`/file-upload/...`).
    pub link: String,
    pub description: Option<String>,
    /// In bytes, when the server says.
    pub size: Option<u64>,
    /// Set for images.
    pub image: Option<ImageAttachment>,
}

/// The picture an image attachment shows: the original, or a thumbnail the server made of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageAttachment {
    /// A path on the server (`/file-upload/...`), which is also what the file cache knows it by.
    pub path: String,
    /// In pixels, when the server says.
    pub width: Option<u32>,
    pub height: Option<u32>,
}

impl From<rc_api::Reaction> for Reaction {
    fn from(reaction: rc_api::Reaction) -> Self {
        Self {
            emoji: reaction.emoji,
            usernames: reaction.usernames,
        }
    }
}

impl From<rc_api::Quote> for Quote {
    fn from(quote: rc_api::Quote) -> Self {
        Self {
            message: quoted_message(&quote.link),
            text: RichText::from_markdown(quote.text, quote.md.as_deref())
                .without_quote_links(&quote.quoted_links),
            link: quote.link,
            author: quote.author,
            ts: quote.ts,
        }
    }
}

impl From<rc_api::Attachment> for Attachment {
    fn from(attachment: rc_api::Attachment) -> Self {
        Self {
            title: attachment.title,
            link: attachment.link,
            description: attachment.description,
            size: attachment.size,
            image: attachment.image.map(|image| ImageAttachment {
                path: image.path,
                width: image.dimensions.map(|(width, _)| width),
                height: image.dimensions.map(|(_, height)| height),
            }),
        }
    }
}

impl Message {
    /// Not confirmed by the server yet: `Pending` or `Failed`.
    #[must_use]
    pub fn is_unsent(&self) -> bool {
        self.delivery != Delivery::Sent
    }

    /// Whether the message calls `username`, or everybody, with an `@`.
    #[must_use]
    pub fn mentions(&self, username: &str) -> bool {
        match &self.kind {
            MessageKind::Text(text) => text.mentions(username),
            MessageKind::System(_) => false,
        }
    }

    /// Adds `username` to the users who reacted with `emoji`, or takes them out of it: a new
    /// reaction goes at the end, and one left without users is dropped. Returns whether anything
    /// changed. The users of a reaction are a set, so doing the same twice, or after the server's
    /// copy already says so, changes nothing.
    pub fn set_reaction(&mut self, emoji: &str, username: &str, react: bool) -> bool {
        let position = self.reactions.iter().position(|r| r.emoji == emoji);
        match (position, react) {
            (Some(at), true) => {
                let users = &mut self.reactions[at].usernames;
                let missing = !users.iter().any(|u| u == username);
                if missing {
                    users.push(username.to_owned());
                }
                missing
            }
            (None, true) => {
                self.reactions.push(Reaction {
                    emoji: emoji.to_owned(),
                    usernames: vec![username.to_owned()],
                });
                true
            }
            (Some(at), false) => {
                let users = &mut self.reactions[at].usernames;
                let before = users.len();
                users.retain(|u| u != username);
                let changed = users.len() != before;
                if users.is_empty() {
                    self.reactions.remove(at);
                }
                changed
            }
            (None, false) => false,
        }
    }
}

/// The message a history entry stands for, or `None` if the timeline doesn't show it: replies in a
/// thread that weren't also sent to the channel (Rocket.Chat's own view leaves them out too).
#[must_use]
pub(crate) fn message_from(message: rc_api::Message) -> Option<Message> {
    if message.thread_id.is_some() && !message.thread_shown {
        return None;
    }
    let quote_links: Vec<String> = message.quotes.iter().map(|q| q.link.clone()).collect();
    let kind = match message.kind.as_deref() {
        None => MessageKind::Text(
            RichText::from_markdown(message.text, message.md.as_deref())
                .without_quote_links(&quote_links),
        ),
        Some("uj") => MessageKind::System(SystemEvent::Joined),
        Some("ul") => MessageKind::System(SystemEvent::Left),
        Some("au") => MessageKind::System(SystemEvent::Added {
            username: message.text,
        }),
        Some("ru") => MessageKind::System(SystemEvent::Removed {
            username: message.text,
        }),
        Some("room_changed_topic") => MessageKind::System(SystemEvent::TopicChanged {
            topic: message.text,
        }),
        Some(code) => {
            tracing::debug!(message = %message.id, code, "system message kind without wording");
            MessageKind::System(SystemEvent::Other(code.to_owned()))
        }
    };
    Some(Message {
        id: message.id.into(),
        room_id: message.room_id.into(),
        author: Author {
            id: message.author.id.into(),
            username: message.author.username,
            display_name: message.author.name,
        },
        ts: message.ts,
        kind,
        delivery: Delivery::Sent,
        attachments: message.attachments.into_iter().map(Into::into).collect(),
        quotes: message.quotes.into_iter().map(Into::into).collect(),
        edited: message.edited,
        reactions: message.reactions.into_iter().map(Into::into).collect(),
    })
}

#[cfg(test)]
mod tests {
    use rc_api::{RoomType, Subscription};

    use super::*;

    fn sub(room_id: &str, room_type: RoomType) -> Subscription {
        Subscription {
            room_id: room_id.to_owned(),
            room_type,
            name: String::new(),
            display_name: None,
            unread: 0,
            user_mentions: 0,
            group_mentions: 0,
            alert: false,
            open: true,
            favorite: false,
            created: None,
            last_message: None,
            avatar_etag: None,
        }
    }

    // ROOM-005
    #[test]
    fn the_last_activity_is_the_last_message_else_the_join_time_else_the_epoch() {
        let at = |text: &str| text.parse::<jiff::Timestamp>().unwrap();
        let mut subscription = sub("r", RoomType::Channel);
        assert_eq!(
            room_from(subscription.clone()).unwrap().last_activity,
            jiff::Timestamp::UNIX_EPOCH
        );
        subscription.created = Some(at("2026-10-19T10:00:00Z"));
        assert_eq!(
            room_from(subscription.clone()).unwrap().last_activity,
            at("2026-10-19T10:00:00Z")
        );
        subscription.last_message = Some(at("2026-10-20T10:00:00Z"));
        assert_eq!(
            room_from(subscription).unwrap().last_activity,
            at("2026-10-20T10:00:00Z")
        );
    }

    // ROOM-004
    #[test]
    fn a_room_is_read_when_no_count_and_no_alert_is_left() {
        let mut room = room_from(sub("r", RoomType::Channel)).unwrap();
        assert!(!room.has_unread());
        for change in [
            |r: &mut Room| r.unread = 1,
            |r: &mut Room| r.user_mentions = 1,
            |r: &mut Room| r.group_mentions = 1,
            |r: &mut Room| r.alert = true,
        ] {
            let mut unread = room.clone();
            change(&mut unread);
            assert!(unread.has_unread());
            assert_eq!(unread.read(), room);
        }
        room.unread = 5;
        assert_eq!(room.read().unread, 0);
    }

    // ROOM-002
    #[test]
    fn maps_the_room_kinds() {
        for (room_type, kind) in [
            (RoomType::Channel, RoomKind::Channel),
            (RoomType::Private, RoomKind::Private),
            (RoomType::Direct, RoomKind::Direct),
        ] {
            let room = room_from(sub("r", room_type)).unwrap();
            assert_eq!(room.kind, kind);
            assert_eq!(room.id.as_str(), "r");
        }
    }

    // ROOM-002
    #[test]
    fn skips_kinds_the_list_does_not_show() {
        assert!(room_from(sub("r", RoomType::Other("l".into()))).is_none());
        assert!(room_from(sub("r", RoomType::Other("v".into()))).is_none());
    }

    // ROOM-001
    #[test]
    fn skips_hidden_rooms() {
        let mut hidden = sub("r", RoomType::Channel);
        hidden.open = false;
        assert!(room_from(hidden).is_none());
    }

    // ROOM-002
    #[test]
    fn picks_the_display_name_then_the_name_then_the_id() {
        let mut s = sub("rid", RoomType::Direct);
        s.name = "bob".into();
        s.display_name = Some("Bob Builder".into());
        assert_eq!(room_from(s.clone()).unwrap().name, "Bob Builder");

        // Empty counts as missing.
        s.display_name = Some(String::new());
        assert_eq!(room_from(s.clone()).unwrap().name, "bob");
        s.display_name = None;
        assert_eq!(room_from(s.clone()).unwrap().name, "bob");

        s.name = String::new();
        assert_eq!(room_from(s).unwrap().name, "rid");
    }

    // ROOM-003
    #[test]
    fn copies_the_counts_and_the_alert() {
        let mut s = sub("r", RoomType::Channel);
        s.unread = 7;
        s.user_mentions = 2;
        s.group_mentions = 1;
        s.alert = true;
        let room = room_from(s).unwrap();
        assert_eq!(
            (
                room.unread,
                room.user_mentions,
                room.group_mentions,
                room.alert
            ),
            (7, 2, 1, true)
        );
    }

    // ROOM-006
    #[test]
    fn copies_the_favorite_flag() {
        let mut s = sub("r", RoomType::Channel);
        assert!(!room_from(s.clone()).unwrap().favorite);
        s.favorite = true;
        assert!(room_from(s).unwrap().favorite);
    }

    // ROOM-007
    #[test]
    fn only_channels_and_private_groups_can_be_left() {
        let can_leave = |kind| {
            let mut s = sub("r", kind);
            s.name = "x".into();
            room_from(s).unwrap().can_leave()
        };
        assert!(can_leave(RoomType::Channel));
        assert!(can_leave(RoomType::Private));
        assert!(!can_leave(RoomType::Direct));
    }

    fn entry(kind: Option<&str>, text: &str) -> rc_api::Message {
        rc_api::Message {
            id: "m1".into(),
            room_id: "r1".into(),
            ts: "2026-10-20T14:12:48.587Z".parse().unwrap(),
            author: rc_api::Author {
                id: "u1".into(),
                username: "alice".into(),
                name: Some("Alice A".into()),
            },
            text: text.into(),
            kind: kind.map(str::to_owned),
            thread_id: None,
            thread_shown: false,
            md: None,
            attachments: Vec::new(),
            quotes: Vec::new(),
            edited: false,
            reactions: Vec::new(),
        }
    }

    // RICH-006, RICH-007
    #[test]
    fn maps_the_edit_flag_and_the_reactions_in_order() {
        let mut entry = entry(None, "hi");
        entry.edited = true;
        entry.reactions = vec![
            rc_api::Reaction {
                emoji: ":b:".into(),
                usernames: vec!["x".into(), "y".into()],
            },
            rc_api::Reaction {
                emoji: ":a:".into(),
                usernames: vec!["z".into()],
            },
        ];
        let message = message_from(entry).unwrap();
        assert!(message.edited);
        assert_eq!(
            message.reactions,
            [
                Reaction {
                    emoji: ":b:".into(),
                    usernames: vec!["x".into(), "y".into()],
                },
                Reaction {
                    emoji: ":a:".into(),
                    usernames: vec!["z".into()],
                },
            ]
        );
        let plain = message_from(self::entry(None, "hi")).unwrap();
        assert!(!plain.edited);
        assert_eq!(plain.reactions, []);
    }

    fn reacted(reactions: &[(&str, &[&str])]) -> Message {
        let mut message = message_from(entry(None, "hi")).unwrap();
        message.reactions = reactions
            .iter()
            .map(|(emoji, users)| Reaction {
                emoji: (*emoji).to_owned(),
                usernames: users.iter().map(|u| (*u).to_owned()).collect(),
            })
            .collect();
        message
    }

    // ACT-003
    #[test]
    fn a_reaction_adds_the_user_to_its_emoji_or_starts_one_at_the_end() {
        let mut message = reacted(&[(":a:", &["x"]), (":b:", &["y"])]);
        assert!(message.set_reaction(":a:", "z", true));
        assert!(message.set_reaction(":c:", "z", true));
        assert_eq!(
            message,
            reacted(&[(":a:", &["x", "z"]), (":b:", &["y"]), (":c:", &["z"])])
        );
    }

    // ACT-003
    #[test]
    fn taking_a_reaction_back_drops_the_emoji_once_nobody_is_left() {
        let mut message = reacted(&[(":a:", &["x", "z"]), (":b:", &["z"])]);
        assert!(message.set_reaction(":a:", "z", false));
        assert!(message.set_reaction(":b:", "z", false));
        assert_eq!(message, reacted(&[(":a:", &["x"])]));
    }

    // ACT-003
    #[test]
    fn reacting_is_a_set_so_repeating_it_changes_nothing() {
        let mut message = reacted(&[(":a:", &["x"])]);
        assert!(!message.set_reaction(":a:", "x", true));
        assert!(!message.set_reaction(":a:", "nobody", false));
        assert!(!message.set_reaction(":other:", "x", false));
        assert_eq!(message, reacted(&[(":a:", &["x"])]));
    }

    // RICH-005
    #[test]
    fn maps_the_attachments_of_a_message() {
        let mut entry = entry(None, "");
        entry.attachments = vec![
            rc_api::Attachment {
                title: "pic.png".into(),
                link: "/file-upload/a/pic.png".into(),
                description: Some("a pic".into()),
                size: None,
                image: Some(rc_api::AttachmentImage {
                    path: "/file-upload/b/pic.png".into(),
                    content_type: Some("image/png".into()),
                    dimensions: Some((480, 360)),
                }),
            },
            rc_api::Attachment {
                title: "notes.txt".into(),
                link: "/file-upload/c/notes.txt".into(),
                description: None,
                size: Some(6),
                image: None,
            },
        ];
        let message = message_from(entry).unwrap();
        assert_eq!(
            message.attachments,
            [
                Attachment {
                    title: "pic.png".into(),
                    link: "/file-upload/a/pic.png".into(),
                    description: Some("a pic".into()),
                    size: None,
                    image: Some(ImageAttachment {
                        path: "/file-upload/b/pic.png".into(),
                        width: Some(480),
                        height: Some(360),
                    }),
                },
                Attachment {
                    title: "notes.txt".into(),
                    link: "/file-upload/c/notes.txt".into(),
                    description: None,
                    size: Some(6),
                    image: None,
                },
            ]
        );
    }

    // MSG-001, MSG-003
    #[test]
    fn maps_a_text_message() {
        let message = message_from(entry(None, "hello")).unwrap();
        assert_eq!(message.id.as_str(), "m1");
        assert_eq!(message.room_id.as_str(), "r1");
        assert_eq!(message.ts.to_string(), "2026-10-20T14:12:48.587Z");
        assert_eq!(message.author.id.as_str(), "u1");
        assert_eq!(message.author.username, "alice");
        assert_eq!(message.author.display_name.as_deref(), Some("Alice A"));
        assert_eq!(message.kind, MessageKind::Text("hello".into()));
    }

    // RICH-001
    #[test]
    fn a_text_message_carries_its_markdown_as_blocks() {
        use crate::rich::{Block, Inline};
        let mut message = entry(None, "**hi**");
        message.md = Some(vec![rc_api::md::Block::Paragraph(vec![
            rc_api::md::Inline::Bold(vec![rc_api::md::Inline::Text("hi".into())]),
        ])]);
        let MessageKind::Text(body) = message_from(message).unwrap().kind else {
            panic!()
        };
        assert_eq!(body.text, "**hi**");
        assert_eq!(
            body.blocks,
            [Block::Paragraph(vec![Inline::Bold(vec![Inline::Text(
                "hi".into()
            )])])]
        );
    }

    fn link_node(url: &str) -> rc_api::md::Inline {
        rc_api::md::Inline::Link {
            url: url.into(),
            label: vec![rc_api::md::Inline::Text(" ".into())],
        }
    }

    fn reply_markdown(link: &str, text: &str) -> Vec<rc_api::md::Block> {
        vec![rc_api::md::Block::Paragraph(vec![
            link_node(link),
            rc_api::md::Inline::Text(format!(" {text}")),
        ])]
    }

    // ACT-005
    #[test]
    fn maps_the_quotes_of_a_message_and_hides_their_links() {
        use crate::rich::{Block, Inline};
        let outer_link = "http://s:3000/channel/c?msg=second";
        let inner_link = "http://s:3000/channel/c?msg=first";
        let mut entry = entry(None, &format!("[ ]({outer_link}) my reply"));
        entry.md = Some(reply_markdown(outer_link, "my reply"));
        // The quoted message is itself a reply, to `first`.
        entry.quotes = vec![rc_api::Quote {
            link: outer_link.into(),
            author: "bob".into(),
            text: format!("[ ]({inner_link}) second"),
            md: Some(reply_markdown(inner_link, "second")),
            ts: Some("2026-10-20T14:00:00Z".parse().unwrap()),
            quoted_links: vec![inner_link.into()],
        }];

        let message = message_from(entry).unwrap();

        let MessageKind::Text(body) = &message.kind else {
            panic!("not text");
        };
        assert_eq!(body.text, "my reply");
        assert_eq!(
            body.blocks,
            [Block::Paragraph(vec![Inline::Text("my reply".into())])]
        );
        let [quote] = message.quotes.as_slice() else {
            panic!("expected one quote, got {:?}", message.quotes);
        };
        assert_eq!(quote.message, Some(MessageId::from("second".to_owned())));
        assert_eq!(quote.link, outer_link);
        assert_eq!(quote.author, "bob");
        assert_eq!(quote.ts, Some("2026-10-20T14:00:00Z".parse().unwrap()));
        assert_eq!(quote.text.text, "second");
        assert_eq!(
            quote.text.blocks,
            [Block::Paragraph(vec![Inline::Text("second".into())])]
        );
    }

    // ACT-005
    #[test]
    fn a_link_that_the_server_made_no_quote_of_stays_in_the_text() {
        let text = "[ ](http://elsewhere/channel/c?msg=x) hi";
        let message = message_from(entry(None, text)).unwrap();
        assert_eq!(message.kind, MessageKind::Text(RichText::plain(text)));
        assert_eq!(message.quotes, []);
    }

    // ACT-005
    #[test]
    fn a_message_that_is_only_a_quote_has_no_text() {
        let link = "http://s:3000/channel/c?msg=x";
        let mut entry = entry(None, &format!("[ ]({link}) "));
        entry.quotes = vec![rc_api::Quote {
            link: link.into(),
            author: "bob".into(),
            text: "x".into(),
            md: None,
            ts: None,
            quoted_links: Vec::new(),
        }];

        let message = message_from(entry).unwrap();

        assert_eq!(message.kind, MessageKind::Text(RichText::plain("")));
        assert_eq!(message.quotes[0].text, RichText::plain("x"));
    }

    // RICH-001
    #[test]
    fn a_text_message_without_markdown_is_its_plain_text() {
        let message = message_from(entry(None, "*hi*")).unwrap();
        assert_eq!(message.kind, MessageKind::Text(RichText::plain("*hi*")));
    }

    // MSG-005
    #[test]
    fn maps_the_system_message_kinds() {
        for (code, text, event) in [
            ("uj", "alice", SystemEvent::Joined),
            ("ul", "alice", SystemEvent::Left),
            (
                "au",
                "bob",
                SystemEvent::Added {
                    username: "bob".into(),
                },
            ),
            (
                "ru",
                "bob",
                SystemEvent::Removed {
                    username: "bob".into(),
                },
            ),
            (
                "room_changed_topic",
                "New topic",
                SystemEvent::TopicChanged {
                    topic: "New topic".into(),
                },
            ),
        ] {
            let message = message_from(entry(Some(code), text)).unwrap();
            assert_eq!(message.kind, MessageKind::System(event), "{code}");
        }
    }

    // MSG-005
    #[test]
    fn an_unknown_system_message_keeps_its_code() {
        let message = message_from(entry(Some("room_changed_description"), "d")).unwrap();
        assert_eq!(
            message.kind,
            MessageKind::System(SystemEvent::Other("room_changed_description".into()))
        );
    }

    // SEND-001
    #[test]
    fn generated_message_ids_are_17_alphanumeric_characters_and_differ() {
        let ids: Vec<_> = (0..100).map(|_| MessageId::generate()).collect();
        assert!(
            ids.iter()
                .all(|id| id.as_str().len() == 17 && id.as_str().chars().all(char::is_alphanumeric))
        );
        let distinct: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(distinct.len(), ids.len());
    }

    // SEND-001
    #[test]
    fn server_messages_are_sent() {
        let message = message_from(entry(None, "hello")).unwrap();
        assert_eq!(message.delivery, Delivery::Sent);
        assert!(!message.is_unsent());
        assert!(
            Message {
                delivery: Delivery::Pending,
                ..message.clone()
            }
            .is_unsent()
        );
        assert!(
            Message {
                delivery: Delivery::Failed,
                ..message
            }
            .is_unsent()
        );
    }

    // RICH-002
    #[test]
    fn only_a_text_message_can_mention_the_user() {
        let mut message = message_from(entry(None, "hello")).unwrap();
        message.kind = MessageKind::Text(RichText {
            text: "hi @me".into(),
            blocks: vec![crate::rich::Block::Paragraph(vec![
                crate::rich::Inline::Mention(crate::rich::Mention::User("me".into())),
            ])],
        });
        assert!(message.mentions("me"));
        assert!(!message.mentions("you"));
        message.kind = MessageKind::System(SystemEvent::Joined);
        assert!(!message.mentions("me"));
    }

    // MSG-001
    #[test]
    fn thread_replies_are_only_shown_when_sent_to_the_channel() {
        let mut reply = entry(None, "in the thread");
        reply.thread_id = Some("parent".into());
        assert!(message_from(reply.clone()).is_none());
        reply.thread_shown = true;
        assert!(message_from(reply).is_some());
    }
}
