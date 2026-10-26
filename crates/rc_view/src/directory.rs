// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! What the room list's search shows of the server's directory: the people and the channels it
//! found that the user has no room for already. Plain Rust, so each frontend shows the same.

use rc_core::directory::{Directory, DirectoryChannel, Person};
use rc_core::domain::{Room, RoomKind};

/// How many people, and how many channels, the search shows.
pub const SHOWN: usize = 5;

/// The part of a directory result worth showing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DirectoryView {
    /// People with no direct message with the user yet, never the user: at most [`SHOWN`].
    pub people: Vec<Person>,
    /// How many more people may match than `people` shows: the ones the server found beyond what
    /// it sent, and the ones this cut off. An upper bound, since some of the first may be people
    /// the user has a room with. The frontend says "N more, keep typing" when it is not zero.
    pub people_more: u32,
    /// Public channels the user is not in, the one with the most members first: at most
    /// [`SHOWN`].
    pub channels: Vec<DirectoryChannel>,
    /// How many more channels may match, as for `people_more`.
    pub channels_more: u32,
}

/// What `result` shows, given the rooms of the account: `shown`, the rooms of the list, and
/// `quiet`, the direct messages it holds back, and `own_username`. A person is left out if the
/// user is that person, or has a direct message with them (a quiet one too, which opening
/// brings back); a channel, if it is one of the rooms. Recompute it whenever the rooms change, so
/// that a channel just joined or a direct message just opened drops out without a new search.
#[must_use]
pub fn directory_view(
    result: &Directory,
    shown: &[Room],
    quiet: &[Room],
    own_username: &str,
) -> DirectoryView {
    let rooms = || shown.iter().chain(quiet);
    let people: Vec<_> = (result.people.iter())
        .filter(|person| !person.username.eq_ignore_ascii_case(own_username))
        .filter(|person| {
            !rooms().any(|room| {
                room.kind == RoomKind::Direct
                    && (room.peer.as_deref())
                        .is_some_and(|peer| peer.eq_ignore_ascii_case(&person.username))
            })
        })
        .cloned()
        .collect();
    let channels: Vec<_> = (result.channels.iter())
        .filter(|channel| !rooms().any(|room| room.id == channel.id))
        .cloned()
        .collect();
    DirectoryView {
        people_more: more(result.people_total, result.people.len(), people.len()),
        people: people.into_iter().take(SHOWN).collect(),
        channels_more: more(result.channels_total, result.channels.len(), channels.len()),
        channels: channels.into_iter().take(SHOWN).collect(),
    }
}

/// How many more entries there may be: those of the `total` that the server did not send (`sent`
/// did come), and those of the `kept` ones that [`SHOWN`] cuts off.
fn more(total: u32, sent: usize, kept: usize) -> u32 {
    let unsent = (total as usize).saturating_sub(sent);
    let cut = kept.saturating_sub(SHOWN);
    u32::try_from(unsent + cut).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use rc_core::directory::DirectoryState;

    use super::*;

    fn person(username: &str) -> Person {
        Person {
            username: username.to_owned(),
            name: None,
        }
    }

    fn channel(id: &str) -> DirectoryChannel {
        DirectoryChannel {
            id: id.to_owned().into(),
            name: id.to_owned(),
            topic: String::new(),
            members: 3,
        }
    }

    fn room(id: &str, kind: RoomKind, peer: Option<&str>) -> Room {
        Room {
            id: id.to_owned().into(),
            kind,
            name: peer.unwrap_or(id).to_owned(),
            peer: peer.map(str::to_owned),
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

    fn result(people: &[&str], channels: &[&str]) -> Directory {
        Directory {
            query: "q".to_owned(),
            state: DirectoryState::Ready,
            people: people.iter().map(|name| person(name)).collect(),
            people_total: u32::try_from(people.len()).unwrap(),
            channels: channels.iter().map(|id| channel(id)).collect(),
            channels_total: u32::try_from(channels.len()).unwrap(),
        }
    }

    fn names(people: &[Person]) -> Vec<&str> {
        people.iter().map(|p| p.username.as_str()).collect()
    }

    fn ids(channels: &[DirectoryChannel]) -> Vec<&str> {
        channels.iter().map(|c| c.id.as_str()).collect()
    }

    // ACT-007
    #[test]
    fn the_user_and_the_people_they_have_a_direct_message_with_are_left_out() {
        let found = result(&["me", "bob", "carol", "dave"], &[]);
        let dms = [room("d1", RoomKind::Direct, Some("bob"))];
        let quiet = [room("d2", RoomKind::Direct, Some("carol"))];

        let view = directory_view(&found, &dms, &quiet, "me");

        assert_eq!(names(&view.people), ["dave"]);
        assert_eq!(view.people_more, 0);
    }

    // ACT-007
    #[test]
    fn usernames_are_compared_without_regard_to_case() {
        let found = result(&["Me", "Bob"], &[]);
        let dms = [room("d1", RoomKind::Direct, Some("bob"))];

        let view = directory_view(&found, &dms, &[], "ME");

        assert!(view.people.is_empty());
    }

    // ACT-007
    #[test]
    fn a_room_that_is_not_a_direct_message_with_the_person_does_not_hide_them() {
        let found = result(&["bob"], &[]);
        // A channel called `bob`, and a group direct message, which has no single peer.
        let rooms = [
            room("bob", RoomKind::Channel, None),
            room("g", RoomKind::Direct, None),
        ];

        let view = directory_view(&found, &rooms, &[], "me");

        assert_eq!(names(&view.people), ["bob"]);
    }

    // ACT-008
    #[test]
    fn the_channels_the_user_is_in_are_left_out_quiet_rooms_included() {
        let found = result(&[], &["c1", "c2", "c3"]);
        let listed = [room("c1", RoomKind::Channel, None)];
        let quiet = [room("c3", RoomKind::Channel, None)];

        let view = directory_view(&found, &listed, &quiet, "me");

        assert_eq!(ids(&view.channels), ["c2"]);
    }

    // ACT-008
    #[test]
    fn at_most_five_of_each_show_and_the_rest_counts_as_more() {
        let found = result(
            &["a", "b", "c", "d", "e", "f", "g"],
            &["1", "2", "3", "4", "5", "6"],
        );

        let view = directory_view(&found, &[], &[], "me");

        assert_eq!(names(&view.people), ["a", "b", "c", "d", "e"]);
        assert_eq!(view.people_more, 2);
        assert_eq!(ids(&view.channels), ["1", "2", "3", "4", "5"]);
        assert_eq!(view.channels_more, 1);
    }

    // ACT-008
    #[test]
    fn what_the_server_found_beyond_what_it_sent_counts_as_more() {
        let found = Directory {
            people_total: 40,
            channels_total: 12,
            ..result(&["a", "b", "c"], &["1", "2"])
        };

        let view = directory_view(&found, &[], &[], "me");

        assert_eq!(view.people_more, 37);
        assert_eq!(view.channels_more, 10);
    }

    // ACT-008
    #[test]
    fn filtered_entries_make_room_for_the_next_ones() {
        let found = result(
            &["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"],
            &["1", "2", "3", "4", "5", "6", "7", "8", "9", "10"],
        );
        let rooms = [
            room("1", RoomKind::Channel, None),
            room("2", RoomKind::Channel, None),
            room("d1", RoomKind::Direct, Some("a")),
            room("d2", RoomKind::Direct, Some("b")),
            room("d3", RoomKind::Direct, Some("c")),
        ];

        let view = directory_view(&found, &rooms, &[], "me");

        assert_eq!(names(&view.people), ["d", "e", "f", "g", "h"]);
        assert_eq!(view.people_more, 2);
        assert_eq!(ids(&view.channels), ["3", "4", "5", "6", "7"]);
        assert_eq!(view.channels_more, 3);
    }

    // ACT-008
    #[test]
    fn a_search_that_has_no_results_shows_nothing() {
        let searching = Directory {
            state: DirectoryState::Searching,
            ..Directory::cleared()
        };

        assert_eq!(
            directory_view(&searching, &[], &[], "me"),
            DirectoryView::default()
        );
    }
}
