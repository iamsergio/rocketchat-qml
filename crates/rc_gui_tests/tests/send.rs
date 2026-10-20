// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Sending messages from the composer, against the local test server (`RC_TEST_SERVER_URL`). Each
//! test makes its own user and channel and only looks at those. The client picks the id of what it
//! sends, so the tests learn it from the server, which is where the message must end up.

mod common;

use rc_gui_tests::paths::{main_window, message, message_row};
use rc_gui_tests::{Key, Modifiers};
use rc_test_server::{Admin, Proxy, unique};

use common::{
    TIMEOUT, launch, log_in_and_wait_for_room, log_in_at_and_wait_for_room, open_room,
    type_and_send, wait_for_message_after,
};

// SEND-001
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_message_typed_in_the_composer_is_sent_and_shown_as_sent() {
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

    let text = format!("hello {}", unique());
    type_and_send(gui, &text);

    // The composer is ready for the next message, and the server holds the message: the row with
    // its id, which the client picked, is the one that was shown.
    gui.wait_until_text(main_window::COMPOSER_FIELD, "", TIMEOUT)
        .unwrap();
    let sent = wait_for_message_after(&admin, &channel_id, &before.id);
    let body = message(&sent.id, message_row::BODY);
    gui.wait_until_text(&body, &text, TIMEOUT).unwrap();
    assert_eq!(
        gui.text(&message(&sent.id, message_row::AUTHOR)).unwrap(),
        user.name
    );
    gui.wait_until_hidden(&message(&sent.id, message_row::PENDING), TIMEOUT)
        .unwrap();
    assert!(
        !gui.exists_and_visible(&message(&sent.id, message_row::FAILED))
            .unwrap()
    );
    // What was there stays.
    assert!(
        gui.exists_and_visible(&message(&before.id, message_row::BODY))
            .unwrap()
    );
}

// SEND-001
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn shift_enter_starts_a_new_line_and_the_button_sends() {
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
    // Nothing to send yet.
    assert!(!gui.enabled(main_window::COMPOSER_SEND).unwrap());

    let text = format!("one line {}", unique());
    gui.mouse_click(main_window::COMPOSER_FIELD).unwrap();
    gui.set_text(main_window::COMPOSER_FIELD, &text).unwrap();
    gui.enter_key(main_window::COMPOSER_FIELD, Key::RETURN, Modifiers::SHIFT)
        .unwrap();

    // A new line, and nothing sent: Enter would have emptied the composer.
    gui.wait_until("a newline in the composer", TIMEOUT, || {
        let typed = gui.text(main_window::COMPOSER_FIELD)?;
        Ok(typed.contains('\n') && typed.contains(&text))
    })
    .unwrap();
    assert!(gui.enabled(main_window::COMPOSER_SEND).unwrap());

    gui.mouse_click(main_window::COMPOSER_SEND).unwrap();

    gui.wait_until_text(main_window::COMPOSER_FIELD, "", TIMEOUT)
        .unwrap();
    let sent = wait_for_message_after(&admin, &channel_id, &before.id);
    // Surrounding blanks aren't sent.
    gui.wait_until_text(&message(&sent.id, message_row::BODY), &text, TIMEOUT)
        .unwrap();
}

// SEND-002
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_message_that_cannot_be_sent_shows_as_failed_and_retrying_sends_it() {
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

    // The server can't be reached while the message is sent.
    proxy.set_accepting(false);
    proxy.drop_connections();
    let text = format!("sent in the dark {}", unique());
    type_and_send(gui, &text);

    gui.wait_until_visible(main_window::UNSENT_FAILED, TIMEOUT)
        .unwrap();
    gui.wait_until_visible(main_window::UNSENT_RETRY, TIMEOUT)
        .unwrap();
    assert!(!gui.exists_and_visible(main_window::UNSENT_PENDING).unwrap());
    assert_eq!(admin.last_message(&channel_id).id, before.id);

    // Back in reach: retrying sends the same message, which then shows as sent.
    proxy.set_accepting(true);
    gui.mouse_click(main_window::UNSENT_RETRY).unwrap();

    let sent = wait_for_message_after(&admin, &channel_id, &before.id);
    gui.wait_until_text(&message(&sent.id, message_row::BODY), &text, TIMEOUT)
        .unwrap();
    gui.wait_until_hidden(&message(&sent.id, message_row::PENDING), TIMEOUT)
        .unwrap();
    gui.wait_until_hidden(main_window::UNSENT_FAILED, TIMEOUT)
        .unwrap();
    gui.wait_until_hidden(main_window::UNSENT_RETRY, TIMEOUT)
        .unwrap();
}
