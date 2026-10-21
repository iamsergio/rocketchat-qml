// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The thread pane beside the room, against the local test server (`RC_TEST_SERVER_URL`): opening
//! a thread from the summary under its parent, from the message menu and from the line of a reply
//! also sent to the room; replying, the replies of others arriving, editing and deleting a reply,
//! and closing the pane. The parent and the replies sent to the room are in both lists under the
//! same names, so the pane's are addressed through it. Each test makes its own user and channel
//! and only looks at those.

mod common;

use rc_gui_tests::paths::{main_window, message, message_row, thread_message};
use rc_gui_tests::{Key, Modifiers};
use rc_test_server::{Admin, unique};

use common::{
    TIMEOUT, choose, choose_in_thread, launch, log_in_and_wait_for_room, open_room,
    replace_text_and_press_enter, type_and_send_in_thread, wait_for_message_after,
    wait_for_reply_after,
};

// ACT-004
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn the_summary_under_a_parent_opens_its_thread_with_the_parent_and_the_replies() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let parent_text = format!("parent {}", unique());
    let parent = admin.post_message(&channel_id, &parent_text);
    let first = format!("first reply {}", unique());
    let second = format!("second reply {}", unique());
    let first_posted = admin.post_in_thread(&channel_id, &parent.id, &first, false);
    let second_posted = admin.post_in_thread(&channel_id, &parent.id, &second, false);
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    open_room(gui, &channel_id);

    // The room shows the parent with its summary, and the pane isn't there.
    gui.wait_until_visible(&message(&parent.id, message_row::THREAD_SUMMARY), TIMEOUT)
        .unwrap();
    gui.wait_until_text(
        &message(&parent.id, message_row::THREAD_COUNT),
        "2 replies",
        TIMEOUT,
    )
    .unwrap();
    assert!(
        gui.text(&message(&parent.id, message_row::THREAD_LAST_REPLY))
            .unwrap()
            .starts_with("Last reply ")
    );
    assert!(!gui.exists_and_visible(main_window::THREAD_PANE).unwrap());
    assert!(
        !gui.exists_and_visible(&message(&parent.id, message_row::THREAD_OPEN))
            .unwrap()
    );

    gui.mouse_click(&message(&parent.id, message_row::THREAD_SUMMARY))
        .unwrap();

    // The pane has the parent, then the divider with the count, then the replies.
    gui.wait_until_visible(main_window::THREAD_PANE, TIMEOUT)
        .unwrap();
    assert_eq!(gui.text(main_window::THREAD_TITLE).unwrap(), "Thread");
    let in_pane = |id: &str| thread_message(id, message_row::BODY);
    gui.wait_until_text(&in_pane(&parent.id), &parent_text, TIMEOUT)
        .unwrap();
    gui.wait_until_text(&in_pane(&first_posted.id), &first, TIMEOUT)
        .unwrap();
    gui.wait_until_text(&in_pane(&second_posted.id), &second, TIMEOUT)
        .unwrap();
    gui.wait_until_text(main_window::THREAD_REPLIES, "2 replies", TIMEOUT)
        .unwrap();
    // Three rows: the parent and the two replies.
    gui.wait_until("three rows in the pane", TIMEOUT, || {
        Ok(gui.count(main_window::THREAD_MESSAGE_LIST)? == 3)
    })
    .unwrap();
    // The parent in the room is marked as the one that is open; the one in the pane has no summary.
    gui.wait_until_visible(&message(&parent.id, message_row::THREAD_OPEN), TIMEOUT)
        .unwrap();
    assert!(
        !gui.exists_and_visible(&thread_message(&parent.id, message_row::THREAD_SUMMARY))
            .unwrap()
    );
    gui.wait_until_hidden(main_window::THREAD_LOADING, TIMEOUT)
        .unwrap();
    assert!(!gui.exists_and_visible(main_window::THREAD_ERROR).unwrap());
}

// ACT-004
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_message_without_replies_starts_a_thread_from_its_menu() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let text = format!("parent {}", unique());
    let parent = admin.post_message(&channel_id, &text);
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    open_room(gui, &channel_id);
    gui.wait_until_visible(&message(&parent.id, message_row::BODY), TIMEOUT)
        .unwrap();
    // No replies, no summary.
    assert!(
        !gui.exists_and_visible(&message(&parent.id, message_row::THREAD_SUMMARY))
            .unwrap()
    );

    choose(gui, &parent.id, main_window::MESSAGE_THREAD_ACTION);

    gui.wait_until_visible(main_window::THREAD_PANE, TIMEOUT)
        .unwrap();
    gui.wait_until_text(
        &thread_message(&parent.id, message_row::BODY),
        &text,
        TIMEOUT,
    )
    .unwrap();
    // Nothing to count yet.
    assert!(!gui.exists_and_visible(main_window::THREAD_REPLIES).unwrap());
    assert!(
        !gui.checked(main_window::THREAD_ALSO_TO_CHANNEL).unwrap(),
        "the check box starts off"
    );

    // The menu in the pane has no way to start a thread, and quoting works there.
    gui.mouse_click(&thread_message(&parent.id, message_row::MENU_BUTTON))
        .unwrap();
    gui.wait_until_visible(main_window::MESSAGE_QUOTE_ACTION, TIMEOUT)
        .unwrap();
    assert!(
        !gui.exists_and_visible(main_window::MESSAGE_THREAD_ACTION)
            .unwrap()
    );
    gui.mouse_click(main_window::MESSAGE_QUOTE_ACTION).unwrap();
    gui.wait_until_visible(main_window::THREAD_COMPOSER_QUOTE_BAR, TIMEOUT)
        .unwrap();
    // The room's composer is not the one that quotes.
    assert!(
        !gui.exists_and_visible(main_window::COMPOSER_QUOTE_BAR)
            .unwrap()
    );
}

// ACT-004
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_reply_typed_in_the_pane_shows_there_and_counts_in_the_room() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let parent = admin.post_message(&channel_id, &format!("parent {}", unique()));
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    open_room(gui, &channel_id);
    gui.wait_until_visible(&message(&parent.id, message_row::BODY), TIMEOUT)
        .unwrap();
    choose(gui, &parent.id, main_window::MESSAGE_THREAD_ACTION);
    gui.wait_until_visible(main_window::THREAD_PANE, TIMEOUT)
        .unwrap();
    let in_room = gui.count(main_window::MESSAGE_LIST).unwrap();

    let reply = format!("my reply {}", unique());
    type_and_send_in_thread(gui, &reply);

    let sent = wait_for_reply_after(&admin, &parent.id, None);
    gui.wait_until_text(
        &thread_message(&sent.id, message_row::BODY),
        &reply,
        TIMEOUT,
    )
    .unwrap();
    gui.wait_until_text(main_window::THREAD_COMPOSER_FIELD, "", TIMEOUT)
        .unwrap();
    gui.wait_until_text(main_window::THREAD_REPLIES, "1 reply", TIMEOUT)
        .unwrap();
    // The parent in the room now has the summary, and the reply, which isn't for the room, is not
    // one of its messages.
    gui.wait_until_text(
        &message(&parent.id, message_row::THREAD_COUNT),
        "1 reply",
        TIMEOUT,
    )
    .unwrap();
    assert_eq!(gui.count(main_window::MESSAGE_LIST).unwrap(), in_room);
    // The pane's check box is off, as it was.
    assert!(!gui.checked(main_window::THREAD_ALSO_TO_CHANNEL).unwrap());

    // Another one: the count goes up.
    let again = format!("another reply {}", unique());
    type_and_send_in_thread(gui, &again);
    gui.wait_until_text(main_window::THREAD_REPLIES, "2 replies", TIMEOUT)
        .unwrap();
    gui.wait_until_text(
        &message(&parent.id, message_row::THREAD_COUNT),
        "2 replies",
        TIMEOUT,
    )
    .unwrap();
}

// ACT-004
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_reply_of_somebody_else_arrives_while_the_thread_is_open() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let parent = admin.post_message(&channel_id, &format!("parent {}", unique()));
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    open_room(gui, &channel_id);
    gui.wait_until_visible(&message(&parent.id, message_row::BODY), TIMEOUT)
        .unwrap();
    choose(gui, &parent.id, main_window::MESSAGE_THREAD_ACTION);
    gui.wait_until_visible(main_window::THREAD_PANE, TIMEOUT)
        .unwrap();

    let text = format!("live reply {}", unique());
    let posted = admin.post_in_thread(&channel_id, &parent.id, &text, false);

    gui.wait_until_text(
        &thread_message(&posted.id, message_row::BODY),
        &text,
        TIMEOUT,
    )
    .unwrap();
    gui.wait_until_text(main_window::THREAD_REPLIES, "1 reply", TIMEOUT)
        .unwrap();
    gui.wait_until_text(
        &message(&parent.id, message_row::THREAD_COUNT),
        "1 reply",
        TIMEOUT,
    )
    .unwrap();
}

// ACT-004
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_reply_also_sent_to_the_room_shows_in_both_and_leads_to_its_thread() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let parent_text = format!("parent {}", unique());
    let parent = admin.post_message(&channel_id, &parent_text);
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    open_room(gui, &channel_id);
    gui.wait_until_visible(&message(&parent.id, message_row::BODY), TIMEOUT)
        .unwrap();
    choose(gui, &parent.id, main_window::MESSAGE_THREAD_ACTION);
    gui.wait_until_visible(main_window::THREAD_PANE, TIMEOUT)
        .unwrap();

    // The check box is off by default, and turns off again after the reply.
    gui.mouse_click(main_window::THREAD_ALSO_TO_CHANNEL)
        .unwrap();
    gui.wait_until("the check box to be on", TIMEOUT, || {
        gui.checked(main_window::THREAD_ALSO_TO_CHANNEL)
    })
    .unwrap();
    let reply = format!("for everybody {}", unique());
    type_and_send_in_thread(gui, &reply);

    let sent = wait_for_message_after(&admin, &channel_id, &parent.id);
    gui.wait_until_text(
        &thread_message(&sent.id, message_row::BODY),
        &reply,
        TIMEOUT,
    )
    .unwrap();
    // In the room too, with the line that says where it belongs. The line is not in the pane.
    let line = message(&sent.id, message_row::IN_THREAD);
    gui.wait_until_visible(&line, TIMEOUT).unwrap();
    assert_eq!(
        gui.text(&line).unwrap(),
        format!("Replied in a thread: {parent_text}")
    );
    assert!(
        !gui.exists_and_visible(&thread_message(&sent.id, message_row::IN_THREAD))
            .unwrap()
    );
    gui.wait_until("the check box to be off", TIMEOUT, || {
        Ok(!gui.checked(main_window::THREAD_ALSO_TO_CHANNEL)?)
    })
    .unwrap();

    // Closed, the line opens the thread again.
    gui.mouse_click(main_window::THREAD_CLOSE).unwrap();
    gui.wait_until_hidden(main_window::THREAD_PANE, TIMEOUT)
        .unwrap();
    gui.mouse_click(&line).unwrap();
    gui.wait_until_visible(main_window::THREAD_PANE, TIMEOUT)
        .unwrap();
    gui.wait_until_text(
        &thread_message(&parent.id, message_row::BODY),
        &parent_text,
        TIMEOUT,
    )
    .unwrap();
}

// ACT-004
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_reply_sent_to_the_room_by_somebody_else_leads_to_its_thread() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let parent_text = format!("parent {}", unique());
    let parent = admin.post_message(&channel_id, &parent_text);
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    open_room(gui, &channel_id);
    gui.wait_until_visible(&message(&parent.id, message_row::BODY), TIMEOUT)
        .unwrap();

    let text = format!("shown in the room {}", unique());
    let posted = admin.post_in_thread(&channel_id, &parent.id, &text, true);

    let line = message(&posted.id, message_row::IN_THREAD);
    gui.wait_until_visible(&line, TIMEOUT).unwrap();
    assert!(!gui.exists_and_visible(main_window::THREAD_PANE).unwrap());
    gui.mouse_click(&line).unwrap();
    gui.wait_until_visible(main_window::THREAD_PANE, TIMEOUT)
        .unwrap();
    gui.wait_until_text(
        &thread_message(&posted.id, message_row::BODY),
        &text,
        TIMEOUT,
    )
    .unwrap();
    gui.wait_until_text(main_window::THREAD_REPLIES, "1 reply", TIMEOUT)
        .unwrap();
}

// ACT-004
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_reply_is_edited_and_deleted_inside_the_pane() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let parent = admin.post_message(&channel_id, &format!("parent {}", unique()));
    let before = format!("before {}", unique());
    let own = Admin::login_as(&user.username, &user.password).post_in_thread(
        &channel_id,
        &parent.id,
        &before,
        false,
    );
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    open_room(gui, &channel_id);
    gui.wait_until_visible(&message(&parent.id, message_row::THREAD_SUMMARY), TIMEOUT)
        .unwrap();
    gui.mouse_click(&message(&parent.id, message_row::THREAD_SUMMARY))
        .unwrap();
    gui.wait_until_text(
        &thread_message(&own.id, message_row::BODY),
        &before,
        TIMEOUT,
    )
    .unwrap();

    // Edit: the thread's composer takes the text, and Enter saves it.
    choose_in_thread(gui, &own.id, main_window::MESSAGE_EDIT_ACTION);
    gui.wait_until_visible(main_window::THREAD_COMPOSER_EDIT_BAR, TIMEOUT)
        .unwrap();
    gui.wait_until_text(main_window::THREAD_COMPOSER_FIELD, &before, TIMEOUT)
        .unwrap();
    let after = format!("after the edit {}", unique());
    gui.set_text(main_window::THREAD_COMPOSER_FIELD, &after)
        .unwrap();
    gui.enter_key(
        main_window::THREAD_COMPOSER_FIELD,
        Key::RETURN,
        Modifiers::NONE,
    )
    .unwrap();
    gui.wait_until_text(&thread_message(&own.id, message_row::BODY), &after, TIMEOUT)
        .unwrap();
    gui.wait_until_visible(&thread_message(&own.id, message_row::EDITED), TIMEOUT)
        .unwrap();
    gui.wait_until_hidden(main_window::THREAD_COMPOSER_EDIT_BAR, TIMEOUT)
        .unwrap();

    // Delete: the reply leaves the pane and the room's count.
    choose_in_thread(gui, &own.id, main_window::MESSAGE_DELETE_ACTION);
    gui.wait_until_visible(main_window::DELETE_CONFIRM, TIMEOUT)
        .unwrap();
    gui.mouse_click(main_window::DELETE_CONFIRM).unwrap();
    gui.wait_until_hidden(&thread_message(&own.id, message_row::BODY), TIMEOUT)
        .unwrap();
    gui.wait_until_hidden(&message(&parent.id, message_row::THREAD_SUMMARY), TIMEOUT)
        .unwrap();
    gui.wait_until_hidden(main_window::THREAD_REPLIES, TIMEOUT)
        .unwrap();
}

// ACT-004
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn the_close_button_escape_and_another_room_close_the_pane() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let (other_channel_id, _) = admin.create_channel(&[&user.username]);
    let parent = admin.post_message(&channel_id, &format!("parent {}", unique()));
    let elsewhere = admin.post_message(&other_channel_id, &format!("elsewhere {}", unique()));
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    open_room(gui, &channel_id);
    gui.wait_until_visible(&message(&parent.id, message_row::BODY), TIMEOUT)
        .unwrap();

    // The button.
    choose(gui, &parent.id, main_window::MESSAGE_THREAD_ACTION);
    gui.wait_until_visible(main_window::THREAD_PANE, TIMEOUT)
        .unwrap();
    gui.mouse_click(main_window::THREAD_CLOSE).unwrap();
    gui.wait_until_hidden(main_window::THREAD_PANE, TIMEOUT)
        .unwrap();
    gui.wait_until_hidden(&message(&parent.id, message_row::THREAD_OPEN), TIMEOUT)
        .unwrap();

    // Escape in the composer, with nothing to cancel.
    choose(gui, &parent.id, main_window::MESSAGE_THREAD_ACTION);
    gui.wait_until_visible(main_window::THREAD_PANE, TIMEOUT)
        .unwrap();
    gui.mouse_click(main_window::THREAD_COMPOSER_FIELD).unwrap();
    gui.enter_key(
        main_window::THREAD_COMPOSER_FIELD,
        Key::ESCAPE,
        Modifiers::NONE,
    )
    .unwrap();
    gui.wait_until_hidden(main_window::THREAD_PANE, TIMEOUT)
        .unwrap();

    // Another room.
    choose(gui, &parent.id, main_window::MESSAGE_THREAD_ACTION);
    gui.wait_until_visible(main_window::THREAD_PANE, TIMEOUT)
        .unwrap();
    open_room(gui, &other_channel_id);
    gui.wait_until_visible(&message(&elsewhere.id, message_row::BODY), TIMEOUT)
        .unwrap();
    gui.wait_until_hidden(main_window::THREAD_PANE, TIMEOUT)
        .unwrap();
    // The room's composer is the one that writes there, in the room.
    let text = format!("plain {}", unique());
    replace_text_and_press_enter(gui, &text);
    let sent = wait_for_message_after(&admin, &other_channel_id, &elsewhere.id);
    gui.wait_until_text(&message(&sent.id, message_row::BODY), &text, TIMEOUT)
        .unwrap();
}
