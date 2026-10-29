// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The popup that completes an @mention in the composer of a room and of a thread, against the
//! local test server (`RC_TEST_SERVER_URL`): the members that match the word, inserting one with
//! Enter, Tab or Down and Enter, Esc, a direct message, `@all`, and the user left out. Each test
//! makes its own users and channel, with names no other test or run shares.

// Helpers outside #[test] fns may unwrap: a failed step should just fail the test.
#![allow(clippy::unwrap_used)]

mod common;

use rc_gui_tests::paths::{
    main_window, mention, mention_part, mention_row, mention_row_part, message, message_row,
};
use rc_gui_tests::{App, Gui, Key, Modifiers};
use rc_test_server::{Admin, TestUser, unique};

use common::{TIMEOUT, choose, log_in_and_wait_for_room, open_room, wait_for_message_after};

/// Starts the app without the pause before the members are asked for, so that the tests don't
/// wait for it.
fn launch() -> App {
    App::launch_with(&[("TZ", "UTC"), ("RC_MEMBERS_DEBOUNCE_MS", "0")]).unwrap()
}

/// A word no other test or run uses, for the display names.
fn word() -> String {
    format!("Mq{}", unique().replace('-', ""))
}

/// Presses `key` in the field of the room's composer.
fn press(gui: &Gui, key: Key) {
    gui.enter_key(main_window::COMPOSER_FIELD, key, Modifiers::NONE)
        .unwrap();
}

/// Focuses the room's composer and types `text`, as the user would.
fn type_in_room(gui: &Gui, text: &str) {
    gui.mouse_click(main_window::COMPOSER_FIELD).unwrap();
    gui.input_text(main_window::COMPOSER_FIELD, text).unwrap();
}

/// A channel with the user and the `others`, which the app has open.
fn open_channel(gui: &Gui, admin: &Admin, user: &TestUser, others: &[&TestUser]) -> String {
    let mut members = vec![user.username.as_str()];
    members.extend(others.iter().map(|other| other.username.as_str()));
    let (channel_id, _) = admin.create_channel(&members);
    log_in_and_wait_for_room(gui, user, &channel_id);
    open_room(gui, &channel_id);
    channel_id
}

fn room_row(username: &str) -> String {
    mention_row(main_window::COMPOSER, username, "")
}

// ACT-013
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_username_lists_the_member_and_enter_inserts_it_and_the_message_shows_the_mention() {
    let admin = Admin::login();
    let user = admin.create_user();
    let friend = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username, &friend.username]);
    let before = admin
        .post_message(&channel_id, &format!("before {}", unique()))
        .id;
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    open_room(gui, &channel_id);

    type_in_room(gui, &format!("hi @{}", friend.username));

    gui.wait_until_visible(&room_row(&friend.username), TIMEOUT)
        .unwrap();
    gui.wait_until_visible(
        &mention(main_window::COMPOSER, mention_part::POPUP),
        TIMEOUT,
    )
    .unwrap();
    assert_eq!(
        gui.text(&mention(main_window::COMPOSER, mention_part::LABEL))
            .unwrap(),
        "Members"
    );
    assert!(
        gui.exists_and_visible(&mention(main_window::COMPOSER, mention_part::HINT))
            .unwrap()
    );
    press(gui, Key::RETURN);
    let inserted = format!("hi @{} ", friend.username);
    gui.wait_until_text(main_window::COMPOSER_FIELD, &inserted, TIMEOUT)
        .unwrap();
    gui.wait_until_hidden(
        &mention(main_window::COMPOSER, mention_part::POPUP),
        TIMEOUT,
    )
    .unwrap();
    // The cursor is after the space: what follows goes there.
    gui.input_text(main_window::COMPOSER_FIELD, "now").unwrap();
    gui.wait_until_text(
        main_window::COMPOSER_FIELD,
        &format!("{inserted}now"),
        TIMEOUT,
    )
    .unwrap();
    // Enter with the popup closed sends.
    press(gui, Key::RETURN);

    let sent = wait_for_message_after(&admin, &channel_id, &before);
    gui.wait_until_visible(&message(&sent.id, message_row::BODY), TIMEOUT)
        .unwrap();
    let html = common::html_containing(gui, &sent.id, "<span class=\"mention\">@");
    assert!(html.contains(&friend.username), "{html}");
}

// ACT-013
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn the_display_name_matches_and_tab_inserts() {
    let admin = Admin::login();
    let user = admin.create_user();
    let word = word();
    let friend = admin.create_user_named(&format!("{word} Friend"));
    let app = launch();
    let gui = app.gui();
    open_channel(gui, &admin, &user, &[&friend]);

    type_in_room(gui, &format!("@{}", word.to_lowercase()));

    gui.wait_until_visible(&room_row(&friend.username), TIMEOUT)
        .unwrap();
    let name = mention_row(
        main_window::COMPOSER,
        &friend.username,
        mention_row_part::NAME,
    );
    assert!(gui.exists_and_visible(&name).unwrap());
    press(gui, Key::TAB);
    gui.wait_until_text(
        main_window::COMPOSER_FIELD,
        &format!("@{} ", friend.username),
        TIMEOUT,
    )
    .unwrap();
}

// ACT-013
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn down_moves_to_the_second_person_and_enter_picks_it() {
    let admin = Admin::login();
    let user = admin.create_user();
    let word = word();
    let first = admin.create_user_named(&format!("{word} Aaron"));
    let second = admin.create_user_named(&format!("{word} Zelda"));
    let app = launch();
    let gui = app.gui();
    open_channel(gui, &admin, &user, &[&first, &second]);

    type_in_room(gui, &format!("@{word}"));

    gui.wait_until_visible(&room_row(&first.username), TIMEOUT)
        .unwrap();
    gui.wait_until_visible(&room_row(&second.username), TIMEOUT)
        .unwrap();
    // Listed by name.
    let y = |username: &str| gui.bounding_box(&room_row(username)).unwrap().y;
    assert!(y(&first.username) < y(&second.username));
    press(gui, Key::DOWN);
    press(gui, Key::RETURN);
    gui.wait_until_text(
        main_window::COMPOSER_FIELD,
        &format!("@{} ", second.username),
        TIMEOUT,
    )
    .unwrap();
}

// ACT-013
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn escape_closes_the_popup_and_keeps_the_text_until_the_word_changes() {
    let admin = Admin::login();
    let user = admin.create_user();
    let word = word();
    let friend = admin.create_user_named(&format!("{word} Friend"));
    let app = launch();
    let gui = app.gui();
    open_channel(gui, &admin, &user, &[&friend]);
    let popup = mention(main_window::COMPOSER, mention_part::POPUP);
    type_in_room(gui, &format!("@{word}"));
    gui.wait_until_visible(&room_row(&friend.username), TIMEOUT)
        .unwrap();

    press(gui, Key::ESCAPE);

    gui.wait_until_hidden(&popup, TIMEOUT).unwrap();
    assert_eq!(
        gui.text(main_window::COMPOSER_FIELD).unwrap(),
        format!("@{word}")
    );
    // Typing on in the word opens it again.
    gui.input_text(main_window::COMPOSER_FIELD, "x").unwrap();
    gui.wait_until_visible(&popup, TIMEOUT).unwrap();
    gui.wait_until_visible(&mention(main_window::COMPOSER, mention_part::NONE), TIMEOUT)
        .unwrap();
}

// ACT-013
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_direct_message_lists_the_other_person_and_not_all_or_here() {
    let admin = Admin::login();
    let user = admin.create_user();
    let word = word();
    let other = admin.create_user_named(&format!("{word} Other"));
    let dm_id = Admin::login_as(&user.username, &user.password).create_dm(&other.username);
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &dm_id);
    open_room(gui, &dm_id);

    type_in_room(gui, "@");

    gui.wait_until_visible(&room_row(&other.username), TIMEOUT)
        .unwrap();
    assert_eq!(
        gui.text(&mention(main_window::COMPOSER, mention_part::LABEL))
            .unwrap(),
        "In this conversation"
    );
    assert!(!gui.exists_and_visible(&room_row("all")).unwrap());
    assert!(!gui.exists_and_visible(&room_row("here")).unwrap());
}

// ACT-013
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn all_is_offered_in_a_channel_and_inserts() {
    let admin = Admin::login();
    let user = admin.create_user();
    let friend = admin.create_user();
    let app = launch();
    let gui = app.gui();
    open_channel(gui, &admin, &user, &[&friend]);

    type_in_room(gui, "@al");

    gui.wait_until_visible(&room_row("all"), TIMEOUT).unwrap();
    assert!(
        gui.exists_and_visible(&mention_row(
            main_window::COMPOSER,
            "all",
            mention_row_part::DESCRIPTION
        ))
        .unwrap()
    );
    // Only what starts with the typed text.
    assert!(!gui.exists_and_visible(&room_row("here")).unwrap());
    press(gui, Key::RETURN);
    gui.wait_until_text(main_window::COMPOSER_FIELD, "@all ", TIMEOUT)
        .unwrap();
}

// ACT-013
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn enter_right_after_typing_inserts_a_row_that_needs_no_answer() {
    let admin = Admin::login();
    let user = admin.create_user();
    let friend = admin.create_user();
    let app = launch();
    let gui = app.gui();
    open_channel(gui, &admin, &user, &[&friend]);

    // `@all` is a row at once, without the server: Enter at once must insert it, not send.
    type_in_room(gui, "@al");
    press(gui, Key::RETURN);

    gui.wait_until_text(main_window::COMPOSER_FIELD, "@all ", TIMEOUT)
        .unwrap();
}

// ACT-013
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn the_thread_composer_completes_mentions_too() {
    let admin = Admin::login();
    let user = admin.create_user();
    let friend = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username, &friend.username]);
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

    gui.mouse_click(main_window::THREAD_COMPOSER_FIELD).unwrap();
    gui.input_text(
        main_window::THREAD_COMPOSER_FIELD,
        &format!("@{}", friend.username),
    )
    .unwrap();

    gui.wait_until_visible(
        &mention_row(main_window::THREAD_COMPOSER, &friend.username, ""),
        TIMEOUT,
    )
    .unwrap();
    // Only the thread's composer shows it.
    assert!(
        !gui.exists_and_visible(&mention(main_window::COMPOSER, mention_part::POPUP))
            .unwrap()
    );
    gui.enter_key(
        main_window::THREAD_COMPOSER_FIELD,
        Key::RETURN,
        Modifiers::NONE,
    )
    .unwrap();
    gui.wait_until_text(
        main_window::THREAD_COMPOSER_FIELD,
        &format!("@{} ", friend.username),
        TIMEOUT,
    )
    .unwrap();
}

// ACT-013
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn the_user_is_not_listed_among_the_members() {
    let admin = Admin::login();
    let user = admin.create_user();
    let friend = admin.create_user();
    let app = launch();
    let gui = app.gui();
    open_channel(gui, &admin, &user, &[&friend]);

    type_in_room(gui, &format!("@{}", user.username));

    gui.wait_until_visible(&mention(main_window::COMPOSER, mention_part::NONE), TIMEOUT)
        .unwrap();
    assert!(!gui.exists_and_visible(&room_row(&user.username)).unwrap());
}
