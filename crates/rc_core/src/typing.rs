// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Who is typing in the open room, and in its threads.

use std::time::Duration;

use crate::domain::MessageId;

/// How long a typist counts after their last event. The web client emits again every 5 s while its
/// user types, so a stop that never came (a closed tab, a lost connection) ends after this.
pub(crate) const EXPIRES_AFTER: Duration = Duration::from_secs(15);

/// Where someone types. The web client sends one activity per place: a user typing in the thread
/// pane is in that thread and not in the room, and one who types in both has two entries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Place {
    Room,
    /// The thread whose parent message has this id.
    Thread(MessageId),
}

#[derive(Debug)]
struct Typist {
    username: String,
    place: Place,
    expires: jiff::Timestamp,
}

/// The typists of one room, in the order they started. Time is passed in, so that the caller
/// decides what the clock says.
#[derive(Debug, Default)]
pub(crate) struct Typists {
    typists: Vec<Typist>,
}

impl Typists {
    /// `username` types in `place`. One who types already keeps their place in the order and their
    /// time starts over.
    pub(crate) fn start(&mut self, username: &str, place: &Place, now: jiff::Timestamp) {
        let expires = now
            .checked_add(EXPIRES_AFTER)
            .unwrap_or(jiff::Timestamp::MAX);
        match self
            .typists
            .iter_mut()
            .find(|t| t.username == username && &t.place == place)
        {
            Some(typist) => typist.expires = expires,
            None => self.typists.push(Typist {
                username: username.to_owned(),
                place: place.clone(),
                expires,
            }),
        }
    }

    /// `username` stopped typing in `place`, or sent a message there. Does nothing if they weren't
    /// typing.
    pub(crate) fn stop(&mut self, username: &str, place: &Place) {
        self.typists
            .retain(|t| !(t.username == username && &t.place == place));
    }

    /// Forgets the typists whose time is up at `now`.
    pub(crate) fn expire(&mut self, now: jiff::Timestamp) {
        self.typists.retain(|t| t.expires > now);
    }

    /// Forgets everyone.
    pub(crate) fn clear(&mut self) {
        self.typists.clear();
    }

    /// When the next typist expires. `None` if nobody types.
    pub(crate) fn next_deadline(&self) -> Option<jiff::Timestamp> {
        self.typists.iter().map(|t| t.expires).min()
    }

    /// The usernames of the typists in `place`, in the order they started.
    pub(crate) fn usernames(&self, place: &Place) -> Vec<&str> {
        self.typists
            .iter()
            .filter(|t| &t.place == place)
            .map(|t| t.username.as_str())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(secs: i64) -> jiff::Timestamp {
        jiff::Timestamp::from_second(secs).unwrap()
    }

    fn thread(id: &str) -> Place {
        Place::Thread(id.to_owned().into())
    }

    // ACT-006
    #[test]
    fn typists_are_listed_in_the_order_they_started() {
        let mut typists = Typists::default();
        typists.start("bob", &Place::Room, at(0));
        typists.start("alice", &Place::Room, at(1));
        assert_eq!(typists.usernames(&Place::Room), ["bob", "alice"]);
    }

    // ACT-006
    #[test]
    fn a_repeated_start_keeps_the_order_and_renews_the_time() {
        let mut typists = Typists::default();
        typists.start("bob", &Place::Room, at(0));
        typists.start("alice", &Place::Room, at(1));
        typists.start("bob", &Place::Room, at(10));
        assert_eq!(typists.usernames(&Place::Room), ["bob", "alice"]);
        // Alice's time is up at 16, Bob's, renewed, at 25.
        assert_eq!(typists.next_deadline(), Some(at(16)));
        typists.expire(at(16));
        assert_eq!(typists.usernames(&Place::Room), ["bob"]);
        assert_eq!(typists.next_deadline(), Some(at(25)));
    }

    // ACT-006
    #[test]
    fn a_typist_expires_after_fifteen_seconds_and_not_before() {
        let mut typists = Typists::default();
        typists.start("bob", &Place::Room, at(100));
        typists.expire(at(114));
        assert_eq!(typists.usernames(&Place::Room), ["bob"]);
        typists.expire(at(115));
        assert!(typists.usernames(&Place::Room).is_empty());
        assert_eq!(typists.next_deadline(), None);
    }

    // ACT-006
    #[test]
    fn the_room_and_each_thread_are_separate_places() {
        let mut typists = Typists::default();
        typists.start("bob", &Place::Room, at(0));
        typists.start("bob", &thread("t1"), at(0));
        typists.start("alice", &thread("t2"), at(0));
        assert_eq!(typists.usernames(&Place::Room), ["bob"]);
        assert_eq!(typists.usernames(&thread("t1")), ["bob"]);
        assert_eq!(typists.usernames(&thread("t2")), ["alice"]);

        typists.stop("bob", &thread("t1"));
        assert_eq!(typists.usernames(&Place::Room), ["bob"]);
        assert!(typists.usernames(&thread("t1")).is_empty());
    }

    // ACT-006
    #[test]
    fn stopping_someone_who_does_not_type_changes_nothing() {
        let mut typists = Typists::default();
        typists.start("bob", &Place::Room, at(0));
        typists.stop("alice", &Place::Room);
        typists.stop("bob", &thread("t1"));
        assert_eq!(typists.usernames(&Place::Room), ["bob"]);
    }

    // ACT-006
    #[test]
    fn clearing_forgets_everyone() {
        let mut typists = Typists::default();
        typists.start("bob", &Place::Room, at(0));
        typists.start("alice", &thread("t1"), at(0));
        typists.clear();
        assert_eq!(typists.next_deadline(), None);
        assert!(typists.usernames(&Place::Room).is_empty());
        assert!(typists.usernames(&thread("t1")).is_empty());
    }
}
