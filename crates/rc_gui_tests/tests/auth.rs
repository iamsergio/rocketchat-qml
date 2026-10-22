// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Authentication flows, against the local test server (`RC_TEST_SERVER_URL`). Each test makes its
//! own user.

mod common;

use rc_gui_tests::App;
use rc_gui_tests::paths::{main_window, server_tile};
use rc_test_server::{Admin, server_url};
use std::net::TcpListener;

use common::{SessionFile, TIMEOUT, log_in, log_out_of};

// AUTH-001
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn logging_in_shows_the_rooms_and_the_user() {
    let user = Admin::login().create_user();
    let app = App::launch().unwrap();
    let gui = app.gui();

    log_in(gui, &server_url(), &user.username, &user.password);

    gui.wait_until_visible(main_window::ROOM_LIST, TIMEOUT)
        .unwrap();
    let current = gui.text(main_window::SERVER_USER).unwrap();
    assert!(current.contains(&user.username), "{current:?}");
    assert!(!gui.exists_and_visible(main_window::LOGIN_BUTTON).unwrap());
}

// AUTH-002
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_wrong_password_shows_an_error_on_the_login_page() {
    let user = Admin::login().create_user();
    let app = App::launch().unwrap();
    let gui = app.gui();

    log_in(gui, &server_url(), &user.username, "not-the-password");

    gui.wait_until_text(
        main_window::LOGIN_ERROR,
        "Wrong username or password",
        TIMEOUT,
    )
    .unwrap();
    assert!(gui.exists_and_visible(main_window::LOGIN_BUTTON).unwrap());
    assert!(!gui.exists_and_visible(main_window::ROOM_LIST).unwrap());
    // The form is usable again.
    assert!(gui.enabled(main_window::LOGIN_BUTTON).unwrap());
}

// AUTH-003
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn an_unreachable_server_shows_an_error() {
    // A port that was free a moment ago.
    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let app = App::launch().unwrap();
    let gui = app.gui();

    log_in(
        gui,
        &format!("http://127.0.0.1:{port}"),
        "someone",
        "secret",
    );

    gui.wait_until_text(main_window::LOGIN_ERROR, "Can't reach the server", TIMEOUT)
        .unwrap();
    assert!(!gui.exists_and_visible(main_window::ROOM_LIST).unwrap());
}

// AUTH-004
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn the_session_resumes_after_a_restart() {
    let user = Admin::login().create_user();
    let session_file = SessionFile::new();
    let store = session_file.store();

    let first = App::launch_with(&[("RC_SECRET_STORE", &store)]).unwrap();
    log_in(first.gui(), &server_url(), &user.username, &user.password);
    first
        .gui()
        .wait_until_visible(main_window::ROOM_LIST, TIMEOUT)
        .unwrap();
    // Exiting by itself lets the app finish storing the session.
    let status = first.quit_and_wait().unwrap();
    assert!(status.success(), "{status}");

    // No login this time.
    let second = App::launch_with(&[("RC_SECRET_STORE", &store)]).unwrap();
    let gui = second.gui();
    // The login page must not show while the session resumes. A poll can miss a short flash, so
    // this never fails on a good build; it catches the regression most of the time.
    gui.wait_until("the room list, without the login page", TIMEOUT, || {
        assert!(!gui.exists_and_visible(main_window::LOGIN_BUTTON)?);
        gui.exists_and_visible(main_window::ROOM_LIST)
    })
    .unwrap();
    let current = gui.text(main_window::SERVER_USER).unwrap();
    assert!(current.contains(&user.username), "{current:?}");
    assert!(!gui.exists_and_visible(main_window::LOGIN_BUTTON).unwrap());
}

// AUTH-005
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn logging_out_returns_to_the_login_page() {
    let user = Admin::login().create_user();
    let app = App::launch().unwrap();
    let gui = app.gui();
    log_in(gui, &server_url(), &user.username, &user.password);
    gui.wait_until_visible(main_window::ROOM_LIST, TIMEOUT)
        .unwrap();

    log_out_of(gui, &server_tile(0, ""));

    gui.wait_until_visible(main_window::LOGIN_BUTTON, TIMEOUT)
        .unwrap();
    assert!(!gui.exists_and_visible(main_window::ROOM_LIST).unwrap());
    // The login page starts empty again.
    assert_eq!(gui.text(main_window::LOGIN_PASSWORD_FIELD).unwrap(), "");
}
