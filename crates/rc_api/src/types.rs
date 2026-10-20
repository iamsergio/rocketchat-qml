// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Public types of the REST API and the realtime connection. Plain data: the serde types they come
//! from are in `wire`.

use std::fmt;

use secrecy::SecretString;

/// What a login gives: enough to make authenticated calls and to resume the session later.
#[derive(Clone, Debug)]
pub struct Credentials {
    pub user_id: String,
    /// Debug output doesn't reveal it.
    pub token: SecretString,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct User {
    pub id: String,
    pub username: String,
    pub name: Option<String>,
}

#[derive(Clone, Debug)]
pub struct LoginResult {
    pub credentials: Credentials,
    pub user: User,
}

/// Server version as `/api/info` reports it. Patch and suffix are ignored: servers only send
/// major.minor to unauthenticated clients.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ServerVersion {
    pub major: u32,
    pub minor: u32,
}

impl ServerVersion {
    /// Reads `8.0`, `8.0.1`, `8.10.0-develop` or `8.9.0-rc.5`. `None` if there's no numeric
    /// major.minor.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let mut parts = text.split('.');
        let major = parts.next()?.parse().ok()?;
        // The minor can carry a suffix when there's no patch ("8.9-rc.5").
        let minor = parts.next()?.split(['-', '+']).next()?.parse().ok()?;
        Some(Self { major, minor })
    }
}

impl fmt::Display for ServerVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerInfo {
    pub version: ServerVersion,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RoomType {
    Channel,
    Private,
    Direct,
    /// Anything else the server sends, e.g. `l` for livechat.
    Other(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Subscription {
    pub room_id: String,
    pub room_type: RoomType,
    /// Empty if the server didn't send one. For direct messages it's the other user's username.
    pub name: String,
    /// The human-readable name, when the server has one.
    pub display_name: Option<String>,
    pub unread: u32,
    pub user_mentions: u32,
    pub group_mentions: u32,
    pub alert: bool,
    pub open: bool,
    /// When the user joined the room.
    pub created: Option<jiff::Timestamp>,
    /// The time of the room's last message. Only [`RestSession::subscriptions`] fills it in, from
    /// the rooms, and only for rooms with messages: the subscription itself doesn't carry it.
    ///
    /// [`RestSession::subscriptions`]: crate::RestSession::subscriptions
    pub last_message: Option<jiff::Timestamp>,
}

/// Who wrote a message, as the message carries it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Author {
    pub id: String,
    pub username: String,
    pub name: Option<String>,
}

/// One message of a room, as the history gives it. System messages (someone joined, the topic
/// changed...) are messages too: [`kind`](Self::kind) tells them apart.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub id: String,
    pub room_id: String,
    pub ts: jiff::Timestamp,
    pub author: Author,
    /// The text. For system messages it is not prose but what the event is about: the new topic,
    /// or the username that was added or removed.
    pub text: String,
    /// The server's type code (`t`): `None` for an ordinary message, `uj` for "user joined" and so
    /// on.
    pub kind: Option<String>,
    /// The thread this message replies to, if it is a thread reply (`tmid`).
    pub thread_id: Option<String>,
    /// Whether a thread reply is also shown in the channel (`tshow`).
    pub thread_shown: bool,
    /// The server's parsed markdown of the text. `None` if it sent none or it isn't a list of
    /// nodes. Some old messages and some system messages have no `md`.
    pub md: Option<Vec<crate::md::Block>>,
}

/// An emoji the server has added, shown as the image at `/emoji-custom/<name>.<extension>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CustomEmoji {
    /// What is typed between the colons.
    pub name: String,
    /// Other names for it.
    pub aliases: Vec<String>,
    /// The file's extension, without the dot.
    pub extension: String,
}

/// One page of a room's history.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryPage {
    /// Newest first, as the server sends them.
    pub messages: Vec<Message>,
    /// The server had fewer messages than asked for, so there is nothing before them.
    pub reached_start: bool,
}

/// What the server pushes over the realtime connection, for the streams the client subscribed to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DdpEvent {
    /// A new or edited message in one of the user's rooms. System messages are messages too. The
    /// server also sends the messages of public channels the user isn't in, so a caller that cares
    /// checks the room against its subscriptions.
    Message(Message),
    /// The user joined a room, or one of their rooms changed: its unread count, its name...
    SubscriptionChanged(Subscription),
    /// The user left a room or was removed from it.
    SubscriptionRemoved { room_id: String },
    /// One of the user's rooms has a message at `last_message`. Pushed for every new message, and
    /// again for other changes of the room (a rename...), with the same time.
    RoomActivity {
        room_id: String,
        last_message: jiff::Timestamp,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(major: u32, minor: u32) -> ServerVersion {
        ServerVersion { major, minor }
    }

    // AUTH-003
    #[test]
    fn parses_versions() {
        assert_eq!(ServerVersion::parse("8.0"), Some(version(8, 0)));
        assert_eq!(ServerVersion::parse("8.0.1"), Some(version(8, 0)));
        assert_eq!(ServerVersion::parse("8.10.0-develop"), Some(version(8, 10)));
        assert_eq!(ServerVersion::parse("8.9.0-rc.5"), Some(version(8, 9)));
        assert_eq!(ServerVersion::parse("8.9-rc.5"), Some(version(8, 9)));
    }

    // AUTH-003
    #[test]
    fn rejects_garbage_versions() {
        for text in ["", "8", "abc", "8.x", ".1", "8.", "-1.0"] {
            assert_eq!(ServerVersion::parse(text), None, "{text:?}");
        }
    }

    // AUTH-003
    #[test]
    fn orders_versions_numerically() {
        let develop = ServerVersion::parse("8.10.0-develop").unwrap();
        assert!(develop > ServerVersion::parse("8.9").unwrap());
        assert!(version(9, 0) > develop);
        assert_eq!(version(8, 0).to_string(), "8.0");
    }
}
