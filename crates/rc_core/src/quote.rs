// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! How a message quotes another: the server makes a quote of a message whose text starts with an
//! empty-label link to the permalink of the quoted one, and only if the permalink begins with its
//! own `Site_Url`. The pure parts of building and reading those links.

use std::fmt::Write as _;

use crate::domain::{MessageId, Quote, Room, RoomKind};

/// The permalink of a message of `room`, as the server's web client makes them:
/// `<site>/<channel|group|direct>/<room name, or room id for a direct message>?msg=<id>`. The
/// server only looks at the host and at `msg`, but the link opens in the web client too.
#[must_use]
pub(crate) fn permalink(site: &str, room: &Room, message: &MessageId) -> String {
    let (kind, name) = match room.kind {
        RoomKind::Channel => ("channel", room.name.as_str()),
        RoomKind::Private => ("group", room.name.as_str()),
        RoomKind::Direct => ("direct", room.id.as_str()),
    };
    format!(
        "{}/{kind}/{}?msg={message}",
        site.trim_end_matches('/'),
        encode(name)
    )
}

/// Percent-encodes what a link or its markdown would take for something else.
fn encode(segment: &str) -> String {
    let mut out = String::with_capacity(segment.len());
    for byte in segment.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            // Writing to a string doesn't fail.
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

/// The message a permalink points at: its `msg` parameter.
#[must_use]
pub(crate) fn quoted_message(link: &str) -> Option<MessageId> {
    let query = link.split('#').next()?.split_once('?')?.1;
    query
        .split('&')
        .find_map(|pair| pair.strip_prefix("msg="))
        .filter(|id| !id.is_empty())
        .map(|id| MessageId::from(id.to_owned()))
}

/// What goes to the server for a message with these quotes and this text: a link to each quoted
/// message, in order, then the text.
#[must_use]
pub(crate) fn wire_text(quotes: &[Quote], text: &str) -> String {
    let mut wire = String::new();
    for quote in quotes {
        wire.push_str("[ ](");
        wire.push_str(&quote.link);
        wire.push_str(") ");
    }
    wire.push_str(text);
    wire
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::RoomId;

    fn room(kind: RoomKind, id: &str, name: &str) -> Room {
        Room {
            id: RoomId::from(id.to_owned()),
            kind,
            name: name.to_owned(),
            peer: None,
            avatar_etag: None,
            topic: String::new(),
            unread: 0,
            user_mentions: 0,
            group_mentions: 0,
            alert: false,
            favorite: false,
            last_activity: jiff::Timestamp::UNIX_EPOCH,
        }
    }

    fn id(text: &str) -> MessageId {
        MessageId::from(text.to_owned())
    }

    fn quote(link: &str) -> Quote {
        Quote {
            message: quoted_message(link),
            link: link.to_owned(),
            author: "bob".into(),
            text: "x".into(),
            ts: None,
        }
    }

    // ACT-005
    #[test]
    fn permalinks_name_the_kind_of_room_and_the_message() {
        let site = "http://127.0.0.1:3000";
        let channel = room(RoomKind::Channel, "r1", "general");
        let group = room(RoomKind::Private, "r2", "secret-club");
        let direct = room(RoomKind::Direct, "r3", "Bob Builder");
        assert_eq!(
            permalink(site, &channel, &id("m1")),
            "http://127.0.0.1:3000/channel/general?msg=m1"
        );
        assert_eq!(
            permalink(site, &group, &id("m1")),
            "http://127.0.0.1:3000/group/secret-club?msg=m1"
        );
        // A direct message is named by its id: its name is a person's.
        assert_eq!(
            permalink(site, &direct, &id("m1")),
            "http://127.0.0.1:3000/direct/r3?msg=m1"
        );
    }

    // ACT-005
    #[test]
    fn a_permalink_has_one_slash_after_the_site_and_encodes_the_room_name() {
        let channel = room(RoomKind::Channel, "r1", "my room (1)/é");
        assert_eq!(
            permalink("https://chat.example.com/", &channel, &id("m1")),
            "https://chat.example.com/channel/my%20room%20%281%29%2F%C3%A9?msg=m1"
        );
    }

    // ACT-005
    #[test]
    fn the_quoted_message_is_the_msg_parameter() {
        let msg = |link: &str| quoted_message(link).map(|id| id.as_str().to_owned());
        assert_eq!(
            msg("http://127.0.0.1:3000/channel/general?msg=abc"),
            Some("abc".into())
        );
        assert_eq!(
            msg("http://s/direct/r?x=1&msg=abc&y=2#top"),
            Some("abc".into())
        );
        assert_eq!(msg("http://s/channel/general"), None);
        assert_eq!(msg("http://s/channel/general?msg="), None);
        assert_eq!(msg("http://s/channel/general?other=abc"), None);
        assert_eq!(msg("http://s/channel/general#?msg=abc"), None);
    }

    // ACT-005
    #[test]
    fn the_wire_text_starts_with_a_link_to_each_quote_in_order() {
        let quotes = [
            quote("http://s/channel/c?msg=a"),
            quote("http://s/channel/c?msg=b"),
        ];
        assert_eq!(
            wire_text(&quotes, "hello"),
            "[ ](http://s/channel/c?msg=a) [ ](http://s/channel/c?msg=b) hello"
        );
        assert_eq!(wire_text(&[], "hello"), "hello");
    }
}
