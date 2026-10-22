// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The menu of a link, against the local test server (`RC_TEST_SERVER_URL`). A right-click on a
//! link opens it, in a message, an attachment or the topic. The clipboard is read by pasting into
//! the composer. Spix clicks the middle of an element, so the links are long enough, and the
//! topics short enough, to be under it. Each test makes its own user and room.

// Helpers outside #[test] fns may unwrap: a failed step should just fail the test.
#![allow(clippy::unwrap_used)]

mod common;

use rc_gui_tests::paths::{
    attachment, attachment_row, main_window, message, message_row, thread_message,
};
use rc_gui_tests::{Gui, Key, Modifiers, MouseButton};
use rc_test_server::{Admin, server_url, unique};

use common::{TIMEOUT, launch, log_in_and_wait_for_room, open_room};

/// An address that is wider than half the message but fits in a line, so that the middle of the
/// message is on it.
fn long_address() -> String {
    format!("https://example.com/{}", unique())
}

fn right_click(gui: &Gui, path: &str) {
    gui.mouse_click_with(path, MouseButton::Right, Modifiers::NONE)
        .unwrap();
}

/// Chooses Copy in the open menu of a link, and pastes what the clipboard holds into the empty
/// composer at `field`. Returns what the composer holds then.
fn copy_and_paste(gui: &Gui, field: &str) -> String {
    gui.wait_until_visible(main_window::LINK_COPY_ACTION, TIMEOUT)
        .unwrap();
    assert!(
        gui.exists_and_visible(main_window::LINK_OPEN_ACTION)
            .unwrap()
    );
    gui.mouse_click(main_window::LINK_COPY_ACTION).unwrap();
    gui.wait_until_hidden(main_window::LINK_COPY_ACTION, TIMEOUT)
        .unwrap();
    gui.mouse_click(field).unwrap();
    gui.enter_key(field, Key::V, Modifiers::CONTROL).unwrap();
    gui.wait_until("the pasted address in the composer", TIMEOUT, || {
        Ok(!gui.text(field)?.is_empty())
    })
    .unwrap();
    gui.text(field).unwrap()
}

// RICH-008
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn copying_a_link_of_a_message_puts_its_address_on_the_clipboard() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let address = long_address();
    let posted = admin.post_message(&channel_id, &address);
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    open_room(gui, &channel_id);
    gui.wait_until_visible(&message(&posted.id, message_row::BODY_HTML), TIMEOUT)
        .unwrap();

    right_click(gui, &message(&posted.id, message_row::BODY_HTML));

    assert_eq!(copy_and_paste(gui, main_window::COMPOSER_FIELD), address);
}

// RICH-008
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_right_click_elsewhere_in_a_message_opens_the_menu_of_the_message() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    // A link at the start, and words past the middle of the message, in one line.
    let text = format!(
        "[here](https://example.com/{}) and then some words that go on past the middle of it",
        unique()
    );
    let posted = admin.post_message(&channel_id, &text);
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    open_room(gui, &channel_id);
    gui.wait_until_visible(&message(&posted.id, message_row::BODY_HTML), TIMEOUT)
        .unwrap();

    right_click(gui, &message(&posted.id, message_row::BODY_HTML));

    gui.wait_until_visible(main_window::MESSAGE_REACT_ACTION, TIMEOUT)
        .unwrap();
    assert!(
        !gui.exists_and_visible(main_window::LINK_COPY_ACTION)
            .unwrap()
    );
}

// RICH-008
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn copying_the_address_of_an_attachment_link() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let posted = admin.upload_file(&channel_id, "notes.txt", "text/plain", b"hello\n", "", "");
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    open_room(gui, &channel_id);
    let link = attachment(&posted.id, 0, attachment_row::LINK);
    gui.wait_until_visible(&link, TIMEOUT).unwrap();
    let address = gui.text(&link).unwrap();
    assert!(address.starts_with(server_url().trim_end_matches('/')));

    right_click(gui, &link);

    assert_eq!(copy_and_paste(gui, main_window::COMPOSER_FIELD), address);
}

// RICH-008
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn copying_a_link_of_a_reply_in_the_thread_pane() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let parent = admin.post_message(&channel_id, "a thread");
    let address = long_address();
    let reply = admin.post_in_thread(&channel_id, &parent.id, &address, false);
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    open_room(gui, &channel_id);
    gui.wait_until_visible(&message(&parent.id, message_row::THREAD_SUMMARY), TIMEOUT)
        .unwrap();
    gui.mouse_click(&message(&parent.id, message_row::THREAD_SUMMARY))
        .unwrap();
    let body = thread_message(&reply.id, message_row::BODY_HTML);
    gui.wait_until_visible(&body, TIMEOUT).unwrap();

    right_click(gui, &body);

    assert_eq!(
        copy_and_paste(gui, main_window::THREAD_COMPOSER_FIELD),
        address
    );
}

// RICH-008
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn copying_a_link_of_the_topic() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let address = format!("https://example.com/{}", unique());
    admin.set_topic(&channel_id, &address);
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    open_room(gui, &channel_id);
    gui.wait_until_visible(main_window::ROOM_TOPIC, TIMEOUT)
        .unwrap();

    right_click(gui, main_window::ROOM_TOPIC);

    assert_eq!(copy_and_paste(gui, main_window::COMPOSER_FIELD), address);
}
