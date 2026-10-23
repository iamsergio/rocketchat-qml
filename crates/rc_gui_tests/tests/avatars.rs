// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Avatars of the authors and the rooms, against the local test server (`RC_TEST_SERVER_URL`). The
//! app keeps its cache in a directory of its own (see `App::cache_dir`), which starts empty. The
//! avatar's `text` is the `file://` URL of the image it shows; without an avatar of its own a user
//! or room shows the initials of its name instead.
//!
//! The test server accepts uploaded avatars (a one-pixel PNG) and says who has one, so the file in
//! the cache is that PNG; what the image shows is not looked at, only that it is loaded.

// Helpers outside the tests: a failed step should just fail the test.
#![allow(clippy::unwrap_used, clippy::panic)]

mod common;

use std::path::{Path, PathBuf};

use rc_gui_tests::paths::{message, message_row, room, room_row};
use rc_gui_tests::{App, Gui};
use rc_test_server::{Admin, TestUser};

use common::{TIMEOUT, file_of, launch, log_in_and_wait_for_room, open_room};

/// Waits until the avatar at `path` shows its image, and returns the file it shows.
fn loaded_avatar(gui: &Gui, path: &str) -> PathBuf {
    gui.wait_until_visible(path, TIMEOUT).unwrap();
    file_of(&gui.text(path).unwrap())
}

fn assert_cached_image(file: &Path, app: &App, name: &str) {
    assert!(
        file.starts_with(app.cache_dir()),
        "{} is not in {}",
        file.display(),
        app.cache_dir().display()
    );
    assert_eq!(file.file_stem().unwrap().to_str().unwrap(), name);
    assert_eq!(file.extension().unwrap(), "png");
    assert!(std::fs::metadata(file).unwrap().len() > 0);
}

/// The initials the placeholder at `path` shows, once it does.
fn initials_of(gui: &Gui, path: &str) -> String {
    gui.wait_until_visible(path, TIMEOUT).unwrap();
    gui.text(path).unwrap()
}

/// The initials of the display name `Admin::create_user` gives: "Contract 1234-5-6" is "C1".
fn initials_of_test_user(user: &TestUser) -> String {
    let words: Vec<&str> = user.name.split_whitespace().collect();
    assert_eq!(words[0], "Contract", "{}", user.name);
    format!("C{}", words[1].chars().next().unwrap())
}

// RICH-004
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn an_author_with_an_avatar_of_their_own_shows_it_as_a_file_in_the_cache() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    admin.set_avatar(&user.username);
    let before = admin.post_message(&channel_id, "first");
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    open_room(gui, &channel_id);
    common::type_and_send(gui, "who am I");
    let mine = common::wait_for_message_after(&admin, &channel_id, &before.id);

    let file = loaded_avatar(gui, &message(&mine.id, message_row::AVATAR));

    assert_cached_image(&file, &app, &user.username);
    // Once the image is there the placeholder is gone.
    gui.wait_until_hidden(&message(&mine.id, message_row::AVATAR_PLACEHOLDER), TIMEOUT)
        .unwrap();
}

// RICH-004
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn an_author_without_one_shows_their_initials_on_a_colour() {
    let admin = Admin::login();
    let user = admin.create_user();
    // Someone who has no avatar, whatever the shared server's admin has.
    let author = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username, &author.username]);
    let posted =
        Admin::login_as(&author.username, &author.password).post_message(&channel_id, "who am I");
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);

    open_room(gui, &channel_id);

    let initials = initials_of(gui, &message(&posted.id, message_row::AVATAR_INITIALS));
    assert_eq!(initials, initials_of_test_user(&author));
    // No image is loaded, and none ever comes: the user has none of their own.
    assert!(
        !gui.exists_and_visible(&message(&posted.id, message_row::AVATAR))
            .unwrap()
    );
    assert!(
        gui.exists_and_visible(&message(&posted.id, message_row::AVATAR_PLACEHOLDER))
            .unwrap()
    );
}

// RICH-004
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn the_room_list_shows_an_avatar_for_a_room_that_has_one_and_initials_for_the_others() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (with, _) = admin.create_channel(&[&user.username]);
    let (without, without_name) = admin.create_channel(&[&user.username]);
    admin.set_room_avatar(&with);
    // The other side of the direct message has no avatar, whatever the shared server's admin has.
    let other = admin.create_user();
    let other_side = Admin::login_as(&other.username, &other.password);
    let dm_id = other_side.create_dm(&user.username);
    other_side.post_message(&dm_id, "hello");
    let app = launch();
    let gui = app.gui();

    log_in_and_wait_for_room(gui, &user, &with);

    let file = loaded_avatar(gui, &room(&with, room_row::AVATAR));
    assert_cached_image(&file, &app, &with);
    // The initials of the name, not of the room id: "ct-chan-1234-5-6" is CC.
    assert!(without_name.starts_with("ct-chan-"));
    gui.wait_until_visible(&room(&without, room_row::NAME), TIMEOUT)
        .unwrap();
    let initials = initials_of(gui, &room(&without, room_row::AVATAR_INITIALS));
    assert_eq!(initials, "CC");
    assert!(
        !gui.exists_and_visible(&room(&without, room_row::AVATAR))
            .unwrap()
    );
    // A direct message shows the other user, here by the initial of their name.
    gui.wait_until_visible(&room(&dm_id, room_row::NAME), TIMEOUT)
        .unwrap();
    let dm = initials_of(gui, &room(&dm_id, room_row::AVATAR_INITIALS));
    assert_eq!(dm, initials_of_test_user(&other));
}

// RICH-004
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn the_cache_keeps_the_avatars_for_the_next_session_and_logging_out_keeps_the_files() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    admin.set_room_avatar(&channel_id);
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    let before = loaded_avatar(gui, &room(&channel_id, room_row::AVATAR));

    common::log_out_of(gui, &rc_gui_tests::paths::server_tile(0, ""));
    gui.wait_until_visible(rc_gui_tests::paths::main_window::LOGIN_BUTTON, TIMEOUT)
        .unwrap();

    // Logged out, the file is still there.
    assert!(before.exists());
    common::log_in(
        gui,
        &rc_test_server::server_url(),
        &user.username,
        &user.password,
    );
    gui.wait_until_visible(&room(&channel_id, room_row::NAME), TIMEOUT)
        .unwrap();
    assert_eq!(
        loaded_avatar(gui, &room(&channel_id, room_row::AVATAR)),
        before
    );
}
