// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Steps shared by the GUI test files. Each test creates its own user and rooms with unique names
//! through `rc_test_server` and only looks at those, so the tests hold on a dirty server, in any
//! order and in parallel. Server settings are never changed.

// Not every test file uses every item, and a failed step should just fail the test.
#![allow(dead_code, clippy::unwrap_used, clippy::panic)]

use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use rc_gui_tests::paths::{main_window, message, message_row, room, room_row};
use rc_gui_tests::{App, Gui, Key, Modifiers};
use rc_test_server::{Admin, Posted, TestUser, server_url};

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

/// The vertical position of the row of the room with this id, to compare the order of rows.
pub fn row_y(gui: &Gui, room_id: &str) -> f64 {
    gui.bounding_box(&room(room_id, room_row::NAME)).unwrap().y
}

/// The time of day (`HH:MM`) of a timestamp in the server's spelling (`2026-10-20T14:12:48.587Z`).
/// The tests launch the app with `TZ=UTC`, which is what the messages are shown in.
pub fn utc_time(ts: &str) -> String {
    ts.get(11..16).unwrap().to_owned()
}

/// Types `text` in the composer and presses Enter. Clicks the composer first: keys go to the item
/// that has the focus.
pub fn type_and_send(gui: &Gui, text: &str) {
    gui.mouse_click(main_window::COMPOSER_FIELD).unwrap();
    gui.set_text(main_window::COMPOSER_FIELD, text).unwrap();
    gui.enter_key(main_window::COMPOSER_FIELD, Key::RETURN, Modifiers::NONE)
        .unwrap();
}

/// The newest message of the channel once it is not `before` any more: how a test learns the id of
/// a message the app sent, which the app picked.
pub fn wait_for_message_after(admin: &Admin, channel_id: &str, before: &str) -> Posted {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let last = admin.last_message(channel_id);
        if last.id != before {
            return last;
        }
        assert!(
            Instant::now() < deadline,
            "the message never reached the server"
        );
        thread::sleep(Duration::from_millis(100));
    }
}

/// Logs in as `user` and waits until the row of the room with this id is there.
pub fn log_in_and_wait_for_room(gui: &Gui, user: &TestUser, room_id: &str) {
    log_in_at_and_wait_for_room(gui, &server_url(), user, room_id);
}

/// As [`log_in_and_wait_for_room`], through the server at `server`: a proxy to the test server,
/// for instance.
pub fn log_in_at_and_wait_for_room(gui: &Gui, server: &str, user: &TestUser, room_id: &str) {
    log_in(gui, server, &user.username, &user.password);
    gui.wait_until_visible(&room(room_id, room_row::NAME), TIMEOUT)
        .unwrap();
}

/// The rich text (`msgBodyHtml`) of the message once it holds `part`.
pub fn html_containing(gui: &Gui, id: &str, part: &str) -> String {
    let path = message(id, message_row::BODY_HTML);
    gui.wait_until(&format!("{part:?} in the body of {id}"), TIMEOUT, || {
        Ok(gui.text(&path)?.contains(part))
    })
    .unwrap();
    gui.text(&path).unwrap()
}

/// The file a `file://` URL names, without the `?v=` that makes QML load a replaced file again.
pub fn file_of(url: &str) -> PathBuf {
    let path = url
        .strip_prefix("file://")
        .unwrap_or_else(|| panic!("not a file URL: {url}"));
    let path = path.split('?').next().unwrap();
    PathBuf::from(percent_decode(path))
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let hex = bytes
            .get(at + 1..at + 3)
            .and_then(|h| std::str::from_utf8(h).ok());
        match (bytes[at], hex.and_then(|h| u8::from_str_radix(h, 16).ok())) {
            (b'%', Some(byte)) => {
                out.push(byte);
                at += 3;
            }
            (byte, _) => {
                out.push(byte);
                at += 1;
            }
        }
    }
    String::from_utf8(out).unwrap()
}
