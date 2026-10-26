// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Sending files from the composer, against the local test server (`RC_TEST_SERVER_URL`): the
//! paperclip puts files in a bar over the field, where each can be taken out; Send uploads them in
//! order with the text on the first; one the server refuses stays as a failed row that can be
//! retried or removed; and the thread pane's composer uploads into the thread. The apps can't open
//! a file dialog under test, so they take the files of `RC_TEST_ATTACH_FILES` when the paperclip is
//! clicked (see `common::launch_attaching`). Dropping files on a pane is not tested: Spix can't drag.
//! Each test makes its own user and channel and only looks at those.

// A failed step should just fail the test.
#![allow(clippy::unwrap_used)]

mod common;

use rc_gui_tests::paths::{
    attachment, attachment_row, main_window, message, message_row, staged_file, staged_file_part,
    staged_file_remove, thread_message, upload, upload_cancel, upload_part, upload_retry,
};
use rc_test_server::{Admin, PIXEL_PNG, temp_file, unique};

use common::{
    TIMEOUT, choose, launch_attaching, log_in_and_wait_for_room, open_room,
    wait_for_messages_after, wait_for_reply_after,
};

/// The two files most tests attach: a text file of 1.5 KB and a PNG of 70 bytes. Their names are
/// unique, since the server keeps every upload.
fn text_and_png() -> (std::path::PathBuf, std::path::PathBuf) {
    let id = unique();
    (
        temp_file(&format!("notes-{id}.txt"), &vec![b'x'; 1536]),
        temp_file(&format!("pixel-{id}.png"), &PIXEL_PNG),
    )
}

fn file_name(path: &std::path::Path) -> String {
    path.file_name().unwrap().to_string_lossy().into_owned()
}

// ACT-009
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn attached_files_wait_in_a_bar_over_the_field_and_can_be_taken_out() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let (text, png) = text_and_png();
    let app = launch_attaching(&[&text, &png]);
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    open_room(gui, &channel_id);
    gui.wait_until_visible(main_window::COMPOSER_ATTACH, TIMEOUT)
        .unwrap();
    assert!(
        !gui.exists_and_visible(main_window::COMPOSER_ATTACHMENTS)
            .unwrap()
    );
    assert!(!gui.enabled(main_window::COMPOSER_SEND).unwrap());

    gui.mouse_click(main_window::COMPOSER_ATTACH).unwrap();

    // In the order attached, each with its name and size in words, and Send is there to use.
    gui.wait_until_visible(main_window::COMPOSER_ATTACHMENTS, TIMEOUT)
        .unwrap();
    let chip = |index, part| staged_file(main_window::COMPOSER, index, part);
    gui.wait_until_text(&chip(0, staged_file_part::NAME), &file_name(&text), TIMEOUT)
        .unwrap();
    assert_eq!(
        gui.text(&chip(0, staged_file_part::SIZE)).unwrap(),
        "1.5 KB"
    );
    gui.wait_until_text(&chip(1, staged_file_part::NAME), &file_name(&png), TIMEOUT)
        .unwrap();
    assert_eq!(gui.text(&chip(1, staged_file_part::SIZE)).unwrap(), "70 B");
    assert!(gui.enabled(main_window::COMPOSER_SEND).unwrap());

    // The cross takes a file out, and the one after it takes its place.
    gui.mouse_click(&staged_file_remove(main_window::COMPOSER, 0))
        .unwrap();
    gui.wait_until_text(&chip(0, staged_file_part::NAME), &file_name(&png), TIMEOUT)
        .unwrap();
    gui.wait_until_hidden(&chip(1, ""), TIMEOUT).unwrap();
    gui.mouse_click(&staged_file_remove(main_window::COMPOSER, 0))
        .unwrap();
    gui.wait_until_hidden(main_window::COMPOSER_ATTACHMENTS, TIMEOUT)
        .unwrap();
    assert!(!gui.enabled(main_window::COMPOSER_SEND).unwrap());
}

// ACT-009
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn sending_uploads_the_files_in_order_with_the_text_on_the_first() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let (text_file, png) = text_and_png();
    let before = admin.post_message(&channel_id, "first").id;
    let app = launch_attaching(&[&text_file, &png]);
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    open_room(gui, &channel_id);
    gui.mouse_click(main_window::COMPOSER_ATTACH).unwrap();
    gui.wait_until_visible(&staged_file(main_window::COMPOSER, 1, ""), TIMEOUT)
        .unwrap();
    let words = format!("here are two files {}", unique());
    gui.mouse_click(main_window::COMPOSER_FIELD).unwrap();
    gui.set_text(main_window::COMPOSER_FIELD, &words).unwrap();

    gui.mouse_click(main_window::COMPOSER_SEND).unwrap();

    // The bar and the field are empty at once, and the uploads are done when their list is empty.
    gui.wait_until_hidden(main_window::COMPOSER_ATTACHMENTS, TIMEOUT)
        .unwrap();
    gui.wait_until_text(main_window::COMPOSER_FIELD, "", TIMEOUT)
        .unwrap();
    let posted = wait_for_messages_after(&admin, &channel_id, &before, 2);
    gui.wait_until_hidden(main_window::COMPOSER_UPLOADS, TIMEOUT)
        .unwrap();

    // The first file is the first message, with the text; the second has none.
    let (first, second) = (&posted[0].id, &posted[1].id);
    gui.wait_until_text(&message(first, message_row::BODY), &words, TIMEOUT)
        .unwrap();
    gui.wait_until_text(
        &attachment(first, 0, attachment_row::TITLE),
        &file_name(&text_file),
        TIMEOUT,
    )
    .unwrap();
    gui.wait_until_visible(&attachment(second, 0, attachment_row::IMAGE), TIMEOUT)
        .unwrap();
    assert!(
        !gui.exists_and_visible(&message(second, message_row::BODY))
            .unwrap()
    );
    // Nothing is left to send.
    assert!(!gui.enabled(main_window::COMPOSER_SEND).unwrap());
}

// ACT-009
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_file_the_server_refuses_fails_with_its_reason_and_can_be_retried_or_removed() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    // The test server doesn't accept SVG.
    let svg = temp_file(
        &format!("drawing-{}.svg", unique()),
        b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>",
    );
    let before = admin.post_message(&channel_id, "first").id;
    let app = launch_attaching(&[&svg]);
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    open_room(gui, &channel_id);
    gui.mouse_click(main_window::COMPOSER_ATTACH).unwrap();
    gui.wait_until_visible(&staged_file(main_window::COMPOSER, 0, ""), TIMEOUT)
        .unwrap();

    gui.mouse_click(main_window::COMPOSER_SEND).unwrap();

    // The row says why, in words, without the server's code, and offers both ways on.
    let row = |part| upload(main_window::COMPOSER, 0, part);
    gui.wait_until_visible(&row(""), TIMEOUT).unwrap();
    gui.wait_until_text(
        &row(upload_part::STATUS),
        "The server refused the file: File type is not accepted.",
        TIMEOUT,
    )
    .unwrap();
    assert_eq!(gui.text(&row(upload_part::NAME)).unwrap(), file_name(&svg));
    assert!(!gui.exists_and_visible(&row(upload_part::PROGRESS)).unwrap());
    gui.wait_until_visible(&upload_retry(main_window::COMPOSER, 0), TIMEOUT)
        .unwrap();
    gui.wait_until_visible(&upload_cancel(main_window::COMPOSER, 0), TIMEOUT)
        .unwrap();

    // Retrying asks the server again, which says the same.
    gui.mouse_click(&upload_retry(main_window::COMPOSER, 0))
        .unwrap();
    gui.wait_until_text(
        &row(upload_part::STATUS),
        "The server refused the file: File type is not accepted.",
        TIMEOUT,
    )
    .unwrap();

    // Remove gives it up: the row and the bar go, and nothing was posted.
    gui.mouse_click(&upload_cancel(main_window::COMPOSER, 0))
        .unwrap();
    gui.wait_until_hidden(main_window::COMPOSER_UPLOADS, TIMEOUT)
        .unwrap();
    assert_eq!(admin.last_message(&channel_id).id, before);
}

// ACT-009
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_file_attached_in_the_thread_pane_is_uploaded_into_the_thread() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let parent = admin.post_message(&channel_id, &format!("parent {}", unique()));
    let (text_file, _) = text_and_png();
    let app = launch_attaching(&[&text_file]);
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    open_room(gui, &channel_id);
    gui.wait_until_visible(&message(&parent.id, message_row::BODY), TIMEOUT)
        .unwrap();
    choose(gui, &parent.id, main_window::MESSAGE_THREAD_ACTION);
    gui.wait_until_visible(main_window::THREAD_PANE, TIMEOUT)
        .unwrap();
    gui.wait_until_visible(main_window::THREAD_COMPOSER_ATTACH, TIMEOUT)
        .unwrap();
    let in_room = admin.last_message(&channel_id).id;

    gui.mouse_click(main_window::THREAD_COMPOSER_ATTACH)
        .unwrap();

    // The file waits in the thread's composer, not in the room's.
    gui.wait_until_text(
        &staged_file(main_window::THREAD_COMPOSER, 0, staged_file_part::NAME),
        &file_name(&text_file),
        TIMEOUT,
    )
    .unwrap();
    assert!(
        !gui.exists_and_visible(main_window::COMPOSER_ATTACHMENTS)
            .unwrap()
    );
    gui.mouse_click(main_window::THREAD_COMPOSER_SEND).unwrap();

    // It arrives as a reply in the thread, shown in the pane, and not in the room.
    gui.wait_until_hidden(main_window::THREAD_COMPOSER_ATTACHMENTS, TIMEOUT)
        .unwrap();
    let reply = wait_for_reply_after(&admin, &parent.id, None);
    gui.wait_until_text(
        &format!(
            "{}/attachment0/{}",
            thread_message(&reply.id, "").trim_end_matches('/'),
            attachment_row::TITLE
        ),
        &file_name(&text_file),
        TIMEOUT,
    )
    .unwrap();
    assert_eq!(admin.last_message(&channel_id).id, in_room);
}
