// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Several accounts in one window, against the local test server (`RC_TEST_SERVER_URL`). The second
//! server is the same one behind a `Proxy`: another address, so the same kind of user can be added
//! twice. Each test makes its own users and rooms.

// The helpers fail the test when a step fails.
#![allow(clippy::unwrap_used)]

mod common;

use rc_gui_tests::paths::{
    main_window, room, room_row, server_tile,
    server_tile_part::{EXPIRED, MENTIONS, MONOGRAM, UNREAD},
};
use rc_gui_tests::{App, Gui};
use rc_test_server::{Admin, Proxy, TestUser, server_url};

use common::{SessionFile, TIMEOUT, launch, log_in, log_in_at_and_wait_for_room, open_room};

/// A user with a channel of their own, which nobody else is in.
fn user_with_channel(admin: &Admin) -> (TestUser, String) {
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    (user, channel_id)
}

/// The letter of the tile of the server at `url`: the first of its host, upper case, after a
/// leading `chat.`, `rc.`, `open.` or `www.` (the rule `rc_qt` has: it can't be linked here).
fn monogram_of(url: &str) -> String {
    let host = url.split_once("://").map_or(url, |(_, rest)| rest);
    let host = ["chat.", "rc.", "open.", "www."]
        .iter()
        .find_map(|prefix| host.strip_prefix(prefix))
        .unwrap_or(host);
    host.chars().next().unwrap().to_uppercase().to_string()
}

fn room_name(room_id: &str) -> String {
    room(room_id, room_row::NAME)
}

/// Logs `a` in at the test server, presses + and logs `b` in through `proxy`. `b` is on screen.
fn log_in_both(gui: &Gui, proxy: &Proxy, a: &(TestUser, String), b: &(TestUser, String)) {
    log_in_at_and_wait_for_room(gui, &server_url(), &a.0, &a.1);
    gui.mouse_click(main_window::ADD_SERVER_BUTTON).unwrap();
    log_in_at_and_wait_for_room(gui, &proxy.url(), &b.0, &b.1);
}

fn wait_until_checked(gui: &Gui, path: &str, checked: bool) {
    gui.wait_until(&format!("{path} checked: {checked}"), TIMEOUT, || {
        Ok(gui.checked(path)? == checked)
    })
    .unwrap();
}

// ACCT-001
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_second_server_is_added_from_the_rail_and_adding_can_be_cancelled() {
    let admin = Admin::login();
    let a = user_with_channel(&admin);
    let b = user_with_channel(&admin);
    let proxy = Proxy::start();
    let app = launch();
    let gui = app.gui();

    log_in_at_and_wait_for_room(gui, &server_url(), &a.0, &a.1);
    // The rail is there with one server: its letter, and on screen.
    gui.wait_until_visible(&server_tile(0, ""), TIMEOUT)
        .unwrap();
    gui.wait_until_visible(main_window::SERVER_RAIL, TIMEOUT)
        .unwrap();
    assert_eq!(
        gui.text(&server_tile(0, MONOGRAM)).unwrap(),
        monogram_of(&server_url())
    );
    assert!(gui.checked(&server_tile(0, "")).unwrap());
    assert!(!gui.exists_and_visible(&server_tile(1, "")).unwrap());

    // + shows the login page for a new server, with a tile holding its place.
    gui.mouse_click(main_window::ADD_SERVER_BUTTON).unwrap();
    gui.wait_until_visible(main_window::LOGIN_CANCEL_BUTTON, TIMEOUT)
        .unwrap();
    assert_eq!(gui.text(main_window::LOGIN_TITLE).unwrap(), "Add a server");
    assert!(gui.exists_and_visible(main_window::LOGIN_LEDE).unwrap());
    assert!(gui.exists_and_visible(&server_tile(1, "")).unwrap());
    assert!(!gui.exists_and_visible(&server_tile(1, MONOGRAM)).unwrap());
    // The new server's tile is where to go for it: no second plus.
    gui.wait_until_hidden(main_window::ADD_SERVER_BUTTON, TIMEOUT)
        .unwrap();

    // Cancel goes back to the rooms of the first one.
    gui.mouse_click(main_window::LOGIN_CANCEL_BUTTON).unwrap();
    gui.wait_until_visible(&room_name(&a.1), TIMEOUT).unwrap();
    gui.wait_until_hidden(&server_tile(1, ""), TIMEOUT).unwrap();
    assert!(!gui.exists_and_visible(main_window::LOGIN_BUTTON).unwrap());

    // + again, and this time log in, through another address.
    gui.mouse_click(main_window::ADD_SERVER_BUTTON).unwrap();
    log_in_at_and_wait_for_room(gui, &proxy.url(), &b.0, &b.1);
    assert!(!gui.exists_and_visible(&room_name(&a.1)).unwrap());
    gui.wait_until_text(
        &server_tile(1, MONOGRAM),
        &monogram_of(&proxy.url()),
        TIMEOUT,
    )
    .unwrap();
    wait_until_checked(gui, &server_tile(1, ""), true);
    assert!(!gui.checked(&server_tile(0, "")).unwrap());
    assert!(!gui.exists_and_visible(&server_tile(2, "")).unwrap());
    let host = gui.text(main_window::SERVER_NAME).unwrap();
    assert_eq!(host, proxy.url().trim_start_matches("http://"));
    assert_eq!(gui.text(main_window::SERVER_USER).unwrap(), b.0.username);
}

// ACCT-003
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn clicking_a_tile_shows_the_rooms_of_that_account() {
    let admin = Admin::login();
    let a = user_with_channel(&admin);
    let b = user_with_channel(&admin);
    let proxy = Proxy::start();
    let app = launch();
    let gui = app.gui();
    log_in_both(gui, &proxy, &a, &b);
    assert!(!gui.exists_and_visible(&room_name(&a.1)).unwrap());

    gui.mouse_click(&server_tile(0, "")).unwrap();
    gui.wait_until_visible(&room_name(&a.1), TIMEOUT).unwrap();
    gui.wait_until_hidden(&room_name(&b.1), TIMEOUT).unwrap();
    wait_until_checked(gui, &server_tile(0, ""), true);
    assert!(!gui.checked(&server_tile(1, "")).unwrap());
    let current = gui.text(main_window::CURRENT_USER).unwrap();
    assert!(current.contains(&a.0.username), "{current:?}");

    gui.mouse_click(&server_tile(1, "")).unwrap();
    gui.wait_until_visible(&room_name(&b.1), TIMEOUT).unwrap();
    gui.wait_until_hidden(&room_name(&a.1), TIMEOUT).unwrap();
    wait_until_checked(gui, &server_tile(1, ""), true);
    let current = gui.text(main_window::CURRENT_USER).unwrap();
    assert!(current.contains(&b.0.username), "{current:?}");
}

// ACCT-004
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_mention_in_the_background_shows_on_the_tile_until_the_room_is_opened() {
    let admin = Admin::login();
    let a = user_with_channel(&admin);
    let b = user_with_channel(&admin);
    let proxy = Proxy::start();
    let app = launch();
    let gui = app.gui();
    log_in_both(gui, &proxy, &a, &b);

    // B is on screen; someone mentions A in a room of A's. The server may count the invitation to
    // the room as a mention already: only the difference is A's.
    let mentions = |gui: &Gui| -> u32 {
        if gui.exists_and_visible(&server_tile(0, MENTIONS)).unwrap() {
            gui.text(&server_tile(0, MENTIONS))
                .unwrap()
                .parse()
                .unwrap()
        } else {
            0
        }
    };
    let before = mentions(gui);
    admin.post_message(&a.1, &format!("@{} are you there?", a.0.username));

    gui.wait_until_text(
        &server_tile(0, MENTIONS),
        &(before + 1).to_string(),
        TIMEOUT,
    )
    .unwrap();
    assert!(gui.exists_and_visible(&server_tile(0, UNREAD)).unwrap());

    // Opening the room reads it: the tile loses the mentions that the room showed.
    gui.mouse_click(&server_tile(0, "")).unwrap();
    gui.wait_until_visible(&room(&a.1, room_row::MENTIONS), TIMEOUT)
        .unwrap();
    let in_room: u32 = gui
        .text(&room(&a.1, room_row::MENTIONS))
        .unwrap()
        .parse()
        .unwrap();
    let on_tile = mentions(gui);
    open_room(gui, &a.1);
    gui.wait_until_hidden(&room(&a.1, room_row::MENTIONS), TIMEOUT)
        .unwrap();
    gui.wait_until(
        "the room's mentions are counted off the tile",
        TIMEOUT,
        || Ok(mentions(gui) == on_tile - in_room),
    )
    .unwrap();
}

// ACCT-005
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn logging_out_removes_the_tile_and_shows_the_next_account() {
    let admin = Admin::login();
    let a = user_with_channel(&admin);
    let b = user_with_channel(&admin);
    let proxy = Proxy::start();
    let app = launch();
    let gui = app.gui();
    log_in_both(gui, &proxy, &a, &b);

    // Out of B, who is on screen: A is back.
    gui.mouse_click(main_window::LOGOUT_BUTTON).unwrap();
    gui.wait_until_visible(&room_name(&a.1), TIMEOUT).unwrap();
    gui.wait_until_hidden(&server_tile(1, ""), TIMEOUT).unwrap();
    wait_until_checked(gui, &server_tile(0, ""), true);
    assert!(!gui.exists_and_visible(main_window::LOGIN_BUTTON).unwrap());

    // Out of A, the last one: a login page for a new account, with a tile holding its place.
    gui.mouse_click(main_window::LOGOUT_BUTTON).unwrap();
    gui.wait_until_visible(main_window::LOGIN_BUTTON, TIMEOUT)
        .unwrap();
    gui.wait_until_text(main_window::LOGIN_TITLE, "Log in to Rocket.Chat", TIMEOUT)
        .unwrap();
    assert!(
        !gui.exists_and_visible(main_window::LOGIN_CANCEL_BUTTON)
            .unwrap()
    );
    // The logged-out account leaves the rail when the core has forgotten it.
    gui.wait_until_hidden(&server_tile(0, MONOGRAM), TIMEOUT)
        .unwrap();
    assert!(gui.exists_and_visible(&server_tile(0, "")).unwrap());
    assert!(!gui.exists_and_visible(&server_tile(1, "")).unwrap());
    assert!(!gui.exists_and_visible(&room_name(&a.1)).unwrap());
}

// ACCT-007
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn adding_the_same_account_again_is_refused() {
    let admin = Admin::login();
    let a = user_with_channel(&admin);
    let app = launch();
    let gui = app.gui();
    log_in_at_and_wait_for_room(gui, &server_url(), &a.0, &a.1);

    gui.mouse_click(main_window::ADD_SERVER_BUTTON).unwrap();
    log_in(gui, &server_url(), &a.0.username, &a.0.password);

    gui.wait_until_text(
        main_window::LOGIN_ERROR,
        "You have added that account already",
        TIMEOUT,
    )
    .unwrap();
    assert_eq!(gui.text(main_window::LOGIN_TITLE).unwrap(), "Add a server");
    // Still just the first one and the new one.
    assert!(gui.exists_and_visible(&server_tile(1, "")).unwrap());
    assert!(!gui.exists_and_visible(&server_tile(2, "")).unwrap());
    // Cancel leaves the first one as it was.
    gui.mouse_click(main_window::LOGIN_CANCEL_BUTTON).unwrap();
    gui.wait_until_visible(&room_name(&a.1), TIMEOUT).unwrap();
}

// ACCT-002
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn every_account_comes_back_after_a_restart() {
    let admin = Admin::login();
    let a = user_with_channel(&admin);
    let b = user_with_channel(&admin);
    let proxy = Proxy::start();
    let session_file = SessionFile::new();
    let store = session_file.store();

    let first = App::launch_with(&[("RC_SECRET_STORE", &store), ("TZ", "UTC")]).unwrap();
    log_in_both(first.gui(), &proxy, &a, &b);
    // Exiting by itself lets the app finish storing the sessions.
    let status = first.quit_and_wait().unwrap();
    assert!(status.success(), "{status}");

    let second = App::launch_with(&[("RC_SECRET_STORE", &store), ("TZ", "UTC")]).unwrap();
    let gui = second.gui();
    // The first account is on screen, and both are in the rail, with no login.
    gui.wait_until_visible(&room_name(&a.1), TIMEOUT).unwrap();
    gui.wait_until_visible(&server_tile(1, ""), TIMEOUT)
        .unwrap();
    assert!(gui.checked(&server_tile(0, "")).unwrap());
    assert!(!gui.exists_and_visible(&room_name(&b.1)).unwrap());
    assert!(!gui.exists_and_visible(main_window::LOGIN_BUTTON).unwrap());
    assert!(!gui.exists_and_visible(&server_tile(2, "")).unwrap());
    gui.wait_until_text(
        &server_tile(1, MONOGRAM),
        &monogram_of(&proxy.url()),
        TIMEOUT,
    )
    .unwrap();
    let current = gui.text(main_window::CURRENT_USER).unwrap();
    assert!(current.contains(&a.0.username), "{current:?}");

    // The other one resumed in the background, so it is there at once.
    gui.mouse_click(&server_tile(1, "")).unwrap();
    gui.wait_until_visible(&room_name(&b.1), TIMEOUT).unwrap();
    gui.wait_until_hidden(&room_name(&a.1), TIMEOUT).unwrap();
    assert!(!gui.exists_and_visible(&server_tile(1, EXPIRED)).unwrap());
}

/// A on the test server and B through `proxy`, with B's session ended: its token is refused, and
/// the dropped connection makes the app notice. A is on screen.
fn log_in_and_expire_b(gui: &Gui, proxy: &Proxy, a: &(TestUser, String), b: &(TestUser, String)) {
    log_in_both(gui, proxy, a, b);
    gui.mouse_click(&server_tile(0, "")).unwrap();
    gui.wait_until_visible(&room_name(&a.1), TIMEOUT).unwrap();
    Admin::login_as(&b.0.username, &b.0.password).logout_other_clients();
    proxy.drop_connections();
    gui.wait_until_visible(&server_tile(1, EXPIRED), TIMEOUT)
        .unwrap();
}

// ACCT-006
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn an_expired_account_logs_in_again_with_its_server_and_username_filled_in() {
    let admin = Admin::login();
    let a = user_with_channel(&admin);
    let b = user_with_channel(&admin);
    let proxy = Proxy::start();
    let app = launch();
    let gui = app.gui();
    log_in_and_expire_b(gui, &proxy, &a, &b);
    // A is not affected.
    assert!(!gui.exists_and_visible(&server_tile(0, EXPIRED)).unwrap());
    assert!(gui.exists_and_visible(&room_name(&a.1)).unwrap());

    gui.mouse_click(&server_tile(1, "")).unwrap();
    gui.wait_until_text(main_window::LOGIN_TITLE, "Log in again", TIMEOUT)
        .unwrap();
    let notice = gui.text(main_window::LOGIN_NOTICE).unwrap();
    assert!(
        notice.contains(proxy.url().trim_start_matches("http://")),
        "{notice:?}"
    );
    assert_eq!(
        gui.text(main_window::LOGIN_SERVER_FIELD).unwrap(),
        proxy.url()
    );
    assert_eq!(
        gui.text(main_window::LOGIN_USERNAME_FIELD).unwrap(),
        b.0.username
    );
    assert_eq!(gui.text(main_window::LOGIN_PASSWORD_FIELD).unwrap(), "");
    assert!(
        gui.exists_and_visible(main_window::LOGIN_REMOVE_SERVER_BUTTON)
            .unwrap()
    );
    assert!(
        !gui.exists_and_visible(main_window::LOGIN_CANCEL_BUTTON)
            .unwrap()
    );
    assert!(!gui.exists_and_visible(main_window::LOGIN_ERROR).unwrap());

    // The password is all it takes.
    gui.set_text(main_window::LOGIN_PASSWORD_FIELD, &b.0.password)
        .unwrap();
    gui.mouse_click(main_window::LOGIN_BUTTON).unwrap();
    gui.wait_until_visible(&room_name(&b.1), TIMEOUT).unwrap();
    gui.wait_until_hidden(&server_tile(1, EXPIRED), TIMEOUT)
        .unwrap();
    wait_until_checked(gui, &server_tile(1, ""), true);
}

// ACCT-006
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn an_expired_server_can_be_removed() {
    let admin = Admin::login();
    let a = user_with_channel(&admin);
    let b = user_with_channel(&admin);
    let proxy = Proxy::start();
    let app = launch();
    let gui = app.gui();
    log_in_and_expire_b(gui, &proxy, &a, &b);
    gui.mouse_click(&server_tile(1, "")).unwrap();
    gui.wait_until_visible(main_window::LOGIN_REMOVE_SERVER_BUTTON, TIMEOUT)
        .unwrap();

    gui.mouse_click(main_window::LOGIN_REMOVE_SERVER_BUTTON)
        .unwrap();

    gui.wait_until_hidden(&server_tile(1, ""), TIMEOUT).unwrap();
    gui.wait_until_visible(&room_name(&a.1), TIMEOUT).unwrap();
    wait_until_checked(gui, &server_tile(0, ""), true);
}

// ACCT-006
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn another_accounts_login_page_starts_empty_whatever_was_typed_on_this_one() {
    let admin = Admin::login();
    let a = user_with_channel(&admin);
    let b = user_with_channel(&admin);
    let proxy = Proxy::start();
    let app = launch();
    let gui = app.gui();
    log_in_and_expire_b(gui, &proxy, &a, &b);

    // On a new account's login page, with a password typed for another server.
    gui.mouse_click(main_window::ADD_SERVER_BUTTON).unwrap();
    gui.wait_until_text(main_window::LOGIN_TITLE, "Add a server", TIMEOUT)
        .unwrap();
    gui.set_text(main_window::LOGIN_SERVER_FIELD, "http://typed.example")
        .unwrap();
    gui.set_text(main_window::LOGIN_USERNAME_FIELD, "someone")
        .unwrap();
    gui.set_text(main_window::LOGIN_PASSWORD_FIELD, "typed-secret")
        .unwrap();

    // The expired account's page shows its own server and username, and no password.
    gui.mouse_click(&server_tile(1, "")).unwrap();
    gui.wait_until_text(main_window::LOGIN_TITLE, "Log in again", TIMEOUT)
        .unwrap();
    gui.wait_until_text(main_window::LOGIN_SERVER_FIELD, &proxy.url(), TIMEOUT)
        .unwrap();
    assert_eq!(
        gui.text(main_window::LOGIN_USERNAME_FIELD).unwrap(),
        b.0.username
    );
    assert_eq!(gui.text(main_window::LOGIN_PASSWORD_FIELD).unwrap(), "");

    // And back on the new account's page nothing is left of it either.
    gui.mouse_click(&server_tile(2, "")).unwrap();
    gui.wait_until_text(main_window::LOGIN_TITLE, "Add a server", TIMEOUT)
        .unwrap();
    assert_eq!(gui.text(main_window::LOGIN_SERVER_FIELD).unwrap(), "");
    assert_eq!(gui.text(main_window::LOGIN_PASSWORD_FIELD).unwrap(), "");
}

// ACCT-002
// ACCT-006
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn an_account_stored_without_a_token_starts_expired_after_a_restart() {
    let admin = Admin::login();
    let a = user_with_channel(&admin);
    let b = user_with_channel(&admin);
    let proxy = Proxy::start();
    let session_file = SessionFile::new();
    let store = session_file.store();

    let first = App::launch_with(&[("RC_SECRET_STORE", &store), ("TZ", "UTC")]).unwrap();
    log_in_and_expire_b(first.gui(), &proxy, &a, &b);
    // Exiting by itself lets the app finish storing the account without its token.
    let status = first.quit_and_wait().unwrap();
    assert!(status.success(), "{status}");

    let second = App::launch_with(&[("RC_SECRET_STORE", &store), ("TZ", "UTC")]).unwrap();
    let gui = second.gui();
    gui.wait_until_visible(&room_name(&a.1), TIMEOUT).unwrap();
    gui.wait_until_visible(&server_tile(1, EXPIRED), TIMEOUT)
        .unwrap();
    assert!(!gui.exists_and_visible(&server_tile(0, EXPIRED)).unwrap());
    gui.mouse_click(&server_tile(1, "")).unwrap();
    gui.wait_until_text(main_window::LOGIN_TITLE, "Log in again", TIMEOUT)
        .unwrap();
    assert_eq!(
        gui.text(main_window::LOGIN_USERNAME_FIELD).unwrap(),
        b.0.username
    );
}
