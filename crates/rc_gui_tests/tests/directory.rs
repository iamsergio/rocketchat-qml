// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The server's directory in the room search: the people and the public channels the user has no
//! room with, found next to their own rooms, a direct message started from a person, a channel
//! previewed and joined. Against the local test server (`RC_TEST_SERVER_URL`). The server holds
//! tens of thousands of channels, so each test makes its own users and channels under a name
//! prefix that is unique to it, and searches for that prefix.

// Helpers outside #[test] fns may unwrap: a failed step should just fail the test.
#![allow(clippy::unwrap_used)]

mod common;

use std::time::Duration;

use rc_gui_tests::paths::{
    directory_channel, directory_person, directory_row, main_window, message, message_row, room,
    room_row, search_result,
};
use rc_gui_tests::{App, Gui, Key, Modifiers};
use rc_test_server::{Admin, Proxy, TestUser, unique};

use common::{TIMEOUT, log_in_and_wait_for_room, open_room, wait_for_message_after};

/// Starts the app without the pause before the directory is asked, so that the tests don't wait
/// for it.
fn launch() -> App {
    App::launch_with(&[("TZ", "UTC"), ("RC_DIRECTORY_DEBOUNCE_MS", "0")]).unwrap()
}

/// A name prefix no other test or run uses, with nothing in it the directory would take apart.
fn prefix() -> String {
    format!("dq{}", unique().replace('-', ""))
}

/// Types the query, as the user would: with the focus in the field.
fn search_for(gui: &Gui, query: &str) {
    gui.mouse_click(main_window::ROOM_SEARCH_FIELD).unwrap();
    gui.set_text(main_window::ROOM_SEARCH_FIELD, query).unwrap();
}

fn press(gui: &Gui, key: Key) {
    gui.enter_key(main_window::ROOM_SEARCH_FIELD, key, Modifiers::NONE)
        .unwrap();
}

/// The user's own channel, named by `prefix` so that the search finds it, which the app logs in
/// to. Returns its id.
fn own_channel(admin: &Admin, user: &TestUser, prefix: &str) -> String {
    admin
        .create_channel_named(&format!("{prefix}-own"), &[&user.username])
        .0
}

/// The vertical position of an element, to compare the order of rows.
fn y(gui: &Gui, path: &str) -> f64 {
    gui.bounding_box(path).unwrap().y
}

/// Searches for `prefix` and waits for the person's row.
fn find_person(gui: &Gui, prefix: &str, person: &TestUser) {
    search_for(gui, prefix);
    gui.wait_until_visible(
        &directory_person(&person.username, directory_row::NAME),
        TIMEOUT,
    )
    .unwrap();
}

// ACT-007
// ACT-008
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn people_and_channels_the_user_has_no_room_with_show_in_their_groups() {
    let admin = Admin::login();
    let user = admin.create_user();
    let prefix = prefix();
    let own_id = own_channel(&admin, &user, &prefix);
    let stranger = admin.create_user_named(&format!("{prefix} Stranger"));
    let friend = admin.create_user_named(&format!("{prefix} Friend"));
    // The user has a direct message with the friend already.
    let _ = Admin::login_as(&user.username, &user.password).create_dm(&friend.username);
    let (channel_id, channel_name) = admin.create_channel_named(&format!("{prefix}-open"), &[]);
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &own_id);

    find_person(gui, &prefix, &stranger);

    gui.wait_until_visible(
        &directory_channel(&channel_id, directory_row::NAME),
        TIMEOUT,
    )
    .unwrap();
    assert_eq!(
        gui.text(&directory_channel(&channel_id, directory_row::NAME))
            .unwrap(),
        channel_name
    );
    assert_eq!(
        gui.text(&directory_person(&stranger.username, directory_row::NAME))
            .unwrap(),
        stranger.name
    );
    assert_eq!(
        gui.text(&directory_person(
            &stranger.username,
            directory_row::USERNAME
        ))
        .unwrap(),
        stranger.username
    );
    assert!(
        gui.text(&directory_channel(&channel_id, directory_row::MEMBERS))
            .unwrap()
            .contains(" member")
    );
    // Their own channel is among their rooms, not in the directory's group; the friend, whom
    // they have a direct message with, is in none.
    gui.wait_until_visible(&search_result(&own_id, room_row::NAME), TIMEOUT)
        .unwrap();
    assert!(
        !gui.exists_and_visible(&directory_channel(&own_id, directory_row::NAME))
            .unwrap()
    );
    assert!(
        !gui.exists_and_visible(&directory_person(&friend.username, directory_row::NAME))
            .unwrap()
    );
    // Top to bottom: channels, people, the user's rooms, then the field.
    for label in [
        main_window::DIRECTORY_GROUP_CHANNELS,
        main_window::DIRECTORY_GROUP_PEOPLE,
        main_window::DIRECTORY_GROUP_ROOMS,
    ] {
        gui.wait_until_visible(label, TIMEOUT).unwrap();
    }
    assert_eq!(
        gui.text(main_window::DIRECTORY_GROUP_ROOMS).unwrap(),
        "Your rooms"
    );
    let rows = [
        main_window::DIRECTORY_GROUP_CHANNELS.to_owned(),
        directory_channel(&channel_id, directory_row::NAME),
        main_window::DIRECTORY_GROUP_PEOPLE.to_owned(),
        directory_person(&stranger.username, directory_row::NAME),
        main_window::DIRECTORY_GROUP_ROOMS.to_owned(),
        search_result(&own_id, room_row::NAME),
        main_window::ROOM_SEARCH_FIELD.to_owned(),
    ];
    let ys: Vec<f64> = rows.iter().map(|path| y(gui, path)).collect();
    assert!(
        ys.windows(2).all(|pair| pair[0] < pair[1]),
        "{rows:?} {ys:?}"
    );
    // The directory is done: nothing says it is still searching.
    assert!(
        !gui.exists_and_visible(main_window::DIRECTORY_SEARCHING)
            .unwrap()
    );
}

// ACT-007
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn messaging_a_person_opens_the_direct_message_and_the_person_leaves_the_directory() {
    let admin = Admin::login();
    let user = admin.create_user();
    let prefix = prefix();
    let own_id = own_channel(&admin, &user, &prefix);
    let person = admin.create_user_named(&format!("{prefix} Newcomer"));
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &own_id);
    find_person(gui, &prefix, &person);

    gui.mouse_click(&directory_person(&person.username, directory_row::NAME))
        .unwrap();

    gui.wait_until_text(main_window::ROOM_TITLE, &person.name, TIMEOUT)
        .unwrap();
    gui.wait_until_text(main_window::ROOM_SEARCH_FIELD, "", TIMEOUT)
        .unwrap();
    // The direct message is in the room list, as an ordinary room.
    let dm_id = Admin::login_as(&user.username, &user.password).create_dm(&person.username);
    gui.wait_until_visible(&room(&dm_id, room_row::NAME), TIMEOUT)
        .unwrap();
    gui.wait_until_visible(&room(&dm_id, room_row::KIND_DIRECT), TIMEOUT)
        .unwrap();
    // The composer has the focus.
    gui.input_text(main_window::COMPOSER_FIELD, "typed")
        .unwrap();
    gui.wait_until_text(main_window::COMPOSER_FIELD, "typed", TIMEOUT)
        .unwrap();
    // Searching again finds the room, and the person is no longer offered.
    search_for(gui, &prefix);
    gui.wait_until_visible(&search_result(&dm_id, room_row::NAME), TIMEOUT)
        .unwrap();
    gui.wait_until_hidden(main_window::DIRECTORY_SEARCHING, TIMEOUT)
        .unwrap();
    assert!(
        !gui.exists_and_visible(&directory_person(&person.username, directory_row::NAME))
            .unwrap()
    );
}

// ACT-007
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn up_moves_the_cursor_to_a_person_and_enter_messages_them() {
    let admin = Admin::login();
    let user = admin.create_user();
    let prefix = prefix();
    let own_id = own_channel(&admin, &user, &prefix);
    let person = admin.create_user_named(&format!("{prefix} Keyboard"));
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &own_id);
    find_person(gui, &prefix, &person);
    // The pointer or the cursor shows what a click does; neither is on the row.
    let hint = directory_person(&person.username, directory_row::HINT);
    assert!(!gui.exists_and_visible(&hint).unwrap());

    // The cursor starts on the user's room, the best match; the person is above it.
    press(gui, Key::UP);

    gui.wait_until_visible(&hint, TIMEOUT).unwrap();
    assert_eq!(gui.text(&hint).unwrap(), "Message");
    press(gui, Key::RETURN);
    gui.wait_until_text(main_window::ROOM_TITLE, &person.name, TIMEOUT)
        .unwrap();
    gui.wait_until_text(main_window::ROOM_SEARCH_FIELD, "", TIMEOUT)
        .unwrap();
}

// ACT-008
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_channel_is_previewed_with_its_messages_and_a_join_button() {
    let admin = Admin::login();
    let user = admin.create_user();
    let prefix = prefix();
    let own_id = own_channel(&admin, &user, &prefix);
    let (channel_id, channel_name) = admin.create_channel_named(&format!("{prefix}-open"), &[]);
    let earlier = admin.post_message(&channel_id, &format!("earlier {}", unique()));
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &own_id);
    search_for(gui, &prefix);
    let row = directory_channel(&channel_id, directory_row::NAME);
    gui.wait_until_visible(&row, TIMEOUT).unwrap();

    gui.mouse_click(&row).unwrap();

    gui.wait_until_text(main_window::ROOM_TITLE, &channel_name, TIMEOUT)
        .unwrap();
    gui.wait_until_visible(&message(&earlier.id, message_row::BODY), TIMEOUT)
        .unwrap();
    gui.wait_until_visible(main_window::COMPOSER_JOIN, TIMEOUT)
        .unwrap();
    assert_eq!(gui.text(main_window::COMPOSER_JOIN).unwrap(), "Join");
    assert!(!gui.exists_and_visible(main_window::COMPOSER_SEND).unwrap());
    // The field is clear, the list is back, and the channel is not in it.
    gui.wait_until_text(main_window::ROOM_SEARCH_FIELD, "", TIMEOUT)
        .unwrap();
    gui.wait_until_visible(&room(&own_id, room_row::NAME), TIMEOUT)
        .unwrap();
    assert!(
        !gui.exists_and_visible(&room(&channel_id, room_row::NAME))
            .unwrap()
    );
    // What others say in the channel shows live.
    let text = format!("live {}", unique());
    let live = admin.post_message(&channel_id, &text);
    gui.wait_until_text(&message(&live.id, message_row::BODY), &text, TIMEOUT)
        .unwrap();
    // Enter in the composer does not send: the text waits for the join.
    gui.mouse_click(main_window::COMPOSER_FIELD).unwrap();
    gui.set_text(main_window::COMPOSER_FIELD, "waiting")
        .unwrap();
    gui.enter_key(main_window::COMPOSER_FIELD, Key::RETURN, Modifiers::NONE)
        .unwrap();
    gui.wait(Duration::from_millis(300)).unwrap();
    assert_eq!(gui.text(main_window::COMPOSER_FIELD).unwrap(), "waiting");
    assert_eq!(admin.last_message(&channel_id).id, live.id);
}

// ACT-008
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn joining_lists_the_channel_as_the_open_room_and_keeps_the_draft() {
    let admin = Admin::login();
    let user = admin.create_user();
    let prefix = prefix();
    let own_id = own_channel(&admin, &user, &prefix);
    let (channel_id, channel_name) = admin.create_channel_named(&format!("{prefix}-open"), &[]);
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &own_id);
    search_for(gui, &prefix);
    gui.wait_until_visible(
        &directory_channel(&channel_id, directory_row::NAME),
        TIMEOUT,
    )
    .unwrap();
    gui.mouse_click(&directory_channel(&channel_id, directory_row::NAME))
        .unwrap();
    gui.wait_until_visible(main_window::COMPOSER_JOIN, TIMEOUT)
        .unwrap();
    gui.mouse_click(main_window::COMPOSER_FIELD).unwrap();
    gui.set_text(main_window::COMPOSER_FIELD, "my draft")
        .unwrap();

    gui.mouse_click(main_window::COMPOSER_JOIN).unwrap();

    gui.wait_until_visible(&room(&channel_id, room_row::NAME), TIMEOUT)
        .unwrap();
    gui.wait_until_visible(main_window::COMPOSER_SEND, TIMEOUT)
        .unwrap();
    gui.wait_until_hidden(main_window::COMPOSER_JOIN, TIMEOUT)
        .unwrap();
    assert_eq!(gui.text(main_window::ROOM_TITLE).unwrap(), channel_name);
    assert_eq!(gui.text(main_window::COMPOSER_FIELD).unwrap(), "my draft");
    // The channel is theirs now: it is among their rooms, not in the directory's group.
    search_for(gui, &prefix);
    gui.wait_until_visible(&search_result(&channel_id, room_row::NAME), TIMEOUT)
        .unwrap();
    gui.wait_until_hidden(main_window::DIRECTORY_SEARCHING, TIMEOUT)
        .unwrap();
    assert!(
        !gui.exists_and_visible(&directory_channel(&channel_id, directory_row::NAME))
            .unwrap()
    );
    // And the draft is sent as an ordinary message now.
    press(gui, Key::ESCAPE);
    let before = admin.last_message(&channel_id).id;
    gui.mouse_click(main_window::COMPOSER_FIELD).unwrap();
    gui.enter_key(main_window::COMPOSER_FIELD, Key::RETURN, Modifiers::NONE)
        .unwrap();
    let sent = wait_for_message_after(&admin, &channel_id, &before);
    gui.wait_until_text(&message(&sent.id, message_row::BODY), "my draft", TIMEOUT)
        .unwrap();
}

// ACT-008
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn opening_another_room_ends_the_preview() {
    let admin = Admin::login();
    let user = admin.create_user();
    let prefix = prefix();
    let own_id = own_channel(&admin, &user, &prefix);
    let (channel_id, _) = admin.create_channel_named(&format!("{prefix}-open"), &[]);
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &own_id);
    search_for(gui, &prefix);
    gui.wait_until_visible(
        &directory_channel(&channel_id, directory_row::NAME),
        TIMEOUT,
    )
    .unwrap();
    gui.mouse_click(&directory_channel(&channel_id, directory_row::NAME))
        .unwrap();
    gui.wait_until_visible(main_window::COMPOSER_JOIN, TIMEOUT)
        .unwrap();

    open_room(gui, &own_id);

    gui.wait_until_hidden(main_window::COMPOSER_JOIN, TIMEOUT)
        .unwrap();
    gui.wait_until_visible(main_window::COMPOSER_SEND, TIMEOUT)
        .unwrap();
    assert!(
        !gui.exists_and_visible(&room(&channel_id, room_row::NAME))
            .unwrap()
    );
    // The channel is still the directory's.
    search_for(gui, &prefix);
    gui.wait_until_visible(
        &directory_channel(&channel_id, directory_row::NAME),
        TIMEOUT,
    )
    .unwrap();
}

// ACT-008
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_group_with_more_than_five_says_how_many_more() {
    let admin = Admin::login();
    let user = admin.create_user();
    let prefix = prefix();
    let own_id = own_channel(&admin, &user, &prefix);
    let people: Vec<TestUser> = (0..7)
        .map(|n| admin.create_user_named(&format!("{prefix} Person{n}")))
        .collect();
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &own_id);

    search_for(gui, &prefix);

    gui.wait_until_visible(main_window::DIRECTORY_MORE_PEOPLE, TIMEOUT)
        .unwrap();
    assert_eq!(
        gui.text(main_window::DIRECTORY_MORE_PEOPLE).unwrap(),
        "2 more, keep typing"
    );
    let shown = people
        .iter()
        .filter(|person| {
            gui.exists_and_visible(&directory_person(&person.username, directory_row::NAME))
                .unwrap()
        })
        .count();
    assert_eq!(shown, 5);
    // The channels have nothing to add.
    assert!(
        !gui.exists_and_visible(main_window::DIRECTORY_MORE_CHANNELS)
            .unwrap()
    );
    // The line is under the group's rows, over the user's rooms.
    let more = y(gui, main_window::DIRECTORY_MORE_PEOPLE);
    assert!(more < y(gui, main_window::DIRECTORY_GROUP_ROOMS));
    for person in &people {
        let path = directory_person(&person.username, directory_row::NAME);
        if gui.exists_and_visible(&path).unwrap() {
            assert!(y(gui, &path) < more);
        }
    }
}

// ACT-008
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn nothing_found_says_so_once_the_directory_has_answered() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (group_id, _) = admin.create_group(&[&user.username]);
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &group_id);
    let query = format!("nobody-{}", prefix());

    search_for(gui, &format!(" {query} "));

    gui.wait_until_text(
        main_window::ROOM_SEARCH_EMPTY,
        &format!("No rooms, people or channels match \u{201c}{query}\u{201d}"),
        TIMEOUT,
    )
    .unwrap();
    // The text is there before the line is: it is shown once the directory has answered.
    gui.wait_until_visible(main_window::ROOM_SEARCH_EMPTY, TIMEOUT)
        .unwrap();
    assert!(
        !gui.exists_and_visible(main_window::DIRECTORY_SEARCHING)
            .unwrap()
    );
    assert_eq!(gui.count(main_window::ROOM_SEARCH_RESULTS).unwrap(), 0);
}

// ACT-008
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn the_user_waits_for_the_directory_over_their_own_rooms() {
    let admin = Admin::login();
    let user = admin.create_user();
    let prefix = prefix();
    let own_id = own_channel(&admin, &user, &prefix);
    let (channel_id, _) = admin.create_channel_named(&format!("{prefix}-open"), &[]);
    // A pause long enough to look at the row that says the directory is being searched.
    let app = App::launch_with(&[("TZ", "UTC"), ("RC_DIRECTORY_DEBOUNCE_MS", "3000")]).unwrap();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &own_id);

    search_for(gui, &prefix);

    // The user's rooms are there at once, under their label.
    gui.wait_until_visible(main_window::DIRECTORY_SEARCHING, TIMEOUT)
        .unwrap();
    assert_eq!(
        gui.text(main_window::DIRECTORY_SEARCHING).unwrap(),
        "Searching the directory\u{2026}"
    );
    gui.wait_until_visible(&search_result(&own_id, room_row::NAME), TIMEOUT)
        .unwrap();
    gui.wait_until_visible(main_window::DIRECTORY_GROUP_ROOMS, TIMEOUT)
        .unwrap();
    assert!(y(gui, main_window::DIRECTORY_SEARCHING) < y(gui, main_window::DIRECTORY_GROUP_ROOMS));
    // Then the answer replaces the row.
    gui.wait_until_visible(
        &directory_channel(&channel_id, directory_row::NAME),
        TIMEOUT,
    )
    .unwrap();
    assert!(
        !gui.exists_and_visible(main_window::DIRECTORY_SEARCHING)
            .unwrap()
    );
}

// ACT-008
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_directory_that_cannot_be_reached_says_so_and_retry_asks_again() {
    let admin = Admin::login();
    let user = admin.create_user();
    let prefix = prefix();
    let own_id = own_channel(&admin, &user, &prefix);
    let (channel_id, _) = admin.create_channel_named(&format!("{prefix}-open"), &[]);
    let proxy = Proxy::start();
    let app = launch();
    let gui = app.gui();
    common::log_in_at_and_wait_for_room(gui, &proxy.url(), &user, &own_id);
    proxy.set_accepting(false);
    proxy.drop_connections();

    search_for(gui, &prefix);

    gui.wait_until_text(
        main_window::DIRECTORY_ERROR,
        "Can't reach the server's directory.",
        TIMEOUT,
    )
    .unwrap();
    // The user's rooms don't go away for it.
    gui.wait_until_visible(&search_result(&own_id, room_row::NAME), TIMEOUT)
        .unwrap();
    gui.wait_until_visible(main_window::DIRECTORY_GROUP_ROOMS, TIMEOUT)
        .unwrap();
    proxy.set_accepting(true);

    gui.mouse_click(main_window::DIRECTORY_RETRY).unwrap();

    gui.wait_until_visible(
        &directory_channel(&channel_id, directory_row::NAME),
        TIMEOUT,
    )
    .unwrap();
    assert!(
        !gui.exists_and_visible(main_window::DIRECTORY_ERROR)
            .unwrap()
    );
}
