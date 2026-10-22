// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Who is typing, against the local test server (`RC_TEST_SERVER_URL`): another user starts and
//! stops typing in the open room, whose line between the messages and the composer comes and goes,
//! then in the open thread, whose line is the pane's and not the room's. Each test makes its own
//! users and channel and only looks at those.

#![allow(clippy::unwrap_used)]

mod common;

use std::time::{Duration, Instant};

use rc_gui_tests::Gui;
use rc_gui_tests::paths::{main_window, message, message_row};
use rc_test_server::{Admin, Typist, unique};

use common::{TIMEOUT, launch, log_in_and_wait_for_room, open_room};

/// How often the typist says again that they type, until the app has heard it: the app follows a
/// room a moment after it opens it, and what is said before goes unheard.
const RESEND: Duration = Duration::from_millis(300);

/// Waits until the element's `text` is `expected`, calling `start` at once and then every
/// [`RESEND`] meanwhile.
fn wait_for_line_while_starting(gui: &Gui, path: &str, expected: &str, mut start: impl FnMut()) {
    let mut last: Option<Instant> = None;
    gui.wait_until(&format!("{path} to show {expected:?}"), TIMEOUT, || {
        if last.is_none_or(|at| at.elapsed() >= RESEND) {
            start();
            last = Some(Instant::now());
        }
        Ok(gui.text(path)? == expected)
    })
    .unwrap();
}

// ACT-006
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn the_line_of_the_open_room_says_who_types_and_empties_when_they_stop() {
    let admin = Admin::login();
    let user = admin.create_user();
    let typist_user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username, &typist_user.username]);
    let mut typist = Typist::connect(&typist_user);
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    open_room(gui, &channel_id);

    // Nobody types: the line is there, empty, and takes its place.
    gui.wait_until_visible(main_window::ROOM_TYPING, TIMEOUT)
        .unwrap();
    assert_eq!(gui.text(main_window::ROOM_TYPING).unwrap(), "");
    let height = gui.bounding_box(main_window::ROOM_TYPING).unwrap().height;
    assert!(height > 0.0);

    let expected = format!("{} is typing\u{2026}", typist_user.username);
    wait_for_line_while_starting(gui, main_window::ROOM_TYPING, &expected, || {
        typist.start(&channel_id);
    });
    assert!(gui.exists_and_visible(main_window::ROOM_TYPING).unwrap());
    // The thread pane isn't there, and the room's line didn't change size.
    assert!(!gui.exists_and_visible(main_window::THREAD_PANE).unwrap());
    assert!((gui.bounding_box(main_window::ROOM_TYPING).unwrap().height - height).abs() < 0.5);

    typist.stop(&channel_id);
    gui.wait_until_text(main_window::ROOM_TYPING, "", TIMEOUT)
        .unwrap();
    // Empty, but still there, with its height.
    assert!(gui.exists_and_visible(main_window::ROOM_TYPING).unwrap());
    assert!((gui.bounding_box(main_window::ROOM_TYPING).unwrap().height - height).abs() < 0.5);
}

// ACT-006
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn the_line_of_the_open_thread_says_who_types_there_and_not_the_rooms() {
    let admin = Admin::login();
    let user = admin.create_user();
    let typist_user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username, &typist_user.username]);
    let parent = admin.post_message(&channel_id, &format!("parent {}", unique()));
    admin.post_in_thread(
        &channel_id,
        &parent.id,
        &format!("reply {}", unique()),
        false,
    );
    let mut typist = Typist::connect(&typist_user);
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    open_room(gui, &channel_id);
    gui.wait_until_visible(&message(&parent.id, message_row::THREAD_SUMMARY), TIMEOUT)
        .unwrap();
    gui.mouse_click(&message(&parent.id, message_row::THREAD_SUMMARY))
        .unwrap();
    gui.wait_until_visible(main_window::THREAD_PANE, TIMEOUT)
        .unwrap();
    gui.wait_until_visible(main_window::THREAD_TYPING, TIMEOUT)
        .unwrap();
    assert_eq!(gui.text(main_window::THREAD_TYPING).unwrap(), "");

    let expected = format!("{} is typing\u{2026}", typist_user.username);
    wait_for_line_while_starting(gui, main_window::THREAD_TYPING, &expected, || {
        typist.start_in_thread(&channel_id, &parent.id);
    });
    // Typing in the thread isn't typing in the room.
    assert_eq!(gui.text(main_window::ROOM_TYPING).unwrap(), "");

    typist.stop_in_thread(&channel_id, &parent.id);
    gui.wait_until_text(main_window::THREAD_TYPING, "", TIMEOUT)
        .unwrap();
    assert!(gui.exists_and_visible(main_window::THREAD_TYPING).unwrap());
}
