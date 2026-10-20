// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! What the server pushes while the app runs, against the local test server
//! (`RC_TEST_SERVER_URL`). Each test makes its own user and rooms and only looks at those. Admin
//! acts after the user is logged in, and the tests wait for the UI to follow, never for a time.

mod common;

use rc_gui_tests::paths::{main_window, message, message_row, room, room_row};
use rc_test_server::{Admin, Proxy, unique};

use common::{
    TIMEOUT, launch, log_in_and_wait_for_room, log_in_at_and_wait_for_room, open_room, row_y,
};

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

// ROOM-004
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_message_in_the_open_room_does_not_raise_its_badge() {
    let admin = Admin::login();
    let user = admin.create_user();
    let dm_id = admin.create_dm(&user.username);
    let (other_id, _) = admin.create_channel(&[&user.username]);
    // The server hides a direct message from the other person until it has a message.
    let first = admin.post_message(&dm_id, "first");
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &dm_id);
    open_room(gui, &dm_id);
    gui.wait_until_visible(&message(&first.id, message_row::BODY), TIMEOUT)
        .unwrap();

    // A direct message counts every message, so the server raises the open room's count. The
    // mention in the other room follows it on the same connection.
    let text = format!("live {}", unique());
    let posted = admin.post_message(&dm_id, &text);
    admin.post_message(&other_id, &format!("hello @{}", user.username));

    gui.wait_until_text(&message(&posted.id, message_row::BODY), &text, TIMEOUT)
        .unwrap();
    gui.wait_until_text(&room(&other_id, room_row::MENTIONS), "1", TIMEOUT)
        .unwrap();
    assert!(
        !gui.exists_and_visible(&room(&dm_id, room_row::UNREAD))
            .unwrap()
    );
}

// ROOM-005
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_new_message_moves_its_room_to_the_top() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (older_id, _) = admin.create_channel(&[&user.username]);
    let (newer_id, _) = admin.create_channel(&[&user.username]);
    admin.post_message(&older_id, "first");
    admin.post_message(&newer_id, "second");
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &older_id);
    gui.wait_until_visible(&room(&newer_id, room_row::NAME), TIMEOUT)
        .unwrap();
    assert!(row_y(gui, &newer_id) < row_y(gui, &older_id));

    admin.post_message(&older_id, "third");

    gui.wait_until("the room with the new message on top", TIMEOUT, || {
        Ok(row_y(gui, &older_id) < row_y(gui, &newer_id))
    })
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

// RT-004
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_banner_shows_while_the_connection_is_down_and_goes_when_it_is_back() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let proxy = Proxy::start();
    let app = launch();
    let gui = app.gui();
    log_in_at_and_wait_for_room(gui, &proxy.url(), &user, &channel_id);
    assert!(
        !gui.exists_and_visible(main_window::CONNECTION_BANNER)
            .unwrap()
    );

    proxy.set_accepting(false);
    proxy.drop_connections();
    gui.wait_until_visible(main_window::CONNECTION_BANNER, TIMEOUT)
        .unwrap();
    // The rooms stay on show meanwhile.
    assert!(
        gui.exists_and_visible(&room(&channel_id, room_row::NAME))
            .unwrap()
    );

    proxy.set_accepting(true);
    gui.wait_until_hidden(main_window::CONNECTION_BANNER, TIMEOUT)
        .unwrap();
}
