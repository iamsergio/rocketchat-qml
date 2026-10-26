// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! What the room list's search shows of the server's directory: the people and the channels it
//! found that the user has no room for already. Plain Rust, so each frontend shows the same.

use rc_core::directory::{
    Directory, DirectoryAction, DirectoryChannel, DirectoryError, DirectoryState, Person,
};
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

/// The groups of the search results, as the labels over them name them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    /// The public channels the user is not in.
    Channels,
    /// The people the user has no direct message with.
    People,
    /// The user's own rooms.
    Rooms,
}

impl Group {
    /// How a frontend names the group in the element names and the roles.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Channels => "channels",
            Self::People => "people",
            Self::Rooms => "rooms",
        }
    }
}

/// One row of the room search's results.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SearchRow {
    /// The label over a group.
    Label(Group),
    /// The directory has not answered yet.
    Searching,
    /// The directory could not be searched; the text says why. Retrying searches again.
    Failed(String),
    Person(Person),
    Channel(DirectoryChannel),
    /// "N more, keep typing", under the last row of a group.
    More(Group, u32),
    /// One of the user's own rooms.
    Room(Room),
}

impl SearchRow {
    /// Whether the cursor can be on the row, and Enter or a click does something with it.
    #[must_use]
    pub const fn actionable(&self) -> bool {
        matches!(self, Self::Person(_) | Self::Channel(_) | Self::Room(_))
    }

    /// Whether `action`, under way, is about this row: it shows "Opening…" meanwhile.
    #[must_use]
    pub fn is_busy_with(&self, action: &DirectoryAction) -> bool {
        match (action, self) {
            (DirectoryAction::OpenDirect(username), Self::Person(person)) => {
                person.username == *username
            }
            (DirectoryAction::Preview(id) | DirectoryAction::Join(id), Self::Channel(channel)) => {
                channel.id == *id
            }
            _ => false,
        }
    }

    /// What identifies the row across searches, so that the cursor can stay on it when the
    /// results change under it.
    #[must_use]
    pub fn key(&self) -> String {
        match self {
            Self::Label(group) => format!("label:{}", group.name()),
            Self::Searching => "searching".to_owned(),
            Self::Failed(_) => "failed".to_owned(),
            Self::Person(person) => format!("person:{}", person.username),
            Self::Channel(channel) => format!("channel:{}", channel.id),
            Self::More(group, _) => format!("more:{}", group.name()),
            Self::Room(room) => format!("room:{}", room.id),
        }
    }
}

/// The text of the row that says the directory could not be searched.
#[must_use]
pub fn failed_text(error: &DirectoryError) -> String {
    match error {
        DirectoryError::Unreachable => "Can't reach the server's directory.".to_owned(),
        DirectoryError::Server(_) => error.to_string(),
    }
}

/// The rows the room search shows for `query` (trimmed), top to bottom as on screen: the channels,
/// the people, then the user's own rooms, `rooms`, each group with its best match at the bottom,
/// nearest the search field. `view` is what [`directory_view`] made of `directory`. A directory
/// that has not answered for this query (it holds another one: the first event of the search has
/// not come yet) counts as searching, and its results are not shown.
///
/// The label over the own rooms shows when the directory has something above them or is not
/// done, so that with no directory results the rooms look as they always did.
#[must_use]
pub fn search_rows(
    query: &str,
    rooms: &[Room],
    directory: &Directory,
    view: &DirectoryView,
) -> Vec<SearchRow> {
    let state = if directory.query == query {
        &directory.state
    } else {
        &DirectoryState::Searching
    };
    let mut rows = Vec::new();
    let mut found = false;
    if *state == DirectoryState::Ready {
        let groups = [
            (Group::Channels, view.channels_more, view.channels.len()),
            (Group::People, view.people_more, view.people.len()),
        ];
        for (group, more, shown) in groups {
            if shown == 0 {
                continue;
            }
            found = true;
            rows.push(SearchRow::Label(group));
            match group {
                Group::Channels => {
                    rows.extend(view.channels.iter().rev().cloned().map(SearchRow::Channel));
                }
                _ => rows.extend(view.people.iter().rev().cloned().map(SearchRow::Person)),
            }
            if more > 0 {
                rows.push(SearchRow::More(group, more));
            }
        }
    }
    match state {
        DirectoryState::Searching => rows.push(SearchRow::Searching),
        DirectoryState::Failed(error) => rows.push(SearchRow::Failed(failed_text(error))),
        DirectoryState::Ready => {}
    }
    if found || *state != DirectoryState::Ready {
        rows.push(SearchRow::Label(Group::Rooms));
    }
    rows.extend(rooms.iter().rev().cloned().map(SearchRow::Room));
    rows
}

/// The first row of `rows` the cursor can be on, counting from index 0.
#[must_use]
pub fn first_actionable(rows: &[SearchRow]) -> Option<usize> {
    rows.iter().position(SearchRow::actionable)
}

/// The next row after `from` that the cursor can be on, going up the indices or down. `None` if
/// there is none, so that the cursor stays.
#[must_use]
pub fn next_actionable(rows: &[SearchRow], from: usize, up: bool) -> Option<usize> {
    if up {
        (from + 1..rows.len()).find(|at| rows[*at].actionable())
    } else {
        (0..from).rev().find(|at| rows[*at].actionable())
    }
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

    fn kinds(rows: &[SearchRow]) -> Vec<String> {
        rows.iter().map(SearchRow::key).collect()
    }

    fn rooms_view(found: &Directory) -> DirectoryView {
        directory_view(found, &[], &[], "me")
    }

    // ACT-007
    #[test]
    fn the_rows_read_channels_people_rooms_with_the_best_match_at_the_bottom() {
        let found = result(&["bob", "carol"], &["c1", "c2"]);
        let own = [
            room("r1", RoomKind::Channel, None),
            room("r2", RoomKind::Channel, None),
        ];

        let rows = search_rows("q", &own, &found, &rooms_view(&found));

        assert_eq!(
            kinds(&rows),
            [
                "label:channels",
                "channel:c2",
                "channel:c1",
                "label:people",
                "person:carol",
                "person:bob",
                "label:rooms",
                "room:r2",
                "room:r1"
            ]
        );
    }

    // ACT-007
    #[test]
    fn without_directory_results_the_rooms_show_as_they_always_did() {
        let found = result(&[], &[]);
        let own = [room("r1", RoomKind::Channel, None)];

        let rows = search_rows("q", &own, &found, &rooms_view(&found));

        assert_eq!(kinds(&rows), ["room:r1"]);
        assert!(search_rows("q", &[], &found, &rooms_view(&found)).is_empty());
    }

    // ACT-007
    #[test]
    fn the_more_line_is_under_the_rows_of_its_group() {
        let found = Directory {
            people_total: 9,
            ..result(&["a", "b"], &[])
        };

        let rows = search_rows("q", &[], &found, &rooms_view(&found));

        assert_eq!(
            kinds(&rows),
            [
                "label:people",
                "person:b",
                "person:a",
                "more:people",
                "label:rooms"
            ]
        );
        assert_eq!(rows[3], SearchRow::More(Group::People, 7));
    }

    // ACT-007
    #[test]
    fn a_search_that_is_not_answered_shows_the_searching_row_over_the_rooms() {
        let own = [room("r1", RoomKind::Channel, None)];
        let searching = Directory::searching("q");
        // The directory still holds another query: it is not answered for this one, and its
        // results are not shown.
        let stale = Directory {
            query: "old".to_owned(),
            ..result(&["bob"], &["c1"])
        };

        for found in [&searching, &stale] {
            let rows = search_rows("q", &own, found, &rooms_view(found));
            assert_eq!(kinds(&rows), ["searching", "label:rooms", "room:r1"]);
        }
    }

    // ACT-007
    #[test]
    fn a_failed_search_says_why_over_the_rooms() {
        let failed = Directory {
            state: DirectoryState::Failed(DirectoryError::Unreachable),
            ..Directory::searching("q")
        };

        let rows = search_rows("q", &[], &failed, &rooms_view(&failed));

        assert_eq!(
            rows[0],
            SearchRow::Failed("Can't reach the server's directory.".to_owned())
        );
        assert_eq!(kinds(&rows), ["failed", "label:rooms"]);
    }

    // ACT-007
    #[test]
    fn the_cursor_skips_the_rows_it_cannot_act_on() {
        let found = result(&["bob"], &[]);
        let own = [room("r1", RoomKind::Channel, None)];
        // Top to bottom: label, bob, label, r1. Indices count from the bottom.
        let mut rows = search_rows("q", &own, &found, &rooms_view(&found));
        rows.reverse();

        assert_eq!(first_actionable(&rows), Some(0));
        assert_eq!(next_actionable(&rows, 0, true), Some(2));
        assert_eq!(next_actionable(&rows, 2, true), None);
        assert_eq!(next_actionable(&rows, 2, false), Some(0));
        assert_eq!(next_actionable(&rows, 0, false), None);
    }

    // ACT-007
    #[test]
    fn an_action_under_way_is_about_its_own_row() {
        let bob = SearchRow::Person(person("bob"));
        let chat = SearchRow::Channel(channel("c1"));
        let open_bob = DirectoryAction::OpenDirect("bob".to_owned());
        let preview = DirectoryAction::Preview("c1".to_owned().into());

        assert!(bob.is_busy_with(&open_bob));
        assert!(!bob.is_busy_with(&DirectoryAction::OpenDirect("carol".to_owned())));
        assert!(chat.is_busy_with(&preview));
        assert!(chat.is_busy_with(&DirectoryAction::Join("c1".to_owned().into())));
        assert!(!chat.is_busy_with(&open_bob));
        assert!(!bob.is_busy_with(&preview));
    }
}
