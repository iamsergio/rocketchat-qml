// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! What the `ThreadList` view model and the `ThreadListModel` rows show for the open room's thread
//! list: the texts of a row, the state of the list, and what the pane beside the room shows. Plain
//! Rust, so it can be unit-tested without Qt and every frontend can use it.

use rc_core::domain::{Message, MessageId, Room, RoomId};
use rc_core::threads::{ThreadFilter, ThreadListStatus};

use crate::message_view::body;

/// The most characters of a parent's text a row is given: the row shows two lines of it.
const TEXT_LIMIT: usize = 240;

/// What the pane beside the room shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Pane {
    /// Nothing: the pane is closed.
    #[default]
    None,
    /// The room's threads.
    List,
    /// The open thread, which hides the list (it is still there, to come back to).
    Thread,
    /// The search of the room's messages, which takes the place of the threads: opening one closes
    /// the other.
    Search,
}

impl Pane {
    /// The name the GUI knows it by.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::List => "list",
            Self::Thread => "thread",
            Self::Search => "search",
        }
    }
}

/// The name the GUI knows a filter by.
#[must_use]
pub fn filter_name(filter: ThreadFilter) -> &'static str {
    match filter {
        ThreadFilter::All => "all",
        ThreadFilter::Following => "following",
        ThreadFilter::Unread => "unread",
    }
}

/// The filter the GUI called `name`, if it is one.
#[must_use]
pub fn parse_filter(name: &str) -> Option<ThreadFilter> {
    match name {
        "all" => Some(ThreadFilter::All),
        "following" => Some(ThreadFilter::Following),
        "unread" => Some(ThreadFilter::Unread),
        _ => None,
    }
}

/// The start of the parent's text, on one line: the row clamps it to two.
#[must_use]
pub fn row_text(parent: &Message) -> String {
    let line = body(parent)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if line.chars().count() <= TEXT_LIMIT {
        return line;
    }
    line.chars().take(TEXT_LIMIT).collect()
}

/// "3 replies" (or "1 reply"), as the row's last line says it. When the last one came is the time at
/// the row's top right.
#[must_use]
pub fn row_summary(parent: &Message) -> String {
    match parent.thread_replies {
        1 => "1 reply".to_owned(),
        n => format!("{n} replies"),
    }
}

/// The values of the `ThreadList` properties: whether the room's threads are listed, how it is
/// going, and what the pane beside the room shows.
#[allow(clippy::struct_excessive_bools)] // A property each.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThreadListState {
    /// What the pane shows (see [`Pane::name`]).
    pub pane: String,
    /// The pane shows the list or a thread (not the search: see `search_open`).
    pub pane_open: bool,
    /// The pane shows the search of the room's messages.
    pub search_open: bool,
    /// Whether the list is open, whether or not the pane shows it.
    pub list_open: bool,
    /// A thread is open (and hides the list, if that is open).
    pub thread_open: bool,
    /// The filter of the open list (see [`filter_name`]); `all` while it is closed.
    pub filter: String,
    pub loading: bool,
    pub loading_more: bool,
    pub reached_end: bool,
    /// Why the last request failed, as it is to be shown. Empty if it didn't.
    pub error_text: String,
    /// The name of the open room, for the pane's heading. Empty while the room isn't in the room
    /// list.
    pub room_name: String,
    /// The list is open and has nothing to show: said so by the server, not still loading.
    pub empty: bool,
    /// What an empty list says (see [`empty_texts`]); empty while it isn't empty.
    pub empty_title: String,
    pub empty_hint: String,
    /// How many threads of the room have replies the user hasn't read: the badge of the room's
    /// threads button, and the count of the "Unread" filter.
    pub unread_count: usize,
    /// One of them mentions the user: the badge is red.
    pub unread_mention: bool,
}

/// The title and the hint of an empty list of `filter` in the room called `room_name`.
#[must_use]
pub fn empty_texts(filter: ThreadFilter, room_name: &str) -> (String, String) {
    match filter {
        ThreadFilter::All => (
            format!("No threads in #{room_name} yet"),
            "Start one with Reply in thread on a message.".to_owned(),
        ),
        ThreadFilter::Following => (
            format!("No threads you follow in #{room_name}"),
            "Reply in a thread to follow it.".to_owned(),
        ),
        ThreadFilter::Unread => (
            format!("No unread threads in #{room_name}"),
            "New replies in a thread you follow show up here.".to_owned(),
        ),
    }
}

/// The state for the list `status` is about, which holds `rows` threads, in the open room `room`
/// (with its name taken from `rooms`), when a thread is open or not, and when the search of the
/// room's messages is open or not.
#[must_use]
pub fn thread_list_state(
    rooms: &[Room],
    room: Option<&RoomId>,
    status: &ThreadListStatus,
    rows: usize,
    thread_open: bool,
    search_open: bool,
) -> ThreadListState {
    let held = room.and_then(|id| rooms.iter().find(|candidate| &candidate.id == id));
    let name = held.map(|room| room.name.clone()).unwrap_or_default();
    let unread_count = held.map_or(0, |room| room.unread_threads.len());
    let unread_mention = held.is_some_and(|room| {
        (room.mentioned_threads.iter()).any(|id| room.unread_threads.contains(id))
    });
    let list_open = status.room.is_some();
    let pane = if thread_open {
        Pane::Thread
    } else if search_open {
        Pane::Search
    } else if list_open {
        Pane::List
    } else {
        Pane::None
    };
    let error_text = status
        .error
        .as_ref()
        .map_or_else(String::new, ToString::to_string);
    let empty =
        list_open && rows == 0 && !status.loading && !status.loading_more && error_text.is_empty();
    let (empty_title, empty_hint) = if empty {
        empty_texts(status.filter, &name)
    } else {
        (String::new(), String::new())
    };
    ThreadListState {
        pane: pane.name().to_owned(),
        pane_open: matches!(pane, Pane::List | Pane::Thread),
        search_open: pane == Pane::Search,
        list_open,
        thread_open,
        filter: filter_name(status.filter).to_owned(),
        loading: status.loading,
        loading_more: status.loading_more,
        reached_end: status.reached_end,
        error_text,
        room_name: name,
        empty,
        empty_title,
        empty_hint,
        unread_count,
        unread_mention,
    }
}

/// The threads of the open room that have replies the user hasn't read, and those among them that
/// mention them: what decides how the rows of the list look.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThreadUnread {
    pub unread: Vec<MessageId>,
    pub mentioned: Vec<MessageId>,
}

impl ThreadUnread {
    /// What the room says.
    #[must_use]
    pub fn of(room: &Room) -> Self {
        Self {
            unread: room.unread_threads.clone(),
            mentioned: room.mentioned_threads.clone(),
        }
    }

    /// Whether the thread of the parent `id` has replies the user hasn't read.
    #[must_use]
    pub fn is_unread(&self, id: &MessageId) -> bool {
        self.unread.contains(id)
    }

    /// Whether one of its unread replies mentions the user.
    #[must_use]
    pub fn is_mentioned(&self, id: &MessageId) -> bool {
        self.is_unread(id) && self.mentioned.contains(id)
    }
}

/// The rows of `threads`, as runs `(first, last)` of consecutive rows, that look different under
/// `after` than under `before`: the only ones to announce when the subscription changes.
#[must_use]
pub fn rows_changed(
    threads: &[Message],
    before: &ThreadUnread,
    after: &ThreadUnread,
) -> Vec<(usize, usize)> {
    crate::avatar_view::runs(threads.len(), |at| {
        let id = &threads[at].id;
        before.is_unread(id) != after.is_unread(id)
            || before.is_mentioned(id) != after.is_mentioned(id)
    })
}

/// One thing a command on the pane asks of the core.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneStep {
    CloseThread,
    CloseList,
    OpenList(ThreadFilter),
    /// Searches the open room's messages, for nothing yet: the pane opens.
    OpenSearch,
    CloseSearch,
}

/// The back button of a thread: the thread closes, and the list opens with `filter` unless it is
/// open. It leads nowhere from the list.
#[must_use]
pub fn back_steps(state: &ThreadListState, filter: ThreadFilter) -> Vec<PaneStep> {
    if !state.thread_open {
        return Vec::new();
    }
    let mut steps = vec![PaneStep::CloseThread];
    if !state.list_open {
        steps.push(PaneStep::OpenList(filter));
    }
    steps
}

/// The pane's close button: the thread, the list and the search close.
#[must_use]
pub fn close_pane_steps(state: &ThreadListState) -> Vec<PaneStep> {
    let mut steps = Vec::new();
    if state.thread_open {
        steps.push(PaneStep::CloseThread);
    }
    if state.list_open {
        steps.push(PaneStep::CloseList);
    }
    if state.search_open {
        steps.push(PaneStep::CloseSearch);
    }
    steps
}

/// The list opens with `filter`, after the search closes: they share the pane.
#[must_use]
pub fn open_list_steps(state: &ThreadListState, filter: ThreadFilter) -> Vec<PaneStep> {
    let mut steps = open_thread_steps(state);
    steps.push(PaneStep::OpenList(filter));
    steps
}

/// A thread opens: the search closes first.
#[must_use]
pub fn open_thread_steps(state: &ThreadListState) -> Vec<PaneStep> {
    if state.search_open {
        vec![PaneStep::CloseSearch]
    } else {
        Vec::new()
    }
}

/// The search opens, after the thread and the list close. Nothing if it is open.
#[must_use]
pub fn open_search_steps(state: &ThreadListState) -> Vec<PaneStep> {
    if state.search_open {
        return Vec::new();
    }
    let mut steps = Vec::new();
    if state.thread_open {
        steps.push(PaneStep::CloseThread);
    }
    if state.list_open {
        steps.push(PaneStep::CloseList);
    }
    steps.push(PaneStep::OpenSearch);
    steps
}

/// The header button of the search: the search closes if it is open, else it opens.
#[must_use]
pub fn toggle_search_steps(state: &ThreadListState) -> Vec<PaneStep> {
    if state.search_open {
        vec![PaneStep::CloseSearch]
    } else {
        open_search_steps(state)
    }
}

/// The header button of the threads: the pane closes if it shows the list or a thread, else the
/// list opens with `filter`.
#[must_use]
pub fn toggle_steps(state: &ThreadListState, filter: ThreadFilter) -> Vec<PaneStep> {
    if state.pane_open {
        close_pane_steps(state)
    } else {
        open_list_steps(state, filter)
    }
}

/// Escape in the pane: back from a thread, else the list or the search closes.
#[must_use]
pub fn escape_steps(state: &ThreadListState, filter: ThreadFilter) -> Vec<PaneStep> {
    if state.thread_open {
        back_steps(state, filter)
    } else if state.list_open {
        vec![PaneStep::CloseList]
    } else if state.search_open {
        vec![PaneStep::CloseSearch]
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use jiff::Timestamp;
    use rc_core::domain::{Author, Delivery, MessageKind, RoomKind};
    use rc_core::timeline::TimelineError;

    use super::*;

    fn parent(text: &str, replies: u32, last: &str) -> Message {
        Message {
            id: "p".to_owned().into(),
            room_id: "1".to_owned().into(),
            author: Author {
                id: "u".to_owned().into(),
                username: "alice".to_owned(),
                display_name: None,
            },
            ts: "2026-10-20T10:00:00Z".parse().unwrap(),
            kind: MessageKind::Text(text.into()),
            delivery: Delivery::Sent,
            attachments: Vec::new(),
            quotes: Vec::new(),
            edited: false,
            reactions: Vec::new(),
            thread_id: None,
            thread_shown: false,
            thread_replies: replies,
            thread_last_reply: last.parse().ok(),
            thread_followers: Vec::new(),
        }
    }

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
            unread_threads: Vec::new(),
            mentioned_threads: Vec::new(),
            last_activity: Timestamp::UNIX_EPOCH,
        }
    }

    fn open(filter: ThreadFilter) -> ThreadListStatus {
        ThreadListStatus {
            room: Some("1".to_owned().into()),
            filter,
            ..ThreadListStatus::default()
        }
    }

    // ACT-010
    #[test]
    fn a_row_shows_the_start_of_the_parent_on_one_line_and_not_more_than_a_limit() {
        assert_eq!(
            row_text(&parent(
                "first\n\n  second   line",
                1,
                "2026-10-20T11:00:00Z"
            )),
            "first second line"
        );
        let long = "ab ".repeat(200);
        assert_eq!(
            row_text(&parent(&long, 1, "2026-10-20T11:00:00Z"))
                .chars()
                .count(),
            240
        );
    }

    // ACT-010
    #[test]
    fn the_summary_counts_the_replies() {
        assert_eq!(
            row_summary(&parent("x", 3, "2026-10-20T14:02:00Z")),
            "3 replies"
        );
        assert_eq!(
            row_summary(&parent("x", 1, "2026-10-20T14:02:00Z")),
            "1 reply"
        );
    }

    // ACT-010
    #[test]
    fn the_pane_shows_the_thread_over_the_list_and_nothing_when_neither_is_open() {
        let rooms = [room("1", "general")];
        let id = RoomId::from("1".to_owned());
        let closed = thread_list_state(
            &rooms,
            Some(&id),
            &ThreadListStatus::default(),
            0,
            false,
            false,
        );
        assert_eq!(closed.pane, "none");
        assert!(!closed.list_open);
        assert_eq!(closed.room_name, "general");

        let list = thread_list_state(&rooms, Some(&id), &open(ThreadFilter::All), 2, false, false);
        assert_eq!((list.pane.as_str(), list.list_open), ("list", true));

        let thread = thread_list_state(&rooms, Some(&id), &open(ThreadFilter::All), 2, true, false);
        assert_eq!((thread.pane.as_str(), thread.list_open), ("thread", true));

        // A thread alone, opened from the summary under a message.
        let alone = thread_list_state(
            &rooms,
            Some(&id),
            &ThreadListStatus::default(),
            0,
            true,
            false,
        );
        assert_eq!((alone.pane.as_str(), alone.list_open), ("thread", false));
    }

    // ACT-010
    #[test]
    fn an_empty_list_says_so_in_the_words_of_its_filter_once_it_has_loaded() {
        let rooms = [room("1", "general")];
        let id = RoomId::from("1".to_owned());
        let all = thread_list_state(&rooms, Some(&id), &open(ThreadFilter::All), 0, false, false);
        assert!(all.empty);
        assert_eq!(all.empty_title, "No threads in #general yet");
        assert_eq!(
            all.empty_hint,
            "Start one with Reply in thread on a message."
        );
        let following = thread_list_state(
            &rooms,
            Some(&id),
            &open(ThreadFilter::Following),
            0,
            false,
            false,
        );
        assert_eq!(following.empty_title, "No threads you follow in #general");
        assert_eq!(following.empty_hint, "Reply in a thread to follow it.");
        assert_eq!(following.filter, "following");

        // Not while loading, with rows, or after a failure.
        let loading = ThreadListStatus {
            loading: true,
            ..open(ThreadFilter::All)
        };
        assert!(!thread_list_state(&rooms, Some(&id), &loading, 0, false, false).empty);
        assert!(
            !thread_list_state(&rooms, Some(&id), &open(ThreadFilter::All), 1, false, false).empty
        );
        let failed = ThreadListStatus {
            error: Some(TimelineError::Unreachable),
            ..open(ThreadFilter::All)
        };
        let state = thread_list_state(&rooms, Some(&id), &failed, 0, false, false);
        assert!(!state.empty);
        assert_eq!(state.error_text, "Can't reach the server");
        assert_eq!(state.empty_title, "");
    }

    // ACT-010
    #[test]
    fn filters_are_known_to_the_gui_by_name() {
        for filter in [
            ThreadFilter::All,
            ThreadFilter::Following,
            ThreadFilter::Unread,
        ] {
            assert_eq!(parse_filter(filter_name(filter)), Some(filter));
        }
        assert_eq!(parse_filter("everything"), None);
    }

    fn state(list_open: bool, thread_open: bool) -> ThreadListState {
        state_with(list_open, thread_open, false)
    }

    fn state_with(list_open: bool, thread_open: bool, search_open: bool) -> ThreadListState {
        let rooms = [room("1", "general")];
        let id = RoomId::from("1".to_owned());
        let status = if list_open {
            open(ThreadFilter::All)
        } else {
            ThreadListStatus::default()
        };
        thread_list_state(&rooms, Some(&id), &status, 1, thread_open, search_open)
    }

    // ACT-010
    #[test]
    fn back_closes_the_thread_and_opens_the_list_unless_it_is_open() {
        let following = ThreadFilter::Following;
        assert_eq!(
            back_steps(&state(false, true), following),
            [PaneStep::CloseThread, PaneStep::OpenList(following)]
        );
        assert_eq!(
            back_steps(&state(true, true), following),
            [PaneStep::CloseThread]
        );
        assert!(back_steps(&state(true, false), following).is_empty());
        assert!(back_steps(&state(false, false), following).is_empty());
    }

    // ACT-010
    #[test]
    fn the_close_button_closes_what_is_open_and_the_header_button_toggles() {
        use PaneStep::{CloseList, CloseThread, OpenList};
        let all = ThreadFilter::All;
        assert_eq!(
            close_pane_steps(&state(true, true)),
            [CloseThread, CloseList]
        );
        assert_eq!(close_pane_steps(&state(false, true)), [CloseThread]);
        assert_eq!(close_pane_steps(&state(true, false)), [CloseList]);
        assert!(close_pane_steps(&state(false, false)).is_empty());

        assert_eq!(toggle_steps(&state(false, false), all), [OpenList(all)]);
        assert_eq!(toggle_steps(&state(true, false), all), [CloseList]);
        assert_eq!(
            toggle_steps(&state(true, true), all),
            [CloseThread, CloseList]
        );
    }

    // ACT-010
    #[test]
    fn escape_goes_back_from_a_thread_and_closes_the_list_from_the_list() {
        use PaneStep::{CloseList, CloseThread, OpenList};
        let all = ThreadFilter::All;
        assert_eq!(escape_steps(&state(true, true), all), [CloseThread]);
        assert_eq!(
            escape_steps(&state(false, true), all),
            [CloseThread, OpenList(all)]
        );
        assert_eq!(escape_steps(&state(true, false), all), [CloseList]);
        assert!(escape_steps(&state(false, false), all).is_empty());
    }

    // MSG-008
    #[test]
    fn the_search_takes_the_pane_and_is_not_the_threads_button_s_pane() {
        let search = state_with(false, false, true);
        assert_eq!(search.pane, "search");
        assert!(search.search_open);
        assert!(!search.pane_open);
        let threads = state(true, false);
        assert!(!threads.search_open);
        assert_eq!(state_with(false, true, false).pane, "thread");
    }

    // MSG-008
    #[test]
    fn opening_the_search_closes_the_threads_and_opening_them_closes_the_search() {
        use PaneStep::{CloseList, CloseSearch, CloseThread, OpenList, OpenSearch};
        let all = ThreadFilter::All;
        assert_eq!(open_search_steps(&state(false, false)), [OpenSearch]);
        assert_eq!(
            open_search_steps(&state(true, false)),
            [CloseList, OpenSearch]
        );
        assert_eq!(
            open_search_steps(&state(true, true)),
            [CloseThread, CloseList, OpenSearch]
        );
        assert!(open_search_steps(&state_with(false, false, true)).is_empty());

        let searching = state_with(false, false, true);
        assert_eq!(toggle_steps(&searching, all), [CloseSearch, OpenList(all)]);
        assert_eq!(open_thread_steps(&searching), [CloseSearch]);
        assert!(open_thread_steps(&state(true, false)).is_empty());
    }

    // MSG-008
    #[test]
    fn the_search_button_toggles_and_the_close_button_and_escape_close_it() {
        use PaneStep::{CloseList, CloseSearch, OpenSearch};
        let all = ThreadFilter::All;
        let searching = state_with(false, false, true);
        assert_eq!(toggle_search_steps(&searching), [CloseSearch]);
        assert_eq!(toggle_search_steps(&state(false, false)), [OpenSearch]);
        assert_eq!(
            toggle_search_steps(&state(true, false)),
            [CloseList, OpenSearch]
        );
        assert_eq!(close_pane_steps(&searching), [CloseSearch]);
        assert_eq!(escape_steps(&searching, all), [CloseSearch]);
    }

    // ACT-011
    #[test]
    fn the_badge_counts_the_unread_threads_and_is_red_when_one_mentions_the_user() {
        let mut general = room("1", "general");
        let id = RoomId::from("1".to_owned());
        let count = |room: &Room| {
            let state = thread_list_state(
                std::slice::from_ref(room),
                Some(&id),
                &ThreadListStatus::default(),
                0,
                false,
                false,
            );
            (state.unread_count, state.unread_mention)
        };
        assert_eq!(count(&general), (0, false));
        general.unread_threads = vec!["a".to_owned().into(), "b".to_owned().into()];
        assert_eq!(count(&general), (2, false));
        general.mentioned_threads = vec!["b".to_owned().into()];
        assert_eq!(count(&general), (2, true));
        // A mention of a thread that was read since doesn't count.
        general.unread_threads = vec!["a".to_owned().into()];
        assert_eq!(count(&general), (1, false));
    }

    // ACT-011
    #[test]
    fn an_empty_unread_list_says_so() {
        let rooms = [room("1", "general")];
        let id = RoomId::from("1".to_owned());
        let state = thread_list_state(
            &rooms,
            Some(&id),
            &open(ThreadFilter::Unread),
            0,
            false,
            false,
        );
        assert_eq!(state.filter, "unread");
        assert_eq!(state.empty_title, "No unread threads in #general");
        assert_eq!(
            state.empty_hint,
            "New replies in a thread you follow show up here."
        );
        assert_eq!(parse_filter("unread"), Some(ThreadFilter::Unread));
    }

    // ACT-011
    #[test]
    fn only_the_rows_whose_look_changes_are_announced() {
        let rows: Vec<Message> = ["a", "b", "c", "d"]
            .iter()
            .map(|id| Message {
                id: (*id).to_owned().into(),
                ..parent("x", 1, "2026-10-20T11:00:00Z")
            })
            .collect();
        let ids = |ids: &[&str]| -> Vec<MessageId> {
            ids.iter().map(|id| (*id).to_owned().into()).collect()
        };
        let before = ThreadUnread {
            unread: ids(&["a", "b"]),
            mentioned: ids(&["b"]),
        };
        let after = ThreadUnread {
            unread: ids(&["b", "c", "d"]),
            mentioned: ids(&["b", "d"]),
        };
        // `a` was read, `b` is the same, `c` and `d` became unread (`d` mentioning).
        assert_eq!(rows_changed(&rows, &before, &after), [(0, 0), (2, 3)]);
        assert!(rows_changed(&rows, &before, &before).is_empty());
        // The mention goes with the unread state.
        assert!(!after.is_mentioned(&"a".to_owned().into()));
        assert!(after.is_mentioned(&"d".to_owned().into()));
    }
}
