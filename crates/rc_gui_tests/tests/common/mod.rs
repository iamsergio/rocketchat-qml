// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Steps shared by the GUI test files. Each test creates its own user and rooms with unique names
//! through `rc_test_server` and only looks at those, so the tests hold on a dirty server, in any
//! order and in parallel. Server settings are never changed.

// Not every test file uses every item, and a failed step should just fail the test.
#![allow(dead_code, clippy::unwrap_used)]

use std::time::Duration;

use rc_gui_tests::paths::{main_window, room, room_row};
use rc_gui_tests::{App, Gui, Key, Modifiers};
use rc_test_server::{TestUser, server_url};

/// The core works asynchronously, against a real server: tests wait this long for the UI to catch
/// up with it before they fail.
pub const TIMEOUT: Duration = Duration::from_secs(15);

/// Fills the login page and clicks the button. Returns without waiting for the outcome.
pub fn log_in(gui: &Gui, server: &str, username: &str, password: &str) {
    // The login page shows once the core has said that there is no session to resume.
    gui.wait_until_visible(main_window::LOGIN_BUTTON, TIMEOUT)
        .unwrap();
    gui.set_text(main_window::LOGIN_SERVER_FIELD, server)
        .unwrap();
    gui.set_text(main_window::LOGIN_USERNAME_FIELD, username)
        .unwrap();
    gui.set_text(main_window::LOGIN_PASSWORD_FIELD, password)
        .unwrap();
    gui.mouse_click(main_window::LOGIN_BUTTON).unwrap();
}

/// Starts the app with the time zone set to UTC, so that the times it shows are the ones the
/// server stored.
pub fn launch() -> App {
    App::launch_with(&[("TZ", "UTC")]).unwrap()
}

/// Presses Home in the message list.
pub fn press_home(gui: &Gui) {
    gui.enter_key(main_window::MESSAGE_LIST, Key::HOME, Modifiers::NONE)
        .unwrap();
}

/// Clicks the row of the room with this id, which shows its messages.
pub fn open_room(gui: &Gui, room_id: &str) {
    gui.mouse_click(&room(room_id, room_row::NAME)).unwrap();
}

/// The time of day (`HH:MM`) of a timestamp in the server's spelling (`2026-10-20T14:12:48.587Z`).
/// The tests launch the app with `TZ=UTC`, which is what the messages are shown in.
pub fn utc_time(ts: &str) -> String {
    ts.get(11..16).unwrap().to_owned()
}

/// Logs in as `user` and waits until the row of the room with this id is there.
pub fn log_in_and_wait_for_room(gui: &Gui, user: &TestUser, room_id: &str) {
    log_in(gui, &server_url(), &user.username, &user.password);
    gui.wait_until_visible(&room(room_id, room_row::NAME), TIMEOUT)
        .unwrap();
}
