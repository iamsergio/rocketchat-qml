// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The client against a real Rocket.Chat server, named by `RC_TEST_SERVER_URL`. Run with
//! `--ignored` (see the README).
//!
//! Every test makes its own user and rooms with unique names and only looks at those, so the tests
//! hold on a dirty server, in any order and in parallel. Server settings are never changed. Admin
//! only sets things up: no test logs a shared account in or out through the client.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::net::TcpListener;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use rc_core::adapters::{MemoryStore, RestConnector};
use rc_core::domain::{Delivery, Message, MessageId, MessageKind, Room, RoomId, RoomKind};
use rc_core::ports::{Connector, SecretStore, ServerApi};
use rc_core::rich::{Block, Emoji, Inline, Mention, TaskItem};
use rc_core::rooms::RoomChange;
use rc_core::session::{LoginFailure, SessionState};
use rc_core::timeline::{PAGE_SIZE, TimelineChange, TimelineStatus};
use rc_core::{Client, Event};
use rc_test_server::{Admin, Proxy, TestUser, server_url, unique};
use secrecy::SecretString;

const TIMEOUT: Duration = Duration::from_secs(15);

/// A new user, who can log in at once.
fn new_user() -> TestUser {
    Admin::login().create_user()
}

fn password(user: &TestUser) -> SecretString {
    SecretString::from(user.password.clone())
}

/// A client with nothing stored, past its first event.
fn start() -> (Client, Receiver<Event>) {
    let (client, events) = start_with(MemoryStore::default());
    assert_eq!(next(&events), SessionState::LoggedOut { failure: None });
    (client, events)
}

/// A client on `store`; the first event is left to the caller.
fn start_with(store: MemoryStore) -> (Client, Receiver<Event>) {
    let (tx, rx) = mpsc::channel();
    let client = Client::start(store, move |event| {
        let _ = tx.send(event);
    })
    .unwrap();
    (client, rx)
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
    client.login(&server_url(), &user.username, password(&user));
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
    client.login(&server_url(), &username, SecretString::from("whatever"));
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
    client.login(
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
    client.login(&server_url(), &user.username, password(&user));
    assert!(matches!(outcome(&events), SessionState::LoggedIn(_)));
    client.logout();
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
    first.login(&server_url(), &user.username, password(&user));
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

    second.logout();
    assert_eq!(next(&events), SessionState::LoggedOut { failure: None });
    // Dropping flushes the queued clear.
    drop(second);
    assert!(store.load().unwrap().is_none());
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
    client.login(&server_url(), &user.username, password(&user));
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
fn logged_in(user: &TestUser) -> (Client, Receiver<Event>) {
    logged_in_at(&server_url(), user)
}

/// As [`logged_in`], through the server at `server`.
fn logged_in_at(server: &str, user: &TestUser) -> (Client, Receiver<Event>) {
    let (client, events) = start();
    client.login(server, &user.username, password(user));
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

    client.open_room(id.clone().into());
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
    client: &Client,
    events: &Receiver<Event>,
    mirror: &mut Vec<Message>,
) -> TimelineStatus {
    client.load_older();
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
        RoomChange::Inserted { room, .. }
        | RoomChange::Updated { room, .. }
        | RoomChange::Moved { room, .. } => Some(room),
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
    client.open_room(id.clone().into());
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
    client.open_room(id.clone().into());
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
            RoomChange::Inserted { room, .. } if room.id.as_str() == id => Some(room.clone()),
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

/// The room `id` as a freshly logged-in client of `user` lists it.
fn listed_room(user: &TestUser, id: &str) -> Room {
    let (client, events) = start();
    client.login(&server_url(), &user.username, password(user));
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

    client.open_room(dm.clone().into());

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
    client.open_room(dm.clone().into());
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
    client.login(&server_url(), &user.username, password(&user));
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
            moved |= matches!(change, RoomChange::Moved { room, .. } if room.id.as_str() == first);
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
    client.open_room(id.clone().into());
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
                    if let RoomChange::Inserted { room, .. } = change
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
    client.open_room(room.clone().into());
    let mut mirror = Vec::new();
    settle(&events, &mut mirror);

    client.send_message(room.clone().into(), "hello from the client");

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
    client.open_room(room.clone().into());
    let mut mirror = Vec::new();
    settle(&events, &mut mirror);

    let before = admin.last_message(&room).id;
    client.send_message(room.clone().into(), "sent and left");
    client.open_room(other.clone().into());
    settle(&events, &mut mirror);
    // Nothing is shown of it here; it arrives on the server all the same.
    let deadline = Instant::now() + TIMEOUT;
    while admin.last_message(&room).id == before {
        assert!(Instant::now() < deadline, "the message never arrived");
        std::thread::sleep(Duration::from_millis(100));
    }

    client.open_room(room.clone().into());
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
        .send_message(&credentials, &room_id, &id, "once")
        .await
        .unwrap();
    let second = api
        .send_message(&credentials, &room_id, &id, "once")
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

// SEND-002
#[test]
#[ignore = "needs a Rocket.Chat server: set RC_TEST_SERVER_URL"]
fn a_message_that_fails_while_the_server_is_out_of_reach_is_retried_under_its_id() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (room, _) = admin.create_channel(&[&user.username]);
    let proxy = Proxy::start();
    let (client, events) = logged_in_at(&proxy.url(), &user);
    client.open_room(room.clone().into());
    let mut mirror = Vec::new();
    settle(&events, &mut mirror);

    proxy.set_accepting(false);
    proxy.drop_connections();
    client.send_message(room.clone().into(), "sent in the dark");
    follow_until(&events, &mut mirror, |mirror| {
        mirror.iter().any(|m| m.delivery == Delivery::Failed)
    });
    let id = unsent_id(&mirror).unwrap();
    assert_eq!(texts(&mirror), ["sent in the dark"]);

    proxy.set_accepting(true);
    client.retry_message(id.clone().into());
    follow_until(&events, &mut mirror, |mirror| is_sent(mirror, &id));

    assert_eq!(texts(&mirror), ["sent in the dark"]);
    assert_eq!(admin.last_message(&room).id, id);
    drop(client);
}
