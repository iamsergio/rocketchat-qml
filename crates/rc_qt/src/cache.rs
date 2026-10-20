// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! What the hub remembers of the core's events, so that a view model created late can catch up.
//! Plain Rust, so it can be unit-tested without Qt or a [`Client`](rc_core::Client).

use rc_core::AvatarUpdate;
use rc_core::domain::{AvatarOwner, Message, Room, RoomId};
use rc_core::emoji::CustomEmojis;
use rc_core::rooms::RoomChange;
use rc_core::session::SessionState;
use rc_core::timeline::{TimelineChange, TimelineStatus};
use rc_core::{Connection, Event};

use crate::avatar_view::AvatarUrls;
use crate::rich_html::Viewer;
use crate::room_view::{RoomState, room_state};
use crate::session_view::{SessionView, session_view};

/// The latest session state, a mirror of the room list and a mirror of the open room's messages.
#[derive(Debug)]
pub struct Cache {
    /// `None` before the first event.
    state: Option<SessionState>,
    connection: Connection,
    rooms: Vec<Room>,
    /// The room `messages` belong to.
    room: Option<RoomId>,
    messages: Vec<Message>,
    status: TimelineStatus,
    /// The logged-in user, if any.
    username: Option<String>,
    /// `None` until the core sends some: a `HashMap` can't be built in `new`.
    emoji: Option<CustomEmojis>,
    /// `None` until the core sends some, like `emoji`.
    avatars: Option<AvatarUrls>,
}

impl Default for Cache {
    fn default() -> Self {
        Self::new()
    }
}

impl Cache {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: None,
            connection: Connection::Connected,
            rooms: Vec::new(),
            room: None,
            messages: Vec::new(),
            status: TimelineStatus {
                room: None,
                loading: false,
                loading_older: false,
                reached_start: false,
                error: None,
            },
            username: None,
            emoji: None,
            avatars: None,
        }
    }

    /// Takes the event in, as the core emits them: in order.
    pub fn apply(&mut self, event: &Event) {
        match event {
            Event::Session(state) => {
                self.state = Some(state.clone());
                self.username = match state {
                    SessionState::LoggedIn(account) => Some(account.user.username.clone()),
                    _ => None,
                };
                // A session starts connected, and the core says nothing about it then.
                self.connection = Connection::Connected;
            }
            Event::Connection(connection) => self.connection = *connection,
            Event::CustomEmoji(emoji) => self.emoji = Some(emoji.clone()),
            Event::Avatars(updates) => {
                self.avatars.get_or_insert_default().apply(updates);
            }
            Event::Rooms(changes) => {
                for change in changes {
                    change.apply(&mut self.rooms);
                }
            }
            Event::Timeline(changes) => {
                for change in changes {
                    change.apply(&mut self.messages);
                    if let TimelineChange::Reset { room, .. } = change {
                        self.room.clone_from(room);
                    }
                }
            }
            Event::TimelineStatus(status) => self.status = status.clone(),
            _ => {}
        }
    }

    /// What the `Session` view model shows now.
    #[must_use]
    pub fn session_view(&self) -> SessionView {
        session_view(self.state.as_ref(), self.connection)
    }

    /// The change that brings an empty model to the current room list.
    #[must_use]
    pub fn rooms_snapshot(&self) -> RoomChange {
        RoomChange::Reset(self.rooms.clone())
    }

    /// The change that brings an empty message model to the current messages.
    #[must_use]
    pub fn timeline_snapshot(&self) -> TimelineChange {
        TimelineChange::Reset {
            room: self.room.clone(),
            messages: self.messages.clone(),
        }
    }

    /// Who the messages are shown to.
    #[must_use]
    pub fn viewer(&self) -> Viewer {
        Viewer {
            username: self.username.clone(),
            emoji: self.emoji.clone().unwrap_or_default(),
        }
    }

    /// The avatar files known now, as URLs.
    #[must_use]
    pub fn avatars(&self) -> AvatarUrls {
        self.avatars.clone().unwrap_or_default()
    }

    /// The URLs, as they are now, of the owners of `updates` (empty for an avatar that is gone).
    #[must_use]
    pub fn avatar_changes(&self, updates: &[AvatarUpdate]) -> Vec<(AvatarOwner, String)> {
        let urls = self.avatars();
        updates
            .iter()
            .map(|update| (update.owner.clone(), urls.url(&update.owner).to_owned()))
            .collect()
    }

    /// What the `Room` view model shows now.
    #[must_use]
    pub fn room_state(&self) -> RoomState {
        room_state(&self.rooms, &self.status)
    }
}

#[cfg(test)]
mod tests {
    use rc_core::domain::{Account, Author, Delivery, MessageKind, RoomKind, User};
    use rc_core::ports::RoomHistory;
    use rc_core::rooms::RoomList;
    use rc_core::timeline::Timeline;

    use super::*;

    fn room(id: &str, name: &str) -> Room {
        Room {
            id: id.to_owned().into(),
            kind: RoomKind::Channel,
            name: name.to_owned(),
            peer: None,
            avatar_etag: None,
            unread: 0,
            user_mentions: 0,
            group_mentions: 0,
            alert: false,
            last_activity: jiff::Timestamp::UNIX_EPOCH,
        }
    }

    /// Events as the core would emit them for a session that lists, adds, renames and removes
    /// rooms, and logs out.
    fn script() -> Vec<Event> {
        let mut list = RoomList::default();
        let rooms = |change: Option<RoomChange>| Event::Rooms(change.into_iter().collect());
        vec![
            Event::Session(SessionState::LoggedOut { failure: None }),
            Event::Session(SessionState::Connecting),
            Event::Rooms(vec![
                list.reset(vec![room("a", "alpha"), room("c", "charlie")]),
            ]),
            rooms(list.upsert(room("b", "bravo"))),
            rooms(list.upsert(room("a", "zulu"))),
            rooms(list.remove(&"c".to_owned().into())),
            Event::Session(SessionState::LoggedOut { failure: None }),
            Event::Rooms(vec![list.reset(Vec::new())]),
        ]
    }

    // AUTH-001
    #[test]
    fn remembers_the_latest_session_state() {
        let mut cache = Cache::new();
        assert_eq!(cache.state, None);
        cache.apply(&Event::Session(SessionState::Connecting));
        assert_eq!(cache.state.as_ref(), Some(&SessionState::Connecting));
        cache.apply(&Event::Session(SessionState::LoggedOut { failure: None }));
        assert_eq!(
            cache.state.as_ref(),
            Some(&SessionState::LoggedOut { failure: None })
        );
    }

    fn logged_in(username: &str) -> Event {
        Event::Session(SessionState::LoggedIn(Account {
            server: "http://h".to_owned(),
            user: User {
                id: "u".to_owned().into(),
                username: username.to_owned(),
                display_name: None,
            },
        }))
    }

    // RICH-002, RICH-003
    #[test]
    fn the_viewer_is_the_logged_in_user_with_the_custom_emoji_of_the_server() {
        let emoji = CustomEmojis::new(
            "http://h",
            &[rc_api::CustomEmoji {
                name: "party".into(),
                aliases: Vec::new(),
                extension: "gif".into(),
            }],
        );
        let mut cache = Cache::new();
        assert_eq!(cache.viewer(), Viewer::default());
        cache.apply(&logged_in("alice"));
        cache.apply(&Event::CustomEmoji(emoji.clone()));
        assert_eq!(
            cache.viewer(),
            Viewer {
                username: Some("alice".into()),
                emoji
            }
        );
        cache.apply(&Event::Session(SessionState::LoggedOut { failure: None }));
        cache.apply(&Event::CustomEmoji(CustomEmojis::default()));
        assert_eq!(cache.viewer(), Viewer::default());
    }

    // RT-004
    #[test]
    fn the_connection_is_remembered_until_the_session_changes() {
        use std::time::Duration;

        let mut cache = Cache::new();
        cache.apply(&Event::Session(SessionState::LoggedOut { failure: None }));
        assert_eq!(cache.session_view().connection_text, "");
        cache.apply(&Event::Session(SessionState::LoggedIn(Account {
            server: "http://h".to_owned(),
            user: User {
                id: "u".to_owned().into(),
                username: "alice".to_owned(),
                display_name: None,
            },
        })));
        cache.apply(&Event::Connection(Connection::Waiting {
            retry_in: Duration::from_secs(3),
        }));
        assert_eq!(
            cache.session_view().connection_text,
            "Connection lost, reconnecting in 3s…"
        );
        cache.apply(&Event::Connection(Connection::Connected));
        assert_eq!(cache.session_view().connection_text, "");
        cache.apply(&Event::Connection(Connection::Reconnecting));
        cache.apply(&Event::Session(SessionState::LoggedOut { failure: None }));
        assert_eq!(cache.connection, Connection::Connected);
    }

    // ROOM-001
    #[test]
    fn a_snapshot_taken_at_any_point_followed_by_the_rest_equals_the_replay() {
        let events = script();
        let mut replay = Vec::new();
        let mut full = Cache::new();
        for event in &events {
            full.apply(event);
            if let Event::Rooms(changes) = event {
                for change in changes {
                    change.apply(&mut replay);
                }
            }
        }

        // A view model registering after `taken` events starts from the snapshot and then gets
        // the later changes one by one.
        for taken in 0..=events.len() {
            let mut cache = Cache::new();
            for event in &events[..taken] {
                cache.apply(event);
            }
            let mut mirror = Vec::new();
            cache.rooms_snapshot().apply(&mut mirror);
            for event in &events[taken..] {
                if let Event::Rooms(changes) = event {
                    for change in changes {
                        change.apply(&mut mirror);
                    }
                }
            }
            assert_eq!(mirror, replay, "registered after {taken} events");
        }
        assert_eq!(full.rooms_snapshot(), RoomChange::Reset(replay));
    }

    fn message(id: &str, secs: i64) -> Message {
        Message {
            id: id.to_owned().into(),
            room_id: "a".to_owned().into(),
            author: Author {
                id: "u".to_owned().into(),
                username: "u".into(),
                display_name: None,
            },
            ts: jiff::Timestamp::from_second(secs).unwrap(),
            kind: MessageKind::Text(id.into()),
            delivery: Delivery::Sent,
        }
    }

    /// Events as the core would emit them for opening a room, loading older messages, receiving
    /// newer ones, switching rooms and logging out.
    fn timeline_script() -> Vec<Event> {
        let status = |room: &str, loading: bool, older: bool, start: bool| {
            Event::TimelineStatus(TimelineStatus {
                room: Some(room.to_owned().into()),
                loading,
                loading_older: older,
                reached_start: start,
                error: None,
            })
        };
        let mut timeline = Timeline::new("a".to_owned().into(), RoomHistory::default());
        let mut events = vec![
            Event::Timeline(vec![timeline.reset()]),
            status("a", true, false, false),
        ];
        let page = |ids: &[(&str, i64)]| ids.iter().map(|(id, s)| message(id, *s)).collect();
        events.push(Event::Timeline(
            timeline
                .merge_latest(page(&[("d", 4), ("c", 3)]), false)
                .into_iter()
                .collect(),
        ));
        events.push(status("a", false, false, false));
        events.push(status("a", false, true, false));
        events.push(Event::Timeline(
            timeline
                .merge_older(page(&[("c", 3), ("b", 2), ("a", 1)]), true)
                .into_iter()
                .collect(),
        ));
        events.push(status("a", false, false, true));
        events.push(Event::Timeline(
            timeline
                .merge_latest(page(&[("e", 5), ("d", 4)]), true)
                .into_iter()
                .collect(),
        ));
        // A message is sent, someone else's comes in front of it, and the server confirms it with
        // a time that puts it before that one.
        let unsent = Message {
            delivery: Delivery::Pending,
            ..message("p", 60)
        };
        events.push(Event::Timeline(vec![timeline.add_unsent(unsent)]));
        events.push(Event::Timeline(timeline.merge_live(message("f", 70))));
        events.push(Event::Timeline(
            timeline.settle(message("p", 65)).into_iter().collect(),
        ));
        events.push(Event::Timeline(vec![TimelineChange::Reset {
            room: None,
            messages: Vec::new(),
        }]));
        events.push(Event::TimelineStatus(TimelineStatus::default()));
        events
    }

    // MSG-001, MSG-002, SEND-001
    #[test]
    fn a_timeline_snapshot_taken_at_any_point_followed_by_the_rest_equals_the_replay() {
        let events = timeline_script();
        let mut replay = Vec::new();
        for event in &events {
            if let Event::Timeline(changes) = event {
                for change in changes {
                    change.apply(&mut replay);
                }
            }
        }
        for taken in 0..=events.len() {
            let mut cache = Cache::new();
            events[..taken].iter().for_each(|event| cache.apply(event));
            let mut mirror = Vec::new();
            cache.timeline_snapshot().apply(&mut mirror);
            for event in &events[taken..] {
                if let Event::Timeline(changes) = event {
                    for change in changes {
                        change.apply(&mut mirror);
                    }
                }
            }
            assert_eq!(mirror, replay, "registered after {taken} events");
        }
    }

    // MSG-001, MSG-002
    #[test]
    fn the_snapshot_holds_the_open_rooms_messages() {
        let mut cache = Cache::new();
        timeline_script()[..6]
            .iter()
            .for_each(|event| cache.apply(event));
        let TimelineChange::Reset { room, messages } = cache.timeline_snapshot() else {
            panic!("not a reset");
        };
        assert_eq!(room, Some("a".to_owned().into()));
        let ids: Vec<_> = messages.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["a", "b", "c", "d"]);
    }

    // MSG-001
    #[test]
    fn the_room_state_follows_the_status_and_the_room_list() {
        let mut cache = Cache::new();
        assert_eq!(cache.room_state(), RoomState::default());
        cache.apply(&Event::Rooms(vec![RoomChange::Reset(vec![room(
            "a", "alpha",
        )])]));
        cache.apply(&Event::TimelineStatus(TimelineStatus {
            room: Some("a".to_owned().into()),
            loading: true,
            ..TimelineStatus::default()
        }));
        let state = cache.room_state();
        assert_eq!(
            (state.room_id.as_str(), state.name.as_str()),
            ("a", "alpha")
        );
        assert!(state.loading);
        // The room list changing the name changes the state too.
        cache.apply(&Event::Rooms(vec![RoomChange::Reset(vec![room(
            "a", "beta",
        )])]));
        assert_eq!(cache.room_state().name, "beta");
        cache.apply(&Event::TimelineStatus(TimelineStatus::default()));
        assert_eq!(cache.room_state(), RoomState::default());
    }

    fn avatar(username: &str, path: Option<&str>) -> AvatarUpdate {
        AvatarUpdate {
            owner: AvatarOwner::User(username.to_owned()),
            path: path.map(std::path::PathBuf::from),
        }
    }

    // RICH-004
    #[test]
    fn a_model_created_late_gets_the_avatars_known_so_far() {
        let mut cache = Cache::new();
        assert_eq!(cache.avatars(), AvatarUrls::default());
        cache.apply(&Event::Avatars(vec![avatar("alice", Some("/c/alice.png"))]));
        cache.apply(&Event::Avatars(vec![
            avatar("bob", Some("/c/bob.svg")),
            avatar("alice", None),
        ]));
        let urls = cache.avatars();
        assert_eq!(urls.url(&AvatarOwner::User("alice".into())), "");
        assert_eq!(
            urls.url(&AvatarOwner::User("bob".into())),
            "file:///c/bob.svg?v=1"
        );
    }

    // RICH-004
    #[test]
    fn the_changes_for_the_models_carry_the_urls_as_they_are_now() {
        let mut cache = Cache::new();
        let first = [avatar("alice", Some("/c/alice.png"))];
        cache.apply(&Event::Avatars(first.to_vec()));
        let changes = cache.avatar_changes(&first);
        assert_eq!(changes[0].1, "file:///c/alice.png?v=1");
        // The same file again, replaced: another URL.
        cache.apply(&Event::Avatars(first.to_vec()));
        assert_eq!(cache.avatar_changes(&first)[0].1, "file:///c/alice.png?v=2");
        let gone = [avatar("alice", None)];
        cache.apply(&Event::Avatars(gone.to_vec()));
        assert_eq!(cache.avatar_changes(&gone)[0].1, "");
    }
}
