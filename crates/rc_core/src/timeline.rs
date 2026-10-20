// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The messages of the open room in display order, and the index-based changes that keep a
//! frontend's copy of them in step. Frontends mirror the list and never sort or de-duplicate it
//! themselves.

use std::cmp::Ordering;
use std::collections::HashSet;

use crate::domain::{Message, RoomId};
use crate::ports::{ApiError, RoomHistory};

/// How many messages a history request asks for.
pub const PAGE_SIZE: usize = 50;

/// One step for a frontend's mirror of the message list. Apply the changes in order.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TimelineChange {
    /// The whole list, oldest first. `room` is the room it belongs to, `None` when no room is open.
    Reset {
        room: Option<RoomId>,
        messages: Vec<Message>,
    },
    /// Older messages, oldest first, to go before everything.
    Prepended(Vec<Message>),
    /// Newer messages, oldest first, to go after everything.
    Appended(Vec<Message>),
}

impl TimelineChange {
    /// Applies the change to a frontend's mirror of the message list, which must be in the state
    /// the change was made against.
    pub fn apply(&self, messages: &mut Vec<Message>) {
        match self {
            Self::Reset { messages: all, .. } => messages.clone_from(all),
            Self::Prepended(older) => {
                messages.splice(0..0, older.iter().cloned());
            }
            Self::Appended(newer) => messages.extend(newer.iter().cloned()),
        }
    }
}

/// Why the last history request failed. The texts are meant to be shown as they are.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TimelineError {
    #[error("Can't reach the server")]
    Unreachable,
    /// Any other error, with its message.
    #[error("The server reported an error: {0}")]
    Server(String),
}

impl From<&ApiError> for TimelineError {
    fn from(error: &ApiError) -> Self {
        match error {
            ApiError::Unreachable(_) => Self::Unreachable,
            other => Self::Server(other.to_string()),
        }
    }
}

/// What the frontend shows next to the messages.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TimelineStatus {
    /// The open room; `None` when there is none.
    pub room: Option<RoomId>,
    /// The newest messages are being fetched.
    pub loading: bool,
    /// Older messages are being fetched.
    pub loading_older: bool,
    /// Nothing exists before the first message.
    pub reached_start: bool,
    /// Why the last request failed; cleared when the next one starts.
    pub error: Option<TimelineError>,
}

/// The display order: by time, and by id between messages sent in the same millisecond, so that
/// the order is the same whichever page a message arrived in.
fn display_order(a: &Message, b: &Message) -> Ordering {
    a.ts.cmp(&b.ts).then_with(|| a.id.cmp(&b.id))
}

/// The messages of one room.
#[derive(Debug)]
pub struct Timeline {
    room: RoomId,
    history: RoomHistory,
}

impl Timeline {
    /// `history` is trusted to be in display order and unique, as [`history`](Self::history)
    /// leaves it.
    #[must_use]
    pub fn new(room: RoomId, history: RoomHistory) -> Self {
        Self { room, history }
    }

    #[must_use]
    pub fn room(&self) -> &RoomId {
        &self.room
    }

    #[must_use]
    pub fn messages(&self) -> &[Message] {
        &self.history.messages
    }

    #[must_use]
    pub fn reached_start(&self) -> bool {
        self.history.reached_start
    }

    /// What to keep, to open the room again later.
    #[must_use]
    pub fn history(&self) -> &RoomHistory {
        &self.history
    }

    /// Where the next page of older messages starts.
    #[must_use]
    pub fn oldest(&self) -> Option<jiff::Timestamp> {
        self.history.messages.first().map(|m| m.ts)
    }

    /// Takes in the newest page of the room. If it doesn't touch what is held and there is more
    /// history before it, the held messages are dropped for the page: keeping both would leave a
    /// hole in the middle that nothing ever fills.
    pub fn merge_latest(
        &mut self,
        page: Vec<Message>,
        reached_start: bool,
    ) -> Option<TimelineChange> {
        let held: HashSet<_> = self.history.messages.iter().map(|m| &m.id).collect();
        let touches = page.iter().any(|m| held.contains(&m.id));
        if !self.history.messages.is_empty() && !touches && !reached_start {
            self.history.messages.clear();
            self.history.reached_start = false;
        }
        self.merge(page, reached_start)
    }

    /// Takes in a page of messages older than those held.
    pub fn merge_older(
        &mut self,
        page: Vec<Message>,
        reached_start: bool,
    ) -> Option<TimelineChange> {
        self.merge(page, reached_start)
    }

    /// Adds the messages not held yet, in order. The change is a prepend or an append when they all
    /// fall on one side of what is held, else (never in practice) a reset.
    fn merge(&mut self, page: Vec<Message>, reached_start: bool) -> Option<TimelineChange> {
        let mut seen: HashSet<_> = self.history.messages.iter().map(|m| m.id.clone()).collect();
        let mut fresh: Vec<Message> = page
            .into_iter()
            .filter(|m| seen.insert(m.id.clone()))
            .collect();
        fresh.sort_by(display_order);
        // The start is known whether or not the page brought anything new.
        self.history.reached_start |= reached_start;
        let (Some(first), Some(last)) = (fresh.first(), fresh.last()) else {
            return None;
        };
        let (older, newer) = match (self.history.messages.first(), self.history.messages.last()) {
            (Some(held_first), Some(held_last)) => (
                display_order(last, held_first).is_lt(),
                display_order(first, held_last).is_gt(),
            ),
            _ => (false, false),
        };
        if older {
            self.history.messages.splice(0..0, fresh.iter().cloned());
            Some(TimelineChange::Prepended(fresh))
        } else if newer {
            self.history.messages.extend(fresh.iter().cloned());
            Some(TimelineChange::Appended(fresh))
        } else {
            self.history.messages.extend(fresh);
            self.history.messages.sort_by(display_order);
            Some(self.reset())
        }
    }

    /// The change that replaces the mirror with what is held.
    #[must_use]
    pub fn reset(&self) -> TimelineChange {
        TimelineChange::Reset {
            room: Some(self.room.clone()),
            messages: self.history.messages.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Author, MessageKind};

    /// A message `secs` seconds after the epoch.
    fn msg(id: &str, secs: i64) -> Message {
        Message {
            id: id.to_owned().into(),
            room_id: "r".to_owned().into(),
            author: Author {
                id: "u".to_owned().into(),
                username: "u".into(),
                display_name: None,
            },
            ts: jiff::Timestamp::from_second(secs).unwrap(),
            kind: MessageKind::Text(id.to_owned()),
        }
    }

    fn ids(messages: &[Message]) -> Vec<&str> {
        messages.iter().map(|m| m.id.as_str()).collect()
    }

    fn empty() -> Timeline {
        Timeline::new("r".to_owned().into(), RoomHistory::default())
    }

    fn timeline_of(messages: Vec<Message>) -> Timeline {
        let mut timeline = empty();
        timeline.merge_latest(messages, false);
        timeline
    }

    // MSG-001
    #[test]
    fn the_first_page_resets_and_sorts() {
        let mut timeline = empty();
        let change = timeline.merge_latest(vec![msg("c", 3), msg("a", 1), msg("b", 2)], false);
        assert_eq!(ids(timeline.messages()), ["a", "b", "c"]);
        assert_eq!(
            change,
            Some(TimelineChange::Reset {
                room: Some("r".to_owned().into()),
                messages: timeline.messages().to_vec()
            })
        );
        assert!(!timeline.reached_start());
        assert_eq!(timeline.oldest(), Some(msg("a", 1).ts));
    }

    // MSG-001
    #[test]
    fn an_empty_first_page_is_no_change_but_may_reach_the_start() {
        let mut timeline = empty();
        assert_eq!(timeline.merge_latest(Vec::new(), true), None);
        assert!(timeline.reached_start());
    }

    // MSG-002
    #[test]
    fn older_messages_are_prepended_without_the_duplicates() {
        let mut timeline = timeline_of(vec![msg("c", 3), msg("d", 4)]);
        // The page starts at the oldest held message, inclusive.
        let change = timeline.merge_older(vec![msg("c", 3), msg("b", 2), msg("a", 1)], false);
        assert_eq!(
            change,
            Some(TimelineChange::Prepended(vec![msg("a", 1), msg("b", 2)]))
        );
        assert_eq!(ids(timeline.messages()), ["a", "b", "c", "d"]);
    }

    // MSG-002
    #[test]
    fn messages_sharing_a_millisecond_are_ordered_by_id() {
        let mut timeline = timeline_of(vec![msg("b", 5), msg("c", 5)]);
        let change = timeline.merge_older(vec![msg("b", 5), msg("a", 5), msg("z", 4)], false);
        assert_eq!(
            change,
            Some(TimelineChange::Prepended(vec![msg("z", 4), msg("a", 5)]))
        );
        assert_eq!(ids(timeline.messages()), ["z", "a", "b", "c"]);
    }

    // MSG-002
    #[test]
    fn a_page_with_nothing_new_changes_nothing_but_can_reach_the_start() {
        let mut timeline = timeline_of(vec![msg("a", 1)]);
        assert_eq!(timeline.merge_older(vec![msg("a", 1)], true), None);
        assert!(timeline.reached_start());
        assert_eq!(ids(timeline.messages()), ["a"]);
    }

    // MSG-001
    #[test]
    fn newer_messages_are_appended() {
        let mut timeline = timeline_of(vec![msg("a", 1), msg("b", 2)]);
        let change = timeline.merge_latest(vec![msg("c", 3), msg("b", 2)], false);
        assert_eq!(change, Some(TimelineChange::Appended(vec![msg("c", 3)])));
        assert_eq!(ids(timeline.messages()), ["a", "b", "c"]);
    }

    // MSG-001
    #[test]
    fn a_newest_page_that_misses_what_is_held_replaces_it() {
        let mut timeline = timeline_of(vec![msg("a", 1), msg("b", 2)]);
        let change = timeline.merge_latest(vec![msg("x", 8), msg("y", 9)], false);
        assert_eq!(ids(timeline.messages()), ["x", "y"]);
        assert!(matches!(change, Some(TimelineChange::Reset { .. })));
        assert!(!timeline.reached_start());
    }

    // MSG-001
    #[test]
    fn a_newest_page_that_reaches_the_start_joins_what_is_held() {
        let mut timeline = timeline_of(vec![msg("b", 2)]);
        let change = timeline.merge_latest(vec![msg("c", 3), msg("a", 1)], true);
        // The page has messages on both sides: not a prepend or an append.
        assert!(matches!(change, Some(TimelineChange::Reset { .. })));
        assert_eq!(ids(timeline.messages()), ["a", "b", "c"]);
        assert!(timeline.reached_start());
    }

    // MSG-002
    #[test]
    fn reached_start_stays_once_known() {
        let mut timeline = Timeline::new(
            "r".to_owned().into(),
            RoomHistory {
                messages: vec![msg("a", 1)],
                reached_start: true,
            },
        );
        timeline.merge_latest(vec![msg("a", 1), msg("b", 2)], false);
        assert!(timeline.reached_start());
    }

    /// Applies each page's change to the mirror, which must then equal the timeline.
    fn step(
        timeline: &mut Timeline,
        mirror: &mut Vec<Message>,
        action: impl FnOnce(&mut Timeline) -> Option<TimelineChange>,
    ) {
        if let Some(change) = action(timeline) {
            change.apply(mirror);
        }
        assert_eq!(mirror.as_slice(), timeline.messages());
    }

    // MSG-001, MSG-002
    #[test]
    fn a_mirror_following_the_changes_equals_the_timeline() {
        let (mut timeline, mut mirror) = (empty(), Vec::new());
        let (t, m) = (&mut timeline, &mut mirror);
        step(t, m, |t| {
            t.merge_latest(vec![msg("e", 5), msg("f", 6)], false)
        });
        step(t, m, |t| {
            t.merge_older(vec![msg("e", 5), msg("c", 3), msg("d", 4)], false)
        });
        step(t, m, |t| {
            t.merge_latest(vec![msg("g", 7), msg("f", 6)], false)
        });
        step(t, m, |t| {
            t.merge_older(vec![msg("c", 3), msg("a", 1), msg("b", 2)], true)
        });
        step(t, m, |t| t.merge_older(vec![msg("a", 1)], true));
        // A newest page that misses everything held.
        step(t, m, |t| {
            t.merge_latest(vec![msg("y", 90), msg("z", 91)], false)
        });
        step(t, m, |t| {
            t.merge_latest(vec![msg("x", 80), msg("y", 90), msg("z", 91)], true)
        });
        assert_eq!(ids(m), ["x", "y", "z"]);
    }

    // MSG-001
    #[test]
    fn a_reset_replaces_the_mirror() {
        let mut mirror = vec![msg("old", 1)];
        TimelineChange::Reset {
            room: None,
            messages: Vec::new(),
        }
        .apply(&mut mirror);
        assert!(mirror.is_empty());
    }

    // MSG-001
    #[test]
    fn errors_have_user_facing_texts() {
        assert_eq!(
            TimelineError::from(&ApiError::Unreachable("refused".into())).to_string(),
            "Can't reach the server"
        );
        let server = ApiError::Server {
            status: 400,
            message: "no such room".into(),
        };
        assert_eq!(
            TimelineError::from(&server).to_string(),
            "The server reported an error: the server answered 400: no such room"
        );
    }
}
