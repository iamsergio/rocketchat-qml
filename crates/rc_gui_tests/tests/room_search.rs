// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The search field of the room list, against the local test server (`RC_TEST_SERVER_URL`). Each
//! test makes its own user and rooms, and searches for the full name of one of them, which is
//! unique, or for the prefix all of them share.

// Helpers outside #[test] fns may unwrap: a failed step should just fail the test.
#![allow(clippy::unwrap_used)]

mod common;

use rc_gui_tests::paths::{main_window, room, room_row, search_result};
use rc_gui_tests::{App, Gui, Key, Modifiers};
use rc_test_server::Admin;

use common::{TIMEOUT, log_in_and_wait_for_room};

/// Types the query, as the user would: with the focus in the field.
fn search_for(gui: &Gui, query: &str) {
    gui.mouse_click(main_window::ROOM_SEARCH_FIELD).unwrap();
    gui.set_text(main_window::ROOM_SEARCH_FIELD, query).unwrap();
}

fn press(gui: &Gui, key: Key) {
    gui.enter_key(main_window::ROOM_SEARCH_FIELD, key, Modifiers::NONE)
        .unwrap();
}

// ROOM-010
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn the_rooms_that_match_replace_the_list_while_typing() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, channel_name) = admin.create_channel(&[&user.username]);
    let (group_id, _) = admin.create_group(&[&user.username]);
    let app = App::launch().unwrap();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &group_id);

    // In capitals: the case doesn't matter.
    search_for(gui, &channel_name.to_uppercase());

    gui.wait_until_visible(&search_result(&channel_id, room_row::NAME), TIMEOUT)
        .unwrap();
    assert_eq!(
        gui.text(&search_result(&channel_id, room_row::NAME))
            .unwrap(),
        channel_name
    );
    // The kind is shown as in the list.
    gui.wait_until_visible(&search_result(&channel_id, room_row::KIND_CHANNEL), TIMEOUT)
        .unwrap();
    assert_eq!(gui.count(main_window::ROOM_SEARCH_RESULTS).unwrap(), 1);
    // The list is gone, not just covered, and nothing says that there is no match.
    gui.wait_until_hidden(main_window::ROOM_LIST, TIMEOUT)
        .unwrap();
    assert!(
        !gui.exists_and_visible(main_window::ROOM_SEARCH_EMPTY)
            .unwrap()
    );
}

// ROOM-010
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn clearing_the_field_brings_the_list_back() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, channel_name) = admin.create_channel(&[&user.username]);
    let (group_id, _) = admin.create_group(&[&user.username]);
    let app = App::launch().unwrap();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &group_id);
    search_for(gui, &channel_name);
    gui.wait_until_hidden(main_window::ROOM_LIST, TIMEOUT)
        .unwrap();

    gui.set_text(main_window::ROOM_SEARCH_FIELD, "").unwrap();

    gui.wait_until_visible(main_window::ROOM_LIST, TIMEOUT)
        .unwrap();
    gui.wait_until_hidden(main_window::ROOM_SEARCH_RESULTS, TIMEOUT)
        .unwrap();
    for id in [&channel_id, &group_id] {
        gui.wait_until_visible(&room(id, room_row::NAME), TIMEOUT)
            .unwrap();
    }
}

// ROOM-010
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn enter_opens_the_first_result_and_clears_the_search() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, channel_name) = admin.create_channel(&[&user.username]);
    let (group_id, _) = admin.create_group(&[&user.username]);
    let app = App::launch().unwrap();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &group_id);
    search_for(gui, &channel_name);
    gui.wait_until_visible(&search_result(&channel_id, room_row::NAME), TIMEOUT)
        .unwrap();

    press(gui, Key::RETURN);

    gui.wait_until_text(main_window::ROOM_TITLE, &channel_name, TIMEOUT)
        .unwrap();
    gui.wait_until_text(main_window::ROOM_SEARCH_FIELD, "", TIMEOUT)
        .unwrap();
    // The list is back, with the room open in it.
    gui.wait_until_visible(&room(&channel_id, room_row::NAME), TIMEOUT)
        .unwrap();
    gui.wait_until_hidden(main_window::ROOM_SEARCH_RESULTS, TIMEOUT)
        .unwrap();
}

// ROOM-010
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn down_moves_to_the_next_result() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let (group_id, group_name) = admin.create_group(&[&user.username]);
    // The channel is the most recently active, so it is the first result.
    admin.post_message(&group_id, "first");
    admin.post_message(&channel_id, "second");
    let app = App::launch().unwrap();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &group_id);
    // Both names start with "ct-".
    search_for(gui, "ct-");
    for id in [&channel_id, &group_id] {
        gui.wait_until_visible(&search_result(id, room_row::NAME), TIMEOUT)
            .unwrap();
    }
    gui.wait_until(&format!("{channel_id} first"), TIMEOUT, || {
        let y = |id: &str| {
            gui.bounding_box(&search_result(id, room_row::NAME))
                .map(|rect| rect.y)
        };
        Ok(y(&channel_id)? < y(&group_id)?)
    })
    .unwrap();

    press(gui, Key::DOWN);
    press(gui, Key::RETURN);

    gui.wait_until_text(main_window::ROOM_TITLE, &group_name, TIMEOUT)
        .unwrap();
}

// ROOM-010
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn clicking_a_result_opens_the_room_and_clears_the_search() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, channel_name) = admin.create_channel(&[&user.username]);
    let (group_id, _) = admin.create_group(&[&user.username]);
    let app = App::launch().unwrap();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &group_id);
    search_for(gui, &channel_name);
    gui.wait_until_visible(&search_result(&channel_id, room_row::NAME), TIMEOUT)
        .unwrap();

    gui.mouse_click(&search_result(&channel_id, room_row::NAME))
        .unwrap();

    gui.wait_until_text(main_window::ROOM_TITLE, &channel_name, TIMEOUT)
        .unwrap();
    gui.wait_until_text(main_window::ROOM_SEARCH_FIELD, "", TIMEOUT)
        .unwrap();
    gui.wait_until_visible(main_window::ROOM_LIST, TIMEOUT)
        .unwrap();
}

// ROOM-010
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn the_clear_button_empties_the_field_and_keeps_the_focus() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, channel_name) = admin.create_channel(&[&user.username]);
    let (group_id, _) = admin.create_group(&[&user.username]);
    let app = App::launch().unwrap();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &group_id);
    // The keycap shows while the field is empty and unfocused, and the button does not.
    gui.wait_until_visible(main_window::ROOM_SEARCH_HINT, TIMEOUT)
        .unwrap();
    assert!(
        !gui.exists_and_visible(main_window::ROOM_SEARCH_CLEAR)
            .unwrap()
    );
    search_for(gui, &channel_name);
    gui.wait_until_visible(main_window::ROOM_SEARCH_CLEAR, TIMEOUT)
        .unwrap();
    gui.wait_until_hidden(main_window::ROOM_SEARCH_HINT, TIMEOUT)
        .unwrap();
    gui.wait_until_hidden(main_window::ROOM_LIST, TIMEOUT)
        .unwrap();

    gui.mouse_click(main_window::ROOM_SEARCH_CLEAR).unwrap();

    gui.wait_until_text(main_window::ROOM_SEARCH_FIELD, "", TIMEOUT)
        .unwrap();
    gui.wait_until_visible(&room(&channel_id, room_row::NAME), TIMEOUT)
        .unwrap();
    gui.wait_until_hidden(main_window::ROOM_SEARCH_CLEAR, TIMEOUT)
        .unwrap();
    // Still focused: the keycap stays away, and typing goes to the field.
    assert!(
        !gui.exists_and_visible(main_window::ROOM_SEARCH_HINT)
            .unwrap()
    );
    gui.input_text(main_window::ROOM_SEARCH_FIELD, &channel_name)
        .unwrap();
    gui.wait_until_text(main_window::ROOM_SEARCH_FIELD, &channel_name, TIMEOUT)
        .unwrap();
}

// ROOM-010
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn escape_clears_the_field() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (_, channel_name) = admin.create_channel(&[&user.username]);
    let (group_id, _) = admin.create_group(&[&user.username]);
    let app = App::launch().unwrap();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &group_id);
    search_for(gui, &channel_name);
    gui.wait_until_hidden(main_window::ROOM_LIST, TIMEOUT)
        .unwrap();

    press(gui, Key::ESCAPE);

    gui.wait_until_text(main_window::ROOM_SEARCH_FIELD, "", TIMEOUT)
        .unwrap();
    gui.wait_until_visible(main_window::ROOM_LIST, TIMEOUT)
        .unwrap();
}

// ROOM-010
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_query_that_matches_nothing_says_so() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (group_id, _) = admin.create_group(&[&user.username]);
    let app = App::launch().unwrap();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &group_id);

    // The line repeats the query without the whitespace around it.
    search_for(gui, "  no-such-room-anywhere ");

    gui.wait_until_text(
        main_window::ROOM_SEARCH_EMPTY,
        "No rooms match \u{201c}no-such-room-anywhere\u{201d}",
        TIMEOUT,
    )
    .unwrap();
    assert_eq!(gui.count(main_window::ROOM_SEARCH_RESULTS).unwrap(), 0);
    gui.wait_until_hidden(main_window::ROOM_LIST, TIMEOUT)
        .unwrap();
    // Enter with nothing highlighted opens nothing, and the search stays.
    press(gui, Key::RETURN);
    assert!(
        !gui.exists_and_visible(main_window::ROOM_TITLE).unwrap(),
        "no room should have opened"
    );
    assert!(
        gui.exists_and_visible(main_window::ROOM_SEARCH_EMPTY)
            .unwrap()
    );
}

// ROOM-010
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn ctrl_k_focuses_the_search_field() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (_, channel_name) = admin.create_channel(&[&user.username]);
    let (group_id, _) = admin.create_group(&[&user.username]);
    let app = App::launch().unwrap();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &group_id);
    // The focus is in the composer of the open room.
    gui.mouse_click(&room(&group_id, room_row::NAME)).unwrap();
    gui.wait_until_visible(main_window::COMPOSER_FIELD, TIMEOUT)
        .unwrap();
    gui.mouse_click(main_window::COMPOSER_FIELD).unwrap();

    gui.enter_key(main_window::COMPOSER_FIELD, Key::K, Modifiers::CONTROL)
        .unwrap();
    // Text input goes to the item with the focus: the search field, if the shortcut worked.
    gui.input_text(main_window::ROOM_SEARCH_FIELD, &channel_name)
        .unwrap();

    gui.wait_until_text(main_window::ROOM_SEARCH_FIELD, &channel_name, TIMEOUT)
        .unwrap();
    gui.wait_until_text(main_window::COMPOSER_FIELD, "", TIMEOUT)
        .unwrap();
}
