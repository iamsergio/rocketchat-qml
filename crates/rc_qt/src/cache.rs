// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! What the hub remembers of the core's events, so that a view model created late can catch up.
//! Plain Rust, so it can be unit-tested without Qt or a [`Client`](rc_core::Client).

use rc_core::domain::{AccountId, AvatarOwner, Identity, Message, Room, RoomId, UserId};
use rc_core::emoji::CustomEmojis;
use rc_core::policy::MessagePolicy;
use rc_core::rooms::{RoomChange, Unread};
use rc_core::session::SessionState;
use rc_core::timeline::{ThreadStatus, TimelineChange, TimelineStatus};
use rc_core::{AvatarUpdate, ImageUpdate};
use rc_core::{Connection, Event};

use crate::accounts::{AccountView, account_status};
use crate::attachment_view::ImageUrls;
use crate::avatar_view::AvatarUrls;
use crate::message_view::{author_name, body, can_edit};
use crate::rich_html::Viewer;
use crate::room_view::{RoomState, room_state};
use crate::session_view::{SessionView, host, session_view};
use crate::thread_view::{ThreadState, thread_state};

/// One account's: its identity and unread totals, the latest session state, a mirror of the room list and mirrors of the open room's messages and
/// of the open thread's.
#[derive(Debug)]
pub struct Cache {
    /// `None` before the first event.
    state: Option<SessionState>,
    connection: Connection,
    /// Who the account is. Unlike `username` and `server` it holds while logged out, and it is
    /// never unset.
    identity: Option<Identity>,
    unread: Unread,
    rooms: Vec<Room>,
    /// The room `messages` belong to.
    room: Option<RoomId>,
    messages: Vec<Message>,
    status: TimelineStatus,
    /// The open thread's messages: the parent, then the replies. Empty when none is open.
    thread_messages: Vec<Message>,
    thread_status: ThreadStatus,
    /// The logged-in user, if any.
    username: Option<String>,
    user_id: Option<UserId>,
    /// The URL the user logged in with.
    server: Option<String>,
    /// `None` until the core sends some, like `emoji`: then everything is allowed.
    policy: Option<MessagePolicy>,
    /// `None` until the core sends some: a `HashMap` can't be built in `new`.
    emoji: Option<CustomEmojis>,
    /// `None` until the core sends some, like `emoji`.
    avatars: Option<AvatarUrls>,
    /// `None` until the core sends some, like `emoji`.
    images: Option<ImageUrls>,
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
            identity: None,
            unread: Unread {
                rooms: 0,
                mentions: 0,
            },
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
            thread_messages: Vec::new(),
            thread_status: ThreadStatus {
                parent: None,
                loading: false,
                loading_older: false,
                reached_start: false,
                error: None,
            },
            username: None,
            user_id: None,
            server: None,
            policy: None,
            emoji: None,
            avatars: None,
            images: None,
        }
    }

    /// Takes the event in, as the core emits them: in order.
    pub fn apply(&mut self, event: &Event) {
        match event {
            Event::Session(state) => {
                self.state = Some(state.clone());
                (self.username, self.user_id, self.server) = match state {
                    SessionState::LoggedIn(account) => (
                        Some(account.user.username.clone()),
                        Some(account.user.id.clone()),
                        Some(account.server.clone()),
                    ),
                    _ => (None, None, None),
                };
                // A session starts connected, and the core says nothing about it then.
                self.connection = Connection::Connected;
            }
            Event::Connection(connection) => self.connection = *connection,
            Event::Identity(identity) => self.identity = Some(identity.clone()),
            Event::Unread(unread) => self.unread = *unread,
            Event::CustomEmoji(emoji) => self.emoji = Some(emoji.clone()),
            Event::MessagePolicy(policy) => self.policy = Some(*policy),
            Event::Avatars(updates) => {
                self.avatars.get_or_insert_default().apply(updates);
            }
            Event::Images(updates) => {
                self.images.get_or_insert_default().apply(updates);
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
            Event::ThreadTimeline(changes) => {
                for change in changes {
                    change.apply(&mut self.thread_messages);
                }
            }
            Event::ThreadStatus(status) => self.thread_status = status.clone(),
            _ => {}
        }
    }

    /// Whether the account has said what state it is in: an account added a moment ago hasn't.
    #[must_use]
    pub fn session_known(&self) -> bool {
        self.state.is_some()
    }

    /// What the `Session` view model shows now.
    #[must_use]
    pub fn session_view(&self) -> SessionView {
        session_view(self.state.as_ref(), self.connection)
    }

    /// What the account list shows of the account `id`, `current` or not.
    #[must_use]
    pub fn account_view(&self, id: &AccountId, current: bool) -> AccountView {
        let (server, username) = self
            .identity
            .as_ref()
            .map_or((String::new(), String::new()), |identity| {
                (identity.server.clone(), identity.username.clone())
            });
        AccountView {
            id: id.as_str().to_owned(),
            host: host(&server),
            server,
            username,
            status: account_status(self.state.as_ref()),
            connected: matches!(self.state, Some(SessionState::LoggedIn(_)))
                && self.connection == Connection::Connected,
            unread_rooms: self.unread.rooms,
            mentions: self.unread.mentions,
            current,
        }
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

    /// The change that brings an empty thread model to the current thread messages.
    #[must_use]
    pub fn thread_snapshot(&self) -> TimelineChange {
        TimelineChange::Reset {
            room: self.room.clone(),
            messages: self.thread_messages.clone(),
        }
    }

    /// Who the messages are shown to.
    #[must_use]
    pub fn viewer(&self) -> Viewer {
        Viewer {
            username: self.username.clone(),
            server: self.server.clone(),
            emoji: self.emoji.clone().unwrap_or_default(),
            user_id: self.user_id.clone(),
            policy: self.policy.unwrap_or_default(),
        }
    }

    /// The newest message of the open room that the user can edit now, if any.
    #[must_use]
    pub fn last_editable(&self, now: jiff::Timestamp) -> Option<&Message> {
        newest_editable(&self.messages, &self.viewer(), now)
    }

    /// The newest message of the open thread (the parent counts) that the user can edit now, if
    /// any.
    #[must_use]
    pub fn thread_last_editable(&self, now: jiff::Timestamp) -> Option<&Message> {
        newest_editable(&self.thread_messages, &self.viewer(), now)
    }

    /// A held message of the open room or the open thread, which hold some messages in common.
    fn held(&self, id: &str) -> Option<&Message> {
        self.messages
            .iter()
            .chain(&self.thread_messages)
            .find(|message| message.id.as_str() == id)
    }

    /// The text of a message of the open room or thread as its author wrote it, if it is held.
    #[must_use]
    pub fn message_text(&self, id: &str) -> Option<&str> {
        self.held(id).map(body)
    }

    /// Who wrote a message of the open room or thread, by display name else username, if it is
    /// held.
    #[must_use]
    pub fn message_author(&self, id: &str) -> Option<&str> {
        self.held(id).map(|message| author_name(&message.author))
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

    /// The attachment image files known now, as URLs.
    #[must_use]
    pub fn images(&self) -> ImageUrls {
        self.images.clone().unwrap_or_default()
    }

    /// The URLs, as they are now, of the images of `updates` (empty for a file that is gone).
    #[must_use]
    pub fn image_changes(&self, updates: &[ImageUpdate]) -> Vec<(String, String)> {
        let urls = self.images();
        updates
            .iter()
            .map(|update| (update.path.clone(), urls.url(&update.path).to_owned()))
            .collect()
    }

    /// What the `Room` view model shows now.
    #[must_use]
    pub fn room_state(&self) -> RoomState {
        room_state(&self.rooms, &self.status)
    }

    /// What the `Thread` view model shows now.
    #[must_use]
    pub fn thread_state(&self) -> ThreadState {
        thread_state(&self.rooms, self.status.room.as_ref(), &self.thread_status)
    }
}

/// The newest of `messages` (held oldest first) that `viewer` can edit at `now`.
fn newest_editable<'a>(
    messages: &'a [Message],
    viewer: &Viewer,
    now: jiff::Timestamp,
) -> Option<&'a Message> {
    messages
        .iter()
        .rev()
        .find(|message| can_edit(message, viewer, now))
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
            favorite: false,
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
                server: Some("http://h".into()),
                emoji,
                user_id: Some("u".to_owned().into()),
                policy: MessagePolicy::default(),
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
            attachments: Vec::new(),
            quotes: Vec::new(),
            edited: false,
            reactions: Vec::new(),
            thread_id: None,
            thread_shown: false,
            thread_replies: 0,
            thread_last_reply: None,
        }
    }

    fn from_other(id: &str, secs: i64) -> Message {
        let mut message = message(id, secs);
        message.author.id = "other".to_owned().into();
        message
    }

    /// Room `a` with the messages `a` (own), `b` (somebody else's), `c` (own) and `d` (somebody
    /// else's), oldest first, shown to `alice` (id `u`).
    fn cache_with_four_messages() -> Cache {
        let mut cache = Cache::new();
        cache.apply(&logged_in("alice"));
        cache.apply(&Event::Timeline(vec![TimelineChange::Reset {
            room: Some("a".to_owned().into()),
            messages: vec![
                message("a", 1),
                from_other("b", 2),
                message("c", 3),
                from_other("d", 4),
            ],
        }]));
        cache
    }

    // ACT-001
    #[test]
    fn the_newest_message_the_user_can_edit_is_the_last_editable() {
        let mut cache = cache_with_four_messages();
        let now = jiff::Timestamp::from_second(100).unwrap();
        assert_eq!(cache.last_editable(now).map(|m| m.id.as_str()), Some("c"));

        // With a time limit, it counts from when the message was sent.
        cache.apply(&Event::MessagePolicy(MessagePolicy::from(
            rc_api::MessageSettings {
                edit_minutes: 1,
                ..rc_api::MessageSettings::default()
            },
        )));
        let soon = jiff::Timestamp::from_second(3 + 60).unwrap();
        assert_eq!(cache.last_editable(soon).map(|m| m.id.as_str()), Some("c"));
        let later = jiff::Timestamp::from_second(3 + 61).unwrap();
        assert_eq!(cache.last_editable(later), None);
    }

    // ACT-001
    #[test]
    fn nothing_is_editable_when_editing_is_off_or_nobody_is_logged_in() {
        let mut cache = cache_with_four_messages();
        let now = jiff::Timestamp::from_second(100).unwrap();
        cache.apply(&Event::MessagePolicy(MessagePolicy::from(
            rc_api::MessageSettings {
                editing_allowed: false,
                ..rc_api::MessageSettings::default()
            },
        )));
        assert_eq!(cache.last_editable(now), None);

        let mut cache = cache_with_four_messages();
        cache.apply(&Event::Session(SessionState::LoggedOut { failure: None }));
        assert_eq!(cache.last_editable(now), None);
    }

    // ACT-001
    #[test]
    fn the_text_of_a_held_message_is_given_as_written() {
        let cache = cache_with_four_messages();
        assert_eq!(cache.message_text("c"), Some("c"));
        assert_eq!(cache.message_text("nope"), None);
    }

    // ACT-005
    #[test]
    fn the_author_of_a_held_message_is_given_by_name() {
        let cache = cache_with_four_messages();
        assert_eq!(cache.message_author("c"), Some("u"));
        assert_eq!(cache.message_author("nope"), None);
    }

    // ACT-001, ACT-002
    #[test]
    fn the_viewer_carries_the_users_id_and_the_policy() {
        let mut cache = cache_with_four_messages();
        let policy = MessagePolicy::from(rc_api::MessageSettings {
            deleting_allowed: false,
            ..rc_api::MessageSettings::default()
        });
        cache.apply(&Event::MessagePolicy(policy));
        let viewer = cache.viewer();
        assert_eq!(viewer.user_id, Some("u".to_owned().into()));
        assert_eq!(viewer.policy, policy);
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

    /// A reply to `parent` in the thread, sent by somebody else.
    fn reply(id: &str, secs: i64) -> Message {
        let mut reply = from_other(id, secs);
        reply.thread_id = Some("p".to_owned().into());
        reply
    }

    /// Events as the core would emit them for opening a thread, loading its replies, receiving
    /// one, a reply being edited and deleted, and the thread closing.
    fn thread_script() -> Vec<Event> {
        let status = |parent: Option<&str>, loading: bool, start: bool| {
            Event::ThreadStatus(ThreadStatus {
                parent: parent.map(|id| id.to_owned().into()),
                loading,
                loading_older: false,
                reached_start: start,
                error: None,
            })
        };
        let room = Some("a".to_owned().into());
        let mut edited = reply("r2", 3);
        edited.edited = true;
        vec![
            Event::ThreadTimeline(vec![TimelineChange::Reset {
                room: room.clone(),
                messages: Vec::new(),
            }]),
            status(Some("p"), true, false),
            Event::ThreadTimeline(vec![TimelineChange::Appended(vec![
                reply("r2", 3),
                reply("r3", 4),
            ])]),
            status(Some("p"), false, false),
            Event::ThreadTimeline(vec![TimelineChange::Prepended(vec![
                message("p", 1),
                reply("r1", 2),
            ])]),
            status(Some("p"), false, true),
            Event::ThreadTimeline(vec![TimelineChange::Appended(vec![reply("r4", 5)])]),
            Event::ThreadTimeline(vec![TimelineChange::Updated {
                index: 2,
                message: edited,
            }]),
            Event::ThreadTimeline(vec![TimelineChange::Removed { index: 3 }]),
            Event::ThreadTimeline(vec![TimelineChange::Reset {
                room,
                messages: Vec::new(),
            }]),
            status(None, false, false),
        ]
    }

    // ACT-004
    #[test]
    fn a_thread_snapshot_taken_at_any_point_followed_by_the_rest_equals_the_replay() {
        let events = thread_script();
        let mut replay = Vec::new();
        for event in &events {
            if let Event::ThreadTimeline(changes) = event {
                for change in changes {
                    change.apply(&mut replay);
                }
            }
        }
        for taken in 0..=events.len() {
            let mut cache = Cache::new();
            events[..taken].iter().for_each(|event| cache.apply(event));
            let mut mirror = Vec::new();
            cache.thread_snapshot().apply(&mut mirror);
            for event in &events[taken..] {
                if let Event::ThreadTimeline(changes) = event {
                    for change in changes {
                        change.apply(&mut mirror);
                    }
                }
            }
            assert_eq!(mirror, replay, "registered after {taken} events");
        }
    }

    // ACT-004
    #[test]
    fn the_thread_snapshot_holds_the_open_threads_messages_and_not_the_rooms() {
        let mut cache = cache_with_four_messages();
        thread_script()[..7]
            .iter()
            .for_each(|event| cache.apply(event));
        let TimelineChange::Reset { room, messages } = cache.thread_snapshot() else {
            panic!("not a reset");
        };
        assert_eq!(room, Some("a".to_owned().into()));
        let ids: Vec<_> = messages.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["p", "r1", "r2", "r3", "r4"]);
        // The room's own snapshot is as it was.
        let TimelineChange::Reset { messages, .. } = cache.timeline_snapshot() else {
            panic!("not a reset");
        };
        assert_eq!(messages.len(), 4);
    }

    // ACT-004
    #[test]
    fn the_text_and_author_of_a_message_in_the_thread_are_given_too() {
        let mut cache = cache_with_four_messages();
        let mut text_reply = reply("r1", 5);
        text_reply.kind = MessageKind::Text("in the thread".into());
        cache.apply(&Event::ThreadTimeline(vec![TimelineChange::Reset {
            room: Some("a".to_owned().into()),
            messages: vec![message("a", 1), text_reply],
        }]));
        assert_eq!(cache.message_text("r1"), Some("in the thread"));
        assert_eq!(cache.message_author("r1"), Some("u"));
        // The room's messages still are.
        assert_eq!(cache.message_text("b"), Some("b"));
        assert_eq!(cache.message_text("a"), Some("a"));
        assert_eq!(cache.message_text("nope"), None);
        // Closed, the thread holds nothing.
        cache.apply(&Event::ThreadTimeline(vec![TimelineChange::Reset {
            room: Some("a".to_owned().into()),
            messages: Vec::new(),
        }]));
        assert_eq!(cache.message_text("r1"), None);
        assert_eq!(cache.message_author("r1"), None);
    }

    // ACT-004
    #[test]
    fn the_newest_message_the_user_can_edit_in_the_thread_is_the_last_editable_there() {
        let mut cache = cache_with_four_messages();
        let now = jiff::Timestamp::from_second(100).unwrap();
        assert_eq!(cache.thread_last_editable(now), None);
        let mut own_reply = reply("r2", 4);
        own_reply.author.id = "u".to_owned().into();
        cache.apply(&Event::ThreadTimeline(vec![TimelineChange::Reset {
            room: Some("a".to_owned().into()),
            messages: vec![message("p", 1), reply("r1", 2), own_reply, reply("r3", 5)],
        }]));
        // The room's newest editable message is another one.
        assert_eq!(
            cache.thread_last_editable(now).map(|m| m.id.as_str()),
            Some("r2")
        );
        assert_eq!(cache.last_editable(now).map(|m| m.id.as_str()), Some("c"));
        cache.apply(&Event::Session(SessionState::LoggedOut { failure: None }));
        assert_eq!(cache.thread_last_editable(now), None);
    }

    // ACT-004
    #[test]
    fn the_thread_state_follows_the_status_and_the_names_of_the_open_room() {
        let mut cache = Cache::new();
        assert_eq!(cache.thread_state(), ThreadState::default());
        cache.apply(&Event::Rooms(vec![RoomChange::Reset(vec![room(
            "a", "alpha",
        )])]));
        cache.apply(&Event::TimelineStatus(TimelineStatus {
            room: Some("a".to_owned().into()),
            ..TimelineStatus::default()
        }));
        // A room without a thread.
        assert_eq!(cache.thread_state().room_name, "alpha");
        assert_eq!(cache.thread_state().parent_id, "");
        cache.apply(&Event::ThreadStatus(ThreadStatus {
            parent: Some("p".to_owned().into()),
            loading: true,
            ..ThreadStatus::default()
        }));
        let state = cache.thread_state();
        assert_eq!(
            (state.parent_id.as_str(), state.room_id.as_str()),
            ("p", "a")
        );
        assert!(state.loading);
        // The room list renaming the room changes the state too.
        cache.apply(&Event::Rooms(vec![RoomChange::Reset(vec![room(
            "a", "beta",
        )])]));
        assert_eq!(cache.thread_state().room_name, "beta");
        cache.apply(&Event::ThreadStatus(ThreadStatus::default()));
        assert_eq!(cache.thread_state().parent_id, "");
    }

    fn avatar(username: &str, path: Option<&str>) -> AvatarUpdate {
        AvatarUpdate {
            owner: AvatarOwner::User(username.to_owned()),
            path: path.map(std::path::PathBuf::from),
        }
    }

    fn image(path: &str, file: Option<&str>) -> ImageUpdate {
        ImageUpdate {
            path: path.to_owned(),
            file: file.map(std::path::PathBuf::from),
        }
    }

    // RICH-005
    #[test]
    fn a_model_created_late_gets_the_images_known_so_far() {
        let mut cache = Cache::new();
        assert_eq!(cache.images(), ImageUrls::default());
        cache.apply(&Event::Images(vec![image("/a", Some("/c/a.png"))]));
        cache.apply(&Event::Images(vec![
            image("/b", Some("/c/b.png")),
            image("/a", None),
        ]));
        let urls = cache.images();
        assert_eq!(urls.url("/a"), "");
        assert_eq!(urls.url("/b"), "file:///c/b.png?v=1");
        assert_eq!(
            cache.image_changes(&[image("/b", Some("/c/b.png")), image("/a", None)]),
            [
                ("/b".to_owned(), "file:///c/b.png?v=1".to_owned()),
                ("/a".to_owned(), String::new())
            ]
        );
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
