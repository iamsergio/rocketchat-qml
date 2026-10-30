// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! What the search pane of a room shows (see `docs/design/message-search.md`): the rows of the
//! results grouped by day, the words of the query marked in them, the note the list area shows
//! when it has no rows, the footer, and the cursor. Plain Rust, so each frontend shows the same.
//! The time zone and the current time are parameters, so tests don't depend on where or when they
//! run.

use std::ops::Range;

use jiff::Timestamp;
use jiff::tz::TimeZone;
use rc_core::domain::{AvatarOwner, Message, MessageId, MessageKind, RoomId};
use rc_core::message_search::{MessageSearch, MessageSearchError, MessageSearchState, More};
use rc_core::timeline::{Found, FoundState};

use crate::avatar_view::message_owner;
use crate::message_view::{author_name, body, day_label, first_of_day, system_text, time_text};

/// The most characters of a message a row is given: the row clamps it to three lines.
const TEXT_LIMIT: usize = 240;

/// What the note says while the field is empty.
pub const EMPTY_HINT: &str =
    "Type a word to find the messages that hold it. The server matches whole words, in any case.";

/// What the note says when nothing matches.
pub const NONE_HINT: &str = "The search matches whole words: try the full word, or another one.";

/// The hint under the field at rest, and while a search runs with rows on show.
pub const FIELD_HINT: &str = "Whole words, any case";
pub const FIELD_HINT_SEARCHING: &str = "Searching…";

/// What the pane says when the messages couldn't be searched because the server didn't answer.
pub const FAILED_TEXT: &str = "Can't search the messages: the server didn't answer.";

/// The text to show for a failure: the server's own words for an error it reported.
#[must_use]
pub fn error_text(error: &MessageSearchError) -> String {
    match error {
        MessageSearchError::Unreachable => FAILED_TEXT.to_owned(),
        MessageSearchError::Server(_) => error.to_string(),
    }
}

/// What the pane says under the field when the message chosen is gone.
pub const MISSING_NOTE: &str = "This message no longer exists.";

/// The note under the field for the message the room was asked to show: empty unless the server
/// has no such message.
#[must_use]
pub fn missing_note(found: Option<&Found>) -> &'static str {
    match found {
        Some(Found {
            state: FoundState::Missing,
            ..
        }) => MISSING_NOTE,
        _ => "",
    }
}

/// How the row of message `id` stands with the message the room was asked to show: `picked` while
/// it is the one being or already shown, and `opening` while the room is still getting it.
#[must_use]
pub fn pick_state(found: Option<&Found>, id: &MessageId) -> (bool, bool) {
    match found {
        Some(found) if found.message == *id => match found.state {
            FoundState::Loading => (true, true),
            FoundState::Shown => (true, false),
            FoundState::Missing => (false, false),
        },
        _ => (false, false),
    }
}

/// What choosing a result does: the message the room shows, and the thread to open over the
/// results, if the result is a reply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PickTarget {
    pub show: MessageId,
    pub thread: Option<MessageId>,
}

/// What to show for `message` found by the search. A reply the room doesn't show has no place
/// there, so the room shows the thread's parent; a reply that was also sent to the room shows
/// itself. Either way its thread opens (it doesn't scroll to the reply: the server can't load
/// the replies around one).
#[must_use]
pub fn pick_target(message: &Message) -> PickTarget {
    let show = match &message.thread_id {
        Some(parent) if !message.shows_in_room() => parent.clone(),
        _ => message.id.clone(),
    };
    PickTarget {
        show,
        thread: message.thread_id.clone(),
    }
}

/// A message found, as its row shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageRow {
    pub id: MessageId,
    pub author: String,
    /// The username, for the colour of the avatar.
    pub username: String,
    /// Whose avatar to show; none for a system message.
    pub avatar: Option<AvatarOwner>,
    /// `HH:MM`.
    pub time: String,
    /// The start of the text, escaped, with `<b>` around the words of the query.
    pub text: String,
    /// The message is a reply in a thread.
    pub in_thread: bool,
    /// What choosing the row shows and opens.
    pub target: PickTarget,
}

/// A row of the results.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Row {
    /// The label of a day: "Today", "Yesterday" or the date.
    Day(String),
    Message(MessageRow),
}

/// What the list area is about.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Status {
    /// The field is empty: the note says what to do.
    #[default]
    Empty,
    /// The first search runs and there are no rows to show meanwhile: a spinner.
    Searching,
    /// The answer holds no message: the note says so.
    None,
    /// The first page couldn't be fetched.
    Failed,
    /// Rows, which may be those of the last answer while a new search runs.
    Results,
}

impl Status {
    /// The name the GUI knows it by.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::Searching => "searching",
            Self::None => "none",
            Self::Failed => "failed",
            Self::Results => "results",
        }
    }
}

/// What is under the last row.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Footer {
    #[default]
    Nothing,
    /// There is a next page: reaching the end asks for it.
    Available,
    /// The next page is on its way: a spinner.
    Loading,
    /// The next page couldn't be fetched, with why.
    Failed(String),
}

impl Footer {
    /// The name the GUI knows it by.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::Nothing => "nothing",
            Self::Available => "available",
            Self::Loading => "loading",
            Self::Failed(_) => "failed",
        }
    }
}

/// The pane as it is shown now.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct View {
    /// The core has a search: the pane is open.
    pub open: bool,
    pub status: Status,
    pub rows: Vec<Row>,
    pub footer: Footer,
    /// The title and the hint of the note of an empty field or of no result; empty otherwise.
    pub note_title: String,
    pub note_hint: String,
    /// Why the first page failed; empty if it didn't.
    pub error: String,
    /// The hint under the field.
    pub field_hint: String,
    /// A search runs while the rows of the last answer are on show.
    pub refreshing: bool,
    /// The name of the room searched, for the header.
    pub room_name: String,
}

/// The answers of the message search the pane lives on. The previous answer stays while a new
/// search runs, so that the list doesn't blink while the user types.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Results {
    room: Option<RoomId>,
    /// The core has a search.
    open: bool,
    /// The query of the answer `messages` came with: the words to mark.
    found_query: String,
    messages: Vec<Message>,
    more: Option<More>,
    /// What the core last said.
    searching: bool,
    failed: Option<MessageSearchError>,
    /// The query the core last said something about.
    query: String,
}

impl Results {
    /// No answer yet.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            room: None,
            open: false,
            found_query: String::new(),
            messages: Vec::new(),
            more: None,
            searching: false,
            failed: None,
            query: String::new(),
        }
    }

    /// Takes what the core says of the message search (`Event::MessageSearch`): `None` forgets
    /// everything, as does a search of another room.
    pub fn apply(&mut self, search: Option<&MessageSearch>) {
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
        self.open = true;
        self.query.clone_from(&search.query);
        (self.searching, self.failed) = (false, None);
        match &search.state {
            MessageSearchState::Searching => self.searching = true,
            MessageSearchState::Found { messages, more } => {
                self.messages.clone_from(messages);
                self.more = Some(more.clone());
                self.found_query.clone_from(&search.query);
            }
            MessageSearchState::Failed(error) => {
                // Rows of another query would mislead.
                self.messages.clear();
                self.more = None;
                self.found_query.clear();
                self.failed = Some(error.clone());
            }
        }
    }

    /// The room the core searches, while it does: the pane is open.
    #[must_use]
    pub fn room(&self) -> Option<&RoomId> {
        self.room.as_ref()
    }

    /// The pane for the room called `room_name`.
    #[must_use]
    pub fn view(&self, room_name: &str, zone: &TimeZone, now: Timestamp) -> View {
        if !self.open {
            return View::default();
        }
        let rows = rows(&self.messages, &self.found_query, zone, now);
        let has_rows = !rows.is_empty();
        let status = if self.failed.is_some() {
            Status::Failed
        } else if has_rows {
            Status::Results
        } else if self.searching {
            Status::Searching
        } else if self.query.is_empty() {
            Status::Empty
        } else {
            Status::None
        };
        let refreshing = self.searching && has_rows;
        let footer = match (&self.more, self.searching) {
            (Some(More::Available), false) => Footer::Available,
            (Some(More::Loading), false) => Footer::Loading,
            (Some(More::Failed(error)), false) => Footer::Failed(error_text(error)),
            _ => Footer::Nothing,
        };
        let (note_title, note_hint) = match status {
            Status::Empty => (format!("Search #{room_name}"), EMPTY_HINT.to_owned()),
            Status::None => (
                format!("No messages with “{}”", self.query),
                NONE_HINT.to_owned(),
            ),
            _ => (String::new(), String::new()),
        };
        View {
            open: true,
            status,
            rows,
            footer,
            note_title,
            note_hint,
            error: self.failed.as_ref().map(error_text).unwrap_or_default(),
            field_hint: if refreshing {
                FIELD_HINT_SEARCHING
            } else {
                FIELD_HINT
            }
            .to_owned(),
            refreshing,
            room_name: room_name.to_owned(),
        }
    }
}

/// The room the search must move to: the core's search (of the room `searched`) is of another
/// room than the open one.
/// `None` if there is no search, no room is open or they are the same.
#[must_use]
pub fn stale_room(searched: Option<&RoomId>, open: Option<&RoomId>) -> Option<RoomId> {
    let (searched, open) = searched.zip(open)?;
    (searched != open).then(|| open.clone())
}

/// The rows of `messages` (newest first, as the server sends them), a label before the first of
/// each day, with the words of `query` marked.
#[must_use]
pub fn rows(messages: &[Message], query: &str, zone: &TimeZone, now: Timestamp) -> Vec<Row> {
    let words = query_words(query);
    let mut rows = Vec::new();
    let mut previous: Option<&Message> = None;
    for message in messages {
        if first_of_day(previous, message, zone) {
            rows.push(Row::Day(day_label(message.ts, zone, now)));
        }
        previous = Some(message);
        let text = row_text(message);
        let ranges = highlights(&text, &words);
        rows.push(Row::Message(MessageRow {
            id: message.id.clone(),
            author: author_name(&message.author).to_owned(),
            username: message.author.username.clone(),
            avatar: message_owner(message),
            time: time_text(message.ts, zone),
            text: styled(&text, &ranges),
            in_thread: message.thread_id.is_some(),
            target: pick_target(message),
        }));
    }
    rows
}

/// The start of what the message says on one line: the text, or what the room says for a system
/// message.
fn row_text(message: &Message) -> String {
    let text = match message.kind {
        MessageKind::Text(_) => body(message).to_owned(),
        MessageKind::System(_) => system_text(message),
    };
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    line.chars().take(TEXT_LIMIT).collect()
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// A `/regex/` query, which the server matches as a substring: nothing to mark.
fn is_regex(query: &str) -> bool {
    query.starts_with('/') && query.rfind('/').is_some_and(|last| last > 0)
}

/// The words of `query` the server looks for, in lower case: none for a regular expression, and
/// not those it leaves out (`-word`).
#[must_use]
pub fn query_words(query: &str) -> Vec<String> {
    if is_regex(query) {
        return Vec::new();
    }
    query
        .split_whitespace()
        .filter(|word| !word.starts_with('-'))
        .flat_map(|word| word.split(|c: char| !is_word_char(c)))
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// Where the whole words of `text` that are one of `words` (lower case) are, as byte ranges.
fn highlights(text: &str, words: &[String]) -> Vec<Range<usize>> {
    if words.is_empty() {
        return Vec::new();
    }
    let mut ranges = Vec::new();
    let mut start: Option<usize> = None;
    let close = |from: usize, to: usize, ranges: &mut Vec<Range<usize>>| {
        if words.contains(&text[from..to].to_lowercase()) {
            ranges.push(from..to);
        }
    };
    for (at, c) in text.char_indices() {
        match (is_word_char(c), start) {
            (true, None) => start = Some(at),
            (false, Some(from)) => {
                close(from, at, &mut ranges);
                start = None;
            }
            _ => {}
        }
    }
    if let Some(from) = start {
        close(from, text.len(), &mut ranges);
    }
    ranges
}

/// `text` for a styled label: escaped, with `<b>` around each of `ranges` (byte ranges of `text`,
/// in order, not overlapping).
fn styled(text: &str, ranges: &[Range<usize>]) -> String {
    let escape = |part: &str| {
        part.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    };
    let mut out = String::new();
    let mut at = 0;
    for range in ranges {
        out.push_str(&escape(&text[at..range.start]));
        out.push_str("<b>");
        out.push_str(&escape(&text[range.clone()]));
        out.push_str("</b>");
        at = range.end;
    }
    out.push_str(&escape(&text[at..]));
    out
}

/// Whether the row at `at` is a message, i.e. a place for the cursor.
fn is_message(rows: &[Row], at: usize) -> bool {
    matches!(rows.get(at), Some(Row::Message(_)))
}

/// The first row that is a message.
#[must_use]
pub fn first_cursor(rows: &[Row]) -> Option<usize> {
    (0..rows.len()).find(|at| is_message(rows, *at))
}

/// Where the cursor goes from `cursor` (none yet: -1 for the model) one message down, or up, the
/// day labels skipped, wrapping.
#[must_use]
pub fn step_cursor(rows: &[Row], cursor: Option<usize>, down: bool) -> Option<usize> {
    let count = rows.len();
    if count == 0 {
        return None;
    }
    // Starting past the end (or before the start) makes the first step land on the first (or
    // last) row.
    let start = cursor.unwrap_or(if down { count - 1 } else { 0 });
    (1..=count)
        .map(|step| {
            if down {
                (start + step) % count
            } else {
                (start + count - step % count) % count
            }
        })
        .find(|at| is_message(rows, *at))
}

/// What choosing the row `at` does, if it is a message.
#[must_use]
pub fn target_at(rows: &[Row], at: usize) -> Option<&PickTarget> {
    match rows.get(at) {
        Some(Row::Message(row)) => Some(&row.target),
        _ => None,
    }
}

/// The id of the message at row `at`, if it is one.
#[must_use]
pub fn message_at(rows: &[Row], at: usize) -> Option<&MessageId> {
    match rows.get(at) {
        Some(Row::Message(row)) => Some(&row.id),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use rc_core::domain::{Author, Delivery, SystemEvent};

    use super::*;

    fn message(id: &str, ts: &str, text: &str) -> Message {
        Message {
            id: id.to_owned().into(),
            room_id: "r".to_owned().into(),
            author: Author {
                id: "u".to_owned().into(),
                username: "alice".to_owned(),
                display_name: Some("Alice A".to_owned()),
            },
            ts: ts.parse().unwrap(),
            kind: MessageKind::Text(text.into()),
            delivery: Delivery::Sent,
            attachments: Vec::new(),
            quotes: Vec::new(),
            edited: false,
            reactions: Vec::new(),
            thread_id: None,
            thread_shown: false,
            thread_replies: 0,
            thread_last_reply: None,
            thread_followers: Vec::new(),
        }
    }

    fn search(query: &str, state: MessageSearchState) -> MessageSearch {
        MessageSearch {
            room: "r".to_owned().into(),
            query: query.to_owned(),
            state,
        }
    }

    fn found(query: &str, messages: Vec<Message>, more: More) -> MessageSearch {
        search(query, MessageSearchState::Found { messages, more })
    }

    fn now() -> Timestamp {
        "2026-10-20T12:00:00Z".parse().unwrap()
    }

    fn view(results: &Results) -> View {
        results.view("general", &TimeZone::UTC, now())
    }

    fn text_of(row: &Row) -> &str {
        match row {
            Row::Message(row) => &row.text,
            Row::Day(label) => label,
        }
    }

    // MSG-008
    #[test]
    fn the_rows_have_a_label_before_the_first_message_of_each_day() {
        let messages = [
            message("a", "2026-10-20T10:00:00Z", "one"),
            message("b", "2026-10-20T09:30:00Z", "two"),
            message("c", "2026-10-19T09:00:00Z", "three"),
            message("d", "2026-10-15T09:00:00Z", "four"),
        ];
        let rows = rows(&messages, "", &TimeZone::UTC, now());
        let texts: Vec<&str> = rows.iter().map(text_of).collect();
        assert_eq!(
            texts,
            [
                "Today",
                "one",
                "two",
                "Yesterday",
                "three",
                "Thursday, 15 October 2026",
                "four"
            ]
        );
        let Row::Message(first) = &rows[1] else {
            panic!("not a message");
        };
        assert_eq!(
            (first.author.as_str(), first.time.as_str()),
            ("Alice A", "10:00")
        );
        assert!(!first.in_thread);
    }

    // MSG-008
    #[test]
    fn whole_words_are_marked_ignoring_case_and_the_text_is_escaped() {
        let messages = [message(
            "a",
            "2026-10-20T10:00:00Z",
            "Alpha alphabet <b>ALPHA</b> & beta",
        )];
        let rows = rows(&messages, "alpha", &TimeZone::UTC, now());
        assert_eq!(
            text_of(&rows[1]),
            "<b>Alpha</b> alphabet &lt;b&gt;<b>ALPHA</b>&lt;/b&gt; &amp; beta"
        );
    }

    // MSG-008
    #[test]
    fn every_word_of_the_query_is_marked_but_not_those_left_out_nor_a_regular_expression() {
        let text = "alpha beta gamma";
        let mark = |query: &str| {
            let messages = [message("a", "2026-10-20T10:00:00Z", text)];
            text_of(&rows(&messages, query, &TimeZone::UTC, now())[1]).to_owned()
        };
        assert_eq!(mark("alpha gamma"), "<b>alpha</b> beta <b>gamma</b>");
        assert_eq!(mark("\"alpha beta\""), "<b>alpha</b> <b>beta</b> gamma");
        assert_eq!(mark("alpha -beta"), "<b>alpha</b> beta gamma");
        assert_eq!(mark("/alph/i"), text);
        assert_eq!(mark(""), text);
        // A word with accents and a number.
        let messages = [message("a", "2026-10-20T10:00:00Z", "Café 42")];
        assert_eq!(
            text_of(&rows(&messages, "café 42", &TimeZone::UTC, now())[1]),
            "<b>Café</b> <b>42</b>"
        );
    }

    // MSG-008
    #[test]
    fn a_thread_reply_says_so_and_a_system_message_shows_its_wording() {
        let mut reply = message("a", "2026-10-20T10:00:00Z", "reply");
        reply.thread_id = Some("p".to_owned().into());
        let mut topic = message("b", "2026-10-20T09:00:00Z", "");
        topic.kind = MessageKind::System(SystemEvent::TopicChanged {
            topic: "alpha plans".to_owned(),
        });
        let rows = rows(&[reply, topic], "alpha", &TimeZone::UTC, now());
        let (Row::Message(reply), Row::Message(topic)) = (&rows[1], &rows[2]) else {
            panic!("not messages");
        };
        assert!(reply.in_thread);
        assert!(!topic.in_thread);
        assert_eq!(
            topic.text,
            "Alice A changed the topic to: <b>alpha</b> plans"
        );
        assert_eq!(topic.avatar, None);
    }

    // MSG-008
    #[test]
    fn the_text_is_one_line_and_not_longer_than_a_limit() {
        let long = format!("first\n\n  second {}", "x".repeat(400));
        let messages = [message("a", "2026-10-20T10:00:00Z", &long)];
        let rows = rows(&messages, "", &TimeZone::UTC, now());
        assert!(text_of(&rows[1]).starts_with("first second x"));
        assert_eq!(text_of(&rows[1]).chars().count(), TEXT_LIMIT);
    }

    // MSG-008
    #[test]
    fn nothing_is_shown_without_a_search_and_an_empty_field_has_its_note() {
        let mut results = Results::default();
        assert_eq!(view(&results), View::default());
        assert!(!view(&results).open);

        results.apply(Some(&found("", Vec::new(), More::No)));

        let shown = view(&results);
        assert!(shown.open);
        assert_eq!(shown.status, Status::Empty);
        assert_eq!(shown.note_title, "Search #general");
        assert_eq!(shown.note_hint, EMPTY_HINT);
        assert_eq!(shown.field_hint, FIELD_HINT);
        results.apply(None);
        assert!(!view(&results).open);
    }

    // MSG-008
    #[test]
    fn a_search_with_no_rows_spins_and_an_answer_with_none_says_so() {
        let mut results = Results::default();
        results.apply(Some(&search("alpha", MessageSearchState::Searching)));
        assert_eq!(view(&results).status, Status::Searching);
        assert!(!view(&results).refreshing);

        results.apply(Some(&found("alpha", Vec::new(), More::No)));

        let shown = view(&results);
        assert_eq!(shown.status, Status::None);
        assert_eq!(shown.note_title, "No messages with “alpha”");
        assert_eq!(shown.note_hint, NONE_HINT);
        // A new search with no rows to keep: the spinner, not the note of the last query.
        results.apply(Some(&search("beta", MessageSearchState::Searching)));
        assert_eq!(view(&results).status, Status::Searching);
    }

    // MSG-008
    #[test]
    fn the_rows_stay_while_a_new_search_runs_and_the_hint_says_so() {
        let mut results = Results::default();
        let messages = vec![message("a", "2026-10-20T10:00:00Z", "alpha")];
        results.apply(Some(&found("alpha", messages, More::Available)));
        let before = view(&results);
        assert_eq!(before.status, Status::Results);
        assert_eq!(before.footer, Footer::Available);

        results.apply(Some(&search("alphas", MessageSearchState::Searching)));

        let shown = view(&results);
        assert_eq!(shown.status, Status::Results);
        assert_eq!(shown.rows, before.rows);
        assert!(shown.refreshing);
        assert_eq!(shown.field_hint, FIELD_HINT_SEARCHING);
        // No next page is asked for until the answer is here.
        assert_eq!(shown.footer, Footer::Nothing);
    }

    // MSG-008
    #[test]
    fn another_room_forgets_the_rows() {
        let mut results = Results::default();
        let messages = vec![message("a", "2026-10-20T10:00:00Z", "alpha")];
        results.apply(Some(&found("alpha", messages, More::No)));
        let mut other = search("alpha", MessageSearchState::Searching);
        other.room = "other".to_owned().into();

        results.apply(Some(&other));

        assert_eq!(view(&results).status, Status::Searching);
        assert!(view(&results).rows.is_empty());
    }

    // MSG-008
    #[test]
    fn a_failed_first_page_says_why_and_drops_the_rows() {
        let mut results = Results::default();
        let messages = vec![message("a", "2026-10-20T10:00:00Z", "alpha")];
        results.apply(Some(&found("alpha", messages, More::No)));

        results.apply(Some(&search(
            "beta",
            MessageSearchState::Failed(MessageSearchError::Unreachable),
        )));

        let shown = view(&results);
        assert_eq!(shown.status, Status::Failed);
        assert_eq!(shown.error, FAILED_TEXT);
        assert!(shown.rows.is_empty());
        results.apply(Some(&search(
            "beta",
            MessageSearchState::Failed(MessageSearchError::Server("nope".to_owned())),
        )));
        assert_eq!(view(&results).error, "The server reported an error: nope");
    }

    // MSG-008
    #[test]
    fn the_footer_follows_the_next_page() {
        let mut results = Results::default();
        let messages = vec![message("a", "2026-10-20T10:00:00Z", "alpha")];
        let footer = |results: &Results| view(results).footer;
        results.apply(Some(&found("alpha", messages.clone(), More::Loading)));
        assert_eq!(footer(&results), Footer::Loading);

        results.apply(Some(&found(
            "alpha",
            messages.clone(),
            More::Failed(MessageSearchError::Unreachable),
        )));
        assert_eq!(footer(&results), Footer::Failed(FAILED_TEXT.to_owned()));
        assert_eq!(view(&results).status, Status::Results);

        results.apply(Some(&found("alpha", messages, More::No)));
        assert_eq!(footer(&results), Footer::Nothing);
        assert_eq!(Footer::Loading.name(), "loading");
        assert_eq!(Status::Results.name(), "results");
    }

    // MSG-008
    #[test]
    fn an_appended_page_keeps_the_rows_before_it() {
        let first = vec![
            message("a", "2026-10-20T10:00:00Z", "alpha"),
            message("b", "2026-10-19T10:00:00Z", "alpha"),
        ];
        let mut all = first.clone();
        all.push(message("c", "2026-10-19T08:00:00Z", "alpha"));
        let mut results = Results::default();
        results.apply(Some(&found("alpha", first, More::Available)));
        let before = view(&results).rows;

        results.apply(Some(&found("alpha", all, More::No)));

        let after = view(&results).rows;
        assert_eq!(after[..before.len()], before[..]);
        assert_eq!(after.len(), before.len() + 1);
    }

    // MSG-008
    #[test]
    fn the_search_follows_the_room_that_is_open() {
        let room: RoomId = "r".to_owned().into();
        let other: RoomId = "other".to_owned().into();
        assert_eq!(stale_room(Some(&room), Some(&other)), Some(other.clone()));
        assert_eq!(stale_room(Some(&room), Some(&room)), None);
        assert_eq!(stale_room(Some(&room), None), None);
        assert_eq!(stale_room(None, Some(&other)), None);
    }

    // MSG-008
    #[test]
    fn the_cursor_skips_the_day_labels_and_wraps() {
        let messages = [
            message("a", "2026-10-20T10:00:00Z", "one"),
            message("b", "2026-10-19T10:00:00Z", "two"),
        ];
        // Today, one, Yesterday, two.
        let rows = rows(&messages, "", &TimeZone::UTC, now());
        assert_eq!(first_cursor(&rows), Some(1));
        assert_eq!(step_cursor(&rows, None, true), Some(1));
        assert_eq!(step_cursor(&rows, Some(1), true), Some(3));
        assert_eq!(step_cursor(&rows, Some(3), true), Some(1));
        assert_eq!(step_cursor(&rows, None, false), Some(3));
        assert_eq!(step_cursor(&rows, Some(3), false), Some(1));
        assert_eq!(step_cursor(&rows, Some(1), false), Some(3));
        assert_eq!(step_cursor(&[], None, true), None);
        assert_eq!(first_cursor(&[]), None);
        assert_eq!(message_at(&rows, 3).map(MessageId::as_str), Some("b"));
        assert_eq!(message_at(&rows, 2), None);
    }

    fn reply(id: &str, parent: &str, in_room: bool) -> Message {
        Message {
            thread_id: Some(parent.to_owned().into()),
            thread_shown: in_room,
            ..message(id, "2026-10-20T10:00:00Z", "reply")
        }
    }

    // MSG-008
    #[test]
    fn a_hidden_reply_shows_its_parent_and_opens_the_thread() {
        let target = pick_target(&reply("r1", "p", false));
        assert_eq!(target.show.as_str(), "p");
        assert_eq!(target.thread.as_ref().map(MessageId::as_str), Some("p"));
    }

    // MSG-008
    #[test]
    fn a_reply_also_sent_to_the_room_shows_itself_and_opens_the_thread() {
        let target = pick_target(&reply("r1", "p", true));
        assert_eq!(target.show.as_str(), "r1");
        assert_eq!(target.thread.as_ref().map(MessageId::as_str), Some("p"));
    }

    // MSG-008
    #[test]
    fn a_plain_message_shows_itself_and_opens_nothing() {
        let target = pick_target(&message("m", "2026-10-20T10:00:00Z", "text"));
        assert_eq!(target.show.as_str(), "m");
        assert_eq!(target.thread, None);
    }

    fn shown(id: &str, state: FoundState) -> Found {
        Found {
            message: id.to_owned().into(),
            state,
        }
    }

    // MSG-008
    #[test]
    fn the_row_of_the_message_being_or_already_shown_is_picked() {
        let id = MessageId::from("m".to_owned());
        assert_eq!(pick_state(None, &id), (false, false));
        assert_eq!(
            pick_state(Some(&shown("m", FoundState::Loading)), &id),
            (true, true)
        );
        assert_eq!(
            pick_state(Some(&shown("m", FoundState::Shown)), &id),
            (true, false)
        );
        // A message that is gone is no row's to show, and another message's state is not ours.
        assert_eq!(
            pick_state(Some(&shown("m", FoundState::Missing)), &id),
            (false, false)
        );
        assert_eq!(
            pick_state(Some(&shown("other", FoundState::Shown)), &id),
            (false, false)
        );
    }

    // MSG-008
    #[test]
    fn only_a_missing_message_has_a_note() {
        assert_eq!(missing_note(None), "");
        assert_eq!(missing_note(Some(&shown("m", FoundState::Loading))), "");
        assert_eq!(missing_note(Some(&shown("m", FoundState::Shown))), "");
        assert_eq!(
            missing_note(Some(&shown("m", FoundState::Missing))),
            "This message no longer exists."
        );
    }
}
