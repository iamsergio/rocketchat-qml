// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Messages edited and deleted by someone else, against the local test server
//! (`RC_TEST_SERVER_URL`). Admin acts after the user is logged in and the tests wait for the UI to
//! follow, never for a time. Each test makes its own user and rooms and only looks at those.

mod common;

use rc_gui_tests::paths::{message, message_row};
use rc_test_server::{Admin, unique};

use common::{TIMEOUT, launch, log_in_and_wait_for_room, open_room};

// RICH-006
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn an_edit_shows_the_new_text_and_the_edited_marker() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let edited = admin.post_message(&channel_id, &format!("before {}", unique()));
    let other = admin.post_message(&channel_id, &format!("other {}", unique()));
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    open_room(gui, &channel_id);
    gui.wait_until_visible(&message(&other.id, message_row::BODY), TIMEOUT)
        .unwrap();
    gui.wait_until_visible(&message(&edited.id, message_row::BODY), TIMEOUT)
        .unwrap();
    assert!(
        !gui.exists_and_visible(&message(&edited.id, message_row::EDITED))
            .unwrap()
    );

    let text = format!("after the edit {}", unique());
    admin.edit_message(&channel_id, &edited.id, &text);

    gui.wait_until_text(&message(&edited.id, message_row::BODY), &text, TIMEOUT)
        .unwrap();
    gui.wait_until_visible(&message(&edited.id, message_row::EDITED), TIMEOUT)
        .unwrap();
    // The other message is as it was.
    assert!(
        !gui.exists_and_visible(&message(&other.id, message_row::EDITED))
            .unwrap()
    );
}

// RICH-006
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_deleted_message_disappears_and_the_others_stay() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let kept = admin.post_message(&channel_id, &format!("kept {}", unique()));
    let deleted = admin.post_message(&channel_id, &format!("deleted {}", unique()));
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    open_room(gui, &channel_id);
    gui.wait_until_visible(&message(&kept.id, message_row::BODY), TIMEOUT)
        .unwrap();
    gui.wait_until_visible(&message(&deleted.id, message_row::BODY), TIMEOUT)
        .unwrap();

    admin.delete_message(&channel_id, &deleted.id);

    gui.wait_until_hidden(&message(&deleted.id, message_row::BODY), TIMEOUT)
        .unwrap();
    assert!(
        gui.exists_and_visible(&message(&kept.id, message_row::BODY))
            .unwrap()
    );
}

// RICH-006
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn what_changed_while_the_room_was_closed_shows_when_it_opens_again() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (first_id, _) = admin.create_channel(&[&user.username]);
    let (second_id, _) = admin.create_channel(&[&user.username]);
    let kept = admin.post_message(&first_id, &format!("kept {}", unique()));
    let edited = admin.post_message(&first_id, &format!("to edit {}", unique()));
    let deleted = admin.post_message(&first_id, &format!("to delete {}", unique()));
    let elsewhere = admin.post_message(&second_id, &format!("elsewhere {}", unique()));
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &first_id);
    open_room(gui, &first_id);
    gui.wait_until_visible(&message(&deleted.id, message_row::BODY), TIMEOUT)
        .unwrap();
    open_room(gui, &second_id);
    gui.wait_until_visible(&message(&elsewhere.id, message_row::BODY), TIMEOUT)
        .unwrap();

    let text = format!("edited meanwhile {}", unique());
    admin.edit_message(&first_id, &edited.id, &text);
    admin.delete_message(&first_id, &deleted.id);
    open_room(gui, &first_id);

    gui.wait_until_visible(&message(&kept.id, message_row::BODY), TIMEOUT)
        .unwrap();
    gui.wait_until_hidden(&message(&deleted.id, message_row::BODY), TIMEOUT)
        .unwrap();
    gui.wait_until_text(&message(&edited.id, message_row::BODY), &text, TIMEOUT)
        .unwrap();
    gui.wait_until_visible(&message(&edited.id, message_row::EDITED), TIMEOUT)
        .unwrap();
}
