// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Steps shared by the GUI test files. Each test creates its own user and rooms through
//! `rc_test_server`, so the tests hold on a dirty server and in parallel.

// Not every test file uses every item, and a failed step should just fail the test.
#![allow(dead_code, clippy::unwrap_used)]

use std::time::Duration;

use rc_gui_tests::Gui;
use rc_gui_tests::paths::{main_window, room, room_row};
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

/// Logs in as `user` and waits until the row of the room with this id is there.
pub fn log_in_and_wait_for_room(gui: &Gui, user: &TestUser, room_id: &str) {
    log_in(gui, &server_url(), &user.username, &user.password);
    gui.wait_until_visible(&room(room_id, room_row::NAME), TIMEOUT)
        .unwrap();
}
