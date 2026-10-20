// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Files uploaded to a channel, against the local test server (`RC_TEST_SERVER_URL`). An image is
//! shown inline, from a file in the app's cache (its `text` is the `file://` URL); any other file
//! is a link to its address on the server. The app's cache directory starts empty (see
//! `App::cache_dir`).

// A failed step should just fail the test.
#![allow(clippy::unwrap_used)]

mod common;

use rc_gui_tests::paths::{attachment, attachment_row, message, message_row};
use rc_test_server::{Admin, PIXEL_PNG, server_url};

use common::{TIMEOUT, file_of, launch, log_in_and_wait_for_room, open_room};

// RICH-005
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn an_uploaded_image_is_shown_inline_from_a_file_in_the_cache() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let posted = admin.upload_file(
        &channel_id,
        "pixel.png",
        "image/png",
        &PIXEL_PNG,
        "look at this",
        "a single pixel",
    );
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);

    open_room(gui, &channel_id);

    let image = attachment(&posted.id, 0, attachment_row::IMAGE);
    gui.wait_until_visible(&image, TIMEOUT).unwrap();
    let file = file_of(&gui.text(&image).unwrap());
    assert!(
        file.starts_with(app.cache_dir()),
        "{} is not in {}",
        file.display(),
        app.cache_dir().display()
    );
    // The image is small enough for the server to send the original.
    assert_eq!(std::fs::read(&file).unwrap(), PIXEL_PNG);
    // Once the picture is there the placeholder is gone, and the image is no link to a file.
    gui.wait_until_hidden(
        &attachment(&posted.id, 0, attachment_row::PLACEHOLDER),
        TIMEOUT,
    )
    .unwrap();
    assert!(
        !gui.exists_and_visible(&attachment(&posted.id, 0, attachment_row::LINK))
            .unwrap()
    );
    assert_eq!(
        gui.text(&attachment(&posted.id, 0, attachment_row::DESCRIPTION))
            .unwrap(),
        "a single pixel"
    );
    assert_eq!(
        gui.text(&message(&posted.id, message_row::BODY)).unwrap(),
        "look at this"
    );
}

// RICH-005
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn another_file_is_a_link_to_its_address_on_the_server() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let posted = admin.upload_file(&channel_id, "notes.txt", "text/plain", b"hello\n", "", "");
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);

    open_room(gui, &channel_id);

    let link = attachment(&posted.id, 0, attachment_row::LINK);
    gui.wait_until_visible(&link, TIMEOUT).unwrap();
    assert_eq!(
        gui.text(&attachment(&posted.id, 0, attachment_row::TITLE))
            .unwrap(),
        "notes.txt"
    );
    assert_eq!(
        gui.text(&attachment(&posted.id, 0, attachment_row::SIZE))
            .unwrap(),
        "6 B"
    );
    let address = gui.text(&link).unwrap();
    let prefix = format!("{}/file-upload/", server_url().trim_end_matches('/'));
    assert!(address.starts_with(&prefix), "{address}");
    assert!(address.ends_with("/notes.txt"), "{address}");
    // It is no image, has no description, and a message of only a file leaves no blank line.
    assert!(
        !gui.exists_and_visible(&attachment(&posted.id, 0, attachment_row::IMAGE))
            .unwrap()
    );
    assert!(
        !gui.exists_and_visible(&attachment(&posted.id, 0, attachment_row::DESCRIPTION))
            .unwrap()
    );
    assert!(
        !gui.exists_and_visible(&message(&posted.id, message_row::BODY))
            .unwrap()
    );
}

// RICH-005
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_file_uploaded_while_the_room_is_open_shows_up() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let before = admin.post_message(&channel_id, "first");
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);
    open_room(gui, &channel_id);
    gui.wait_until_visible(&message(&before.id, message_row::BODY), TIMEOUT)
        .unwrap();

    let image = admin.upload_file(&channel_id, "live.png", "image/png", &PIXEL_PNG, "", "");
    let file = admin.upload_file(&channel_id, "live.txt", "text/plain", b"live\n", "", "");

    gui.wait_until_visible(&attachment(&image.id, 0, attachment_row::IMAGE), TIMEOUT)
        .unwrap();
    gui.wait_until_visible(&attachment(&file.id, 0, attachment_row::LINK), TIMEOUT)
        .unwrap();
}
