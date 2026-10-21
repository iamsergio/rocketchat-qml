// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! The favorites section of the room list and the menu of a room, against the local test server
//! (`RC_TEST_SERVER_URL`). The menu opens with a right-click on the row and is a popup, so its
//! items are found from the window while it is open. Each test makes its own user and rooms and
//! only looks at those. Favorites belong to the user, so they are set as the user.
//!
//! A new user also joins `general`, so the list holds a handful of rooms and headers: they fit in
//! the window and every row exists.

// Helpers outside #[test] fns may unwrap: a failed step should just fail the test.
#![allow(clippy::unwrap_used)]

mod common;

use rc_gui_tests::paths::{main_window, room, room_row};
use rc_gui_tests::{Gui, Key, Modifiers, MouseButton};
use rc_test_server::Admin;

use common::{TIMEOUT, log_in_and_wait_for_room, open_room};

/// The row of the room, whole.
fn row(id: &str) -> String {
    format!("{}/room_{id}", main_window::WINDOW)
}

/// Opens the menu of the room with a right-click on its row.
fn open_menu(gui: &Gui, id: &str) {
    gui.mouse_click_with(&row(id), MouseButton::Right, Modifiers::NONE)
        .unwrap();
    gui.wait_until_visible(main_window::ROOM_FAVORITE_ACTION, TIMEOUT)
        .unwrap();
}

/// Opens the menu of the room and chooses the item at `action`.
fn choose(gui: &Gui, id: &str, action: &str) {
    open_menu(gui, id);
    gui.wait_until_visible(action, TIMEOUT).unwrap();
    gui.mouse_click(action).unwrap();
}

/// Closes the open menu with Escape.
fn close_menu(gui: &Gui) {
    gui.enter_key(main_window::ROOM_LIST, Key::ESCAPE, Modifiers::NONE)
        .unwrap();
    gui.wait_until_hidden(main_window::ROOM_FAVORITE_ACTION, TIMEOUT)
        .unwrap();
}

fn y_of(gui: &Gui, path: &str) -> f64 {
    gui.bounding_box(path).unwrap().y
}

/// Waits until the rooms stand one above the other in this order.
fn wait_for_order(gui: &Gui, ids: &[&str]) {
    gui.wait_until(&format!("the rooms in the order {ids:?}"), TIMEOUT, || {
        let ys: Result<Vec<f64>, _> = ids
            .iter()
            .map(|id| gui.bounding_box(&room(id, room_row::NAME)).map(|r| r.y))
            .collect();
        Ok(ys?.windows(2).all(|pair| pair[0] < pair[1]))
    })
    .unwrap();
}

fn headers_shown(gui: &Gui) -> bool {
    gui.exists_and_visible(main_window::ROOM_SECTION_FAVORITES_HEADER)
        .unwrap()
        && gui
            .exists_and_visible(main_window::ROOM_SECTION_ROOMS_HEADER)
            .unwrap()
}

/// A user with two channels, the second with the newer message, so that it comes first.
struct Pair {
    user: rc_test_server::TestUser,
    older: String,
    newer: String,
}

fn user_with_two_channels(admin: &Admin) -> Pair {
    let user = admin.create_user();
    let (older, _) = admin.create_channel(&[&user.username]);
    let (newer, _) = admin.create_channel(&[&user.username]);
    admin.post_message(&older, "first");
    admin.post_message(&newer, "second");
    Pair { user, older, newer }
}

// ROOM-006
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn without_favorites_the_list_has_no_headers() {
    let admin = Admin::login();
    let Pair {
        user, older, newer, ..
    } = user_with_two_channels(&admin);
    let app = common::launch();
    let gui = app.gui();

    log_in_and_wait_for_room(gui, &user, &older);
    wait_for_order(gui, &[&newer, &older]);

    for header in [
        main_window::ROOM_SECTION_FAVORITES_HEADER,
        main_window::ROOM_SECTION_ROOMS_HEADER,
    ] {
        assert!(!gui.exists_and_visible(header).unwrap(), "{header}");
    }
    assert!(
        !gui.exists_and_visible(&room(&older, room_row::FAVORITE))
            .unwrap()
    );
}

// ROOM-006, ROOM-007
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_room_is_favorited_and_unfavorited_through_its_menu() {
    let admin = Admin::login();
    let Pair {
        user, older, newer, ..
    } = user_with_two_channels(&admin);
    let app = common::launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &older);
    wait_for_order(gui, &[&newer, &older]);

    open_menu(gui, &older);
    assert_eq!(
        gui.text(main_window::ROOM_FAVORITE_ACTION).unwrap(),
        "Add to favorites"
    );
    gui.mouse_click(main_window::ROOM_FAVORITE_ACTION).unwrap();

    // The older room moves up, under "Favorites", above "Rooms" and the newer room.
    gui.wait_until_visible(&room(&older, room_row::FAVORITE), TIMEOUT)
        .unwrap();
    gui.wait_until("the favorites section above the rest", TIMEOUT, || {
        Ok(headers_shown(gui)
            && y_of(gui, main_window::ROOM_SECTION_FAVORITES) < y_of(gui, &row(&older))
            && y_of(gui, &row(&older)) < y_of(gui, main_window::ROOM_SECTION_ROOMS)
            && y_of(gui, main_window::ROOM_SECTION_ROOMS) < y_of(gui, &row(&newer)))
    })
    .unwrap();
    assert_eq!(
        gui.text(main_window::ROOM_SECTION_FAVORITES_HEADER)
            .unwrap(),
        "Favorites"
    );
    assert_eq!(
        gui.text(main_window::ROOM_SECTION_ROOMS_HEADER).unwrap(),
        "Rooms"
    );
    assert!(
        !gui.exists_and_visible(&room(&newer, room_row::FAVORITE))
            .unwrap()
    );

    open_menu(gui, &older);
    assert_eq!(
        gui.text(main_window::ROOM_FAVORITE_ACTION).unwrap(),
        "Remove from favorites"
    );
    gui.mouse_click(main_window::ROOM_FAVORITE_ACTION).unwrap();

    // Back among the rooms, in the order of their activity, and no headers any more.
    gui.wait_until_hidden(&room(&older, room_row::FAVORITE), TIMEOUT)
        .unwrap();
    gui.wait_until_hidden(main_window::ROOM_SECTION_FAVORITES_HEADER, TIMEOUT)
        .unwrap();
    gui.wait_until_hidden(main_window::ROOM_SECTION_ROOMS_HEADER, TIMEOUT)
        .unwrap();
    wait_for_order(gui, &[&newer, &older]);
}

// ROOM-006
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_favorite_set_elsewhere_arrives_live_and_moves_the_room() {
    let admin = Admin::login();
    let Pair {
        user, older, newer, ..
    } = user_with_two_channels(&admin);
    let app = common::launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &older);
    wait_for_order(gui, &[&newer, &older]);

    // As the web client would.
    let web = Admin::login_as(&user.username, &user.password);
    web.set_favorite(&older, true);

    gui.wait_until_visible(main_window::ROOM_SECTION_FAVORITES_HEADER, TIMEOUT)
        .unwrap();
    gui.wait_until_visible(&room(&older, room_row::FAVORITE), TIMEOUT)
        .unwrap();
    gui.wait_until("the favorite above the other room", TIMEOUT, || {
        Ok(y_of(gui, &row(&older)) < y_of(gui, &row(&newer)))
    })
    .unwrap();

    web.set_favorite(&older, false);

    gui.wait_until_hidden(main_window::ROOM_SECTION_FAVORITES_HEADER, TIMEOUT)
        .unwrap();
    wait_for_order(gui, &[&newer, &older]);
}

// ROOM-006
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn favorites_stay_on_top_and_order_themselves_by_activity() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (first, _) = admin.create_channel(&[&user.username]);
    let (second, _) = admin.create_channel(&[&user.username]);
    let (other, _) = admin.create_channel(&[&user.username]);
    admin.post_message(&other, "the newest of all");
    admin.post_message(&first, "older");
    admin.post_message(&second, "newer");
    // The newest room is the one that isn't a favorite.
    let web = Admin::login_as(&user.username, &user.password);
    web.set_favorite(&first, true);
    web.set_favorite(&second, true);
    let app = common::launch();
    let gui = app.gui();

    log_in_and_wait_for_room(gui, &user, &first);

    // Favorites by activity, then the other room, whose message is the oldest.
    wait_for_order(gui, &[&second, &first, &other]);
    assert!(y_of(gui, &row(&first)) < y_of(gui, main_window::ROOM_SECTION_ROOMS));
    assert!(y_of(gui, main_window::ROOM_SECTION_ROOMS) < y_of(gui, &row(&other)));

    // A message in the other room keeps it below the favorites.
    admin.post_message(&other, "still not a favorite");
    // Channels count only mentions as unread; the bump of the room is what moves it.
    gui.wait(std::time::Duration::from_millis(500)).unwrap();
    wait_for_order(gui, &[&second, &first, &other]);

    // One in the older favorite puts it above the other favorite.
    admin.post_message(&first, "now the newest favorite");
    wait_for_order(gui, &[&first, &second, &other]);
    assert!(y_of(gui, &row(&second)) < y_of(gui, main_window::ROOM_SECTION_ROOMS));
}

// ROOM-007
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_room_is_marked_read_and_unread_through_its_menu() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (id, _) = admin.create_channel(&[&user.username]);
    admin.post_message(&id, &format!("hello @{}", user.username));
    let app = common::launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &id);
    gui.wait_until_text(&room(&id, room_row::UNREAD), "1", TIMEOUT)
        .unwrap();

    open_menu(gui, &id);
    assert!(
        !gui.exists_and_visible(main_window::ROOM_MARK_UNREAD_ACTION)
            .unwrap()
    );
    gui.mouse_click(main_window::ROOM_MARK_READ_ACTION).unwrap();

    gui.wait_until_hidden(&room(&id, room_row::UNREAD), TIMEOUT)
        .unwrap();
    gui.wait_until_hidden(&room(&id, room_row::MENTIONS), TIMEOUT)
        .unwrap();

    open_menu(gui, &id);
    assert!(
        !gui.exists_and_visible(main_window::ROOM_MARK_READ_ACTION)
            .unwrap()
    );
    gui.mouse_click(main_window::ROOM_MARK_UNREAD_ACTION)
        .unwrap();

    gui.wait_until_text(&room(&id, room_row::UNREAD), "1", TIMEOUT)
        .unwrap();
    // It stays so: the server's push of the same change doesn't undo it.
    gui.wait(std::time::Duration::from_millis(500)).unwrap();
    assert_eq!(gui.text(&room(&id, room_row::UNREAD)).unwrap(), "1");
    assert!(
        !gui.exists_and_visible(main_window::ROOM_LIST_ERROR)
            .unwrap()
    );
}

// ROOM-007
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn the_open_room_is_not_offered_to_be_marked_unread() {
    let admin = Admin::login();
    let Pair { user, older, .. } = user_with_two_channels(&admin);
    // Nothing was said in it, so nothing is unread.
    let (quiet, _) = admin.create_channel(&[&user.username]);
    let app = common::launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &older);
    open_room(gui, &older);
    gui.wait_until_visible(main_window::ROOM_TITLE, TIMEOUT)
        .unwrap();

    open_menu(gui, &older);
    assert!(
        !gui.exists_and_visible(main_window::ROOM_MARK_UNREAD_ACTION)
            .unwrap()
    );
    assert!(
        gui.exists_and_visible(main_window::ROOM_LEAVE_ACTION)
            .unwrap()
    );
    close_menu(gui);

    // Another room that shows read can be.
    gui.wait_until_visible(&room(&quiet, room_row::NAME), TIMEOUT)
        .unwrap();
    open_menu(gui, &quiet);
    gui.wait_until_visible(main_window::ROOM_MARK_UNREAD_ACTION, TIMEOUT)
        .unwrap();
    assert_eq!(
        gui.text(main_window::ROOM_MARK_UNREAD_ACTION).unwrap(),
        "Mark as unread"
    );
}

// ROOM-007
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn leaving_asks_first_cancel_keeps_the_room_and_confirming_removes_it() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel, channel_name) = admin.create_channel(&[&user.username]);
    let (group, _) = admin.create_group(&[&user.username]);
    let app = common::launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel);
    gui.wait_until_visible(&room(&group, room_row::NAME), TIMEOUT)
        .unwrap();

    choose(gui, &channel, main_window::ROOM_LEAVE_ACTION);

    gui.wait_until_visible(main_window::ROOM_LEAVE_DIALOG, TIMEOUT)
        .unwrap();
    assert!(
        gui.text(main_window::ROOM_LEAVE_DIALOG)
            .unwrap()
            .contains(&channel_name)
    );
    gui.mouse_click(main_window::ROOM_LEAVE_CANCEL).unwrap();
    gui.wait_until_hidden(main_window::ROOM_LEAVE_DIALOG, TIMEOUT)
        .unwrap();
    assert!(
        gui.exists_and_visible(&room(&channel, room_row::NAME))
            .unwrap()
    );

    choose(gui, &channel, main_window::ROOM_LEAVE_ACTION);
    gui.wait_until_visible(main_window::ROOM_LEAVE_CONFIRM, TIMEOUT)
        .unwrap();
    gui.mouse_click(main_window::ROOM_LEAVE_CONFIRM).unwrap();

    gui.wait_until_hidden(&room(&channel, room_row::NAME), TIMEOUT)
        .unwrap();
    // A private group is left the same way, and the other rooms stay.
    choose(gui, &group, main_window::ROOM_LEAVE_ACTION);
    gui.wait_until_visible(main_window::ROOM_LEAVE_CONFIRM, TIMEOUT)
        .unwrap();
    gui.mouse_click(main_window::ROOM_LEAVE_CONFIRM).unwrap();
    gui.wait_until_hidden(&room(&group, room_row::NAME), TIMEOUT)
        .unwrap();
    assert!(
        !gui.exists_and_visible(main_window::ROOM_LIST_ERROR)
            .unwrap()
    );
}

// ROOM-007
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn leaving_the_open_room_shows_the_room_placeholder() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (id, _) = admin.create_channel(&[&user.username]);
    admin.post_message(&id, "hello");
    let app = common::launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &id);
    open_room(gui, &id);
    gui.wait_until_visible(main_window::ROOM_TITLE, TIMEOUT)
        .unwrap();

    choose(gui, &id, main_window::ROOM_LEAVE_ACTION);
    gui.wait_until_visible(main_window::ROOM_LEAVE_CONFIRM, TIMEOUT)
        .unwrap();
    gui.mouse_click(main_window::ROOM_LEAVE_CONFIRM).unwrap();

    gui.wait_until_hidden(&room(&id, room_row::NAME), TIMEOUT)
        .unwrap();
    gui.wait_until_visible(main_window::ROOM_PLACEHOLDER, TIMEOUT)
        .unwrap();
    assert!(!gui.exists_and_visible(main_window::ROOM_TITLE).unwrap());
}

// ROOM-007
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_direct_message_menu_has_no_leave_item() {
    let admin = Admin::login();
    let user = admin.create_user();
    let dm = admin.create_dm(&user.username);
    admin.post_message(&dm, "hello");
    let (channel, _) = admin.create_channel(&[&user.username]);
    let app = common::launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &dm);
    gui.wait_until_visible(&room(&channel, room_row::NAME), TIMEOUT)
        .unwrap();

    open_menu(gui, &dm);

    assert!(
        gui.exists_and_visible(main_window::ROOM_MARK_READ_ACTION)
            .unwrap()
    );
    assert!(
        !gui.exists_and_visible(main_window::ROOM_LEAVE_ACTION)
            .unwrap()
    );
    close_menu(gui);

    // A channel's has it.
    open_menu(gui, &channel);
    assert!(
        gui.exists_and_visible(main_window::ROOM_LEAVE_ACTION)
            .unwrap()
    );
}

// ROOM-007
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn the_last_owner_leaving_shows_the_servers_refusal_until_dismissed() {
    let admin = Admin::login();
    let user = admin.create_user();
    // The user creates the group, so is its only owner.
    let (group, _) = Admin::login_as(&user.username, &user.password).create_group(&[]);
    let app = common::launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &group);
    assert!(
        !gui.exists_and_visible(main_window::ROOM_LIST_ERROR)
            .unwrap()
    );

    choose(gui, &group, main_window::ROOM_LEAVE_ACTION);
    gui.wait_until_visible(main_window::ROOM_LEAVE_CONFIRM, TIMEOUT)
        .unwrap();
    gui.mouse_click(main_window::ROOM_LEAVE_CONFIRM).unwrap();

    gui.wait_until_visible(main_window::ROOM_LIST_ERROR, TIMEOUT)
        .unwrap();
    let text = gui.text(main_window::ROOM_LIST_ERROR_TEXT).unwrap();
    assert!(text.contains("You are the last owner"), "{text}");
    // The room stays.
    assert!(
        gui.exists_and_visible(&room(&group, room_row::NAME))
            .unwrap()
    );

    gui.mouse_click(main_window::ROOM_LIST_ERROR_DISMISS)
        .unwrap();
    gui.wait_until_hidden(main_window::ROOM_LIST_ERROR, TIMEOUT)
        .unwrap();

    // The next command clears an error too.
    choose(gui, &group, main_window::ROOM_LEAVE_ACTION);
    gui.wait_until_visible(main_window::ROOM_LEAVE_CONFIRM, TIMEOUT)
        .unwrap();
    gui.mouse_click(main_window::ROOM_LEAVE_CONFIRM).unwrap();
    gui.wait_until_visible(main_window::ROOM_LIST_ERROR, TIMEOUT)
        .unwrap();
    choose(gui, &group, main_window::ROOM_FAVORITE_ACTION);
    gui.wait_until_hidden(main_window::ROOM_LIST_ERROR, TIMEOUT)
        .unwrap();
    // Not a command that fails: favorites are fine for the last owner.
    gui.wait_until_visible(&room(&group, room_row::FAVORITE), TIMEOUT)
        .unwrap();
}
