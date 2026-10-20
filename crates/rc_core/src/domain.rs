// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! What the application knows about, independent of how the server spells it.

use std::fmt;

use secrecy::SecretString;

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
    pub unread: u32,
    /// Mentions of the user (`@name`).
    pub user_mentions: u32,
    /// `@all` and `@here`.
    pub group_mentions: u32,
    /// Something new that the counts don't cover (e.g. thread replies): show the room as unread.
    pub alert: bool,
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
    // username while its display name is their real name.
    let name = sub
        .display_name
        .filter(|name| !name.is_empty())
        .or_else(|| Some(sub.name).filter(|name| !name.is_empty()))
        .unwrap_or_else(|| sub.room_id.clone());
    Some(Room {
        id: sub.room_id.into(),
        kind,
        name,
        unread: sub.unread,
        user_mentions: sub.user_mentions,
        group_mentions: sub.group_mentions,
        alert: sub.alert,
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
        }
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
}
