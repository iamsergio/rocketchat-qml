// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Searching the room list by name. Plain Rust, so each frontend shows the same results.

use rc_core::domain::Room;
use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

/// The query as typed, without the whitespace around it. Empty means not searching.
#[must_use]
pub fn trimmed(query: &str) -> &str {
    query.trim()
}

/// `text` as compared: without accents and in lower case, so `Zoë` and `ZOE` are the same.
fn folded(text: &str) -> String {
    text.nfd()
        .filter(|c| !is_combining_mark(*c))
        .flat_map(char::to_lowercase)
        .collect()
}

/// The rooms matching `query`, from `shown`, the rooms of the list in its order, and `quiet`, the
/// ones it holds back, in the core's order. The ones whose name starts with the query come first,
/// then the ones that merely contain it; in each group the shown rooms come first, then the quiet
/// ones, each in the order they were given. A room is found by the name the list shows and, for a
/// direct message, by the other user's username too. An empty query (after trimming) matches
/// nothing: it isn't a search.
#[must_use]
pub fn search_rooms(shown: &[Room], quiet: &[Room], query: &str) -> Vec<Room> {
    let query = folded(trimmed(query));
    if query.is_empty() {
        return Vec::new();
    }
    let (mut starting, containing): (Vec<_>, Vec<_>) = shown
        .iter()
        .chain(quiet)
        .map(|room| {
            let names = [Some(room.name.as_str()), room.peer.as_deref()]
                .into_iter()
                .flatten()
                .map(folded)
                .collect::<Vec<_>>();
            (room, names)
        })
        .filter(|(_, names)| names.iter().any(|name| name.contains(&query)))
        .partition(|(_, names)| names.iter().any(|name| name.starts_with(&query)));
    starting.extend(containing);
    starting.into_iter().map(|(room, _)| room.clone()).collect()
}

#[cfg(test)]
mod tests {
    use rc_core::domain::RoomKind;

    use super::*;

    fn room(id: &str, name: &str) -> Room {
        Room {
            id: id.to_owned().into(),
            kind: RoomKind::Channel,
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

    fn dm(id: &str, name: &str, username: &str) -> Room {
        Room {
            kind: RoomKind::Direct,
            peer: Some(username.to_owned()),
            ..room(id, name)
        }
    }

    fn ids(rooms: &[Room]) -> Vec<&str> {
        rooms.iter().map(|room| room.id.as_str()).collect()
    }

    // ROOM-010
    #[test]
    fn rooms_starting_with_the_query_come_before_those_containing_it() {
        let rooms = [
            room("a", "my-dev"),
            room("b", "dev-ops"),
            room("c", "devel"),
        ];
        assert_eq!(ids(&search_rooms(&rooms, &[], "dev")), ["b", "c", "a"]);
    }

    // ROOM-010
    #[test]
    fn each_group_keeps_the_order_of_the_list() {
        let rooms = [
            room("a", "x-dev"),
            room("b", "dev-2"),
            room("c", "y-dev"),
            room("d", "dev-1"),
        ];
        assert_eq!(ids(&search_rooms(&rooms, &[], "dev")), ["b", "d", "a", "c"]);
    }

    // ROOM-010
    #[test]
    fn case_does_not_matter_on_either_side() {
        let rooms = [room("a", "General"), room("b", "random")];
        assert_eq!(ids(&search_rooms(&rooms, &[], "gEN")), ["a"]);
        assert_eq!(ids(&search_rooms(&rooms, &[], "RAND")), ["b"]);
    }

    // ROOM-010
    #[test]
    fn accents_do_not_matter_on_either_side() {
        let rooms = [
            room("a", "Café Olé"),
            room("b", "cafe-two"),
            room("c", "Zoë"),
        ];
        assert_eq!(ids(&search_rooms(&rooms, &[], "cafe")), ["a", "b"]);
        assert_eq!(ids(&search_rooms(&rooms, &[], "CAFÉ")), ["a", "b"]);
        assert_eq!(ids(&search_rooms(&rooms, &[], "zoe")), ["c"]);
        assert_eq!(ids(&search_rooms(&rooms, &[], "ole")), ["a"]);
    }

    // ROOM-010
    #[test]
    fn a_direct_message_is_found_by_the_name_shown_and_by_the_username() {
        let rooms = [dm("a", "Alice Smith", "asmith"), room("b", "general")];
        assert_eq!(ids(&search_rooms(&rooms, &[], "alice")), ["a"]);
        assert_eq!(ids(&search_rooms(&rooms, &[], "asm")), ["a"]);
        assert_eq!(ids(&search_rooms(&rooms, &[], "ith")), ["a"]);
    }

    // ROOM-010
    #[test]
    fn a_username_prefix_ranks_with_the_names_that_start_with_it() {
        let rooms = [room("a", "the-bob-room"), dm("b", "Robert", "bob")];
        assert_eq!(ids(&search_rooms(&rooms, &[], "bob")), ["b", "a"]);
    }

    // ROOM-010
    #[test]
    fn a_room_is_listed_once_when_both_names_match() {
        let rooms = [dm("a", "bob", "bob")];
        assert_eq!(ids(&search_rooms(&rooms, &[], "bob")), ["a"]);
    }

    // ROOM-010
    #[test]
    fn whitespace_around_the_query_is_ignored() {
        let rooms = [room("a", "general")];
        assert_eq!(ids(&search_rooms(&rooms, &[], "  gen\t")), ["a"]);
    }

    // ROOM-010
    #[test]
    fn an_empty_or_blank_query_is_not_a_search() {
        let rooms = [room("a", "general")];
        assert!(search_rooms(&rooms, &[], "").is_empty());
        assert!(search_rooms(&rooms, &[], " \t ").is_empty());
        assert_eq!(trimmed(" \t "), "");
    }

    // ROOM-010
    #[test]
    fn no_match_is_no_rooms() {
        let rooms = [room("a", "general"), room("b", "random")];
        assert!(search_rooms(&rooms, &[], "zzz").is_empty());
        assert!(search_rooms(&[], &[], "gen").is_empty());
    }

    // ROOM-010
    #[test]
    fn a_quiet_room_ranks_by_its_name_like_any_other() {
        let shown = [room("a", "the-alice-room")];
        let quiet = [dm("b", "alice", "alice")];
        assert_eq!(ids(&search_rooms(&shown, &quiet, "alice")), ["b", "a"]);
    }

    // ROOM-010
    #[test]
    fn in_each_group_the_listed_rooms_come_before_the_quiet_ones() {
        let shown = [room("a", "x-alice"), room("b", "alice2")];
        let quiet = [dm("c", "alice", "alice"), dm("d", "y-alice", "yal")];
        assert_eq!(
            ids(&search_rooms(&shown, &quiet, "alice")),
            ["b", "c", "a", "d"]
        );
    }

    // ROOM-010
    #[test]
    fn the_quiet_rooms_keep_the_order_they_were_given() {
        let quiet = [dm("a", "bob b", "bb"), dm("b", "bob a", "ba")];
        assert_eq!(ids(&search_rooms(&[], &quiet, "bob")), ["a", "b"]);
        assert_eq!(ids(&search_rooms(&[], &quiet, "ba")), ["b"]);
    }

    // ROOM-010
    #[test]
    fn a_quiet_room_is_found_by_its_username_and_an_empty_query_finds_none() {
        let quiet = [dm("a", "Alice Smith", "asmith")];
        assert_eq!(ids(&search_rooms(&[], &quiet, "asm")), ["a"]);
        assert!(search_rooms(&[], &quiet, " ").is_empty());
    }
}
