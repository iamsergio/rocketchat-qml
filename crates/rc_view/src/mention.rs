// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The composer's mention autocomplete (see `docs/design/mentions.md`): the `@word` at the cursor,
//! the rows of the popup, and what inserting a row does to the text. Plain Rust, so each frontend
//! shows the same.
//!
//! Positions in the text are in UTF-16 code units, which is what Qt's cursor and selection use. A
//! frontend with another unit converts at its edge. A position inside a surrogate pair counts as
//! the start of that character; one past the end counts as the end.

use std::ops::Range;

use rc_core::domain::{AvatarOwner, RoomKind};
use rc_core::members::{MEMBERS_PER_SEARCH, Member, MemberSearch, MemberSearchState};

/// How many people the popup lists; the rest are counted in the "more" row.
pub const SHOWN: usize = MEMBERS_PER_SEARCH;

/// What the popup says when the members couldn't be searched, whatever the reason.
pub const FAILED_TEXT: &str = "Can't search the members: the server didn't answer.";

/// What `@all` and `@here` do, as the popup describes them.
const ALL_TEXT: &str = "Notify everyone in this room";
const HERE_TEXT: &str = "Notify the people who are online";

/// The `@word` the cursor is in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MentionWord {
    /// Where the `@` is, in UTF-16 units.
    pub start: usize,
    /// What was typed after the `@`, up to the cursor: not the rest of the word.
    pub query: String,
}

/// The `@word` at `cursor` (UTF-16 units), if the popup is to be open: the word the cursor is in
/// or at the end of starts with `@`, at the start of the text or after whitespace, and holds no
/// other `@` before the cursor. A space, or leaving the word, gives `None`.
#[must_use]
pub fn word_at_cursor(text: &str, cursor: usize) -> Option<MentionWord> {
    let cursor = byte_of(text, cursor);
    let before = &text[..cursor];
    let start = before
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_whitespace())
        .map_or(0, |(i, c)| i + c.len_utf8());
    let query = before[start..].strip_prefix('@')?;
    if query.contains('@') {
        return None;
    }
    Some(MentionWord {
        start: units_of(&text[..start]),
        query: query.to_owned(),
    })
}

/// The text with the word that starts at `start` (the `@`, from [`word_at_cursor`]) replaced by
/// `@username` and a space, and where the cursor goes: after the space. The word is replaced
/// whole, including what follows the cursor up to the next whitespace.
#[must_use]
pub fn insert(text: &str, cursor: usize, start: usize, username: &str) -> (String, usize) {
    let start = byte_of(text, start);
    let cursor = byte_of(text, cursor).max(start);
    let end = text[cursor..]
        .char_indices()
        .find(|(_, c)| c.is_whitespace())
        .map_or(text.len(), |(i, _)| cursor + i);
    let mention = format!("@{username} ");
    let new_cursor = units_of(&text[..start]) + units_of(&mention);
    (
        format!("{}{mention}{}", &text[..start], &text[end..]),
        new_cursor,
    )
}

/// The byte offset of the UTF-16 position `units`.
fn byte_of(text: &str, units: usize) -> usize {
    let mut seen = 0;
    for (i, c) in text.char_indices() {
        if seen + c.len_utf16() > units {
            return i;
        }
        seen += c.len_utf16();
    }
    text.len()
}

/// How many UTF-16 units `text` has.
fn units_of(text: &str) -> usize {
    text.chars().map(char::len_utf16).sum()
}

/// A person the popup suggests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersonRow {
    pub username: String,
    /// The display name, if the person has one.
    pub name: Option<String>,
    /// Whose avatar to show; the core fetches it when it finds the person.
    pub avatar: AvatarOwner,
    /// Where the typed text is in the name, as a byte range, to highlight.
    pub name_match: Option<Range<usize>>,
    /// Where the typed text is in the username, as a byte range, to highlight.
    pub username_match: Option<Range<usize>>,
}

/// `@all` or `@here`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpecialRow {
    /// `all` or `here`, without the `@`.
    pub username: &'static str,
    pub description: &'static str,
    /// Where the typed text is in the name, as a byte range, to highlight.
    pub username_match: Option<Range<usize>>,
}

/// A row of the popup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Row {
    Person(PersonRow),
    Special(SpecialRow),
    /// Not a choice: this many more people match, so the user keeps typing. Never first.
    More(u32),
}

impl Row {
    /// The username to insert, `None` for a row that is not a choice.
    #[must_use]
    pub fn username(&self) -> Option<&str> {
        match self {
            Self::Person(person) => Some(&person.username),
            Self::Special(special) => Some(special.username),
            Self::More(_) => None,
        }
    }
}

/// What the popup says besides its rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// Nothing to say.
    Idle,
    /// A search runs and there is nothing to show yet: the popup spins.
    Searching,
    /// The last search failed: the popup says [`FAILED_TEXT`], above the rows it still has.
    Failed,
}

/// The popup as it is shown now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Popup {
    pub rows: Vec<Row>,
    pub status: Status,
}

/// The answers of the member search the popup lives on. The previous answer stays while a new
/// search runs, so that the popup doesn't flicker while the user types.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Suggestions {
    room: Option<rc_core::domain::RoomId>,
    members: Vec<Member>,
    total: u32,
    searching: bool,
    failed: bool,
    /// The query the core last said something about (a search started, answered or failed).
    query: Option<String>,
}

impl Suggestions {
    /// No answer yet.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            room: None,
            members: Vec::new(),
            total: 0,
            searching: false,
            failed: false,
            query: None,
        }
    }

    /// Takes what the core says of the member search (`Event::MemberSearch`): `None` forgets
    /// everything, as does a search of another room.
    pub fn apply(&mut self, search: Option<&MemberSearch>) {
        let Some(search) = search else {
            *self = Self::default();
            return;
        };
        if self.room.as_ref() != Some(&search.room) {
            *self = Self {
                room: Some(search.room.clone()),
                ..Self::default()
            };
        }
        (self.searching, self.failed) = (false, false);
        self.query = Some(search.query.clone());
        match &search.state {
            MemberSearchState::Searching => self.searching = true,
            MemberSearchState::Found { members, total } => {
                self.members.clone_from(members);
                self.total = *total;
            }
            MemberSearchState::Failed(_) => self.failed = true,
        }
    }

    /// The popup for what is typed after the `@` now, in a room of `kind`: the people of the last
    /// answer that still match `query`, which may have moved on since, at most [`SHOWN`], the
    /// "more" row if the answer had others (the last answer's count: its total minus the people
    /// it showed, whether or not `query` still matches them), then `@all` and `@here` if they start with `query`
    /// (not in a direct message).
    #[must_use]
    pub fn popup(&self, kind: RoomKind, query: &str) -> Popup {
        // The people of an answer to a shorter query that the longer one rules out are gone at
        // once, not when the next answer comes.
        let people = (self.members.iter().take(SHOWN)).filter(|member| is_match(member, query));
        let mut rows: Vec<Row> = people
            .map(|member| Row::Person(person_row(member, query)))
            .collect();
        let more = usize::try_from(self.total)
            .unwrap_or(usize::MAX)
            .saturating_sub(self.members.len().min(SHOWN));
        if more > 0 {
            rows.push(Row::More(u32::try_from(more).unwrap_or(u32::MAX)));
        }
        if kind != RoomKind::Direct {
            for (username, description) in [("all", ALL_TEXT), ("here", HERE_TEXT)] {
                let range = match_range(username, query);
                if query.is_empty() || range.as_ref().is_some_and(|r| r.start == 0) {
                    rows.push(Row::Special(SpecialRow {
                        username,
                        description,
                        username_match: range,
                    }));
                }
            }
        }
        let status = if self.failed {
            Status::Failed
        } else if rows.is_empty() && (self.searching || self.query.as_deref() != Some(query)) {
            // Also when the core has said nothing yet of this query: its `Searching` is on its
            // way, and the popup must not say that nobody matches meanwhile.
            Status::Searching
        } else {
            Status::Idle
        };
        Popup { rows, status }
    }
}

/// Whether `query` is in the member's username or name, ignoring case; the empty query is in all.
fn is_match(member: &Member, query: &str) -> bool {
    query.is_empty()
        || match_range(&member.username, query).is_some()
        || member
            .name
            .as_deref()
            .is_some_and(|name| match_range(name, query).is_some())
}

fn person_row(member: &Member, query: &str) -> PersonRow {
    PersonRow {
        username: member.username.clone(),
        name: member.name.clone(),
        avatar: AvatarOwner::User(member.username.clone()),
        name_match: member
            .name
            .as_deref()
            .and_then(|name| match_range(name, query)),
        username_match: match_range(&member.username, query),
    }
}

/// Where `query` is in `text`, ignoring case, as a byte range of `text`; the first place. `None`
/// for an empty `query`: there is nothing to highlight.
fn match_range(text: &str, query: &str) -> Option<Range<usize>> {
    if query.is_empty() {
        return None;
    }
    let wanted: Vec<char> = query.chars().map(fold).collect();
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let folded: Vec<char> = chars.iter().map(|(_, c)| fold(*c)).collect();
    let first = folded.windows(wanted.len()).position(|w| w == wanted)?;
    let last = first + wanted.len() - 1;
    let end = chars[last].0 + chars[last].1.len_utf8();
    Some(chars[first].0..end)
}

/// The lower case of `c`, if it is a single character; else `c`, so that folding keeps one
/// character per character and the positions stay those of the original.
fn fold(c: char) -> char {
    let mut lower = c.to_lowercase();
    match (lower.next(), lower.next()) {
        (Some(single), None) => single,
        _ => c,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rc_core::members::MemberError;

    #[allow(clippy::unnecessary_wraps)] // Compared with what `word_at_cursor` gives.
    fn word(start: usize, query: &str) -> Option<MentionWord> {
        Some(MentionWord {
            start,
            query: query.to_owned(),
        })
    }

    // ACT-013
    #[test]
    fn a_word_that_starts_with_an_at_opens_the_popup_with_what_follows_up_to_the_cursor() {
        assert_eq!(word_at_cursor("@", 1), word(0, ""));
        assert_eq!(word_at_cursor("@ali", 4), word(0, "ali"));
        assert_eq!(word_at_cursor("hi @ali", 7), word(3, "ali"));
        assert_eq!(word_at_cursor("hi\n@ali", 7), word(3, "ali"));
        // The cursor in the middle: the rest of the word is not the query.
        assert_eq!(word_at_cursor("hi @alice", 6), word(3, "al"));
        assert_eq!(word_at_cursor("@alice", 1), word(0, ""));
    }

    // ACT-013
    #[test]
    fn leaving_the_word_or_not_starting_one_closes_the_popup() {
        assert_eq!(word_at_cursor("", 0), None);
        assert_eq!(word_at_cursor("@ali ", 5), None);
        assert_eq!(word_at_cursor("@ali bob", 8), None);
        assert_eq!(word_at_cursor("mail@ali", 8), None);
        assert_eq!(word_at_cursor("@@ali", 5), None);
        assert_eq!(word_at_cursor("@ali@bob", 8), None);
        // Before the `@`.
        assert_eq!(word_at_cursor("hi @ali", 3), None);
        assert_eq!(word_at_cursor("hi @ali", 0), None);
    }

    // ACT-013
    #[test]
    fn positions_are_utf16_units_so_a_wide_character_counts_for_two() {
        // "é" is one unit, the emoji two.
        let text = "é😀 @ali";
        assert_eq!(word_at_cursor(text, 8), word(4, "ali"));
        assert_eq!(word_at_cursor(text, 5), word(4, ""));
        // Inside the emoji's pair is its start.
        assert_eq!(word_at_cursor("😀@a", 1), None);
        // Past the end is the end.
        assert_eq!(word_at_cursor("@ali", 99), word(0, "ali"));
        let accented = "@ñandú";
        assert_eq!(word_at_cursor(accented, 3), word(0, "ña"));
    }

    // ACT-013
    #[test]
    fn inserting_replaces_the_typed_text_and_the_rest_of_the_word_and_adds_a_space() {
        assert_eq!(insert("@ali", 4, 0, "alice"), ("@alice ".to_owned(), 7));
        assert_eq!(
            insert("hi @al there", 6, 3, "alice"),
            ("hi @alice  there".to_owned(), 10)
        );
        // The cursor in the middle of the word: the rest goes too.
        assert_eq!(
            insert("hi @alxyz and", 6, 3, "alice"),
            ("hi @alice  and".to_owned(), 10)
        );
        assert_eq!(insert("@", 1, 0, "all"), ("@all ".to_owned(), 5));
    }

    // ACT-013
    #[test]
    fn inserting_counts_the_cursor_in_utf16_units() {
        // "😀" is two units, so the word starts at 3.
        let (text, cursor) = insert("😀 @ñ", 5, 3, "ñandú");
        assert_eq!(text, "😀 @ñandú ");
        assert_eq!(cursor, 3 + 7);
        let (text, cursor) = insert("é @a", 4, 2, "😀x");
        assert_eq!(text, "é @😀x ");
        assert_eq!(cursor, 2 + 5);
    }

    fn member(username: &str, name: Option<&str>) -> Member {
        Member {
            id: String::new(),
            username: username.to_owned(),
            name: name.map(str::to_owned),
        }
    }

    fn answer(members: Vec<Member>, total: u32) -> MemberSearch {
        MemberSearch {
            room: "r".to_owned().into(),
            query: String::new(),
            state: MemberSearchState::Found { members, total },
        }
    }

    fn with(search: &MemberSearch) -> Suggestions {
        let mut suggestions = Suggestions::default();
        suggestions.apply(Some(search));
        suggestions
    }

    fn usernames(popup: &Popup) -> Vec<String> {
        (popup.rows.iter())
            .map(|row| match row {
                Row::More(n) => format!("+{n}"),
                other => other.username().unwrap_or_default().to_owned(),
            })
            .collect()
    }

    // ACT-013
    #[test]
    fn at_most_six_people_then_the_count_of_the_others_then_all_and_here() {
        let members: Vec<_> = (0..6).map(|i| member(&format!("u{i}"), None)).collect();
        let suggestions = with(&answer(members, 10));

        let popup = suggestions.popup(RoomKind::Channel, "");

        assert_eq!(
            usernames(&popup),
            ["u0", "u1", "u2", "u3", "u4", "u5", "+4", "all", "here"]
        );
        assert_eq!(popup.status, Status::Idle);
    }

    // ACT-013
    #[test]
    fn more_members_than_shown_are_cut_and_counted() {
        let members: Vec<_> = (0..8).map(|i| member(&format!("u{i}"), None)).collect();

        let popup = with(&answer(members, 8)).popup(RoomKind::Private, "u");

        assert_eq!(popup.rows.len(), SHOWN + 1 /* more */);
        assert_eq!(popup.rows[SHOWN], Row::More(2));
    }

    // ACT-013
    #[test]
    fn all_and_here_follow_only_when_they_start_with_the_query_and_not_in_a_direct_message() {
        let suggestions = with(&answer(vec![member("alice", None)], 1));

        assert_eq!(
            usernames(&suggestions.popup(RoomKind::Channel, "a")),
            ["alice", "all"]
        );
        assert_eq!(
            usernames(&suggestions.popup(RoomKind::Private, "HE")),
            ["here"]
        );
        assert_eq!(
            usernames(&suggestions.popup(RoomKind::Channel, "x")),
            [] as [&str; 0]
        );
        // Not a prefix.
        assert_eq!(
            usernames(&suggestions.popup(RoomKind::Channel, "ll")),
            [] as [&str; 0]
        );
        assert_eq!(
            usernames(&suggestions.popup(RoomKind::Direct, "a")),
            ["alice"]
        );
        assert_eq!(
            usernames(&suggestions.popup(RoomKind::Direct, "")),
            ["alice"]
        );
    }

    // ACT-013
    #[test]
    fn people_of_the_last_answer_that_the_query_now_rules_out_are_left_out_but_still_counted() {
        let members = vec![
            member("alice", None),
            member("alan", None),
            member("x", Some("Ali B")),
        ];
        let suggestions = with(&answer(members, 9));

        let popup = suggestions.popup(RoomKind::Direct, "ali");

        assert_eq!(usernames(&popup), ["alice", "x", "+6"]);
        assert_eq!(
            usernames(&suggestions.popup(RoomKind::Direct, "al"))[..2],
            ["alice", "alan"]
        );
    }

    // ACT-013
    #[test]
    fn a_person_row_has_the_avatar_owner_and_the_matched_ranges_in_both_names() {
        let suggestions = with(&answer(vec![member("malice", Some("Alice Doe"))], 1));

        let popup = suggestions.popup(RoomKind::Direct, "LIC");

        let Row::Person(person) = &popup.rows[0] else {
            panic!("{:?}", popup.rows);
        };
        assert_eq!(person.avatar, AvatarOwner::User("malice".to_owned()));
        assert_eq!(person.name.as_deref(), Some("Alice Doe"));
        assert_eq!(person.name_match, Some(1..4));
        assert_eq!(person.username_match, Some(2..5));
    }

    // ACT-013
    #[test]
    fn no_highlight_for_the_empty_query_and_a_name_that_does_not_match_gets_none_of_it() {
        let suggestions = with(&answer(vec![member("alice", Some("Bob"))], 1));

        let Row::Person(person) = &suggestions.popup(RoomKind::Direct, "").rows[0] else {
            panic!();
        };
        assert_eq!((&person.name_match, &person.username_match), (&None, &None));
        let Row::Person(person) = &suggestions.popup(RoomKind::Direct, "lic").rows[0] else {
            panic!();
        };
        assert_eq!(
            (&person.name_match, &person.username_match),
            (&None, &Some(1..4))
        );
    }

    // ACT-013
    #[test]
    fn the_matched_range_is_in_bytes_of_the_original_text_for_non_ascii_names() {
        // "İ" lower-cases to two characters; it stays itself, so the positions hold.
        let suggestions = with(&answer(vec![member("x", Some("Ñandú İvan"))], 1));

        let Row::Person(person) = &suggestions.popup(RoomKind::Direct, "ñand").rows[0] else {
            panic!();
        };
        assert_eq!(person.name_match, Some(0..5));
        let Row::Person(person) = &suggestions.popup(RoomKind::Direct, "van").rows[0] else {
            panic!();
        };
        // "Ñandú " is 8 bytes, "İ" 2.
        assert_eq!(person.name_match, Some(10..13));
    }

    // ACT-013
    #[test]
    fn the_previous_answer_stays_while_a_new_search_runs_and_a_spinner_only_without_rows() {
        let mut suggestions = Suggestions::default();
        suggestions.apply(Some(&MemberSearch {
            state: MemberSearchState::Searching,
            ..answer(vec![], 0)
        }));
        // Nothing yet: not in a direct message, where `@all` would be a row.
        let popup = suggestions.popup(RoomKind::Direct, "a");
        assert_eq!((popup.rows.len(), popup.status), (0, Status::Searching));

        suggestions.apply(Some(&answer(vec![member("alice", None)], 1)));
        suggestions.apply(Some(&MemberSearch {
            state: MemberSearchState::Searching,
            ..answer(vec![], 0)
        }));

        let popup = suggestions.popup(RoomKind::Direct, "al");
        assert_eq!(usernames(&popup), ["alice"]);
        assert_eq!(popup.status, Status::Idle);
    }

    // ACT-013
    #[test]
    fn a_query_the_core_has_not_spoken_of_yet_spins_instead_of_saying_nobody_matches() {
        let mut suggestions = with(&MemberSearch {
            query: "bo".to_owned(),
            ..answer(vec![], 0)
        });

        // Answered: nobody matches.
        assert_eq!(
            suggestions.popup(RoomKind::Direct, "bo").status,
            Status::Idle
        );
        // Typed on: the search for "bob" has not been announced yet.
        assert_eq!(
            suggestions.popup(RoomKind::Direct, "bob").status,
            Status::Searching
        );
        suggestions.apply(Some(&MemberSearch {
            query: "bob".to_owned(),
            ..answer(vec![], 0)
        }));
        assert_eq!(
            suggestions.popup(RoomKind::Direct, "bob").status,
            Status::Idle
        );
        // Nothing said at all yet.
        assert_eq!(
            Suggestions::default().popup(RoomKind::Direct, "a").status,
            Status::Searching
        );
    }

    // ACT-013
    #[test]
    fn a_failure_is_a_status_above_the_rows_it_keeps_and_a_new_search_clears_it() {
        let mut suggestions = with(&answer(vec![member("alice", None)], 1));
        let failed = MemberSearch {
            state: MemberSearchState::Failed(MemberError::Unreachable),
            ..answer(vec![], 0)
        };

        suggestions.apply(Some(&failed));

        let popup = suggestions.popup(RoomKind::Direct, "a");
        assert_eq!(
            (usernames(&popup), popup.status),
            (vec!["alice".to_owned()], Status::Failed)
        );
        suggestions.apply(Some(&MemberSearch {
            state: MemberSearchState::Searching,
            ..answer(vec![], 0)
        }));
        assert_eq!(
            suggestions.popup(RoomKind::Direct, "a").status,
            Status::Idle
        );
    }

    // ACT-013
    #[test]
    fn closing_or_another_room_forgets_the_answers() {
        let mut suggestions = with(&answer(vec![member("alice", None)], 1));

        suggestions.apply(None);

        assert_eq!(suggestions, Suggestions::default());
        let mut suggestions = with(&answer(vec![member("alice", None)], 1));
        suggestions.apply(Some(&MemberSearch {
            room: "other".to_owned().into(),
            state: MemberSearchState::Searching,
            ..answer(vec![], 0)
        }));
        assert_eq!(suggestions.popup(RoomKind::Direct, "").rows, []);
    }
}
