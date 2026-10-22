// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Avatars of rooms and users.

use super::*;

// Scripts take the result of the call, which is what the other scripts pass.
#[allow(clippy::unnecessary_wraps)]
fn png(bytes: &[u8]) -> Result<Option<Image>, ApiError> {
    Ok(Some(Image {
        bytes: bytes.to_vec(),
        content_type: Some("image/png".to_owned()),
    }))
}

fn user_owner(name: &str) -> AvatarOwner {
    AvatarOwner::User(name.to_owned())
}

fn room_owner(id: &str) -> AvatarOwner {
    AvatarOwner::Room(id.to_owned().into())
}

/// A message of room `1` written by `author`.
fn message_by(id: &str, secs: i64, author: &str) -> Message {
    let mut message = message(id, secs);
    message.author.username = author.to_owned();
    message
}

fn update(owner: AvatarOwner, path: &str) -> AvatarUpdate {
    AvatarUpdate {
        owner,
        path: Some(PathBuf::from(path)),
    }
}

// RICH-004
#[test]
fn cached_avatars_of_the_rooms_are_sent_at_once() {
    let fake = Fake::new();
    let cache = FakeCache::default();
    cache.seed("h", &room_owner("1"), "png", b"cached");
    let harness = logged_in_with_cache_events(&fake, &cache);
    assert_eq!(
        harness.next_avatars(),
        [update(room_owner("1"), "/cache/h/room/1.png")]
    );
    // Nothing for the room that has none. The downloads run at once, in any order.
    let calls = fake.wait_for_avatar_calls(2);
    assert_eq!(calls.len(), 2, "{calls:?}");
    assert!(calls.contains(&room_owner("1")), "{calls:?}");
    assert!(calls.contains(&room_owner("2")), "{calls:?}");
}

/// Like [`logged_in_with_cache`], for a cache that makes `Avatars` events come in between.
fn logged_in_with_cache_events(fake: &Fake, cache: &FakeCache) -> Harness {
    fake.script_login(None, Ok(()));
    fake.script_fetch(
        None,
        Ok(vec![
            avatar_subscription("1", "general", rc_api::RoomType::Channel),
            avatar_subscription("2", "secret", rc_api::RoomType::Private),
        ]),
    );
    let harness = Harness::with_cache(fake, cache.clone());
    harness.login("http://h", "alice");
    harness
}

// RICH-004
#[test]
fn a_downloaded_avatar_is_sent_once_it_is_stored() {
    let fake = Fake::new();
    fake.script_avatar(room_owner("1"), None, png(b"one"));
    let cache = FakeCache::default();
    let harness = logged_in_with_cache_events(&fake, &cache);
    assert_eq!(
        harness.next_avatars(),
        [update(room_owner("1"), "/cache/h/room/1.png")]
    );
    assert_eq!(cache.bytes("h", &room_owner("1")), Some(b"one".to_vec()));
}

// RICH-004
#[test]
fn a_direct_message_shows_the_avatar_of_the_other_user() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    fake.script_fetch(
        None,
        Ok(vec![
            avatar_subscription("d1", "bob", rc_api::RoomType::Direct),
            avatar_subscription("d2", "bob,carol", rc_api::RoomType::Direct),
        ]),
    );
    let harness = Harness::with_cache(&fake, FakeCache::default());
    harness.login("http://h", "alice");
    let calls = fake.wait_for_avatar_calls(2);
    assert!(calls.contains(&user_owner("bob")), "{calls:?}");
    assert!(!calls.contains(&room_owner("d1")), "{calls:?}");
    // A group conversation has no other user to show.
    assert!(calls.contains(&room_owner("d2")), "{calls:?}");
    drop(harness);
}

// RICH-004
#[test]
fn the_authors_of_the_open_room_get_their_avatars() {
    let fake = Fake::new();
    let cache = FakeCache::default();
    cache.seed("h", &user_owner("bob"), "png", b"cached");
    fake.script_avatar(user_owner("bob"), None, png(b"cached"));
    fake.script_avatar(user_owner("carol"), None, png(b"carol"));
    let harness = logged_in_with_cache(&fake, &cache);
    fake.script_history(
        None,
        page(
            vec![message_by("b", 2, "carol"), message_by("a", 1, "bob")],
            true,
        ),
    );
    harness.account().open_room("1".to_owned().into());

    let mut sent = harness.next_avatars();
    sent.extend(harness.next_avatars());
    assert!(sent.contains(&update(user_owner("bob"), "/cache/h/user/bob.png")));
    assert!(sent.contains(&update(user_owner("carol"), "/cache/h/user/carol.png")));
}

// RICH-004
#[test]
fn the_author_of_a_live_message_gets_an_avatar_and_system_messages_none() {
    let fake = Fake::new();
    fake.script_avatar(user_owner("dave"), None, png(b"dave"));
    let cache = FakeCache::default();
    let harness = logged_in_with_cache(&fake, &cache);
    open_first_room_of(&fake, &harness);
    let mut joined = message_by("j", 9, "erin");
    joined.kind = MessageKind::System(crate::domain::SystemEvent::Joined);
    fake.push(0, Pushed::Message(joined));
    fake.push(0, Pushed::Message(message_by("c", 3, "dave")));

    assert_eq!(
        harness.next_avatars(),
        [update(user_owner("dave"), "/cache/h/user/dave.png")]
    );
    let calls = fake.state().avatar_calls.clone();
    assert!(!calls.contains(&user_owner("erin")), "{calls:?}");
}

// RICH-004
#[test]
fn an_avatar_is_downloaded_once_per_session_however_often_it_is_wanted() {
    let fake = Fake::new();
    let gate = Arc::new(Notify::new());
    fake.script_avatar(user_owner("bob"), Some(gate.clone()), png(b"bob"));
    let harness = logged_in_with_cache(&fake, &FakeCache::default());
    fake.script_history(
        None,
        page(
            vec![message_by("b", 2, "bob"), message_by("a", 1, "bob")],
            true,
        ),
    );
    harness.account().open_room("1".to_owned().into());
    fake.wait_for_avatar_calls(3);
    // Held by the gate: a live message from the same author finds the download in flight.
    fake.push(0, Pushed::Message(message_by("c", 3, "bob")));
    gate.notify_one();
    assert_eq!(
        harness.next_avatars(),
        [update(user_owner("bob"), "/cache/h/user/bob.png")]
    );
    fake.push(0, Pushed::Message(message_by("d", 4, "bob")));
    harness.wait_for_appended("d");
    let calls = fake.state().avatar_calls.clone();
    assert_eq!(
        calls
            .iter()
            .filter(|owner| **owner == user_owner("bob"))
            .count(),
        1,
        "{calls:?}"
    );
}

// RICH-004
#[test]
fn no_more_than_four_avatars_download_at_once() {
    let fake = Fake::new();
    let gate = Arc::new(Notify::new());
    let names = ["1", "2", "3", "4", "5", "6"];
    for name in names {
        fake.script_avatar(room_owner(name), Some(gate.clone()), png(name.as_bytes()));
    }
    fake.script_login(None, Ok(()));
    fake.script_fetch(
        None,
        Ok(names
            .iter()
            .map(|name| avatar_subscription(name, name, rc_api::RoomType::Channel))
            .collect()),
    );
    let harness = Harness::with_cache(&fake, FakeCache::default());
    harness.login("http://h", "alice");

    fake.wait_for_avatar_calls(4);
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(fake.state().avatar_calls.len(), 4);
    assert_eq!(fake.state().avatar_max_running, 4);

    // One finishing lets the next start.
    gate.notify_one();
    fake.wait_for_avatar_calls(5);
    gate.notify_waiters();
    fake.wait_for_avatar_calls(6);
    assert!(fake.state().avatar_max_running <= 4);
    drop(harness);
}

// RICH-004
#[test]
fn an_avatar_that_is_the_same_as_the_cached_one_sends_nothing_more() {
    let fake = Fake::new();
    let cache = FakeCache::default();
    cache.seed("h", &room_owner("1"), "png", b"same");
    fake.script_avatar(room_owner("1"), None, png(b"same"));
    let harness = logged_in_with_cache_events(&fake, &cache);
    assert_eq!(
        harness.next_avatars(),
        [update(room_owner("1"), "/cache/h/room/1.png")]
    );
    fake.wait_for_avatar_calls(2);
    harness.assert_no_avatars();
}

// RICH-004
#[test]
fn an_avatar_that_differs_from_the_cached_one_is_sent_again() {
    let fake = Fake::new();
    let cache = FakeCache::default();
    cache.seed("h", &room_owner("1"), "png", b"old");
    fake.script_avatar(room_owner("1"), None, png(b"new"));
    let harness = logged_in_with_cache_events(&fake, &cache);
    let path = "/cache/h/room/1.png";
    assert_eq!(harness.next_avatars(), [update(room_owner("1"), path)]);
    assert_eq!(harness.next_avatars(), [update(room_owner("1"), path)]);
    assert_eq!(cache.bytes("h", &room_owner("1")), Some(b"new".to_vec()));
}

// RICH-004
#[test]
fn an_avatar_of_another_type_replaces_the_cached_one() {
    let fake = Fake::new();
    let cache = FakeCache::default();
    cache.seed("h", &room_owner("1"), "svg", b"<svg/>");
    fake.script_avatar(room_owner("1"), None, png(b"png"));
    let harness = logged_in_with_cache_events(&fake, &cache);
    assert_eq!(
        harness.next_avatars(),
        [update(room_owner("1"), "/cache/h/room/1.svg")]
    );
    assert_eq!(
        harness.next_avatars(),
        [update(room_owner("1"), "/cache/h/room/1.png")]
    );
}

// RICH-004
#[test]
fn a_missing_or_refused_avatar_is_no_avatar_and_nothing_else_happens() {
    for error in [
        ApiError::Server {
            status: 404,
            message: String::new(),
        },
        ApiError::Unauthorized,
        ApiError::Unexpected("odd".into()),
    ] {
        let fake = Fake::new();
        fake.script_avatar(room_owner("1"), None, Err(error));
        let harness = logged_in_with_cache(&fake, &FakeCache::default());
        fake.wait_for_avatar_calls(2);
        harness.assert_no_avatars();
        // The session carries on: opening a room still works.
        open_first_room_of(&fake, &harness);
    }
}

// RICH-004
#[test]
fn an_avatar_of_a_type_the_cache_doesnt_keep_is_no_avatar() {
    let fake = Fake::new();
    fake.script_avatar(
        room_owner("1"),
        None,
        Ok(Some(Image {
            bytes: b"<html>".to_vec(),
            content_type: Some("text/html".to_owned()),
        })),
    );
    let cache = FakeCache::default();
    let harness = logged_in_with_cache(&fake, &cache);
    fake.wait_for_avatar_calls(2);
    harness.assert_no_avatars();
    assert_eq!(cache.bytes("h", &room_owner("1")), None);
}

// RICH-004
#[test]
fn an_avatar_that_couldnt_be_downloaded_is_tried_again_after_reconnecting() {
    let fake = Fake::new();
    fake.script_avatar(
        room_owner("1"),
        None,
        Err(ApiError::Unreachable("down".into())),
    );
    fake.script_avatar(room_owner("1"), None, png(b"one"));
    fake.script_avatar(room_owner("2"), None, png(b"two"));
    fake.script_login(None, Ok(()));
    let rooms = || {
        Ok(vec![
            avatar_subscription("1", "general", rc_api::RoomType::Channel),
            avatar_subscription("2", "secret", rc_api::RoomType::Private),
        ])
    };
    fake.script_fetch(None, rooms());
    fake.script_fetch(None, rooms());
    let harness = Harness::with_cache_and_backoff(&fake, FakeCache::default(), fast());
    harness.login("http://h", "alice");
    // Room 2's came; room 1's didn't.
    assert_eq!(
        harness.next_avatars(),
        [update(room_owner("2"), "/cache/h/room/2.png")]
    );

    fake.end_connection(0);

    assert_eq!(
        harness.next_avatars(),
        [update(room_owner("1"), "/cache/h/room/1.png")]
    );
    let calls = fake.state().avatar_calls.clone();
    // Room 2 had its refresh already.
    assert_eq!(
        calls
            .iter()
            .filter(|owner| **owner == room_owner("2"))
            .count(),
        1,
        "{calls:?}"
    );
}

// RICH-004
#[test]
fn logging_out_forgets_the_avatars_and_drops_what_was_still_downloading() {
    let fake = Fake::new();
    let cache = FakeCache::default();
    cache.seed("h", &room_owner("1"), "png", b"cached");
    let gate = Arc::new(Notify::new());
    fake.script_avatar(room_owner("1"), None, png(b"cached"));
    fake.script_avatar(room_owner("2"), Some(gate.clone()), png(b"two"));
    let harness = logged_in_with_cache_events(&fake, &cache);
    assert_eq!(
        harness.next_avatars(),
        [update(room_owner("1"), "/cache/h/room/1.png")]
    );
    fake.wait_for_avatar_calls(2);

    harness.logout();
    gate.notify_waiters();

    assert_eq!(
        harness.next_avatars(),
        [AvatarUpdate {
            owner: room_owner("1"),
            path: None
        }]
    );
    harness.assert_no_avatars();
    // The cache keeps the file for the next session.
    assert_eq!(cache.bytes("h", &room_owner("1")), Some(b"cached".to_vec()));
}

// RICH-004
#[test]
fn the_cache_is_per_server() {
    let fake = Fake::new();
    let cache = FakeCache::default();
    cache.seed("other:3000", &room_owner("1"), "png", b"another server's");
    let harness = logged_in_with_cache_events(&fake, &cache);
    fake.wait_for_avatar_calls(2);
    harness.assert_no_avatars();
}

// RICH-004
#[test]
fn a_pushed_avatar_change_fetches_it_again_and_shows_the_new_file() {
    let fake = Fake::new();
    let cache = FakeCache::default();
    cache.seed("h", &room_owner("1"), "png", b"old");
    fake.script_avatar(room_owner("1"), None, png(b"old"));
    fake.script_avatar(room_owner("1"), None, png(b"new"));
    let harness = logged_in_with_cache_events(&fake, &cache);
    assert_eq!(
        harness.next_avatars(),
        [update(room_owner("1"), "/cache/h/room/1.png")]
    );
    fake.wait_for_avatar_calls(2);

    assert!(fake.push(0, Pushed::AvatarChanged(room_owner("1"))));

    assert_eq!(
        harness.next_avatars(),
        [update(room_owner("1"), "/cache/h/room/1.png")]
    );
    assert_eq!(cache.bytes("h", &room_owner("1")), Some(b"new".to_vec()));
}

// RICH-004
#[test]
fn a_pushed_change_of_an_avatar_nobody_looked_at_is_ignored() {
    let fake = Fake::new();
    let harness = logged_in_with_cache(&fake, &FakeCache::default());
    fake.wait_for_avatar_calls(2);

    assert!(fake.push(0, Pushed::AvatarChanged(user_owner("stranger"))));

    harness.assert_no_avatars();
    assert_eq!(fake.state().avatar_calls.len(), 2);
}

// RICH-004
#[test]
fn a_room_without_an_avatar_of_its_own_is_not_asked_for_one_but_a_direct_message_is() {
    let fake = Fake::new();
    fake.script_login(None, Ok(()));
    fake.script_fetch(
        None,
        Ok(vec![
            subscription("1", "general", rc_api::RoomType::Channel),
            subscription("2", "secret", rc_api::RoomType::Private),
            subscription("d1", "bob", rc_api::RoomType::Direct),
        ]),
    );
    let harness = Harness::with_cache(&fake, FakeCache::default());
    harness.login("http://h", "alice");

    // The other user may have one: the server says when asked.
    assert_eq!(fake.wait_for_avatar_calls(1), [user_owner("bob")]);
    harness.assert_no_avatars();
    assert_eq!(fake.state().avatar_calls.len(), 1);
}

// RICH-004
#[test]
fn an_avatar_the_server_doesnt_have_goes_from_the_screen_and_the_cache() {
    let fake = Fake::new();
    let cache = FakeCache::default();
    // Cached when the user had one, or when the server drew one.
    cache.seed("h", &room_owner("1"), "png", b"stale");
    fake.script_avatar(room_owner("1"), None, Ok(None));
    let harness = logged_in_with_cache_events(&fake, &cache);

    assert_eq!(
        harness.next_avatars(),
        [update(room_owner("1"), "/cache/h/room/1.png")]
    );
    assert_eq!(
        harness.next_avatars(),
        [AvatarUpdate {
            owner: room_owner("1"),
            path: None
        }]
    );
    assert_eq!(cache.bytes("h", &room_owner("1")), None);
}

// RICH-004
#[test]
fn a_pushed_avatar_change_of_a_listed_room_looks_at_it_even_without_an_etag() {
    let fake = Fake::new();
    fake.script_avatar(room_owner("1"), None, png(b"new"));
    let harness = logged_in_harness(&fake);
    assert!(fake.state().avatar_calls.is_empty());

    assert!(fake.push(0, Pushed::AvatarChanged(room_owner("1"))));

    assert_eq!(
        harness.next_avatars(),
        [update(room_owner("1"), "/cache/h/room/1.png")]
    );
}
