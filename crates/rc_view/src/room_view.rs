// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! What the `RoomList` model and the `Room` view model show for a room. Plain Rust, so it can be
//! unit-tested without Qt.

use rc_core::domain::{MessageId, Room, RoomId, RoomKind};
use rc_core::policy::MessageAction;
use rc_core::timeline::TimelineStatus;

/// How the GUI sees the kind of a room: a string rather than an enum of the toolkit's.
#[must_use]
pub fn kind_name(kind: RoomKind) -> &'static str {
    match kind {
        RoomKind::Channel => "channel",
        RoomKind::Private => "private",
        RoomKind::Direct => "direct",
    }
}

/// How the GUI sees what was done to a message, in the answer to an edit, a deletion or a
/// reaction.
#[must_use]
pub fn action_name(action: MessageAction) -> &'static str {
    match action {
        MessageAction::Edit => "edit",
        MessageAction::Delete => "delete",
        MessageAction::React => "react",
    }
}

/// Whether any of the rooms is a favorite: the room list shows its sections only then.
#[must_use]
pub fn any_favorite(rooms: &[Room]) -> bool {
    rooms.iter().any(|room| room.favorite)
}

/// What sending `text` from the composer hands to the core: the open room, the text and the
/// message it quotes (`quoted_id` is empty for none), or `None` if there is nothing to send,
/// because no room is open or the text is blank.
#[must_use]
pub fn outgoing(
    room_id: &str,
    text: &str,
    quoted_id: &str,
) -> Option<(RoomId, String, Option<MessageId>)> {
    if room_id.is_empty() || text.trim().is_empty() {
        return None;
    }
    let quoted = (!quoted_id.is_empty()).then(|| MessageId::from(quoted_id.to_owned()));
    Some((RoomId::from(room_id.to_owned()), text.to_owned(), quoted))
}

/// The line that says who is typing, from their names in the order they started: empty for none,
/// then `alice is typing…`, `alice and bob are typing…`, `alice, bob and carol are typing…` and,
/// from four, `alice, bob and 2 others are typing…` (the first two, then how many more).
#[must_use]
pub fn typing_line(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => format!("{one} is typing\u{2026}"),
        [a, b] => format!("{a} and {b} are typing\u{2026}"),
        [a, b, c] => format!("{a}, {b} and {c} are typing\u{2026}"),
        [a, b, rest @ ..] => format!("{a}, {b} and {} others are typing\u{2026}", rest.len()),
    }
}

/// The values of the `Room` properties: the open room, and how loading its messages is going.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RoomState {
    /// Empty when no room is open.
    pub room_id: String,
    /// The name and kind the room list shows; empty while the room isn't in it.
    pub name: String,
    pub kind: String,
    /// What the room is about, as the markdown the server has; empty for none.
    pub topic: String,
    pub loading: bool,
    pub loading_older: bool,
    pub reached_start: bool,
    /// Why the last load failed, as it is to be shown. Empty if it didn't.
    pub error_text: String,
    /// Who is typing in the room, as [`typing_line`] says it. Empty for nobody.
    pub typing: String,
}

/// The state for the room `status` is about, with its name, kind and topic taken from `rooms`.
#[must_use]
pub fn room_state(rooms: &[Room], status: &TimelineStatus) -> RoomState {
    let Some(id) = &status.room else {
        return RoomState::default();
    };
    let room = rooms.iter().find(|room| &room.id == id);
    RoomState {
        room_id: id.to_string(),
        name: room.map(|room| room.name.clone()).unwrap_or_default(),
        kind: room
            .map(|room| kind_name(room.kind).to_owned())
            .unwrap_or_default(),
        topic: room.map_or_else(String::new, |room| room.topic.clone()),
        loading: status.loading,
        loading_older: status.loading_older,
        reached_start: status.reached_start,
        error_text: status
            .error
            .as_ref()
            .map_or_else(String::new, ToString::to_string),
        typing: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use rc_core::rooms::{RoomChange, RoomList};
    use rc_core::timeline::TimelineError;

    use super::*;

    // ROOM-002
    #[test]
    fn each_kind_has_its_own_name() {
        let kinds = [RoomKind::Channel, RoomKind::Private, RoomKind::Direct];
        assert_eq!(kinds.map(kind_name), ["channel", "private", "direct"]);
    }

    fn room(id: &str, name: &str, kind: RoomKind) -> Room {
        Room {
            id: id.to_owned().into(),
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
            unread_threads: Vec::new(),
            mentioned_threads: Vec::new(),
            last_activity: jiff::Timestamp::UNIX_EPOCH,
        }
    }

    fn names(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    // ACT-006
    #[test]
    fn nobody_typing_is_no_line() {
        assert_eq!(typing_line(&[]), "");
    }

    // ACT-006
    #[test]
    fn one_typist_is_named() {
        assert_eq!(typing_line(&names(&["alice"])), "alice is typing\u{2026}");
    }

    // ACT-006
    #[test]
    fn two_typists_are_named_with_and() {
        assert_eq!(
            typing_line(&names(&["alice", "bob"])),
            "alice and bob are typing\u{2026}"
        );
    }

    // ACT-006
    #[test]
    fn three_typists_are_all_named() {
        assert_eq!(
            typing_line(&names(&["alice", "bob", "carol"])),
            "alice, bob and carol are typing\u{2026}"
        );
    }

    // ACT-006
    #[test]
    fn from_four_typists_the_first_two_are_named_and_the_rest_counted() {
        assert_eq!(
            typing_line(&names(&["alice", "bob", "carol", "dave"])),
            "alice, bob and 2 others are typing\u{2026}"
        );
        assert_eq!(
            typing_line(&names(&["a", "b", "c", "d", "e", "f"])),
            "a, b and 4 others are typing\u{2026}"
        );
    }

    // ACT-001, ACT-002, ACT-003
    #[test]
    fn each_action_has_its_own_name() {
        let actions = [
            MessageAction::Edit,
            MessageAction::Delete,
            MessageAction::React,
        ];
        assert_eq!(actions.map(action_name), ["edit", "delete", "react"]);
    }

    fn favorite(room: Room) -> Room {
        Room {
            favorite: true,
            ..room
        }
    }

    // ROOM-006
    #[test]
    fn there_are_favorites_while_any_room_is_one() {
        let (mut list, mut mirror) = (RoomList::default(), Vec::new());
        let now = jiff::Timestamp::UNIX_EPOCH;
        let mut check = |change: Option<RoomChange>, expected: bool| {
            if let Some(change) = change {
                change.apply(&mut mirror);
            }
            assert_eq!(any_favorite(&mirror), expected, "{mirror:?}");
        };
        check(Some(list.reset(Vec::new(), now)), false);
        check(
            list.upsert(room("a", "alpha", RoomKind::Channel), now),
            false,
        );
        check(
            list.upsert(favorite(room("b", "bravo", RoomKind::Direct)), now),
            true,
        );
        // Moves: a bump of a room above the other one, and a favorite taken back and set again.
        check(
            list.upsert(favorite(room("a", "alpha", RoomKind::Channel)), now),
            true,
        );
        check(list.upsert(room("b", "bravo", RoomKind::Direct), now), true);
        check(
            list.upsert(room("a", "alpha", RoomKind::Channel), now),
            false,
        );
        check(
            list.upsert(favorite(room("c", "charlie", RoomKind::Private)), now),
            true,
        );
        // Updates in place.
        let mut busy = favorite(room("c", "charlie", RoomKind::Private));
        busy.unread = 2;
        check(list.upsert(busy, now), true);
        // Removals.
        check(list.remove(&"a".to_owned().into()), true);
        check(list.remove(&"c".to_owned().into()), false);
        check(
            Some(list.reset(vec![favorite(room("d", "delta", RoomKind::Channel))], now)),
            true,
        );
        check(Some(list.reset(Vec::new(), now)), false);
    }

    // SEND-001
    #[test]
    fn only_text_with_something_in_it_is_sent_to_an_open_room() {
        let (room, text, quoted) = outgoing("r1", "hello\n", "").unwrap();
        assert_eq!((room.as_str(), text.as_str()), ("r1", "hello\n"));
        assert_eq!(quoted, None);
        assert!(outgoing("", "hello", "").is_none());
        assert!(outgoing("r1", "", "").is_none());
        assert!(outgoing("r1", " \n\t", "").is_none());
    }

    // ACT-005
    #[test]
    fn a_message_to_quote_goes_along_with_the_text() {
        let (_, text, quoted) = outgoing("r1", "hello", "m7").unwrap();
        assert_eq!(text, "hello");
        assert_eq!(quoted, Some(MessageId::from("m7".to_owned())));
        // The quote doesn't make blank text worth sending.
        assert!(outgoing("r1", " ", "m7").is_none());
    }

    // MSG-001
    #[test]
    fn no_open_room_is_the_empty_state() {
        let rooms = [room("1", "general", RoomKind::Channel)];
        assert_eq!(
            room_state(&rooms, &TimelineStatus::default()),
            RoomState::default()
        );
    }

    // MSG-001
    #[test]
    fn the_open_room_takes_its_name_and_kind_from_the_list() {
        let rooms = [
            room("1", "general", RoomKind::Channel),
            room("2", "Bob", RoomKind::Direct),
        ];
        let status = TimelineStatus {
            room: Some("2".to_owned().into()),
            loading: true,
            ..TimelineStatus::default()
        };
        assert_eq!(
            room_state(&rooms, &status),
            RoomState {
                room_id: "2".into(),
                name: "Bob".into(),
                kind: "direct".into(),
                loading: true,
                ..RoomState::default()
            }
        );
    }

    // ROOM-009
    #[test]
    fn the_open_room_takes_its_topic_from_the_list() {
        let rooms = [
            Room {
                topic: "**Plans** for <b>May</b> & https://example.com :rocket:".into(),
                ..room("1", "general", RoomKind::Channel)
            },
            room("2", "random", RoomKind::Channel),
        ];
        let topic_of = |id: &str| {
            let status = TimelineStatus {
                room: Some(id.to_owned().into()),
                ..TimelineStatus::default()
            };
            room_state(&rooms, &status).topic
        };
        assert_eq!(
            topic_of("1"),
            "**Plans** for <b>May</b> & https://example.com :rocket:"
        );
        assert_eq!(topic_of("2"), "");
        let none = room_state(&rooms, &TimelineStatus::default());
        assert_eq!(none.topic, "");
    }

    // MSG-002
    #[test]
    fn the_state_carries_the_loading_flags_and_the_error() {
        let status = TimelineStatus {
            room: Some("9".to_owned().into()),
            loading: false,
            loading_older: true,
            reached_start: true,
            error: Some(TimelineError::Unreachable),
        };
        let state = room_state(&[], &status);
        assert_eq!(state.room_id, "9");
        assert_eq!(state.name, "");
        assert!(state.loading_older && state.reached_start && !state.loading);
        assert_eq!(state.error_text, "Can't reach the server");
    }
}
