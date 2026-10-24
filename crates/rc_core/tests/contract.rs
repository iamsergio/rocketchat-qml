// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The client against a real Rocket.Chat server, named by `RC_TEST_SERVER_URL`. Run with
//! `--ignored` (see the README).
//!
//! Every test makes its own user and rooms with unique names and only looks at those, so the tests
//! hold on a dirty server, in any order and in parallel. Server settings are never changed. Admin
//! only sets things up: no test logs a shared account in or out through the client.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use rc_core::adapters::{DiskFileCache, MemoryStore, RestConnector};
use rc_core::domain::AccountId;
use rc_core::domain::{
    AvatarOwner, Delivery, Message, MessageId, MessageKind, Room, RoomId, RoomKind,
};
use rc_core::emoji::{CustomEmojis, catalog};
use rc_core::policy::{ActionError, MessageAction};
use rc_core::ports::{
    CacheError, CacheKey, Connector, FileCache, Pushed, SecretStore, ServerApi, Stored,
};
use rc_core::rich::{Block, Emoji, Inline, Mention, TaskItem};
use rc_core::rooms::{RoomAction, RoomActionError, RoomChange};
use rc_core::session::{LoginFailure, SessionState};
use rc_core::threads::{ThreadFilter, ThreadListStatus};
use rc_core::timeline::{PAGE_SIZE, ThreadStatus, TimelineChange, TimelineStatus};
use rc_core::{AccountHandle, Client, Event};
use rc_test_server::{Admin, PIXEL_PNG, Proxy, TestUser, Typist, server_url, unique};
use secrecy::SecretString;

const TIMEOUT: Duration = Duration::from_secs(20);

/// A new user, who can log in at once.
fn new_user() -> TestUser {
    Admin::login().create_user()
}

fn password(user: &TestUser) -> SecretString {
    SecretString::from(user.password.clone())
}

/// A client and the account that the test drives: the first one added.
struct Session {
    client: Client,
    id: AccountId,
}

impl Session {
    fn account(&self) -> AccountHandle<'_> {
        self.client.account(&self.id)
    }

    /// Logs the account out and removes it.
    fn remove(&self) {
        self.client.remove_account(&self.id);
    }
}

/// A client with nothing stored, past its first event.
fn start() -> (Session, Receiver<Event>) {
    let (client, events) = start_with(MemoryStore::default());
    assert_eq!(next(&events), SessionState::LoggedOut { failure: None });
    (client, events)
}

/// A cache that keeps nothing: avatars are for the tests that look at them.
struct NoCache;

impl FileCache for NoCache {
    fn get(&self, _key: &CacheKey) -> Option<PathBuf> {
        None
    }

    fn put(
        &self,
        _key: &CacheKey,
        _type: Option<&str>,
        _bytes: &[u8],
    ) -> Result<Stored, CacheError> {
        Err(CacheError::Io("no cache".to_owned()))
    }

    fn remove(&self, _key: &CacheKey) -> Result<bool, CacheError> {
        Ok(false)
    }
}

/// A client on `store`; the first event is left to the caller.
fn start_with(store: MemoryStore) -> (Session, Receiver<Event>) {
    start_with_cache(store, NoCache)
}

fn start_with_cache(store: MemoryStore, cache: impl FileCache) -> (Session, Receiver<Event>) {
    let (tx, rx) = mpsc::channel();
    let (added_tx, added) = mpsc::channel();
    let client = Client::start(store, cache, move |id, event| {
        // Whoever starts a client follows its first account. The others' events are for the
        // tests that add accounts (see `start_tagged`).
        let _ = if event == Event::Added {
            added_tx.send(id).is_ok()
        } else {
            tx.send(event).is_ok()
        };
    })
    .unwrap();
    let id = added.recv_timeout(TIMEOUT).expect("no account added");
    (Session { client, id }, rx)
}

/// The next session state. Room events are skipped: only the room test looks at them.
fn next(events: &Receiver<Event>) -> SessionState {
    loop {
        if let Event::Session(state) = events.recv_timeout(TIMEOUT).expect("no event") {
            return state;
        }
    }
}

/// Expects `Connecting`, then returns the state the login ends in.
fn outcome(events: &Receiver<Event>) -> SessionState {
    assert_eq!(next(events), SessionState::Connecting);
    next(events)
}

fn assert_failure(state: &SessionState, expected: &LoginFailure) {
    assert_eq!(
        state,
        &SessionState::LoggedOut {
            failure: Some(expected.clone())
        }
    );
}

// AUTH-001
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn a_user_logs_in() {
    let user = new_user();
    let (client, events) = start();
    client
        .account()
        .login(&server_url(), &user.username, password(&user));
    let SessionState::LoggedIn(account) = outcome(&events) else {
        panic!("login failed");
    };
    assert_eq!(account.user.username, user.username);
    assert_eq!(account.server, server_url());
}

// AUTH-002
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn unknown_user_is_refused() {
    let (client, events) = start();
    let username = format!("ct-nobody-{}", unique());
    client
        .account()
        .login(&server_url(), &username, SecretString::from("whatever"));
    assert_failure(&outcome(&events), &LoginFailure::InvalidCredentials);
}

// AUTH-003
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn closed_port_is_unreachable() {
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let (client, events) = start();
    client.account().login(
        &format!("http://127.0.0.1:{port}"),
        "u",
        SecretString::from("p"),
    );
    assert_failure(&outcome(&events), &LoginFailure::Unreachable);
}

// AUTH-005
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn a_user_logs_out() {
    let user = new_user();
    let (client, events) = start();
    client
        .account()
        .login(&server_url(), &user.username, password(&user));
    assert!(matches!(outcome(&events), SessionState::LoggedIn(_)));
    client.remove();
    assert_eq!(next(&events), SessionState::LoggedOut { failure: None });
}

// AUTH-004
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn a_new_client_resumes_the_stored_session() {
    let user = new_user();
    let store = MemoryStore::default();

    let (first, events) = start_with(store.clone());
    assert_eq!(next(&events), SessionState::LoggedOut { failure: None });
    first
        .account()
        .login(&server_url(), &user.username, password(&user));
    assert!(matches!(outcome(&events), SessionState::LoggedIn(_)));
    // Dropping flushes the queued save, like quitting the app.
    drop(first);
    drop(events);

    // No login command this time.
    let (second, events) = start_with(store.clone());
    assert_eq!(next(&events), SessionState::Resuming);
    let SessionState::LoggedIn(account) = next(&events) else {
        panic!("resume failed");
    };
    assert_eq!(account.user.username, user.username);

    second.remove();
    assert_eq!(next(&events), SessionState::LoggedOut { failure: None });
    // Dropping flushes the queued removal.
    drop(second);
    assert!(store.load().unwrap().is_empty());
}

// ROOM-001, ROOM-002
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn a_user_gets_their_rooms_after_logging_in() {
    // A channel of the test's own for the new user: the server may hold anything else, and the
    // user may be in rooms the server adds by itself.
    let admin = Admin::login();
    let user = admin.create_user();
    let (id, name) = admin.create_channel(&[&user.username]);
    let (client, events) = start();
    client
        .account()
        .login(&server_url(), &user.username, password(&user));
    assert!(matches!(outcome(&events), SessionState::LoggedIn(_)));
    let Event::Rooms(changes) = events.recv_timeout(TIMEOUT).expect("no rooms event") else {
        panic!("expected the rooms right after logging in");
    };
    let [RoomChange::Reset(rooms)] = changes.as_slice() else {
        panic!("expected a single reset, got {changes:?}");
    };
    // Only the test's own room: the server may hold any number of others.
    let room: &Room = rooms
        .iter()
        .find(|room| room.id.as_str() == id)
        .expect("the new channel isn't listed");
    assert_eq!(room.kind, RoomKind::Channel);
    assert_eq!(room.name, name);
}

/// Logs `user` in and waits for the room list. Everything the client reports meanwhile is dropped.
fn logged_in(user: &TestUser) -> (Session, Receiver<Event>) {
    logged_in_at(&server_url(), user)
}

/// As [`logged_in`], through the server at `server`.
fn logged_in_at(server: &str, user: &TestUser) -> (Session, Receiver<Event>) {
    let (client, events) = start();
    client
        .account()
        .login(server, &user.username, password(user));
    assert!(matches!(outcome(&events), SessionState::LoggedIn(_)));
    loop {
        if let Event::Rooms(_) = events.recv_timeout(TIMEOUT).expect("no rooms event") {
            return (client, events);
        }
    }
}

/// Follows the events like a frontend until the status says that nothing is loading, and returns
/// that status.
fn settle(events: &Receiver<Event>, mirror: &mut Vec<Message>) -> TimelineStatus {
    loop {
        match events.recv_timeout(TIMEOUT).expect("no event") {
            Event::Timeline(changes) => changes.iter().for_each(|change| change.apply(mirror)),
            Event::TimelineStatus(status) if !status.loading && !status.loading_older => {
                return status;
            }
            _ => {}
        }
    }
}

fn texts(messages: &[Message]) -> Vec<&str> {
    messages
        .iter()
        .filter_map(|m| match &m.kind {
            MessageKind::Text(text) => Some(text.text.as_str()),
            MessageKind::System(_) => None,
        })
        .collect()
}

// MSG-001, MSG-002, MSG-003
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn the_whole_history_loads_page_by_page_in_order() {
    let admin_api = Admin::login();
    let user = admin_api.create_user();
    let (id, _) = admin_api.create_channel(&[&user.username]);
    let posted: Vec<String> = (0..PAGE_SIZE + 10)
        .map(|n| format!("message {n:03}"))
        .collect();
    for text in &posted {
        admin_api.post_message(&id, text);
    }
    let (client, events) = logged_in(&user);

    client.account().open_room(id.clone().into());
    let mut mirror = Vec::new();
    let status = settle(&events, &mut mirror);
    // Just the newest page: the older messages are still to load, and the order is oldest first.
    assert!(!status.reached_start);
    assert_eq!(status.room.as_ref().map(RoomId::as_str), Some(id.as_str()));
    assert!(mirror.len() <= PAGE_SIZE && texts(&mirror).len() < posted.len());
    let newest = posted.last().unwrap().as_str();
    assert_eq!(texts(&mirror).last().copied(), Some(newest));

    let mut pages = 0;
    while !settle_after_load_older(&client, &events, &mut mirror).reached_start {
        pages += 1;
        assert!(pages < 10, "the history never ends");
    }

    // Every message once, oldest first.
    let expected: Vec<&str> = posted.iter().map(String::as_str).collect();
    assert_eq!(texts(&mirror), expected);
    assert!(mirror.iter().all(|m| m.room_id.as_str() == id));
    let mut unique: Vec<_> = mirror.iter().map(|m| m.id.clone()).collect();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), mirror.len());
}

fn settle_after_load_older(
    client: &Session,
    events: &Receiver<Event>,
    mirror: &mut Vec<Message>,
) -> TimelineStatus {
    client.account().load_older();
    settle(events, mirror)
}

/// Reads events like a frontend until `found` returns something. Fails if that takes too long.
fn wait_for<T>(events: &Receiver<Event>, mut found: impl FnMut(&Event) -> Option<T>) -> T {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        let event = events.recv_timeout(left).expect("the event never came");
        if let Some(value) = found(&event) {
            return value;
        }
    }
}

/// The room a change inserts, updates or moves, as it is afterwards.
fn changed_room(change: &RoomChange) -> Option<&Room> {
    match change {
        RoomChange::Inserted { item: room, .. }
        | RoomChange::Updated { item: room, .. }
        | RoomChange::Moved { item: room, .. } => Some(room),
        _ => None,
    }
}

/// The first room that `event` changes and `wanted` accepts.
fn room_changed(event: &Event, wanted: impl Fn(&Room) -> bool) -> Option<Room> {
    let Event::Rooms(changes) = event else {
        return None;
    };
    changes
        .iter()
        .filter_map(changed_room)
        .find(|room| wanted(room))
        .cloned()
}

// RT-001
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn a_message_posted_to_the_open_room_arrives() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (id, _) = admin.create_channel(&[&user.username]);
    let (client, events) = logged_in(&user);
    client.account().open_room(id.clone().into());
    let mut mirror = Vec::new();
    settle(&events, &mut mirror);

    let posted = admin.post_message(&id, "hello from admin");
    wait_for(&events, |event| {
        if let Event::Timeline(changes) = event {
            for change in changes {
                assert!(
                    matches!(change, TimelineChange::Appended(_)),
                    "expected an append, got {change:?}"
                );
                change.apply(&mut mirror);
            }
        }
        mirror
            .iter()
            .any(|m| m.id.as_str() == posted.id)
            .then_some(())
    });
    assert_eq!(texts(&mirror), ["hello from admin"]);
    assert!(mirror.iter().all(|m| m.room_id.as_str() == id));
}

/// The blocks of the ordinary message with that text.
fn blocks_of<'a>(messages: &'a [Message], text: &str) -> &'a [Block] {
    messages
        .iter()
        .find_map(|m| match &m.kind {
            MessageKind::Text(body) if body.text == text => Some(body.blocks.as_slice()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no message {text:?}"))
}

// RICH-001, RICH-002, RICH-003
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn formatted_messages_arrive_as_blocks_from_history_and_live() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (id, _) = admin.create_channel(&[&user.username]);
    let formatted = "**bold** and `code` for @all in #general :smile: 😀";
    let list = "- [x] done\n- [ ] todo\n\n> quoted\n\n```rust\nlet x = 1;\n```";
    admin.post_message(&id, formatted);
    admin.post_message(&id, "just text");
    let (client, events) = logged_in(&user);
    client.account().open_room(id.clone().into());
    let mut mirror = Vec::new();
    settle(&events, &mut mirror);

    let Block::Paragraph(inlines) = &blocks_of(&mirror, formatted)[0] else {
        panic!("{:?}", blocks_of(&mirror, formatted));
    };
    assert_eq!(inlines[0], Inline::Bold(vec![Inline::Text("bold".into())]));
    assert_eq!(inlines[2], Inline::Code("code".into()));
    assert_eq!(inlines[4], Inline::Mention(Mention::All));
    assert_eq!(inlines[6], Inline::ChannelMention("general".into()));
    assert!(matches!(
        &inlines[8],
        Inline::Emoji(Emoji { short_code: Some(code), unicode: None }) if code == "smile"
    ));
    assert!(matches!(
        &inlines[10],
        Inline::Emoji(Emoji { short_code: None, unicode: Some(c) }) if c == "😀"
    ));
    assert_eq!(
        blocks_of(&mirror, "just text"),
        [Block::Paragraph(vec![Inline::Text("just text".into())])]
    );

    let posted = admin.post_message(&id, list);
    wait_for(&events, |event| {
        if let Event::Timeline(changes) = event {
            for change in changes {
                change.apply(&mut mirror);
            }
        }
        mirror
            .iter()
            .any(|m| m.id.as_str() == posted.id)
            .then_some(())
    });
    assert_eq!(
        blocks_of(&mirror, list),
        [
            Block::TaskList(vec![
                TaskItem {
                    done: true,
                    content: vec![Inline::Text("done".into())]
                },
                TaskItem {
                    done: false,
                    content: vec![Inline::Text("todo".into())]
                }
            ]),
            Block::Quote(vec![Block::Paragraph(vec![Inline::Text("quoted".into())])]),
            Block::Code {
                language: Some("rust".into()),
                text: "let x = 1;".into()
            }
        ]
    );
}

// RICH-003
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn logging_in_brings_the_custom_emoji_of_the_server() {
    let admin = Admin::login();
    let user = admin.create_user();
    let name = format!("ct_emoji_{}", unique().replace('-', "_"));
    admin.create_emoji(&name);
    let (client, events) = start();
    client
        .account()
        .login(&server_url(), &user.username, password(&user));

    let url = wait_for(&events, |event| match event {
        Event::CustomEmoji(emoji) => emoji.url(&name).map(str::to_owned),
        _ => None,
    });

    assert_eq!(
        url,
        format!(
            "{}/emoji-custom/{name}.png",
            server_url().trim_end_matches('/')
        )
    );
    client.remove();
    wait_for(&events, |event| match event {
        Event::CustomEmoji(emoji) => emoji.url(&name).is_none().then_some(()),
        _ => None,
    });
}

/// A cache directory that goes away with the test.
struct CacheDir(PathBuf);

impl CacheDir {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!("rc_core-contract-{}", unique())))
    }
}

impl Drop for CacheDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The avatar files announced until all of `owners` have one, by owner.
fn avatars_until(
    events: &Receiver<Event>,
    owners: &[AvatarOwner],
) -> std::collections::HashMap<AvatarOwner, PathBuf> {
    let mut found = std::collections::HashMap::new();
    wait_for(events, |event| {
        if let Event::Avatars(updates) = event {
            for update in updates {
                if let Some(path) = &update.path {
                    found.insert(update.owner.clone(), path.clone());
                }
            }
        }
        owners
            .iter()
            .all(|owner| found.contains_key(owner))
            .then_some(())
    });
    found
}

/// The uploaded one-pixel PNG, kept in the cache under `root`.
fn assert_cached_image(path: &Path, root: &Path) {
    assert!(path.starts_with(root), "{}", path.display());
    assert_eq!(path.extension().unwrap(), "png", "{}", path.display());
    assert!(
        std::fs::metadata(path).unwrap().len() > 0,
        "{}",
        path.display()
    );
}

// RICH-004
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn the_avatars_that_users_and_rooms_have_are_downloaded_and_kept_for_the_next_session() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    admin.set_avatar(&user.username);
    admin.set_room_avatar(&channel_id);
    let me = AvatarOwner::User(user.username.clone());
    let channel = AvatarOwner::Room(channel_id.clone().into());
    let cache = CacheDir::new();
    let (client, events) = start_with_cache(MemoryStore::default(), DiskFileCache::new(&cache.0));
    client
        .account()
        .login(&server_url(), &user.username, password(&user));

    // The room's own avatar, from the etag of the room list.
    let first = avatars_until(&events, std::slice::from_ref(&channel));
    assert_cached_image(&first[&channel], &cache.0);
    // The author of a message, once the room is open: the user's own message is one.
    client.account().open_room(channel_id.clone().into());
    client
        .account()
        .send_message(channel_id.into(), "hello", None, None, false);
    let mine = avatars_until(&events, std::slice::from_ref(&me));
    assert_cached_image(&mine[&me], &cache.0);
    drop(client);

    // A new session starts from the files.
    let (client, events) = start_with_cache(MemoryStore::default(), DiskFileCache::new(&cache.0));
    client
        .account()
        .login(&server_url(), &user.username, password(&user));
    let second = avatars_until(&events, std::slice::from_ref(&channel));
    assert_eq!(second[&channel], first[&channel]);
}

// RICH-004
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn rooms_and_users_with_the_generated_avatar_get_none() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    // The other side of the direct message has no avatar, whatever the shared server's admin has.
    let other = admin.create_user();
    let other_side = Admin::login_as(&other.username, &other.password);
    let dm_id = other_side.create_dm(&user.username);
    other_side.post_message(&dm_id, "hi");
    let cache = CacheDir::new();
    let (client, events) = start_with_cache(MemoryStore::default(), DiskFileCache::new(&cache.0));
    client
        .account()
        .login(&server_url(), &user.username, password(&user));
    wait_for(&events, |event| {
        matches!(event, Event::Rooms(_)).then_some(())
    });
    client.account().open_room(channel_id.clone().into());
    client
        .account()
        .send_message(channel_id.into(), "who am I", None, None, false);
    wait_for(&events, |event| match event {
        Event::Timeline(changes) => changes
            .iter()
            .any(|change| matches!(change, TimelineChange::Appended(_)))
            .then_some(()),
        _ => None,
    });

    // Give a download that would be made the time to come.
    std::thread::sleep(Duration::from_secs(2));
    while let Ok(event) = events.try_recv() {
        assert!(!matches!(event, Event::Avatars(_)), "{event:?}");
    }
}

/// The image files announced until all of `paths` have one, by path.
fn images_until(
    events: &Receiver<Event>,
    paths: &[String],
) -> std::collections::HashMap<String, PathBuf> {
    let mut found = std::collections::HashMap::new();
    wait_for(events, |event| {
        if let Event::Images(updates) = event {
            for update in updates {
                if let Some(file) = &update.file {
                    found.insert(update.path.clone(), file.clone());
                }
            }
        }
        paths
            .iter()
            .all(|path| found.contains_key(path))
            .then_some(())
    });
    found
}

/// The messages of the open room, as a frontend mirrors them, until the status says that nothing
/// is loading.
fn loaded_messages(events: &Receiver<Event>) -> Vec<Message> {
    let mut mirror = Vec::new();
    settle(events, &mut mirror);
    mirror
}

// RICH-005
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn the_attachments_of_a_room_are_read_and_its_image_is_downloaded_into_the_cache() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let image = admin.upload_file(
        &channel_id,
        "pixel.png",
        "image/png",
        &PIXEL_PNG,
        "look",
        "a pixel",
    );
    let file = admin.upload_file(&channel_id, "notes.txt", "text/plain", b"hello\n", "", "");
    let cache = CacheDir::new();
    let (client, events) = start_with_cache(MemoryStore::default(), DiskFileCache::new(&cache.0));
    client
        .account()
        .login(&server_url(), &user.username, password(&user));
    wait_for(&events, |event| {
        matches!(event, Event::Rooms(_)).then_some(())
    });

    client.account().open_room(channel_id.into());
    let messages = loaded_messages(&events);

    let find = |id: &str| messages.iter().find(|m| m.id.as_str() == id).unwrap();
    let attachment = &find(&image.id).attachments[0];
    assert_eq!(attachment.title, "pixel.png");
    assert_eq!(attachment.description.as_deref(), Some("a pixel"));
    assert!(attachment.link.starts_with("/file-upload/"));
    let picture = attachment.image.as_ref().unwrap();
    assert_eq!((picture.width, picture.height), (Some(1), Some(1)));
    let stored = images_until(&events, std::slice::from_ref(&picture.path));
    let path = &stored[&picture.path];
    assert!(path.starts_with(&cache.0), "{}", path.display());
    assert_eq!(path.extension().and_then(|e| e.to_str()), Some("png"));
    // A one-pixel file has no thumbnail: the cache holds the original.
    assert_eq!(std::fs::read(path).unwrap(), PIXEL_PNG);
    let notes = &find(&file.id).attachments[0];
    assert_eq!(notes.title, "notes.txt");
    assert_eq!(notes.size, Some(6));
    assert_eq!(notes.image, None);
}

// RICH-005
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn an_image_in_the_cache_is_shown_in_the_next_session_without_a_download() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    admin.upload_file(&channel_id, "pixel.png", "image/png", &PIXEL_PNG, "", "");
    let cache = CacheDir::new();
    let (client, events) = start_with_cache(MemoryStore::default(), DiskFileCache::new(&cache.0));
    client
        .account()
        .login(&server_url(), &user.username, password(&user));
    wait_for(&events, |event| {
        matches!(event, Event::Rooms(_)).then_some(())
    });
    client.account().open_room(channel_id.clone().into());
    let messages = loaded_messages(&events);
    let path = messages
        .iter()
        .find_map(|m| m.attachments.first()?.image.as_ref())
        .unwrap()
        .path
        .clone();
    let first = images_until(&events, std::slice::from_ref(&path));
    drop(client);

    let (client, events) = start_with_cache(MemoryStore::default(), DiskFileCache::new(&cache.0));
    client
        .account()
        .login(&server_url(), &user.username, password(&user));
    wait_for(&events, |event| {
        matches!(event, Event::Rooms(_)).then_some(())
    });
    client.account().open_room(channel_id.into());
    let second = images_until(&events, std::slice::from_ref(&path));

    assert_eq!(second[&path], first[&path]);
}

// RT-002, ROOM-001
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn a_channel_created_with_the_user_appears_in_the_room_list() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (client, events) = logged_in(&user);

    let (id, name) = admin.create_channel(&[&user.username]);
    let room = wait_for(&events, |event| {
        let Event::Rooms(changes) = event else {
            return None;
        };
        changes.iter().find_map(|change| match change {
            RoomChange::Inserted { item: room, .. } if room.id.as_str() == id => Some(room.clone()),
            _ => None,
        })
    });
    assert_eq!(room.kind, RoomKind::Channel);
    assert_eq!(room.name, name);
    drop(client);
}

// RT-002, ROOM-003
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn a_direct_message_raises_the_unread_count() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (client, events) = logged_in(&user);

    let dm = admin.create_dm(&user.username);
    admin.post_message(&dm, "one");
    let room = wait_for(&events, |event| {
        room_changed(event, |room| room.id.as_str() == dm && room.unread == 1)
    });
    assert_eq!(room.kind, RoomKind::Direct);
    admin.post_message(&dm, "two");
    wait_for(&events, |event| {
        room_changed(event, |room| room.id.as_str() == dm && room.unread == 2)
    });
    drop(client);
}

// ROOM-009
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn the_topic_of_a_room_is_listed_and_a_change_of_it_is_shown() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel, _) = admin.create_channel(&[&user.username]);
    admin.set_topic(&channel, "Plans for May");
    assert_eq!(listed_room(&user, &channel).topic, "Plans for May");
    let (client, events) = logged_in(&user);

    admin.set_topic(&channel, "Plans for June");
    wait_for(&events, |event| {
        room_changed(event, |room| {
            room.id.as_str() == channel && room.topic == "Plans for June"
        })
    });
    admin.set_topic(&channel, "");
    wait_for(&events, |event| {
        room_changed(event, |room| {
            room.id.as_str() == channel && room.topic.is_empty()
        })
    });
    drop(client);
}

/// The room `id` as a freshly logged-in client of `user` lists it.
fn listed_room(user: &TestUser, id: &str) -> Room {
    let (client, events) = start();
    client
        .account()
        .login(&server_url(), &user.username, password(user));
    assert!(matches!(outcome(&events), SessionState::LoggedIn(_)));
    wait_for(&events, |event| {
        let Event::Rooms(changes) = event else {
            return None;
        };
        changes.iter().find_map(|change| match change {
            RoomChange::Reset(rooms) => rooms.iter().find(|room| room.id.as_str() == id).cloned(),
            _ => None,
        })
    })
}

// ROOM-004
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn opening_a_room_marks_it_read_on_the_server() {
    let admin = Admin::login();
    let user = admin.create_user();
    let dm = admin.create_dm(&user.username);
    admin.post_message(&dm, "one");
    admin.post_message(&dm, "two");
    let (client, events) = logged_in(&user);
    assert_eq!(listed_room(&user, &dm).unread, 2);

    client.account().open_room(dm.clone().into());

    let room = wait_for(&events, |event| {
        room_changed(event, |room| room.id.as_str() == dm)
    });
    assert!(!room.has_unread(), "{room:?}");
    // What the server remembers, as another client sees it.
    let deadline = Instant::now() + TIMEOUT;
    while listed_room(&user, &dm).has_unread() {
        assert!(Instant::now() < deadline, "the server still counts unread");
        std::thread::sleep(Duration::from_millis(200));
    }
}

// ROOM-004
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn a_message_in_the_open_room_stays_read() {
    let admin = Admin::login();
    let user = admin.create_user();
    let dm = admin.create_dm(&user.username);
    // The user's side of a direct message is hidden until the first message.
    admin.post_message(&dm, "before");
    let (client, events) = logged_in(&user);
    client.account().open_room(dm.clone().into());
    let mut mirror = Vec::new();
    settle(&events, &mut mirror);

    let mut unread_seen = false;
    for text in ["one", "two"] {
        let posted = admin.post_message(&dm, text);
        wait_for(&events, |event| {
            unread_seen |=
                room_changed(event, |room| room.id.as_str() == dm && room.has_unread()).is_some();
            if let Event::Timeline(changes) = event {
                for change in changes {
                    change.apply(&mut mirror);
                }
            }
            mirror
                .iter()
                .any(|m| m.id.as_str() == posted.id)
                .then_some(())
        });
    }
    // Rooms changes may follow the message: let them come.
    while let Ok(event) = events.recv_timeout(Duration::from_millis(500)) {
        unread_seen |=
            room_changed(&event, |room| room.id.as_str() == dm && room.has_unread()).is_some();
    }

    assert!(!unread_seen, "the open room showed up as unread");
    let deadline = Instant::now() + TIMEOUT;
    while listed_room(&user, &dm).has_unread() {
        assert!(Instant::now() < deadline, "the server still counts unread");
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// The outcome of the next command on the room `id` that the client reports.
fn room_action_outcome(events: &Receiver<Event>, id: &str) -> Result<(), RoomActionError> {
    wait_for(events, |event| match event {
        Event::RoomAction { room, outcome, .. } if room.as_str() == id => Some(outcome.clone()),
        _ => None,
    })
}

/// Whether a freshly logged-in client of `user` lists the room `id`.
fn lists_room(user: &TestUser, id: &str) -> bool {
    let (client, events) = start();
    client
        .account()
        .login(&server_url(), &user.username, password(user));
    assert!(matches!(outcome(&events), SessionState::LoggedIn(_)));
    wait_for(&events, |event| {
        let Event::Rooms(changes) = event else {
            return None;
        };
        changes.iter().find_map(|change| match change {
            RoomChange::Reset(rooms) => Some(rooms.iter().any(|room| room.id.as_str() == id)),
            _ => None,
        })
    })
}

/// Waits until a freshly logged-in client of `user` lists the room `id` as `wanted` says. Fails if
/// that takes too long: the server's counts can lag a little behind.
fn wait_for_listed(user: &TestUser, id: &str, wanted: impl Fn(&Room) -> bool) {
    let deadline = Instant::now() + TIMEOUT;
    while !wanted(&listed_room(user, id)) {
        assert!(Instant::now() < deadline, "the server never listed it so");
        std::thread::sleep(Duration::from_millis(200));
    }
}

// ROOM-006
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn a_favorite_is_kept_by_the_server_and_moves_the_room_to_the_top() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (first, _) = admin.create_channel(&[&user.username]);
    let (second, _) = admin.create_channel(&[&user.username]);
    admin.post_message(&first, "newer");
    let (client, events) = logged_in(&user);

    client.account().set_favorite(second.clone().into(), true);

    assert_eq!(room_action_outcome(&events, &second), Ok(()));
    wait_for_listed(&user, &second, |room| room.favorite);
    assert!(!listed_room(&user, &first).favorite);

    client.account().set_favorite(second.clone().into(), false);

    assert_eq!(room_action_outcome(&events, &second), Ok(()));
    wait_for_listed(&user, &second, |room| !room.favorite);
}

// ROOM-006
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn a_favorite_set_by_another_client_arrives_live() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (id, _) = admin.create_channel(&[&user.username]);
    let (client, events) = logged_in(&user);

    Admin::login_as(&user.username, &user.password).set_favorite(&id, true);
    let room = wait_for(&events, |event| {
        room_changed(event, |room| room.id.as_str() == id && room.favorite)
    });
    assert!(room.favorite);

    Admin::login_as(&user.username, &user.password).set_favorite(&id, false);
    wait_for(&events, |event| {
        room_changed(event, |room| room.id.as_str() == id && !room.favorite)
    });
    drop(client);
}

// ROOM-006
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn a_favorite_of_a_room_that_left_the_list_is_refused_at_once() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (id, _) = admin.create_channel(&[&user.username]);
    let (client, events) = logged_in(&user);
    // The room leaves the list: the command is refused before asking the server.
    admin.kick(&id, &user.id);
    wait_for(&events, |event| match event {
        Event::Rooms(changes) => changes
            .iter()
            .any(|change| matches!(change, RoomChange::Removed { .. }))
            .then_some(()),
        _ => None,
    });

    client.account().set_favorite(id.clone().into(), true);

    assert_eq!(
        room_action_outcome(&events, &id),
        Err(RoomActionError::NotAllowed(RoomAction::Favorite))
    );
}

// ROOM-007
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn marking_a_room_read_and_unread_is_kept_by_the_server() {
    let admin = Admin::login();
    let user = admin.create_user();
    let dm = admin.create_dm(&user.username);
    admin.post_message(&dm, "one");
    admin.post_message(&dm, "two");
    let (client, events) = logged_in(&user);
    assert_eq!(listed_room(&user, &dm).unread, 2);

    client.account().mark_read(dm.clone().into());

    assert_eq!(room_action_outcome(&events, &dm), Ok(()));
    wait_for_listed(&user, &dm, |room| !room.has_unread());

    client.account().mark_unread(dm.clone().into());

    assert_eq!(room_action_outcome(&events, &dm), Ok(()));
    // The server counts one unread message and raises the alert.
    wait_for_listed(&user, &dm, |room| room.unread == 1 && room.alert);
    // Its push of the same change leaves the room as it is.
    admin.post_message(&dm, "three");
    let room = wait_for(&events, |event| {
        room_changed(event, |room| room.id.as_str() == dm && room.unread == 2)
    });
    assert!(room.alert);
}

// ROOM-007
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn marking_a_room_without_messages_unread_is_refused_with_the_servers_text() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (id, _) = admin.create_channel(&[&user.username]);
    let (client, events) = logged_in(&user);

    client.account().mark_unread(id.clone().into());

    assert_eq!(
        room_action_outcome(&events, &id),
        Err(RoomActionError::Server(
            "the server answered 400: There are no messages to mark unread \
             [error-no-message-for-unread]"
                .to_owned()
        ))
    );
}

// ROOM-007
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn leaving_a_channel_and_a_group_removes_them_from_the_list_and_from_the_server() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel, _) = admin.create_channel(&[&user.username]);
    let (group, _) = admin.create_group(&[&user.username]);
    let (kept, _) = admin.create_channel(&[&user.username]);
    let (client, events) = logged_in(&user);

    for id in [&channel, &group] {
        client.account().leave_room(id.clone().into());
        assert_eq!(room_action_outcome(&events, id), Ok(()));
    }

    assert!(!lists_room(&user, &channel));
    assert!(!lists_room(&user, &group));
    assert!(lists_room(&user, &kept));
}

// ROOM-007
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn leaving_the_open_room_closes_it() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (id, _) = admin.create_channel(&[&user.username]);
    admin.post_message(&id, "hello");
    let (client, events) = logged_in(&user);
    client.account().open_room(id.clone().into());
    settle(&events, &mut Vec::new());

    client.account().leave_room(id.clone().into());

    wait_for(&events, |event| match event {
        Event::Timeline(changes) => changes
            .iter()
            .any(|change| matches!(change, TimelineChange::Reset { room: None, .. }))
            .then_some(()),
        _ => None,
    });
    assert_eq!(room_action_outcome(&events, &id), Ok(()));
}

// ROOM-007
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn the_last_owner_cannot_leave_and_the_room_stays() {
    let admin = Admin::login();
    let user = admin.create_user();
    // The user creates the room, so is its only owner.
    let owner = Admin::login_as(&user.username, &user.password);
    let (group, _) = owner.create_group(&[]);
    let (channel, _) = owner.create_channel(&[]);
    let (client, events) = logged_in(&user);

    // The text is shown to the user as the server words it.
    let refusal = RoomActionError::Server(
        "the server answered 400: You are the last owner. Please set new owner before leaving \
         the room. [error-you-are-last-owner]"
            .to_owned(),
    );
    for id in [&group, &channel] {
        client.account().leave_room(id.clone().into());
        assert_eq!(room_action_outcome(&events, id), Err(refusal.clone()));
    }

    assert!(lists_room(&user, &group));
    assert!(lists_room(&user, &channel));
}

/// Where the rooms `ids` stand in the list `mirror`, in the order `mirror` has them.
fn order_of<'a>(mirror: &[Room], ids: &[&'a str]) -> Vec<&'a str> {
    mirror
        .iter()
        .filter_map(|room| ids.iter().find(|id| **id == room.id.as_str()).copied())
        .collect()
}

// ROOM-005
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn rooms_are_listed_by_last_message_and_a_new_message_moves_its_room_up() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (first, _) = admin.create_channel(&[&user.username]);
    let (second, _) = admin.create_channel(&[&user.username]);
    admin.post_message(&first, "older");
    admin.post_message(&second, "newer");
    let (client, events) = start();
    client
        .account()
        .login(&server_url(), &user.username, password(&user));
    assert!(matches!(outcome(&events), SessionState::LoggedIn(_)));
    let mut mirror = Vec::new();
    wait_for(&events, |event| {
        let Event::Rooms(changes) = event else {
            return None;
        };
        for change in changes {
            change.apply(&mut mirror);
        }
        Some(())
    });
    let ids = [first.as_str(), second.as_str()];
    assert_eq!(order_of(&mirror, &ids), [second.as_str(), first.as_str()]);

    admin.post_message(&first, "newest");

    let mut moved = false;
    wait_for(&events, |event| {
        let Event::Rooms(changes) = event else {
            return None;
        };
        for change in changes {
            moved |=
                matches!(change, RoomChange::Moved { item: room, .. } if room.id.as_str() == first);
            assert!(
                !matches!(change, RoomChange::Reset(_)),
                "a move is not a reset"
            );
            change.apply(&mut mirror);
        }
        (order_of(&mirror, &ids) == [first.as_str(), second.as_str()]).then_some(())
    });
    assert!(moved);
    drop(client);
}

// RT-003, RT-002, RT-001
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn a_client_catches_up_after_the_connection_was_cut() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (id, _) = admin.create_channel(&[&user.username]);
    // Something held already, so that what comes later joins it: a first page is a reset.
    admin.post_message(&id, "said before");
    let proxy = Proxy::start();
    let (client, events) = logged_in_at(&proxy.url(), &user);
    client.account().open_room(id.clone().into());
    let mut mirror = Vec::new();
    settle(&events, &mut mirror);

    // Cut off, and kept out while admin acts.
    proxy.set_accepting(false);
    proxy.drop_connections();
    let posted = admin.post_message(&id, "said while away");
    let (new_id, new_name) = admin.create_channel(&[&user.username]);
    proxy.set_accepting(true);

    // The client comes back by itself, and what it missed arrives as changes, not as a new list.
    let (mut got_message, mut new_room) = (false, None);
    wait_for(&events, |event| {
        match event {
            Event::Rooms(changes) => {
                assert!(
                    !changes.iter().any(|c| matches!(c, RoomChange::Reset(_))),
                    "the room list was reset: {changes:?}"
                );
                for change in changes {
                    if let RoomChange::Inserted { item: room, .. } = change
                        && room.id.as_str() == new_id
                    {
                        new_room = Some(room.clone());
                    }
                }
            }
            Event::Timeline(changes) => {
                for change in changes {
                    if matches!(change, TimelineChange::Appended(messages)
                        if messages.iter().any(|m| m.id.as_str() == posted.id))
                    {
                        got_message = true;
                    }
                    change.apply(&mut mirror);
                }
            }
            _ => {}
        }
        (got_message && new_room.is_some()).then_some(())
    });
    let new_room = new_room.unwrap();
    assert_eq!(new_room.name, new_name);
    assert_eq!(texts(&mirror), ["said before", "said while away"]);

    // And it is live again.
    let later = admin.post_message(&id, "after coming back");
    wait_for(&events, |event| {
        if let Event::Timeline(changes) = event {
            for change in changes {
                change.apply(&mut mirror);
            }
        }
        mirror
            .iter()
            .any(|m| m.id.as_str() == later.id)
            .then_some(())
    });
    drop(client);
}

// RT-003, RT-002
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn a_room_that_is_not_open_gets_its_unread_count_after_the_connection_was_cut() {
    let admin = Admin::login();
    let user = admin.create_user();
    let dm = admin.create_dm(&user.username);
    let proxy = Proxy::start();
    let (client, events) = logged_in_at(&proxy.url(), &user);

    proxy.set_accepting(false);
    proxy.drop_connections();
    admin.post_message(&dm, "one while away");
    admin.post_message(&dm, "two while away");
    proxy.set_accepting(true);

    wait_for(&events, |event| {
        room_changed(event, |room| room.id.as_str() == dm && room.unread == 2)
    });
    let later = admin.post_message(&dm, "three, live");
    wait_for(&events, |event| {
        room_changed(event, |room| {
            room.id.as_str() == dm
                && room.unread == 3
                && room.last_activity == later.ts.parse().unwrap()
        })
    });
    drop(client);
}

/// Follows the timeline like a frontend until `done` accepts the mirror.
fn follow_until(
    events: &Receiver<Event>,
    mirror: &mut Vec<Message>,
    mut done: impl FnMut(&[Message]) -> bool,
) {
    wait_for(events, |event| {
        if let Event::Timeline(changes) = event {
            for change in changes {
                change.apply(mirror);
            }
        }
        done(mirror).then_some(())
    });
}

/// The id of the unsent message in the mirror.
fn unsent_id(mirror: &[Message]) -> Option<String> {
    mirror
        .iter()
        .find(|m| m.is_unsent())
        .map(|m| m.id.to_string())
}

fn is_sent(mirror: &[Message], id: &str) -> bool {
    mirror
        .iter()
        .any(|m| m.id.as_str() == id && m.delivery == Delivery::Sent)
}

// SEND-001
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn a_sent_message_is_stored_under_the_id_it_was_shown_with() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (room, _) = admin.create_channel(&[&user.username]);
    admin.post_message(&room, "said before");
    let (client, events) = logged_in(&user);
    client.account().open_room(room.clone().into());
    let mut mirror = Vec::new();
    settle(&events, &mut mirror);

    client.account().send_message(
        room.clone().into(),
        "hello from the client",
        None,
        None,
        false,
    );

    // Shown at once, as pending, then confirmed, with nothing else in between.
    let mut pending = None;
    follow_until(&events, &mut mirror, |mirror| {
        pending = pending.take().or_else(|| unsent_id(mirror));
        pending.as_deref().is_some_and(|id| is_sent(mirror, id))
    });
    let id = pending.unwrap();
    assert_eq!(texts(&mirror), ["said before", "hello from the client"]);
    let shown = mirror.last().unwrap();
    assert_eq!(shown.id.as_str(), id);
    assert_eq!(shown.author.username, user.username);
    assert_eq!(admin.last_message(&room).id, id);
    drop(client);
}

// SEND-001
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn a_message_sent_to_a_room_that_is_not_open_any_more_is_sent_all_the_same() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (room, _) = admin.create_channel(&[&user.username]);
    let (other, _) = admin.create_channel(&[&user.username]);
    admin.post_message(&room, "said before");
    let (client, events) = logged_in(&user);
    client.account().open_room(room.clone().into());
    let mut mirror = Vec::new();
    settle(&events, &mut mirror);

    let before = admin.last_message(&room).id;
    client
        .account()
        .send_message(room.clone().into(), "sent and left", None, None, false);
    client.account().open_room(other.clone().into());
    settle(&events, &mut mirror);
    // Nothing is shown of it here; it arrives on the server all the same.
    let deadline = Instant::now() + TIMEOUT;
    while admin.last_message(&room).id == before {
        assert!(Instant::now() < deadline, "the message never arrived");
        std::thread::sleep(Duration::from_millis(100));
    }

    client.account().open_room(room.clone().into());
    follow_until(&events, &mut mirror, |mirror| {
        mirror.iter().any(|m| {
            m.delivery == Delivery::Sent && texts(std::slice::from_ref(m)) == ["sent and left"]
        })
    });
    assert!(mirror.iter().all(|m| !m.is_unsent()));
    drop(client);
}

// SEND-002
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn sending_an_id_the_server_stored_already_answers_with_the_stored_message() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (room, _) = admin.create_channel(&[&user.username]);
    let api = RestConnector.connect(&server_url()).unwrap();
    let (credentials, _) = api.login(&user.username, &password(&user)).await.unwrap();
    let room_id = RoomId::from(room.clone());
    let id = MessageId::from(unique().replace('-', ""));

    let first = api
        .send_message(&credentials, &room_id, &id, "once", None)
        .await
        .unwrap();
    let second = api
        .send_message(&credentials, &room_id, &id, "once", None)
        .await
        .unwrap();

    assert_eq!(second, first);
    assert_eq!(first.id, id);
    let page = api
        .history(&credentials, &room_id, RoomKind::Channel, None, PAGE_SIZE)
        .await
        .unwrap();
    assert_eq!(page.messages.iter().filter(|m| m.id == id).count(), 1);
}

// ACT-003
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn the_server_accepts_every_emoji_the_picker_offers() {
    use std::sync::Arc;
    use tokio::sync::Semaphore;

    let admin = Admin::login();
    let user = admin.create_user();
    let (room, _) = admin.create_channel(&[&user.username]);
    let posted = admin.post_message(&room, "react with everything");
    let api = Arc::new(RestConnector.connect(&server_url()).unwrap());
    let (credentials, _) = api.login(&user.username, &password(&user)).await.unwrap();
    let credentials = Arc::new(credentials);
    let id = MessageId::from(posted.id);
    let limit = Arc::new(Semaphore::new(8));

    let mut tasks = Vec::new();
    for emoji in catalog(&CustomEmojis::default()) {
        let (api, credentials, id, limit) = (
            Arc::clone(&api),
            Arc::clone(&credentials),
            id.clone(),
            Arc::clone(&limit),
        );
        tasks.push(tokio::spawn(async move {
            let _turn = limit.acquire().await.unwrap();
            let result = api.react(&credentials, &id, &emoji.shortcode, true).await;
            (emoji.shortcode, result)
        }));
    }
    let mut asked = 0;
    let mut refused = Vec::new();
    for task in tasks {
        let (shortcode, result) = task.await.unwrap();
        asked += 1;
        if let Err(error) = result {
            refused.push(format!("{shortcode} ({error})"));
        }
    }

    assert!(asked > 1000, "only {asked} emoji in the catalog");
    assert!(refused.is_empty(), "refused: {}", refused.join(", "));
}

// SEND-002
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn a_message_that_fails_while_the_server_is_out_of_reach_is_retried_under_its_id() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (room, _) = admin.create_channel(&[&user.username]);
    let proxy = Proxy::start();
    let (client, events) = logged_in_at(&proxy.url(), &user);
    client.account().open_room(room.clone().into());
    let mut mirror = Vec::new();
    settle(&events, &mut mirror);

    proxy.set_accepting(false);
    proxy.drop_connections();
    client
        .account()
        .send_message(room.clone().into(), "sent in the dark", None, None, false);
    follow_until(&events, &mut mirror, |mirror| {
        mirror.iter().any(|m| m.delivery == Delivery::Failed)
    });
    let id = unsent_id(&mirror).unwrap();
    assert_eq!(texts(&mirror), ["sent in the dark"]);

    proxy.set_accepting(true);
    client.account().retry_message(id.clone().into());
    follow_until(&events, &mut mirror, |mirror| is_sent(mirror, &id));

    assert_eq!(texts(&mirror), ["sent in the dark"]);
    assert_eq!(admin.last_message(&room).id, id);
    drop(client);
}

/// The text of the message with this id in the mirror.
fn text_of<'a>(mirror: &'a [Message], id: &str) -> Option<&'a str> {
    let message = mirror.iter().find(|m| m.id.as_str() == id)?;
    match &message.kind {
        MessageKind::Text(text) => Some(text.text.as_str()),
        MessageKind::System(_) => None,
    }
}

/// A channel with the user, the user logged in with the channel open and loaded, and the ids of
/// the messages admin posted to it before that.
fn open_channel_with(
    texts: &[&str],
) -> (
    Admin,
    String,
    Vec<String>,
    Session,
    Receiver<Event>,
    Vec<Message>,
) {
    let admin = Admin::login();
    let user = admin.create_user();
    let (room, _) = admin.create_channel(&[&user.username]);
    let ids = texts
        .iter()
        .map(|text| admin.post_message(&room, text).id)
        .collect();
    let (client, events) = logged_in(&user);
    client.account().open_room(room.clone().into());
    let mut mirror = Vec::new();
    settle(&events, &mut mirror);
    (admin, room, ids, client, events, mirror)
}

// RICH-006
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn an_edit_in_the_open_room_updates_the_message_in_place() {
    let (admin, room, ids, _client, events, mut mirror) = open_channel_with(&["one", "two"]);
    assert!(mirror.iter().all(|m| !m.edited));

    admin.edit_message(&room, &ids[0], "one, edited");

    wait_for(&events, |event| {
        if let Event::Timeline(changes) = event {
            for change in changes {
                assert!(
                    matches!(change, TimelineChange::Updated { .. }),
                    "expected an update, got {change:?}"
                );
                change.apply(&mut mirror);
            }
        }
        (text_of(&mirror, &ids[0]) == Some("one, edited")).then_some(())
    });
    let edited: Vec<_> = mirror.iter().filter(|m| m.edited).collect();
    assert_eq!(edited.len(), 1);
    assert_eq!(edited[0].id.as_str(), ids[0]);
    assert_eq!(texts(&mirror), ["one, edited", "two"]);
}

// RICH-007
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn a_reaction_in_the_open_room_shows_and_taking_it_back_removes_it() {
    let (admin, _room, ids, _client, events, mut mirror) = open_channel_with(&["react to me"]);

    admin.react(&ids[0], ":thumbsup:", true);
    follow_until(&events, &mut mirror, |mirror| {
        mirror.iter().any(|m| !m.reactions.is_empty())
    });
    let reactions = &mirror[0].reactions;
    assert_eq!(reactions.len(), 1);
    assert_eq!(reactions[0].emoji, ":thumbsup:");
    assert_eq!(
        reactions[0].usernames,
        std::slice::from_ref(&admin.username)
    );

    admin.react(&ids[0], ":thumbsup:", false);
    follow_until(&events, &mut mirror, |mirror| {
        mirror.iter().all(|m| m.reactions.is_empty())
    });
    assert_eq!(texts(&mirror), ["react to me"]);
}

// RICH-006
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn a_deletion_in_the_open_room_removes_the_message() {
    let (admin, room, ids, _client, events, mut mirror) = open_channel_with(&["stays", "goes"]);

    admin.delete_message(&room, &ids[1]);

    wait_for(&events, |event| {
        if let Event::Timeline(changes) = event {
            for change in changes {
                if let TimelineChange::Removed { index } = change {
                    assert_eq!(mirror[*index].id.as_str(), ids[1]);
                }
                change.apply(&mut mirror);
            }
        }
        (text_of(&mirror, &ids[1]).is_none()).then_some(())
    });
    assert_eq!(texts(&mirror), ["stays"]);
}

// RICH-006
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn a_deletion_follows_the_room_that_is_open_now() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (first, _) = admin.create_channel(&[&user.username]);
    let (second, _) = admin.create_channel(&[&user.username]);
    let in_first = admin.post_message(&first, "first room");
    let in_second = admin.post_message(&second, "second room");
    let (client, events) = logged_in(&user);
    let mut mirror = Vec::new();

    // Open the first, then the second: only the second is followed now.
    client.account().open_room(first.clone().into());
    settle(&events, &mut mirror);
    client.account().open_room(second.clone().into());
    settle(&events, &mut mirror);
    admin.delete_message(&second, &in_second.id);
    follow_until(&events, &mut mirror, |mirror| {
        text_of(mirror, &in_second.id).is_none()
    });

    // And back: the first is followed again.
    client.account().open_room(first.clone().into());
    settle(&events, &mut mirror);
    admin.delete_message(&first, &in_first.id);
    follow_until(&events, &mut mirror, |mirror| {
        text_of(mirror, &in_first.id).is_none()
    });
    assert_eq!(texts(&mirror), Vec::<&str>::new());
}

// ACT-006
#[tokio::test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
async fn typing_comes_for_the_room_the_connection_follows_and_for_no_other() {
    let admin = Admin::login();
    let user = admin.create_user();
    let typist_user = admin.create_user();
    let members = [user.username.as_str(), typist_user.username.as_str()];
    let (first, _) = admin.create_channel(&members);
    let (second, _) = admin.create_channel(&members);
    let thread = admin.post_message(&first, "a thread");
    let api = RestConnector.connect(&server_url()).unwrap();
    let (credentials, _) = api.login(&user.username, &password(&user)).await.unwrap();
    let mut realtime = api.realtime(&credentials).await.unwrap();
    let mut typist = Typist::connect(&typist_user);
    let typing = |room: &str, typing: bool, thread: Option<&str>| Pushed::Typing {
        room: room.to_owned().into(),
        username: typist_user.username.clone(),
        typing,
        thread: thread.map(|t| t.to_owned().into()),
    };

    // The first room is followed: its typing and its threads' arrive, its stop too.
    realtime.watch.send(Some(first.clone().into())).unwrap();
    start_until_pushed(
        &mut realtime,
        &mut typist,
        &first,
        &typing(&first, true, None),
    )
    .await;
    typist.start_in_thread(&first, &thread.id);
    next_pushed(&mut realtime, |p| {
        *p == typing(&first, true, Some(&thread.id))
    })
    .await;
    typist.stop(&first);
    next_pushed(&mut realtime, |p| *p == typing(&first, false, None)).await;

    // The second is followed now: once its typing arrives the first has been left, since the
    // server handles the connection's requests in order. Then the first's typing is not pushed,
    // and the second's stop, sent after it, comes with none of the first's before it.
    realtime.watch.send(Some(second.clone().into())).unwrap();
    start_until_pushed(
        &mut realtime,
        &mut typist,
        &second,
        &typing(&second, true, None),
    )
    .await;
    typist.start(&first);
    typist.stop(&second);
    // Repeated starts of the second may still be queued; none is of the first.
    next_pushed(&mut realtime, |p| {
        assert_ne!(
            *p,
            typing(&first, true, None),
            "the first room is still followed"
        );
        *p == typing(&second, false, None)
    })
    .await;

    // Nothing is followed once the room closes.
    realtime.watch.send(None).unwrap();
}

/// Starts the typist typing in the room again until `expected` arrives: the connection was told to
/// follow the room a moment ago, and the typing before it is done goes unheard.
async fn start_until_pushed(
    realtime: &mut rc_core::ports::Realtime,
    typist: &mut Typist,
    room: &str,
    expected: &Pushed,
) {
    let deadline = Instant::now() + TIMEOUT;
    while Instant::now() < deadline {
        typist.start(room);
        let heard = tokio::time::timeout(
            Duration::from_millis(300),
            next_pushed(realtime, |p| p == expected),
        )
        .await;
        if heard.is_ok() {
            return;
        }
    }
    panic!("{expected:?} never came");
}

/// The first pushed event that `wanted` accepts.
async fn next_pushed(
    realtime: &mut rc_core::ports::Realtime,
    mut wanted: impl FnMut(&Pushed) -> bool,
) -> Pushed {
    let wait = async {
        while let Some(pushed) = realtime.events.recv().await {
            if wanted(&pushed) {
                return pushed;
            }
        }
        panic!("the events ended");
    };
    tokio::time::timeout(TIMEOUT, wait)
        .await
        .expect("the event never came")
}

// RICH-006
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn what_changed_while_a_room_was_closed_shows_when_it_opens_again() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (room, _) = admin.create_channel(&[&user.username]);
    let (other, _) = admin.create_channel(&[&user.username]);
    let kept = admin.post_message(&room, "kept");
    let edited = admin.post_message(&room, "to edit");
    let deleted = admin.post_message(&room, "to delete");
    let (client, events) = logged_in(&user);
    let mut mirror = Vec::new();
    client.account().open_room(room.clone().into());
    settle(&events, &mut mirror);
    assert_eq!(texts(&mirror), ["kept", "to edit", "to delete"]);

    client.account().open_room(other.into());
    settle(&events, &mut mirror);
    admin.edit_message(&room, &edited.id, "edited meanwhile");
    admin.delete_message(&room, &deleted.id);
    client.account().open_room(room.clone().into());

    // It opens with what was held, and the changes arrive as updates.
    follow_until(&events, &mut mirror, |mirror| {
        text_of(mirror, &deleted.id).is_none()
            && text_of(mirror, &edited.id) == Some("edited meanwhile")
    });
    assert_eq!(text_of(&mirror, &kept.id), Some("kept"));
    assert!(
        mirror
            .iter()
            .any(|m| m.id.as_str() == edited.id && m.edited)
    );
}

// RICH-006, RICH-007, RT-003
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn a_client_catches_up_with_edits_reactions_and_deletions_after_the_connection_was_cut() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (room, _) = admin.create_channel(&[&user.username]);
    let edited = admin.post_message(&room, "to edit");
    let reacted = admin.post_message(&room, "to react to");
    let deleted = admin.post_message(&room, "to delete");
    let proxy = Proxy::start();
    let (client, events) = logged_in_at(&proxy.url(), &user);
    client.account().open_room(room.clone().into());
    let mut mirror = Vec::new();
    settle(&events, &mut mirror);

    proxy.set_accepting(false);
    proxy.drop_connections();
    admin.edit_message(&room, &edited.id, "edited while away");
    admin.react(&reacted.id, ":tada:", true);
    admin.delete_message(&room, &deleted.id);
    proxy.set_accepting(true);

    follow_until(&events, &mut mirror, |mirror| {
        text_of(mirror, &deleted.id).is_none()
            && text_of(mirror, &edited.id) == Some("edited while away")
            && mirror
                .iter()
                .any(|m| m.id.as_str() == reacted.id && !m.reactions.is_empty())
    });
    drop(client);
}

/// A channel with the user, where the user (as themselves) and admin posted `texts` in turns
/// (the user first), and the user logged in with the channel open and loaded.
struct OwnMessages {
    admin: Admin,
    /// The username of the user.
    username: String,
    room: String,
    room_name: String,
    /// The ids of the messages in `texts`' order.
    ids: Vec<String>,
    client: Session,
    events: Receiver<Event>,
    mirror: Vec<Message>,
}

fn open_channel_with_own_and_others(texts: &[&str]) -> OwnMessages {
    let admin = Admin::login();
    let user = admin.create_user();
    let own = Admin::login_as(&user.username, &user.password);
    let (room, room_name) = admin.create_channel(&[&user.username]);
    let ids = texts
        .iter()
        .enumerate()
        .map(|(n, text)| {
            let poster = if n % 2 == 0 { &own } else { &admin };
            poster.post_message(&room, text).id
        })
        .collect();
    let (client, events) = logged_in(&user);
    client.account().open_room(room.clone().into());
    let mut mirror = Vec::new();
    settle(&events, &mut mirror);
    OwnMessages {
        admin,
        username: user.username,
        room,
        room_name,
        ids,
        client,
        events,
        mirror,
    }
}

/// Follows the events like a frontend until the answer to `id` and returns its outcome.
fn action_outcome(
    events: &Receiver<Event>,
    mirror: &mut Vec<Message>,
    id: &str,
) -> Result<(), ActionError> {
    wait_for(events, |event| match event {
        Event::Timeline(changes) => {
            for change in changes {
                change.apply(mirror);
            }
            None
        }
        Event::MessageAction {
            id: answered,
            outcome,
            ..
        } if answered.as_str() == id => Some(outcome.clone()),
        _ => None,
    })
}

// ACT-001
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn a_user_edits_their_own_message_and_it_shows_edited() {
    let mut open = open_channel_with_own_and_others(&["mine", "theirs"]);
    let id = open.ids[0].clone();

    open.client
        .account()
        .edit_message(id.clone().into(), "mine, edited");

    let outcome = action_outcome(&open.events, &mut open.mirror, &id);
    assert_eq!(outcome, Ok(()));
    assert_eq!(texts(&open.mirror), ["mine, edited", "theirs"]);
    assert!(open.mirror[0].edited);
    assert!(!open.mirror[1].edited);
    // The server's push of the same copy leaves it so.
    open.admin.post_message(&open.room, "flush");
    follow_until(&open.events, &mut open.mirror, |mirror| {
        text_of(mirror, &id).is_some() && mirror.len() == 3
    });
    assert_eq!(texts(&open.mirror), ["mine, edited", "theirs", "flush"]);
}

// ACT-002
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn a_user_deletes_their_own_message_and_it_is_gone() {
    let mut open = open_channel_with_own_and_others(&["mine", "theirs"]);
    let id = open.ids[0].clone();

    open.client.account().delete_message(id.clone().into());

    let outcome = action_outcome(&open.events, &mut open.mirror, &id);
    assert_eq!(outcome, Ok(()));
    assert_eq!(texts(&open.mirror), ["theirs"]);
}

// ACT-001, ACT-002
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn the_message_of_somebody_else_is_refused_without_a_request_and_stays() {
    let mut open = open_channel_with_own_and_others(&["mine", "theirs"]);
    let id = open.ids[1].clone();

    open.client
        .account()
        .edit_message(id.clone().into(), "mine now");
    let edit = action_outcome(&open.events, &mut open.mirror, &id);
    open.client.account().delete_message(id.clone().into());
    let delete = action_outcome(&open.events, &mut open.mirror, &id);

    assert_eq!(edit, Err(ActionError::NotAllowed(MessageAction::Edit)));
    assert_eq!(delete, Err(ActionError::NotAllowed(MessageAction::Delete)));
    assert_eq!(texts(&open.mirror), ["mine", "theirs"]);
}

// ACT-003
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn a_user_reacts_to_a_message_of_somebody_else_and_takes_the_reaction_back() {
    let mut open = open_channel_with_own_and_others(&["mine", "theirs"]);
    let id = open.ids[1].clone();

    open.client.account().react(id.clone().into(), "tada", true);

    let outcome = action_outcome(&open.events, &mut open.mirror, &id);
    assert_eq!(outcome, Ok(()));
    let reactions = &open.mirror[1].reactions;
    assert_eq!(reactions.len(), 1);
    assert_eq!(reactions[0].emoji, ":tada:");
    assert_eq!(reactions[0].usernames, [open.username.clone()]);
    // The server's push of the same reaction leaves it so.
    open.admin.post_message(&open.room, "flush");
    follow_until(&open.events, &mut open.mirror, |mirror| mirror.len() == 3);
    assert_eq!(open.mirror[1].reactions.len(), 1);
    assert_eq!(open.mirror[1].reactions[0].usernames.len(), 1);

    open.client
        .account()
        .react(id.clone().into(), ":tada:", false);

    let outcome = action_outcome(&open.events, &mut open.mirror, &id);
    assert_eq!(outcome, Ok(()));
    assert!(open.mirror[1].reactions.is_empty());
    open.admin.post_message(&open.room, "flush again");
    follow_until(&open.events, &mut open.mirror, |mirror| mirror.len() == 4);
    assert!(open.mirror[1].reactions.is_empty());
}

// ACT-003
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn an_unknown_emoji_is_refused_by_the_server_and_leaves_no_reaction() {
    let mut open = open_channel_with_own_and_others(&["mine", "theirs"]);
    let id = open.ids[1].clone();

    open.client
        .account()
        .react(id.clone().into(), "no_such_emoji_here", true);

    let outcome = action_outcome(&open.events, &mut open.mirror, &id);
    assert!(
        matches!(outcome, Err(ActionError::Server(_))),
        "{outcome:?}"
    );
    assert!(open.mirror[1].reactions.is_empty());
}

/// The message of the mirror that says `text`.
fn message_saying<'a>(mirror: &'a [Message], text: &str) -> Option<&'a Message> {
    mirror
        .iter()
        .find(|m| matches!(&m.kind, MessageKind::Text(body) if body.text == text))
}

// ACT-005
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn a_reply_that_quotes_a_message_shows_the_quote_and_not_the_link() {
    let mut open = open_channel_with_own_and_others(&["mine", "theirs"]);
    let theirs = open.ids[1].clone();

    open.client.account().send_message(
        open.room.clone().into(),
        "my reply",
        Some(theirs.clone().into()),
        None,
        false,
    );

    // The local copy has the quote already, and the server's copy, which replaces it, has it too.
    follow_until(&open.events, &mut open.mirror, |mirror| {
        message_saying(mirror, "my reply").is_some_and(|m| !m.is_unsent())
    });
    let reply = message_saying(&open.mirror, "my reply").unwrap();
    let [quote] = reply.quotes.as_slice() else {
        panic!("expected one quote, got {:?}", reply.quotes);
    };
    assert_eq!(quote.message, Some(MessageId::from(theirs.clone())));
    assert_eq!(quote.author, open.admin.username);
    assert_eq!(quote.text.text, "theirs");
    assert!(
        quote.link.contains(&format!("?msg={theirs}")),
        "{}",
        quote.link
    );
    assert!(quote.ts.is_some());
    // Nobody else's message and no other reply made a quote.
    assert_eq!(
        open.mirror.iter().filter(|m| !m.quotes.is_empty()).count(),
        1
    );
    assert_eq!(texts(&open.mirror), ["mine", "theirs", "my reply"]);
}

// ACT-005
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn a_quote_made_by_somebody_else_arrives_live_and_editing_the_reply_keeps_it() {
    let mut open = open_channel_with_own_and_others(&["mine", "theirs"]);
    let mine = open.ids[0].clone();
    // The admin quotes the user's message, as another client would.
    let path = format!("channel/{}", open.room_name);
    let posted = open
        .admin
        .post_quote(&open.room, &path, &mine, "admin replies");
    follow_until(&open.events, &mut open.mirror, |mirror| {
        message_saying(mirror, "admin replies").is_some()
    });
    let reply = message_saying(&open.mirror, "admin replies").unwrap();
    assert_eq!(reply.id.as_str(), posted.id);
    assert_eq!(reply.quotes.len(), 1);
    assert_eq!(reply.quotes[0].message, Some(MessageId::from(mine.clone())));
    assert_eq!(reply.quotes[0].text.text, "mine");

    // The user replies to the admin's reply and then edits their own reply.
    open.client.account().send_message(
        open.room.clone().into(),
        "quoting a quote",
        Some(posted.id.clone().into()),
        None,
        false,
    );
    follow_until(&open.events, &mut open.mirror, |mirror| {
        message_saying(mirror, "quoting a quote").is_some_and(|m| !m.is_unsent())
    });
    let own = message_saying(&open.mirror, "quoting a quote").unwrap();
    // What the server shows of the quoted reply is its text, without the link it quotes.
    assert_eq!(own.quotes[0].text.text, "admin replies");
    let own_id = own.id.to_string();

    open.client
        .account()
        .edit_message(own_id.clone().into(), "quoting a quote, edited");

    assert_eq!(
        action_outcome(&open.events, &mut open.mirror, &own_id),
        Ok(())
    );
    let edited = message_saying(&open.mirror, "quoting a quote, edited").unwrap();
    assert!(edited.edited);
    assert_eq!(edited.quotes.len(), 1);
    assert_eq!(edited.quotes[0].text.text, "admin replies");
}

// ACT-005
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn a_quote_sent_right_away_works_when_the_server_is_reached_by_another_address() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (room, _) = admin.create_channel(&[&user.username]);
    let theirs = admin.post_message(&room, "theirs");
    // The proxy's address is not the server's `Site_Url`, as the container's name is not in CI.
    let proxy = Proxy::start();
    let (client, events) = logged_in_at(&proxy.url(), &user);
    client.account().open_room(room.clone().into());
    let mut mirror = Vec::new();
    settle(&events, &mut mirror);

    client.account().send_message(
        room.clone().into(),
        "right away",
        Some(theirs.id.clone().into()),
        None,
        false,
    );

    follow_until(&events, &mut mirror, |mirror| {
        message_saying(mirror, "right away").is_some_and(|m| !m.is_unsent())
    });
    // The server's copy, which replaced the local one, has the quote.
    let reply = message_saying(&mirror, "right away").unwrap();
    assert_eq!(reply.quotes.len(), 1, "{:?}", reply.quotes);
    assert_eq!(reply.quotes[0].text.text, "theirs");
    assert_eq!(reply.quotes[0].message, Some(MessageId::from(theirs.id)));
    drop(client);
}

/// What a frontend shows of the open room and the open thread.
#[derive(Default)]
struct Mirrors {
    room: Vec<Message>,
    thread: Vec<Message>,
    status: ThreadStatus,
    /// The thread list.
    list: Vec<Message>,
    list_status: ThreadListStatus,
    rooms: Vec<Room>,
}

/// Follows the events like a frontend until `done` accepts what is shown.
fn follow_both(
    events: &Receiver<Event>,
    mirrors: &mut Mirrors,
    mut done: impl FnMut(&Mirrors) -> bool,
) {
    wait_for(events, |event| {
        show(event, mirrors);
        done(mirrors).then_some(())
    });
}

/// Like [`follow_both`], but takes in the events that are waiting already first, and stops there
/// if `done` accepts what they show: what it waits for may have come before the events that an
/// earlier step waited for.
fn follow_queued(
    events: &Receiver<Event>,
    mirrors: &mut Mirrors,
    mut done: impl FnMut(&Mirrors) -> bool,
) {
    while let Ok(event) = events.try_recv() {
        show(&event, mirrors);
    }
    if !done(mirrors) {
        follow_both(events, mirrors, done);
    }
}

/// Puts what `event` changes into the mirrors.
fn show(event: &Event, mirrors: &mut Mirrors) {
    match event {
        Event::Timeline(changes) => changes
            .iter()
            .for_each(|change| change.apply(&mut mirrors.room)),
        Event::ThreadTimeline(changes) => changes
            .iter()
            .for_each(|change| change.apply(&mut mirrors.thread)),
        Event::ThreadStatus(status) => mirrors.status = status.clone(),
        Event::ThreadList(changes) => changes
            .iter()
            .for_each(|change| change.apply(&mut mirrors.list)),
        Event::ThreadListStatus(status) => mirrors.list_status = status.clone(),
        // The rooms listed at the login came before the mirror: it keeps the latest copy of
        // each room that changes.
        Event::Rooms(changes) => {
            for room in changes.iter().filter_map(changed_room) {
                mirrors.rooms.retain(|held| held.id != room.id);
                mirrors.rooms.push(room.clone());
            }
        }
        _ => {}
    }
}

/// The mirrors of an open channel whose message `theirs` has the thread open, with `replies`
/// (admin's) in it, `r1` first.
fn open_thread_with_replies(replies: usize) -> (OwnMessages, Mirrors) {
    let mut open = open_channel_with_own_and_others(&["mine", "theirs"]);
    let parent = open.ids[1].clone();
    for n in 1..=replies {
        open.admin
            .post_in_thread(&open.room, &parent, &format!("r{n}"), false);
    }
    open.client.account().open_thread(parent.into());
    let mut mirrors = Mirrors {
        room: std::mem::take(&mut open.mirror),
        ..Mirrors::default()
    };
    follow_both(&open.events, &mut mirrors, |m| {
        !m.status.loading && !m.status.loading_older && m.status.parent.is_some()
    });
    (open, mirrors)
}

// ACT-004
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn a_thread_loads_page_by_page_with_the_parent_first() {
    let replies = PAGE_SIZE + 5;
    let (open, mut mirrors) = open_thread_with_replies(replies);
    assert!(!mirrors.status.reached_start);
    assert_eq!(texts(&mirrors.thread).len(), PAGE_SIZE);

    open.client.account().load_older_thread();
    follow_both(&open.events, &mut mirrors, |m| m.status.reached_start);

    let expected: Vec<String> = std::iter::once("theirs".to_owned())
        .chain((1..=replies).map(|n| format!("r{n}")))
        .collect();
    assert_eq!(texts(&mirrors.thread), expected);
    // The replies are not in the room.
    assert_eq!(texts(&mirrors.room), ["mine", "theirs"]);
    // The parent knows how many they are.
    let parent = message_saying(&mirrors.room, "theirs").unwrap();
    assert_eq!(parent.thread_replies as usize, replies);
    assert!(parent.thread_last_reply.is_some());
    assert!(parent.thread_id.is_none());
    assert!(mirrors.thread[1..].iter().all(|m| m.thread_id.is_some()));
}

/// The mirrors of a channel whose first page starts after its message "old parent", which has a
/// reply, "r1". The user (not admin) has the channel open.
fn open_channel_past_its_thread() -> (OwnMessages, Mirrors, String) {
    let admin = Admin::login();
    let user = admin.create_user();
    let (room, room_name) = admin.create_channel(&[&user.username]);
    let parent = admin.post_message(&room, "old parent").id;
    admin.post_in_thread(&room, &parent, "r1", false);
    for n in 0..PAGE_SIZE + 5 {
        admin.post_message(&room, &format!("filler {n}"));
    }
    let (client, events) = logged_in(&user);
    client.account().open_room(room.clone().into());
    let mut mirrors = Mirrors::default();
    follow_both(&events, &mut mirrors, |m| m.room.len() == PAGE_SIZE);
    let open = OwnMessages {
        admin,
        username: user.username,
        room,
        room_name,
        ids: Vec::new(),
        client,
        events,
        mirror: Vec::new(),
    };
    (open, mirrors, parent)
}

// ACT-004
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn a_thread_whose_parent_is_older_than_the_first_page_opens_and_follows_its_replies() {
    let (open, mut mirrors, parent) = open_channel_past_its_thread();
    assert!(message_saying(&mirrors.room, "old parent").is_none());

    open.client.account().open_thread(parent.clone().into());
    follow_both(&open.events, &mut mirrors, |m| {
        !m.status.loading && m.status.parent.is_some() && m.status.reached_start
    });
    assert_eq!(texts(&mirrors.thread), ["old parent", "r1"]);
    assert_eq!(mirrors.thread[0].thread_replies, 1);

    // A reply pushed now reaches the thread and counts in its parent, which the room doesn't hold.
    let r2 = open.admin.post_in_thread(&open.room, &parent, "r2", false);
    follow_both(&open.events, &mut mirrors, |m| {
        message_saying(&m.thread, "r2").is_some() && m.thread[0].thread_replies == 2
    });
    assert!(message_saying(&mirrors.room, "old parent").is_none());
    assert!(message_saying(&mirrors.room, "r2").is_none());

    // The server doesn't push the parent for a deletion: the core counts it down.
    open.admin.delete_message(&open.room, &r2.id);
    follow_both(&open.events, &mut mirrors, |m| {
        message_saying(&m.thread, "r2").is_none() && m.thread[0].thread_replies == 1
    });

    // Deleting the parent, which has a reply, closes the thread.
    open.admin.delete_message(&open.room, &parent);
    follow_both(&open.events, &mut mirrors, |m| m.status.parent.is_none());
    assert!(mirrors.thread.is_empty());
}

// ACT-004
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn a_message_of_another_room_or_one_that_is_missing_does_not_open_a_thread() {
    let (open, mut mirrors, _) = open_channel_past_its_thread();
    let (other, _) = open.admin.create_channel(&[&open.username]);
    let elsewhere = open.admin.post_message(&other, "elsewhere").id;
    open.admin
        .post_in_thread(&other, &elsewhere, "reply elsewhere", false);

    for id in [elsewhere.as_str(), "noSuchMessage"] {
        open.client.account().open_thread(id.to_owned().into());
    }

    // The room goes on: a message sent after the asks is the last thing heard, and no thread
    // was opened before it.
    open.admin.post_message(&open.room, "after");
    follow_both(&open.events, &mut mirrors, |m| {
        message_saying(&m.room, "after").is_some()
    });
    assert!(mirrors.status.parent.is_none());
    assert!(mirrors.thread.is_empty());
}

// ACT-004
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn a_reply_sent_from_the_client_settles_in_the_thread_and_counts_in_the_parent() {
    let (open, mut mirrors) = open_thread_with_replies(1);
    let parent = open.ids[1].clone();

    open.client.account().send_message(
        open.room.clone().into(),
        "my reply",
        None,
        Some(parent.clone().into()),
        false,
    );

    follow_both(&open.events, &mut mirrors, |m| {
        message_saying(&m.thread, "my reply").is_some_and(|r| !r.is_unsent())
            && message_saying(&m.room, "theirs").is_some_and(|p| p.thread_replies == 2)
            && m.thread[0].thread_replies == 2
    });
    assert_eq!(texts(&mirrors.thread), ["theirs", "r1", "my reply"]);
    let reply = message_saying(&mirrors.thread, "my reply").unwrap();
    assert_eq!(reply.thread_id, Some(MessageId::from(parent)));
    // Not sent to the channel, it is not in the room.
    assert_eq!(texts(&mirrors.room), ["mine", "theirs"]);
}

// ACT-004
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn a_reply_also_sent_to_the_channel_settles_in_the_thread_and_in_the_room() {
    let (open, mut mirrors) = open_thread_with_replies(0);
    let parent = open.ids[1].clone();

    open.client.account().send_message(
        open.room.clone().into(),
        "for everybody",
        None,
        Some(parent.into()),
        true,
    );

    follow_both(&open.events, &mut mirrors, |m| {
        let settled = |mirror: &[Message]| {
            message_saying(mirror, "for everybody").is_some_and(|r| !r.is_unsent())
        };
        settled(&m.room) && settled(&m.thread)
    });
    assert_eq!(texts(&mirrors.room), ["mine", "theirs", "for everybody"]);
    assert_eq!(texts(&mirrors.thread), ["theirs", "for everybody"]);
}

// ACT-004
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn live_replies_arrive_in_the_thread_and_the_parent_follows_their_count() {
    let (open, mut mirrors) = open_thread_with_replies(0);
    let parent = open.ids[1].clone();

    open.admin
        .post_in_thread(&open.room, &parent, "quietly", false);
    let loud = open
        .admin
        .post_in_thread(&open.room, &parent, "loudly", true);

    follow_both(&open.events, &mut mirrors, |m| {
        message_saying(&m.thread, "loudly").is_some()
            && message_saying(&m.room, "loudly").is_some()
            && m.thread[0].thread_replies == 2
            // The thread's copy of the parent is announced before the room's.
            && message_saying(&m.room, "theirs").is_some_and(|p| p.thread_replies == 2)
    });
    assert_eq!(texts(&mirrors.thread), ["theirs", "quietly", "loudly"]);
    // Only the one sent to the channel is in the room.
    assert_eq!(texts(&mirrors.room), ["mine", "theirs", "loudly"]);
    assert_eq!(
        message_saying(&mirrors.room, "theirs")
            .unwrap()
            .thread_replies,
        2
    );
    assert_eq!(mirrors.room[2].id.as_str(), loud.id);
}

// ACT-004
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn a_deleted_reply_leaves_the_thread_and_the_parent_counts_one_less() {
    let (open, mut mirrors) = open_thread_with_replies(0);
    let parent = open.ids[1].clone();
    let first = open
        .admin
        .post_in_thread(&open.room, &parent, "first", true);
    open.admin
        .post_in_thread(&open.room, &parent, "second", false);
    follow_both(&open.events, &mut mirrors, |m| {
        message_saying(&m.thread, "second").is_some() && m.thread[0].thread_replies == 2
    });

    open.admin.delete_message(&open.room, &first.id);

    follow_both(&open.events, &mut mirrors, |m| {
        message_saying(&m.thread, "first").is_none()
            && message_saying(&m.room, "first").is_none()
            && m.thread[0].thread_replies == 1
            && message_saying(&m.room, "theirs").is_some_and(|p| p.thread_replies == 1)
    });
    assert_eq!(texts(&mirrors.thread), ["theirs", "second"]);
}

// ACT-004
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn deleting_the_parent_closes_the_thread_and_so_does_another_room() {
    // The server keeps a deleted parent with replies as a removed message, which closes it.
    let (open, mut mirrors) = open_thread_with_replies(1);
    open.admin.delete_message(&open.room, &open.ids[1]);
    follow_both(&open.events, &mut mirrors, |m| {
        m.status.parent.is_none() && m.thread.is_empty()
    });

    let parent = open.admin.post_message(&open.room, "another parent");
    follow_both(&open.events, &mut mirrors, |m| {
        message_saying(&m.room, "another parent").is_some()
    });
    open.client.account().open_thread(parent.id.into());
    follow_both(&open.events, &mut mirrors, |m| m.status.parent.is_some());
    let (other, _) = open.admin.create_channel(&[&open.username]);
    wait_for(&open.events, |event| match event {
        Event::Rooms(changes) => changes
            .iter()
            .any(|change| changed_room(change).is_some_and(|room| room.id.as_str() == other))
            .then_some(()),
        _ => None,
    });
    open.client.account().open_room(other.into());
    follow_both(&open.events, &mut mirrors, |m| {
        m.status.parent.is_none() && m.thread.is_empty()
    });
}

// ACT-004
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn a_thread_is_fetched_again_after_the_connection_comes_back() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (room, _) = admin.create_channel(&[&user.username]);
    let parent = admin.post_message(&room, "parent");
    admin.post_in_thread(&room, &parent.id, "before", false);
    let proxy = Proxy::start();
    let (client, events) = logged_in_at(&proxy.url(), &user);
    client.account().open_room(room.clone().into());
    let mut mirrors = Mirrors::default();
    follow_both(&events, &mut mirrors, |m| {
        message_saying(&m.room, "parent").is_some()
    });
    client.account().open_thread(parent.id.clone().into());
    follow_both(&events, &mut mirrors, |m| {
        message_saying(&m.thread, "before").is_some()
    });

    proxy.set_accepting(false);
    proxy.drop_connections();
    admin.post_in_thread(&room, &parent.id, "while away", false);
    proxy.set_accepting(true);

    follow_both(&events, &mut mirrors, |m| {
        texts(&m.thread) == ["parent", "before", "while away"]
    });
}

/// A client with nothing stored whose accounts each have a receiver for their own events, which
/// comes with their `Added`.
fn start_accounts() -> (Client, Receiver<(AccountId, Receiver<Event>)>) {
    let (new_tx, new_rx) = mpsc::channel();
    let senders = Mutex::new(HashMap::new());
    let client = Client::start(MemoryStore::default(), NoCache, move |id, event| {
        let mut senders = senders.lock().unwrap();
        if event == Event::Added {
            let (tx, rx) = mpsc::channel();
            senders.insert(id.clone(), tx);
            let _ = new_tx.send((id, rx));
        } else if let Some(tx) = senders.get(&id) {
            let _ = tx.send(event);
        }
    })
    .unwrap();
    (client, new_rx)
}

/// The room list, once it holds the room `wanted`.
fn rooms_with(events: &Receiver<Event>, wanted: &str) -> Vec<Room> {
    let mut rooms = Vec::new();
    wait_for(events, |event| {
        let Event::Rooms(changes) = event else {
            return None;
        };
        for change in changes {
            change.apply(&mut rooms);
        }
        rooms
            .iter()
            .any(|room| room.id.as_str() == wanted)
            .then_some(())
    });
    rooms
}

// ACCT-001
// ACCT-005
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn two_accounts_see_their_own_rooms_and_removing_one_leaves_the_other_working() {
    let admin = Admin::login();
    let alice = admin.create_user();
    let bob = admin.create_user();
    let (alice_room, _) = admin.create_channel(&[&alice.username]);
    let (bob_room, _) = admin.create_channel(&[&bob.username]);
    // The second account reaches the same server by another address.
    let proxy = Proxy::start();
    let (client, accounts) = start_accounts();

    let (a, a_events) = accounts.recv_timeout(TIMEOUT).expect("no account");
    assert_eq!(next(&a_events), SessionState::LoggedOut { failure: None });
    client
        .account(&a)
        .login(&server_url(), &alice.username, password(&alice));
    assert!(matches!(outcome(&a_events), SessionState::LoggedIn(_)));
    let alice_rooms = rooms_with(&a_events, &alice_room);

    let b = client.add_account();
    let (added, b_events) = accounts.recv_timeout(TIMEOUT).expect("no account");
    assert_eq!(added, b);
    assert_eq!(next(&b_events), SessionState::LoggedOut { failure: None });
    client
        .account(&b)
        .login(&proxy.url(), &bob.username, password(&bob));
    let SessionState::LoggedIn(account) = outcome(&b_events) else {
        panic!("login failed");
    };
    assert_eq!(account.user.username, bob.username);
    let bob_rooms = rooms_with(&b_events, &bob_room);

    // Each has its own rooms.
    assert!(alice_rooms.iter().all(|room| room.id.as_str() != bob_room));
    assert!(bob_rooms.iter().all(|room| room.id.as_str() != alice_room));

    // Removing one logs it out first, and leaves the other alone: no new account is added.
    client.remove_account(&a);
    let mut logged_out = false;
    wait_for(&a_events, |event| match event {
        Event::Session(SessionState::LoggedOut { failure: None }) => {
            logged_out = true;
            None
        }
        Event::Removed => {
            assert!(logged_out, "Removed came before the logout");
            Some(())
        }
        _ => None,
    });
    client.account(&b).open_room(bob_room.clone().into());
    let mut mirror = Vec::new();
    settle(&b_events, &mut mirror);
    client
        .account(&b)
        .send_message(bob_room.clone().into(), "from bob", None, None, false);
    follow_until(&b_events, &mut mirror, |mirror| {
        texts(mirror) == ["from bob"] && mirror.iter().all(|m| m.delivery == Delivery::Sent)
    });
    assert_eq!(admin.last_message(&bob_room).id, mirror[0].id.as_str());
    assert!(accounts.try_recv().is_err(), "an account was added");
}

// ACCT-007
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn the_same_user_cannot_be_added_twice_but_the_server_under_another_address_is_another_server() {
    let user = new_user();
    // The same server, reached by another address, counts as another server.
    let proxy = Proxy::start();
    let (client, accounts) = start_accounts();

    let (a, a_events) = accounts.recv_timeout(TIMEOUT).expect("no account");
    assert_eq!(next(&a_events), SessionState::LoggedOut { failure: None });
    client
        .account(&a)
        .login(&server_url(), &user.username, password(&user));
    assert!(matches!(outcome(&a_events), SessionState::LoggedIn(_)));

    let b = client.add_account();
    let (added, b_events) = accounts.recv_timeout(TIMEOUT).expect("no account");
    assert_eq!(added, b);
    assert_eq!(next(&b_events), SessionState::LoggedOut { failure: None });
    client
        .account(&b)
        .login(&server_url(), &user.username, password(&user));
    assert_failure(&outcome(&b_events), &LoginFailure::AlreadyAdded);

    // The account that failed is logged out, not gone: through the proxy it works.
    client
        .account(&b)
        .login(&proxy.url(), &user.username, password(&user));
    assert!(matches!(outcome(&b_events), SessionState::LoggedIn(_)));
}

/// The room list as the events say it, and the unread totals as they last said them.
#[derive(Default)]
struct Badge {
    rooms: Vec<Room>,
    unread: (u32, u32),
}

impl Badge {
    /// Takes in the event. Every `Unread` comes right after the `Rooms` that changes it, so it
    /// must equal what the room list says. Other rooms of a dirty server may change too, so that
    /// is all a test can check about the totals as a whole.
    fn follow(&mut self, event: &Event) {
        match event {
            Event::Rooms(changes) => {
                for change in changes {
                    change.apply(&mut self.rooms);
                }
            }
            Event::Unread(unread) => {
                self.unread = (unread.rooms, unread.mentions);
                assert_eq!(self.unread, self.listed_totals(), "{:?}", self.rooms);
            }
            _ => {}
        }
    }

    fn listed_totals(&self) -> (u32, u32) {
        let unread = self.rooms.iter().filter(|room| room.has_unread()).count();
        (
            u32::try_from(unread).unwrap(),
            self.rooms.iter().map(|room| room.user_mentions).sum(),
        )
    }

    /// The totals have caught up with the room list.
    fn settled(&self) -> bool {
        self.unread == self.listed_totals()
    }

    fn room(&self, id: &str) -> Option<&Room> {
        self.rooms.iter().find(|room| room.id.as_str() == id)
    }
}

// ACCT-004
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn the_unread_totals_follow_what_other_users_post() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel, _) = admin.create_channel(&[&user.username]);
    let dm = admin.create_dm(&user.username);
    let (client, events) = start();
    client
        .account()
        .login(&server_url(), &user.username, password(&user));
    assert!(matches!(outcome(&events), SessionState::LoggedIn(_)));

    let mut badge = Badge::default();
    wait_for(&events, |event| {
        badge.follow(event);
        (badge.settled() && badge.room(&channel).is_some()).then_some(())
    });

    admin.post_message(&dm, "hello");
    admin.post_message(&channel, &format!("hello @{}", user.username));
    wait_for(&events, |event| {
        badge.follow(event);
        let unread = badge.room(&dm).is_some_and(Room::has_unread);
        let mentioned = badge.room(&channel).is_some_and(|r| r.user_mentions == 1);
        (badge.settled() && unread && mentioned).then_some(())
    });
    assert!(
        badge.unread.0 >= 2 && badge.unread.1 >= 1,
        "{:?}",
        badge.unread
    );

    client.account().mark_read(dm.clone().into());
    wait_for(&events, |event| {
        badge.follow(event);
        let read = badge.room(&dm).is_some_and(|room| !room.has_unread());
        (badge.settled() && read).then_some(())
    });
    // The channel's mention is still counted.
    assert!(
        badge.unread.0 >= 1 && badge.unread.1 >= 1,
        "{:?}",
        badge.unread
    );
}

/// A channel with the threads "t0", "t1" and "t2" (the parents posted by admin in that order, one
/// reply each, in the order t1, t2, t0, so that "t0" has the most recent reply), opened by the
/// user. The user's thread list is not open.
fn open_channel_with_threads() -> (OwnMessages, Mirrors, Vec<String>, Admin) {
    let admin = Admin::login();
    let user = admin.create_user();
    let (room, room_name) = admin.create_channel(&[&user.username]);
    let parents: Vec<String> = (0..3)
        .map(|n| admin.post_message(&room, &format!("t{n}")).id)
        .collect();
    for n in [1, 2, 0] {
        admin.post_in_thread(&room, &parents[n], &format!("reply to t{n}"), false);
        std::thread::sleep(Duration::from_millis(20));
    }
    let (client, events) = logged_in(&user);
    client.account().open_room(room.clone().into());
    let mut mirrors = Mirrors::default();
    follow_both(&events, &mut mirrors, |m| !m.room.is_empty());
    let open = OwnMessages {
        admin,
        username: user.username.clone(),
        room,
        room_name,
        ids: parents.clone(),
        client,
        events,
        mirror: Vec::new(),
    };
    let me = Admin::login_as(&user.username, &user.password);
    (open, mirrors, parents, me)
}

fn listed(mirrors: &Mirrors) -> Vec<&str> {
    texts(&mirrors.list)
}

/// Follows the events until the list has loaded its first page.
fn list_loaded(open: &OwnMessages, mirrors: &mut Mirrors) {
    follow_both(&open.events, mirrors, |m| {
        m.list_status.room.is_some() && !m.list_status.loading
    });
}

// ACT-010
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn the_thread_list_loads_the_rooms_threads_and_a_thread_opens_from_it() {
    let (open, mut mirrors, parents, _me) = open_channel_with_threads();

    open.client.account().open_thread_list(ThreadFilter::All);
    list_loaded(&open, &mut mirrors);

    assert_eq!(listed(&mirrors), ["t0", "t2", "t1"]);
    assert!(mirrors.list_status.reached_end);
    assert_eq!(mirrors.list[0].id.as_str(), parents[0]);
    assert_eq!(mirrors.list[0].thread_replies, 1);
    assert_eq!(mirrors.list_status.filter, ThreadFilter::All);

    // The parent comes from the list when the room doesn't hold it: here it does, and the thread
    // opens either way.
    open.client.account().open_thread(parents[2].clone().into());
    follow_both(&open.events, &mut mirrors, |m| {
        m.status.parent.is_some() && !m.status.loading && m.status.reached_start
    });
    assert_eq!(texts(&mirrors.thread), ["t2", "reply to t2"]);

    // The thread closes without the list.
    open.client.account().close_thread();
    follow_both(&open.events, &mut mirrors, |m| m.status.parent.is_none());
    assert_eq!(listed(&mirrors), ["t0", "t2", "t1"]);
    open.client.account().close_thread_list();
    follow_both(&open.events, &mut mirrors, |m| m.list_status.room.is_none());
    assert!(mirrors.list.is_empty());
}

// ACT-010
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn the_followed_threads_are_the_ones_the_user_started_or_replied_to() {
    let (open, mut mirrors, parents, me) = open_channel_with_threads();
    open.client
        .account()
        .open_thread_list(ThreadFilter::Following);
    list_loaded(&open, &mut mirrors);
    assert!(mirrors.list.is_empty());
    assert_eq!(mirrors.list_status.filter, ThreadFilter::Following);

    // Admin replied to every thread, and the user replies to t2: only t2 has them as a follower.
    me.post_in_thread(&open.room, &parents[2], "mine", false);
    follow_both(&open.events, &mut mirrors, |m| {
        listed(m) == ["t2"] && m.list[0].thread_replies == 2
    });

    // Another filter starts again.
    open.client.account().open_thread_list(ThreadFilter::All);
    follow_both(&open.events, &mut mirrors, |m| {
        m.list_status.filter == ThreadFilter::All && !m.list_status.loading && m.list.len() == 3
    });
    assert_eq!(listed(&mirrors), ["t2", "t0", "t1"]);
}

// ACT-010
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn a_live_reply_moves_its_thread_to_the_front_and_a_first_reply_adds_one() {
    let (open, mut mirrors, parents, _me) = open_channel_with_threads();
    open.client.account().open_thread_list(ThreadFilter::All);
    list_loaded(&open, &mut mirrors);

    open.admin
        .post_in_thread(&open.room, &parents[1], "a late reply", false);
    follow_both(&open.events, &mut mirrors, |m| {
        listed(m) == ["t1", "t0", "t2"] && m.list[0].thread_replies == 2
    });

    // A message that gets its first reply is a thread from then on.
    let fresh = open.admin.post_message(&open.room, "fresh").id;
    open.admin
        .post_in_thread(&open.room, &fresh, "the first reply", false);
    follow_both(&open.events, &mut mirrors, |m| {
        listed(m) == ["fresh", "t1", "t0", "t2"]
    });

    // A parent deleted while it has replies leaves the list.
    open.admin.delete_message(&open.room, &parents[0]);
    follow_both(&open.events, &mut mirrors, |m| {
        listed(m) == ["fresh", "t1", "t2"]
    });
}

// ACT-010
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn a_thread_the_user_unfollowed_leaves_the_followed_list_with_its_next_push() {
    let (open, mut mirrors, parents, me) = open_channel_with_threads();
    open.client
        .account()
        .open_thread_list(ThreadFilter::Following);
    list_loaded(&open, &mut mirrors);
    me.post_in_thread(&open.room, &parents[1], "I follow this", false);
    follow_both(&open.events, &mut mirrors, |m| listed(m) == ["t1"]);

    // Nothing announces the unfollow: the row stays...
    me.follow_message(&parents[1], false);
    std::thread::sleep(Duration::from_millis(500));
    follow_both(&open.events, &mut mirrors, |m| listed(m) == ["t1"]);
    // ...until the thread is pushed again, without the user among its followers.
    open.admin
        .post_in_thread(&open.room, &parents[1], "somebody else", false);
    follow_both(&open.events, &mut mirrors, |m| m.list.is_empty());
}

// ACT-010
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn the_thread_list_pages_until_the_last_thread() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (room, _) = admin.create_channel(&[&user.username]);
    let total = PAGE_SIZE + 3;
    for n in 0..total {
        let parent = admin.post_message(&room, &format!("thread {n}")).id;
        admin.post_in_thread(&room, &parent, "reply", false);
    }
    let (client, events) = logged_in(&user);
    client.account().open_room(room.clone().into());
    let mut mirrors = Mirrors::default();
    follow_both(&events, &mut mirrors, |m| !m.room.is_empty());

    client.account().open_thread_list(ThreadFilter::All);
    follow_both(&events, &mut mirrors, |m| {
        m.list_status.room.is_some() && !m.list_status.loading
    });
    assert_eq!(mirrors.list.len(), PAGE_SIZE);
    assert!(!mirrors.list_status.reached_end);

    client.account().load_more_threads();
    follow_both(&events, &mut mirrors, |m| m.list_status.reached_end);
    assert_eq!(mirrors.list.len(), total);
    // The most recent last reply first: the threads were made in order.
    assert_eq!(texts(&mirrors.list)[0], format!("thread {}", total - 1));
    assert_eq!(texts(&mirrors.list)[total - 1], "thread 0");
}

/// The open room, as the room list holds it.
fn room_of<'a>(mirrors: &'a Mirrors, open: &OwnMessages) -> Option<&'a Room> {
    mirrors
        .rooms
        .iter()
        .find(|room| room.id.as_str() == open.room)
}

/// A room opened by the user where they started the thread "mine" (so they follow it) and admin
/// the thread "theirs". The room list is mirrored too.
fn open_channel_for_unread_threads() -> (OwnMessages, Mirrors, String, String, Admin) {
    let admin = Admin::login();
    let user = admin.create_user();
    let (room, room_name) = admin.create_channel(&[&user.username]);
    let me = Admin::login_as(&user.username, &user.password);
    let mine = me.post_message(&room, "mine").id;
    let theirs = admin.post_message(&room, "theirs").id;
    let (client, events) = logged_in(&user);
    let mut mirrors = Mirrors::default();
    // The rooms came with the login: the mirror starts from what the listing says.
    client.account().open_room(room.clone().into());
    let open = OwnMessages {
        admin,
        username: user.username.clone(),
        room,
        room_name,
        ids: Vec::new(),
        client,
        events,
        mirror: Vec::new(),
    };
    follow_both(&open.events, &mut mirrors, |m| !m.room.is_empty());
    (open, mirrors, mine, theirs, me)
}

// ACT-011
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn the_rooms_unread_threads_follow_the_replies_and_the_mentions_and_the_unread_list_with_them() {
    let (open, mut mirrors, mine, theirs, _me) = open_channel_for_unread_threads();
    open.client.account().open_thread_list(ThreadFilter::Unread);
    list_loaded(&open, &mut mirrors);
    assert!(mirrors.list.is_empty());

    // A reply to the user's own thread, then one that mentions them in a thread they don't follow.
    open.admin.post_in_thread(&open.room, &mine, "plain", false);
    follow_both(&open.events, &mut mirrors, |m| {
        listed(m) == ["mine"]
            && room_of(m, &open).is_some_and(|room| room.unread_threads.len() == 1)
    });
    open.admin.post_in_thread(
        &open.room,
        &theirs,
        &format!("hey @{}", open.username),
        false,
    );
    follow_both(&open.events, &mut mirrors, |m| {
        listed(m) == ["theirs", "mine"]
            && room_of(m, &open).is_some_and(|room| {
                room.unread_threads.len() == 2 && room.mentioned_threads == [theirs.clone().into()]
            })
    });
}

// ACT-011
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn opening_an_unread_thread_reads_it_and_it_leaves_the_unread_list() {
    let (open, mut mirrors, mine, _theirs, _me) = open_channel_for_unread_threads();
    open.client.account().open_thread_list(ThreadFilter::Unread);
    list_loaded(&open, &mut mirrors);
    open.admin.post_in_thread(&open.room, &mine, "plain", false);
    follow_both(&open.events, &mut mirrors, |m| listed(m) == ["mine"]);

    open.client.account().open_thread(mine.clone().into());

    // The core calls `readThreads` and the server pushes the subscription without the thread.
    follow_both(&open.events, &mut mirrors, |m| {
        m.list.is_empty() && room_of(m, &open).is_some_and(|room| room.unread_threads.is_empty())
    });
    // A reply that comes while it is open is read as well.
    open.admin
        .post_in_thread(&open.room, &mine, "while open", false);
    follow_both(&open.events, &mut mirrors, |m| {
        message_saying(&m.thread, "while open").is_some()
    });
    std::thread::sleep(Duration::from_millis(500));
    follow_queued(&open.events, &mut mirrors, |m| {
        room_of(m, &open).is_some_and(|room| room.unread_threads.is_empty())
    });
}
