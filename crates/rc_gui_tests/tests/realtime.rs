// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! What the server pushes while the app runs, against the local test server
//! (`RC_TEST_SERVER_URL`). Each test makes its own user and rooms and only looks at those. Admin
//! acts after the user is logged in, and the tests wait for the UI to follow, never for a time.

mod common;

use rc_gui_tests::paths::{message, message_row, room, room_row};
use rc_test_server::{Admin, Proxy, unique};

use common::{TIMEOUT, launch, log_in_and_wait_for_room, log_in_at_and_wait_for_room, open_room};

// RT-001
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_message_posted_to_the_open_room_appears_without_reloading() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let before = admin.post_message(&channel_id, &format!("before {}", unique()));
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    open_room(gui, &channel_id);
    gui.wait_until_visible(&message(&before.id, message_row::BODY), TIMEOUT)
        .unwrap();

    let text = format!("live {}", unique());
    let posted = admin.post_message(&channel_id, &text);

    let body = message(&posted.id, message_row::BODY);
    gui.wait_until_text(&body, &text, TIMEOUT).unwrap();
    // What was there stays.
    assert!(
        gui.exists_and_visible(&message(&before.id, message_row::BODY))
            .unwrap()
    );
}

// RT-002, ROOM-003
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn the_unread_badges_of_other_rooms_update_while_a_room_is_open() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (open_id, _) = admin.create_channel(&[&user.username]);
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let dm_id = admin.create_dm(&user.username);
    // The server hides a direct message from the other person until it has a message.
    admin.post_message(&dm_id, "first");
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &open_id);
    gui.wait_until_text(&room(&dm_id, room_row::UNREAD), "1", TIMEOUT)
        .unwrap();
    open_room(gui, &open_id);

    // A direct message counts every message, a channel only the ones that mention the user.
    admin.post_message(&dm_id, "second");
    admin.post_message(&channel_id, "nothing for you");
    admin.post_message(&channel_id, &format!("hello @{}", user.username));

    gui.wait_until_text(&room(&dm_id, room_row::UNREAD), "2", TIMEOUT)
        .unwrap();
    gui.wait_until_text(&room(&channel_id, room_row::MENTIONS), "1", TIMEOUT)
        .unwrap();
    gui.wait_until_text(&room(&channel_id, room_row::UNREAD), "1", TIMEOUT)
        .unwrap();
}

// RT-003, RT-002
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn what_happens_while_the_connection_is_down_shows_up_after_it_is_back() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let before = admin.post_message(&channel_id, &format!("before {}", unique()));
    let proxy = Proxy::start();
    let app = launch();
    let gui = app.gui();
    log_in_at_and_wait_for_room(gui, &proxy.url(), &user, &channel_id);
    open_room(gui, &channel_id);
    gui.wait_until_visible(&message(&before.id, message_row::BODY), TIMEOUT)
        .unwrap();

    // The app is cut off, and can't come back for as long as admin takes to act.
    proxy.set_accepting(false);
    proxy.drop_connections();
    let text = format!("while away {}", unique());
    let posted = admin.post_message(&channel_id, &text);
    let (new_id, _) = admin.create_channel(&[&user.username]);
    proxy.set_accepting(true);

    gui.wait_until_text(&message(&posted.id, message_row::BODY), &text, TIMEOUT)
        .unwrap();
    gui.wait_until_visible(&room(&new_id, room_row::NAME), TIMEOUT)
        .unwrap();
    // What was there stays, and the connection is live again: the next message arrives too.
    assert!(
        gui.exists_and_visible(&message(&before.id, message_row::BODY))
            .unwrap()
    );
    let live = format!("live again {}", unique());
    let after = admin.post_message(&channel_id, &live);
    gui.wait_until_text(&message(&after.id, message_row::BODY), &live, TIMEOUT)
        .unwrap();
}
