// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The messages of the open room in display order, and the index-based changes that keep a
//! frontend's copy of them in step. Frontends mirror the list and never sort or de-duplicate it
//! themselves.

use std::cmp::Ordering;
use std::collections::HashSet;

use crate::domain::{Delivery, Message, MessageId, RoomId};
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
    /// Messages, oldest first, to go in at `index`, in front of what is there now: newer than the
    /// messages the server confirmed, but in front of those it hasn't.
    Inserted {
        index: usize,
        messages: Vec<Message>,
    },
    /// Same position, new content: a message the server confirmed or refused, or one that was
    /// edited or reacted to.
    Updated { index: usize, message: Message },
    /// The message moved; the content may have changed too. `to` is its index once the move is
    /// done (`Vec::remove(from)` then `Vec::insert(to, ..)`). Qt's `beginMoveRows` takes a
    /// different destination, so `rc_qt` converts.
    Moved {
        from: usize,
        to: usize,
        message: Message,
    },
    /// The message at `index` is gone: it was deleted.
    Removed { index: usize },
}

impl TimelineChange {
    /// The messages the change brings or replaces.
    #[must_use]
    pub fn messages(&self) -> &[Message] {
        match self {
            Self::Reset { messages, .. }
            | Self::Inserted { messages, .. }
            | Self::Prepended(messages)
            | Self::Appended(messages) => messages,
            Self::Removed { .. } => &[],
            Self::Updated { message, .. } | Self::Moved { message, .. } => {
                std::slice::from_ref(message)
            }
        }
    }

    /// Applies the change to a frontend's mirror of the message list, which must be in the state
    /// the change was made against: a change that doesn't fit is a bug in the caller, and panics
    /// like the `Vec` operation it stands for.
    pub fn apply(&self, messages: &mut Vec<Message>) {
        match self {
            Self::Reset { messages: all, .. } => messages.clone_from(all),
            Self::Prepended(older) => {
                messages.splice(0..0, older.iter().cloned());
            }
            Self::Appended(newer) => messages.extend(newer.iter().cloned()),
            Self::Inserted {
                index,
                messages: new,
            } => {
                messages.splice(*index..*index, new.iter().cloned());
            }
            Self::Updated { index, message } => messages[*index] = message.clone(),
            Self::Moved { from, to, message } => {
                messages.remove(*from);
                messages.insert(*to, message.clone());
            }
            Self::Removed { index } => {
                messages.remove(*index);
            }
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
/// the order is the same whichever page a message arrived in. Messages the server hasn't confirmed
/// come after all that it has, in the order they were written: their time is the user's clock,
/// which says nothing about where they fall among the server's.
fn display_order(a: &Message, b: &Message) -> Ordering {
    match (a.is_unsent(), b.is_unsent()) {
        (false, true) => Ordering::Less,
        (true, false) => Ordering::Greater,
        (true, true) => Ordering::Equal,
        (false, false) => a.ts.cmp(&b.ts).then_with(|| a.id.cmp(&b.id)),
    }
}

/// The messages of one room: the ones the server confirmed in display order, then the unsent ones.
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

    /// The message with this id, if it is held.
    #[must_use]
    pub fn message(&self, id: &MessageId) -> Option<&Message> {
        self.history.messages.iter().find(|m| &m.id == id)
    }

    #[must_use]
    pub fn reached_start(&self) -> bool {
        self.history.reached_start
    }

    /// Notes when the messages were last known to be up to date (see
    /// [`RoomHistory::synced_at`]).
    pub fn set_synced_at(&mut self, at: jiff::Timestamp) {
        self.history.synced_at = Some(at);
    }

    /// What to keep, to open the room again later.
    #[must_use]
    pub fn history(&self) -> &RoomHistory {
        &self.history
    }

    /// The server has confirmed at least one of the messages held.
    #[must_use]
    pub fn has_sent(&self) -> bool {
        self.history
            .messages
            .first()
            .is_some_and(|m| !m.is_unsent())
    }

    /// Where the next page of older messages starts.
    #[must_use]
    pub fn oldest(&self) -> Option<jiff::Timestamp> {
        self.history
            .messages
            .first()
            .filter(|m| !m.is_unsent())
            .map(|m| m.ts)
    }

    /// How many messages the server confirmed: the unsent ones come after them.
    fn sent_len(&self) -> usize {
        self.history.messages.partition_point(|m| !m.is_unsent())
    }

    /// Adds a message the user wrote, after everything. It stays last until the server confirms
    /// it (see [`settle`](Self::settle)).
    pub fn add_unsent(&mut self, message: Message) -> TimelineChange {
        self.history.messages.push(message.clone());
        TimelineChange::Appended(vec![message])
    }

    /// Changes the delivery state of an unsent message. `None` if there is no unsent message with
    /// that id or it is in that state already. A message becomes sent through
    /// [`settle`](Self::settle) only.
    pub fn set_delivery(&mut self, id: &MessageId, delivery: Delivery) -> Option<TimelineChange> {
        let index = self.unsent_position(id)?;
        let message = &mut self.history.messages[index];
        if message.delivery == delivery || delivery == Delivery::Sent {
            return None;
        }
        message.delivery = delivery;
        Some(TimelineChange::Updated {
            index,
            message: message.clone(),
        })
    }

    /// Takes in the server's copy of a message that is held as unsent. It replaces the local one
    /// and goes where the server's time puts it among the confirmed messages, which is usually
    /// where it is, so the change is an update; else it is a move. Nothing if no unsent message
    /// has that id.
    pub fn settle(&mut self, mut message: Message) -> Option<TimelineChange> {
        let from = self.unsent_position(&message.id)?;
        message.delivery = Delivery::Sent;
        let sent = self.sent_len();
        self.history.messages.remove(from);
        let to = self.history.messages[..sent]
            .partition_point(|other| display_order(other, &message).is_lt());
        self.history.messages.insert(to, message.clone());
        Some(if to == from {
            TimelineChange::Updated {
                index: from,
                message,
            }
        } else {
            TimelineChange::Moved { from, to, message }
        })
    }

    /// Takes in the server's newer copy of a message that is held as confirmed (an edit, a
    /// reaction...). Nothing if no confirmed message has that id or the copy is the same as the
    /// one held. A copy is whole, so it replaces the held one outright.
    pub fn replace(&mut self, message: Message) -> Option<TimelineChange> {
        let sent = self.sent_len();
        let from = self.history.messages[..sent]
            .iter()
            .position(|m| m.id == message.id)?;
        if self.history.messages[from] == message {
            return None;
        }
        self.history.messages.remove(from);
        let to = self.history.messages[..sent - 1]
            .partition_point(|other| display_order(other, &message).is_lt());
        self.history.messages.insert(to, message.clone());
        Some(if to == from {
            TimelineChange::Updated {
                index: from,
                message,
            }
        } else {
            TimelineChange::Moved { from, to, message }
        })
    }

    /// Takes a deleted message out. Nothing if it isn't held. An unsent message goes too: the
    /// server's answer for it, if it comes, finds nothing to settle.
    pub fn remove(&mut self, id: &MessageId) -> Option<TimelineChange> {
        let index = self.history.messages.iter().position(|m| &m.id == id)?;
        self.history.messages.remove(index);
        Some(TimelineChange::Removed { index })
    }

    fn unsent_position(&self, id: &MessageId) -> Option<usize> {
        self.history
            .messages
            .iter()
            .position(|m| &m.id == id && m.is_unsent())
    }

    /// Settles the messages of `page` that are held as unsent and replaces those held as confirmed
    /// whose copy differs (they were edited meanwhile), and returns the others.
    fn settle_page(&mut self, page: Vec<Message>) -> (Vec<TimelineChange>, Vec<Message>) {
        let mut changes = Vec::new();
        let mut rest = Vec::new();
        for message in page {
            if self.unsent_position(&message.id).is_some() {
                changes.extend(self.settle(message));
            } else if self.message(&message.id).is_some() {
                changes.extend(self.replace(message));
            } else {
                rest.push(message);
            }
        }
        (changes, rest)
    }

    /// Takes in the newest page of the room. If it doesn't touch what is held and there is more
    /// history before it, the confirmed messages held are dropped for the page: keeping both would
    /// leave a hole in the middle that nothing ever fills. Unsent messages are never dropped.
    pub fn merge_latest(&mut self, page: Vec<Message>, reached_start: bool) -> Vec<TimelineChange> {
        let sent = self.sent_len();
        let held: HashSet<_> = self.history.messages[..sent]
            .iter()
            .map(|m| &m.id)
            .collect();
        let touches = page.iter().any(|m| held.contains(&m.id));
        let replaces = sent > 0 && !touches && !reached_start;
        if replaces {
            self.history.messages.drain(..sent);
            self.history.reached_start = false;
        }
        let (mut changes, rest) = self.settle_page(page);
        changes.extend(self.merge(rest, reached_start));
        if replaces {
            return vec![self.reset()];
        }
        changes
    }

    /// Takes in a page of messages older than those held.
    pub fn merge_older(&mut self, page: Vec<Message>, reached_start: bool) -> Vec<TimelineChange> {
        let (mut changes, rest) = self.settle_page(page);
        changes.extend(self.merge(rest, reached_start));
        changes
    }

    /// Takes in a message the server pushed. It is the server's copy of an unsent one if the ids
    /// match, else it is inserted after the confirmed messages, which is almost always where it
    /// belongs; one that isn't the newest takes the same way in as a page would, except that one
    /// older than everything held is dropped while older messages exist: it is an edit of a
    /// message far back, and showing it would leave a hole between it and the rest. A copy of a
    /// confirmed message that is held replaces it (an edit, a reaction), or changes nothing if it
    /// is the same.
    pub fn merge_live(&mut self, message: Message) -> Vec<TimelineChange> {
        if self.unsent_position(&message.id).is_some() {
            return self.settle(message).into_iter().collect();
        }
        if self.message(&message.id).is_some() {
            return self.replace(message).into_iter().collect();
        }
        let sent = self.sent_len();
        let newest = self.history.messages[..sent]
            .last()
            .is_none_or(|last| display_order(&message, last).is_gt());
        if !newest {
            let before_all = self.history.messages[..sent]
                .first()
                .is_some_and(|first| display_order(&message, first).is_lt());
            if before_all && !self.history.reached_start {
                return Vec::new();
            }
            return self.merge(vec![message], false);
        }
        self.history.messages.insert(sent, message.clone());
        vec![self.after_confirmed(sent, vec![message])]
    }

    /// The change for `fresh` having been inserted at `index`, the end of the confirmed messages.
    fn after_confirmed(&self, index: usize, fresh: Vec<Message>) -> TimelineChange {
        if index + fresh.len() == self.history.messages.len() {
            TimelineChange::Appended(fresh)
        } else {
            TimelineChange::Inserted {
                index,
                messages: fresh,
            }
        }
    }

    /// Adds the messages not held yet, in order. The change is a prepend, or an insert after the
    /// confirmed messages, when they all fall on one side of the confirmed messages held, else
    /// (never in practice) a reset.
    fn merge(&mut self, page: Vec<Message>, reached_start: bool) -> Vec<TimelineChange> {
        let mut seen: HashSet<_> = self.history.messages.iter().map(|m| m.id.clone()).collect();
        let mut fresh: Vec<Message> = page
            .into_iter()
            .filter(|m| seen.insert(m.id.clone()))
            .collect();
        fresh.sort_by(display_order);
        // The start is known whether or not the page brought anything new.
        self.history.reached_start |= reached_start;
        let (Some(first), Some(last)) = (fresh.first(), fresh.last()) else {
            return Vec::new();
        };
        let sent = self.sent_len();
        let (older, newer) = match (
            self.history.messages[..sent].first(),
            self.history.messages[..sent].last(),
        ) {
            (Some(held_first), Some(held_last)) => (
                display_order(last, held_first).is_lt(),
                display_order(first, held_last).is_gt(),
            ),
            // Nothing confirmed is held, so the page goes in front of the unsent messages, if any.
            _ => (false, !self.history.messages.is_empty()),
        };
        if older {
            self.history.messages.splice(0..0, fresh.iter().cloned());
            vec![TimelineChange::Prepended(fresh)]
        } else if newer {
            self.history
                .messages
                .splice(sent..sent, fresh.iter().cloned());
            vec![self.after_confirmed(sent, fresh)]
        } else {
            self.history.messages.extend(fresh);
            self.history.messages.sort_by(display_order);
            vec![self.reset()]
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
    use crate::domain::{Author, MessageKind, Reaction};

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
            kind: MessageKind::Text(id.into()),
            delivery: Delivery::Sent,
            attachments: Vec::new(),
            quotes: Vec::new(),
            edited: false,
            reactions: Vec::new(),
        }
    }

    /// The message edited: same id and time, other text.
    fn edited(id: &str, secs: i64) -> Message {
        Message {
            kind: MessageKind::Text(format!("{id} edited").as_str().into()),
            edited: true,
            ..msg(id, secs)
        }
    }

    fn reacted(id: &str, secs: i64) -> Message {
        Message {
            reactions: vec![Reaction {
                emoji: ":thumbsup:".into(),
                usernames: vec!["u".into()],
            }],
            ..msg(id, secs)
        }
    }

    /// A message the user wrote, not confirmed yet.
    fn unsent(id: &str, secs: i64) -> Message {
        Message {
            delivery: Delivery::Pending,
            ..msg(id, secs)
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

    fn id(text: &str) -> MessageId {
        text.to_owned().into()
    }

    // MSG-001
    #[test]
    fn the_first_page_resets_and_sorts() {
        let mut timeline = empty();
        let changes = timeline.merge_latest(vec![msg("c", 3), msg("a", 1), msg("b", 2)], false);
        assert_eq!(ids(timeline.messages()), ["a", "b", "c"]);
        assert_eq!(
            changes,
            [TimelineChange::Reset {
                room: Some("r".to_owned().into()),
                messages: timeline.messages().to_vec()
            }]
        );
        assert!(!timeline.reached_start());
        assert_eq!(timeline.oldest(), Some(msg("a", 1).ts));
    }

    // MSG-001
    #[test]
    fn an_empty_first_page_is_no_change_but_may_reach_the_start() {
        let mut timeline = empty();
        assert_eq!(timeline.merge_latest(Vec::new(), true), []);
        assert!(timeline.reached_start());
    }

    // MSG-002
    #[test]
    fn older_messages_are_prepended_without_the_duplicates() {
        let mut timeline = timeline_of(vec![msg("c", 3), msg("d", 4)]);
        // The page starts at the oldest held message, inclusive.
        let changes = timeline.merge_older(vec![msg("c", 3), msg("b", 2), msg("a", 1)], false);
        assert_eq!(
            changes,
            [TimelineChange::Prepended(vec![msg("a", 1), msg("b", 2)])]
        );
        assert_eq!(ids(timeline.messages()), ["a", "b", "c", "d"]);
    }

    // MSG-002
    #[test]
    fn messages_sharing_a_millisecond_are_ordered_by_id() {
        let mut timeline = timeline_of(vec![msg("b", 5), msg("c", 5)]);
        let changes = timeline.merge_older(vec![msg("b", 5), msg("a", 5), msg("z", 4)], false);
        assert_eq!(
            changes,
            [TimelineChange::Prepended(vec![msg("z", 4), msg("a", 5)])]
        );
        assert_eq!(ids(timeline.messages()), ["z", "a", "b", "c"]);
    }

    // MSG-002
    #[test]
    fn a_page_with_nothing_new_changes_nothing_but_can_reach_the_start() {
        let mut timeline = timeline_of(vec![msg("a", 1)]);
        assert_eq!(timeline.merge_older(vec![msg("a", 1)], true), []);
        assert!(timeline.reached_start());
        assert_eq!(ids(timeline.messages()), ["a"]);
    }

    // MSG-001
    #[test]
    fn newer_messages_are_appended() {
        let mut timeline = timeline_of(vec![msg("a", 1), msg("b", 2)]);
        let changes = timeline.merge_latest(vec![msg("c", 3), msg("b", 2)], false);
        assert_eq!(changes, [TimelineChange::Appended(vec![msg("c", 3)])]);
        assert_eq!(ids(timeline.messages()), ["a", "b", "c"]);
    }

    // MSG-001
    #[test]
    fn a_newest_page_that_misses_what_is_held_replaces_it() {
        let mut timeline = timeline_of(vec![msg("a", 1), msg("b", 2)]);
        let changes = timeline.merge_latest(vec![msg("x", 8), msg("y", 9)], false);
        assert_eq!(ids(timeline.messages()), ["x", "y"]);
        assert!(matches!(changes.as_slice(), [TimelineChange::Reset { .. }]));
        assert!(!timeline.reached_start());
    }

    // MSG-001
    #[test]
    fn a_newest_page_that_reaches_the_start_joins_what_is_held() {
        let mut timeline = timeline_of(vec![msg("b", 2)]);
        let changes = timeline.merge_latest(vec![msg("c", 3), msg("a", 1)], true);
        // The page has messages on both sides: not a prepend or an append.
        assert!(matches!(changes.as_slice(), [TimelineChange::Reset { .. }]));
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
                synced_at: None,
            },
        );
        timeline.merge_latest(vec![msg("a", 1), msg("b", 2)], false);
        assert!(timeline.reached_start());
    }

    // RT-001
    #[test]
    fn a_live_message_is_appended_even_to_an_empty_timeline() {
        let mut timeline = empty();
        assert_eq!(
            timeline.merge_live(msg("a", 1)),
            [TimelineChange::Appended(vec![msg("a", 1)])]
        );
        assert_eq!(
            timeline.merge_live(msg("b", 2)),
            [TimelineChange::Appended(vec![msg("b", 2)])]
        );
        assert_eq!(ids(timeline.messages()), ["a", "b"]);
    }

    // RT-001
    #[test]
    fn a_live_message_already_held_is_ignored() {
        let mut timeline = timeline_of(vec![msg("a", 1), msg("b", 2)]);
        assert_eq!(timeline.merge_live(msg("a", 1)), []);
        assert_eq!(timeline.merge_live(msg("b", 2)), []);
        assert_eq!(ids(timeline.messages()), ["a", "b"]);
    }

    // RT-001
    #[test]
    fn a_live_message_out_of_order_goes_where_it_belongs() {
        let mut timeline = timeline_of(vec![msg("a", 1), msg("c", 3)]);
        // Between two held messages: a reset, as a page would give.
        let changes = timeline.merge_live(msg("b", 2));
        assert!(matches!(changes.as_slice(), [TimelineChange::Reset { .. }]));
        assert_eq!(ids(timeline.messages()), ["a", "b", "c"]);
        // Same instant as the newest one: ordered by id.
        assert_eq!(
            timeline.merge_live(msg("d", 3)),
            [TimelineChange::Appended(vec![msg("d", 3)])]
        );
        assert!(matches!(
            timeline.merge_live(msg("cc", 3)).as_slice(),
            [TimelineChange::Reset { .. }]
        ));
        assert_eq!(ids(timeline.messages()), ["a", "b", "c", "cc", "d"]);
    }

    /// Applies each action's changes to the mirror, which must then equal the timeline.
    fn step(
        timeline: &mut Timeline,
        mirror: &mut Vec<Message>,
        action: impl FnOnce(&mut Timeline) -> Vec<TimelineChange>,
    ) {
        for change in action(timeline) {
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
        step(t, m, |t| t.merge_live(msg("zz", 95)));
        step(t, m, |t| t.merge_live(msg("w", 85)));
        step(t, m, |t| t.merge_live(msg("zz", 95)));
        assert_eq!(ids(m), ["x", "w", "y", "z", "zz"]);
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

    // SEND-001
    #[test]
    fn an_unsent_message_goes_last() {
        let mut timeline = timeline_of(vec![msg("a", 1), msg("b", 2)]);
        let change = timeline.add_unsent(unsent("p", 1));
        assert_eq!(change, TimelineChange::Appended(vec![unsent("p", 1)]));
        assert_eq!(ids(timeline.messages()), ["a", "b", "p"]);
    }

    // SEND-001
    #[test]
    fn a_live_message_goes_in_front_of_the_unsent_ones() {
        let mut timeline = timeline_of(vec![msg("a", 1)]);
        timeline.add_unsent(unsent("p", 1));
        timeline.add_unsent(unsent("q", 1));
        // Newer than everything the server confirmed, though older than the user's clock says.
        let changes = timeline.merge_live(msg("b", 2));
        assert_eq!(
            changes,
            [TimelineChange::Inserted {
                index: 1,
                messages: vec![msg("b", 2)]
            }]
        );
        assert_eq!(ids(timeline.messages()), ["a", "b", "p", "q"]);
    }

    // SEND-001
    #[test]
    fn a_page_goes_in_front_of_the_unsent_ones_even_when_nothing_else_is_held() {
        let mut timeline = empty();
        timeline.add_unsent(unsent("p", 9));
        let changes = timeline.merge_latest(vec![msg("a", 1), msg("b", 2)], false);
        assert_eq!(
            changes,
            [TimelineChange::Inserted {
                index: 0,
                messages: vec![msg("a", 1), msg("b", 2)]
            }]
        );
        assert_eq!(ids(timeline.messages()), ["a", "b", "p"]);
        assert!(timeline.has_sent());
        assert_eq!(timeline.oldest(), Some(msg("a", 1).ts));
    }

    // SEND-001
    #[test]
    fn only_unsent_messages_are_no_start_for_the_next_page() {
        let mut timeline = empty();
        assert!(!timeline.has_sent());
        timeline.add_unsent(unsent("p", 1));
        assert!(!timeline.has_sent());
        assert_eq!(timeline.oldest(), None);
    }

    // SEND-001
    #[test]
    fn older_pages_go_before_everything_with_unsent_messages_held() {
        let mut timeline = timeline_of(vec![msg("c", 3)]);
        timeline.add_unsent(unsent("p", 1));
        let changes = timeline.merge_older(vec![msg("a", 1), msg("b", 2)], true);
        assert_eq!(
            changes,
            [TimelineChange::Prepended(vec![msg("a", 1), msg("b", 2)])]
        );
        assert_eq!(ids(timeline.messages()), ["a", "b", "c", "p"]);
    }

    // SEND-001
    #[test]
    fn the_server_copy_updates_an_unsent_message_in_place() {
        let mut timeline = timeline_of(vec![msg("a", 1)]);
        timeline.add_unsent(unsent("p", 5));
        let change = timeline.settle(msg("p", 6));
        assert_eq!(
            change,
            Some(TimelineChange::Updated {
                index: 1,
                message: msg("p", 6)
            })
        );
        assert_eq!(timeline.messages()[1], msg("p", 6));
    }

    // SEND-001
    #[test]
    fn a_confirmed_message_moves_ahead_of_the_unsent_ones_after_it() {
        let mut timeline = timeline_of(vec![msg("a", 1)]);
        timeline.add_unsent(unsent("p", 5));
        timeline.add_unsent(unsent("q", 6));
        let change = timeline.settle(msg("q", 7));
        assert_eq!(
            change,
            Some(TimelineChange::Moved {
                from: 2,
                to: 1,
                message: msg("q", 7)
            })
        );
        assert_eq!(ids(timeline.messages()), ["a", "q", "p"]);
    }

    // SEND-001
    #[test]
    fn a_confirmed_message_moves_to_where_the_server_time_puts_it() {
        let mut timeline = timeline_of(vec![msg("a", 1), msg("c", 5), msg("d", 8)]);
        timeline.add_unsent(unsent("p", 9));
        // The server's clock is behind: its time falls among the confirmed messages.
        let change = timeline.settle(msg("p", 4));
        assert_eq!(
            change,
            Some(TimelineChange::Moved {
                from: 3,
                to: 1,
                message: msg("p", 4)
            })
        );
        assert_eq!(ids(timeline.messages()), ["a", "p", "c", "d"]);
    }

    // SEND-001
    #[test]
    fn settling_ignores_what_is_not_unsent() {
        let mut timeline = timeline_of(vec![msg("a", 1)]);
        assert_eq!(timeline.settle(msg("a", 1)), None);
        assert_eq!(timeline.settle(msg("nope", 1)), None);
    }

    // SEND-001
    #[test]
    fn a_pushed_copy_of_an_unsent_message_settles_it() {
        let mut timeline = timeline_of(vec![msg("a", 1)]);
        timeline.add_unsent(unsent("p", 5));
        let changes = timeline.merge_live(msg("p", 6));
        assert_eq!(
            changes,
            [TimelineChange::Updated {
                index: 1,
                message: msg("p", 6)
            }]
        );
        // The REST answer comes after: nothing left to settle.
        assert_eq!(timeline.settle(msg("p", 6)), None);
        assert_eq!(timeline.merge_live(msg("p", 6)), []);
        assert_eq!(ids(timeline.messages()), ["a", "p"]);
    }

    // SEND-001
    #[test]
    fn a_page_with_the_copy_of_an_unsent_message_settles_it() {
        let mut timeline = timeline_of(vec![msg("a", 1)]);
        timeline.add_unsent(unsent("p", 5));
        let changes = timeline.merge_latest(vec![msg("p", 6), msg("b", 3), msg("a", 1)], false);
        assert_eq!(timeline.messages()[2], msg("p", 6));
        assert_eq!(ids(timeline.messages()), ["a", "b", "p"]);
        assert!(timeline.messages().iter().all(|m| !m.is_unsent()));
        assert_eq!(changes.len(), 2);
    }

    // SEND-002
    #[test]
    fn unsent_messages_survive_a_page_that_replaces_the_rest() {
        let mut timeline = timeline_of(vec![msg("a", 1), msg("b", 2)]);
        timeline.add_unsent(unsent("p", 3));
        let changes = timeline.merge_latest(vec![msg("x", 8), msg("y", 9)], false);
        assert_eq!(ids(timeline.messages()), ["x", "y", "p"]);
        assert!(matches!(changes.as_slice(), [TimelineChange::Reset { .. }]));
        assert!(timeline.messages()[2].is_unsent());
    }

    // SEND-002
    #[test]
    fn a_failed_message_that_the_server_has_after_all_is_settled_by_a_page() {
        let mut timeline = timeline_of(vec![msg("a", 1), msg("b", 2)]);
        timeline.add_unsent(unsent("p", 3));
        timeline.set_delivery(&id("p"), Delivery::Failed);
        // Nothing the page has is held: the page replaces the confirmed messages, and `p` is in it.
        timeline.merge_latest(vec![msg("p", 9), msg("y", 8)], false);
        assert_eq!(ids(timeline.messages()), ["y", "p"]);
        assert!(timeline.messages().iter().all(|m| !m.is_unsent()));
    }

    // SEND-002
    #[test]
    fn the_delivery_state_changes_in_place() {
        let mut timeline = timeline_of(vec![msg("a", 1)]);
        timeline.add_unsent(unsent("p", 5));
        let failed = Message {
            delivery: Delivery::Failed,
            ..unsent("p", 5)
        };
        assert_eq!(
            timeline.set_delivery(&id("p"), Delivery::Failed),
            Some(TimelineChange::Updated {
                index: 1,
                message: failed.clone()
            })
        );
        assert_eq!(timeline.set_delivery(&id("p"), Delivery::Failed), None);
        assert_eq!(
            timeline.set_delivery(&id("p"), Delivery::Pending),
            Some(TimelineChange::Updated {
                index: 1,
                message: unsent("p", 5)
            })
        );
    }

    // SEND-002
    #[test]
    fn only_unsent_messages_change_their_delivery_state() {
        let mut timeline = timeline_of(vec![msg("a", 1)]);
        timeline.add_unsent(unsent("p", 5));
        assert_eq!(timeline.set_delivery(&id("a"), Delivery::Failed), None);
        assert_eq!(timeline.set_delivery(&id("nope"), Delivery::Failed), None);
        assert_eq!(timeline.set_delivery(&id("p"), Delivery::Sent), None);
        assert_eq!(timeline.messages()[0], msg("a", 1));
        assert_eq!(timeline.messages()[1].delivery, Delivery::Pending);
    }

    // SEND-001, SEND-002
    #[test]
    fn a_mirror_following_the_send_changes_equals_the_timeline() {
        let (mut timeline, mut mirror) = (empty(), Vec::new());
        let (t, m) = (&mut timeline, &mut mirror);
        step(t, m, |t| {
            t.merge_latest(vec![msg("a", 1), msg("b", 2)], false)
        });
        step(t, m, |t| vec![t.add_unsent(unsent("p", 10))]);
        step(t, m, |t| vec![t.add_unsent(unsent("q", 11))]);
        step(t, m, |t| vec![t.add_unsent(unsent("r", 12))]);
        // Someone else's message arrives meanwhile.
        step(t, m, |t| t.merge_live(msg("c", 3)));
        step(t, m, |t| {
            t.set_delivery(&id("q"), Delivery::Failed)
                .into_iter()
                .collect()
        });
        // The middle one is confirmed first, with a time before the others.
        step(t, m, |t| t.settle(msg("q", 2)).into_iter().collect());
        assert_eq!(ids(m), ["a", "b", "q", "c", "p", "r"]);
        step(t, m, |t| t.merge_live(msg("p", 13)));
        step(t, m, |t| t.settle(msg("r", 14)).into_iter().collect());
        step(t, m, |t| {
            t.merge_latest(vec![msg("d", 15), msg("r", 14)], false)
        });
        assert_eq!(ids(m), ["a", "b", "q", "c", "p", "r", "d"]);
        assert!(timeline.messages().iter().all(|m| !m.is_unsent()));
    }

    // RICH-006
    #[test]
    fn a_live_edit_replaces_the_held_message_in_place() {
        let mut timeline = timeline_of(vec![msg("a", 1), msg("b", 2), msg("c", 3)]);
        let changes = timeline.merge_live(edited("b", 2));
        assert_eq!(
            changes,
            [TimelineChange::Updated {
                index: 1,
                message: edited("b", 2)
            }]
        );
        assert_eq!(timeline.messages()[1], edited("b", 2));
        assert_eq!(ids(timeline.messages()), ["a", "b", "c"]);
    }

    // RICH-007
    #[test]
    fn a_live_reaction_replaces_the_held_message_and_removing_it_replaces_it_back() {
        let mut timeline = timeline_of(vec![msg("a", 1), msg("b", 2)]);
        assert_eq!(
            timeline.merge_live(reacted("b", 2)),
            [TimelineChange::Updated {
                index: 1,
                message: reacted("b", 2)
            }]
        );
        // The server's copy without reactions: they are gone, not merged.
        assert_eq!(
            timeline.merge_live(msg("b", 2)),
            [TimelineChange::Updated {
                index: 1,
                message: msg("b", 2)
            }]
        );
        assert_eq!(timeline.messages()[1].reactions, []);
    }

    // RICH-006
    #[test]
    fn a_live_copy_equal_to_the_held_message_changes_nothing() {
        let mut timeline = timeline_of(vec![msg("a", 1), reacted("b", 2)]);
        assert_eq!(timeline.merge_live(msg("a", 1)), []);
        assert_eq!(timeline.merge_live(reacted("b", 2)), []);
    }

    // RICH-006
    #[test]
    fn a_page_with_an_edited_copy_of_a_held_message_replaces_it() {
        let mut timeline = timeline_of(vec![msg("a", 1), msg("b", 2)]);
        let changes = timeline.merge_latest(vec![msg("c", 3), edited("b", 2), msg("a", 1)], false);
        assert_eq!(
            changes,
            [
                TimelineChange::Updated {
                    index: 1,
                    message: edited("b", 2)
                },
                TimelineChange::Appended(vec![msg("c", 3)]),
            ]
        );
        assert_eq!(timeline.messages()[1], edited("b", 2));
        let changes = timeline.merge_older(vec![edited("a", 1)], true);
        assert_eq!(
            changes,
            [TimelineChange::Updated {
                index: 0,
                message: edited("a", 1)
            }]
        );
    }

    // RICH-006
    #[test]
    fn a_page_with_identical_copies_changes_nothing() {
        let mut timeline = timeline_of(vec![msg("a", 1), reacted("b", 2)]);
        assert_eq!(
            timeline.merge_latest(vec![reacted("b", 2), msg("a", 1)], false),
            []
        );
    }

    // RICH-006
    #[test]
    fn a_live_edit_of_a_message_older_than_what_is_held_is_dropped_unless_the_start_is_held() {
        let mut timeline = timeline_of(vec![msg("c", 3), msg("d", 4)]);
        // Older messages exist but aren't held: inserting it would leave a hole.
        assert_eq!(timeline.merge_live(edited("a", 1)), []);
        assert_eq!(ids(timeline.messages()), ["c", "d"]);
        // Once the start is held, everything older is held too, so it is a message like any other.
        timeline.merge_older(vec![msg("c", 3)], true);
        assert_eq!(
            timeline.merge_live(edited("a", 1)),
            [TimelineChange::Prepended(vec![edited("a", 1)])]
        );
    }

    // RICH-006
    #[test]
    fn a_live_edit_between_held_messages_that_is_not_held_goes_in_by_time() {
        let mut timeline = timeline_of(vec![msg("a", 1), msg("c", 3)]);
        let changes = timeline.merge_live(edited("b", 2));
        assert!(matches!(changes.as_slice(), [TimelineChange::Reset { .. }]));
        assert_eq!(ids(timeline.messages()), ["a", "b", "c"]);
    }

    // RICH-006
    #[test]
    fn an_edit_leaves_the_unsent_messages_alone() {
        let mut timeline = timeline_of(vec![msg("a", 1), msg("b", 2)]);
        timeline.add_unsent(unsent("p", 10));
        let changes = timeline.merge_live(edited("a", 1));
        assert_eq!(
            changes,
            [TimelineChange::Updated {
                index: 0,
                message: edited("a", 1)
            }]
        );
        assert_eq!(ids(timeline.messages()), ["a", "b", "p"]);
        assert!(timeline.messages()[2].is_unsent());
    }

    // RICH-006
    #[test]
    fn a_replacement_that_changes_the_time_moves_the_message() {
        let mut timeline = timeline_of(vec![msg("a", 1), msg("b", 2), msg("c", 3)]);
        let moved = Message {
            ts: jiff::Timestamp::from_second(5).unwrap(),
            ..edited("a", 1)
        };
        let changes = timeline.replace(moved.clone());
        assert_eq!(
            changes,
            Some(TimelineChange::Moved {
                from: 0,
                to: 2,
                message: moved
            })
        );
        assert_eq!(ids(timeline.messages()), ["b", "c", "a"]);
    }

    // RICH-006
    #[test]
    fn a_deleted_message_is_removed() {
        let mut timeline = timeline_of(vec![msg("a", 1), msg("b", 2), msg("c", 3)]);
        assert_eq!(
            timeline.remove(&id("b")),
            Some(TimelineChange::Removed { index: 1 })
        );
        assert_eq!(ids(timeline.messages()), ["a", "c"]);
        assert_eq!(timeline.remove(&id("b")), None);
        assert_eq!(timeline.remove(&id("nope")), None);
    }

    // RICH-006
    #[test]
    fn deleting_the_oldest_message_moves_where_the_next_page_starts() {
        let mut timeline = timeline_of(vec![msg("a", 1), msg("b", 2)]);
        timeline.remove(&id("a"));
        assert_eq!(timeline.oldest(), Some(msg("b", 2).ts));
        timeline.remove(&id("b"));
        assert_eq!(timeline.oldest(), None);
    }

    // RICH-006
    #[test]
    fn a_deleted_unsent_message_is_removed_and_its_answer_settles_nothing() {
        let mut timeline = timeline_of(vec![msg("a", 1)]);
        timeline.add_unsent(unsent("p", 10));
        assert_eq!(
            timeline.remove(&id("p")),
            Some(TimelineChange::Removed { index: 1 })
        );
        assert_eq!(timeline.settle(msg("p", 11)), None);
    }

    // RICH-006, RICH-007
    #[test]
    fn a_mirror_following_edits_and_deletions_equals_the_timeline() {
        let (mut timeline, mut mirror) = (empty(), Vec::new());
        let (t, m) = (&mut timeline, &mut mirror);
        step(t, m, |t| {
            t.merge_latest(vec![msg("a", 1), msg("b", 2), msg("c", 3)], false)
        });
        step(t, m, |t| vec![t.add_unsent(unsent("p", 10))]);
        step(t, m, |t| t.merge_live(edited("b", 2)));
        step(t, m, |t| t.merge_live(reacted("c", 3)));
        step(t, m, |t| t.remove(&id("a")).into_iter().collect());
        step(t, m, |t| t.merge_live(msg("d", 4)));
        step(t, m, |t| t.remove(&id("p")).into_iter().collect());
        step(t, m, |t| t.remove(&id("d")).into_iter().collect());
        step(t, m, |t| {
            t.merge_latest(vec![edited("c", 3), edited("b", 2)], false)
        });
        assert_eq!(ids(m), ["b", "c"]);
        assert!(m[0].edited && m[1].edited);
    }
}
