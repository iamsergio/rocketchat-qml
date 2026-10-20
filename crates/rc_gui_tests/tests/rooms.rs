// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The room list, against the local test server (`RC_TEST_SERVER_URL`). Each test makes its own
//! user and rooms and only looks at those.
//!
//! A new user also joins `general`, so the list holds about four rooms: they fit in the window and
//! every row exists. A longer list would need scrolling to create the rows below the fold.

mod common;

use rc_gui_tests::App;
use rc_gui_tests::paths::{room, room_row};
use rc_test_server::Admin;

use common::{TIMEOUT, log_in_and_wait_for_room, open_room, row_y};

// ROOM-001, ROOM-002
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn the_rooms_are_listed_with_their_names_and_kinds() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, channel_name) = admin.create_channel(&[&user.username]);
    let (group_id, group_name) = admin.create_group(&[&user.username]);
    let dm_id = admin.create_dm(&user.username);
    // The server hides a direct message from the other person until it has a message, and the
    // list skips hidden rooms.
    admin.post_message(&dm_id, "hello");
    let app = App::launch().unwrap();
    let gui = app.gui();

    log_in_and_wait_for_room(gui, &user, &dm_id);
    for id in [&channel_id, &group_id] {
        gui.wait_until_visible(&room(id, room_row::NAME), TIMEOUT)
            .unwrap();
    }

    let name = |id: &str| gui.text(&room(id, room_row::NAME)).unwrap();
    assert_eq!(name(&channel_id), channel_name);
    assert_eq!(name(&group_id), group_name);
    // A direct message is named after the other person.
    let admin_name = admin.name.as_deref().unwrap_or(&admin.username);
    assert_eq!(name(&dm_id), admin_name);
    // Each row has the icon of its kind. The icons only show once their SVG has loaded, so this
    // also fails if the Qt SVG image plugin is missing.
    for (id, kind) in [
        (&channel_id, room_row::KIND_CHANNEL),
        (&group_id, room_row::KIND_PRIVATE),
        (&dm_id, room_row::KIND_DIRECT),
    ] {
        gui.wait_until_visible(&room(id, kind), TIMEOUT).unwrap();
    }
}

// ROOM-003
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn unread_and_mention_counts_are_shown() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let (group_id, _) = admin.create_group(&[&user.username]);
    let dm_id = admin.create_dm(&user.username);
    // Before the user logs in, so the counts are part of the first room list.
    admin.post_message(&channel_id, &format!("hello @{}", user.username));
    admin.post_message(&dm_id, "hi there");
    let app = App::launch().unwrap();
    let gui = app.gui();

    log_in_and_wait_for_room(gui, &user, &group_id);

    // The counts can lag behind the rows on the server side.
    gui.wait_until_text(&room(&channel_id, room_row::UNREAD), "1", TIMEOUT)
        .unwrap();
    gui.wait_until_text(&room(&channel_id, room_row::MENTIONS), "1", TIMEOUT)
        .unwrap();
    gui.wait_until_text(&room(&dm_id, room_row::UNREAD), "1", TIMEOUT)
        .unwrap();
    // A message that doesn't mention the user shows no mention marker.
    assert!(
        !gui.exists_and_visible(&room(&dm_id, room_row::MENTIONS))
            .unwrap()
    );
    // Nothing was posted to the group.
    assert!(
        !gui.exists_and_visible(&room(&group_id, room_row::UNREAD))
            .unwrap()
    );
    assert!(
        !gui.exists_and_visible(&room(&group_id, room_row::MENTIONS))
            .unwrap()
    );
}

// ROOM-004
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn opening_a_room_clears_its_badges() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let (other_id, _) = admin.create_channel(&[&user.username]);
    let dm_id = admin.create_dm(&user.username);
    admin.post_message(&channel_id, &format!("hello @{}", user.username));
    admin.post_message(&other_id, &format!("hello @{}", user.username));
    admin.post_message(&dm_id, "hi there");
    let app = App::launch().unwrap();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &dm_id);
    for id in [&channel_id, &other_id] {
        gui.wait_until_text(&room(id, room_row::MENTIONS), "1", TIMEOUT)
            .unwrap();
    }
    gui.wait_until_text(&room(&dm_id, room_row::UNREAD), "1", TIMEOUT)
        .unwrap();

    open_room(gui, &channel_id);

    gui.wait_until_hidden(&room(&channel_id, room_row::MENTIONS), TIMEOUT)
        .unwrap();
    gui.wait_until_hidden(&room(&channel_id, room_row::UNREAD), TIMEOUT)
        .unwrap();
    // Only the opened room.
    assert!(
        gui.exists_and_visible(&room(&other_id, room_row::MENTIONS))
            .unwrap()
    );
    assert!(
        gui.exists_and_visible(&room(&dm_id, room_row::UNREAD))
            .unwrap()
    );
}

// ROOM-005
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn rooms_are_listed_with_the_most_recently_active_first() {
    let admin = Admin::login();
    let user = admin.create_user();
    // Created in the opposite order of their activity, which a name order would follow.
    let (older_id, _) = admin.create_channel(&[&user.username]);
    let (newer_id, _) = admin.create_channel(&[&user.username]);
    let (quiet_id, _) = admin.create_channel(&[&user.username]);
    admin.post_message(&newer_id, "first");
    admin.post_message(&older_id, "second");
    let app = App::launch().unwrap();
    let gui = app.gui();

    log_in_and_wait_for_room(gui, &user, &older_id);
    for id in [&newer_id, &quiet_id] {
        gui.wait_until_visible(&room(id, room_row::NAME), TIMEOUT)
            .unwrap();
    }

    // Named after what it was, `older_id` was posted to last.
    assert!(row_y(gui, &older_id) < row_y(gui, &newer_id));
    // A room without messages comes after the ones with.
    assert!(row_y(gui, &newer_id) < row_y(gui, &quiet_id));
}
