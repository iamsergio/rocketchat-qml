// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! App-level GUI tests.

use rc_gui_tests::App;
use rc_gui_tests::paths::main_window;
use rc_test_server::{Admin, server_url};

mod common;

use common::{TIMEOUT, log_in};

/// The version the app shows, from the same file the app is built from.
const VERSION: &str = include_str!("../../../version.txt");

// APP-001
#[test]
#[ignore = "needs the app: run through ctest"]
fn shows_the_version_from_rust() {
    let app = App::launch().unwrap();
    let text = app.gui().text(main_window::VERSION_LABEL).unwrap();
    assert!(
        text.contains(VERSION.trim_ascii()),
        "version label shows {text:?}"
    );
}

// APP-003
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn dump_info_is_in_the_settings_menu() {
    let user = Admin::login().create_user();
    let app = App::launch().unwrap();
    let gui = app.gui();
    log_in(gui, &server_url(), &user.username, &user.password);
    gui.wait_until_visible(main_window::ROOM_LIST, TIMEOUT)
        .unwrap();

    // The menu is a popup: its items exist only while it is open.
    assert!(
        !gui.exists_and_visible(main_window::DUMP_INFO_ACTION)
            .unwrap()
    );
    gui.mouse_click(main_window::SETTINGS_BUTTON).unwrap();
    gui.wait_until_visible(main_window::DUMP_INFO_ACTION, TIMEOUT)
        .unwrap();

    gui.mouse_click(main_window::DUMP_INFO_ACTION).unwrap();
    gui.wait_until_hidden(main_window::DUMP_INFO_ACTION, TIMEOUT)
        .unwrap();
}
