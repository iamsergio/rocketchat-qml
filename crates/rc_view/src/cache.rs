// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! What the hub remembers of the core's events, so that a view model created late can catch up.
//! Plain Rust, so it can be unit-tested without Qt or a [`Client`](rc_core::Client).

use std::collections::HashMap;

use rc_core::domain::{AccountId, AvatarOwner, Identity, Message, MessageId, Room, RoomId, UserId};
use rc_core::emoji::CustomEmojis;
use rc_core::policy::MessagePolicy;
use rc_core::rooms::{RoomChange, Unread};
use rc_core::session::{LoginFailure, SessionState};
use rc_core::threads::{ThreadFilter, ThreadListChange, ThreadListStatus};
use rc_core::timeline::{ThreadStatus, TimelineChange, TimelineStatus};
use rc_core::{AvatarUpdate, EmojiImageUpdate, ImageUpdate};
use rc_core::{Connection, Event};

use crate::accounts::{AccountView, Status, account_status, monogram};
use crate::attachment_view::ImageUrls;
use crate::avatar_view::AvatarUrls;
use crate::emoji_image_view::EmojiImageUrls;
use crate::message_view::{author_name, body, can_edit};
use crate::room_view::{RoomState, room_state, typing_line};
use crate::session_view::{LoginMode, SessionView, host, session_view};
use crate::thread_list_view::{ThreadListState, ThreadUnread, thread_list_state};
use crate::thread_view::{ThreadState, thread_state};
use crate::viewer::Viewer;

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
    /// The last time the account settled logged out, it was because its session ended. Stays
    /// while it logs in again, so that the login page doesn't change under the user.
    expired: bool,
    unread: Unread,
    rooms: Vec<Room>,
    /// The rooms the room list holds back, in the order the core sent them.
    quiet_rooms: Vec<Room>,
    /// The room `messages` belong to.
    room: Option<RoomId>,
    messages: Vec<Message>,
    status: TimelineStatus,
    /// The open thread's messages: the parent, then the replies. Empty when none is open.
    thread_messages: Vec<Message>,
    thread_status: ThreadStatus,
    /// The parents the open room's thread list holds, the one with the most recent reply first.
    /// Empty when it is closed.
    thread_list: Vec<Message>,
    thread_list_status: ThreadListStatus,
    /// The filter the thread list was last opened with in each room, which opening it again
    /// without a choice (the back button of a thread) starts with.
    /// `None` until there is one: a `HashMap` can't be built in `new`.
    thread_list_filters: Option<HashMap<RoomId, ThreadFilter>>,
    /// Who the core says is typing, and where: in a room, in a thread (by its parent). Empty
    /// names once nobody does.
    typing: Option<(RoomId, Vec<String>)>,
    thread_typing: Option<(MessageId, Vec<String>)>,
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
    /// `None` until the core sends some, like `emoji`. Only a frontend that asked for them gets
    /// any.
    emoji_images: Option<EmojiImageUrls>,
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
            expired: false,
            unread: Unread {
                rooms: 0,
                mentions: 0,
            },
            rooms: Vec::new(),
            quiet_rooms: Vec::new(),
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
            thread_list: Vec::new(),
            thread_list_status: ThreadListStatus {
                room: None,
                filter: ThreadFilter::All,
                loading: false,
                loading_more: false,
                reached_end: false,
                error: None,
            },
            thread_list_filters: None,
            typing: None,
            thread_typing: None,
            username: None,
            user_id: None,
            server: None,
            policy: None,
            emoji: None,
            avatars: None,
            images: None,
            emoji_images: None,
        }
    }

    /// Takes the event in, as the core emits them: in order.
    pub fn apply(&mut self, event: &Event) {
        match event {
            Event::Session(state) => {
                match state {
                    SessionState::LoggedIn(_) => self.expired = false,
                    SessionState::LoggedOut { failure } => {
                        self.expired = *failure == Some(LoginFailure::SessionExpired);
                    }
                    SessionState::Connecting | SessionState::Resuming => {}
                }
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
            Event::EmojiImages(updates) => {
                self.emoji_images.get_or_insert_default().apply(updates);
            }
            Event::Rooms(changes) => {
                for change in changes {
                    change.apply(&mut self.rooms);
                }
            }
            Event::QuietRooms(rooms) => self.quiet_rooms.clone_from(rooms),
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
            Event::ThreadList(changes) => {
                for change in changes {
                    change.apply(&mut self.thread_list);
                }
            }
            Event::ThreadListStatus(status) => {
                if let Some(room) = &status.room {
                    (self.thread_list_filters.get_or_insert_default())
                        .insert(room.clone(), status.filter);
                }
                self.thread_list_status = status.clone();
            }
            Event::Typing { room, names } => self.typing = Some((room.clone(), names.clone())),
            Event::ThreadTyping { thread, names } => {
                self.thread_typing = Some((thread.clone(), names.clone()));
            }
            _ => {}
        }
    }

    /// Whether the account has said what state it is in: an account added a moment ago hasn't.
    #[must_use]
    pub fn session_known(&self) -> bool {
        self.state.is_some()
    }

    /// What the `Session` view model shows now, for the account `id`, when `others` says that other
    /// accounts are listed besides it.
    #[must_use]
    pub fn session_view(&self, id: &AccountId, others: bool) -> SessionView {
        let mut view = session_view(self.state.as_ref(), self.connection);
        id.as_str().clone_into(&mut view.account_id);
        view.login_mode = self.login_mode(others);
        let host = host(self.identity.as_ref().map_or("", |i| i.server.as_str()));
        view.login_mode.title().clone_into(&mut view.login_title);
        view.login_mode.lede().clone_into(&mut view.login_lede);
        view.login_notice = view.login_mode.notice(&host);
        if view.login_mode == LoginMode::Again {
            // The notice says so, with the host.
            view.error_text.clear();
        }
        if matches!(
            self.state,
            Some(SessionState::LoggedOut { .. } | SessionState::Connecting)
        ) && let Some(identity) = &self.identity
        {
            view.login_server.clone_from(&identity.server);
            view.login_username.clone_from(&identity.username);
        }
        view
    }

    /// What the login page is for, if the account has one up.
    fn login_mode(&self, others: bool) -> LoginMode {
        if self.expired
            && matches!(
                self.state,
                Some(SessionState::LoggedOut { .. } | SessionState::Connecting)
            )
        {
            LoginMode::Again
        } else if others && self.is_blank() {
            LoginMode::Adding
        } else {
            LoginMode::Login
        }
    }

    /// The account is a new one: nobody is logged in on it and it doesn't say who it is yet.
    #[must_use]
    pub const fn is_blank(&self) -> bool {
        self.identity.is_none() && !matches!(self.state, Some(SessionState::LoggedIn(_)))
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
        let status = account_status(self.state.as_ref());
        let fresh = self.is_blank();
        let tip_title = if fresh {
            "New server".to_owned()
        } else {
            host(&server)
        };
        let tip_line = if fresh {
            "Log in to add it".to_owned()
        } else if status == Status::Expired {
            format!("{username}: log in again")
        } else if self.unread.mentions > 0 {
            let n = self.unread.mentions;
            format!(
                "{username} \u{b7} {n} mention{}",
                if n == 1 { "" } else { "s" }
            )
        } else {
            username.clone()
        };
        AccountView {
            id: id.as_str().to_owned(),
            monogram: monogram(&host(&server)),
            host: host(&server),
            server,
            username,
            status,
            fresh,
            label: format!("{tip_title}, {tip_line}"),
            tip_title,
            tip_line,
            busy: matches!(status, Status::Connecting | Status::Resuming),
            unread: self.unread.rooms > 0 && !current,
            connected: matches!(self.state, Some(SessionState::LoggedIn(_)))
                && self.connection == Connection::Connected,
            unread_rooms: self.unread.rooms,
            mentions: self.unread.mentions,
            current,
        }
    }

    /// The rooms as the room list shows them, in its order.
    #[must_use]
    pub fn rooms(&self) -> &[Room] {
        &self.rooms
    }

    /// The rooms the room list holds back (quiet direct messages), most recently active first. Only
    /// a search looks at them.
    #[must_use]
    pub fn quiet_rooms(&self) -> &[Room] {
        &self.quiet_rooms
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

    /// The change that brings an empty thread list model to the current threads.
    #[must_use]
    pub fn thread_list_snapshot(&self) -> ThreadListChange {
        ThreadListChange::Reset(self.thread_list.clone())
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
    #[must_use]
    pub fn held(&self, id: &str) -> Option<&Message> {
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

    /// The custom emoji image files known now, as URLs.
    #[must_use]
    pub fn emoji_images(&self) -> EmojiImageUrls {
        self.emoji_images.clone().unwrap_or_default()
    }

    /// The URL of the image file of the custom emoji `name` (without colons), as it is now: empty
    /// if it has none, or if no frontend asked for it.
    #[must_use]
    pub fn emoji_image_url(&self, name: &str) -> &str {
        self.emoji_images.as_ref().map_or("", |urls| urls.url(name))
    }

    /// The URLs, as they are now, of the emoji of `updates` (empty for a file that is gone).
    #[must_use]
    pub fn emoji_image_changes(&self, updates: &[EmojiImageUpdate]) -> Vec<(String, String)> {
        updates
            .iter()
            .map(|update| {
                (
                    update.name.clone(),
                    self.emoji_image_url(&update.name).to_owned(),
                )
            })
            .collect()
    }

    /// What the `Room` view model shows now.
    #[must_use]
    pub fn room_state(&self) -> RoomState {
        let mut state = room_state(&self.rooms, &self.status);
        // Only for the room the view model shows: a late event of another one means nothing.
        if let (Some((room, names)), Some(open)) = (&self.typing, &self.status.room)
            && room == open
        {
            state.typing = typing_line(names);
        }
        state
    }

    /// Which threads of the open room have replies the user hasn't read, as its subscription says.
    #[must_use]
    pub fn thread_unread(&self) -> ThreadUnread {
        (self.status.room.as_ref())
            .and_then(|id| self.rooms.iter().find(|room| &room.id == id))
            .map(ThreadUnread::of)
            .unwrap_or_default()
    }

    /// What the `ThreadList` view model shows now.
    #[must_use]
    pub fn thread_list_state(&self) -> ThreadListState {
        thread_list_state(
            &self.rooms,
            self.status.room.as_ref(),
            &self.thread_list_status,
            self.thread_list.len(),
            self.thread_status.parent.is_some(),
        )
    }

    /// The filter to open the open room's thread list with when the user didn't choose one: the
    /// last one used in the room, else all of them.
    #[must_use]
    pub fn thread_list_filter(&self) -> ThreadFilter {
        self.status
            .room
            .as_ref()
            .zip(self.thread_list_filters.as_ref())
            .and_then(|(room, filters)| filters.get(room))
            .copied()
            .unwrap_or_default()
    }

    /// What the `Thread` view model shows now.
    #[must_use]
    pub fn thread_state(&self) -> ThreadState {
        let mut state = thread_state(&self.rooms, self.status.room.as_ref(), &self.thread_status);
        if let (Some((thread, names)), Some(open)) =
            (&self.thread_typing, &self.thread_status.parent)
            && thread == open
        {
            state.typing = typing_line(names);
        }
        state
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

    /// Events as the core would emit them for a session that lists, adds, renames and removes
    /// rooms, and logs out.
    fn script() -> Vec<Event> {
        let mut list = RoomList::default();
        let now = jiff::Timestamp::UNIX_EPOCH;
        let rooms = |change: Option<RoomChange>| Event::Rooms(change.into_iter().collect());
        vec![
            Event::Session(SessionState::LoggedOut { failure: None }),
            Event::Session(SessionState::Connecting),
            Event::Rooms(vec![
                list.reset(vec![room("a", "alpha"), room("c", "charlie")], now),
            ]),
            rooms(list.upsert(room("b", "bravo"), now)),
            rooms(list.upsert(room("a", "zulu"), now)),
            rooms(list.remove(&"c".to_owned().into())),
            Event::Session(SessionState::LoggedOut { failure: None }),
            Event::Rooms(vec![list.reset(Vec::new(), now)]),
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
        assert_eq!(view(&cache).connection_text, "");
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
            view(&cache).connection_text,
            "Connection lost, reconnecting in 3s…"
        );
        cache.apply(&Event::Connection(Connection::Connected));
        assert_eq!(view(&cache).connection_text, "");
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
            thread_followers: Vec::new(),
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

    // ACT-001, ACT-002
    #[test]
    fn a_held_message_is_found_by_its_id() {
        let cache = cache_with_four_messages();
        assert_eq!(cache.held("b").map(|m| m.id.as_str()), Some("b"));
        assert_eq!(cache.held("nope"), None);
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

    // ROOM-010
    #[test]
    fn the_quiet_rooms_are_the_last_set_the_core_sent() {
        let mut cache = Cache::new();
        assert!(cache.quiet_rooms().is_empty());
        cache.apply(&Event::QuietRooms(vec![
            room("a", "alpha"),
            room("b", "beta"),
        ]));
        assert_eq!(cache.quiet_rooms(), [room("a", "alpha"), room("b", "beta")]);
        // The room list is another thing: its changes leave the set alone.
        cache.apply(&Event::Rooms(vec![RoomChange::Reset(vec![room(
            "c", "gamma",
        )])]));
        assert_eq!(cache.quiet_rooms().len(), 2);
        assert_eq!(cache.rooms(), [room("c", "gamma")]);
        cache.apply(&Event::QuietRooms(vec![]));
        assert!(cache.quiet_rooms().is_empty());
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

    // ROOM-009
    #[test]
    fn the_room_state_follows_the_topic_of_the_open_room_live() {
        let mut cache = Cache::new();
        cache.apply(&Event::Rooms(vec![RoomChange::Reset(vec![room(
            "a", "alpha",
        )])]));
        cache.apply(&Event::TimelineStatus(TimelineStatus {
            room: Some("a".to_owned().into()),
            ..TimelineStatus::default()
        }));
        assert_eq!(cache.room_state().topic, "");
        let with_topic = |topic: &str| {
            Event::Rooms(vec![RoomChange::Updated {
                index: 0,
                item: Room {
                    topic: topic.to_owned(),
                    ..room("a", "alpha")
                },
            }])
        };
        cache.apply(&with_topic("Plans"));
        assert_eq!(cache.room_state().topic, "Plans");
        cache.apply(&with_topic(""));
        assert_eq!(cache.room_state().topic, "");
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

    // ACT-006
    #[test]
    fn the_room_state_carries_the_line_of_the_open_rooms_typists_only() {
        let mut cache = Cache::new();
        let open = |room: Option<&str>| {
            Event::TimelineStatus(TimelineStatus {
                room: room.map(|id| id.to_owned().into()),
                ..TimelineStatus::default()
            })
        };
        let typing = |room: &str, names: &[&str]| Event::Typing {
            room: room.to_owned().into(),
            names: names.iter().map(|name| (*name).to_owned()).collect(),
        };
        cache.apply(&open(Some("a")));
        cache.apply(&typing("a", &["alice"]));
        assert_eq!(cache.room_state().typing, "alice is typing\u{2026}");
        // Another room's typists are not this room's.
        cache.apply(&typing("b", &["bob"]));
        assert_eq!(cache.room_state().typing, "");
        cache.apply(&typing("a", &["alice", "bob"]));
        assert_eq!(
            cache.room_state().typing,
            "alice and bob are typing\u{2026}"
        );
        cache.apply(&typing("a", &[]));
        assert_eq!(cache.room_state().typing, "");
        // A line that outlives its room doesn't show in the next one.
        cache.apply(&typing("a", &["alice"]));
        cache.apply(&open(Some("c")));
        assert_eq!(cache.room_state().typing, "");
        cache.apply(&open(None));
        assert_eq!(cache.room_state(), RoomState::default());
    }

    // ACT-006
    #[test]
    fn the_thread_state_carries_the_line_of_the_open_threads_typists_only() {
        let mut cache = Cache::new();
        let open = |parent: Option<&str>| {
            Event::ThreadStatus(ThreadStatus {
                parent: parent.map(|id| id.to_owned().into()),
                ..ThreadStatus::default()
            })
        };
        let typing = |thread: &str, names: &[&str]| Event::ThreadTyping {
            thread: thread.to_owned().into(),
            names: names.iter().map(|name| (*name).to_owned()).collect(),
        };
        cache.apply(&open(Some("p")));
        cache.apply(&typing("p", &["alice"]));
        assert_eq!(cache.thread_state().typing, "alice is typing\u{2026}");
        cache.apply(&typing("q", &["bob"]));
        assert_eq!(cache.thread_state().typing, "");
        cache.apply(&typing("p", &[]));
        assert_eq!(cache.thread_state().typing, "");
        cache.apply(&typing("p", &["alice"]));
        cache.apply(&open(None));
        assert_eq!(cache.thread_state(), ThreadState::default());
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

    // RICH-007
    #[test]
    fn a_model_created_late_gets_the_emoji_images_known_so_far() {
        let mut cache = Cache::new();
        assert_eq!(cache.emoji_image_url("party"), "");
        let update = |name: &str, file: Option<&str>| EmojiImageUpdate {
            name: name.to_owned(),
            file: file.map(std::path::PathBuf::from),
        };
        cache.apply(&Event::EmojiImages(vec![update("party", Some("/c/p.png"))]));
        cache.apply(&Event::EmojiImages(vec![
            update("shrug", Some("/c/s.png")),
            update("party", None),
        ]));
        assert_eq!(cache.emoji_image_url("party"), "");
        assert_eq!(cache.emoji_images().url("shrug"), "file:///c/s.png?v=1");
        assert_eq!(
            cache.emoji_image_changes(&[update("shrug", Some("/c/s.png")), update("party", None)]),
            [
                ("shrug".to_owned(), "file:///c/s.png?v=1".to_owned()),
                ("party".to_owned(), String::new())
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

    fn identity(server: &str, username: &str) -> Event {
        Event::Identity(Identity {
            server: server.to_owned(),
            username: username.to_owned(),
        })
    }

    fn expired() -> Event {
        Event::Session(SessionState::LoggedOut {
            failure: Some(LoginFailure::SessionExpired),
        })
    }

    fn view(cache: &Cache) -> SessionView {
        cache.session_view(&AccountId::from("a".to_owned()), false)
    }

    // ACCT-006
    #[test]
    fn an_expired_session_asks_to_log_in_again_as_who_the_account_was() {
        let mut cache = Cache::new();
        cache.apply(&identity("http://h", "alice"));
        cache.apply(&Event::Session(SessionState::Resuming));
        cache.apply(&logged_in("alice"));
        assert_eq!(view(&cache).login_mode, LoginMode::Login);
        assert_eq!(view(&cache).login_server, "");

        cache.apply(&expired());
        let shown = view(&cache);
        assert_eq!(shown.login_mode, LoginMode::Again);
        assert_eq!(shown.login_title, "Log in again");
        assert_eq!(
            shown.login_notice,
            "Your session on h has ended. Log in to see its rooms again."
        );
        assert_eq!(shown.login_server, "http://h");
        assert_eq!(shown.login_username, "alice");
        // The notice says it already.
        assert_eq!(shown.error_text, "");

        // Logging in again stays on that page.
        cache.apply(&Event::Session(SessionState::Connecting));
        assert_eq!(view(&cache).login_mode, LoginMode::Again);
        assert_eq!(view(&cache).login_server, "http://h");

        cache.apply(&logged_in("alice"));
        let shown = view(&cache);
        assert_eq!(shown.login_mode, LoginMode::Login);
        assert_eq!(shown.login_server, "");
        assert_eq!(shown.login_notice, "");
    }

    // ACCT-006
    #[test]
    fn a_failed_login_again_shows_the_login_page_prefilled_with_the_error() {
        let mut cache = Cache::new();
        cache.apply(&identity("http://h", "alice"));
        cache.apply(&expired());
        cache.apply(&Event::Session(SessionState::Connecting));
        cache.apply(&Event::Session(SessionState::LoggedOut {
            failure: Some(LoginFailure::InvalidCredentials),
        }));
        let shown = view(&cache);
        assert_eq!(shown.login_mode, LoginMode::Login);
        assert_eq!(shown.login_server, "http://h");
        assert_eq!(shown.login_username, "alice");
        assert_eq!(shown.error_text, "Wrong username or password");
    }

    // ACCT-006
    #[test]
    fn an_unreachable_server_at_resume_is_a_login_prefilled_not_an_expired_one() {
        let mut cache = Cache::new();
        cache.apply(&identity("http://h", "alice"));
        cache.apply(&Event::Session(SessionState::Resuming));
        cache.apply(&Event::Session(SessionState::LoggedOut {
            failure: Some(LoginFailure::Unreachable),
        }));
        let shown = view(&cache);
        // Not adding either, with others: the account has an identity.
        assert_eq!(
            cache
                .session_view(&AccountId::from("a".to_owned()), true)
                .login_mode,
            LoginMode::Login
        );
        assert_eq!(shown.login_mode, LoginMode::Login);
        assert_eq!(shown.login_server, "http://h");
        assert_eq!(shown.login_username, "alice");
        assert_eq!(shown.error_text, "Can't reach the server");
        assert_eq!(shown.login_notice, "");
    }

    // ACCT-002
    #[test]
    fn a_stored_account_without_a_token_is_expired() {
        let mut cache = Cache::new();
        cache.apply(&identity("http://h", "alice"));
        cache.apply(&expired());
        assert_eq!(view(&cache).login_mode, LoginMode::Again);
        let row = cache.account_view(&AccountId::from("a".to_owned()), false);
        assert_eq!(row.status, Status::Expired);
        assert!(!row.fresh);
    }

    // ACCT-001
    #[test]
    fn a_first_login_does_not_look_like_a_login_again() {
        let mut cache = Cache::new();
        cache.apply(&Event::Session(SessionState::LoggedOut { failure: None }));
        assert!(cache.is_blank());
        cache.apply(&Event::Session(SessionState::Connecting));
        // The identity comes after the login, before the session.
        cache.apply(&identity("http://h", "alice"));
        assert_eq!(view(&cache).login_mode, LoginMode::Login);
        cache.apply(&logged_in("alice"));
        assert_eq!(view(&cache).login_mode, LoginMode::Login);
        assert!(!cache.is_blank());
    }

    /// A parent of a thread with `replies` replies.
    fn listed(id: &str, replies: u32) -> Message {
        let mut parent = from_other(id, 1);
        parent.thread_replies = replies;
        parent
    }

    fn list_status(filter: ThreadFilter) -> Event {
        Event::ThreadListStatus(ThreadListStatus {
            room: Some("a".to_owned().into()),
            filter,
            ..ThreadListStatus::default()
        })
    }

    // ACT-010
    #[test]
    fn a_thread_list_snapshot_taken_at_any_point_followed_by_the_rest_equals_the_replay() {
        let events = vec![
            Event::ThreadList(vec![ThreadListChange::Reset(Vec::new())]),
            Event::ThreadList(vec![ThreadListChange::Reset(vec![
                listed("p1", 1),
                listed("p2", 2),
            ])]),
            Event::ThreadList(vec![ThreadListChange::Inserted {
                index: 2,
                item: listed("p3", 1),
            }]),
            Event::ThreadList(vec![ThreadListChange::Moved {
                from: 2,
                to: 0,
                item: listed("p3", 2),
            }]),
            Event::ThreadList(vec![ThreadListChange::Updated {
                index: 1,
                item: listed("p1", 5),
            }]),
            Event::ThreadList(vec![ThreadListChange::Removed { index: 0 }]),
            Event::ThreadList(vec![ThreadListChange::Reset(Vec::new())]),
        ];
        let mut replay = Vec::new();
        for event in &events {
            if let Event::ThreadList(changes) = event {
                for change in changes {
                    change.apply(&mut replay);
                }
            }
        }
        for split in 0..=events.len() {
            let mut cache = Cache::default();
            events[..split].iter().for_each(|event| cache.apply(event));
            let mut mirror = Vec::new();
            cache.thread_list_snapshot().apply(&mut mirror);
            for event in &events[split..] {
                if let Event::ThreadList(changes) = event {
                    for change in changes {
                        change.apply(&mut mirror);
                    }
                }
            }
            assert_eq!(mirror, replay, "split at {split}");
        }
    }

    // ACT-010
    #[test]
    fn the_thread_list_state_follows_the_status_the_rows_and_the_open_thread() {
        let mut cache = Cache::default();
        assert_eq!(cache.thread_list_state().pane, "none");
        cache.apply(&Event::Rooms(vec![RoomChange::Reset(vec![room(
            "a", "alpha",
        )])]));
        cache.apply(&Event::TimelineStatus(TimelineStatus {
            room: Some("a".to_owned().into()),
            ..TimelineStatus::default()
        }));
        cache.apply(&list_status(ThreadFilter::Following));
        let state = cache.thread_list_state();
        assert_eq!(
            (state.pane.as_str(), state.filter.as_str()),
            ("list", "following")
        );
        assert_eq!(state.room_name, "alpha");
        assert!(state.empty);
        cache.apply(&Event::ThreadList(vec![ThreadListChange::Reset(vec![
            listed("p1", 1),
        ])]));
        assert!(!cache.thread_list_state().empty);

        // A thread hides the list, which stays.
        cache.apply(&Event::ThreadStatus(ThreadStatus {
            parent: Some("p1".to_owned().into()),
            ..ThreadStatus::default()
        }));
        let state = cache.thread_list_state();
        assert_eq!((state.pane.as_str(), state.list_open), ("thread", true));
        cache.apply(&Event::ThreadStatus(ThreadStatus::default()));
        assert_eq!(cache.thread_list_state().pane, "list");

        cache.apply(&Event::ThreadListStatus(ThreadListStatus::default()));
        assert_eq!(cache.thread_list_state().pane, "none");
    }

    // ACT-010
    #[test]
    fn the_filter_last_used_in_a_room_is_the_one_to_open_its_list_with() {
        let mut cache = Cache::default();
        let open_room = |cache: &mut Cache, id: &str| {
            cache.apply(&Event::TimelineStatus(TimelineStatus {
                room: Some(id.to_owned().into()),
                ..TimelineStatus::default()
            }));
        };
        open_room(&mut cache, "a");
        assert_eq!(cache.thread_list_filter(), ThreadFilter::All);
        cache.apply(&list_status(ThreadFilter::Following));
        // Closing the list keeps the choice, and so does a thread alone.
        cache.apply(&Event::ThreadListStatus(ThreadListStatus::default()));
        assert_eq!(cache.thread_list_filter(), ThreadFilter::Following);
        // Another room has its own.
        open_room(&mut cache, "b");
        assert_eq!(cache.thread_list_filter(), ThreadFilter::All);
        open_room(&mut cache, "a");
        assert_eq!(cache.thread_list_filter(), ThreadFilter::Following);
    }
}
