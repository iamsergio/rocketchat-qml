// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The list of a room's threads in display order, and the index-based changes that keep a
//! frontend's copy of it in step. Frontends mirror the list and never sort or diff it themselves.

use crate::domain::{Message, MessageId, MessageKind, RoomId, UserId};
use crate::list_change::ListChange;
use crate::timeline::TimelineError;

/// One step for a frontend's mirror of the thread list: the parent messages, the one with the most
/// recent reply first. Apply the changes in order.
pub type ThreadListChange = ListChange<Message>;

/// Which threads of a room the list holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ThreadFilter {
    /// Every thread of the room.
    #[default]
    All,
    /// The threads the user follows: those they started or replied to, until they unfollow.
    Following,
}

/// What the frontend shows next to the thread list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThreadListStatus {
    /// The room whose threads are listed; `None` when the list is closed.
    pub room: Option<RoomId>,
    /// Which threads it holds. Its default, `All`, while the list is closed.
    pub filter: ThreadFilter,
    /// The first page is being fetched.
    pub loading: bool,
    /// A later page is being fetched.
    pub loading_more: bool,
    /// The last thread is in the list.
    pub reached_end: bool,
    /// Why the last request failed; cleared when the next one starts.
    pub error: Option<TimelineError>,
}

/// The threads of a room that the user has the list open on. The server orders them by the time of
/// the last reply, newest first, and so does the list, whichever way the changes reach it.
#[derive(Debug)]
pub(crate) struct ThreadList {
    room: RoomId,
    filter: ThreadFilter,
    rows: Vec<Message>,
    /// The last thread is in `rows`.
    reached_end: bool,
}

impl ThreadList {
    pub(crate) fn new(room: RoomId, filter: ThreadFilter) -> Self {
        Self {
            room,
            filter,
            rows: Vec::new(),
            reached_end: false,
        }
    }

    pub(crate) fn room(&self) -> &RoomId {
        &self.room
    }

    pub(crate) fn filter(&self) -> ThreadFilter {
        self.filter
    }

    pub(crate) fn reached_end(&self) -> bool {
        self.reached_end
    }

    pub(crate) fn rows(&self) -> &[Message] {
        &self.rows
    }

    /// The parent with this id, if the list holds it.
    pub(crate) fn message(&self, id: &MessageId) -> Option<&Message> {
        self.rows.iter().find(|row| &row.id == id)
    }

    /// Whether a message belongs in the list as it stands: a sent text message that is not a reply
    /// and has replies, and for the followed threads one the user follows.
    fn lists(&self, message: &Message, me: Option<&UserId>) -> bool {
        message.room_id == self.room
            && message.thread_id.is_none()
            && message.thread_replies > 0
            && !message.is_unsent()
            && matches!(message.kind, MessageKind::Text(_))
            && match self.filter {
                ThreadFilter::All => true,
                ThreadFilter::Following => {
                    me.is_some_and(|me| message.thread_followers.contains(me))
                }
            }
    }

    /// Where a parent whose last reply is `message`'s goes among the other rows: before the first
    /// one that is not newer, so that of two threads with the same last reply, the one pushed
    /// last is first. `skip` is the row the parent comes from, if it is in the list.
    fn position_of(&self, message: &Message, skip: Option<usize>) -> usize {
        let mut others = (self.rows.iter().enumerate())
            .filter(|(index, _)| Some(*index) != skip)
            .map(|(_, row)| row);
        let count = self.rows.len() - usize::from(skip.is_some());
        // The place among the others is the index once the thread is out of its old one.
        others
            .position(|row| row.thread_last_reply <= message.thread_last_reply)
            .unwrap_or(count)
    }

    /// The first page: the list becomes exactly it.
    pub(crate) fn reset_to(
        &mut self,
        threads: Vec<Message>,
        reached_end: bool,
    ) -> Vec<ThreadListChange> {
        self.rows = threads;
        self.reached_end = reached_end;
        vec![ListChange::Reset(self.rows.clone())]
    }

    /// A later page: the threads not held yet, after the last one. A thread that moved to the front
    /// meanwhile is on the page again.
    pub(crate) fn append(
        &mut self,
        threads: Vec<Message>,
        reached_end: bool,
    ) -> Vec<ThreadListChange> {
        self.reached_end = reached_end;
        let mut changes = Vec::new();
        for thread in threads {
            if self.rows.iter().any(|row| row.id == thread.id) {
                continue;
            }
            changes.push(ListChange::Inserted {
                index: self.rows.len(),
                item: thread.clone(),
            });
            self.rows.push(thread);
        }
        changes
    }

    /// The server's copy of a parent that was pushed, whether it is new or a reply, an edit or
    /// anything else changed it. A thread with a new reply goes to the front. A message that is not
    /// a parent (any more: its replies are gone or it was removed), and for the followed threads
    /// one the user does not follow, leaves the list. A parent that is not held, and whose last
    /// reply is older than the last thread held while later pages are to come, is left to those.
    pub(crate) fn pushed(
        &mut self,
        message: &Message,
        me: Option<&UserId>,
    ) -> Vec<ThreadListChange> {
        let held = self.rows.iter().position(|row| row.id == message.id);
        if !self.lists(message, me) {
            return held
                .map(|index| self.remove_at(index))
                .into_iter()
                .collect();
        }
        let Some(from) = held else {
            let index = self.position_of(message, None);
            if index == self.rows.len() && !self.reached_end {
                return Vec::new();
            }
            self.rows.insert(index, message.clone());
            return vec![ListChange::Inserted {
                index,
                item: message.clone(),
            }];
        };
        // Only a newer reply moves a thread: an edit or a reaction leaves it where it is.
        let moves = message.thread_last_reply > self.rows[from].thread_last_reply;
        let to = if moves {
            self.position_of(message, Some(from))
        } else {
            from
        };
        self.rows[from].clone_from(message);
        if to == from {
            vec![ListChange::Updated {
                index: from,
                item: message.clone(),
            }]
        } else {
            let row = self.rows.remove(from);
            self.rows.insert(to, row);
            vec![ListChange::Moved {
                from,
                to,
                item: message.clone(),
            }]
        }
    }

    /// A newer copy of a message the list holds, from anywhere but a push of the parent (an edit
    /// the user made, a count that went down): same place. The followers are kept if the copy has
    /// none to say.
    pub(crate) fn replaced(&mut self, message: &Message) -> Option<ThreadListChange> {
        let index = self.rows.iter().position(|row| row.id == message.id)?;
        let mut copy = message.clone();
        if copy.thread_followers.is_empty() {
            copy.thread_followers
                .clone_from(&self.rows[index].thread_followers);
        }
        self.rows[index].clone_from(&copy);
        Some(ListChange::Updated { index, item: copy })
    }

    /// The parent with this id is gone.
    pub(crate) fn removed(&mut self, id: &MessageId) -> Option<ThreadListChange> {
        let index = self.rows.iter().position(|row| &row.id == id)?;
        Some(self.remove_at(index))
    }

    fn remove_at(&mut self, index: usize) -> ThreadListChange {
        self.rows.remove(index);
        ListChange::Removed { index }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Author, Delivery};

    fn me() -> UserId {
        UserId::from("me".to_owned())
    }

    /// A parent in room `r` whose last reply is at `last` seconds, followed by `followers`.
    fn parent(id: &str, last: i64, followers: &[&str]) -> Message {
        Message {
            id: id.to_owned().into(),
            room_id: "r".to_owned().into(),
            author: Author {
                id: "uid".to_owned().into(),
                username: "alice".to_owned(),
                display_name: None,
            },
            ts: jiff::Timestamp::from_second(1).unwrap(),
            kind: MessageKind::Text(id.into()),
            delivery: Delivery::Sent,
            attachments: Vec::new(),
            quotes: Vec::new(),
            edited: false,
            reactions: Vec::new(),
            thread_id: None,
            thread_shown: false,
            thread_replies: 1,
            thread_last_reply: jiff::Timestamp::from_second(last).ok(),
            thread_followers: followers
                .iter()
                .map(|id| UserId::from((*id).to_owned()))
                .collect(),
        }
    }

    fn list_of(filter: ThreadFilter, parents: Vec<Message>, reached_end: bool) -> ThreadList {
        let mut list = ThreadList::new("r".to_owned().into(), filter);
        list.reset_to(parents, reached_end);
        list
    }

    fn ids(list: &ThreadList) -> Vec<&str> {
        list.rows().iter().map(|m| m.id.as_str()).collect()
    }

    // ACT-010
    #[test]
    fn a_page_resets_the_list_and_the_next_one_goes_after_it_without_what_is_held() {
        let mut list = ThreadList::new("r".to_owned().into(), ThreadFilter::All);
        let changes = list.reset_to(vec![parent("a", 9, &[]), parent("b", 8, &[])], false);
        assert_eq!(
            changes,
            [ListChange::Reset(vec![
                parent("a", 9, &[]),
                parent("b", 8, &[])
            ])]
        );
        assert!(!list.reached_end());
        // `b` is on the next page again: a thread moved to the front meanwhile shifts the pages.
        let changes = list.append(vec![parent("b", 8, &[]), parent("c", 7, &[])], true);
        assert_eq!(
            changes,
            [ListChange::Inserted {
                index: 2,
                item: parent("c", 7, &[])
            }]
        );
        assert_eq!(ids(&list), ["a", "b", "c"]);
        assert!(list.reached_end());
    }

    // ACT-010
    #[test]
    fn a_new_reply_moves_its_thread_to_the_front() {
        let mut list = list_of(
            ThreadFilter::All,
            vec![
                parent("a", 9, &[]),
                parent("b", 8, &[]),
                parent("c", 7, &[]),
            ],
            true,
        );
        let newer = Message {
            thread_replies: 2,
            ..parent("c", 10, &[])
        };
        let changes = list.pushed(&newer, Some(&me()));
        assert_eq!(
            changes,
            [ListChange::Moved {
                from: 2,
                to: 0,
                item: newer
            }]
        );
        assert_eq!(ids(&list), ["c", "a", "b"]);
        // The front stays the front.
        let again = Message {
            thread_replies: 3,
            ..parent("c", 11, &[])
        };
        assert_eq!(
            list.pushed(&again, Some(&me())),
            [ListChange::Updated {
                index: 0,
                item: again
            }]
        );
    }

    // ACT-010
    #[test]
    fn an_edit_or_a_reaction_changes_the_row_where_it_is() {
        let mut list = list_of(
            ThreadFilter::All,
            vec![parent("a", 9, &[]), parent("b", 8, &[])],
            true,
        );
        let edited = Message {
            edited: true,
            ..parent("b", 8, &[])
        };
        assert_eq!(
            list.pushed(&edited, Some(&me())),
            [ListChange::Updated {
                index: 1,
                item: edited
            }]
        );
    }

    // ACT-010
    #[test]
    fn a_message_that_gets_its_first_reply_comes_in_at_the_front() {
        let mut list = list_of(ThreadFilter::All, vec![parent("a", 9, &[])], false);
        let new = parent("n", 10, &[]);
        assert_eq!(
            list.pushed(&new, Some(&me())),
            [ListChange::Inserted {
                index: 0,
                item: new
            }]
        );
        assert_eq!(ids(&list), ["n", "a"]);
    }

    // ACT-010
    #[test]
    fn an_old_thread_that_is_not_held_is_left_to_the_pages_to_come() {
        let mut list = list_of(ThreadFilter::All, vec![parent("a", 9, &[])], false);
        // An edit of a thread further down: its last reply is older than the last one held.
        assert!(list.pushed(&parent("old", 3, &[]), Some(&me())).is_empty());
        // With every thread held, it would have been there.
        let mut whole = list_of(ThreadFilter::All, vec![parent("a", 9, &[])], true);
        assert_eq!(whole.pushed(&parent("old", 3, &[]), Some(&me())).len(), 1);
        assert_eq!(ids(&whole), ["a", "old"]);
    }

    // ACT-010
    #[test]
    fn only_the_threads_the_user_follows_are_in_the_followed_list() {
        let mut list = list_of(
            ThreadFilter::Following,
            vec![parent("a", 9, &["me"]), parent("b", 8, &["me"])],
            true,
        );
        // Somebody else's thread gets a reply: not for this list.
        assert!(
            list.pushed(&parent("x", 10, &["you"]), Some(&me()))
                .is_empty()
        );
        // One the user replied to comes in.
        assert_eq!(
            list.pushed(&parent("y", 11, &["you", "me"]), Some(&me()))
                .len(),
            1
        );
        assert_eq!(ids(&list), ["y", "a", "b"]);
        // A copy that no longer lists the user takes the thread out.
        assert_eq!(
            list.pushed(&parent("a", 12, &["you"]), Some(&me())),
            [ListChange::Removed { index: 1 }]
        );
        assert_eq!(ids(&list), ["y", "b"]);
        // Nobody is followed by an unknown user.
        assert_eq!(
            list.pushed(&parent("b", 13, &["me"]), None),
            [ListChange::Removed { index: 1 }]
        );
    }

    // ACT-010
    #[test]
    fn a_removed_or_deleted_parent_and_one_without_replies_leave_the_list() {
        let mut list = list_of(
            ThreadFilter::All,
            vec![
                parent("a", 9, &[]),
                parent("b", 8, &[]),
                parent("c", 7, &[]),
            ],
            true,
        );
        assert_eq!(
            list.removed(&"b".to_owned().into()),
            Some(ListChange::Removed { index: 1 })
        );
        assert_eq!(list.removed(&"b".to_owned().into()), None);
        let emptied = Message {
            thread_replies: 0,
            ..parent("c", 7, &[])
        };
        assert_eq!(
            list.pushed(&emptied, Some(&me())),
            [ListChange::Removed { index: 1 }]
        );
        assert_eq!(ids(&list), ["a"]);
    }

    // ACT-010
    #[test]
    fn what_is_not_a_sent_text_parent_of_the_room_is_not_listed() {
        let mut list = list_of(ThreadFilter::All, Vec::new(), true);
        let elsewhere = Message {
            room_id: "other".to_owned().into(),
            ..parent("e", 5, &[])
        };
        let reply = Message {
            thread_id: Some("a".to_owned().into()),
            ..parent("r1", 5, &[])
        };
        let system = Message {
            kind: MessageKind::System(crate::domain::SystemEvent::Other("rm".into())),
            ..parent("s", 5, &[])
        };
        for message in [elsewhere, reply, system] {
            assert!(list.pushed(&message, Some(&me())).is_empty());
        }
    }

    // ACT-010
    #[test]
    fn a_replaced_copy_keeps_its_place_and_the_followers_when_it_has_none() {
        let mut list = list_of(
            ThreadFilter::Following,
            vec![parent("a", 9, &["me"]), parent("b", 8, &["me"])],
            true,
        );
        let fewer = Message {
            thread_replies: 0,
            thread_followers: Vec::new(),
            ..parent("b", 8, &[])
        };
        let change = list.replaced(&fewer).unwrap();
        assert_eq!(
            change,
            ListChange::Updated {
                index: 1,
                item: Message {
                    thread_followers: vec![me()],
                    ..fewer
                }
            }
        );
        assert!(list.replaced(&parent("zzz", 1, &[])).is_none());
    }
}
