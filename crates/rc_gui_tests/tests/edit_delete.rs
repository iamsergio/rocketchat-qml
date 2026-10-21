// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Editing and deleting the user's own messages from the message menu and the composer, against
//! the local test server (`RC_TEST_SERVER_URL`). The user's messages are posted as themselves.
//! The menu is a popup, so its items are found from the window while it is open. Each test makes
//! its own user and channel and only looks at those.

mod common;

use rc_gui_tests::paths::{main_window, message, message_row};
use rc_gui_tests::{Key, Modifiers, MouseButton};
use rc_test_server::{Admin, Proxy, unique};

use common::{
    TIMEOUT, choose, launch, log_in_and_wait_for_room, log_in_at_and_wait_for_room, open_room,
    post_as, replace_text_and_press_enter,
};

// ACT-001
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_message_is_edited_through_its_menu() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let before = format!("before {}", unique());
    let own = post_as(&user, &channel_id, &before);
    let other = admin.post_message(&channel_id, &format!("other {}", unique()));
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    open_room(gui, &channel_id);
    gui.wait_until_visible(&message(&other.id, message_row::BODY), TIMEOUT)
        .unwrap();
    assert!(
        !gui.exists_and_visible(main_window::COMPOSER_EDIT_BAR)
            .unwrap()
    );
    assert_eq!(gui.text(main_window::COMPOSER_SEND).unwrap(), "Send");

    choose(gui, &own.id, main_window::MESSAGE_EDIT_ACTION);

    // The composer holds the text and says that it edits.
    gui.wait_until_visible(main_window::COMPOSER_EDIT_BAR, TIMEOUT)
        .unwrap();
    gui.wait_until_text(main_window::COMPOSER_FIELD, &before, TIMEOUT)
        .unwrap();
    assert_eq!(gui.text(main_window::COMPOSER_SEND).unwrap(), "Save");
    let after = format!("after the edit {}", unique());
    replace_text_and_press_enter(gui, &after);

    gui.wait_until_text(&message(&own.id, message_row::BODY), &after, TIMEOUT)
        .unwrap();
    gui.wait_until_visible(&message(&own.id, message_row::EDITED), TIMEOUT)
        .unwrap();
    gui.wait_until_hidden(main_window::COMPOSER_EDIT_BAR, TIMEOUT)
        .unwrap();
    gui.wait_until_text(main_window::COMPOSER_FIELD, "", TIMEOUT)
        .unwrap();
    assert_eq!(gui.text(main_window::COMPOSER_SEND).unwrap(), "Send");
    assert!(
        !gui.exists_and_visible(&message(&other.id, message_row::EDITED))
            .unwrap()
    );
    assert!(
        !gui.exists_and_visible(main_window::ROOM_ACTION_ERROR)
            .unwrap()
    );
}

// ACT-001
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn up_in_the_empty_composer_edits_the_last_message_of_the_user() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let first = format!("first {}", unique());
    let last = format!("last {}", unique());
    post_as(&user, &channel_id, &first);
    let newest = post_as(&user, &channel_id, &last);
    // Somebody else's message is newer, and not the one that is edited.
    let other = admin.post_message(&channel_id, &format!("other {}", unique()));
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    open_room(gui, &channel_id);
    gui.wait_until_visible(&message(&other.id, message_row::BODY), TIMEOUT)
        .unwrap();

    gui.mouse_click(main_window::COMPOSER_FIELD).unwrap();
    gui.enter_key(main_window::COMPOSER_FIELD, Key::UP, Modifiers::NONE)
        .unwrap();

    gui.wait_until_visible(main_window::COMPOSER_EDIT_BAR, TIMEOUT)
        .unwrap();
    gui.wait_until_text(main_window::COMPOSER_FIELD, &last, TIMEOUT)
        .unwrap();
    let edited = format!("{last} edited");
    replace_text_and_press_enter(gui, &edited);
    gui.wait_until_text(&message(&newest.id, message_row::BODY), &edited, TIMEOUT)
        .unwrap();
    gui.wait_until_hidden(main_window::COMPOSER_EDIT_BAR, TIMEOUT)
        .unwrap();
}

// ACT-001
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn escape_stops_editing_leaves_the_message_alone_and_brings_the_draft_back() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let text = format!("untouched {}", unique());
    let own = post_as(&user, &channel_id, &text);
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    open_room(gui, &channel_id);
    gui.wait_until_visible(&message(&own.id, message_row::BODY), TIMEOUT)
        .unwrap();
    gui.mouse_click(main_window::COMPOSER_FIELD).unwrap();
    gui.set_text(main_window::COMPOSER_FIELD, "my draft")
        .unwrap();

    choose(gui, &own.id, main_window::MESSAGE_EDIT_ACTION);
    gui.wait_until_text(main_window::COMPOSER_FIELD, &text, TIMEOUT)
        .unwrap();
    gui.set_text(main_window::COMPOSER_FIELD, "changed my mind")
        .unwrap();
    gui.enter_key(main_window::COMPOSER_FIELD, Key::ESCAPE, Modifiers::NONE)
        .unwrap();

    gui.wait_until_hidden(main_window::COMPOSER_EDIT_BAR, TIMEOUT)
        .unwrap();
    gui.wait_until_text(main_window::COMPOSER_FIELD, "my draft", TIMEOUT)
        .unwrap();
    assert_eq!(
        gui.text(&message(&own.id, message_row::BODY)).unwrap(),
        text
    );
    assert!(
        !gui.exists_and_visible(&message(&own.id, message_row::EDITED))
            .unwrap()
    );

    // The button in the bar stops editing too.
    choose(gui, &own.id, main_window::MESSAGE_EDIT_ACTION);
    gui.wait_until_visible(main_window::COMPOSER_EDIT_BAR, TIMEOUT)
        .unwrap();
    gui.mouse_click(main_window::COMPOSER_EDIT_CANCEL).unwrap();
    gui.wait_until_hidden(main_window::COMPOSER_EDIT_BAR, TIMEOUT)
        .unwrap();
    gui.wait_until_text(main_window::COMPOSER_FIELD, "my draft", TIMEOUT)
        .unwrap();
}

// ACT-001
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn opening_another_room_stops_editing() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let (other_channel_id, _) = admin.create_channel(&[&user.username]);
    let own = post_as(&user, &channel_id, &format!("mine {}", unique()));
    let elsewhere = admin.post_message(&other_channel_id, &format!("elsewhere {}", unique()));
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    open_room(gui, &channel_id);
    gui.wait_until_visible(&message(&own.id, message_row::BODY), TIMEOUT)
        .unwrap();
    choose(gui, &own.id, main_window::MESSAGE_EDIT_ACTION);
    gui.wait_until_visible(main_window::COMPOSER_EDIT_BAR, TIMEOUT)
        .unwrap();

    open_room(gui, &other_channel_id);

    gui.wait_until_visible(&message(&elsewhere.id, message_row::BODY), TIMEOUT)
        .unwrap();
    gui.wait_until_hidden(main_window::COMPOSER_EDIT_BAR, TIMEOUT)
        .unwrap();
    gui.wait_until_text(main_window::COMPOSER_FIELD, "", TIMEOUT)
        .unwrap();
}

// ACT-002
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_message_is_deleted_through_its_menu_after_confirming() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let kept = post_as(&user, &channel_id, &format!("kept {}", unique()));
    let doomed = post_as(&user, &channel_id, &format!("doomed {}", unique()));
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    open_room(gui, &channel_id);
    gui.wait_until_visible(&message(&doomed.id, message_row::BODY), TIMEOUT)
        .unwrap();

    // Cancelling the confirmation keeps the message.
    choose(gui, &doomed.id, main_window::MESSAGE_DELETE_ACTION);
    gui.wait_until_visible(main_window::DELETE_DIALOG, TIMEOUT)
        .unwrap();
    gui.mouse_click(main_window::DELETE_CANCEL).unwrap();
    gui.wait_until_hidden(main_window::DELETE_DIALOG, TIMEOUT)
        .unwrap();
    assert!(
        gui.exists_and_visible(&message(&doomed.id, message_row::BODY))
            .unwrap()
    );

    choose(gui, &doomed.id, main_window::MESSAGE_DELETE_ACTION);
    gui.wait_until_visible(main_window::DELETE_CONFIRM, TIMEOUT)
        .unwrap();
    gui.mouse_click(main_window::DELETE_CONFIRM).unwrap();

    gui.wait_until_hidden(&message(&doomed.id, message_row::BODY), TIMEOUT)
        .unwrap();
    assert!(
        gui.exists_and_visible(&message(&kept.id, message_row::BODY))
            .unwrap()
    );
    assert!(
        !gui.exists_and_visible(main_window::ROOM_ACTION_ERROR)
            .unwrap()
    );
}

// ACT-001, ACT-002, ACT-005
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_right_click_opens_the_menu_of_a_message() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let own = post_as(&user, &channel_id, &format!("mine {}", unique()));
    let other = admin.post_message(&channel_id, &format!("theirs {}", unique()));
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    open_room(gui, &channel_id);
    gui.wait_until_visible(&message(&other.id, message_row::BODY), TIMEOUT)
        .unwrap();

    gui.mouse_click_with(
        &format!("{}/msg_{}", main_window::WINDOW, own.id),
        MouseButton::Right,
        Modifiers::NONE,
    )
    .unwrap();

    gui.wait_until_visible(main_window::MESSAGE_EDIT_ACTION, TIMEOUT)
        .unwrap();
    assert!(
        gui.exists_and_visible(main_window::MESSAGE_DELETE_ACTION)
            .unwrap()
    );
    assert!(
        gui.exists_and_visible(main_window::MESSAGE_QUOTE_ACTION)
            .unwrap()
    );
}

// ACT-001, ACT-002, ACT-005
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn the_menu_of_the_message_of_somebody_else_can_only_quote() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let own = post_as(&user, &channel_id, &format!("mine {}", unique()));
    let other = admin.post_message(&channel_id, &format!("theirs {}", unique()));
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    open_room(gui, &channel_id);
    gui.wait_until_visible(&message(&other.id, message_row::BODY), TIMEOUT)
        .unwrap();
    gui.wait_until_visible(&message(&own.id, message_row::BODY), TIMEOUT)
        .unwrap();

    gui.mouse_click(&message(&other.id, message_row::MENU_BUTTON))
        .unwrap();

    gui.wait_until_visible(main_window::MESSAGE_QUOTE_ACTION, TIMEOUT)
        .unwrap();
    assert!(
        !gui.exists_and_visible(main_window::MESSAGE_EDIT_ACTION)
            .unwrap()
    );
    assert!(
        !gui.exists_and_visible(main_window::MESSAGE_DELETE_ACTION)
            .unwrap()
    );
}

// ACT-001
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn an_edit_that_cannot_reach_the_server_keeps_the_text_and_shows_the_error() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let before = format!("before {}", unique());
    let own = post_as(&user, &channel_id, &before);
    let proxy = Proxy::start();
    let app = launch();
    let gui = app.gui();
    log_in_at_and_wait_for_room(gui, &proxy.url(), &user, &channel_id);
    open_room(gui, &channel_id);
    gui.wait_until_visible(&message(&own.id, message_row::BODY), TIMEOUT)
        .unwrap();
    choose(gui, &own.id, main_window::MESSAGE_EDIT_ACTION);
    gui.wait_until_text(main_window::COMPOSER_FIELD, &before, TIMEOUT)
        .unwrap();

    // The server can't be reached while the edit is saved.
    proxy.set_accepting(false);
    proxy.drop_connections();
    let after = format!("after the edit {}", unique());
    replace_text_and_press_enter(gui, &after);

    gui.wait_until_visible(main_window::ROOM_ACTION_ERROR, TIMEOUT)
        .unwrap();
    assert_eq!(
        gui.text(main_window::ROOM_ACTION_ERROR_TEXT).unwrap(),
        "Can't reach the server"
    );
    // Still editing, with the text, and the message as it was.
    assert!(
        gui.exists_and_visible(main_window::COMPOSER_EDIT_BAR)
            .unwrap()
    );
    assert_eq!(gui.text(main_window::COMPOSER_FIELD).unwrap(), after);
    assert_eq!(
        gui.text(&message(&own.id, message_row::BODY)).unwrap(),
        before
    );

    // Back in reach: saving again works, and the error goes with the next try.
    proxy.set_accepting(true);
    gui.enter_key(main_window::COMPOSER_FIELD, Key::RETURN, Modifiers::NONE)
        .unwrap();
    gui.wait_until_text(&message(&own.id, message_row::BODY), &after, TIMEOUT)
        .unwrap();
    gui.wait_until_hidden(main_window::COMPOSER_EDIT_BAR, TIMEOUT)
        .unwrap();
    gui.wait_until_hidden(main_window::ROOM_ACTION_ERROR, TIMEOUT)
        .unwrap();
}

// ACT-002
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_deletion_that_cannot_reach_the_server_shows_an_error_that_can_be_dismissed() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let own = post_as(&user, &channel_id, &format!("mine {}", unique()));
    let proxy = Proxy::start();
    let app = launch();
    let gui = app.gui();
    log_in_at_and_wait_for_room(gui, &proxy.url(), &user, &channel_id);
    open_room(gui, &channel_id);
    gui.wait_until_visible(&message(&own.id, message_row::BODY), TIMEOUT)
        .unwrap();

    proxy.set_accepting(false);
    proxy.drop_connections();
    choose(gui, &own.id, main_window::MESSAGE_DELETE_ACTION);
    gui.wait_until_visible(main_window::DELETE_CONFIRM, TIMEOUT)
        .unwrap();
    gui.mouse_click(main_window::DELETE_CONFIRM).unwrap();

    gui.wait_until_visible(main_window::ROOM_ACTION_ERROR, TIMEOUT)
        .unwrap();
    assert!(
        gui.exists_and_visible(&message(&own.id, message_row::BODY))
            .unwrap()
    );
    gui.mouse_click(main_window::ROOM_ACTION_ERROR_DISMISS)
        .unwrap();
    gui.wait_until_hidden(main_window::ROOM_ACTION_ERROR, TIMEOUT)
        .unwrap();
}
