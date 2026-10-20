// SPDX-FileCopyrightText: 2026 Sergio Martins
// SPDX-License-Identifier: MIT

//! Formatted messages, mentions and emoji, against the local test server (`RC_TEST_SERVER_URL`).
//! Each test makes its own user, channel and emoji and only looks at those. `msgBody` holds the
//! text as written and `msgBodyHtml` what is shown, which is what these tests read.

mod common;

use rc_gui_tests::paths::{message, message_row};
use rc_test_server::{Admin, server_url, unique};

use common::{TIMEOUT, html_containing, launch, log_in_and_wait_for_room, open_room};

// RICH-001
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_formatted_message_shows_its_formatting_and_keeps_its_text() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let text = format!(
        "**bold** and [a link](https://example.com/{}) and `code`",
        unique()
    );
    let posted = admin.post_message(&channel_id, &text);
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);

    open_room(gui, &channel_id);

    let html = html_containing(gui, &posted.id, "<b>bold</b>");
    assert!(html.contains("<a href=\"https://example.com/"), "{html}");
    assert!(html.contains(">a link</a>"), "{html}");
    assert!(html.contains(">code</code>"), "{html}");
    assert_eq!(
        gui.text(&message(&posted.id, message_row::BODY)).unwrap(),
        text
    );
}

// RICH-001
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn what_the_author_typed_is_never_taken_for_markup() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let text = "<img src=x onerror=alert(1)> <b>not bold</b>";
    let posted = admin.post_message(&channel_id, text);
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);

    open_room(gui, &channel_id);

    let html = html_containing(gui, &posted.id, "&lt;b&gt;not bold&lt;/b&gt;");
    assert!(!html.contains("<img"), "{html}");
    assert!(!html.contains("<b>not bold"), "{html}");
}

// RICH-002
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_message_that_mentions_the_user_is_marked_and_others_are_not() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let calling = admin.post_message(&channel_id, &format!("hello @{}", user.username));
    let everybody = admin.post_message(&channel_id, "hello @all");
    let other = admin.post_message(&channel_id, "hello @somebody.else");
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);

    open_room(gui, &channel_id);

    let html = html_containing(gui, &calling.id, "<b>@");
    assert!(html.contains("rgba(255,152,0,0.55)"), "{html}");
    gui.wait_until_visible(&message(&calling.id, message_row::MENTIONS_ME), TIMEOUT)
        .unwrap();
    gui.wait_until_visible(&message(&everybody.id, message_row::MENTIONS_ME), TIMEOUT)
        .unwrap();
    html_containing(gui, &other.id, "@somebody.else");
    assert!(
        !gui.exists_and_visible(&message(&other.id, message_row::MENTIONS_ME))
            .unwrap()
    );
}

// RICH-003
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_shortcode_shows_as_unicode_and_an_unknown_one_stays_as_typed() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let known = admin.post_message(&channel_id, "so happy :smile: today");
    let unknown = admin.post_message(&channel_id, "what is :no_such_emoji_here: now");
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);

    open_room(gui, &channel_id);

    let html = html_containing(gui, &known.id, "😄");
    assert!(!html.contains(":smile:"), "{html}");
    html_containing(gui, &unknown.id, ":no_such_emoji_here:");
}

// RICH-003
#[test]
#[ignore = "needs the app and a Rocket.Chat server: run through ctest"]
fn a_custom_emoji_of_the_server_shows_as_its_image() {
    let admin = Admin::login();
    let user = admin.create_user();
    let (channel_id, _) = admin.create_channel(&[&user.username]);
    let name = format!("gt_emoji_{}", unique().replace('-', "_"));
    admin.create_emoji(&name);
    let posted = admin.post_message(&channel_id, &format!("custom :{name}: here"));
    let app = launch();
    let gui = app.gui();
    log_in_and_wait_for_room(gui, &user, &channel_id);

    open_room(gui, &channel_id);

    let url = format!(
        "{}/emoji-custom/{name}.png",
        server_url().trim_end_matches('/')
    );
    let html = html_containing(gui, &posted.id, &format!("<img src=\"{url}\""));
    assert!(!html.contains(&format!(":{name}:")), "{html}");
}
