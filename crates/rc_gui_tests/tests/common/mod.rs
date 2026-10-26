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
use std::{env, fs};

use rc_gui_tests::paths::{
    emoji_pick, main_window, message, message_row, room, room_row, thread_message,
};
use rc_gui_tests::{App, Gui, Key, Modifiers, MouseButton};
use rc_test_server::{Admin, Posted, TestUser, server_url, unique};

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

/// Logs out of the account of the tile at `tile` (see `server_tile`): opens its menu with a
/// right-click and chooses the item.
pub fn log_out_of(gui: &Gui, tile: &str) {
    gui.mouse_click_with(tile, MouseButton::Right, Modifiers::NONE)
        .unwrap();
    gui.wait_until_visible(main_window::LOGOUT_ACTION, TIMEOUT)
        .unwrap();
    gui.mouse_click(main_window::LOGOUT_ACTION).unwrap();
}

/// Logs out of the only account and waits for the login page of the new one that takes its place.
/// The logged-out account shows its own login page first, with its server filled in, until the
/// core has forgotten it: what is typed there is lost when the new account's empty page replaces
/// it.
pub fn log_out_of_the_last(gui: &Gui) {
    log_out_of(gui, &rc_gui_tests::paths::server_tile(0, ""));
    gui.wait_until_text(main_window::LOGIN_SERVER_FIELD, "", TIMEOUT)
        .unwrap();
}

/// Starts the app with the time zone set to UTC, so that the times it shows are the ones the
/// server stored.
pub fn launch() -> App {
    App::launch_with(&[("TZ", "UTC")]).unwrap()
}

/// Starts the app like [`launch`], with the files the paperclip of a composer attaches: the apps
/// take the paths of `RC_TEST_ATTACH_FILES` instead of opening a file dialog, which Spix can't
/// drive.
pub fn launch_attaching(files: &[&std::path::Path]) -> App {
    let paths = env::join_paths(files).unwrap();
    App::launch_with(&[
        ("TZ", "UTC"),
        ("RC_TEST_ATTACH_FILES", paths.to_str().unwrap()),
    ])
    .unwrap()
}

/// The `count` messages of the channel that come after the one with id `before`, oldest first,
/// once the server has them all: how a test learns the ids of what the app uploaded, which the
/// server picked.
pub fn wait_for_messages_after(
    admin: &Admin,
    channel_id: &str,
    before: &str,
    count: usize,
) -> Vec<Posted> {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let mut newest = admin.last_messages(channel_id, count + 1);
        if newest.len() == count + 1 && newest[count].id == before {
            newest.truncate(count);
            newest.reverse();
            return newest;
        }
        assert!(
            Instant::now() < deadline,
            "the {count} messages never reached the server"
        );
        thread::sleep(Duration::from_millis(100));
    }
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

/// Whether the room shows as unread (its name is emphasised): its menu offers to mark it read. A
/// channel with news that is not a mention shows no number, so the menu is what tells.
pub fn shows_unread(gui: &Gui, room_id: &str) -> bool {
    gui.mouse_click_with(
        &format!("{}/room_{room_id}", main_window::WINDOW),
        MouseButton::Right,
        Modifiers::NONE,
    )
    .unwrap();
    gui.wait_until_visible(main_window::ROOM_FAVORITE_ACTION, TIMEOUT)
        .unwrap();
    let unread = gui
        .exists_and_visible(main_window::ROOM_MARK_READ_ACTION)
        .unwrap();
    gui.enter_key(main_window::ROOM_LIST, Key::ESCAPE, Modifiers::NONE)
        .unwrap();
    gui.wait_until_hidden(main_window::ROOM_FAVORITE_ACTION, TIMEOUT)
        .unwrap();
    unread
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

/// Opens the menu of the message with its button and chooses the item at `action`.
pub fn choose(gui: &Gui, message_id: &str, action: &str) {
    gui.mouse_click(&message(message_id, message_row::MENU_BUTTON))
        .unwrap();
    gui.wait_until_visible(action, TIMEOUT).unwrap();
    gui.mouse_click(action).unwrap();
}

/// In the open emoji picker, searches for `name` and clicks the emoji of that shortcode (without
/// colons), which closes the picker.
pub fn pick_emoji(gui: &Gui, name: &str) {
    gui.wait_until_visible(main_window::EMOJI_SEARCH, TIMEOUT)
        .unwrap();
    gui.set_text(main_window::EMOJI_SEARCH, name).unwrap();
    gui.wait_until_visible(&emoji_pick(name), TIMEOUT).unwrap();
    gui.mouse_click(&emoji_pick(name)).unwrap();
    gui.wait_until_hidden(main_window::EMOJI_SEARCH, TIMEOUT)
        .unwrap();
}

/// Types `text` over what the composer holds and presses Enter.
pub fn replace_text_and_press_enter(gui: &Gui, text: &str) {
    gui.mouse_click(main_window::COMPOSER_FIELD).unwrap();
    gui.set_text(main_window::COMPOSER_FIELD, text).unwrap();
    gui.enter_key(main_window::COMPOSER_FIELD, Key::RETURN, Modifiers::NONE)
        .unwrap();
}

/// Posts a message to the channel as `user` themselves, so that it is theirs to edit and delete.
pub fn post_as(user: &TestUser, channel_id: &str, text: &str) -> Posted {
    Admin::login_as(&user.username, &user.password).post_message(channel_id, text)
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

/// Waits until the message is shown as sent, which is when the server's copy has replaced the one
/// the app showed at once.
pub fn wait_until_sent(gui: &Gui) {
    gui.wait_until_hidden(main_window::UNSENT_PENDING, TIMEOUT)
        .unwrap();
}

/// Presses Escape in the composer.
pub fn escape(gui: &Gui) {
    gui.enter_key(main_window::COMPOSER_FIELD, Key::ESCAPE, Modifiers::NONE)
        .unwrap();
}

/// Opens the menu of the message with this id in the thread pane with its button and chooses the
/// item at `action`.
pub fn choose_in_thread(gui: &Gui, message_id: &str, action: &str) {
    gui.mouse_click(&thread_message(message_id, message_row::MENU_BUTTON))
        .unwrap();
    gui.wait_until_visible(action, TIMEOUT).unwrap();
    gui.mouse_click(action).unwrap();
}

/// Types `text` in the composer of the thread pane and presses Enter.
pub fn type_and_send_in_thread(gui: &Gui, text: &str) {
    gui.mouse_click(main_window::THREAD_COMPOSER_FIELD).unwrap();
    gui.set_text(main_window::THREAD_COMPOSER_FIELD, text)
        .unwrap();
    gui.enter_key(
        main_window::THREAD_COMPOSER_FIELD,
        Key::RETURN,
        Modifiers::NONE,
    )
    .unwrap();
}

/// The newest reply in the thread once it is not `before` any more (`None` for the first reply):
/// how a test learns the id of a reply the app sent, which the app picked, and which the channel's
/// history does not list unless it was also sent to the channel.
pub fn wait_for_reply_after(admin: &Admin, thread_id: &str, before: Option<&str>) -> Posted {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if let Some(last) = admin.last_in_thread(thread_id)
            && Some(last.id.as_str()) != before
        {
            return last;
        }
        assert!(
            Instant::now() < deadline,
            "the reply never reached the server"
        );
        thread::sleep(Duration::from_millis(100));
    }
}

/// Deletes the session file even if an assertion fails.
pub struct SessionFile(PathBuf);

impl SessionFile {
    pub fn new() -> Self {
        Self(env::temp_dir().join(format!("rc-gui-session-{}.json", unique())))
    }

    /// The `RC_SECRET_STORE` value for it.
    pub fn store(&self) -> String {
        format!("file:{}", self.0.display())
    }
}

impl Drop for SessionFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
